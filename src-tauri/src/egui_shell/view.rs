//! egui メインウィンドウの検索 view（#532 SU2 外殻 + SU3 検索 driver + SU3.5 tool 選択）。
//! SearchState（純粋核）を駆動する imperative shell: TextEdit/結果リスト描画・直 Engine 検索
//! （debounce）・↑↓/→←ナビ・folder 展開（async ロード + staleness）・tool 選択（§18・
//! Shift+Enter 入場/起動/Escape 復帰）・instant/slash コマンド・起動/実行 dispatch・動的高さ。
//! フォント登録は #532 SU4 で 2 枝へ進化: config font_family 解決時は user_font 先頭 + jp_font
//! fallback（WebView2 CSS スタック parity）、解決失敗時のみ jp_font 単一・index 0（#579 の元不変条件）。

use std::sync::OnceLock;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use snotra_core::config::{OpenerTool, find_matching_tools};
use snotra_core::engine::FolderListContext;
use snotra_core::ui_types::SearchResult;
use snotra_egui_runtime::{EguiView, RuntimeFrame};
use tauri::{Emitter, Manager};

use crate::egui_shell::{
    Debouncer, EscapeOutcome, HeightParams, QueryIntent, SearchState, SlashCmd, ViewKind,
    compute_parent_dir, compute_window_height, find_slash_command, folder_load_pending,
};

static JP_FONT_BYTES: OnceLock<Box<[u8]>> = OnceLock::new();

fn font_definitions(
    jp_bytes: &'static [u8],
    user: Option<(Vec<u8>, u32)>,
) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    let mut jp = egui::FontData::from_static(jp_bytes);
    jp.tweak = egui::FontTweak {
        scale: 1.0,
        y_offset_factor: 0.3,
        y_offset: 0.0,
        ..Default::default()
    };
    fonts.font_data.insert("jp_font".to_owned(), jp.into());
    match user {
        Some((bytes, face_index)) => {
            let mut uf = egui::FontData::from_owned(bytes);
            uf.index = face_index; // TTC face 指定（settings font.rs:138 と同型）
            fonts.font_data.insert("user_font".to_owned(), uf.into());
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                // user_font 先頭（font_family 優先）+ jp_font fallback（CJK 被覆）= CSS スタック parity。
                let list = fonts.families.entry(family).or_default();
                list.insert(0, "jp_font".to_owned());
                list.insert(0, "user_font".to_owned());
            }
        }
        None => {
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                // 解決失敗時は jp_font 単一・先頭（#579: push=末尾だとベースラインずれ再発）。
                fonts.families.entry(family).or_default().insert(0, "jp_font".to_owned());
            }
        }
    }
    fonts
}

/// config font_family をシステムから解決して (バイト列, face index) を返す。
/// 見つからなければ None（呼び出し側が jp_font 単一へフォールバック）。Database は
/// 解決後に drop（非常駐・列挙コストはフォント設定時の一度きり）。
fn resolve_font_family(name: &str) -> Option<(Vec<u8>, u32)> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    let query = fontdb::Query {
        families: &[fontdb::Family::Name(name)],
        weight: fontdb::Weight::NORMAL,
        stretch: fontdb::Stretch::Normal,
        style: fontdb::Style::Normal,
    };
    let id = db.query(&query)?;
    db.with_face_data(id, |data, face_index| (data.to_vec(), face_index))
}

fn configure_japanese_font(context: &egui::Context, font_family: &str) {
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
        let user = resolve_font_family(font_family);
        context.set_fonts(font_definitions(static_bytes, user));
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

/// 起動 worker への仕事（#631・spec C 節）。worker スレッドが実行し、成功時の履歴記録も
/// worker 側で行う（spec 決定 5: WebView2 が backend 側で UI 可視性と無関係に記録する parity。
/// hide 中に完了した起動の記録消失 gap を閉じる）。Instant は記録しない（IPC 経路 parity）。
enum LaunchWork {
    /// 通常起動（§4.8）。tools 先頭があれば launch_with_tool_core、無ければ launch_item_core。
    Normal { path: String, query: String, tools: Vec<OpenerTool> },
    /// ツール選択起動（§18.4）。
    Tool { target_path: String, launch_query: String, exe: String, args: String },
    /// instant 実行（§19.6）。clipboard 読み + 展開 + 実行の全体を worker で行う
    /// （engine ロック内の action 抽出だけ UI スレッド・spec C 節）。
    Instant { name: String, action: snotra_core::config::InstantAction, instant_query: String },
}

/// 起動成功時に drain が行う UI 後処理の種別（M1/M3 の同期版と同じ末尾へ合流させる）。
#[derive(Clone, Copy)]
enum LaunchTag {
    Normal,  // emit_hide のみ（M1 activate parity・クエリは次 show の reset で消える）
    Tool,    // clear_search + state.reset + emit_hide（execute_tool_selected parity）
    Instant, // clear_search + emit_hide（execute_instant_selected parity）
}

/// in-flight 起動（spec C 節 不変条件 1: channel は per-launch）。rx を本構造体が所有し、
/// `launching = None` で Receiver ごと drop → worker の遅着 send は Err で自然消滅する。
/// folder の「view 寿命の共有 channel + 世代 token」をコピーしないこと（token が要るのは
/// 共有 channel だから。per-launch なら不要——並行性レビューで確定）。
struct LaunchInFlight {
    started: Instant,
    rx: Receiver<crate::commands::launch::LaunchResult>,
    tag: LaunchTag,
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
    /// path→TextureHandle（セッション内保持。可視集合に頭打ち・#532 SU4）。
    icon_textures: std::collections::HashMap<String, egui::TextureHandle>,
    /// 抽出済みだが PNG 化できなかった/存在しない path（再抽出しない・SU4）。
    icon_missing: std::collections::HashSet<String>,
    /// worker へ渡し済みだが IconMsg 未 drain の path（in-flight）。request_icons_for_results の
    /// wanted 収集から除外し、同一 settle に対する repaint 毎の重複 spawn を防ぐ（thread pileup 対策）。
    /// drain 時（Loaded/Missing）に remove、reset_pending 消費時に clear。retain の対象外
    /// （in-flight worker の結果が届くまで残す＝可視外へスクロールしても drain until remove）。
    icon_pending: std::collections::HashSet<String>,
    icon_tx: Sender<crate::egui_shell::IconMsg>,
    icon_rx: Receiver<crate::egui_shell::IconMsg>,
    /// ナビゲーションでロードした (ctx, 全ソート済み) キャッシュ。打鍵フィルタの源（#532 SU3 M2）。
    folder_cache: Option<(FolderListContext, Vec<SearchResult>)>,
    /// 列挙失敗時の単一エラー行（filter を無視して表示）。
    folder_error: Option<Vec<SearchResult>>,
    /// 表示中の results の来歴: instant 候補なら Some(instant_query)。Enter/クリック/← の判定は
    /// live config の prefix から interp を再導出せず、この snapshot を使う——prefix の hot-change
    /// 後に stale instant 行（path=description/display）が activate() へ流れて文字列をパスとして
    /// 起動・履歴汚染するのを防ぐ（/code-review #637 finding 0）。run_search が行と一体で更新する。
    instant_rows_query: Option<String>,
    /// 直近に scroll_to_me した選択 index。選択変化時のみ scroll するための gate（#632）。
    last_scrolled_selected: Option<usize>,
    /// in-flight 起動（single-flight の実体: Some の間は新規起動 dispatch を拒否）。
    launching: Option<LaunchInFlight>,
    /// 一時通知（起動失敗/結果不明）。時刻は notice_base からの経過で注入（純粋核）。
    notice: crate::egui_shell::NoticeSlot,
    /// notice の単調時刻基準（view 生成時に固定・Instant 差分を Duration で渡す）。
    notice_base: Instant,
}

impl SearchWindowView {
    pub(crate) fn new(app_handle: tauri::AppHandle) -> Self {
        let (folder_tx, folder_rx) = channel();
        let (icon_tx, icon_rx) = channel();
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
            icon_textures: std::collections::HashMap::new(),
            icon_missing: std::collections::HashSet::new(),
            icon_pending: std::collections::HashSet::new(),
            icon_tx,
            icon_rx,
            folder_cache: None,
            folder_error: None,
            instant_rows_query: None,
            last_scrolled_selected: None,
            launching: None,
            notice: crate::egui_shell::NoticeSlot::default(),
            notice_base: Instant::now(),
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

    /// index 行の起動を worker へ投げる（§4.8 シングルクリック / Enter・#631 async 化）。
    /// 実起動（ShellExecuteW）と成功時の履歴記録は `start_launch` の worker スレッド側で行う
    /// （§4.3/§5 の query_count 加点・全起動経路の共通末尾は `finish_launch` へ合流）。
    /// エラー行（is_error）／フォルダロード中（cache・error 未着で results が stale）は起動しない。
    /// Enter とシングルクリックの単一チョークポイント（#636 レビュー Finding A）。
    fn activate(&mut self, index: usize, ctx: &egui::Context) {
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
        let is_folder = result.is_folder;
        let query = self.state.query().to_string();
        let tools = self.resolve_tools(&path, is_folder);
        crate::trace_main(
            "egui_launch",
            serde_json::json!({ "index": index, "opener": !tools.is_empty() }),
        );
        self.start_launch(LaunchWork::Normal { path, query, tools }, LaunchTag::Normal, ctx);
    }

    /// 起動を per-launch worker スレッドへ投げる（#631・spec C 節）。single-flight:
    /// in-flight 中は拒否（WebView2 activationLane parity・二重起動防止）。突入時に results を
    /// クリアする（withLaunchLifecycle の await 前 clearResults parity・spec 決定 7）——
    /// launching 中は 52px collapse・↑↓/クリックは空リストゆえ自然に inert。クエリは保持。
    fn start_launch(&mut self, work: LaunchWork, tag: LaunchTag, ctx: &egui::Context) {
        if self.launching.is_some() {
            return; // single-flight 拒否（拒否された Enter が後で再生されるキューは egui に無い）
        }
        let (tx, rx) = channel::<crate::commands::launch::LaunchResult>();
        self.launching = Some(LaunchInFlight { started: Instant::now(), rx, tag });
        self.state.set_results(Vec::new());
        self.instant_rows_query = None; // 行が消えるため来歴も一体でクリア（finding 0 の規律）
        self.last_scrolled_selected = None;
        let app = self.app_handle.clone();
        let egui_ctx = ctx.clone();
        std::thread::spawn(move || {
            use crate::commands::launch::{LaunchStatus, launch_item_core, launch_with_tool_core, record_and_save};
            let (outcome, record) = match work {
                LaunchWork::Normal { path, query, tools } => {
                    let o = if let Some(first) = tools.first() {
                        launch_with_tool_core(&path, &first.exe, &first.args)
                    } else {
                        launch_item_core(&path)
                    };
                    (o, Some((path, query)))
                }
                LaunchWork::Tool { target_path, launch_query, exe, args } => {
                    let o = launch_with_tool_core(&target_path, &exe, &args);
                    (o, Some((target_path, launch_query)))
                }
                LaunchWork::Instant { name, action, instant_query } => {
                    // clipboard 読み（Win32）はロック外・worker 内（commands/instant.rs と同順）。
                    let clipboard = arboard::Clipboard::new()
                        .and_then(|mut cb| cb.get_text())
                        .unwrap_or_default();
                    let o = crate::commands::instant::execute_instant_action_core(
                        action, &instant_query, &clipboard,
                    );
                    crate::trace_main(
                        "egui_instant",
                        serde_json::json!({ "name": name, "status": format!("{:?}", o.status) }),
                    );
                    (o, None) // instant は履歴を記録しない（IPC 経路 parity）
                }
            };
            // 履歴記録は worker 側（spec 決定 5）。timeout で drain が破棄済みでも記録は行われる
            // ＝「実際に起動したのに履歴が無い」窓を Normal/Tool では作らない。
            if matches!(outcome.status, LaunchStatus::Ok)
                && let Some((path, query)) = record
                && let Some(state) = app.try_state::<crate::AppState>()
            {
                record_and_save(&state, &path, &query);
            }
            let _ = tx.send(outcome); // 遅着（rx drop 済み）は Err で自然消滅（不変条件 1）
            egui_ctx.request_repaint(); // イベント駆動 runtime を起こす（folder/icon と同理由）
        });
    }

    /// drain が回収した結果の UI 後処理（成功列は M1/M3 同期版と同じ末尾へ合流）。
    fn finish_launch(&mut self, tag: LaunchTag, outcome: crate::commands::launch::LaunchResult) {
        use crate::commands::launch::LaunchStatus;
        crate::trace_main(
            "egui_launch_done",
            serde_json::json!({ "status": format!("{:?}", outcome.status) }),
        );
        let l = self.lang();
        match outcome.status {
            LaunchStatus::Ok => match tag {
                LaunchTag::Normal => self.emit_hide(),
                LaunchTag::Tool => {
                    self.clear_search();
                    self.state.reset();
                    self.emit_hide();
                }
                LaunchTag::Instant => {
                    self.clear_search();
                    self.emit_hide();
                }
            },
            LaunchStatus::Failed | LaunchStatus::Timeout => {
                // 失敗: hide しない・同期 run_search で結果を再取得（runRefresh parity）+ 一時通知。
                // Timeout ステータスがここへ来るのは core が同期 Timeout を返す場合のみ
                // （drain 側の 4 秒は Empty 経路で扱う）。文言は失敗系で扱う。
                let detail = outcome
                    .message
                    .as_deref()
                    .map(|m| format!(" ({m})"))
                    .unwrap_or_default();
                self.notice.set(
                    crate::egui_shell::ui_strings::launch_failed(l, &detail),
                    self.notice_base.elapsed(),
                    crate::egui_shell::NOTICE_LAUNCH,
                );
                self.run_search();
            }
        }
    }

    /// フレーム毎の in-flight 回収（spec C 節 不変条件 2: **reset_pending 消費の後**に呼ぶ。
    /// 前に置くと show 直後フレームで stale Ok が reset より先に処理され再 show 窓を hide で撃つ）。
    fn drain_launch(&mut self, ctx: &egui::Context) {
        let Some(inflight) = &self.launching else { return };
        match inflight.rx.try_recv() {
            Ok(outcome) => {
                let tag = inflight.tag;
                self.launching = None;
                self.finish_launch(tag, outcome);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                let elapsed = inflight.started.elapsed();
                if elapsed >= crate::egui_shell::LAUNCH_TIMEOUT {
                    // 4 秒経過＝「結果不明」（spec 決定 8）。rx ごと破棄→遅着は自然消滅。
                    // 起動という副作用は取り消せない（abandoned spawn_blocking parity）。
                    self.launching = None;
                    let l = self.lang();
                    self.notice.set(
                        crate::egui_shell::ui_strings::launch_timeout(l, ""),
                        self.notice_base.elapsed(),
                        crate::egui_shell::NOTICE_LAUNCH,
                    );
                    self.run_search(); // WebView2 timeout 分岐（runRefresh）parity
                } else {
                    // deadline で確実に起きる（**可視中のみ有効**——hidden 中に update() が
                    // 走らない場合は次 show まで宙吊りになるが、reset-on-show の launching
                    // クリアが backstop・spec C 節「hidden 中の drain」）。
                    ctx.request_repaint_after(crate::egui_shell::LAUNCH_TIMEOUT - elapsed);
                }
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                // worker panic 等の異常終了。失敗扱いで回復（永久 in-flight を防ぐ）。
                self.launching = None;
                let l = self.lang();
                self.notice.set(
                    crate::egui_shell::ui_strings::launch_failed(l, ""),
                    self.notice_base.elapsed(),
                    crate::egui_shell::NOTICE_LAUNCH,
                );
                self.run_search();
            }
        }
    }

    /// クエリ/結果/armed trailing/instant 来歴をまとめてクリアする単一チョークポイント
    /// （execute_slash・instant 成功経路・`execute_tool_selected` が共有）。追加のクリアサイトが
    /// `search_debounce.cancel()` を書き忘れて「クリア後に stale trailing 検索が発火」を
    /// 再発させないための集約（/code-review #637 finding 6）。
    fn clear_search(&mut self) {
        self.state.set_query(String::new());
        self.state.set_results(Vec::new());
        self.search_debounce.cancel();
        self.instant_rows_query = None;
        self.last_scrolled_selected = None; // 再表示後に確実に一度 scroll し直す（#632）
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

    /// 選択中の instant コマンドの action を抽出し worker へ投げる（§19.6・#631 async 化）。
    /// action 抽出はここ（UI スレッド・engine ロック内）で行い、clipboard 読み + 実行は
    /// `start_launch` の worker スレッド側（IPC の execute_instant_command と同じ手順・
    /// action 抽出をロック内・clipboard 読みをロック外）。instant は履歴を記録しない
    /// （IPC 経路 parity）。成功/失敗の後処理は `finish_launch` へ合流。
    fn execute_instant_selected(&mut self, index: usize, instant_query: &str, ctx: &egui::Context) {
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
        self.start_launch(
            LaunchWork::Instant { name, action, instant_query: instant_query.to_string() },
            LaunchTag::Instant,
            ctx,
        );
    }

    /// Enter/クリックの単一 dispatch（§19.6/§4.8・#532 SU3 M3）。判定は live config の prefix
    /// からの interp 再導出ではなく**表示行の来歴**（`instant_rows_query` snapshot）で行う——
    /// prefix の hot-change 後に stale instant 行（path=description/display）を activate() へ流し、
    /// 文字列をパスとして起動・履歴汚染するのを防ぐ（/code-review #637 finding 0）。行と来歴は
    /// run_search が同一フレームで一体更新するため常に整合する。行 index で参照（パス文字列を
    /// 使わない・ui ルール踏襲）。tool ビュー中は Shift の有無に依らず `execute_tool_selected` へ
    /// 振る（`shift_activate` が Tool ビューではここへ委譲するため到達点が一致する）。
    fn activate_or_execute(&mut self, index: usize, ctx: &egui::Context) {
        if self.state.view_kind() == ViewKind::Tool {
            self.execute_tool_selected(index, ctx); // §18.4 Enter/クリック＝選択ツールで起動
        } else if let Some(iq) = self.instant_rows_query.clone() {
            self.execute_instant_selected(index, &iq, ctx);
        } else {
            self.activate(index, ctx);
        }
    }

    /// Shift+Enter（§18.3）: 選択行の tools ≥ 2 ならツール選択メニューへ、それ以外
    /// （≤1・instant 行・tool ビュー中）は通常 Enter と同一（hide も同様）。folder ロード
    /// 未確定窓は activate と同じ理由で入場もしない（stale 行からの解決防止・#636 Finding A）。
    fn shift_activate(&mut self, index: usize, ctx: &egui::Context) {
        if self.instant_rows_query.is_some() || self.state.view_kind() == ViewKind::Tool {
            // instant 行は §19.6「Shift+Enter=Enter」。tool ビュー中の Shift+Enter も Enter と同一。
            self.activate_or_execute(index, ctx);
            return;
        }
        if folder_load_pending(
            self.state.view_kind(),
            self.folder_cache.is_some(),
            self.folder_error.is_some(),
        ) {
            return;
        }
        let Some(row) = self.state.results().get(index) else { return };
        if row.is_error {
            return;
        }
        let (path, is_folder) = (row.path.clone(), row.is_folder);
        let tools = self.resolve_tools(&path, is_folder);
        if tools.len() >= 2 {
            // armed trailing がツール一覧を上書きしないよう掃除（finding 6 と同じ債務）
            self.search_debounce.cancel();
            self.state.enter_tool(path, is_folder, tools);
            if let Some(f) = self.state.tool_frame() {
                crate::trace_main(
                    "egui_tool_enter",
                    serde_json::json!({
                        "target_is_folder": f.target_is_folder,
                        "tools": f.tools.len(),
                    }),
                );
            }
        } else {
            self.activate_or_execute(index, ctx); // §18.3: 1 件以下は通常 Enter と同じ動作
        }
    }

    /// ツール選択中の起動（§18.4）。行 index で tools を照合（同一 exe でも引数違いを区別・
    /// パス文字列照合は禁止＝ui ルールと同根）。成功時は IPC `launch_with_tool` と同じく
    /// launch_query で履歴記録 → 全クリア + hide（§19.6 instant の完了列と同型。reset は
    /// tool/folder/gen 込みで in-flight folder ロードも失効させる）。
    fn execute_tool_selected(&mut self, index: usize, ctx: &egui::Context) {
        let Some((target_path, launch_query, tool)) = self.state.tool_frame().and_then(|f| {
            f.tools
                .get(index)
                .map(|t| (f.target_path.clone(), f.launch_query.clone(), t.clone()))
        }) else {
            return;
        };
        crate::trace_main("egui_tool_launch", serde_json::json!({ "index": index }));
        self.start_launch(
            LaunchWork::Tool {
                target_path,
                launch_query,
                exe: tool.exe.clone(),
                args: tool.args.clone(),
            },
            LaunchTag::Tool,
            ctx,
        );
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

    /// 選択行のオープナー解決（§18.3 最具体ルール 1 件の tools）。IPC/トレイと同じ core
    /// `find_matching_tools` を共有（drift 防止）。is_folder は行（index の真実）から渡し、
    /// `resolve_all_openers` の `Path::is_dir` 再判定（fs touch・dead UNC で滞留しうる）を
    /// egui 経路では踏まない。lock は解決の間だけ保持（ロック内純 CPU → clone）。
    fn resolve_tools(&self, path: &str, is_folder: bool) -> Vec<OpenerTool> {
        let Some(state) = self.app_handle.try_state::<crate::AppState>() else {
            return Vec::new();
        };
        let engine = state.engine.lock().unwrap();
        find_matching_tools(path, is_folder, &engine.config().openers).to_vec()
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

    /// UI 文言の言語（config general.language・起動時一回でなく都度読み——lock 1 回/フレームの
    /// 既存ヘルパー群と同型。SU6 の hot-reload 拡張時もこの読み口のまま動く）。
    fn lang(&self) -> snotra_core::config::Language {
        self.app_handle
            .try_state::<crate::AppState>()
            .map(|s| s.engine.lock().unwrap().config().general.language)
            .unwrap_or(snotra_core::config::Language::Ja)
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

    /// 現結果集合の未取得アイコンを別スレッドで抽出し IconMsg を channel へ送る（SU4）。
    /// folder の per-nav thread パターン踏襲。token は載せない（staleness は path キーで無害）。
    /// show_icons=false 時は呼ばない（呼び出し側でガード）。
    fn spawn_icon_load(&self, paths: Vec<String>, egui_ctx: egui::Context) {
        if paths.is_empty() {
            return;
        }
        let app = self.app_handle.clone();
        let tx = self.icon_tx.clone();
        std::thread::spawn(move || {
            let (Some(state), Some(icons)) = (
                app.try_state::<crate::AppState>(),
                app.try_state::<crate::icon::IconCacheState>(),
            ) else {
                return;
            };
            let loaded = crate::commands::load_icon_pngs(&state, &icons, paths);
            for (path, png) in loaded {
                let msg = match png.and_then(|b| crate::egui_shell::png_to_color_image(&b)) {
                    Some(img) => crate::egui_shell::IconMsg::Loaded(path, img),
                    None => crate::egui_shell::IconMsg::Missing(path),
                };
                let _ = tx.send(msg);
            }
            egui_ctx.request_repaint(); // イベント駆動 runtime を起こす（folder と同理由）
        });
    }

    /// show_icons を実行中 config から都度読む（キャッシュしない・#576 と同設計）。
    fn show_icons(&self) -> bool {
        self.app_handle
            .try_state::<crate::AppState>()
            .map(|s| s.engine.lock().unwrap().config().appearance.show_icons)
            .unwrap_or(true)
    }

    /// 現結果の未取得アイコンを worker に積む（settled 相当・描画前に呼ぶ）。連打中は
    /// debounce armed のため呼ばない（呼び出し側で is_armed ガード）。in-flight（icon_pending）
    /// の path は除外し、抽出中の repaint（マウス移動・カーソル点滅等）による同一 path 集合への
    /// 重複 spawn を防ぐ（thread pileup 対策）。spawn した path は icon_pending へ積み、
    /// drain（Loaded/Missing）で remove する。wanted が空なら insert も spawn もしない。
    fn request_icons_for_results(&mut self, ctx: &egui::Context) {
        if !self.show_icons() {
            return;
        }
        let mut wanted: Vec<String> = Vec::new();
        for r in self.state.results() {
            if !r.is_error
                && crate::egui_shell::needs_extraction(&r.path, &self.icon_textures, &self.icon_missing)
                && !self.icon_pending.contains(&r.path)
                && !wanted.contains(&r.path)
            {
                wanted.push(r.path.clone());
            }
        }
        if wanted.is_empty() {
            return;
        }
        for p in &wanted {
            self.icon_pending.insert(p.clone());
        }
        self.spawn_icon_load(wanted, ctx.clone());
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
        // 結果が総入れ替えされうる箇所ゆえ scroll gate をリセットする。selected index の
        // みをキーにすると、手動スクロール後の打鍵で結果が置換されても selected=0 のままだと
        // do_scroll=false になり新結果の選択行が画面外に留まる（#632 reviewer Important 3）。
        self.last_scrolled_selected = None;
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

    /// 1 行を描画。selected かつ scroll なら scroll_to_me（選択変化時のみ・#632）。返り値:
    /// single_clicked。ダブルクリックは扱わない（ユーザー決定: §4.8 の double-click=選択は
    /// as-built でも到達不能ゆえ落とす。単クリック=起動のみ）。self を借りない関連関数
    /// （借用衝突回避）。色/サイズは呼び出し側が都度導出する `RowTheme` から取る。
    /// name/path の重なりは name galley の実幅を測って path 開始 x を決めることで防ぐ
    /// （#632）。path は中間省略（`truncate_middle`）で利用可能幅に収める。
    /// `show_icons=false` はアイコン slot 自体を畳む（skip でなくレイアウト変更・#532 SU4 Task 6）
    /// ——テキストが左端 8px 寄せになり、slot 分の空白が残らない。
    fn draw_result_row(
        ui: &mut egui::Ui,
        result: &SearchResult,
        selected: bool,
        scroll: bool,
        icon: Option<&egui::TextureHandle>,
        show_icons: bool,
        theme: &RowTheme,
    ) -> bool {
        let row_h = 30.0;
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), row_h),
            egui::Sense::click(),
        );
        if selected {
            ui.painter().rect_filled(rect, 4.0, theme.selection);
            if scroll {
                response.scroll_to_me(Some(egui::Align::Center)); // 選択変化時のみ（#632）
            }
        }
        // アイコン: show_icons=true のときのみ左 28px slot の中央に 16x16 を描く。欠落
        // （icon=None）は drawn placeholder（draw_icon_fallback）で埋める。
        let slot = if show_icons { 28.0 } else { 8.0 };
        if show_icons {
            match icon {
                Some(tex) => {
                    let icon_size = 16.0;
                    let icon_rect = egui::Rect::from_center_size(
                        egui::pos2(rect.left() + 14.0, rect.center().y),
                        egui::vec2(icon_size, icon_size),
                    );
                    ui.painter().image(
                        tex.id(),
                        icon_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }
                // 通常の欠落のみ placeholder。エラー行（is_error＝フォルダ列挙失敗行等）には
                // アイコン形の装飾を描かない（エラーメッセージに不要・whole-branch review Minor）。
                None if !result.is_error => draw_icon_fallback(ui, rect, result, theme),
                None => {}
            }
        }
        let text_x = rect.left() + slot;
        let right = rect.right() - 8.0;
        let cy = rect.center().y;
        // name galley を作り、実幅から path 開始 x を決める（重なり回避）。name が幅の 60%
        // を超えたら単一行 + 末尾 … 省略にクリップする。`Painter::layout`（simple 版）は
        // wrap_width で折り返す（複数行化）だけなので、30px 固定行をはみ出して隣接行と
        // 重なる（#632 reviewer Important 1）。`TextWrapping::truncate_at_width` で
        // max_rows=1 + break_anywhere を明示し、折り返しではなく省略にする。
        let name_max = (right - text_x) * 0.6;
        let mut name_job = egui::text::LayoutJob::single_section(
            result.name.clone(),
            egui::TextFormat {
                font_id: egui::FontId::proportional(theme.name_size),
                color: theme.name_color,
                ..Default::default()
            },
        );
        name_job.wrap = egui::text::TextWrapping::truncate_at_width(name_max);
        let name_galley = ui.painter().layout_job(name_job);
        ui.painter().galley(
            egui::pos2(text_x, cy - name_galley.size().y / 2.0),
            name_galley.clone(),
            theme.name_color,
        );
        // truncate_at_width 済みゆえ実幅は既に name_max 以下（.min は不要）。
        let path_x = text_x + name_galley.size().x + 12.0;
        // path は右寄せ・path_x 以降に収まる幅で中間省略。egui galley は末尾省略のため、
        // 中間省略は truncate_middle（純関数）で文字列側を縮めてから描く。per-char 幅は
        // 固定係数ではなく実 galley から実測する（CJK 過小評価対策・reviewer Important 2）。
        let path_avail = (right - path_x).max(0.0);
        let path_full = ui.painter().layout_no_wrap(
            result.path.clone(),
            egui::FontId::proportional(theme.path_size),
            theme.path_color,
        );
        let path_str = if path_full.size().x <= path_avail {
            result.path.clone()
        } else {
            let per_char_px = path_full.size().x / (result.path.chars().count().max(1) as f32);
            truncate_middle(&result.path, path_avail, per_char_px)
        };
        ui.painter().text(
            egui::pos2(right, cy),
            egui::Align2::RIGHT_CENTER,
            &path_str,
            egui::FontId::proportional(theme.path_size),
            theme.path_color,
        );
        response.clicked()
    }

    /// 実行中 config テーマ値から 1 結果行の描画テーマを都度導出する（キャッシュしない・
    /// #576 と同設計）。config が読めなければ既定値へフォールバック。
    fn row_theme(&self) -> RowTheme {
        let (text, hint, sel, size) = self
            .app_handle
            .try_state::<crate::AppState>()
            .map(|s| {
                let engine = s.engine.lock().unwrap();
                let v = &engine.config().visual;
                (v.text_color.clone(), v.hint_text_color.clone(),
                 v.selected_row_color.clone(), v.font_size)
            })
            .unwrap_or_else(|| ("#E0E0E0".into(), "#808080".into(), "#333333".into(), 15));
        RowTheme {
            name_color: hex_color(&text, egui::Color32::from_rgb(0xE0, 0xE0, 0xE0)),
            path_color: hex_color(&hint, egui::Color32::from_rgb(0x80, 0x80, 0x80)),
            selection: hex_color(&sel, egui::Color32::from_rgb(0x33, 0x33, 0x33)),
            name_size: size as f32,
            path_size: (size as f32 * 0.78).max(9.0), // WebView2 の name>path 比を踏襲
        }
    }

    /// toast ボタンの処理（#532 SU5）。install は Update を原子取得して async へ（Task 8）。
    fn handle_toast_action(&mut self, action: ToastAction) {
        let Some(st) = self.app_handle.try_state::<crate::egui_shell::UpdaterUiState>() else {
            return;
        };
        match action {
            ToastAction::Dismiss => {
                let _ = st.0.lock().unwrap().dismiss(); // Installing 中は拒否（false）＝無視
            }
            ToastAction::Install => {
                let taken = st.0.lock().unwrap().try_begin_install();
                if let Some(update) = taken {
                    self.spawn_install(update);
                } else {
                    crate::trace_main("egui_update_install_noop", serde_json::json!({}));
                }
            }
        }
    }

    /// install 実行（§20.4・spec B 節）。`download_and_install` は Windows では内部で
    /// download → `on_before_exit`（=flush_persistent_state・Task 6 で builder に登録済み）→
    /// installer 起動 → `std::process::exit(0)` し**復帰しない**（updater.rs:865）。
    /// Err 復帰時のみ InstallFailed へ遷移して toast をエラー表示にする（updaterError parity）。
    fn spawn_install(&self, update: Box<tauri_plugin_updater::Update>) {
        let handle = self.app_handle.clone();
        crate::trace_main("egui_update_install_begin", serde_json::json!({ "version": update.version }));
        tauri::async_runtime::spawn(async move {
            match update.download_and_install(|_, _| {}, || {}).await {
                Ok(()) => {
                    // Windows では到達しない（内部 exit）。他 OS ビルドや将来変更の防波堤として trace。
                    crate::trace_main("egui_update_install_returned", serde_json::json!({}));
                }
                Err(e) => {
                    crate::trace_main(
                        "egui_update_install_failed",
                        serde_json::json!({ "error": e.to_string() }),
                    );
                    if let Some(st) = handle.try_state::<crate::egui_shell::UpdaterUiState>() {
                        st.0.lock().unwrap().phase =
                            crate::egui_shell::UpdaterPhase::InstallFailed { message: e.to_string() };
                    }
                    if let Some(sh) = handle.try_state::<crate::egui_shell::EguiShellState>()
                        && let Ok(guard) = sh.egui_ctx.lock()
                        && let Some(ctx) = guard.as_ref()
                    {
                        ctx.request_repaint(); // 可視中の失敗を即座に描く
                    }
                }
            }
        });
    }
}

/// `#RRGGBB` 文字列を Color32 へ。失敗時は fallback（release は panic=abort ゆえ unwrap しない）。
fn hex_color(s: &str, fallback: egui::Color32) -> egui::Color32 {
    egui::Color32::from_hex(s).unwrap_or(fallback)
}

/// アイコン欠落時の fallback（drawn placeholder）。§3.4 は 📁📄 を規定するが softbuffer +
/// 単一 TTF で色 emoji が描けない懸念があるため単色プレースホルダに倒す（視覚スモークは
/// Task 7 に集約・コントローラ決定）。Task 7 の視覚スモークで jp_font が 📁📄 を描けると
/// 確認できたら emoji へ upgrade を検討する。
fn draw_icon_fallback(ui: &egui::Ui, rect: egui::Rect, result: &SearchResult, theme: &RowTheme) {
    let center = egui::pos2(rect.left() + 14.0, rect.center().y);
    let r = egui::Rect::from_center_size(center, egui::vec2(14.0, 14.0));
    let col = if result.is_folder { theme.name_color } else { theme.path_color };
    ui.painter().rect_filled(r, 2.0, col.linear_multiply(0.5));
}

/// path を avail_px におよそ収める中間省略（`C:\a\...\app.exe`）。`per_char_px` は呼び出し側が
/// 実 galley（`Painter::layout_no_wrap`）から実測した平均文字幅を渡す（固定係数 size*0.55 は
/// Latin 想定で CJK グリフ（~1.0-1.8×）を過小評価し under-truncate する・reviewer Important 2）。
/// release は panic=abort ゆえ、`max_chars < 4` ガードと空文字境界で範囲外アクセスを避ける。
fn truncate_middle(s: &str, avail_px: f32, per_char_px: f32) -> String {
    let per = per_char_px.max(1.0);
    let max_chars = (avail_px / per).floor() as usize;
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars || max_chars < 4 {
        return s.to_string();
    }
    let keep = max_chars - 1; // '…' の分
    let head = keep / 2;
    let tail = keep - head;
    let mut out: String = chars[..head].iter().collect();
    out.push('…');
    out.extend(&chars[chars.len() - tail..]);
    out
}

/// 1 結果行の描画テーマ（config テーマ値から都度導出・#576 と同設計でキャッシュしない）。
struct RowTheme {
    name_color: egui::Color32,
    path_color: egui::Color32,
    selection: egui::Color32,
    name_size: f32,
    path_size: f32,
}

/// toast ボタン種別（クリック結果を borrow 外で処理するための遅延 dispatch）。
enum ToastAction {
    Install,
    Dismiss,
}

/// 右端から左へ詰める toast ボタン 1 個。クリックされたら true。disabled は淡色 + 無反応。
///
/// id は `label` から導出する（`ui.next_auto_id()` は非 mutating getter のため、同一フレーム内で
/// 中間の widget allocation を挟まず2回呼ぶと dismiss/install 両ボタンが同一 id になり
/// egui の id クラッシュ検知に触れる——ローカライズ済みラベルは Available 局面で互いに異なるため
/// これを id salt に使う）。
fn draw_toast_button(
    ui: &mut egui::Ui,
    cursor_x: &mut f32,
    center_y: f32,
    label: &str,
    enabled: bool,
    theme: &RowTheme,
) -> bool {
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        egui::FontId::proportional(12.0),
        theme.name_color,
    );
    let w = galley.size().x + 16.0;
    let rect = egui::Rect::from_min_max(
        egui::pos2(*cursor_x - w, center_y - 11.0),
        egui::pos2(*cursor_x, center_y + 11.0),
    );
    *cursor_x -= w + 8.0;
    let id = ui.id().with(("toast_btn", label));
    let response = ui.interact(rect, id, egui::Sense::click());
    let color = if enabled { theme.name_color } else { theme.path_color };
    ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0, color), egui::StrokeKind::Inside);
    ui.painter().galley(
        egui::pos2(rect.left() + 8.0, center_y - galley.size().y / 2.0),
        galley,
        color,
    );
    enabled && response.clicked()
}

impl EguiView for SearchWindowView {
    fn setup(&mut self, context: &egui::Context) {
        let font_family = self
            .app_handle
            .try_state::<crate::AppState>()
            .map(|s| s.engine.lock().unwrap().config().visual.font_family.clone())
            .unwrap_or_else(|| "Segoe UI".to_string());
        configure_japanese_font(context, &font_family);
        // updater check 完了時の wake-up 用（mod.rs spawn_update_check が読む・#532 SU5）。
        if let Some(sh) = self.app_handle.try_state::<crate::egui_shell::EguiShellState>()
            && let Ok(mut guard) = sh.egui_ctx.lock()
        {
            *guard = Some(context.clone());
        }
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
            self.last_scrolled_selected = None; // 再表示後に確実に一度 scroll し直す（#632）
            // hide 中の常駐テクスチャを残さない（メモリ境界・SU4 決定 A）。
            self.icon_textures.clear();
            self.icon_missing.clear();
            self.icon_pending.clear(); // in-flight 追跡も show 直後に全 clear（thread pileup 対策）
            // SU5: in-flight 起動と一時通知は show を跨がない（resetForShow の
            // setLaunching(false) + clearLaunchNotice parity）。rx ごと drop するため
            // hide 中に完了した遅着結果もここで自然消滅する（stale Ok が再 show 窓を
            // hide で撃つ事故の backstop・並行性レビュー High）。updater toast は触らない。
            self.launching = None;
            self.notice.clear();
        }

        let ctx = ui.ctx().clone();

        // §11: パネル/入力欄/選択色を config テーマから（ハードコード撤廃・runtime CLEAR_COLOR は不変）。
        if let Some(s) = self.app_handle.try_state::<crate::AppState>() {
            let (bg, input_bg, sel) = {
                let engine = s.engine.lock().unwrap();
                let v = &engine.config().visual;
                (v.background_color.clone(), v.input_background_color.clone(), v.selected_row_color.clone())
            };
            let mut visuals = ctx.style_of(ctx.theme()).visuals.clone();
            visuals.panel_fill = hex_color(&bg, egui::Color32::from_rgb(0x28, 0x28, 0x28));
            visuals.window_fill = visuals.panel_fill;
            visuals.extreme_bg_color = hex_color(&input_bg, egui::Color32::from_rgb(0x38, 0x38, 0x38)); // TextEdit 背景
            visuals.selection.bg_fill = hex_color(&sel, egui::Color32::from_rgb(0x33, 0x33, 0x33));
            ctx.set_visuals(visuals);
        }

        // 起動結果の回収（#631）。reset_pending 消費の後に置くこと（spec C 節 不変条件 2）。
        self.drain_launch(&ctx);
        // 一時通知の期限管理（期限切れで repaint・表示中は残余で wake 予約）。
        if self.notice.poll(self.notice_base.elapsed()) {
            ctx.request_repaint();
        }
        if let Some(remaining) = self.notice.remaining(self.notice_base.elapsed()) {
            ctx.request_repaint_after(remaining);
        }

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

        // アイコン drain（token 無し・path キーで適用）。到着したら load_texture して map へ。
        // load_texture は egui context 必須ゆえ、ここ（メインスレッドの update()）でのみ呼ぶ
        // ——worker（spawn_icon_load）は ColorImage を送るだけで load_texture は呼ばない。
        let mut icon_arrived = false;
        while let Ok(msg) = self.icon_rx.try_recv() {
            match msg {
                crate::egui_shell::IconMsg::Loaded(path, img) => {
                    self.icon_pending.remove(&path); // in-flight 解除（thread pileup 対策）
                    let handle = ctx.load_texture(&path, img, egui::TextureOptions::LINEAR);
                    self.icon_textures.insert(path, handle);
                    icon_arrived = true;
                }
                crate::egui_shell::IconMsg::Missing(path) => {
                    self.icon_pending.remove(&path); // in-flight 解除（thread pileup 対策）
                    self.icon_missing.insert(path);
                }
            }
        }
        if icon_arrived {
            ctx.request_repaint();
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
        let in_tool = self.state.view_kind() == ViewKind::Tool;
        let in_folder = self.state.view_kind() == ViewKind::Folder;
        let l = self.lang();
        let hint: &str = if in_tool {
            // SolidJS placeholder.tool_select parity（egui の hint は buf が空のときだけ描かれる＝
            // HTML placeholder と同条件。表示されるのは対象パスが区切り終端等でファイル名が空のとき）
            crate::egui_shell::ui_strings::tool_select_hint(l)
        } else if !in_folder && self.indexing() && self.state.query().trim().is_empty() {
            crate::egui_shell::ui_strings::indexing_hint(l)
        } else {
            crate::egui_shell::ui_strings::search_hint(l)
        };
        let mut buf = if in_tool {
            // §18.5: 対象の**ファイル名部分のみ**を表示——SolidJS inputValue は targetPath を
            // 区切りで split した末尾を返す（SearchWindow.tsx:255-267）。フルパスではない
            //（plan-review scout-parity 指摘で是正）。
            self.state
                .tool_frame()
                .map(|f| {
                    f.target_path
                        .rsplit(['\\', '/'])
                        .next()
                        .unwrap_or("")
                        .to_string()
                })
                .unwrap_or_default()
        } else if in_folder {
            self.state.folder_filter().to_string()
        } else {
            self.state.query().to_string()
        };
        let response = ui.add(
            egui::TextEdit::singleline(&mut buf)
                // §18.5 ツール選択中の入力は無効化。add_enabled（全体グレーアウト）でなく
                // interactive(false)（通常描画のまま読み取り専用・changed 不発火）——外観維持。
                // launching 中も同様に打鍵を止める（Escape/blur/Alt+Q・↑↓は従来どおり通す・
                // spec 決定 3・4。↑↓は空リストゆえ自然 no-op）。
                .interactive(!in_tool && self.launching.is_none())
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
        if focused && !in_tool && !response.has_focus() {
            response.request_focus();
        }

        // 一時 overlay（#532 SU5）: 「起動中…」/ 失敗・結果不明通知を検索バーに重ね描く。
        // hint_text は空クエリ時のみ描かれるため使えない（launching/notice 中は query 非空・
        // 状態機械レビュー）——painted label で TextEdit の rect を塗り潰して上書きする。
        // 優先順は WebView2 SearchWindow.tsx の Switch 先頭一致 parity: indexing > 起動中 > 通知。
        // indexing はここでは描かない（egui では空クエリ hint が担う・SU3 as-built）。indexing 中に
        // launching/notice が重なる窓（instant は indexing 中も実行可）は indexing 表示を優先し
        // overlay を抑止する（Switch 順 parity・parity レビュー要修正 3）。
        let overlay_text: Option<String> = if self.indexing() && self.state.view_kind() == ViewKind::Results {
            None // indexing が最優先（hint が見える・overlay は描かない）
        } else if self.launching.is_some() {
            Some(crate::egui_shell::ui_strings::launching(self.lang()).to_string())
        } else {
            self.notice.message().map(|m| m.to_string())
        };
        if let Some(text) = overlay_text {
            let rect = response.rect;
            let (input_bg, hint_color) = self
                .app_handle
                .try_state::<crate::AppState>()
                .map(|s| {
                    let engine = s.engine.lock().unwrap();
                    let v = &engine.config().visual;
                    (v.input_background_color.clone(), v.hint_text_color.clone())
                })
                .unwrap_or_else(|| ("#383838".into(), "#808080".into()));
            ui.painter().rect_filled(
                rect,
                4.0,
                hex_color(&input_bg, egui::Color32::from_rgb(0x38, 0x38, 0x38)),
            );
            ui.painter().text(
                egui::pos2(rect.left() + 8.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                &text,
                egui::FontId::proportional(15.0),
                hex_color(&hint_color, egui::Color32::from_rgb(0x80, 0x80, 0x80)),
            );
        }

        // updater toast（§20.3・#532 SU5）: 検索バー直下の 52px 行・モード非依存
        //（folder/tool/instant 中も表示・状態機械レビュー項 1）。
        let toast_row = self
            .app_handle
            .try_state::<crate::egui_shell::UpdaterUiState>()
            .and_then(|st| st.0.lock().unwrap().toast());
        let has_toast = toast_row.is_some();
        let mut toast_action: Option<ToastAction> = None;
        if let Some(row) = toast_row {
            let l = self.lang();
            let theme = self.row_theme();
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), 52.0),
                egui::Sense::hover(),
            );
            let line1 = match &row.kind {
                crate::egui_shell::ToastKind::Available { version } => {
                    crate::egui_shell::ui_strings::update_available(l, version)
                }
                crate::egui_shell::ToastKind::Installing => {
                    crate::egui_shell::ui_strings::update_installing(l).to_string()
                }
                crate::egui_shell::ToastKind::Failed { .. } => {
                    crate::egui_shell::ui_strings::update_failed(l).to_string()
                }
            };
            ui.painter().text(
                egui::pos2(rect.left() + 8.0, rect.top() + 13.0),
                egui::Align2::LEFT_CENTER,
                &line1,
                egui::FontId::proportional(13.0),
                theme.name_color,
            );
            // 行2: ボタン（右寄せ・installing 中は disabled・WebView2 UpdateToast parity）。
            let mut cursor_x = rect.right() - 8.0;
            let btn_y = rect.top() + 39.0;
            let dismiss_label = crate::egui_shell::ui_strings::update_dismiss(l);
            if draw_toast_button(ui, &mut cursor_x, btn_y, dismiss_label, row.buttons_enabled, &theme) {
                toast_action = Some(ToastAction::Dismiss);
            }
            if row.show_install {
                let install_label = crate::egui_shell::ui_strings::update_install_now(l);
                if draw_toast_button(ui, &mut cursor_x, btn_y, install_label, row.buttons_enabled, &theme) {
                    toast_action = Some(ToastAction::Install);
                }
            }
        }
        if let Some(action) = toast_action {
            self.handle_toast_action(action);
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

        // Enter: 選択項目を起動/実行（Shift は §18.3 のツール選択入場・後置 dispatch は M3 のまま）。
        // TextEdit の changed 処理より後で判定する——同一フレームに入力確定（貼り付け・IME 確定）と
        // Enter が入ったとき、旧 state の interp/選択で起動しないため（codex 発見 4・spec M3 実装確定）。
        // egui の input はフレーム内で不変（読む順序は消費に影響しない）ため後置しても Enter は取りこぼさない。
        let (enter_pressed, shift_held) =
            ctx.input(|i| (i.key_pressed(egui::Key::Enter), i.modifiers.shift));
        if enter_pressed {
            // #631 flush-on-Enter: trailing 窓内（打鍵後 50ms 以内）の Enter は leading 時点の
            // 結果で起動しうる。armed な plain クエリは cancel → 同期 run_search で最終クエリの
            // 結果に置換してから dispatch（SolidJS resolveActivationTarget の flushPendingRefresh 同型）。
            let prefix = self.instant_prefix();
            let is_plain = matches!(self.state.interp(&prefix), QueryIntent::Plain);
            if crate::egui_shell::should_flush_on_enter(
                self.state.view_kind(),
                is_plain,
                self.search_debounce.is_armed(),
            ) {
                self.search_debounce.cancel();
                self.run_search_with(&prefix);
                // flush 後の selected は set_results 内の clamp_selected（min クランプ・0 リセットではない）
                // に委ねる——SolidJS parity（resolveActivationTarget → clampSelectedIndex(selected, len)）。
                // trailing 窓内に ↓↑ で動かした非 0 選択は新結果リストへ clamp されたまま引き継がれる
                //（WebView2 と同挙動。flush 前のリストで確認した行と別物になりうるのは現行製品と同じ受容済み特性）。
            }
            if !self.state.results().is_empty() {
                if shift_held {
                    self.shift_activate(self.state.selected(), &ctx);
                } else {
                    self.activate_or_execute(self.state.selected(), &ctx);
                }
            }
        }

        // アイコン: 可視集合（現結果）に頭打ちして drop（メモリ境界・SU4 決定 A）。連打中
        // （debounce armed）は積まない——結果が確定してから worker へ回す（呼び出し側ガード）。
        let visible: std::collections::HashSet<String> =
            self.state.results().iter().map(|r| r.path.clone()).collect();
        crate::egui_shell::retain_visible(&mut self.icon_textures, &visible);
        self.icon_missing.retain(|p| visible.contains(p));
        if !self.search_debounce.is_armed() {
            self.request_icons_for_results(&ctx);
        }

        // 結果リスト（shouldShowResults 相当。results 軸〔plain〕と folder 軸を描く。空なら描かない）。
        let show_results = !self.state.results().is_empty();
        let mut clicked: Option<usize> = None;
        if show_results {
            // 借用衝突回避: results を clone してから描画（draw_result_row は関連関数で self 非借用）。
            let results = self.state.results().to_vec();
            let selected = self.state.selected();
            let theme = self.row_theme();
            let show_icons = self.show_icons(); // ループ前に 1 回読む（#532 SU4 Task 6）
            // 選択変化時のみ scroll_to_me（毎フレーム発火だと手動スクロールを奪い返す・#632）。
            let do_scroll = self.last_scrolled_selected != Some(selected);
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (i, result) in results.iter().enumerate() {
                    let sel = i == selected;
                    let icon = self.icon_textures.get(&result.path);
                    if Self::draw_result_row(
                        ui,
                        result,
                        sel,
                        sel && do_scroll,
                        icon,
                        show_icons,
                        &theme,
                    ) {
                        clicked = Some(i); // シングルクリック（§4.8 単=起動）。double は扱わない
                    }
                }
            });
            if do_scroll {
                self.last_scrolled_selected = Some(selected);
            }
        }
        // シングルクリック＝起動（§4.8 単=起動）。double-click は扱わない（ユーザー決定・
        // as-built でも double-click=選択は到達不能。SPEC §4.8 を as-built へ同期済み）。
        if let Some(i) = clicked {
            self.activate_or_execute(i, &ctx);
        }

        // 動的ウィンドウ高さ（§4.5/§4.7）。show_results 可否 × max_results から算出し set_size。
        // view 直呼び（SU1 runtime 不変・ユーザー決定）。update はイベントループスレッドで走る
        // ので set_size は安全な見込み（G-RESIZE で確認。本タスクではスモークまで到達しない）。
        let height = compute_window_height(&HeightParams {
            show_results,
            max_results: self.max_results(),
            has_update_toast: has_toast,
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
    use super::font_definitions;

    #[test]
    fn hex_color_parses_and_falls_back() {
        use super::hex_color;
        assert_eq!(hex_color("#E0E0E0", egui::Color32::BLACK),
            egui::Color32::from_rgb(0xE0, 0xE0, 0xE0));
        // 不正文字列は fallback（release panic=abort ゆえ unwrap しない）。
        assert_eq!(hex_color("not-a-color", egui::Color32::RED), egui::Color32::RED);
    }

    #[test]
    fn font_definitions_fallback_is_jp_single_stack() {
        // user=None（font_family 解決失敗）: jp_font 単一・両ファミリ index 0（#579 の元不変条件）。
        let dummy: &'static [u8] = &[0u8; 4];
        let fonts = font_definitions(dummy, None);
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            let list = fonts.families.get(&family).expect("family present");
            assert_eq!(list.first().map(String::as_str), Some("jp_font"),
                "解決失敗時は jp_font 単一・先頭（#579 再発防止）");
        }
    }

    #[test]
    fn truncate_middle_shortens_long_path() {
        use super::truncate_middle;
        // 第3引数は per_char_px（実測 galley 幅から呼び出し側が導出する平均文字幅）。
        // size*0.55 概算値だった旧シグネチャの名残で 11.0 を使うが、意味は「1 文字の
        // 実測幅」に変わった（#632 reviewer Important 2）。
        let long = r"C:\Users\Eoh\AppData\Local\Programs\app\bin\tool.exe";
        let out = truncate_middle(long, 100.0, 11.0);
        assert!(out.chars().count() < long.chars().count(), "省略される");
        assert!(out.contains('…'), "中間省略記号を含む");
        // 短い文字列・極小幅は原文（max_chars<4 ガード）。
        assert_eq!(truncate_middle("a.exe", 1.0, 11.0), "a.exe");
        assert_eq!(truncate_middle("short", 1000.0, 11.0), "short");
        // 空文字列は範囲外アクセスなく原文（空文字）を返す（reviewer Minor）。
        assert_eq!(truncate_middle("", 50.0, 11.0), "");
    }

    #[test]
    fn font_definitions_honor_puts_user_first_jp_fallback() {
        // user=Some（honor）: user_font 先頭・jp_font は fallback（index 1）＝WebView2 CSS スタック parity。
        let dummy: &'static [u8] = &[0u8; 4];
        let user = vec![0u8; 4];
        let fonts = font_definitions(dummy, Some((user, 0)));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            let list = fonts.families.get(&family).expect("family present");
            assert_eq!(list.first().map(String::as_str), Some("user_font"),
                "honor 時は user_font 先頭（font_family 優先）");
            assert_eq!(list.get(1).map(String::as_str), Some("jp_font"),
                "honor 時も jp_font は fallback として残す（CJK 被覆）");
        }
    }
}
