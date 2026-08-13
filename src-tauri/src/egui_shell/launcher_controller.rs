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
    Debouncer, EscapeOutcome, QueryIntent, SearchState, SlashCmd, ViewKind, compute_parent_dir,
    find_slash_command, folder_load_pending,
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
    /// 検索要求の同一性（#1004）。PR 1 では同期経路の計器、PR 2 では worker 結果の裁定に使う。
    dispatch: crate::egui_shell::SearchDispatch,
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
            dispatch: Default::default(),
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
    /// 意味は同名フィールドの doc が正本。
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
        // 突入時に in-flight を失効させる——しないと launching 中に worker の遅着結果が届き、隠れているはずの results 窓が drain_search 経由で生え直す（spec §4.5）。
        self.dispatch.invalidate();
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
        self.dispatch.invalidate(); // 同期でクエリごと差し替える＝in-flight は古い（spec §4.5）
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
    /// action 抽出はここ（UI スレッド・engine ロック内）で行い、clipboard 読み + 実行は
    /// `start_launch` の worker スレッド側（action 抽出をロック内・clipboard 読みをロック外
    /// ——`execute_instant_action_core` の契約）。instant は履歴を記録しない（§19.6）。
    /// 成功/失敗の後処理は `finish_launch` へ合流。
    fn execute_instant_selected(&mut self, index: usize, instant_query: &str, ctx: &egui::Context) {
        let Some(sel) = self.state.results().get(index) else {
            return;
        };
        if sel.is_error {
            return;
        }
        let name = sel.name.clone();
        let Some(state) = self.app_handle.try_state::<crate::AppState>() else {
            return;
        };
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
    pub(super) fn activate_or_execute(&mut self, index: usize, ctx: &egui::Context) {
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
            .unwrap_or_else(|| GeneralConfig::default().auto_hide_on_focus_lost)
    }

    /// instant prefix を実行中 config から都度読む（キャッシュしない・#576 と同設計）。
    /// フィールドは `SearchConfig::instant_command_prefix`（既定は同 struct の `Default` 実装）。
    ///
    /// **この読みは `engine.lock()` 越しであり、#1032 の規範（config の live-read は [`crate::egui_shell::read_config`] を通す）の未移行の残余である**——#1036 の移設に入らなかった。射程と例外の定義は `src-tauri/CLAUDE.md` の当該条項が正本で、**「エッジ駆動だから対象外」ではない**（同型の未移行は `egui_shell` にほかにもあり、ここだけが例外なのではない）。**新しい読みを足すなら [`crate::egui_shell::read_config`] へ寄せること。**
    ///
    /// **この関数を移設するときは `docs/architecture.md`「検索フロー（入力 → 結果表示）」の Enter の補足も直すこと**——そこが「Enter の費用は #1038 の前後で変わらない」の根拠に、判定より前でここが払う錠待ちを使っている。
    fn instant_prefix(&self) -> String {
        self.app_handle
            .try_state::<crate::AppState>()
            .map(|s| {
                s.engine
                    .lock()
                    .unwrap()
                    .config()
                    .search
                    .instant_command_prefix
                    .clone()
            })
            .unwrap_or_else(|| SearchConfig::default().instant_command_prefix)
    }

    /// index 構築中か（AppState.indexing: AtomicBool・state.rs:14 で確認済み）。実装は
    /// `window_coordinator::read_indexing` へ委譲する——show 経路とバイト単位で同一の
    /// 独立実装を持っていた重複の解消（レビュー是正 3）。
    pub(super) fn indexing(&self) -> bool {
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
        // 世代の加算はここには無い（#699）——`SearchState::set_results` が持つ。この関数は
        // folder cache 未着などで `set_results` を呼ばずに返る経路があり、ここで無条件に
        // 進めると「行は変わっていないのに世代だけ進む」空撃ちになる。世代の意味を
        // 「行が差し替わった」と一致させるため、加算は差し替えと同じ場所に置く。
        // 来歴は行と一体で更新する（Instant 分岐だけが Some を立て直す・finding 0）。
        self.instant_rows_query = None;
        match self.state.view_kind() {
            ViewKind::Folder => {
                if let Some(err) = &self.folder_error {
                    self.dispatch.invalidate(); // 同期で差し替える＝in-flight は古い（spec §4.5）
                    self.state.set_results(err.clone()); // 列挙失敗行（filter 非適用）
                } else if let Some((ctx, sorted)) = &self.folder_cache {
                    let filtered = ctx.filter_sorted(sorted, self.state.folder_filter());
                    self.dispatch.invalidate(); // 同期で差し替える＝in-flight は古い（spec §4.5）
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
                            // 空クエリと構築中は**同期でクリアする**（worker を経由させると消した文字が 1 フレーム残る）。同期で差し替える以上、in-flight は失効させる（spec の §4.5）。
                            self.dispatch.invalidate();
                            self.state.set_results(Vec::new());
                            return;
                        }
                        let query = self.state.query().to_string();
                        let seq = self.dispatch.issue(self.last_input_at, Instant::now());
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
                            self.dispatch.invalidate();
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
                        self.dispatch.invalidate(); // 同期で差し替える＝in-flight は古い（spec §4.5）
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
                            self.dispatch.invalidate(); // 同期で差し替える＝in-flight は古い（spec §4.5）
                            self.state.set_results(rows);
                        } else {
                            self.dispatch.invalidate(); // 同期で差し替える＝in-flight は古い（spec §4.5）
                            self.state.set_results(Vec::new());
                        }
                    }
                }
            }
        }
    }

    /// worker の結果を採り込む（#1004）。**seq が現 pending と一致するときだけ行を差し替える**——追い越された結果は捨てる。世代は `set_results` が進める（#699 は無傷）。
    pub(super) fn drain_search(&mut self) {
        while let Ok(crate::egui_shell::SearchMsg::Done {
            seq,
            results,
            index_entries,
        }) = self.search_rx.try_recv()
        {
            let now = Instant::now();
            let Some(settled) = self.dispatch.accept(seq, now) else {
                crate::trace::trace(
                    "egui_search:dropped",
                    serde_json::json!({
                        "dispatch_seq": seq,
                        "pending_seq": self.dispatch.pending_seq(),
                    }),
                );
                continue;
            };
            self.state.set_results(results);
            crate::trace::trace(
                "egui_search:settled",
                serde_json::json!({
                    "dispatch_seq": settled.seq,
                    "pending_seq": self.dispatch.pending_seq(),
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
            self.state.reset();
            self.dispatch.invalidate(); // hide を跨いだ in-flight は show 後の行を汚さない
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
    pub(super) fn on_enter(&mut self, shift_held: bool, ctx: &egui::Context) {
        // #631 flush-on-Enter: 最終クエリの結果がまだ行へ反映されていない間の Enter は、leading 時点の結果や連打前のクエリの結果で起動しうる。未反映の plain クエリは cancel → 同期 engine.search で最終クエリの結果に置換してから dispatch（SolidJS resolveActivationTarget の flushPendingRefresh 同型）。
        // **何をもって「未反映」とするかは `search_dispatch::is_unsettled` の doc が正本である**（#1038。`armed` だけを渡していた頃に開いていた隙もそこが記す）。
        let prefix = self.instant_prefix();
        let is_plain = matches!(self.state.interp(&prefix), QueryIntent::Plain);
        if crate::egui_shell::should_flush_on_enter(
            self.state.view_kind(),
            is_plain,
            crate::egui_shell::is_unsettled(
                self.search_debounce.is_armed(),
                self.dispatch.pending_seq(),
            ),
        ) {
            self.search_debounce.cancel();
            // #1004: Enter は最終クエリの結果をその場で要求するため、worker の往復を待てない（待つ設計は Enter 二度押し・Escape・hide の in-flight を全部抱える）。
            // Enter は 1 回きりで、ユーザーは結果を待っている——ここの同期は正当である。
            let query = self.state.query().to_string();
            let searched = if query.trim().is_empty() || self.indexing() {
                None
            } else {
                self.app_handle.try_state::<crate::AppState>().map(|state| {
                    let mut engine = state.engine.lock().unwrap();
                    engine.search(&query)
                })
            };
            // **どちらの枝でも同期で行を差し替える**——空クエリ・indexing 中にクリアを落とすと、古い行が残ったまま直後の activate_or_execute がそれを起動する（`run_search_with` の Plain 早期 return が旧実装で担っていた処置である）。
            self.dispatch.invalidate();
            self.state.set_results(searched.unwrap_or_default());
            // flush 後の selected は set_results 内の clamp_selected（min クランプ・0 リセットではない）
            // に委ねる——SolidJS parity（resolveActivationTarget → clampSelectedIndex(selected, len)）。
            // flush までの間に ↓↑ で動かした非 0 選択は新結果リストへ clamp されたまま引き継がれる
            //（WebView2 と同挙動。flush 前のリストで確認した行と別物になりうるのは現行製品と同じ受容済み特性）。
        }
        if !self.state.results().is_empty() {
            if shift_held {
                self.shift_activate(self.state.selected(), ctx);
            } else {
                self.activate_or_execute(self.state.selected(), ctx);
            }
        }
    }
}
