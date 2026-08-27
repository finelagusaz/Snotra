//! folder の突入・深掘り・折り返しと、列挙の別スレッドロード・その drain（#532 SU3 M2）。
//!
//! **channel は view の寿命で共有し、staleness は世代 token で判定する**——per-nav に channel を
//! 建てる形（起動側の `LaunchInFlight`）とは逆の選択であり、その非対称は意図である
//! （理由は `activation.rs` の `LaunchInFlight` の doc）。[`FolderMsg`] をこのファイルへ閉じる
//! ため、drain は `poll_async` から `drain_folder` として切り出してある。
//!
//! ここに**無いもの**:
//!
//! - **`folder_gen` の bump と `accept_folder_result` の述語**は
//!   [`crate::egui_shell::search_state`] が持つ。ここに在るのは token の発行を受けて spawn する
//!   側と、返ってきた token を照合させる側だけである
//! - **展開後の行の絞り込み**は `search_flow.rs` の `run_search_with`（Folder 腕）。ここは
//!   cache / error を差し替えたあと `run_search` を撃つところまでを担う

use snotra_core::engine::FolderListContext;
use snotra_core::ui_types::SearchResult;
use tauri::Manager;

use super::LauncherController;
use crate::egui_shell::{ViewKind, compute_parent_dir};

/// ナビゲーションスレッド → driver のメッセージ（#532 SU3 M2）。token（= folder_gen）で
/// staleness 判定する（`SearchState::accept_folder_result`）。
pub(super) enum FolderMsg {
    /// 列挙成功: (token, ctx, 全ソート済み full 集合)。driver がキャッシュし filter_sorted で絞る。
    Loaded(u64, FolderListContext, Vec<SearchResult>),
    /// 列挙失敗: (token, 単一エラー行)（§6.6・filter 非適用で常時表示）。
    Failed(u64, Vec<SearchResult>),
}

impl LauncherController {
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

    /// 到着したナビ結果を採り込む（`poll_async` の段 12・#532 SU3 M2）。
    ///
    /// **前後関係は両側とも他所にある**（前は `folder_gen` の bump、後ろは #699 の世代照合）
    /// ——内訳は親モジュールの `//!`「ここに無いもの」が正本である。**`poll_async` から切り出して
    /// あるのは [`FolderMsg`] をこのファイルに閉じるためであって、呼ぶ位置に自由度が生まれた
    /// わけではない**（`drain_launch` → 通知の期限 → ここ、の並びは `poll_async` が持つ）。
    pub(super) fn drain_folder(&mut self, ctx: &egui::Context) {
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
    pub(in crate::egui_shell) fn on_nav_keys(
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
}
