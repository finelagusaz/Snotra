//! egui メインウィンドウの placeholder view（#532 SU2）。show/hide/focus/位置を視覚検証できる
//! 最小 chrome を描く。検索本体は SU3。font-first（jp_font を index 0）は SU1 申し送りの義務。

use std::sync::OnceLock;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use snotra_core::engine::FolderListContext;
use snotra_core::ui_types::SearchResult;
use snotra_egui_runtime::{EguiView, RuntimeFrame};
use tauri::{Emitter, Manager};

use crate::egui_shell::{
    Debouncer, EscapeOutcome, HeightParams, QueryIntent, SearchState, ViewKind,
    compute_parent_dir, compute_window_height,
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
    /// エラー行（is_error）は起動しない。
    fn activate(&self, index: usize) {
        use crate::commands::launch::{LaunchStatus, launch_item_core, record_and_save};
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
    fn spawn_folder_load(&self, token: u64, dir: String) {
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
                    return;
                }
            };
            let sorted = { state.engine.lock().unwrap().finalize_folder_list_unlimited(entries) };
            let _ = tx.send(FolderMsg::Loaded(token, ctx, sorted));
        });
    }

    /// view_kind 先の同期 dispatch（#532 SU3 M2）。folder は cache/error を同期フィルタ、
    /// results は M1 の interp 分岐（plain 検索）。folder 打鍵が engine.search へ漏れない。
    fn run_search(&mut self) {
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
            ViewKind::Results => {
                let prefix = self.instant_prefix();
                match self.state.interp(&prefix) {
                    QueryIntent::Plain => {
                        if self.state.query().trim().is_empty() || self.indexing() {
                            self.state.set_results(Vec::new());
                            return;
                        }
                        let query = self.state.query().to_string();
                        let results = {
                            let state = match self.app_handle.try_state::<crate::AppState>() {
                                Some(s) => s,
                                None => return,
                            };
                            let mut engine = state.engine.lock().unwrap();
                            engine.search(&query)
                        }; // lock 解放
                        self.state.set_results(results);
                    }
                    // command/instant は M3。M1/M2 では結果を出さない（空維持）。
                    _ => {
                        self.state.set_results(Vec::new());
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
                    // folder 離脱 → cache/error 破棄、復帰済み results を描く
                    self.folder_cache = None;
                    self.folder_error = None;
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
            && let Some(sel) = self.state.results().get(self.state.selected())
            && sel.is_folder
            && !sel.is_error
        {
            let dir = sel.path.clone();
            let tok = match self.state.view_kind() {
                ViewKind::Folder => self.state.navigate_folder(dir.clone()),
                ViewKind::Results => self.state.enter_folder(dir.clone()),
            };
            self.folder_cache = None;
            self.folder_error = None;
            self.spawn_folder_load(tok, dir);
        }
        // ← : folder 中は親へ、通常検索中は選択項目の親を展開して folder 突入。
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
            match self.state.view_kind() {
                ViewKind::Folder => {
                    if let Some(parent) = self.state.parent_dir() {
                        let tok = self.state.navigate_folder(parent.clone());
                        self.folder_cache = None;
                        self.folder_error = None;
                        self.spawn_folder_load(tok, parent);
                    }
                }
                ViewKind::Results => {
                    if let Some(sel) = self.state.results().get(self.state.selected())
                        && !sel.is_error
                        && let Some(parent) = compute_parent_dir(&sel.path)
                    {
                        let tok = self.state.enter_folder(parent.clone());
                        self.folder_cache = None;
                        self.folder_error = None;
                        self.spawn_folder_load(tok, parent);
                    }
                }
            }
        }

        // Enter: 選択項目を起動（結果があるとき）。TextEdit の Enter より先に ctx で拾う。
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) && !self.state.results().is_empty() {
            self.activate(self.state.selected());
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
                self.last_input_at = Instant::now();
                if self.search_debounce.on_input() {
                    self.run_search(); // leading
                }
                // trailing 発火のため interval 後に再描画を要求する（SU2 blur と同じ egui idiom）。
                ctx.request_repaint_after(self.search_debounce.interval());
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

        // 結果リスト（shouldShowResults 相当。M1: results 軸・plain のみ。空なら描かない）。
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
            self.activate(i);
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
