//! 検索セッション層（show を跨ぐ状態・結果・選択・起動・履歴・期限）の所有者（#666 段 3）。
//! クエリ/結果/選択・folder ナビ（async ロード + staleness）・tool 選択・instant/slash・起動
//! dispatch と in-flight 回収・一時通知の期限を単独で持ち、`view.rs` からは遷移メソッドと
//! 読み口（`&self`）としてだけ届く。**依存は一方向である**——ここから `view` を参照しない。
//!
//! ここに**無いもの**（`window_coordinator.rs` の様式に倣い、前提条件付きで明記する）:
//!
//! - **フレームを所有しない。** `update()` の文の実行順序を決めるのは `view.rs` であり、ここに
//!   あるのは呼ばれる側の遷移である。`egui::Context` もフィールドに持たず毎回引数で受け取る
//!   （Context の clone は repaint callback ごと複製し、その `RepaintScheduler` の `Arc` が窓の
//!   `Destroyed` を越えて worker の停止・join を止める・#671 PR D）
//! - **7 番目の外部イベント消費（`take_clicked_for`）はここに無い**（#699）。行クリックの逆流は
//!   `view.rs` が snapshot publish の**後**に消費し、照合を通った index だけが
//!   `activate_or_execute` としてここへ届く
//! - **`SearchState::reset()` の `rows_generation` bump は、`view.rs` 末尾のクリック照合と
//!   結ばれている**（#699）。`consume_reset_pending` がフレーム冒頭で進める世代が、同じフレームの
//!   `take_clicked_for` で stale クリックを棄却する根拠になる——ここだけを読んで「世代は検索の
//!   都合」と読まないこと
//! - **`drain_launch` の `notice.set` を撃つ 3 分岐は自前の `request_repaint` を持たない**
//!   （通知の期限を張る唯一の主体は `poll_async` の `notice.remaining()` ブロックであり、両者が
//!   **同じフレームに呼ばれる**ことが期限の成立条件である）。**`drain_launch` 全体が repaint を
//!   持たないという意味ではない**——Empty 腕の未 timeout 側は
//!   `request_repaint_after(LAUNCH_TIMEOUT - elapsed)` を張るが、これは起動タイムアウトの期限で
//!   あって通知の期限ではない（別の armed 期限・#711）
//! - **folder drain の前後関係**は両側とも他所にある: 前は `reset()` / `enter_folder` 等の
//!   `folder_gen` bump（`accept_folder_result` の stale 棄却はこれを根拠に成立する）、後ろは
//!   #699 の世代照合（drain が行を差し替えるのはクリック消費より前でなければならない）

use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use snotra_core::config::{GeneralConfig, OpenerTool, SearchConfig, find_matching_tools};
use snotra_core::engine::FolderListContext;
use snotra_core::ui_types::SearchResult;
use tauri::{Emitter, Manager};

use crate::egui_shell::{
    Debouncer, EscapeOutcome, FrameIndexing, FrameVisibleRows, QueryIntent, SearchState, SlashCmd,
    ViewKind, compute_parent_dir, find_slash_command, folder_load_pending, plain_results_hidden,
    results_area_collapsed,
};

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
    Normal {
        path: String,
        query: String,
        tools: Vec<OpenerTool>,
    },
    /// ツール選択起動（§18.4）。
    Tool {
        target_path: String,
        launch_query: String,
        exe: String,
        args: String,
    },
    /// instant 実行（§19.6）。clipboard 読み + 展開 + 実行の全体を worker で行う
    /// （engine ロック内の action 抽出だけ UI スレッド・spec C 節）。
    Instant {
        name: String,
        action: snotra_core::config::InstantAction,
        instant_query: String,
    },
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

/// toast ボタン種別（クリック結果を borrow 外で処理するための遅延 dispatch）。
pub(super) enum ToastAction {
    Install,
    Dismiss,
}

pub(super) struct LauncherController {
    app_handle: tauri::AppHandle,
    /// blur 猶予の状態機械（#745）。**hide を跨いだ持ち越しは `consume_reset_pending` の
    /// `reset()` が塞ぐ**——旧 2 フィールド（`was_focused` / `unfocus_at`）はこの型へ畳んだ。
    blur_grace: crate::egui_shell::BlurGrace,
    state: SearchState,
    search_debounce: Debouncer,
    last_input_at: Instant,
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
    /// Plain 検索の要求を worker へ送る送信端（#1004 PR 2）。
    search_tx: Sender<crate::egui_shell::SearchRequest>,
    /// worker からの結果受信端。`drain_search` が毎フレーム吸い出す（#1004 PR 2）。
    search_rx: Receiver<crate::egui_shell::SearchMsg>,
}

impl LauncherController {
    pub(super) fn new(app_handle: tauri::AppHandle) -> Self {
        let (folder_tx, folder_rx) = channel();
        let (search_tx, search_rx) = crate::egui_shell::spawn_search_worker(app_handle.clone());
        Self {
            app_handle,
            blur_grace: crate::egui_shell::BlurGrace::default(),
            state: SearchState::new(),
            search_debounce: Debouncer::new(Duration::from_millis(50), true),
            last_input_at: Instant::now(),
            folder_tx,
            folder_rx,
            folder_cache: None,
            folder_error: None,
            instant_rows_query: None,
            launching: None,
            last_seen_index_generation: 0,
            notice: crate::egui_shell::NoticeSlot::default(),
            notice_base: Instant::now(),
            search_tx,
            search_rx,
        }
    }

    // ---- view へ公開する読み口（すべて `&self`）------------------------------------------
    // **mutator は `&mut self` を要するため、この 1 群を通して view から状態を変える経路は無い**
    // （設計 §3.7）。`app_handle` の clone を view 側にもう 1 本持たないための `app()` も含む。

    /// managed state を引くための `AppHandle`（唯一の所有者はここ・不変条件 13）。
    /// **view は `update()` / `setup()` の冒頭でこれを 1 回 clone してローカルに置く**——
    /// `tauri::State<'_, T>` は借用元に紐付くため、戻り値を保持したまま `&mut` の遷移メソッドを
    /// 呼ぶと E0502 になる（先例は `results_view.rs` の `let app_handle = self.app_handle.clone();`）。
    pub(super) fn app(&self) -> &tauri::AppHandle {
        &self.app_handle
    }

    /// 検索状態の読み口（`view_kind` / `results` / `selected` / `query` / `folder_filter` /
    /// `tool_frame` / `rows_generation`）。共有参照 1 本で読みを全て通す（設計 §3.7）。
    pub(super) fn state(&self) -> &SearchState {
        &self.state
    }

    /// 一時通知の本文（status 行が描く・段 24）。期限管理は `poll_async` が持つ。
    pub(super) fn notice_message(&self) -> Option<&str> {
        self.notice.message()
    }

    /// in-flight 起動があるか（入力欄の非対話化と「起動中…」表示・段 21/24）。
    pub(super) fn is_launching(&self) -> bool {
        self.launching.is_some()
    }

    /// 検索 debounce が armed か（snapshot の [`crate::egui_shell::results_view::RowsSnapshot::input_idle`] の材料・段 30）。連打中は results 側が icon worker を積まないためのゲートで、live 値を snapshot 経由で運ぶ。
    pub(super) fn is_search_armed(&self) -> bool {
        self.search_debounce.is_armed()
    }

    /// 表示中の行が instant 候補なら Some（表示ゲートの連言②・段 29）。来歴 snapshot の
    /// 意味は同名フィールドの doc が正本。**この読み口は `view.rs` の表示ゲート用である**——
    /// 起動側（`activate_or_execute` / `shift_activate`）は同じ述語へ `self.instant_rows_query` を
    /// 直接渡すので、ここを通らない（#1077）。
    pub(super) fn instant_rows_query(&self) -> Option<&str> {
        self.instant_rows_query.as_deref()
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
        let _ = self.app_handle.emit(crate::events::EGUI_HIDE_REQUESTED, ());
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
        let Some(result) = self.state.results().get(index) else {
            return;
        };
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
        self.start_launch(
            LaunchWork::Normal { path, query, tools },
            LaunchTag::Normal,
            ctx,
        );
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
        self.launching = Some(LaunchInFlight {
            started: Instant::now(),
            rx,
            tag,
        });
        // 突入時のクリアが in-flight も失効させる——しないと launching 中に worker の遅着結果が届き、隠れているはずの results 窓が drain_search 経由で生え直す（#1039 で `SearchState` の内側へ入った）。
        self.state.set_results(Vec::new());
        self.instant_rows_query = None; // 行が消えるため来歴も一体でクリア（finding 0 の規律）
        let app = self.app_handle.clone();
        let egui_ctx = ctx.clone();
        std::thread::spawn(move || {
            use crate::commands::launch::{
                LaunchStatus, launch_item_core, launch_with_tool_core, record_and_save,
            };
            let (outcome, record) = match work {
                LaunchWork::Normal { path, query, tools } => {
                    let o = if let Some(first) = tools.first() {
                        launch_with_tool_core(&path, &first.exe, &first.args)
                    } else {
                        launch_item_core(&path)
                    };
                    (o, Some((path, query)))
                }
                LaunchWork::Tool {
                    target_path,
                    launch_query,
                    exe,
                    args,
                } => {
                    let o = launch_with_tool_core(&target_path, &exe, &args);
                    (o, Some((target_path, launch_query)))
                }
                LaunchWork::Instant {
                    name,
                    action,
                    instant_query,
                } => {
                    // clipboard 読み（Win32）はロック外・worker 内（commands/instant.rs と同順）。
                    let clipboard = arboard::Clipboard::new()
                        .and_then(|mut cb| cb.get_text())
                        .unwrap_or_default();
                    let o = crate::commands::instant::execute_instant_action_core(
                        action,
                        &instant_query,
                        &clipboard,
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
        let Some(inflight) = &self.launching else {
            return;
        };
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
    }

    /// slash コマンドを実行する（§15.3 即実行・#532 SU3 M3）。SolidJS handleCommandQueryInput と
    /// 同順: クエリ/結果クリア（clearCommandModeState 相当）→ action。`/r`（History）は結果注入型で
    /// ここへ来ない（changed ハンドラが run_search へ振る）。失敗通知は建てない（trace のみ・#631 一本化）。
    fn execute_slash(&mut self, cmd: SlashCmd) {
        crate::trace_main(
            "egui_slash",
            serde_json::json!({ "cmd": format!("{cmd:?}") }),
        );
        self.clear_search();
        let app = self.app_handle.clone();
        match cmd {
            // 到達しない: 呼び出し側（changed ハンドラ）が History を run_search へ振る。
            // 将来 execute_slash の呼び出しサイトが増えて誤配線したとき dev/test で loud に
            // 落とす（release は panic=abort ゆえ unreachable! は採らない）。
            SlashCmd::History => debug_assert!(
                false,
                "History は execute_slash へ来ない（run_search が注入する）"
            ),
            SlashCmd::OpenSettings => {
                // indexing 中の Err（ERR_INDEXING_IN_PROGRESS）は trace のみ（spec M3 実装確定・
                // クエリクリア後は検索バーの indexing hint が可視＝degraded な理由提示）。
                if let Err(e) = crate::commands::open_settings(app.state(), app.clone()) {
                    crate::trace_main(
                        "egui_slash_error",
                        serde_json::json!({ "cmd": "/o", "error": e }),
                    );
                }
            }
            SlashCmd::RebuildIndex => {
                // SolidJS /s parity: hide してから rebuild（hide は emit 合流・順序は視覚のみで
                // rebuild は backend スレッド）。indexing 中の Err は意図的無音（#434 parity）。
                self.emit_hide();
                if let Err(e) = crate::commands::rebuild_index(app.state(), app.clone()) {
                    crate::trace_main(
                        "egui_slash_error",
                        serde_json::json!({ "cmd": "/s", "error": e }),
                    );
                }
            }
            SlashCmd::Quit => {
                // exit-requested listener（main.rs）が history/icon flush → exit する
                // 唯一の終了合流点。トレイの終了メニューと同じ経路。
                let _ = app.emit(crate::events::EXIT_REQUESTED, ());
            }
        }
    }

    /// 選択中の instant コマンドの action を抽出し worker へ投げる（§19.6・#631 async 化）。
    /// action 抽出はここ（UI スレッド）で行い、clipboard 読み + 実行は `start_launch` の
    /// worker スレッド側（`execute_instant_action_core` の契約）。instant は履歴を記録しない
    /// （§19.6）。成功/失敗の後処理は `finish_launch` へ合流。
    ///
    /// **抽出が engine lock を経ないのは #1076 の移行による**（それ以前はここで `engine.lock()`
    /// を取っていた）——ここは egui フレームの中であり、検索 worker が `engine.search` で
    /// `Mutex<Engine>` を握っている間フレームが返らなくなる。**分けている境界は錠の内外ではなく
    /// フレームの内外である**: config の読みは [`crate::egui_shell::read_config`] でフレームの中に
    /// 置き、Win32 clipboard と `ShellExecuteW` は worker へ出す（射程と例外は
    /// `src-tauri/CLAUDE.md`「モジュール構成」の #1032 条項が正本）。
    fn execute_instant_selected(&mut self, index: usize, instant_query: &str, ctx: &egui::Context) {
        let Some(sel) = self.state.results().get(index) else {
            return;
        };
        if sel.is_error {
            return;
        }
        let name = sel.name.clone();
        let Some(action) = crate::egui_shell::read_config(
            &self.app_handle,
            |cfg| {
                cfg.instant_commands
                    .iter()
                    .find(|cmd| cmd.name == name)
                    .map(|cmd| cmd.action.clone())
            },
            // AppState 不在（`.manage` は `.setup` より前ゆえ理論経路のみ）は「見つからない」と
            // 同じ処置へ落とす。**silent no-op にはならない**——下の trace が「黙って消えたのでは
            // ない」ことを残す（**payload は両者を区別しない**。区別が要る日が来たら足すこと）。
            || None,
        ) else {
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
            LaunchWork::Instant {
                name,
                action,
                instant_query: instant_query.to_string(),
            },
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
    pub(super) fn activate_or_execute(
        &mut self,
        index: usize,
        indexing: FrameIndexing,
        visible_rows: FrameVisibleRows,
        ctx: &egui::Context,
    ) {
        // §4.5 の連言④で results 窓が 1 行も描いていないなら起動しない（#1106）。
        //
        // **③（下の `plain_results_hidden`）とは独立した規則であり、carve-out を持たない。**
        // ③が隠すのは Results ビューの通常結果だけだが、④は最大表示件数そのものが 0 なので
        // tool 選択・instant 行・フォルダ展開を含む**すべてのビュー**が 1 行も出ない
        // （`SPEC.md`「4.5 最大列挙数」の「すべてのビューへ一様に適用される」）。ゆえに
        // **`view_kind` による dispatch より前**に置く——下の③のガードの位置では覆えない。
        //
        // **判定は表示側と同じ述語である**——`results_area_collapsed` は
        // `layout::results_window_height` が `0.0`（= hide の契約値）を返す条件そのもので、
        // 同関数がこの述語を呼ぶ。別式を書けば表示と起動が片方だけ変わる将来が生まれる。
        //
        // **`visible_rows` は引数で受け取る**（`indexing` と同じ理由）——`view.rs` が 1 フレーム
        // 1 回読んだ値をそのまま使う。自分で読み直すと、同じフレームの表示ゲートと食い違う。
        //
        // **止めるのは操作だけである。** 行データと選択は消さず、→ / ← のフォルダ突入も
        // 止めない。**#1077（③）の「突入すれば行は可視へ戻る」という理由はここでは成り立たない**
        // ——窓高は 0 のままである。それでも止めないのは、突入が**行の起動ではなく現在地の移動**
        // であり可逆だからで、③と射程を揃えることを優先した。
        //
        // **詰みは作らない。** `/o`（設定を開く）は完全一致した時点で Enter を経ずに走るため
        // （`SPEC.md`「15.1 概要」）このゲートを通らない。設定画面の `1..=50` clamp で戻せる。
        //
        // **`start_launch`（起動の合流点）へ寄せない理由は、③のときと同じではない。**
        // `ADR-activation-gate-placement` 却下 1 は 2 つの独立した理由を挙げるが、
        // **④に当たるのは (b) だけである**——「却下しても数は減らない」（`tools >= 2` の枝が
        // `start_launch` を通らないので `shift_activate` には個別のガードが要る）。(a)
        // （「ガードの意味は行の選択の性質であり、`start_launch` へ届く時点で行はもう無い」）は、
        // **④の述語が行の情報を一切取らないので当たらない**。同じ結論を同じ理由で支えていると
        // 読むと、将来「行を見ない述語」を足す人が誤る。
        if results_area_collapsed(visible_rows.get()) {
            return;
        }
        // §4.7 の表示ゲートで隠れている行は起動しない（#1077）。index 再構築中の通常結果は
        // results 窓から消えるが**行データは保持される**（「データと選択は保持——クリアしない」）
        // ため、`is_unsettled` が偽（打鍵が落ち着いた状態）の Enter は `on_enter` の flush 枝を
        // 通らず、**画面に 1 行も出ていないまま古い行を起動する**（2026-08-16 に実機再現）。
        // #1072 が塞いだのは同じ族の unsettled 側の切片だけだった。
        //
        // **判定は表示側と同じ述語を使う**——別式を書けば真実が 2 つになり、片方だけ変わる
        // 将来が生まれる。ゲートが真になるのは `Results ∧ !instant_rows` のときだけなので、
        // 下の tool / instant の枝は構造的に阻害されない（§19.7 の carve-out は不変）。
        //
        // **止めるのは不可逆な起動だけである**——行を消さないのは `folder_load_pending` と
        // 同じ方針で、前フレーム結果の保持は意図的設計ゆえ温存する。→ / ← のフォルダ突入も
        // 止めない（`on_nav_keys`。突入すれば Folder ビューになり行は可視へ戻る）。
        //
        // **`indexing` は引数で受け取る**（#1077）——`view.rs` が status 行のために 1 フレーム 1 回
        // 読んだ値をそのまま使う。自分で読み直すと、同じフレームの表示ゲートと食い違いうる。
        if plain_results_hidden(
            self.state.view_kind(),
            self.instant_rows_query.is_some(),
            indexing.get(),
        ) {
            return;
        }
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
    fn shift_activate(
        &mut self,
        index: usize,
        indexing: FrameIndexing,
        visible_rows: FrameVisibleRows,
        ctx: &egui::Context,
    ) {
        if self.instant_rows_query.is_some() || self.state.view_kind() == ViewKind::Tool {
            // instant 行は §19.6「Shift+Enter=Enter」。tool ビュー中の Shift+Enter も Enter と同一。
            // **④のガードはここに要らない**——委譲先の冒頭が同じ述語を見る。
            self.activate_or_execute(index, indexing, visible_rows, ctx);
            return;
        }
        // §4.5 の連言④（#1106）。**`activate_or_execute` の同じガードでは覆えない**——
        // `tools >= 2` の枝はそちらを通らず `SearchState::enter_tool` を直接呼ぶ（③と同じ理由）。
        // ツール選択への入場も、1 行も出ていない行を対象に始まる点で起動と同じ性質を持つ。
        // 理由の全文は `activate_or_execute` のガードのコメントが正本である。
        if results_area_collapsed(visible_rows.get()) {
            return;
        }
        // §4.7 の表示ゲート（#1077）。**`activate_or_execute` の同じガードでは覆えない**——
        // `tools >= 2` の枝はそちらを通らず `SearchState::enter_tool` を直接呼ぶ。ツール選択への
        // 入場も、ユーザーが見ていない行を対象に始まる点で起動と同じ性質を持つ。理由の全文と、
        // `indexing` を引数で受け取る理由は `activate_or_execute` のガードのコメントが正本である。
        if plain_results_hidden(
            self.state.view_kind(),
            self.instant_rows_query.is_some(),
            indexing.get(),
        ) {
            return;
        }
        if folder_load_pending(
            self.state.view_kind(),
            self.folder_cache.is_some(),
            self.folder_error.is_some(),
        ) {
            return;
        }
        let Some(row) = self.state.results().get(index) else {
            return;
        };
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
            self.activate_or_execute(index, indexing, visible_rows, ctx); // §18.3: 1 件以下は通常 Enter と同じ動作
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
    /// egui 経路では踏まない。
    ///
    /// **解決は config の読みの中で行う**（#1076 で `engine.lock()` から
    /// [`crate::egui_shell::read_config`] へ移した）。`find_matching_tools` は錠も I/O も取らない
    /// 純 CPU なので、[`crate::egui_shell::read_config`] の「`read` の中で lock を取る操作を
    /// 書かないこと」に反しない——**その純粋性がこの形の前提である**（`snotra_core::opener` 側で
    /// fs に触れるようになったら、解決を読みの外へ出すこと）。
    fn resolve_tools(&self, path: &str, is_folder: bool) -> Vec<OpenerTool> {
        crate::egui_shell::read_config(
            &self.app_handle,
            |cfg| find_matching_tools(path, is_folder, &cfg.openers).to_vec(),
            Vec::new,
        )
    }

    /// auto_hide_on_focus_lost を実行中 config から都度読む（キャッシュしない・#576 と同設計）。
    ///
    /// **毎フレーム無条件に読むのは意図的である**（`ADR-blur-grace-single-field-state-machine` が
    /// 遅延評価を却下して値渡しを採った）。**ただしその却下理由が置いた費用の前提は #1036 で
    /// 変わった**——当時は `read_visual` と `lang()` も engine lock を取っていたので「1 回増える
    /// だけ」と評価できたが、両者が [`crate::egui_shell::read_config`] へ移ったあとは**ここが
    /// 毎フレーム engine lock を取る唯一の箇所**になっていた。#1076 で読み口だけを移し、
    /// 毎フレーム無条件という決定はそのまま保っている。
    fn auto_hide_enabled(&self) -> bool {
        crate::egui_shell::read_config(
            &self.app_handle,
            |c| c.general.auto_hide_on_focus_lost,
            || GeneralConfig::default().auto_hide_on_focus_lost,
        )
    }

    /// instant prefix を実行中 config から都度読む（キャッシュしない・#576 と同設計）。
    /// フィールドは `SearchConfig::instant_command_prefix`（既定は同 struct の `Default` 実装）。
    ///
    /// **この読みは #1076 で `engine.lock()` から [`crate::egui_shell::read_config`] へ移した。**
    /// 呼び出し点（`run_search` / 打鍵の changed エッジ / `on_enter`）はどれも毎フレームではないが、
    /// **どれも egui フレームの中にあり、ユーザーが待っている**——#1032 の規範が挙げる害は
    /// 「フレームが走査の完了まで返らない」ことであって頻度ではない（射程と例外の定義は
    /// `src-tauri/CLAUDE.md`「モジュール構成」の #1032 条項が正本）。
    fn instant_prefix(&self) -> String {
        crate::egui_shell::read_config(
            &self.app_handle,
            |c| c.search.instant_command_prefix.clone(),
            || SearchConfig::default().instant_command_prefix,
        )
    }

    /// index 構築中か（AppState.indexing: AtomicBool・state.rs:14 で確認済み）。実装は
    /// `window_coordinator::read_indexing` へ委譲する——show 経路とバイト単位で同一の
    /// 独立実装を持っていた重複の解消（レビュー是正 3）。
    ///
    /// **返すのは [`FrameIndexing`] であって `bool` ではない**（#1077）——「実際に読んだ」
    /// 証拠を型で運び、別の `bool` を `indexing` のつもりで渡す書き方を構築不能にする。
    /// **`view.rs` はこれを 1 フレーム 1 回だけ呼ぶ**。ここが `pub(super)` のままなのは
    /// `run_search_with` が自分の時点で読むためで（用途が違う——行をクリアするか）、
    /// そこは意図的に live-read を残してある。
    pub(super) fn indexing(&self) -> FrameIndexing {
        super::window_coordinator::read_indexing(&self.app_handle)
    }

    /// UI 文言の言語（config general.language・起動時一回でなく都度読み——lock 1 回/フレームの
    /// 既存ヘルパー群と同型。SU6 の hot-reload 拡張時もこの読み口のまま動く）。
    pub(super) fn lang(&self) -> snotra_core::config::Language {
        crate::egui_shell::read_config(
            &self.app_handle,
            |c| c.general.language,
            // AppState 不在（setup 完了前の理論経路のみ——`.manage` は `.setup` より前）は OS
            // ロケールから導く（#824 の 2 で決定）。固定の `Ja` は `SPEC.md`「7.6 起動時の設定初期化」
            // の「`ja` で始まれば日本語、それ以外は英語」と食い違っており、到達すれば非 ja 環境で
            // 誤った文言を出す。
            // `GeneralConfig::default()` を経由するのは、`default_language()` を `pub` にすると
            // lib crate の公開面が増えて `dead_code` による到達性の検出を失うためである
            // （`docs/adr/ADR-config-default-fallback-references.md`）。
            || snotra_core::config::GeneralConfig::default().language,
        )
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
            let Some(state) = app.try_state::<crate::AppState>() else {
                return;
            };
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
            let sorted = {
                state
                    .engine
                    .lock()
                    .unwrap()
                    .finalize_folder_list_unlimited(entries)
            };
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
        // 世代の加算も in-flight の失効もここには無い（#699 / #1039）——`SearchState` の
        // 行差し替え点が両方を持つ。この関数は folder cache 未着などで `set_results` を
        // 呼ばずに返る経路があり、ここで無条件に進めると「行は変わっていないのに世代だけ
        // 進む」空撃ちになる。世代の意味を「行が差し替わった」と一致させるため、加算は
        // 差し替えと同じ場所に置く。
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
                        if self.state.query().trim().is_empty() || self.indexing().get() {
                            // 空クエリと構築中は**同期でクリアする**（worker を経由させると消した文字が 1 フレーム残る）。同期で差し替える以上 in-flight は失効するが、それは `SearchState` の内側で起きる（#1039）。
                            self.state.set_results(Vec::new());
                            return;
                        }
                        let query = self.state.query().to_string();
                        let seq = self.state.issue_search(self.last_input_at, Instant::now());
                        // 送信できたなら**結果が届くまで前の行を保つ**（folder cache 未着枝と同じ扱い）。
                        if self
                            .search_tx
                            .send(crate::egui_shell::SearchRequest { seq, query })
                            .is_err()
                        {
                            // **worker が死んでいる。** 無界チャネルゆえ `Err` はこれ以外を意味せず
                            // （混雑・一時的失敗が存在しない）、死因（早期 return・panic・将来足す
                            // 経路）を問わず必ずここを通る——死因ごとの塞ぎ方より射程が広い。
                            // **再送では回復しない**（受け手はもう居ない）ため、前の行を保つと
                            // 保持が恒久化し、debounce が armed でない Enter が旧クエリの項目を
                            // 起動する（`on_enter` の flush が「どちらの枝でもクリアする」理由と
                            // 同じ危険である）。上の空クエリ枝と同じ処置へ合流させる。
                            self.state.set_results(Vec::new());
                        }
                    }
                    QueryIntent::Instant {
                        filter_name,
                        instant_query,
                    } => {
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
                            path: if dto.description.is_empty() {
                                dto.display
                            } else {
                                dto.description
                            },
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
                        if matches!(
                            find_slash_command(self.state.query()),
                            Some(SlashCmd::History)
                        ) {
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

    /// worker の結果を採り込む（#1004）。**判定は `SearchState` の内側にある**（#1039）——seq が現
    /// pending と一致し、かつ Results ビューにいるときだけ行が差し替わり、世代と in-flight の失効も
    /// そこで同時に起きる（#699 は無傷）。
    ///
    /// **捨てた理由（`egui_search:dropped` の `"reason"`）は、採り込みを呼ぶ前の `pending_seq` から
    /// 導く**（`SearchState::accept_worker_rows` は 2 種の `None` を区別しない）。`pending_seq() == seq`
    /// のまま `None` が返るのは view ガードの発火だけであり、**ガードの効きを実機で観測しているのは
    /// この区別である**——区別しないと、新設したガードが一度も効いていなくても件数からは分からない。
    ///
    /// **この 2 行の順序を固定する検知器は無い。** 入れ替えると view のケースでも `accept` が既に
    /// pending を take しているため `was_current` が常に偽になり、`reason` が恒久的に `"seq"` へ化ける
    /// ——**この reason を足した理由そのものが静かに失われる**。`SearchState` 側の
    /// `pending_seq_separates_the_two_drop_reasons` は純粋核の内側を測るので、ここの入れ替えでは落ちない。
    pub(super) fn drain_search(&mut self) {
        while let Ok(crate::egui_shell::SearchMsg::Done {
            seq,
            results,
            index_entries,
        }) = self.search_rx.try_recv()
        {
            let now = Instant::now();
            // **採り込みより前に読む**——`accept` が pending を take するため、後では区別できない。
            let was_current = self.state.pending_seq() == seq;
            let Some(settled) = self.state.accept_worker_rows(seq, results, now) else {
                crate::trace::trace(
                    "egui_search:dropped",
                    serde_json::json!({
                        "dispatch_seq": seq,
                        "pending_seq": self.state.pending_seq(),
                        // "view" = Folder ビューへ遷移していた（#1039 のガード）。ガードは Tool も
                        //          弾くが、そちらは `enter_tool` が in-flight を失効させるため
                        //          production では到達しない（`accept_worker_rows` の doc）。
                        // "seq"  = 追い越された、または同期で差し替えて失効していた。
                        "reason": if was_current { "view" } else { "seq" },
                    }),
                );
                continue;
            };
            crate::trace::trace(
                "egui_search:settled",
                serde_json::json!({
                    "dispatch_seq": settled.seq,
                    "pending_seq": self.state.pending_seq(),
                    "index_entries": index_entries,
                    "since_key_us": settled.since_key.as_micros() as u64,
                    "since_dispatch_us": settled.since_dispatch.as_micros() as u64,
                }),
            );
        }
    }

    /// toast ボタンの処理（#532 SU5）。install は Update を原子取得して async へ（Task 8）。
    ///
    /// **状態を変えたら `ctx.request_repaint()` する**（Task 10 実機スモークで発見・
    /// `spawn_folder_load` の egui_ctx wake（本ファイル該当箇所のコメント参照）と同じ理由）:
    /// このランタイムはイベント駆動で、click を処理したこのフレームの描画は toast_action の
    /// 遅延 dispatch より前に完了している。ここで状態を変えても誰も次のフレームを起こさないため、
    /// 無関係な入力（マウス移動等）が来るまで旧 toast が画面に残る（dismiss 後の stale 表示）。
    pub(super) fn handle_toast_action(&mut self, action: ToastAction, ctx: &egui::Context) {
        let Some(st) = self
            .app_handle
            .try_state::<crate::egui_shell::UpdaterUiState>()
        else {
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
        crate::trace_main(
            "egui_update_install_begin",
            serde_json::json!({ "version": update.version }),
        );
        tauri::async_runtime::spawn(async move {
            match update.download_and_install(|_, _| {}, || {}).await {
                Ok(()) => {
                    // Windows では到達しない（内部 exit）。他 OS ビルドや将来変更の防波堤として trace。
                    crate::trace_main("egui_update_install_returned", serde_json::json!({}));
                }
                Err(e) => {
                    // trace と toast の両方が同じ文字列を要する（#654）。**lock を取る前に**
                    // 作る——確保を lock 保持区間へ入れない。
                    let reason = e.to_string();
                    crate::trace_main(
                        "egui_update_install_failed",
                        serde_json::json!({ "error": reason }),
                    );
                    if let Some(st) = handle.try_state::<crate::egui_shell::UpdaterUiState>() {
                        st.0.lock().unwrap().phase =
                            crate::egui_shell::UpdaterPhase::InstallFailed { message: reason };
                    }
                    crate::egui_shell::wake_main(&handle); // 可視中の失敗を即座に描く
                }
            }
        });
    }

    // ---- `update()` の各段から落ちる遷移（呼ぶ順序を決めるのは `view.rs` である）------------

    /// 段 3: show 直後の resetForShow を消費し、**今フレームが reset フレームなら `true`**。
    /// **返り値の破棄は `#[must_use]` が禁じる**（#934。それまでこの散文が唯一の歯止めだった）——
    /// 同一フレームの `ResultsWindow::reset_size_guard()` は
    /// view 側に残る（#749 の位置不変条件・理由は `view.rs` の呼び出し点のコメント）ため、
    /// view が reset フレームを知る手段はこの返り値だけである。
    #[must_use = "消費（swap(false)）した後に reset フレームを知る手段はこの返り値だけである（#749 の位置不変条件・#934）"]
    pub(super) fn consume_reset_pending(&mut self) -> bool {
        // show 直後の resetForShow（EguiShellState.reset_pending を消費）。stale な debounce
        // armed 状態が再表示後に誤発火しないよう、debounce も併せて作り直す。
        if let Some(sh) = self
            .app_handle
            .try_state::<crate::egui_shell::EguiShellState>()
            && sh.reset_pending.swap(false, Ordering::SeqCst)
        {
            self.state.reset(); // hide を跨いだ in-flight もここで失効する（#1039）
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
            // #745: blur 猶予も hide を跨がない。**これを消すと、猶予 armed のまま別経路で
            // hide された後の再 show で、初フレームが `focused == false` なら自動 hide される**
            //（**この呼び出しの消失は `dead_code` が捕まえる**——射程と脆さ、および残る欠落は
            // `BlurGrace::reset` の doc が正本）。
            self.blur_grace.reset();
            true
        } else {
            false
        }
    }

    /// 段 5–6: 外部から届いた pending の消費（index build 完了世代・hotkey 登録失敗）。
    /// **2 段を 1 メソッドに束ねているのは連続する塊だからであって、両者に関係があるからでは
    /// ない**——順序の理由はそれぞれの本文のコメントが持つ。
    pub(super) fn consume_external_pending(&mut self, ctx: &egui::Context) {
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
        if let Some(sh) = self
            .app_handle
            .try_state::<crate::egui_shell::EguiShellState>()
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
            self.notice.set(
                msg,
                self.notice_base.elapsed(),
                crate::egui_shell::NOTICE_HOTKEY,
            );
            ctx.request_repaint();
        }
    }

    /// 段 10–12: 非同期の到着物を回収する（起動結果 → 通知期限 → folder 列挙）。
    /// **この 3 者が同じフレームで呼ばれることが `drain_launch` の通知の期限を成立させている**
    /// （`drain_launch` の `notice.set` 3 分岐は自前の repaint を持たない・`//!` 参照）。
    pub(super) fn poll_async(&mut self, ctx: &egui::Context) {
        // 起動結果の回収（#631）。reset_pending 消費の後に置くこと（spec C 節 不変条件 2）。
        self.drain_launch(ctx);
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
    }

    /// 段 15: Escape の処置（呼ぶのは `key_pressed(Escape)` が真のフレームだけ）。
    /// folder から展開前 query を復元した場合だけ `true` を返す。view はこの信号で、同じ
    /// TextEdit id に残るキャレットを復元 query の末尾へ同期する（#840）。
    #[must_use = "キャレット同期の信号を落とすと、復元した query の末尾へキャレットが寄らない（#840）"]
    pub(super) fn on_escape_pressed(&mut self, ctx: &egui::Context) -> bool {
        // Escape ラダー（folder 中は展開前状態へ復帰、top-level は hide 要求・#532 SU3 M2）。
        // TextEdit より前に ctx から拾うので入力欄に focus があっても届く。
        match self.state.on_escape() {
            EscapeOutcome::RestoredSearch => {
                // folder 離脱 → cache/error 破棄、復帰済み results（展開前の plain 行）を描く
                self.folder_cache = None;
                self.folder_error = None;
                self.instant_rows_query = None;
                ctx.request_repaint();
                true
            }
            EscapeOutcome::RestoredFromTool => {
                // tool 解除 → 直下ビュー（folder/results）を復元描画。folder が下に生きて
                // いるため cache/error は破棄しない（RestoredSearch との差・純粋核 doc 参照）
                ctx.request_repaint();
                false
            }
            EscapeOutcome::Hide => {
                self.emit_hide();
                false
            }
        }
    }

    /// 段 16–17: 今フレームの focus を `BlurGrace` へ畳み、返った処置を実行する。
    ///
    /// **旧・段 14（focus 復帰で猶予を捨てる）と旧・段 34（前フレームの focus を畳む）は
    /// この 1 段へ合流した**（#745）。前フレームとの比較は `BlurGrace` が状態として持つ。
    ///
    /// **`now` はここで 1 回だけ読む**——多重読みが underflow を招く機序は `BlurGrace` の doc。
    pub(super) fn on_focus_changed(&mut self, focused: bool, ctx: &egui::Context) {
        // **`let` へ束縛してから渡す**——`self.blur_grace.observe(.., self.auto_hide_enabled())`
        // は two-phase borrow に依存する形になり、意図が読み取りにくい。
        let auto_hide = self.auto_hide_enabled();
        match self.blur_grace.observe(focused, Instant::now(), auto_hide) {
            crate::egui_shell::BlurAction::Hide => self.emit_hide(),
            // 契約③: 予約はフレームの到来を約束しない（worker は最も早い deadline だけを
            // 単一スロットで持ち、dispatch で take() するため、より早い要求が 1 つ割り込むと
            // 猶予の deadline は黙って消える）。armed の間は毎フレーム残余を要求し直す
            // ——検索 debounce・通知期限・起動タイムアウトと同じ流儀（#711）。
            crate::egui_shell::BlurAction::Rearm(remaining) => ctx.request_repaint_after(remaining),
            // 時間経過では解消しない不成立。再要求すると永久スピンになる（純粋核の doc）。
            crate::egui_shell::BlurAction::Idle => {}
        }
    }

    /// 段 18–20: ↑↓・→← の処置（`move_selection` / folder 展開）。**読み（↑↓ の
    /// `events.retain` 消費込み・→← の非破壊 `key_pressed`）は本メソッドの責務ではない**——
    /// `view.rs` の `read_pre_widget_input`（段 13）が先に読み切り、結果を `nav_down` /
    /// `nav_up` / `right` / `left` の bool で受け取る（#666 段 3）。↑↓ を `events` から
    /// 取り除く責務・#700 の経緯は `read_pre_widget_input` の doc を参照。呼び出し位置
    /// （`view.rs` の TextEdit 構築＝段 21 より前）は本フェーズで動かしていない。
    ///
    /// **別の順序制約が今も本メソッドの呼び出し位置を縛っている**（#700 とは無関係・本
    /// diff 以前から不記載のまま存在）: `move_selection` は `view.rs` の RowsSnapshot
    /// publish（`self.controller.state().selected()` を読み snapshot へ積む段・#699）より
    /// **前**に呼ばれている必要がある——選択直後のフレームで新しい選択値を results 窓へ
    /// 配るためで、現状 `update()` 内の呼び出し順序（本メソッド → snapshot publish）が
    /// それを満たしている。
    pub(super) fn on_nav_keys(
        &mut self,
        nav_down: bool,
        nav_up: bool,
        right: bool,
        left: bool,
        ctx: &egui::Context,
    ) {
        if nav_down {
            self.state.move_selection(1);
        }
        if nav_up {
            self.state.move_selection(-1);
        }

        // → : 選択中がフォルダなら展開（results 中は enter、folder 中は深掘り）。ファイル/エラー行は無反応。
        if right
            && self.state.view_kind() != ViewKind::Tool // §18.5 ←→無効
            && let Some(sel) = self.state.results().get(self.state.selected())
            && sel.is_folder
            && !sel.is_error
        {
            let dir = sel.path.clone();
            let tok = if self.state.view_kind() == ViewKind::Folder {
                self.state.navigate_folder(dir.clone())
            } else {
                // #1079: 突入時点の未反映を frame へ控えさせる。**渡すのは `armed` だけである**
                // ——合成は `SearchState` の内側が持つ（`enter_folder` の doc）。
                self.state
                    .enter_folder(dir.clone(), self.search_debounce.is_armed())
            };
            // → は Folder 中の深掘り・Results からの enter どちらも展開履歴に記録
            // （SolidJS enterFolderExpansion と同一サイト・#532 SU3 M2 Finding #1）。
            self.record_folder_expansion(&dir);
            self.folder_cache = None;
            self.folder_error = None;
            self.spawn_folder_load(tok, dir, ctx.clone());
        }
        // ← : folder 中は親へ、通常検索中は選択項目の親を展開して folder 突入。
        if left {
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
                    if self.instant_rows_query.is_none()
                        && let Some(sel) = self.state.results().get(self.state.selected())
                        && !sel.is_error
                        && let Some(parent) = compute_parent_dir(&sel.path)
                    {
                        // #1079: `→` 側と同じく突入時点の未反映を控えさせる。
                        let tok = self
                            .state
                            .enter_folder(parent.clone(), self.search_debounce.is_armed());
                        // ← from Results は enterFolderExpansion(parent) 相当・記録する。
                        self.record_folder_expansion(&parent);
                        self.folder_cache = None;
                        self.folder_error = None;
                        self.spawn_folder_load(tok, parent, ctx.clone());
                    }
                }
            }
        }
    }

    /// 段 22: TextEdit が `changed()` を返したフレームの処置。`buf` は編集後のバッファ、
    /// `in_folder` は**その TextEdit を組み立てたときの** view_kind（段 21 で読んだ値をそのまま
    /// 渡す——ここで読み直すと同一フレーム内で 2 つの真実ができる）。
    pub(super) fn on_input_changed(&mut self, buf: String, in_folder: bool, ctx: &egui::Context) {
        if crate::trace::trace_enabled() {
            // #840 の実機回帰検査用。入力文字列そのものは診断ログへ残さず、変更前後の
            // 文字数と「旧文字列を prefix に持つ増加か」だけで末尾追記を観測する。
            // state 更新より前でなければ比較元を失うため、この位置を保つ。
            let previous = if in_folder {
                self.state.folder_filter()
            } else {
                self.state.query()
            };
            let before_chars = previous.chars().count();
            let after_chars = buf.chars().count();
            crate::trace_main(
                "egui_input:changed",
                serde_json::json!({
                    "scope": if in_folder { "folder" } else { "search" },
                    "before_chars": before_chars,
                    "after_chars": after_chars,
                    "appended_at_end": after_chars > before_chars && buf.starts_with(previous),
                }),
            );
        }
        if in_folder {
            self.state.set_folder_filter(buf);
            self.run_search(); // folder は同期フィルタ（debounce 不要・I/O 無し）
        } else {
            self.state.set_query(buf);
            // `SPEC.md`「4.9 入力と選択」の実体（folder 側の対は `set_folder_filter` 内の
            // `selected = 0`）。
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

    /// 段 27: trailing debounce の poll と再 arm。
    pub(super) fn poll_search_debounce(&mut self, ctx: &egui::Context) {
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
    }

    /// 段 28: Enter の処置（呼ぶのは `key_pressed(Enter)` が真のフレームだけ・`shift_held` は
    /// 同じ読みで取った modifier）。**TextEdit の `changed()` 処理より後で呼ぶこと**——同一
    /// フレームの IME 確定・paste が旧 state で起動されるのを防ぐ（不変条件 3）。
    ///
    /// **`indexing` は引数で受け取る**（#1077）。`AppState.indexing` は `AtomicBool` の live-read で
    /// 同一フレーム内でも変わりうるため、ここで読み直すと `view.rs` が同じフレームで表示ゲート
    /// （`plain_results_hidden`）へ渡す値と食い違いうる——「画面には出ていないが Enter は起動する」
    /// あるいはその逆が構築可能になる。**値の出所は `view.rs` が status 行のために読む 1 回だけ**で、
    /// [`FrameIndexing`] がその 1 回ぶんを表す。**`shift_held` との取り違えを型で塞ぐために
    /// newtype にしてある**（両方 `bool` なので素で並べると入れ替えてもコンパイルが通る）。
    pub(super) fn on_enter(
        &mut self,
        shift_held: bool,
        indexing: FrameIndexing,
        visible_rows: FrameVisibleRows,
        ctx: &egui::Context,
    ) {
        // #631 flush-on-Enter: 最終クエリの結果がまだ行へ反映されていない間の Enter は、leading 時点の結果や連打前のクエリの結果で起動しうる。未反映の plain クエリは cancel → 同期 engine.search で最終クエリの結果に置換してから dispatch（SolidJS resolveActivationTarget の flushPendingRefresh 同型）。
        // **何をもって「未反映」とするかは `SearchState::is_unsettled` の doc が正本である**（#1038。`armed` だけを渡していた頃に開いていた隙もそこが記す。#1039 で `search_dispatch.rs` の自由関数から移設）。
        let prefix = self.instant_prefix();
        let is_plain = matches!(self.state.interp(&prefix), QueryIntent::Plain);
        if crate::egui_shell::should_flush_on_enter(
            self.state.view_kind(),
            is_plain,
            self.state.is_unsettled(self.search_debounce.is_armed()),
        ) {
            self.search_debounce.cancel();
            // #1004: Enter は最終クエリの結果をその場で要求するため、worker の往復を待てない（待つ設計は Enter 二度押し・Escape・hide の in-flight を全部抱える）。
            // Enter は 1 回きりで、ユーザーは結果を待っている——ここの同期は正当である。
            let query = self.state.query().to_string();
            let searched = if query.trim().is_empty() || indexing.get() {
                None
            } else {
                self.app_handle.try_state::<crate::AppState>().map(|state| {
                    let mut engine = state.engine.lock().unwrap();
                    engine.search(&query)
                })
            };
            // **どちらの枝でも同期で行を差し替える**——空クエリ・indexing 中にクリアを落とすと、古い行が残ったまま直後の activate_or_execute がそれを起動する（`run_search_with` の Plain 早期 return が旧実装で担っていた処置である）。
            self.state.set_results(searched.unwrap_or_default());
            // flush 後の selected は set_results 内の clamp_selected（min クランプ・0 リセットではない）
            // に委ねる——SolidJS parity（resolveActivationTarget → clampSelectedIndex(selected, len)）。
            // flush までの間に ↓↑ で動かした非 0 選択は新結果リストへ clamp されたまま引き継がれる
            //（WebView2 と同挙動。flush 前のリストで確認した行と別物になりうるのは現行製品と同じ受容済み特性）。
        }
        if !self.state.results().is_empty() {
            if shift_held {
                self.shift_activate(self.state.selected(), indexing, visible_rows, ctx);
            } else {
                self.activate_or_execute(self.state.selected(), indexing, visible_rows, ctx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// メソッドの本体を切り出す（終端は 4 スペース字下げの閉じ括弧・内側のブロックはより深い）。
    ///
    /// **母集団は狭すぎても広すぎても壊れる。両方を assert する。**
    ///
    /// - **狭すぎる（空）**: 目印（`canary`）が本体に在ることで確かめる。沈黙する検知器は検知器ではない
    /// - **広すぎる（終端を取り逃す）**: 終端が実際に見つかったことで確かめる。**取り逃すと本体が
    ///   EOF まで伸び、この `mod tests` 自身が持つ文字列リテラルを飲み込む**——`contains` 系の
    ///   assert は必ず真になり、**検知器が空虚になったまま緑で通る**
    ///
    /// **行の走査に `str::lines` を使うのは改行コード非依存にするためである**（CI 実測）。
    /// `find("\n    }\n")` は CRLF で checkout された作業ツリーに一致せず、上の「広すぎる」を
    /// 起こした。手元の `core.autocrlf=input` では再現せず、**CI（git-for-windows の system
    /// 既定 `core.autocrlf=true`）でだけ落ちた**。同じ非対称は `.gitattributes` の冒頭コメントが
    /// `.githooks/**` について記録している。`str::lines` は `\n` で分割し末尾の `\r` を落とす。
    fn method_body(src: &str, anchor: &str, canary: &str) -> String {
        let (before, after) = src
            .split_once(anchor)
            .unwrap_or_else(|| panic!("{anchor} が見つからない（改名したらこの検査も直す）"));
        // **アンカーの字下げは終端の字下げと組である。** ずれると既存の 2 assert は
        // どちらも発火しないまま母集団が壊れる——#1108 で両方向を実測した（列 0 のアンカーは
        // 内側ブロックの `    }` で黙って狭まり、8 スペースのアンカーは自分の終端を通り越して
        // 隣のメソッドを黙って飲み込む）。**見るのは字下げ幅だけである**——アンカーと行頭の
        // あいだには可視性修飾が挟まりうる（現に `pub(super) ` が挟まる呼び出しが在る）。
        // ゆえに**同じ字下げの doc コメント行にアンカー文字列が先行出現した場合は通る**——
        // そこは下流の canary が捕まえる（`top_level_fn_body` 側はアンカーを行頭に密着させる
        // 形なので、あちらでは doc の先行出現もこの assert が落とす。非対称は意図である）。
        // **空白文字の種類まで見る**——`trim_start` はタブも落とすので、バイト差だけで数えると
        // `\t\t\t\t` 字下げのアンカーが字下げ 4 として通る（終端は `    }` なので母集団は壊れる）。
        // [`method_header`] と同じ形の欠陥であり、同じ形で塞ぐ（2026-08-17 の反証レビューが
        // `method_header` 側で実測し、同一パターンの走査でこちらを見つけた）。
        let head = before.rsplit('\n').next().unwrap_or("");
        assert!(
            head.len() - head.trim_start().len() == 4 && head.starts_with("    "),
            "{anchor} を含む行が 4 スペース字下げで始まっていない——終端の `    }}` が内側ブロックか\
             外側の閉じ括弧に一致し、母集団が黙って狭まる／広がる"
        );
        let mut body = String::new();
        let mut terminated = false;
        for line in after.lines() {
            if line == "    }" {
                terminated = true;
                break;
            }
            body.push_str(line);
            body.push('\n');
        }
        assert!(
            terminated,
            "{anchor} の終端（4 スペース字下げの `}}`）が見つからない——母集団が EOF まで\
             伸びており、この検査は空虚である"
        );
        assert!(
            body.contains(canary),
            "母集団が {anchor} の本体を含まない——終端の切り出しがずれた。\
             沈黙する検知器は検知器ではない"
        );
        body
    }

    /// [`method_body`] が改行コードに依存しないことを、**この作業ツリーの改行コードによらず**
    /// 固定する（#1077）。`include_str!` は checkout された実ファイルを読むため、
    /// LF の環境で `cargo test` が緑でも CRLF の環境で母集団が壊れうる——実際に CI でそうなった。
    #[test]
    fn method_body_is_line_ending_agnostic() {
        let lf = "    fn target(&self) {\n        marker();\n    }\n    fn next(&self) {\n";
        let crlf = lf.replace('\n', "\r\n");
        for (label, src) in [("LF", lf), ("CRLF", crlf.as_str())] {
            let body = method_body(src, "fn target(", "marker(");
            assert!(
                !body.contains("fn next("),
                "{label}: 終端を取り逃して次の関数まで飲み込んでいる"
            );
        }
    }

    /// [`method_body`] が**アンカーの字下げ違反を拒む**ことを固定する（#1108）。
    ///
    /// 終端が「4 スペース字下げの `}`」なので、アンカーの字下げがずれると母集団は黙って狭まる
    /// （列 0 のアンカーは内側ブロックの `    }` で切れる）か、黙って広がる（8 スペースの
    /// アンカーは自分の終端を通り越して隣のメソッドを飲み込む）。**両方向とも既存の 2 assert
    /// （終端・canary）は 1 つも発火しない**——#1108 で実測した。
    #[test]
    #[should_panic(expected = "4 スペース字下げで始まっていない")]
    fn method_body_rejects_an_anchor_at_the_wrong_indent() {
        method_body(
            "pub fn target() {\n    marker();\n    if c {\n    }\n}\n",
            "pub fn target(",
            "marker(",
        );
    }

    /// [`method_body`] が**深すぎる字下げのアンカーも拒む**ことを固定する（#1108）。
    ///
    /// 上のテストと**別に置く**——浅い側（列 0）の fixture だけでは、述語を `== 4` から `>= 4` へ
    /// 弱める変異が捕まらない（どちらの述語でも赤になるため）。**広がる方向こそ #1077 / #1108 の
    /// 沈黙そのものである**——自分の終端を通り越して隣のメソッドを飲み込む。
    #[test]
    #[should_panic(expected = "4 スペース字下げで始まっていない")]
    fn method_body_rejects_an_anchor_indented_too_deeply() {
        method_body(
            "mod outer {\n    impl C {\n        fn target(&self) {\n            marker();\n        }\n        fn other(&self) {\n            secret();\n        }\n    }\n}\n",
            "fn target(",
            "marker(",
        );
    }

    /// [`method_body`] が**タブ字下げのアンカーを拒む**ことを固定する（#1112）。
    ///
    /// 上の 2 本と**別に置く**——どちらも字下げの「幅」がずれる fixture なので、`trim_start` の
    /// バイト差だけで数える形をどちらも落とさない。終端は `    }`（スペース 4）に固定なので、
    /// タブ 4 個のアンカーを受理すると母集団は隣のメソッドまで伸びる。
    #[test]
    #[should_panic(expected = "4 スペース字下げで始まっていない")]
    fn method_body_rejects_a_tab_indented_anchor() {
        method_body(
            "impl C {\n\t\t\t\tfn target(&self) {\n\t\t\t\t\tmarker();\n\t\t\t\t}\n    fn other(&self) {\n    }\n}\n",
            "fn target(",
            "marker(",
        );
    }

    /// [`method_body`] が**終端の無い母集団を拒む**ことを固定する（#1112）。
    ///
    /// `top_level_fn_body`（`indexing.rs`）は同じ内容の回帰を 3 本持つのに、こちらは字下げの
    /// 2 本しか持っていなかった——**assert は在るが、それを消しても全部緑のままだった**
    /// （#1108 の PR 本文が形 B 側について記した状態が、形 A 側に残っていた）。
    #[test]
    #[should_panic(expected = "母集団が EOF まで伸びており")]
    fn method_body_rejects_a_population_without_a_terminator() {
        method_body(
            "    fn target(&self) {\n        marker();\n",
            "fn target(",
            "marker(",
        );
    }

    /// [`method_body`] が**canary を含まない母集団を拒む**ことを固定する（#1112）。
    ///
    /// 上のテストと**別に置く**——終端の fixture は canary を含むので、canary の assert を
    /// 消す変異はあちらでは捕まらない。
    #[test]
    #[should_panic(expected = "の本体を含まない")]
    fn method_body_rejects_a_population_without_the_canary() {
        method_body("    fn target(&self) {\n    }\n", "fn target(", "marker(");
    }

    /// 字下げ 4 のメソッドヘッダ行なら、その行を trim したものを返す。
    ///
    /// 受理する形は `fn` の前に可視性修飾（`pub` / `pub(super)` 等）と `async` が挟まるもの
    /// までである——[`method_body`] が字下げ幅だけを見ているのと同じ理由で、現に
    /// `pub(super) ` が挟まる定義が在る。`///` の doc 行は `fn` へ辿り着かないのでヘッダに
    /// ならない（doc はヘッダの**上**に在るため、[`owners_of`] では 1 つ前のメソッドへ
    /// 帰属する——コードではないので取りこぼしてよい方向である）。
    ///
    /// **字下げは幅だけでなく空白文字の種類まで見る。** `trim_start` はタブも落とすので
    /// バイト差だけで数えると `\t\t\t\tfn …` が字下げ 4 として通り、[`owners_of`] では偽の
    /// ヘッダとして帰属を横取りしうる（2026-08-17 の反証レビューが実測。当時このファイルに
    /// タブ字下げの行は 1 件も無く、`rustfmt.toml` が無いので `hard_tabs=false` が効いていた
    /// ——**露出は無かったが、露出が無いことは述語が正しいことを意味しない**）。
    fn method_header(line: &str) -> Option<&str> {
        let trimmed = line.trim_start();
        if line.len() - trimmed.len() != 4 || !line.starts_with("    ") {
            return None;
        }
        let mut rest = trimmed;
        if let Some(after) = rest.strip_prefix("pub") {
            // `pub` / `pub(crate)` / `pub(super)` …——`(` から `)` までを読み飛ばす
            rest = match after.strip_prefix('(') {
                Some(scoped) => scoped.split_once(')').map_or(after, |(_, r)| r),
                None => after,
            };
        }
        rest = rest.trim_start();
        rest = rest.strip_prefix("async ").unwrap_or(rest).trim_start();
        rest.starts_with("fn ").then_some(trimmed)
    }

    /// `needle` の出現を**ファイル全体から**列挙し、各出現を直前の [`method_header`] へ
    /// 帰属させる（出現行ごとに 1 要素・要素は帰属先のヘッダ行）。
    ///
    /// **否定形の検査のための道具である。** 切り出した母集団 `P` に対する
    /// `assert!(!P.contains(x))` は `P ⊇ B`（本体を取りこぼさないこと）を要求するのに、
    /// [`method_body`] の 2 assert（終端・canary）が縛るのは `P ⊆ B` の側だけである——
    /// **canary より後・禁止語より前で母集団が切れると、canary は通り禁止語は切り捨てられ、
    /// 検査は緑のまま沈黙する**（#1112 で rustc 実測。存在形ならこの切れ方で赤になるので、
    /// 沈黙は否定形に固有である）。ここでは**終端を求めない**ので切り詰めが起こりえず、
    /// 下界は「ファイル全体は常に本体の上位集合」から構成で満たされる。
    ///
    /// **境界の倒れ方**（網羅の主張はしない——下の残余がその実例である）:
    /// - 本体内の入れ子 `fn`（字下げ 8）はヘッダに一致しないので、その中の出現は**外側の
    ///   メソッドへ帰属する**＝過剰発火＝赤
    /// - 複数行文字列・コメントの中の**出現**も帰属して過剰発火する＝赤
    /// - 最初のヘッダより前の出現（`use` 節・モジュール doc）は誰にも帰属せず無視される。
    ///   起動の入口の外なので赤にする理由が無い
    ///
    /// # 受容する残余（緑へ倒れる形）
    ///
    /// **少なくとも次の 2 つが在る**（尽くしてはいない）。どちらも**文字列の状態追跡や複数行の
    /// 正規化では直さない**——検査の道具立てが検査対象より複雑になる。
    ///
    /// **(1) 偽ヘッダによる帰属の横取り。** 複数行文字列やブロックコメントの中に**字下げ 4 で
    /// `fn ` から始まる行**が在ると、それが偽のヘッダとして帰属先を横取りし、同じメソッドの
    /// 後続の禁止語はその偽ヘッダへ帰属して**緑になる**（2026-08-17 に対照つきで実測——偽ヘッダ
    /// 有りで緑・無しで赤）。**#1112 の穴より狭い**: 旧設計は文字列中の `    }` 1 行で切れたのに
    /// 対し、こちらは起動の入口のいずれかの中で、禁止語より前に、ヘッダの形をした行が文字列／
    /// コメントへ入ることを要する（実測時点でこのファイルに偽ヘッダは 1 件も無く、認識された
    /// ヘッダはすべて実定義だった）。**タブ字下げも偽ヘッダの形だった**——`\t\t\t\tfn …` が
    /// バイト差 4 で通っていた分は [`method_header`] の空白文字判定で塞いだが、**スペース
    /// 字下げ 4 の偽ヘッダは残る**。
    ///
    /// **(2) 整形器が出現そのものを消す形。** 照合は `line.contains(needle)` で**行内に閉じて
    /// いる**ため、綴りが行をまたぐと出現が消える。`self.indexing()` を根とするメソッドチェーンが
    /// 十分長いと rustfmt は `self` を単独の行へ落とし、次の行を `.indexing()` から始める——
    /// 2026-08-17 に rustfmt へ直接与えて実測した（`&&` での折り返しや 2 連鎖では `self.indexing()`
    /// は 1 行に保たれる。パス呼び出しの `read_config(` はチェーンではないのでこの形を取らない）。
    /// **これは #1112 が入れた回帰ではない**——旧実装の `body.contains("self.indexing()")` も、
    /// `body` が改行を含む文字列である以上まったく同じ折り返しで偽になる。
    ///
    /// **列挙した 2 つのうち、人間の意図を要さないのは (2) である**——(1) はヘッダの形をした行を
    /// 文字列やコメントへ書く人間を要するのに対し、(2) は整形器が勝手に作る（尽くしていない残余の
    /// 側にも同じ性格のものが在りうる）。**その意味でこの検査は `cargo fmt` が実際に走ることに
    /// 暗黙に依存している**——PostToolUse hook と PR CI の両方が現行の設定（`rustfmt.toml` を
    /// 置いていないので `hard_tabs=false` を含む既定値）で走ることが、ここで扱う行の形を
    /// 保っている。整形の設定を変えるときはこの検査の当たり方も一緒に見ること。
    fn owners_of(src: &str, needle: &str) -> Vec<String> {
        let mut owners = Vec::new();
        let mut current: Option<&str> = None;
        for line in src.lines() {
            // 帰属先の更新を needle の照合より**先**に行う——ヘッダ行自身に出現がある場合は
            // そのメソッドへ帰属する（過剰発火＝赤側）。
            if let Some(header) = method_header(line) {
                current = Some(header);
            }
            if line.contains(needle) {
                owners.extend(current.map(str::to_string));
            }
        }
        owners
    }

    /// [`method_header`] が**字下げ 4 ちょうど**を要求することを固定する（#1112）。
    ///
    /// **コーパスからは測れない。** `include_str!` が読むこのファイルには字下げ 0 / 8 の
    /// `fn ` 行が 1 本も無く、述語を `>= 4` へ緩めても字下げを見なくしても、認識される
    /// ヘッダの集合が変わらない——2026-08-17 に鏡の実装で 3 通りを実測し、いずれも
    /// ヘッダ数が同数で [`activation_uses_frame_values_not_live_reads`] は緑のままだった。
    /// ゆえに合成 fixture でしか固定できない。
    ///
    /// **`>= 4` への緩みを落とすのは字下げ 8 の行が `None` である assert だけではない**——
    /// 帰属の側から [`owners_of_attributes_a_nested_fn_to_the_outer_method`] が独立に
    /// もう 1 本持つ。字下げ 0 の行はその変異では落ちないが、字下げを見なくする変異を落とす。
    ///
    /// **fixture の形は「幅」だけでは足りない。** 2026-08-17 の反証レビューが、字下げ幅の
    /// 変異（`>= 4` へ緩める・見なくする）は落ちるのに、**受理する幅の集合を `{4, 12}` へ
    /// 広げる**・**スペースの個数を数える形へ替える**・**先頭 4 スペースの prefix 判定へ
    /// 替える**の 3 つが素通りすることを実測した。ゆえに下の 3 fixture を足してある——
    /// 字下げ 12・タブ 4 個・スペース 4 + タブ 1 個。**どれが何を落とすかは各行のコメント**。
    #[test]
    fn method_header_requires_exactly_four_spaces_of_indent() {
        assert_eq!(
            method_header("    fn target(&self) {"),
            Some("fn target(&self) {")
        );
        // 入れ子の `fn`——`>= 4` へ緩めるとここが偽のヘッダになり、その中の禁止語は
        // 外側のメソッドではなく偽ヘッダへ帰属して緑へ落ちる。prefix 判定への変異も落とす。
        assert_eq!(method_header("        fn nested() {"), None);
        // トップレベルの `fn`——字下げを見なくする変異をここが落とす。
        assert_eq!(method_header("fn top_level() {"), None);
        assert_eq!(method_header("  fn odd() {"), None);
        // 字下げ 12——受理する幅の集合を `{4, 12}` へ広げる変異をここが落とす。字下げ 8 の
        // 行だけでは、8 を除いたまま 12 を足す形が通ってしまう。
        assert_eq!(method_header("            fn deep() {"), None);
        // タブ 4 個——`trim_start` のバイト差だけで数える形をここが落とす（タブもバイト 1 個
        // ずつ落ちるので差は 4 になる）。**スペースを数える変異はここでは落ちない**（数えると
        // 0 個なので、その変異も `None` を返す）。
        assert_eq!(method_header("\t\t\t\tfn tabbed() {"), None);
        // スペース 4 + タブ 1 個——スペースを数える変異をここが落とす（数えると 4 個なので
        // 受理へ倒れる）。現行の実装はバイト差が 5 になるので拒む。
        assert_eq!(method_header("    \tfn mixed() {"), None);
        assert_eq!(method_header("    let counted = fn_like();"), None);
    }

    /// [`method_header`] が **`fn` の前の可視性修飾と `async` を読み飛ばす**ことを固定する
    /// （#1112）。現に `pub(super) ` が挟まる定義が起動の入口に在り、読み飛ばしが壊れると
    /// [`activation_uses_frame_values_not_live_reads`] の入口が 1 本認識されなくなる。
    #[test]
    fn method_header_accepts_visibility_and_async_before_fn() {
        for line in [
            "    pub fn a(&self) {",
            "    pub(crate) fn b(&self) {",
            "    pub(super) fn c(&self) {",
            "    async fn d(&self) {",
            "    pub(crate) async fn e(&self) {",
            "    pub(super) async fn f(&self) {",
        ] {
            assert_eq!(method_header(line), Some(line.trim_start()), "{line}");
        }
    }

    /// [`owners_of`] が**入れ子の `fn` の中の出現を外側のメソッドへ帰属させる**ことを固定する
    /// （#1112）。[`owners_of`] の doc が「境界の倒れ方」の 1 つ目として主張している挙動で、
    /// それを成立させている実装事実は [`method_header`] の字下げ 4 ちょうどである。
    ///
    /// 帰属先を**完全一致で**測る——`contains` で測ると、偽ヘッダへ横取りされた帰属が
    /// 綴りの部分一致で通ってしまう形を作りやすい。
    #[test]
    fn owners_of_attributes_a_nested_fn_to_the_outer_method() {
        let src = "impl C {\n    fn outer(&self) {\n        fn nested() {\n            forbidden();\n        }\n        forbidden();\n    }\n    fn other(&self) {\n    }\n}\n";
        assert_eq!(
            owners_of(src, "forbidden("),
            vec![
                "fn outer(&self) {".to_string(),
                "fn outer(&self) {".to_string(),
            ]
        );
    }

    /// [`owners_of`] が**字下げ 4 のヘッダを持たない出現を落とす**ことを固定する（#1112）。
    ///
    /// [`owners_of`] の doc が「最初のヘッダより前の出現は誰にも帰属せず無視される」と書く
    /// 側の挙動である。トップレベル（字下げ 0）の `fn ` がヘッダに数えられないことも同時に
    /// 固定する——数えられると、この fixture の 1 件目が帰属先を持って列挙へ現れる。
    #[test]
    fn owners_of_drops_occurrences_without_an_indent_four_owner() {
        let src = "fn top_level() {\n    forbidden();\n}\nimpl C {\n    fn outer(&self) {\n        forbidden();\n    }\n}\n";
        assert_eq!(
            owners_of(src, "forbidden("),
            vec!["fn outer(&self) {".to_string()]
        );
    }

    /// [`owners_of`] が改行コードに依存しないことを固定する（#1112）。
    ///
    /// [`method_body`] と同じ処方である（`docs/development-principles.md`「検証の層と、層と層の
    /// 隙間」——切り出しの helper 自身を LF / CRLF 両方の fixture で測る。#1077 の CI 実害から
    /// 生えた条項で、あちらは終端の探索が CRLF の作業ツリーに一致せず母集団が壊れた）。
    ///
    /// **帰属先を完全一致で測るのが要点である**——`contains` で測ると、`src.lines()` を
    /// `src.split('\n')` へ替える変異が捕まらない。`trim_start` は行頭しか見ないので末尾の
    /// `\r` はヘッダ文字列の中に残り、部分一致はそれでも通る（2026-08-17 に対照つきで実測）。
    #[test]
    fn owners_of_is_line_ending_agnostic() {
        let lf = "impl C {\n    fn outer(&self) {\n        forbidden();\n    }\n}\n";
        let crlf = lf.replace('\n', "\r\n");
        let expected = vec!["fn outer(&self) {".to_string()];
        for (label, src) in [("LF", lf), ("CRLF", crlf.as_str())] {
            assert_eq!(owners_of(src, "forbidden("), expected, "{label}");
        }
    }

    /// Enter の判定と表示ゲートが**同一フレームの同じ値**を見ることを固定する（#1077 / #1106）。
    ///
    /// 対象は表示ゲートの入力 2 つである。`AppState.indexing` は `AtomicBool` の live-read で
    /// **同一フレーム内でも変わりうる**。`visible_rows` は config の live-read で、
    /// `config_watcher` の適用が同じフレームへ割り込みうる。**どちらも、起動の入口が自分で
    /// 読み直すと `view.rs` が表示ゲートへ渡す値と食い違う**——「画面には出ていないが Enter は
    /// 起動する」あるいはその逆が構築可能になる。値は `view.rs` が 1 回だけ読み、
    /// [`FrameIndexing`] / [`FrameVisibleRows`] として配る。
    ///
    /// **測れるのは構造だけである。** どちらの型にもテスト席が無く、食い違いの発生は
    /// タイミング依存ゆえ決定的に再現できない。ゆえに「渡された値を使っていること」を
    /// ソーステキストで固定する——読み直しの形が本体に無いことがその形である。
    ///
    /// **この検査は母集団を切り出さない**（#1112）。禁止語の不在を測る否定形ゆえ、
    /// [`method_body`] で切り出すと**canary より後・禁止語より前で切れたときに沈黙する**
    /// （機序と境界規則は [`owners_of`] が正本）。代わりにファイル全体から出現を列挙し、
    /// 各出現を直前のメソッドヘッダへ**帰属**させて、起動の入口 3 本に帰属するものが
    /// 1 つも無いことを測る。
    ///
    /// **`run_search_with` は対象外である**（意図的）。あちらの読みは用途が違い
    /// （行をクリアするか）、到達経路ごとにその時点で判断するのが正しい。`read_config` を
    /// 正当に使うヘルパー（`lang()` など）も同様で、**帰属先が起動の入口ではないので対象外へ
    /// 落ちる**。**受け入れ条件はこの帰属の規則そのものであって、対象外の件数ではない**
    /// ——件数を書くと、正当な読みを 1 つ足すたびにこの散文だけが黙って腐る
    /// （#1076 で `read_config` を使うヘルパーが増えたときに実際に腐った）。
    ///
    /// **帰属は 1 段の間接で抜ける。** 起動の入口がヘルパーを呼び、そのヘルパーが `read_config`
    /// を呼ぶ形（`on_enter` → `instant_prefix`、`activate_or_execute` → `activate` →
    /// `resolve_tools`）は緑のまま通る。**現時点で欠陥ではない**——どちらも `visible_rows` を
    /// 読み直しておらず、読み自体は #1076 の移行より前から在ったものである。**この検査が塞ぐのは
    /// 「起動の入口が自分の中で読み直す」形だけだと読むこと**、そして**ヘルパーの本体を入口へ
    /// インライン展開しないこと**（展開した瞬間に帰属が入口へ移り、この検査は赤になる）。
    ///
    /// **この検査は禁止語と needle を自分のソースへリテラルで綴る。** 母集団がファイル全体
    /// なので、その行も列挙に入る。緑であるのは**帰属の副作用**である——それらの行は自分の
    /// テスト関数のヘッダへ帰属し、そのヘッダは起動の入口ではないので assert を通る
    /// （母集団を切り出していた頃は、テスト側が母集団の外に在ることが構造でそれを保証して
    /// いた）。帰結として、禁止語のリテラルを起動の入口のいずれかの中へ書けばこの検査は
    /// 自分で自分を赤にする——そう書く理由が無いので**受容する残余**とする。
    #[test]
    fn activation_uses_frame_values_not_live_reads() {
        let src = include_str!("launcher_controller.rs");
        let entry_points = [
            "fn on_enter(",
            "fn activate_or_execute(",
            "fn shift_activate(",
        ];
        // **canary の代役はここである。** 切り出しを無くしたので「母集団が空」は起こりえないが、
        // 「対象が 1 つも認識されていない」なら検査は同じように沈黙する。3 本のアンカーは
        // 可視性修飾の有無で 2 形（`pub(super) fn` / 素の `fn`）に分かれるので、この assert は
        // 改名だけでなく [`method_header`] の修飾読み飛ばしが壊れた場合にも赤になる。
        // **消さないこと**——これが沈黙を塞いでいる唯一の assert である。
        let headers: Vec<&str> = src.lines().filter_map(method_header).collect();
        for anchor in entry_points {
            assert!(
                headers.iter().any(|header| header.contains(anchor)),
                "{anchor} が字下げ 4 のメソッドヘッダとして見つからない——改名したかヘッダの\
                 認識が壊れており、以下の検査は 1 つも発火しない（沈黙する検知器は検知器ではない）"
            );
        }
        for owner in owners_of(src, "self.indexing()") {
            for anchor in entry_points {
                assert!(
                    !owner.contains(anchor),
                    "{anchor} が `indexing` を自分で読み直している——`view.rs` が表示ゲートへ渡す値と\
                     同一フレーム内で食い違いうる（#1077）。引数で受けた FrameIndexing を使うこと"
                );
            }
        }
        // 連言④も同じ形で守る（#1106）。**構築子が private なので偽の値は作れない**——
        // 残る一手が「本物をもう 1 回読む」ことであり、それをここで塞ぐ。読み直す形は
        // `read_visible_rows` の直呼びと、`read_config` から `effective_visible_rows` を
        // 引く形の 2 つである（後者は `lang()` が同じ関数を正当に使うので、起動の入口へ
        // 帰属する出現だけを見るこの検査でしか禁止にできない）。
        for forbidden in ["read_visible_rows(", "read_config("] {
            for owner in owners_of(src, forbidden) {
                for anchor in entry_points {
                    assert!(
                        !owner.contains(anchor),
                        "{anchor} が `{forbidden}` を呼んでいる——**起動の入口での config 読みは\
                         `visible_rows` の読み直しと区別できない**（無関係な読みでもここは落ちる。\
                         それでよい: 読み直しなら `view.rs` が表示ゲートへ渡す値と同一フレーム内で\
                         食い違い、#1106 の症状が再発する）。連言④は引数で受けた FrameVisibleRows で\
                         判定し、他の config 値が要るならこの入口の外で読むこと"
                    );
                }
            }
        }
    }

    /// 起動の入口が §4.7 の表示ゲートを見ていることを、**ソーステキストで**固定する（#1077）。
    ///
    /// **述語のテストでは呼び出し点の脱落を捕まえられない。** `plain_results_hidden` 自身は
    /// [`crate::egui_shell::search_state`] の `mod tests` が固定しているが、それは
    /// 「述語がどんな値を返すか」しか測らず、**入口がその述語を呼んでいるか**は測らない。
    /// この型にはテスト席が無い——本ファイルに `mod tests` が無かったのは
    /// [`LauncherController`] の構築が `AppHandle` と engine lock を要求するためで、
    /// **ソーステキスト検査はそのどちらも要らない**（`indexing.rs` の
    /// `start_index_build_invalidates_the_icon_cache` と同じ形）。
    ///
    /// **これが落ちたとき失うもの**: index 再構築中は §4.7 の表示ゲートが通常結果を隠すが、
    /// 行データは保持される（「データと選択は保持——クリアしない」）。入口がゲートを見なくなると、
    /// **画面に 1 行も出ていない状態の Enter / クリック / Shift+Enter が古い行を起動する**。
    /// 2026-08-16 に実機で再現済みで、行は正しく出るため挙動テストでは捕まらない。
    ///
    /// **見るべきゲートは 2 つあり、独立である**（#1106 で④を足した）。③（`plain_results_hidden`）は
    /// index 再構築中の Results ビューの通常結果だけを隠すが、④（`results_area_collapsed`）は
    /// 最大表示件数そのものが 0 なので tool 選択・instant 行・フォルダ展開を含む**すべてのビュー**が
    /// 1 行も出ない。**片方を見ているだけでは足りない**——④の症状も 2026-08-16 に実機で再現した
    /// （`visible_rows = 0` で `egui_results:show` が 0 件のまま `egui_launch` が出た）。
    ///
    /// **残る死角**: 母集団は当該メソッドのソーステキストだけであり、呼び出しグラフは辿らない。
    /// **ゲートをこのメソッドの外のヘルパーへ移すこと自体は、この検査が赤にする**——本体から
    /// `plain_results_hidden(` / `results_area_collapsed(` の綴りが消えるためである（同じ機序を
    /// #1108 で実測した）。**ただし測っているのは本体テキストへの部分文字列一致であって呼び出しでは
    /// ない**——移設後も本体にその綴りが残れば緑のまま通る（移し先の名前がそれを含む場合も、
    /// 説明コメントへ書き残した場合も同じ）。捕まらないのは、**移した先でゲートが落ちる**退行の
    /// 方である。
    #[test]
    fn activation_entry_points_consult_the_display_gate() {
        let src = include_str!("launcher_controller.rs");
        // (アンカー, 母集団が空でないことを示す目印)
        let targets = [
            ("fn activate_or_execute(", "execute_tool_selected("),
            ("fn shift_activate(", "folder_load_pending("),
        ];
        for (anchor, canary) in targets {
            let body = method_body(src, anchor, canary);
            assert!(
                body.contains("plain_results_hidden("),
                "{anchor} が §4.7 の表示ゲート（連言③）を見ていない——index 再構築中に\
                 画面から消えた行を Enter / クリック / Shift+Enter が起動する（#1077 で実機再現済み）"
            );
            assert!(
                body.contains("results_area_collapsed("),
                "{anchor} が §4.5 の表示ゲート（連言④）を見ていない——最大表示件数が 0 で\
                 1 行も描かれていない状態を Enter / クリック / Shift+Enter が起動する\
                 （#1106 で実機再現済み）"
            );
        }
    }

    /// `on_enter` が flush 判定を**述語へ委ねている**ことを、ソーステキストで固定する（#1112）。
    ///
    /// **述語のテストでは呼び出し点の脱落を捕まえられない**（この規範の正本は上の
    /// [`activation_entry_points_consult_the_display_gate`] の doc）。`should_flush_on_enter`
    /// を綴る production はこの呼び出しの 1 行だけで、述語自身のテストは
    /// [`crate::egui_shell::search_state`] の `mod tests` に在る——呼び出しを外しても
    /// あちらは緑のままである（2026-08-17 に対照つきで実測）。
    ///
    /// **`on_enter` を上の `targets` へ足す形は採らない。** あちらが当てる 2 つのゲート
    /// （`plain_results_hidden` / `results_area_collapsed`）を `on_enter` の本体は持たず、
    /// 持つ場所でもない——ゲートを見るのは委譲先の `activate_or_execute` / `shift_activate`
    /// で、両方とも既に `targets` に在る。固定したい不変条件が別物なのでテストを分ける。
    ///
    /// **これは存在形の assert である**——母集団が途中で切れれば綴りごと消えて赤になるので、
    /// [`owners_of`] が塞いだ否定形の沈黙はここには当たらない。**ただし赤になるのは、探す綴りが
    /// canary（`self.activate_or_execute(`）より前に在る現在の並びにおいてである**——切り詰めが
    /// 綴りより後・canary より前で起きれば canary が落ちるが、綴りより前で起きれば綴りが落ちる。
    /// **綴りを canary より後ろへ動かすと、その間で切れた母集団は canary を通しつつ綴りを
    /// 捨てる**（否定形の沈黙と同じ機序が、極性を変えて存在形に当たる形）。並びを変えるなら
    /// canary も動かすこと。
    ///
    /// # 何を保証し、何を保証しないか
    ///
    /// **保証するのは 1 つだけである**——`if crate::egui_shell::should_flush_on_enter(` という
    /// 綴りが `on_enter` の本体テキストに現れること。#631 の flush 判定が丸ごと落ちる形（当該の
    /// 行が消える）はこれで赤になる。
    ///
    /// **保証しないもの**（2026-08-17 の反証レビューが実測した経路。**少なくとも次を含む**
    /// ——尽くしてはいない）:
    /// - **テキストであって呼び出しではない。** 同じ綴りを説明コメントや文字列リテラルへ
    ///   書き残せば緑で通る。パスまで含む長い綴りなので偶然そう書く形ではないが、機構としては
    ///   何も止めていない
    /// - **呼び出しが在ることは委譲が在ることを意味しない。** これは上の「部分文字列一致」の
    ///   特殊例ではなく**別種の欠落**である——`let _ = crate::egui_shell::should_flush_on_enter(…);`
    ///   と書けば**本物の述語への本物の呼び出しが残ったまま**判定は本体の書き下ろしへ移る。
    ///   `rustc -D warnings` も `clippy -D warnings -W pedantic` も exit 0 で通る（実測）
    ///
    /// **綴りを長くして塞いだもの**: 同名のクロージャで影を作る形・別レシーバの同名メソッドへ
    /// 差し替える形・上の `let _ =` の形は、`if ` と `crate::egui_shell::` を綴りへ含めたことで
    /// 落ちるようになった（対照つきで実測——短い綴り `should_flush_on_enter(` では 3 つとも緑）。
    /// **述語の道具立ては増やしていない**——増えたのは探す文字列の長さだけである。
    ///
    /// **代償は整形と書き方への脆さで、向きは赤側である**: `let should = crate::egui_shell::…;
    /// if should {` のような分解や、rustfmt が `if` とパスのあいだで折り返す形はここを赤にする。
    /// 偽陽性であって沈黙ではないので受容する（直すなら綴りを短くするのではなく、当該の並びへ
    /// 合わせて綴りを更新すること）。
    #[test]
    fn on_enter_delegates_the_flush_decision_to_the_predicate() {
        let src = include_str!("launcher_controller.rs");
        let body = method_body(src, "fn on_enter(", "self.activate_or_execute(");
        assert!(
            body.contains("if crate::egui_shell::should_flush_on_enter("),
            "on_enter が `should_flush_on_enter` を分岐の条件式として呼んでいない——#631 の\
             flush-on-Enter が判定ごと落ちたか、判定の写しが本体へ書き下ろされている（呼び出し\
             だけ残して判定から外す形も含む）。どちらも述語側のテストは緑のまま通る（最終クエリの\
             結果が行へ反映される前の Enter が、leading 時点の結果や連打前のクエリの結果で\
             起動しうる）。**整形や分解でこの綴りが崩れただけの偽陽性もありうる**——その場合は\
             綴りを短くせず、現在の並びへ合わせて更新すること"
        );
    }
}
