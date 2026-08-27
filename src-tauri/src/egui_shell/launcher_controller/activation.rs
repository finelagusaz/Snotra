//! 起動の入口と、そこから走る dispatch・in-flight 回収・slash の即実行（#631 / #666 段 3）。
//!
//! **起動の入口をこのファイルへ集める規範は要らなくなった**（#1201）。`activation/tests.rs` の
//! ソーステキスト検査は `launcher_controller/` 直下の子 `*.rs` を母集団に取るので、**入口が
//! どの子モジュールに在っても検査は生き続ける**（移設して測った——移すだけなら 3 本とも緑、
//! 移した先でゲートを落とすと赤。同じ移設を変更前の版へ当てると赤だった）。**どの入口が対象かの
//! 正本は散文ではなく `tests.rs` の `entry_points` 配列である**——ここで数え上げると、足したときに
//! この行だけが腐る。
//!
//! **ただし追加は今も沈黙する。** **別の子モジュールに 4 本目の入口を新設する形は、どの検査も
//! 赤にしない**——`entry_points` は明示の配列で、母集団の側から導かれていないためである。
//! **これは受容する死角であり、塞いだとは書けない**（#1201 が消したのは「移動で射程が黙って
//! 狭まる」側だけである）。
//!
//! slash コマンドの即実行（`execute_slash`）もここに置く。§15.3 の「完全一致した時点で実行する」は
//! 起動と同じ不可逆な副作用であり、`clear_search` → action → `emit_hide` という末尾を
//! `finish_launch` と共有する。
//!
//! ここに**無いもの**:
//!
//! - **表示ゲートの述語そのもの**は [`crate::egui_shell::search_state`] と
//!   [`crate::egui_shell::layout`] が持つ。ここに在るのは**その呼び出し点**であり、別式を
//!   書き起こしてはならない（表示と起動が片方だけ変わる将来が生まれる・#1077 / #1106）
//! - **`indexing` / `visible_rows` の読み**はここに無い。`view.rs` が 1 フレーム 1 回読んだ値を
//!   [`FrameIndexing`] / [`FrameVisibleRows`] として受け取る（読み直しを禁じる根拠と、その
//!   検査の射程は `activation/tests.rs` の当該 doc が正本）

use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use snotra_core::config::{OpenerTool, find_matching_tools};
use tauri::{Emitter, Manager};

use super::LauncherController;
use crate::egui_shell::{
    FrameIndexing, FrameVisibleRows, QueryIntent, SlashCmd, ViewKind, folder_load_pending,
    plain_results_hidden, results_area_collapsed,
};

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
    /// （action 抽出だけ UI スレッド・spec C 節。抽出が読む config は `read_config` から取る・#1076）。
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
pub(super) struct LaunchInFlight {
    started: Instant,
    rx: Receiver<crate::commands::launch::LaunchResult>,
    tag: LaunchTag,
}

impl LauncherController {
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
    pub(super) fn drain_launch(&mut self, ctx: &egui::Context) {
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

    /// slash コマンドを実行する（§15.3 即実行・#532 SU3 M3）。SolidJS handleCommandQueryInput と
    /// 同順: クエリ/結果クリア（clearCommandModeState 相当）→ action。`/r`（History）は結果注入型で
    /// ここへ来ない（changed ハンドラが run_search へ振る）。失敗通知は建てない（trace のみ・#631 一本化）。
    pub(super) fn execute_slash(&mut self, cmd: SlashCmd) {
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
    /// 置き、Win32 clipboard と `ShellExecuteW` は worker へ出す（射程は
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
    pub(in crate::egui_shell) fn activate_or_execute(
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

    /// 選択行のオープナー解決（§18.3 最具体ルール 1 件の tools）。IPC/トレイと同じ core
    /// `find_matching_tools` を共有（drift 防止）。is_folder は行（index の真実）から渡し、
    /// `resolve_all_openers` の `Path::is_dir` 再判定（fs touch・dead UNC で滞留しうる）を
    /// egui 経路では踏まない。
    ///
    /// **解決は config の読みの中で行う**（#1076 で `engine.lock()` から
    /// [`crate::egui_shell::read_config`] へ移した）。`find_matching_tools` は錠も I/O も取らない
    /// 純 CPU なので、[`crate::AppState::read_config`] が読みの中に許す範囲に収まる
    /// ——**その純粋性がこの形の前提である**（`snotra_core::opener` 側で fs に触れるように
    /// なったら、解決を読みの外へ出すこと）。
    fn resolve_tools(&self, path: &str, is_folder: bool) -> Vec<OpenerTool> {
        crate::egui_shell::read_config(
            &self.app_handle,
            |cfg| find_matching_tools(path, is_folder, &cfg.openers).to_vec(),
            Vec::new,
        )
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
    pub(in crate::egui_shell) fn on_enter(
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
mod tests;
