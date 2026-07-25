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

use crate::egui_shell::results_view::{self, RowTheme};
use crate::egui_shell::{
    Debouncer, EscapeOutcome, QueryIntent, SearchState, SlashCmd, ViewKind, compute_parent_dir,
    find_slash_command, folder_load_pending,
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

pub(crate) fn configure_japanese_font(context: &egui::Context, font_family: &str) {
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
    /// SU6 spec 決定 2: 適用済み font_family。config 値と毎フレーム比較し差分で再ロード。
    /// **解決の成否に依らず config 値へ無条件更新する**——未解決名（typo・未インストール）で
    /// 毎フレーム load_system_fonts（数十 ms）が走る perf cliff を避ける（並行性レビュー）。
    applied_font_family: String,
    /// SU6 spec 決定 2: 適用済み native 背景ブラシ（hex 文字列）。painted panel は live-read だが
    /// リサイズ時に露出する native surface の色は生成時ブラシ由来のため実行時追従が要る（codex 反証）。
    applied_background_hex: String,
    /// SU6 spec 決定 2: 直近 set_size の幅。main（本 view）が両窓（main・results）の唯一の
    /// size writer に一意化されている（幅は config live-read・#646 PR2 決定 6）。
    last_set_width: f64,
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
    /// 結果集合が総入れ替えされるたびに加算する世代番号（#632 reviewer Important 3 の後継・
    /// Fix 3）。`run_search_with`/`clear_search`/`start_launch` の「旧 scroll gate reset」が
    /// 居た地点で加算し、`RowsSnapshot.generation` に載せて ResultsView 側の scroll gate
    /// リセットを駆動する（selected の値だけでは総入れ替えを検出できないため）。
    snapshot_generation: u64,
    /// in-flight 起動（single-flight の実体: Some の間は新規起動 dispatch を拒否）。
    launching: Option<LaunchInFlight>,
    /// #633: index build 完了世代の last-seen（AppState.index_generation と比較・SU6 spec 決定 3）。
    /// 差分で現クエリを再検索（SolidJS `indexing-complete`→runRefresh parity）。bool エッジ検出で
    /// ないのは started/complete の repaint が 1 フレームに合流するとパルスが見えないため。
    last_seen_index_generation: u64,
    /// 一時通知（起動失敗/結果不明）。時刻は notice_base からの経過で注入（純粋核）。
    notice: crate::egui_shell::NoticeSlot,
    /// notice の単調時刻基準（view 生成時に固定・Instant 差分を Duration で渡す）。
    notice_base: Instant,
    /// results 窓の直近設定高さ（デルタガード）。**可視フラグは `ResultsWindow` が持つ**——
    /// こちらは冗長な `set_size` を避ける性能上のガードであり概念が別（#671 spec 決定 2）。
    last_results_height: f64,
    /// results 窓の直近設定幅（デルタガード）。**`last_set_width`（main 用）を流用しない**——
    /// 同一フレーム内で main のブロックが先に `last_set_width` を更新済みのため、それと
    /// 比較すると常に差分 0 になり results が幅の live-reload に追従しなくなる（Important 1）。
    last_results_width: f64,
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
            applied_font_family: String::new(),
            applied_background_hex: String::new(),
            last_set_width: 0.0,
            folder_tx,
            folder_rx,
            folder_cache: None,
            folder_error: None,
            instant_rows_query: None,
            snapshot_generation: 0,
            launching: None,
            last_seen_index_generation: 0,
            notice: crate::egui_shell::NoticeSlot::default(),
            notice_base: Instant::now(),
            last_results_height: 0.0,
            last_results_width: 0.0,
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
    /// launching 中は results 窓が hide される（結果 0 件→snapshot.show=false・#646 PR2 決定 6）・
    /// ↑↓/クリックは空リストゆえ自然に inert。クエリは保持。
    fn start_launch(&mut self, work: LaunchWork, tag: LaunchTag, ctx: &egui::Context) {
        if self.launching.is_some() {
            return; // single-flight 拒否（拒否された Enter が後で再生されるキューは egui に無い）
        }
        let (tx, rx) = channel::<crate::commands::launch::LaunchResult>();
        self.launching = Some(LaunchInFlight { started: Instant::now(), rx, tag });
        self.state.set_results(Vec::new());
        self.instant_rows_query = None; // 行が消えるため来歴も一体でクリア（finding 0 の規律）
        self.snapshot_generation += 1; // 結果総入れ替え（#632 reviewer Important 3 の後継・Fix 3）
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
        // 状態を変えたら起こす（#648 A）。旧実装は「results クリア → 高さ collapse →
        // set_size → Resized → repaint」の暗黙連鎖で初回 overlay と timeout 予約を成立させて
        // いたが、#646 PR2 決定 6 で main 高さが bar(+toast)固定になり results クリアで main は
        // 伸縮しなくなった（暗黙連鎖は消滅・results 側の repaint は snapshot 差分 wake が明示的に
        // 担う）。ここは toast dismiss の同型バグ（SU5・e746826）と同じ規範で、この行単体を
        // 自己完結させた明示 repaint として残す（暗黙連鎖に頼らない）。
        ctx.request_repaint();
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
            LaunchStatus::Failed => {
                // 失敗: hide しない・同期 run_search で結果を再取得（runRefresh parity）+ 一時通知。
                // 旧 Timeout ステータスは IPC の run_launch_blocking 専用で #532 SU7 PR3 で消滅
                // （drain 側の 4 秒 timeout は Empty 経路で扱う）。
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
        self.snapshot_generation += 1; // 結果総入れ替え（#632 reviewer Important 3 の後継・Fix 3）
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
                // exit-requested listener（main.rs）が history/icon flush → exit する
                // 唯一の終了合流点。トレイの終了メニューと同じ経路。
                let _ = app.emit("exit-requested", ());
            }
        }
    }

    /// 選択中の instant コマンドの action を抽出し worker へ投げる（§19.6・#631 async 化）。
    /// action 抽出はここ（UI スレッド・engine ロック内）で行い、clipboard 読み + 実行は
    /// `start_launch` の worker スレッド側（action 抽出をロック内・clipboard 読みをロック外
    /// ——`execute_instant_action_core` の契約）。instant は履歴を記録しない（§19.6）。
    /// 成功/失敗の後処理は `finish_launch` へ合流。
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
    /// パス文字列照合は禁止）。成功時は launch_query で履歴記録 → 全クリア + hide
    /// （§19.6 instant の完了列と同型。reset は tool/folder/gen 込みで in-flight folder
    /// ロードも失効させる）。
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

    /// フォルダ展開を履歴に記録する（`record_and_save` と同一パターン:
    /// lock → record → prepare_history_save_if_dirty → drop → save）。
    /// 呼び出しサイトは → の展開時のみ（← の親フォルダへの折り返しでは呼ばない・§4.6）。
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

    /// ウィンドウ論理幅は config live-read（SU6 spec 決定 2）。**main（本 view）が両窓（main・
    /// results）の唯一の size writer に一意化されている**（#646 PR2 決定 6: results への幅適用は
    /// `drive_results_window` 経由で main が担い、results 自身は書かない）。
    /// 旧実装の inner_size() 読みは「幅を維持」だったが、config_watcher（notify スレッド）の幅
    /// set_size と 2 次元 read-modify-write で潰し合う race の片翼だった——config を正本にすれば
    /// cross-thread writer 自体が消える（初版 spec の watcher flag 分岐案は却下・並行性レビュー）。
    /// なお flag ON では config_watcher の幅 set_size は get_webview_window=None で元々 no-op。
    fn window_width(&self) -> f64 {
        self.app_handle
            .try_state::<crate::AppState>()
            .map(|s| f64::from(s.engine.lock().unwrap().config().appearance.window_width))
            .unwrap_or(600.0)
    }

    /// results 窓の可視性・サイズ・位置を main から駆動する(#646 PR2 決定 6)。
    /// 位置 = main の直下 + window_gap(従属)。デルタガードで無変化フレームは no-op。
    /// show は focusable(false) 窓ゆえフォーカスを奪わない(決定 4)。
    fn drive_results_window(
        &mut self,
        show_results: bool,
        width: f64,
        metrics: &crate::egui_shell::layout::Metrics,
    ) {
        let Some(results) = self
            .app_handle
            .try_state::<crate::egui_shell::ResultsWindow>()
        else {
            return;
        };
        let count = self.state.results().len();
        let res_h = crate::egui_shell::layout::results_window_height(
            count,
            self.max_results(),
            metrics.row_height,
        );
        let visible = show_results && res_h > 0.0;
        if !visible {
            // 可視フラグは ResultsWindow が持つ（#671 PR A′ spec 決定 2）。hide() は遷移した
            // ときだけ true を返すため、trace は 1 回だけ出る（毎フレーム撃たない）。
            // trace を型の内側でなく呼び出し側に置く理由は spec 決定 7。
            if results.hide() {
                crate::trace_main("egui_results:hide", serde_json::json!({ "from": "drive" }));
            }
            return;
        }
        // 位置: main の外形直下 + gap(物理座標。gap は論理 px を scale で換算)。無ガードの
        // 単一点(position_results_below_main・mod.rs)へ委譲——Moved リスナーと共用する
        // ため、デルタガードはヘルパー側に持たない(#646 PR2 決定 10)。
        crate::egui_shell::position_results_below_main(&self.app_handle);
        if (res_h - self.last_results_height).abs() > 0.5
            || (width - self.last_results_width).abs() > 0.5
        {
            results.set_size(width, res_h);
            self.last_results_height = res_h;
            self.last_results_width = width;
        }
        // フォーカスを奪わない表示（tauri show() は SW_SHOW で活性化する・#646 PR2）。
        // 置き場の理由は上の hide 側コメントと同じ（spec 決定 7）。
        if results.show() {
            crate::trace_main("egui_results:show", serde_json::json!({ "rows": count }));
        }
        crate::egui_shell::wake_results(&self.app_handle);
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
        // 結果が総入れ替えされうる箇所（#632 reviewer Important 3 の後継・Fix 3）。selected の
        // 値だけでは「打鍵で結果が丸ごと変わったが selected は偶然 0 のまま」を検出できないため、
        // 世代番号を進めて RowsSnapshot 経由で ResultsView 側の scroll gate をリセットさせる。
        self.snapshot_generation += 1;
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

    /// toast ボタンの処理（#532 SU5）。install は Update を原子取得して async へ（Task 8）。
    ///
    /// **状態を変えたら `ctx.request_repaint()` する**（Task 10 実機スモークで発見・
    /// `spawn_folder_load` の egui_ctx wake（本ファイル該当箇所のコメント参照）と同じ理由）:
    /// このランタイムはイベント駆動で、click を処理したこのフレームの描画は toast_action の
    /// 遅延 dispatch より前に完了している。ここで状態を変えても誰も次のフレームを起こさないため、
    /// 無関係な入力（マウス移動等）が来るまで旧 toast が画面に残る（dismiss 後の stale 表示）。
    fn handle_toast_action(&mut self, action: ToastAction, ctx: &egui::Context) {
        let Some(st) = self.app_handle.try_state::<crate::egui_shell::UpdaterUiState>() else {
            return;
        };
        match action {
            ToastAction::Dismiss => {
                if st.0.lock().unwrap().dismiss() {
                    ctx.request_repaint(); // Installing 中の拒否（false）は表示不変ゆえ不要
                }
            }
            ToastAction::Install => {
                let taken = st.0.lock().unwrap().try_begin_install();
                if let Some(update) = taken {
                    ctx.request_repaint(); // Available→Installing の即時反映（disabled ボタン）
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
                        st.0.lock().unwrap().phase = crate::egui_shell::UpdaterPhase::InstallFailed;
                    }
                    crate::egui_shell::wake_view(&handle); // 可視中の失敗を即座に描く
                }
            }
        });
    }
}

/// `#RRGGBB` 文字列を Color32 へ。失敗時は fallback（release は panic=abort ゆえ unwrap しない）。
fn hex_color(s: &str, fallback: egui::Color32) -> egui::Color32 {
    egui::Color32::from_hex(s).unwrap_or(fallback)
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
        self.applied_font_family = font_family;
        // updater check 完了時の wake-up 用（mod.rs spawn_update_check が読む・#532 SU5）。
        if let Some(sh) = self.app_handle.try_state::<crate::egui_shell::EguiShellState>() {
            crate::egui_shell::register_ctx(&sh.egui_ctx, context);
        }
    }

    fn update(&mut self, ui: &mut egui::Ui, frame: &mut RuntimeFrame) {
        // #646 PR2 決定 10: 入力欄以外の全域を掴んでドラッグ移動。背景 interact を先に
        // 登録し、後続ウィジェット(TextEdit・toast ボタン)はヒットテストで勝つ(egui は
        // 後着が上位)。start_dragging は runtime の frame コマンド経由(配管済み)。
        let drag_resp = ui.interact(
            ui.max_rect(),
            egui::Id::new("main-window-drag"),
            egui::Sense::drag(),
        );
        if drag_resp.drag_started_by(egui::PointerButton::Primary) {
            frame.drag_window();
        }

        // Metrics は 1 フレーム 1 回だけ導出し、バー帯・toast 高・行高・窓高で使い回す
        //(/simplify: フレーム内の重複 lock を 1 回へ。live-read 契約はフレーム間の話で不変・
        // 導出は mod.rs read_metrics に一元化)。
        let metrics = crate::egui_shell::read_metrics(&self.app_handle);

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
            // scroll gate（#632: 再表示後に確実に一度 scroll し直す）は results 窓の
            // ResultsView::update() 側（実ゲート）に移設済み——main はもう読み書きしない。
            // icon パイプライン（icon_textures/icon_missing/icon_pending）も Task 5 で
            // results 窓へ移設済み——main はもう保持しない。hide 中の常駐テクスチャは
            // results 側の retain_visible が空 rows で自然に全クリアする（Task 5 申し送り）。
            // SU5: in-flight 起動と一時通知は show を跨がない（resetForShow の
            // setLaunching(false) + clearLaunchNotice parity）。rx ごと drop するため
            // hide 中に完了した遅着結果もここで自然消滅する（stale Ok が再 show 窓を
            // hide で撃つ事故の backstop・並行性レビュー High）。updater toast は触らない。
            self.launching = None;
            self.notice.clear();
            // results 窓の drive **サイズ**デルタガードを初期値へ戻す（#646 PR2 決定 6）。
            // これは冗長な set_size を避ける性能上のガードであり、可視性のような
            // correctness のフラグではない（#671 spec 決定 2 の意図的な分割）。0 へ戻すことで
            // 再 show 後に必ず 1 度は現行 metrics で set_size させる。
            // 可視フラグはここに無い——`ResultsWindow` が所有し、hide_egui_main と
            // drive_results_window の 2 経路が同じ型を通るため後始末が要らない（PR A′）。
            self.last_results_height = 0.0;
            self.last_results_width = 0.0;
        }

        let ctx = ui.ctx().clone();

        // #633: index build 完了の世代検知 → 現クエリで再検索（runRefresh parity・SU6 spec 決定 3）。
        // reset_pending 消費の後に置く（show 直後は reset 済み空クエリの no-op になるだけ）。
        // folder 中は fs 由来 cache の再フィルタ、tool 中は no-op——run_search が view_kind で分岐済み。
        // 順序不変条件: このブロックが後段の indexing() 読み（run_search 内・show_results ゲート）
        // より前にあることは、完了フレームをフリッカーなしで新結果にするために効いている
        // （世代 SeqCst acquire が後続 Relaxed 読みへ happens-before を運ぶ）。後ろへ動かしても
        // 正しさは壊れないが 1 フレームのフリッカーが出る。
        if let Some(s) = self.app_handle.try_state::<crate::AppState>() {
            let generation = s.index_generation.load(Ordering::SeqCst);
            if crate::egui_shell::needs_index_refresh(self.last_seen_index_generation, generation) {
                self.last_seen_index_generation = generation;
                self.run_search();
            }
        }

        // hotkey 登録失敗の pending 消費（SU6 spec 追補 2 + #652）。reset_pending 消費より後
        //（順序不変条件——reset の notice.clear() がこの set を消さないため）。整形はここで
        // lang() live-read: config-applied wake のフレームは update_config 後なので言語同時
        // 変更でも新言語で整形される。hidden 中の失敗は次 show のこの消費で表示される
        //（WebView2 は hidden 中に期限切れ・改善方向の受容差異・SU6 spec 追補 2）。
        if let Some(sh) = self.app_handle.try_state::<crate::egui_shell::EguiShellState>()
            && let Some((kind, hk)) = sh.pending_hotkey_failure.lock().unwrap().take()
        {
            let msg = match kind {
                crate::egui_shell::HotkeyFailureKind::Initial => {
                    crate::egui_shell::ui_strings::hotkey_initial_failed(self.lang(), &hk)
                }
                crate::egui_shell::HotkeyFailureKind::Change => {
                    crate::egui_shell::ui_strings::hotkey_change_failed(self.lang(), &hk)
                }
            };
            self.notice.set(msg, self.notice_base.elapsed(), crate::egui_shell::NOTICE_HOTKEY);
            ctx.request_repaint();
        }

        // §11: パネル/入力欄/選択色を config テーマから（ハードコード撤廃・runtime CLEAR_COLOR は不変）。
        // font_family / native 背景ブラシのエッジ検出も同一 lock で読む（SU6 spec 決定 2・lock 1 回/フレーム）。
        if let Some(s) = self.app_handle.try_state::<crate::AppState>() {
            let (bg, input_bg, sel, font_family) = {
                let engine = s.engine.lock().unwrap();
                let v = &engine.config().visual;
                (
                    v.background_color.clone(),
                    v.input_background_color.clone(),
                    v.selected_row_color.clone(),
                    v.font_family.clone(),
                )
            };
            let mut visuals = ctx.style_of(ctx.theme()).visuals.clone();
            visuals.panel_fill = hex_color(&bg, egui::Color32::from_rgb(0x28, 0x28, 0x28));
            visuals.window_fill = visuals.panel_fill;
            visuals.extreme_bg_color = hex_color(&input_bg, egui::Color32::from_rgb(0x38, 0x38, 0x38)); // TextEdit 背景
            visuals.selection.bg_fill = hex_color(&sel, egui::Color32::from_rgb(0x33, 0x33, 0x33));
            ctx.set_visuals(visuals);

            // SU6 spec 決定 2: font_family hot-reload（WebView2 の --font-family CSS 変数即時反映 parity）。
            // applied は解決成否に依らず無条件更新（フィールド doc 参照）。
            if font_family != self.applied_font_family {
                self.applied_font_family = font_family.clone();
                configure_japanese_font(&ctx, &font_family);
                ctx.request_repaint(); // set_fonts は次フレーム適用——欠くと新フォントが 1 イベント遅れる
            }

            // SU6 spec 決定 2: native 背景ブラシ追従（生成時一度きり → 実行時変更へ・codex 反証）。
            if bg != self.applied_background_hex {
                self.applied_background_hex = bg.clone();
                if let Some(window) = self.app_handle.get_window("main") {
                    let color = crate::config_watcher::parse_hex_color(&bg)
                        .unwrap_or(tauri::window::Color(0x28, 0x28, 0x28, 0xff));
                    let _ = window.set_background_color(Some(color));
                }
            }
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
        // ときだけ描かれるため、indexing+空クエリの条件と一致する——window は bar_height のまま
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
        // §11 Part C（#643）: 入力欄フォントを config `font_size` へ追従させ、hint を
        // `hint_text_color` で描く。#646 決定 2: バー高は `font_size + bar_padding`（Metrics）。
        // SU6.5 決定 3 の 52px 据え置きは WebView2 parity 制約下の判断で、SU7 の WebView2 撤去
        // により失効した。極端な font_size でバーからはみ出す挙動は変わらず残る。
        let bar_theme = results_view::row_theme(&self.app_handle);
        let bar_font = egui::FontId::proportional(bar_theme.name_size);
        // 入力欄はバー帯の内側に四辺一様の余白（`Metrics::bar_inset`）を残して置く
        //（#646 PR2・実機目視で追加）。egui の既定配置では上と左が詰まり余りが下だけに
        // 溜まっていた。Frame の inner_margin で四辺の枠を作り、中身の高さを
        // `bar_height - 2*inset` に固定することで帯をちょうど埋める（窓高は bar_height
        // ゆえ、下に取り残しも溢れも出ない）。余白部はドラッグ掴み領域になる（決定 10）。
        let inset = metrics.bar_inset as f32;
        let field_height = (metrics.bar_height as f32 - 2.0 * inset).max(1.0);
        let response = egui::Frame::new()
            .inner_margin(egui::Margin::same(inset.round() as i8))
            .show(ui, |ui| {
                ui.add_sized(
                    egui::vec2(ui.available_width(), field_height),
                    egui::TextEdit::singleline(&mut buf)
                        // §18.5 ツール選択中の入力は無効化。add_enabled（全体グレーアウト）でなく
                        // interactive(false)（通常描画のまま読み取り専用・changed 不発火）——外観維持。
                        // launching 中も同様に打鍵を止める（Escape/blur/Alt+Q・↑↓は従来どおり通す・
                        // spec 決定 3・4。↑↓は空リストゆえ自然 no-op）。
                        .interactive(!in_tool && self.launching.is_none())
                        .font(bar_font.clone())
                        .hint_text(
                            egui::RichText::new(hint).font(bar_font).color(bar_theme.path_color),
                        ),
                )
            })
            .inner;
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

        // 一時 overlay（#532 SU5）: 「起動中…」/ 失敗・結果不明通知/非空クエリ indexing 案内を
        // 検索バーに重ね描く。hint_text は空クエリ時のみ描かれるため launching/notice/非空クエリ
        // indexing 中（query 非空）は使えない——painted label で TextEdit の rect を塗り潰して上書きする。
        // 優先順は WebView2 SearchWindow.tsx の Switch 先頭一致 parity: indexing > 起動中 > 通知。
        // 空クエリの indexing は hint が描く。非空クエリの indexing は表示ゲート（§4.7）で結果が
        // 消えるため overlay が唯一の案内（spec 追補 1・ladder は overlay_kind に抽出しテスト固定）。
        let overlay_text: Option<String> = match crate::egui_shell::overlay_kind(
            self.indexing() && self.state.view_kind() == ViewKind::Results,
            self.state.query().trim().is_empty(),
            self.launching.is_some(),
            self.notice.message().is_some(),
        ) {
            Some(crate::egui_shell::OverlayKind::Indexing) => {
                Some(crate::egui_shell::ui_strings::indexing_hint(self.lang()).to_string())
            }
            Some(crate::egui_shell::OverlayKind::Launching) => {
                Some(crate::egui_shell::ui_strings::launching(self.lang()).to_string())
            }
            Some(crate::egui_shell::OverlayKind::Notice) => self.notice.message().map(|m| m.to_string()),
            None => None,
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

        // updater toast（§20.3・#532 SU5）: 検索バー直下の toast_height（= bar_height・#646 決定 2）行・モード非依存
        //（folder/tool/instant 中も表示・状態機械レビュー項 1）。
        let toast_row = self
            .app_handle
            .try_state::<crate::egui_shell::UpdaterUiState>()
            .and_then(|st| st.0.lock().unwrap().toast());
        let has_toast = toast_row.is_some();
        let mut toast_action: Option<ToastAction> = None;
        if let Some(row) = toast_row {
            let l = self.lang();
            let theme = results_view::row_theme(&self.app_handle);
            let toast_h = metrics.toast_height as f32;
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), toast_h),
                egui::Sense::hover(),
            );
            let line1 = match &row.kind {
                crate::egui_shell::ToastKind::Available { version } => {
                    crate::egui_shell::ui_strings::update_available(l, version)
                }
                crate::egui_shell::ToastKind::Installing => {
                    crate::egui_shell::ui_strings::update_installing(l).to_string()
                }
                crate::egui_shell::ToastKind::Failed => {
                    crate::egui_shell::ui_strings::update_failed(l).to_string()
                }
            };
            ui.painter().text(
                egui::pos2(rect.left() + 8.0, rect.top() + toast_h * 0.25),
                egui::Align2::LEFT_CENTER,
                &line1,
                egui::FontId::proportional(13.0),
                theme.name_color,
            );
            // 行2: ボタン（右寄せ・installing 中は disabled・WebView2 UpdateToast parity）。
            let mut cursor_x = rect.right() - 8.0;
            let btn_y = rect.top() + toast_h * 0.75;
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
            self.handle_toast_action(action, &ctx);
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

        // 結果リスト（shouldShowResults 相当）。§4.7: 再インデックス中は plain 結果のみ隠す
        // （instant/folder/tool carve-out・SU6 spec 決定 3）。データと選択は保持——クリアしない
        // （SolidJS parity: setIndexing は結果を触らず派生 memo が非表示を担う）。indexing 中の
        // 案内は空クエリ=hint・非空クエリ=overlay（Task 7・spec 追補 1）が担い、show_results=false
        // では results 窓が hide される（main は bar(+toast)固定高で伸縮しない・#646 PR2 決定 6）。
        let show_results = !self.state.results().is_empty()
            && !crate::egui_shell::plain_results_hidden(
                self.state.view_kind(),
                self.instant_rows_query.is_some(),
                self.indexing(),
            );
        // #646 PR2 決定 5: 結果は snapshot として発行し、描画は results 窓(ResultsView)が担う。
        // 変化があったフレームだけ store + wake(毎フレーム wake だと results が常時回る)。
        // 判定は Vec を作る前に行う（/simplify・効率）——無変化フレームで行数ぶんの String
        // 確保を払わないため、`RowsSnapshot::matches` にスライスのまま突き合わせさせる。
        if let Some(shared) = self.app_handle.try_state::<crate::egui_shell::ResultsShared>() {
            // 表示すべきでないフレームは空スライスを発行する（rows 空 = results 非表示）。
            let rows: &[snotra_core::ui_types::SearchResult] =
                if show_results { self.state.results() } else { &[] };
            let selected = self.state.selected();
            // 旧 view.rs の icon request ゲート `!self.search_debounce.is_armed()`（連打中は
            // icon worker を積まない・perf 最適化）の後継。ResultsView は search_debounce を
            // 持てないため、live 値を snapshot 経由で運ぶ（Task 5 concern 2 の fix・controller 依頼）。
            let settled = !self.search_debounce.is_armed();
            {
                let mut guard = shared.snapshot.lock().unwrap();
                if !guard.matches(rows, selected, self.snapshot_generation, settled) {
                    *guard = crate::egui_shell::RowsSnapshot {
                        rows: rows.to_vec(),
                        selected,
                        generation: self.snapshot_generation,
                        settled,
                    };
                    drop(guard);
                    crate::egui_shell::wake_results(&self.app_handle);
                }
            }
            // クリック逆流の消費(決定 5): 起動ロジックは main の一箇所に保つ。
            let clicked = shared.clicked.lock().unwrap().take();
            if let Some(i) = clicked {
                self.activate_or_execute(i, &ctx);
            }
        }

        // #646 PR2 決定 6: main は bar(+toast)のみ。結果窓の可視性・サイズ・位置も
        // ここ(毎フレーム走る main)が駆動する——hidden 窓は update() が走らず自分では
        // show できない(SU5 要石)。位置 → サイズ → show の順(main の show と同じ制約)。
        let height = crate::egui_shell::layout::main_window_height(
            metrics.bar_height,
            has_toast.then_some(metrics.toast_height),
        );
        let width = self.window_width();
        if (height - self.last_set_height).abs() > 0.5 || (width - self.last_set_width).abs() > 0.5 {
            self.last_set_height = height;
            self.last_set_width = width;
            if let Some(window) = self.app_handle.get_window("main") {
                let _ = window.set_size(tauri::LogicalSize::new(width, height));
            }
            ui.ctx().request_repaint();
        }
        self.drive_results_window(show_results, width, &metrics);

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
