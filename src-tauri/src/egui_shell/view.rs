//! egui メインウィンドウの検索 view（#532 SU2 外殻 + SU3 検索 driver）。SearchState（純粋核）を
//! 駆動する imperative shell: TextEdit/結果リスト描画・直 Engine 検索（debounce）・↑↓/→←ナビ・
//! folder 展開（async ロード + staleness）・instant/slash コマンド・起動/実行 dispatch・動的高さ。
//! font-first（jp_font を index 0）は SU1 申し送りの義務。

use std::sync::OnceLock;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use snotra_core::engine::FolderListContext;
use snotra_core::ui_types::SearchResult;
use snotra_egui_runtime::{EguiView, RuntimeFrame};
use tauri::{Emitter, Manager};

use crate::egui_shell::{
    Debouncer, EscapeOutcome, HeightParams, QueryIntent, SearchState, SlashCmd, ViewKind,
    compute_parent_dir, compute_window_height, find_slash_command, folder_load_pending,
};

static JP_FONT_BYTES: OnceLock<Box<[u8]>> = OnceLock::new();

fn japanese_font_definitions(bytes: &'static [u8]) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    let mut font = egui::FontData::from_static(bytes);
    font.tweak = egui::FontTweak {
        scale: 1.0,
        y_offset_factor: 0.3,
        y_offset: 0.0,
        ..Default::default()
    };
    fonts.font_data.insert("jp_font".to_owned(), font.into());
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        // insert(0)＝先頭。jp_font を最優先にして単一フォント化する。push（末尾 fallback）だと
        // Latin=egui 既定 / CJK=Yu Gothic に分離し、被覆 AA 無の softbuffer でベースラインずれ（#579/#399）。
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, "jp_font".to_owned());
    }
    fonts
}

fn configure_japanese_font(context: &egui::Context) {
    let candidates = [
        "C:/Windows/Fonts/YuGothM.ttc",
        "C:/Windows/Fonts/yugothic.ttf",
        "C:/Windows/Fonts/msgothic.ttc",
        "C:/Windows/Fonts/meiryo.ttc",
    ];
    if JP_FONT_BYTES.get().is_none() {
        for path in candidates {
            if let Ok(bytes) = std::fs::read(path) {
                let _ = JP_FONT_BYTES.set(bytes.into_boxed_slice());
                break;
            }
        }
    }
    if let Some(bytes) = JP_FONT_BYTES.get() {
        // OnceLock の中身は以後不変ゆえ 'static として安全に借用できる。
        let static_bytes: &'static [u8] =
            unsafe { std::mem::transmute::<&[u8], &'static [u8]>(bytes) };
        context.set_fonts(japanese_font_definitions(static_bytes));
    }
}

/// ナビゲーションスレッド → driver のメッセージ（#532 SU3 M2）。token（= folder_gen）で
/// staleness 判定する（`SearchState::accept_folder_result`）。
enum FolderMsg {
    /// 列挙成功: (token, ctx, 全ソート済み full 集合)。driver がキャッシュし filter_sorted で絞る。
    Loaded(u64, FolderListContext, Vec<SearchResult>),
    /// 列挙失敗: (token, 単一エラー行)（§6.6・filter 非適用で常時表示）。
    Failed(u64, Vec<SearchResult>),
}

pub(crate) struct SearchWindowView {
    app_handle: tauri::AppHandle,
    was_focused: bool,
    unfocus_at: Option<Instant>,
    state: SearchState,
    search_debounce: Debouncer,
    last_input_at: Instant,
    last_set_height: f64,
    // query フィールドは SearchState.query へ移譲（削除）。
    // emit dedup は共有 EguiShellState.hide_pending（show がクリア・codex #8）。view-local には持たない。
    folder_tx: Sender<FolderMsg>,
    folder_rx: Receiver<FolderMsg>,
    /// ナビゲーションでロードした (ctx, 全ソート済み) キャッシュ。打鍵フィルタの源（#532 SU3 M2）。
    folder_cache: Option<(FolderListContext, Vec<SearchResult>)>,
    /// 列挙失敗時の単一エラー行（filter を無視して表示）。
    folder_error: Option<Vec<SearchResult>>,
    /// 表示中の results の来歴: instant 候補なら Some(instant_query)。Enter/クリック/← の判定は
    /// live config の prefix から interp を再導出せず、この snapshot を使う——prefix の hot-change
    /// 後に stale instant 行（path=description/display）が activate() へ流れて文字列をパスとして
    /// 起動・履歴汚染するのを防ぐ（/code-review #637 finding 0）。run_search が行と一体で更新する。
    instant_rows_query: Option<String>,
}

impl SearchWindowView {
    pub(crate) fn new(app_handle: tauri::AppHandle) -> Self {
        let (folder_tx, folder_rx) = channel();
        Self {
            app_handle,
            was_focused: false,
            unfocus_at: None,
            state: SearchState::new(),
            search_debounce: Debouncer::new(Duration::from_millis(50), true),
            last_input_at: Instant::now(),
            last_set_height: 52.0,
            folder_tx,
            folder_rx,
            folder_cache: None,
            folder_error: None,
            instant_rows_query: None,
        }
    }

    /// hide 要求を emit する。多重防止は共有 EguiShellState.hide_pending（show_egui_main が
    /// クリア・codex #8）。view-local フラグだと hide 後 Focused(true) 非着信で永久 true 化し、
    /// 以後の hide を抑止してしまう。
    fn emit_hide(&self) {
        let already = self
            .app_handle
            .try_state::<crate::egui_shell::EguiShellState>()
            .map(|sh| sh.hide_pending.swap(true, Ordering::SeqCst))
            .unwrap_or(false);
        if already {
            return;
        }
        let _ = self.app_handle.emit("egui-hide-requested", ());
    }

    /// index 行を起動し、成功なら履歴記録して hide 要求を出す（§4.8 シングルクリック / Enter）。
    /// launch_item_core は ShellExecuteW（エンジンロック外で呼ぶ・launch.rs:226）。成功時のみ
    /// record_and_save で履歴を記録（§4.3/§5 の query_count 加点・全起動経路の共通末尾を再利用）。
    /// エラー行（is_error）／フォルダロード中（cache・error 未着で results が stale）は起動しない。
    /// Enter とシングルクリックの単一チョークポイント（#636 レビュー Finding A）。
    fn activate(&self, index: usize) {
        use crate::commands::launch::{LaunchStatus, launch_item_core, record_and_save};
        // フォルダ展開直後、列挙結果も失敗行も未着の窓では results が展開前ビューの残存物ゆえ、
        // 誤項目の起動を止める（dead/slow UNC でロードが滞留すると Enter/クリックが前ビューの
        // 項目を起動しうる・#636 レビュー Finding A）。判定核は search_state の純粋述語。
        if folder_load_pending(
            self.state.view_kind(),
            self.folder_cache.is_some(),
            self.folder_error.is_some(),
        ) {
            return;
        }
        let Some(result) = self.state.results().get(index) else { return };
        if result.is_error {
            return;
        }
        let path = result.path.clone();
        let query = self.state.query().to_string();
        let outcome = launch_item_core(&path); // ロック外・ShellExecuteW
        crate::trace_main(
            "egui_launch",
            serde_json::json!({ "index": index, "status": format!("{:?}", outcome.status) }),
        );
        if matches!(outcome.status, LaunchStatus::Ok) {
            if let Some(state) = self.app_handle.try_state::<crate::AppState>() {
                record_and_save(&state, &path, &query); // 履歴記録 + 保存（ロックは内部で最小保持）
            }
            // 起動成功時のみ hide（SU2 の hide 合流点へ・view から window を直接触らない）。
            self.emit_hide();
        }
    }

    /// クエリ/結果/armed trailing/instant 来歴をまとめてクリアする単一チョークポイント
    /// （execute_slash と instant 成功経路が共有）。第 3 のクリアサイト（SU3.5 tool 等）が
    /// `search_debounce.cancel()` を書き忘れて「クリア後に stale trailing 検索が発火」を
    /// 再発させないための集約（/code-review #637 finding 6）。
    fn clear_search(&mut self) {
        self.state.set_query(String::new());
        self.state.set_results(Vec::new());
        self.search_debounce.cancel();
        self.instant_rows_query = None;
    }

    /// slash コマンドを実行する（§15.3 即実行・#532 SU3 M3）。SolidJS handleCommandQueryInput と
    /// 同順: クエリ/結果クリア（clearCommandModeState 相当）→ action。`/r`（History）は結果注入型で
    /// ここへ来ない（changed ハンドラが run_search へ振る）。失敗通知は建てない（trace のみ・#631 一本化）。
    fn execute_slash(&mut self, cmd: SlashCmd) {
        crate::trace_main("egui_slash", serde_json::json!({ "cmd": format!("{cmd:?}") }));
        self.clear_search();
        let app = self.app_handle.clone();
        match cmd {
            // 到達しない: 呼び出し側（changed ハンドラ）が History を run_search へ振る。
            // 将来 execute_slash の呼び出しサイトが増えて誤配線したとき dev/test で loud に
            // 落とす（release は panic=abort ゆえ unreachable! は採らない）。
            SlashCmd::History => debug_assert!(false, "History は execute_slash へ来ない（run_search が注入する）"),
            SlashCmd::OpenSettings => {
                // indexing 中の Err（ERR_INDEXING_IN_PROGRESS）は trace のみ（spec M3 実装確定・
                // クエリクリア後は検索バーの indexing hint が可視＝degraded な理由提示）。
                if let Err(e) = crate::commands::open_settings(app.state(), app.clone()) {
                    crate::trace_main("egui_slash_error", serde_json::json!({ "cmd": "/o", "error": e }));
                }
            }
            SlashCmd::RebuildIndex => {
                // SolidJS /s parity: hide してから rebuild（hide は emit 合流・順序は視覚のみで
                // rebuild は backend スレッド）。indexing 中の Err は意図的無音（#434 parity）。
                self.emit_hide();
                if let Err(e) = crate::commands::rebuild_index(app.state(), app.clone()) {
                    crate::trace_main("egui_slash_error", serde_json::json!({ "cmd": "/s", "error": e }));
                }
            }
            SlashCmd::Quit => {
                // quit_app（commands/system.rs）と同一実体: exit-requested listener が
                // history/icon flush → exit（main.rs）。egui 経路も同じ合流点を使う。
                let _ = app.emit("exit-requested", ());
            }
        }
    }

    /// 選択中の instant コマンドを同期実行する（§19.6・#532 SU3 M3）。IPC の
    /// execute_instant_command（spawn_blocking + 4s）と同じ手順（action 抽出をロック内・
    /// clipboard 読みをロック外）を、イベントループで同期直呼びに畳む（spec M3 実装確定・
    /// ブロックリスクは #631 スコープ）。instant は履歴を記録しない（IPC 経路 parity）。
    /// 成功: クエリ/結果クリア + hide（§19.6）。失敗: 据え置き + trace（M1 起動失敗と同型）。
    fn execute_instant_selected(&mut self, index: usize, instant_query: &str) {
        use crate::commands::launch::LaunchStatus;
        let Some(sel) = self.state.results().get(index) else { return };
        if sel.is_error {
            return;
        }
        let name = sel.name.clone();
        let Some(state) = self.app_handle.try_state::<crate::AppState>() else { return };
        let Some(action) = ({
            let engine = state.engine.lock().unwrap();
            engine
                .config()
                .instant_commands
                .iter()
                .find(|c| c.name == name)
                .map(|c| c.action.clone())
        }) else {
            // config hot-reload で行が stale 化（改名/削除）した場合。IPC 経路の
            // Err("instant command not found") に相当する痕跡を trace へ残す——silent no-op に
            // すると Enter が死んで見えても診断不能（/code-review #637 finding 1）。
            crate::trace_main(
                "egui_instant_error",
                serde_json::json!({ "name": name, "error": "not found" }),
            );
            return;
        };
        // clipboard 読み（Win32）はロック外（commands/instant.rs と同順）。
        let clipboard = arboard::Clipboard::new()
            .and_then(|mut cb| cb.get_text())
            .unwrap_or_default();
        // 種別ディスパッチは IPC 経路と共有の core（二重実装の drift 防止・finding 4）。
        let outcome = crate::commands::instant::execute_instant_action_core(
            action,
            instant_query,
            &clipboard,
        );
        crate::trace_main(
            "egui_instant",
            serde_json::json!({ "name": name, "status": format!("{:?}", outcome.status) }),
        );
        if matches!(outcome.status, LaunchStatus::Ok) {
            self.clear_search();
            self.emit_hide();
        }
    }

    /// Enter/クリックの単一 dispatch（§19.6/§4.8・#532 SU3 M3）。判定は live config の prefix
    /// からの interp 再導出ではなく**表示行の来歴**（`instant_rows_query` snapshot）で行う——
    /// prefix の hot-change 後に stale instant 行（path=description/display）を activate() へ流し、
    /// 文字列をパスとして起動・履歴汚染するのを防ぐ（/code-review #637 finding 0）。行と来歴は
    /// run_search が同一フレームで一体更新するため常に整合する。行 index で参照（パス文字列を
    /// 使わない・ui ルール踏襲）。Shift+Enter も同じ Enter として届くため §19.6
    /// 「Shift+Enter=Enter」は追加コードなしで成立する（tool-selection は SU3.5）。
    fn activate_or_execute(&mut self, index: usize) {
        if let Some(iq) = self.instant_rows_query.clone() {
            self.execute_instant_selected(index, &iq);
        } else {
            self.activate(index);
        }
    }

    /// フォルダ展開を履歴に記録する（IPC コマンド `commands/system.rs:record_folder_expansion`
    /// と同一パターン：lock → record → prepare_history_save_if_dirty → drop → save。egui 経路は
    /// IPC を経由しないため、driver（本 view）から SolidJS の `enterFolderExpansion` と同じ
    /// 呼び出しサイトを再現する（→ 展開時のみ・← の折り返し `navigateFolderUp` 相当では呼ばない）。
    fn record_folder_expansion(&self, dir: &str) {
        let Some(state) = self.app_handle.try_state::<crate::AppState>() else {
            return;
        };
        let save = {
            let mut engine = state.engine.lock().unwrap();
            engine.record_folder_expansion(dir);
            engine.prepare_history_save_if_dirty(5)
        };
        if let Some(save) = save {
            let _ = save.save();
        }
    }

    /// auto_hide_on_focus_lost を実行中 config から都度読む（キャッシュしない・#576 と同設計）。
    fn auto_hide_enabled(&self) -> bool {
        self.app_handle
            .try_state::<crate::AppState>()
            .map(|s| {
                s.engine
                    .lock()
                    .unwrap()
                    .config()
                    .general
                    .auto_hide_on_focus_lost
            })
            .unwrap_or(true) // config.rs 既定と一致
    }

    /// 設定サイドカー起動中は blur で hide しない（設定が focus を奪っても本体を消さない）。
    fn settings_running(&self) -> bool {
        self.app_handle
            .try_state::<crate::SettingsProcessState>()
            .map(|p| p.lock().unwrap().is_some())
            .unwrap_or(false)
    }

    /// instant prefix を実行中 config から都度読む（キャッシュしない・#576 と同設計）。
    /// フィールドは config.search.instant_command_prefix（config.rs:956 で確認済み）。
    fn instant_prefix(&self) -> String {
        self.app_handle
            .try_state::<crate::AppState>()
            .map(|s| s.engine.lock().unwrap().config().search.instant_command_prefix.clone())
            .unwrap_or_else(|| "@".to_string())
    }

    /// index 構築中か（AppState.indexing: AtomicBool・state.rs:14 で確認済み）。
    fn indexing(&self) -> bool {
        self.app_handle
            .try_state::<crate::AppState>()
            .map(|s| s.indexing.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(false)
    }

    /// 動的高さ算出用の max_results（§4.5/§4.7）。visible_rows は `Option<usize>` のため
    /// effective_visible_rows() で既定補完する（config.rs:327）。
    fn max_results(&self) -> u32 {
        self.app_handle
            .try_state::<crate::AppState>()
            .map(|s| s.engine.lock().unwrap().config().appearance.effective_visible_rows() as u32)
            .unwrap_or(8)
    }

    /// 現在のウィンドウ論理幅。set_size で高さのみ変え幅を維持するために読む
    /// （M1 では幅は不変・SU2 が config から生成済み）。読めなければ 600.0 にフォールバック。
    fn window_width(&self) -> f64 {
        self.app_handle
            .get_window("main")
            .and_then(|w| {
                w.inner_size()
                    .ok()
                    .map(|s| s.to_logical::<f64>(w.scale_factor().unwrap_or(1.0)).width)
            })
            .unwrap_or(600.0)
    }

    /// dir を別スレッドで全列挙・全ソートし FolderMsg を channel へ送る（token 付き）。
    /// capture（ロック内）→ read_dir_entries（ロック外 I/O・dead UNC でも event-loop を止めない）
    /// → finalize_folder_list_unlimited（ロック内・history でソート）の 3 段（engine.rs のロック最小化パターン）。
    ///
    /// **per-nav `std::thread::spawn` は意図的な選択**（advisor 2026-07-23）: 単一 worker だと 1 つの
    /// hung `read_dir`（dead UNC）が後続の全フォルダロードを塞ぐ。per-nav spawn は hang を 1 スレッドに
    /// 隔離し、正常な dir へのナビは動き続ける。dead UNC でのスレッドリークは best-effort として受容する
    /// （M1 の event-loop 同期 `ShellExecuteW`-on-dead-UNC と同類のトレードオフ）。正常な dir の read_dir は
    /// ミリ秒オーダーで完了するため高速 →/← でも実質的な pileup は起きない。共有 tokio blocking pool
    /// （`spawn_blocking`）は採らない——dead UNC が pool を飽和させ icon/index 等の他利用者を巻き込むため。
    ///
    /// **egui_ctx（呼び出し側の `ui.ctx().clone()`）を送信毎に `request_repaint()` する**
    /// （advisor 2026-07-23）: このランタイム（`snotra-egui-runtime`）はイベント駆動（`RedrawRequested`
    /// 待ち・`repaint.rs`）であり、通常フレームは不要な再描画をしない。channel 送信だけでは次の
    /// `update()` を誰も起こさないため、無関係な入力（マウス移動等）が来るまで到着済みの FolderMsg が
    /// drain されず、フォルダ内容が画面に反映されない（→/← 直後に応答が無いように見える）。
    fn spawn_folder_load(&self, token: u64, dir: String, egui_ctx: egui::Context) {
        let app = self.app_handle.clone();
        let tx = self.folder_tx.clone();
        std::thread::spawn(move || {
            let Some(state) = app.try_state::<crate::AppState>() else { return };
            let ctx = { state.engine.lock().unwrap().capture_folder_list_context() };
            let entries = match ctx.read_dir_entries(std::path::Path::new(&dir), "") {
                Ok(e) => e,
                Err(_) => {
                    let err = snotra_core::folder::error_result(std::path::Path::new(&dir));
                    let _ = tx.send(FolderMsg::Failed(token, err));
                    egui_ctx.request_repaint();
                    return;
                }
            };
            let sorted = { state.engine.lock().unwrap().finalize_folder_list_unlimited(entries) };
            let _ = tx.send(FolderMsg::Loaded(token, ctx, sorted));
            egui_ctx.request_repaint();
        });
    }

    /// view_kind 先の同期 dispatch（#532 SU3 M2）。folder は cache/error を同期フィルタ、
    /// results は M1 の interp 分岐（plain 検索）。folder 打鍵が engine.search へ漏れない。
    /// prefix を内部で取得する薄いラッパー（trailing poll・folder drain 用）。changed エッジは
    /// 取得済み prefix を `run_search_with` へ渡し、毎打鍵の engine lock 回数を減らす
    /// （/code-review #637 finding 9）。
    fn run_search(&mut self) {
        let prefix = self.instant_prefix();
        self.run_search_with(&prefix);
    }

    fn run_search_with(&mut self, prefix: &str) {
        // 来歴は行と一体で更新する（Instant 分岐だけが Some を立て直す・finding 0）。
        self.instant_rows_query = None;
        match self.state.view_kind() {
            ViewKind::Folder => {
                if let Some(err) = &self.folder_error {
                    self.state.set_results(err.clone()); // 列挙失敗行（filter 非適用）
                } else if let Some((ctx, sorted)) = &self.folder_cache {
                    let filtered = ctx.filter_sorted(sorted, self.state.folder_filter());
                    self.state.set_results(filtered);
                }
                // cache 未着（ロード中）は前フレーム結果を保持（フリット無し・set しない）
            }
            ViewKind::Tool => {
                // §18.5: ツール選択中は検索結果を上書きしない（trailing/changed とも no-op）
            }
            ViewKind::Results => {
                match self.state.interp(prefix) {
                    QueryIntent::Plain => {
                        if self.state.query().trim().is_empty() || self.indexing() {
                            self.state.set_results(Vec::new());
                            return;
                        }
                        let query = self.state.query().to_string();
                        // 計測は lock 取得込み（フレームを塞ぐ全区間・#634 G-SYNC）。
                        let search_started = std::time::Instant::now();
                        let results = {
                            let state = match self.app_handle.try_state::<crate::AppState>() {
                                Some(s) => s,
                                None => return,
                            };
                            let mut engine = state.engine.lock().unwrap();
                            engine.search(&query)
                        }; // lock 解放
                        crate::trace::trace(
                            "egui_search:dispatch",
                            serde_json::json!({
                                "query_chars": query.chars().count(),
                                "results": results.len(),
                                "elapsed_us": search_started.elapsed().as_micros() as u64,
                            }),
                        );
                        self.state.set_results(results);
                    }
                    QueryIntent::Instant { filter_name, instant_query } => {
                        // §19.5: 前方一致フィルタ。毎打鍵同期（30ms debounce 撤廃・spec M3 実装確定）。
                        // indexing を見ない（§19.7: instant はインデックス非依存ゆえ構築中でも使用可）。
                        // 候補取得は IPC コマンドと同一 fn を共有（二重実装の drift 防止・finding 5）。
                        let rows = crate::commands::instant::get_instant_commands(
                            filter_name,
                            self.app_handle.clone(),
                        )
                        .unwrap_or_default()
                        .into_iter()
                        .map(|dto| SearchResult {
                            name: dto.name,
                            // §19.5: description 設定時は優先、無ければ display（URL / exe args）
                            path: if dto.description.is_empty() { dto.display } else { dto.description },
                            is_folder: false,
                            is_error: false,
                        })
                        .collect::<Vec<_>>();
                        // 来歴 snapshot: この行集合が instant 候補であることと、その時点の
                        // instant_query を一体で記録する（activate_or_execute が参照・finding 0）。
                        self.instant_rows_query = Some(instant_query);
                        self.state.set_results(rows);
                    }
                    QueryIntent::Command => {
                        // §15.2 /r: 履歴を注入して留まる（冪等ゆえ trailing 再発火も無害）。
                        // 他（部分入力・実行済み直後）は候補なしクリア（§15.3: command 中は検索しない）。
                        if matches!(find_slash_command(self.state.query()), Some(SlashCmd::History)) {
                            let rows = {
                                let state = match self.app_handle.try_state::<crate::AppState>() {
                                    Some(s) => s,
                                    None => return,
                                };
                                let engine = state.engine.lock().unwrap();
                                engine.recent_history()
                            };
                            self.state.set_results(rows);
                        } else {
                            self.state.set_results(Vec::new());
                        }
                    }
                }
            }
        }
    }

    /// 1 行を描画。selected ならハイライト + scroll_to_me。返り値: single_clicked。
    /// ダブルクリックは扱わない（ユーザー決定: §4.8 の double-click=選択は as-built でも
    /// 到達不能ゆえ落とす。単クリック=起動のみ）。self を借りない関連関数（借用衝突回避）。
    fn draw_result_row(ui: &mut egui::Ui, result: &SearchResult, selected: bool) -> bool {
        let row_h = 30.0;
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), row_h),
            egui::Sense::click(),
        );
        if selected {
            ui.painter().rect_filled(rect, 4.0, ui.visuals().selection.bg_fill);
            response.scroll_to_me(Some(egui::Align::Center));
        }
        // アイコンスロット（SU4 が埋める）: 左に 24px 空ける。
        let text_x = rect.left() + 28.0;
        let name_color = ui.visuals().text_color();
        let path_color = ui.visuals().weak_text_color(); // 淡色パス
        let painter = ui.painter();
        painter.text(
            egui::pos2(text_x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            &result.name,
            egui::FontId::proportional(14.0),
            name_color,
        );
        // 名前の右にパスを淡色で（簡易・galley 省略は egui 既定に委ねる）。
        painter.text(
            egui::pos2(rect.right() - 8.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            &result.path,
            egui::FontId::proportional(11.0),
            path_color,
        );
        response.clicked()
    }
}

impl EguiView for SearchWindowView {
    fn setup(&mut self, context: &egui::Context) {
        configure_japanese_font(context);
    }

    fn update(&mut self, ui: &mut egui::Ui, _frame: &mut RuntimeFrame) {
        // show 直後の resetForShow（EguiShellState.reset_pending を消費）。stale な debounce
        // armed 状態が再表示後に誤発火しないよう、debounce も併せて作り直す。
        if let Some(sh) = self.app_handle.try_state::<crate::egui_shell::EguiShellState>()
            && sh.reset_pending.swap(false, Ordering::SeqCst)
        {
            self.state.reset();
            self.folder_cache = None;
            self.folder_error = None;
            self.instant_rows_query = None; // §19.7: resetForShow で instant モード解除
            self.search_debounce = Debouncer::new(Duration::from_millis(50), true);
        }

        let ctx = ui.ctx().clone();

        // ナビ結果を drain し、現行 folder_gen と一致する最新のものだけ適用する（stale 破棄・滞留 drain）。
        let mut latest: Option<FolderMsg> = None;
        while let Ok(msg) = self.folder_rx.try_recv() {
            let tok = match &msg {
                FolderMsg::Loaded(t, ..) | FolderMsg::Failed(t, ..) => *t,
            };
            if self.state.accept_folder_result(tok) {
                latest = Some(msg); // 後着で上書き＝最新を採る
            }
        }
        if let Some(msg) = latest {
            match msg {
                FolderMsg::Loaded(_, folder_ctx, sorted) => {
                    self.folder_error = None;
                    self.folder_cache = Some((folder_ctx, sorted));
                }
                FolderMsg::Failed(_, err) => {
                    self.folder_cache = None;
                    self.folder_error = Some(err);
                }
            }
            self.run_search(); // 現 folder_filter で即再フィルタ（ロード中打鍵の消失防止）
            ctx.request_repaint(); // 到着フレームを描く
        }

        let focused = ctx.input(|i| i.focused);
        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));

        let was_focused = self.was_focused;
        // 再表示直後の stale 猶予をリセット: focused に戻ったら pending 破棄（codex #8）。
        // emit dedup（hide_pending）は show_egui_main がクリアするので view では触らない。
        if focused {
            self.unfocus_at = None;
        }
        // Escape ラダー（folder 中は展開前状態へ復帰、top-level は hide 要求・#532 SU3 M2）。
        // TextEdit より前に ctx から拾うので入力欄に focus があっても届く。
        if escape {
            match self.state.on_escape() {
                EscapeOutcome::RestoredSearch => {
                    // folder 離脱 → cache/error 破棄、復帰済み results（展開前の plain 行）を描く
                    self.folder_cache = None;
                    self.folder_error = None;
                    self.instant_rows_query = None;
                    ctx.request_repaint();
                }
                EscapeOutcome::RestoredFromTool => {
                    // tool 解除 → 直下ビュー（folder/results）を復元描画。folder が下に生きて
                    // いるため cache/error は破棄しない（RestoredSearch との差・純粋核 doc 参照）
                    ctx.request_repaint();
                }
                EscapeOutcome::Hide => self.emit_hide(),
            }
        }
        // focus 喪失 → 100ms 猶予を張り、猶予明けに repaint させる。
        if was_focused && !focused {
            self.unfocus_at = Some(Instant::now());
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        // 猶予明け判定は純粋核 blur_should_hide に委ねる（focus 復帰・auto_hide・設定起動を AND）。
        if let Some(at) = self.unfocus_at {
            let grace_elapsed = at.elapsed() >= Duration::from_millis(100);
            if crate::egui_shell::blur_should_hide(
                focused,
                grace_elapsed,
                self.auto_hide_enabled(),
                self.settings_running(),
            ) {
                self.unfocus_at = None;
                self.emit_hide();
            }
        }

        // ↑↓ ナビ（結果があるとき）。TextEdit より前に ctx から拾い、入力欄 focus 中も効かせる。
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
            self.state.move_selection(1);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
            self.state.move_selection(-1);
        }

        // → : 選択中がフォルダなら展開（results 中は enter、folder 中は深掘り）。ファイル/エラー行は無反応。
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight))
            && self.state.view_kind() != ViewKind::Tool // §18.5 ←→無効
            && let Some(sel) = self.state.results().get(self.state.selected())
            && sel.is_folder
            && !sel.is_error
        {
            let dir = sel.path.clone();
            let tok = if self.state.view_kind() == ViewKind::Folder {
                self.state.navigate_folder(dir.clone())
            } else {
                self.state.enter_folder(dir.clone())
            };
            // → は Folder 中の深掘り・Results からの enter どちらも展開履歴に記録
            // （SolidJS enterFolderExpansion と同一サイト・#532 SU3 M2 Finding #1）。
            self.record_folder_expansion(&dir);
            self.folder_cache = None;
            self.folder_error = None;
            self.spawn_folder_load(tok, dir, ctx.clone());
        }
        // ← : folder 中は親へ、通常検索中は選択項目の親を展開して folder 突入。
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
            match self.state.view_kind() {
                ViewKind::Tool => {} // §18.5 ←→無効
                ViewKind::Folder => {
                    if let Some(parent) = self.state.parent_dir() {
                        // ← の folder 折り返しは navigateFolderUp 相当・記録しない（Finding #1）。
                        let tok = self.state.navigate_folder(parent.clone());
                        self.folder_cache = None;
                        self.folder_error = None;
                        self.spawn_folder_load(tok, parent, ctx.clone());
                    }
                }
                ViewKind::Results => {
                    // instant 行表示中のみ ← 無効（§19.7・SolidJS allowsFolderNav parity）。判定は
                    // interp 再導出でなく行来歴（instant_rows_query・prefix hot-change に頑健）。
                    // instant 行の path は description/display ゆえ compute_parent_dir が偶然 Some を
                    // 返して bogus folder 突入しうるのを塞ぐ。command（/r 履歴）中は許可＝→ と対称。
                    if self.instant_rows_query.is_none() && let Some(sel) = self.state.results().get(self.state.selected())
                        && !sel.is_error
                        && let Some(parent) = compute_parent_dir(&sel.path)
                    {
                        let tok = self.state.enter_folder(parent.clone());
                        // ← from Results は enterFolderExpansion(parent) 相当・記録する。
                        self.record_folder_expansion(&parent);
                        self.folder_cache = None;
                        self.folder_error = None;
                        self.spawn_folder_load(tok, parent, ctx.clone());
                    }
                }
            }
        }

        // 検索入力欄。state.query を編集し、変化があれば debounce leading で同期検索。
        // 構築中かつ空クエリなら hint を案内文へ差し替える（§4.7）。egui の hint は入力が空の
        // ときだけ描かれるため、indexing+空クエリの条件と一致する——window は 52px のまま
        // （show_results=false）で、案内はバー内に収まり見える（旧: 別 label はバー下に描かれ
        // クリップされ不可視だった）。
        let in_folder = self.state.view_kind() == ViewKind::Folder;
        let hint: &str = if !in_folder && self.indexing() && self.state.query().trim().is_empty() {
            "インデックス構築中..."
        } else {
            "検索…"
        };
        let mut buf = if in_folder {
            self.state.folder_filter().to_string()
        } else {
            self.state.query().to_string()
        };
        let response = ui.add(
            egui::TextEdit::singleline(&mut buf)
                .hint_text(hint)
                .desired_width(f32::INFINITY),
        );
        if response.changed() {
            if in_folder {
                self.state.set_folder_filter(buf);
                self.run_search(); // folder は同期フィルタ（debounce 不要・I/O 無し）
            } else {
                self.state.set_query(buf);
                self.state.reset_selection(); // SolidJS parity: 毎打鍵 selected=0（M1 gap 是正）
                // prefix はこの changed エッジで 1 回だけ取得し run_search_with へ渡す
                //（interp と合わせ engine lock の毎打鍵多重取得を避ける・finding 9）。
                let prefix = self.instant_prefix();
                match self.state.interp(&prefix) {
                    QueryIntent::Plain => {
                        self.last_input_at = Instant::now();
                        if self.search_debounce.on_input() {
                            self.run_search_with(&prefix); // leading
                        }
                        ctx.request_repaint_after(self.search_debounce.interval());
                    }
                    QueryIntent::Instant { .. } => {
                        // 同期直フィルタ（30ms debounce 撤廃・spec M3 実装確定）。
                        // plain 由来の armed trailing は掃除（cancelDebounce parity）。
                        self.search_debounce.cancel();
                        self.run_search_with(&prefix);
                    }
                    QueryIntent::Command => {
                        // §15.3: debounce をキャンセルして即実行（changed エッジ＝query 変化時
                        // のみゆえ immediate-mode でも fire-once）。/r と部分入力は run_search の
                        // Command 分岐（冪等: /r=履歴注入・他=結果クリア）。
                        self.search_debounce.cancel();
                        match find_slash_command(self.state.query()) {
                            Some(SlashCmd::History) | None => self.run_search_with(&prefix),
                            Some(cmd) => self.execute_slash(cmd),
                        }
                    }
                }
            }
        }
        // 窓に focus があるのに入力欄が focus を持たないなら移す（Alt+Q 表示直後に打てる）。
        // was_focused に依存しないので、hide→reshow で was_focused が stale でも確実に戻る。
        if focused && !response.has_focus() {
            response.request_focus();
        }

        // trailing debounce: 連打が収まって interval 経過したら最終クエリで検索し直す。
        if self.search_debounce.poll(self.last_input_at.elapsed()) {
            self.run_search();
        }
        // armed のまま = trailing 未発火。scheduler の coalescing で +interval の wake が
        // 消されても deadline で確実に起きるよう毎フレーム残り時間を再要求する。
        if self.search_debounce.is_armed() {
            let remaining = self
                .search_debounce
                .interval()
                .saturating_sub(self.last_input_at.elapsed());
            ctx.request_repaint_after(remaining);
        }

        // Enter: 選択項目を起動/実行。TextEdit の changed 処理より後で判定する——同一フレームに
        // 入力確定（貼り付け・IME 確定）と Enter が入ったとき、旧 state の interp/選択で起動
        // しないため（codex 発見 4・spec M3 実装確定）。egui の input はフレーム内で不変
        // （読む順序は消費に影響しない）ため後置しても Enter は取りこぼさない。
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) && !self.state.results().is_empty() {
            self.activate_or_execute(self.state.selected());
        }

        // 結果リスト（shouldShowResults 相当。results 軸〔plain〕と folder 軸を描く。空なら描かない）。
        let show_results = !self.state.results().is_empty();
        let mut clicked: Option<usize> = None;
        if show_results {
            // 借用衝突回避: results を clone してから描画（draw_result_row は関連関数で self 非借用）。
            let results = self.state.results().to_vec();
            let selected = self.state.selected();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (i, result) in results.iter().enumerate() {
                    if Self::draw_result_row(ui, result, i == selected) {
                        clicked = Some(i); // シングルクリック（§4.8 単=起動）。double は扱わない
                    }
                }
            });
        }
        // シングルクリック＝起動（§4.8 単=起動）。double-click は扱わない（ユーザー決定・
        // as-built でも double-click=選択は到達不能。SPEC §4.8 を as-built へ同期済み）。
        if let Some(i) = clicked {
            self.activate_or_execute(i);
        }

        // 動的ウィンドウ高さ（§4.5/§4.7）。show_results 可否 × max_results から算出し set_size。
        // view 直呼び（SU1 runtime 不変・ユーザー決定）。update はイベントループスレッドで走る
        // ので set_size は安全な見込み（G-RESIZE で確認。本タスクではスモークまで到達しない）。
        let height = compute_window_height(&HeightParams {
            show_results,
            max_results: self.max_results(),
            has_update_toast: false, // SU5
            search_bar_height: 52.0,
            result_row_height: 30.0,
            results_padding: 8.0,
            update_toast_height: 52.0,
        });
        if (height - self.last_set_height).abs() > 0.5 {
            self.last_set_height = height;
            if let Some(window) = self.app_handle.get_window("main") {
                let _ = window.set_size(tauri::LogicalSize::new(self.window_width(), height));
            }
            // 新サイズでの再描画を即要求し 1 フレームの空きを詰める（背景 0x282828 で
            // フラッシュは緩和済みだが空き自体が G-RESIZE のちらつき機構・advisor 指摘）。
            ui.ctx().request_repaint();
        }

        self.was_focused = focused;
    }
}

#[cfg(test)]
mod tests {
    use super::japanese_font_definitions;

    #[test]
    fn jp_font_is_registered_at_index_zero_for_both_families() {
        let dummy: &'static [u8] = &[0u8; 4];
        let fonts = japanese_font_definitions(dummy);
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            let list = fonts.families.get(&family).expect("family present");
            assert_eq!(
                list.first().map(String::as_str),
                Some("jp_font"),
                "jp_font must be index 0 for {family:?}（push=末尾だと #579 再発）"
            );
        }
    }
}
