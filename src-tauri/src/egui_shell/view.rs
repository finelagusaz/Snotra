//! egui メインウィンドウの main 窓 1 フレーム（入力の読みと描画・OS 窓への適用）。
//! 検索セッションの状態と遷移は `launcher_controller`（`LauncherController`。責務詳細は
//! そちらの `//!`）が持つ（#666 段 3。依存は一方向——`launcher_controller` はこの型を
//! 見ない）。
//!
//! **入力変換は pre/post の 2 段である**（`read_pre_widget_input` / `read_post_widget_input`）。
//! 1 段にまとめられない理由は各関数の doc にあり、正本は `read_pre_widget_input` の doc。
//!
//! **反映境界は 4 つ（`ui.visuals_mut()` / `ctx.set_visuals` / `ctx.set_fonts` /
//! `window.set_background_color`）あり、1 つの名前に畳んでいない**——このうち本ファイルが
//! 直接呼ぶのは `ctx.set_visuals` と `window.set_background_color` の 2 つで、フォント登録は
//! `font_stack::configure_japanese_font` の**呼び出し点** 2 箇所（`setup` と `update` の
//! font_family 差分の分岐）として持つ。**`ctx.set_fonts` 自体の呼び出しは `font_stack.rs` に
//! あり本ファイルには無い**（#666 段 3 タスク 1 で移設）。`ui.visuals_mut()` は
//! `--include=*.rs` の全域 grep で 0 件である（2026-07-28 実測）。
//!
//! フォント解決と登録は `font_stack`（独立モジュールへ切り出した理由は `font_stack.rs` の
//! `//!`・#666 段 3 タスク 1）。

use snotra_egui_runtime::{EguiView, RuntimeFrame};
use tauri::Manager;

use crate::egui_shell::launcher_controller::{LauncherController, ToastAction};
use crate::egui_shell::{RowTheme, ViewKind};

pub(crate) struct SearchWindowView {
    /// 検索セッション層（show を跨ぐ状態・結果・選択・起動・履歴・期限）の所有者
    /// （#666 段 3）。**依存は一方向である**——`launcher_controller` からこの型は見えない。
    /// `AppHandle` もこちらが単独所有し、view は毎フレーム冒頭で 1 回 clone して使う。
    controller: LauncherController,
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
    last_set_height: f64,
    // results 窓のサイズデルタガードは `ResultsWindow` が持つ（#749 で移設）。**`last_set_*`
    // （main 用）を流用してはならない**という当時の不変条件（Important 1）は、memo が別の型に
    // 分かれたことで構造的に保たれる——同一フレーム内で main のブロックが先に
    // `last_set_width` を更新するため、共有すると results が幅の live-reload に追従しなくなる。
}

impl SearchWindowView {
    pub(crate) fn new(app_handle: tauri::AppHandle) -> Self {
        Self {
            controller: LauncherController::new(app_handle),
            applied_font_family: String::new(),
            applied_background_hex: String::new(),
            last_set_width: 0.0,
            last_set_height: 52.0,
        }
    }

    /// ウィンドウ論理幅は config live-read（SU6 spec 決定 2）。**main（本 view）が両窓（main・
    /// results）の唯一の size writer に一意化されている**（#646 PR2 決定 6: results への幅適用は
    /// `drive_results_window` 経由で main が担い、results 自身は書かない）。
    /// 旧実装の inner_size() 読みは「幅を維持」だったが、config_watcher（notify スレッド）の幅
    /// set_size と 2 次元 read-modify-write で潰し合う race の片翼だった——config を正本にすれば
    /// cross-thread writer 自体が消える（初版 spec の watcher flag 分岐案は却下・並行性レビュー）。
    /// なお flag ON では config_watcher の幅 set_size は get_webview_window=None で元々 no-op。
    fn window_width(&self) -> f64 {
        self.controller
            .app()
            .try_state::<crate::AppState>()
            .map(|s| f64::from(s.engine.lock().unwrap().config().appearance.window_width))
            .unwrap_or(600.0)
    }
}

/// 右端から左へ詰める toast ボタン 1 個。クリックされたら true。disabled は淡色 + 無反応。
///
/// id は `label` から導出する（`ui.next_auto_id()` は非 mutating getter のため、同一フレーム内で
/// 中間の widget allocation を挟まず2回呼ぶと dismiss/install 両ボタンが同一 id になり
/// egui の id クラッシュ検知に触れる——ローカライズ済みラベルは Available 局面で互いに異なるため
/// これを id salt に使う）。
///
/// 文字サイズは `theme.button_size`（正本は `layout::status_size` × 0.92）。固定値を
/// 置かない（SPEC §11「文字サイズに固定値を書かない」・#672）。
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
        egui::FontId::proportional(theme.button_size),
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

/// pre-widget 入力（段 13 で読み切る値）。Escape・↑↓・→← の**読み**は TextEdit 構築（段 21）
/// より前に終える必要があり、うち **↑↓ は消費（`events.retain`）まで含む**——この `retain` は
/// #700 の再発を防ぐ唯一の場所であり、TextEdit の後に回すと ↑↓ が TextEdit へも効いて
/// キャレットが飛ぶ症状が戻る。**Escape・→← のイベントは 1 つも除かない**（読むだけである）。
/// Enter/Shift はここに含まない（`response.changed()` に依存するため後段・
/// `read_post_widget_input` 参照）。1 段にまとめられない理由はこの前後関係の非対称にある。
///
/// **この関数より後で `ctx.input(|i| i.key_pressed(egui::Key::ArrowUp))` /
/// `ArrowDown` を読んでも常に `false` である。** `nav_down` / `nav_up` の読み出しと同時に
/// `events.retain` で ↑↓ の `Event::Key` を `events` から取り除くため、
/// `InputState::key_pressed()`（`num_presses()` 経由で `self.events` を走査する・
/// `egui-0.35.0/src/input_state/mod.rs:743,750-760`・一次資料で確認済み）は以後この 2 キー
/// について沈黙して `false` を返す。**将来 ↑↓ を読む文をこの関数と `on_nav_keys` 呼び出しの
/// 間（旧・段 14〜20 相当の位置）に足す編集者が踏む罠であり、構造では塞げない。**
///
/// 読みを本関数の位置（段 13）へ前寄せしてよい根拠は次の 2 つの**局所的な事実**に限る
/// （「egui の入力はフレーム内で不変だから読む順序は関係ない」という一般命題は**偽**であり、
/// 根拠にしてはならない）: (i) `retain` が `events` から取り除くのは ↑↓ の `Event::Key` だけ
/// である (ii) 本関数の呼び出し位置と、↑↓/→← の**処置**（`move_selection` / folder 展開）を
/// 行う `on_nav_keys` 呼び出しの間にある文（focus 判定・Escape ラダー・blur 猶予）は、
/// ↑↓ イベントを 1 度も読まない（実測）。
fn read_pre_widget_input(ctx: &egui::Context) -> PreWidgetInput {
    let focused = ctx.input(|i| i.focused);
    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));

    // ↑↓ ナビ（結果があるとき）。TextEdit より前に ctx から拾い、入力欄 focus 中も効かせる。
    //
    // **キーイベントは入力欄へ渡さず消費する**（#700）。読むだけ（`ctx.input`）では
    // イベントが残り、focus を保持したままの TextEdit も同じ ↑↓ を処理する——単一行の
    // galley では ↑ が `CCursor::default()`（クエリ先頭）、↓ が `galley.end()`（末尾）へ
    // キャレットを飛ばす（epaint 0.35 の `cursor_up_one_row` / `cursor_down_one_row` の
    // 行外分岐）。結果を ↑ で選び直した直後の打鍵がクエリ**先頭**へ挿入され、
    // 「検索ワードが編集できない」として観測された（`abc` → ↑ → `x` が `xabc` になる・実測）。
    // 消費は無条件に行う: 単一行入力欄で ↑↓ にキャレット移動の用途は無く（SPEC §4.8）、
    // ツール選択中・launching 中は入力欄が非対話ゆえ元から影響が無い。
    let (nav_down, nav_up) = ctx.input_mut(|i| {
        let down = i.key_pressed(egui::Key::ArrowDown);
        let up = i.key_pressed(egui::Key::ArrowUp);
        i.events.retain(|e| {
            !matches!(
                e,
                egui::Event::Key {
                    key: egui::Key::ArrowUp | egui::Key::ArrowDown,
                    ..
                }
            )
        });
        (down, up)
    });

    let right = ctx.input(|i| i.key_pressed(egui::Key::ArrowRight));
    let left = ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft));

    PreWidgetInput { focused, escape, nav_down, nav_up, right, left }
}

/// pre-widget 入力の読み取り値（段 13）。`nav_down`/`nav_up`/`right`/`left` は
/// `on_nav_keys` の処置（`move_selection` / folder 展開）へそのまま渡す——**処置は
/// 1 つもここへ移していない**（#666 段 3）。
struct PreWidgetInput {
    focused: bool,
    escape: bool,
    nav_down: bool,
    nav_up: bool,
    right: bool,
    left: bool,
}

/// post-widget 入力（TextEdit 構築後・段 22 の `changed()` 処理より後で読む値）。Enter/Shift
/// の判定は **`response.changed()` の後**でなければならない——先に読むと同一フレームの
/// 入力確定（貼り付け・IME 確定）と Enter が同時に入ったとき、旧 state の interp/選択で
/// 起動してしまう（codex 発見 4・spec M3 実装確定）。`read_pre_widget_input` とは逆に
/// TextEdit の描画結果に依存するため、1 段にまとめられない。
fn read_post_widget_input(ctx: &egui::Context) -> PostWidgetInput {
    let (enter, shift) = ctx.input(|i| (i.key_pressed(egui::Key::Enter), i.modifiers.shift));
    PostWidgetInput { enter, shift }
}

/// post-widget 入力の読み取り値（段 28）。
struct PostWidgetInput {
    enter: bool,
    shift: bool,
}

impl EguiView for SearchWindowView {
    fn setup(&mut self, context: &egui::Context) {
        // `AppHandle` は controller が単独所有する（不変条件 13）。ここで 1 回 clone して
        // ローカルへ置く——`update()` 冒頭と同じ理由（`tauri::State<'_, T>` の借用元問題）。
        let app = self.controller.app().clone();
        let font_family = super::font_stack::font_family_from_config(&app);
        super::font_stack::configure_japanese_font(context, &font_family);
        self.applied_font_family = font_family;
        // 外部 wake 用の ctx 登録はここに無い（#671 PR D）——wake handle は `attach` が
        // 返し、`create()` から `EguiShellState` へ直接渡る。**この setup が
        // `EguiShellState` を読まなくなったことが、その manage を `create()` の後へ
        // 移せる根拠である**（spec 決定 8）。
    }

    fn update(&mut self, ui: &mut egui::Ui, frame: &mut RuntimeFrame) {
        // managed state を引く `AppHandle` は controller が単独所有する（不変条件 13）。
        // **ここで 1 回だけ clone してローカルに置き、以降の `try_state` はすべてこれから引く**
        // ——`tauri::State<'_, T>` は借用元（= `self.controller`、ひいては `self`）に紐付くため、
        // `self.controller.app()` の戻り値を保持したまま `&mut` の遷移メソッドを呼ぶと E0502 に
        // なる。先例は `results_view.rs` の同型のローカル（Task 5 で実際に踏んだ）。
        let app = self.controller.app().clone();

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

        // テーマ値（色・font・Metrics・show_icons）は 1 フレーム 1 lock で読み切る
        //(#673 spec 決定 4)。live-read 契約はフレーム間の話で不変——**`self.` へ保持しないこと**。
        // 導出は純粋核 visual::visual_snapshot、行高の正本は layout::Metrics::from_config。
        // **ここで読むのは値だけである**: `ctx.set_visuals` / `configure_japanese_font` /
        // ネイティブ背景ブラシの**適用**は従来の位置に残す（描画順の制約があるため）。
        let visual = crate::egui_shell::read_visual(
            &app,
            crate::egui_shell::VisualApplied {
                font_family: &self.applied_font_family,
                background_hex: Some(&self.applied_background_hex),
            },
        );
        let metrics = &visual.metrics;

        // show 直後の resetForShow の消費（検索セッション側のクリアは controller が行う）。
        if self.controller.consume_reset_pending() {
            // results 窓の **サイズ**デルタガードを初期値へ戻す（#646 PR2 決定 6・memo 自体は
            // #749 で `ResultsWindow` へ移設）。これは冗長な set_size を避ける性能上のガードで
            // あり、可視性のような correctness のフラグではない（#671 spec 決定 2 の意図的な分割）。
            // 0 へ戻すことで再 show 後に必ず 1 度は現行 metrics で set_size させる。
            // 可視フラグはここに無い——`ResultsWindow` が所有し、hide_egui_main と
            // drive_results_window の 2 経路が同じ型を通るため後始末が要らない（PR A′）。
            //
            // **呼び出し点をここに保つ**（#749）: この reset は同一フレームの
            // `drive_results_window`（update 末尾）より**前**でなければならない。show 経路
            // （`show_egui_main`）は egui のイベントループとは別のスレッドから走りうるため、そちらへ
            // 移すと「同一スレッド・同一フレーム」というこの前提が消える。
            if let Some(results) = app.try_state::<crate::egui_shell::ResultsWindow>() {
                results.reset_size_guard();
            }
        }

        let ctx = ui.ctx().clone();

        // 外部から届いた pending の消費（index build 完了世代 → 再検索・hotkey 登録失敗の通知）。
        // どちらも reset_pending 消費の後でなければならない（順序の理由は controller 側の本文）。
        self.controller.consume_external_pending(&ctx);

        // §11: パネル/入力欄/選択色を config テーマから（ハードコード撤廃・runtime CLEAR_COLOR は不変）。
        // font_family / native 背景ブラシのエッジ検出も同一 lock で読む（SU6 spec 決定 2・lock 1 回/フレーム）。
        // 値はフレーム冒頭の `visual` から取る（#673）。**適用はこの位置のまま**（呼び出し
        // 位置は本段では動かさない）——egui 0.35.0 では root `Ui` が pass 冒頭で
        // `ctx.global_style()` を `Arc` snapshot するため、ここで呼ぶ `ctx.set_visuals` は
        // 現在の pass の `Ui` に届かない。この潜在バグは #751 であり、**本段では直さない**。
        let mut visuals = ctx.style_of(ctx.theme()).visuals.clone();
        visuals.panel_fill = visual.background;
        visuals.window_fill = visuals.panel_fill;
        visuals.extreme_bg_color = visual.input_bg; // TextEdit 背景
        visuals.selection.bg_fill = visual.selection;
        // TextEdit の hint 色はここだけが効く（#654・詳細は TextEdit 構築部のコメント）。
        // **他の描画を巻き込まない**: この view が使う egui ウィジェットは TextEdit 1 つだけで
        //（`ui.label` / `ui.button` の類は 0 件・残りは raw painter に色を明示渡し）、
        // weak text を読むのはその hint のみ。results 窓は別 Context ゆえ影響外。
        visuals.weak_text_color = Some(visual.hint);
        ctx.set_visuals(visuals);

        // SU6 spec 決定 2: font_family hot-reload（WebView2 の --font-family CSS 変数即時反映 parity）。
        // applied は解決成否に依らず無条件更新（フィールド doc 参照）。
        if let Some(name) = &visual.font_family_changed {
            self.applied_font_family = name.clone();
            super::font_stack::configure_japanese_font(&ctx, name);
            ctx.request_repaint(); // set_fonts は次フレーム適用——欠くと新フォントが 1 イベント遅れる
        }

        // SU6 spec 決定 2: native 背景ブラシ追従（生成時一度きり → 実行時変更へ・codex 反証）。
        // **描画色とは別のパーサを使う**（`parse_hex_color` は `#RRGGBB` 厳格。統合すると
        // `#FFF` 等でこの経路の挙動が黙って変わる・#673 spec / visual.rs の `//!`）。
        if let Some(hex) = &visual.background_hex_changed {
            self.applied_background_hex = hex.clone();
            if let Some(window) = app.get_window("main") {
                let color = crate::config_watcher::parse_hex_color(hex)
                    .unwrap_or(tauri::window::Color(0x28, 0x28, 0x28, 0xff));
                let _ = window.set_background_color(Some(color));
            }
        }

        // 非同期の到着物の回収（起動結果 → 通知期限 → folder 列挙）。**reset_pending 消費の後**
        // に置くこと（spec C 節 不変条件 2）。3 者が同一フレームで走ることも不変条件である。
        self.controller.poll_async(&ctx);

        let pre = read_pre_widget_input(&ctx);

        self.controller.clear_blur_grace_if_focused(pre.focused);
        if pre.escape {
            self.controller.on_escape_pressed(&ctx);
        }
        self.controller.on_focus_changed(pre.focused, &ctx);

        // ↑↓・→← の処置（`move_selection` / folder 展開）。消費込みの読みは
        // `read_pre_widget_input` が段 13 で既に終えている。**この読み（段 13 の
        // `read_pre_widget_input`）が TextEdit 構築より前にあることが #700 の不変条件**
        // ——`on_nav_keys` 自体の呼び出し位置には #700 由来の制約は無い（消費は既に
        // 完了済みのため）。
        self.controller
            .on_nav_keys(pre.nav_down, pre.nav_up, pre.right, pre.left, &ctx);

        // 検索入力欄。state.query を編集し、変化があれば debounce leading で同期検索。
        //
        // **hint は indexing で差し替えない**（#700）。かつては「構築中かつ空クエリなら hint を
        // 案内文へ差し替える」形で、非空クエリのときだけ別の描画面（重ね描き overlay）が担って
        // いた。#700 発見 C で overlay を status 行へ移した結果、この 2 面構成が「1 文字目で案内が
        // 入力欄から下の行へ飛ぶ」動きとして可視化された（実機で観測）。案内の描画面は status 行に
        // 一本化し、hint は本来のプレースホルダへ戻す——**同じ情報に描画面を 2 つ持たない**。
        let in_tool = self.controller.state().view_kind() == ViewKind::Tool;
        let in_folder = self.controller.state().view_kind() == ViewKind::Folder;
        // 入力欄が編集可能か（§18.5 ツール選択中・spec 決定 3/4 の launching 中は無効）。
        // **`interactive` と再フォーカスの両方がこの 1 つを読む**（#700 state-check 発見 B）。
        // 以前は `interactive` が 2 項・再フォーカスが `in_tool` の 1 項で、launching 中は
        // 「非対話ゆえ focus を持てない widget へ毎フレーム `request_focus()` を撃つ」状態だった。
        // 同じ「入力欄を無効化する条件」が 2 箇所で別々に書かれると、片方だけ足した条件が
        // 黙って食い違う——束ねることで構造的に消す。
        let input_editable = !in_tool && !self.controller.is_launching();
        let l = self.controller.lang();
        let hint: &str = if in_tool {
            // SolidJS placeholder.tool_select parity（egui の hint は buf が空のときだけ描かれる＝
            // HTML placeholder と同条件。表示されるのは対象パスが区切り終端等でファイル名が空のとき）
            crate::egui_shell::ui_strings::tool_select_hint(l)
        } else {
            crate::egui_shell::ui_strings::search_hint(l)
        };
        let mut buf = if in_tool {
            // §18.5: 対象の**ファイル名部分のみ**を表示——SolidJS inputValue は targetPath を
            // 区切りで split した末尾を返す（SearchWindow.tsx:255-267）。フルパスではない
            //（plan-review scout-parity 指摘で是正）。
            self.controller
                .state()
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
            self.controller.state().folder_filter().to_string()
        } else {
            self.controller.state().query().to_string()
        };
        // §11 Part C（#643）: 入力欄フォントを config `font_size` へ追従させる。
        // #646 決定 2: バー高は `font_size + bar_padding`（Metrics）。
        // SU6.5 決定 3 の 52px 据え置きは WebView2 parity 制約下の判断で、SU7 の WebView2 撤去
        // により失効した。極端な font_size でバーからはみ出す挙動は変わらず残る。
        //
        // **入力欄は 2 つの文字列を持ち、色の受け取り機構が別である**（#654）:
        // - 入力テキスト本体 → `.text_color()`。egui は `self.text_color` を最優先で見る
        //   （builder.rs `text_color.or_else(override_text_color).unwrap_or(widgets.inactive)`）。
        //   **これを欠くと config を見ずに egui 既定 gray(180) へ落ちる**——#643 は hint だけを
        //   直したため、既定設定のまま入力文字が結果行の表示名より暗い状態が残っていた
        // - hint → **`Visuals::weak_text_color` だけが効く**。egui 0.35 の TextEdit は
        //   `hint_text.map_texts(|t| t.color(visuals.weak_text_color()))` で**無条件に上書き**し、
        //   egui 自身が "users won't be able to override it" と注記している。ゆえに
        //   `RichText::color()` は届かない（#643 の指定は dead だった）。適用は `set_visuals` 側
        //
        // **どちらも「色リテラルを書かない」だけでは守れない**（SPEC §11 の規範はこの 2 様態を
        // 名指しするよう #654 で拡張した）。片方を直してもう片方を放置しない。
        let bar_theme = &visual.row;
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
                        .interactive(input_editable)
                        .font(bar_font.clone())
                        .text_color(bar_theme.name_color)
                        // 色を付けない——付けても egui が weak_text_color で上書きする（上の
                        // コメント）。hint の色は `set_visuals` の `weak_text_color` が正本。
                        .hint_text(egui::RichText::new(hint).font(bar_font)),
                )
            })
            .inner;
        if response.changed() {
            self.controller.on_input_changed(buf, in_folder, &ctx);
        }
        // 窓に focus があるのに入力欄が focus を持たないなら移す（Alt+Q 表示直後に打てる）。
        // was_focused に依存しないので、hide→reshow で was_focused が stale でも確実に戻る。
        // 条件は `interactive` と同じ `input_editable` を読む（非対称の解消は同変数の doc 参照）。
        if pre.focused && input_editable && !response.has_focus() {
            response.request_focus();
        }

        // status 行（#532 SU5 の一時 overlay・#700 で位置を変更）: 「起動中…」/ 失敗・結果不明通知/
        // 非空クエリ indexing 案内を**検索バーの直下に独立した行として**描く。hint_text は空クエリ時
        // のみ描かれるため launching/notice/非空クエリ indexing（query 非空）では使えず、別の描画面が要る
        // ——かつてはそれを TextEdit の rect への重ね描きで賄っていた（#700 で撤回・下のブロック参照）。
        // 優先順は WebView2 SearchWindow.tsx の Switch 先頭一致 parity: indexing > 起動中 > 通知。
        // 空クエリの indexing は hint が描く。非空クエリの indexing は表示ゲート（§4.7）で結果が
        // 消えるため overlay が唯一の案内（spec 追補 1・ladder は overlay_kind に抽出しテスト固定）。
        let overlay_text: Option<String> = match crate::egui_shell::overlay_kind(
            self.controller.indexing() && self.controller.state().view_kind() == ViewKind::Results,
            self.controller.is_launching(),
            self.controller.notice_message().is_some(),
        ) {
            Some(crate::egui_shell::OverlayKind::Indexing) => {
                Some(crate::egui_shell::ui_strings::indexing_hint(self.controller.lang()).to_string())
            }
            Some(crate::egui_shell::OverlayKind::Launching) => {
                Some(crate::egui_shell::ui_strings::launching(self.controller.lang()).to_string())
            }
            Some(crate::egui_shell::OverlayKind::Notice) => {
                self.controller.notice_message().map(|m| m.to_string())
            }
            None => None,
        };
        // #700 発見 C: **入力欄に重ねず、バー直下の独立した行へ描く。** 以前は
        // `response.rect` を不透明に塗り潰していたため、入力欄は編集可能なまま
        // 「打った文字が見えない」状態になり、実際に「検索ワードを編集できない」と
        // 報告された。launching 中は入力欄が非対話（`input_editable`）で整合していたが、
        // indexing（数分に及びうる）と notice（数秒）は編集可能なまま覆われていた。
        // 行の高さは toast と同じ `metrics.toast_height`（= bar_height・#646 決定 2）で、
        // 窓高は `main_window_height` の `status_height` が積む。
        let has_status = overlay_text.is_some();
        if let Some(text) = overlay_text {
            let status_h = metrics.toast_height as f32;
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), status_h),
                egui::Sense::hover(),
            );
            // 色はフレーム冒頭の `visual` から（#673）。ここは ctx のスタイルではなく config を
            // 直接読んでいた箇所ゆえ、`set_visuals` より後という位置に意味は無い（監査 #4）。
            ui.painter().rect_filled(rect, 4.0, visual.input_bg);
            ui.painter().text(
                egui::pos2(rect.left() + 8.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                &text,
                egui::FontId::proportional(visual.row.status_size),
                visual.hint,
            );
        }

        // updater toast（§20.3・#532 SU5）: 検索バー直下の toast_height（= bar_height・#646 決定 2）行・モード非依存
        //（folder/tool/instant 中も表示・状態機械レビュー項 1）。
        let toast_row = app
            .try_state::<crate::egui_shell::UpdaterUiState>()
            .and_then(|st| st.0.lock().unwrap().toast());
        let has_toast = toast_row.is_some();
        let mut toast_action: Option<ToastAction> = None;
        if let Some(row) = toast_row {
            let l = self.controller.lang();
            let theme = &visual.row;
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
                // `..` で受けない（#654）——payload を足して描き忘れる経路を compile-fail に
                // 残すため。整形（空理由でコロンを残さない）は `update_failed` の責務。
                crate::egui_shell::ToastKind::Failed { message } => {
                    crate::egui_shell::ui_strings::update_failed(l, message)
                }
            };
            // メッセージとボタンは**同じ行の中央**に揃える（#700 実機指摘）。旧実装は
            // メッセージを行の 25%・ボタンを 75% に置く 2 行構成（WebView2 UpdateToast の
            // 縦積み parity）だったが、行高は toast_height（= bar_height・既定 43px）しか
            // 無く、2 行ぶんの間隔が取れずに「左上のテキストと右下のボタン」という
            // ちぐはぐな配置になっていた。
            //
            // **ボタンを先に描く**——右寄せの `cursor_x` がボタン群の左端を返すので、
            // それをメッセージの clip 境界に使える。1 行に寄せたことで、行が別だった
            // ときには起きなかった「長いメッセージがボタンへ潜り込む」衝突が生じうる
            // （ボタンは stroke だけで塗り潰さないため、下のテキストが透けて重なる）。
            let mut cursor_x = rect.right() - 8.0;
            let btn_y = rect.center().y;
            let dismiss_label = crate::egui_shell::ui_strings::update_dismiss(l);
            if draw_toast_button(ui, &mut cursor_x, btn_y, dismiss_label, row.buttons_enabled, theme) {
                toast_action = Some(ToastAction::Dismiss);
            }
            if row.show_install {
                let install_label = crate::egui_shell::ui_strings::update_install_now(l);
                if draw_toast_button(ui, &mut cursor_x, btn_y, install_label, row.buttons_enabled, theme) {
                    toast_action = Some(ToastAction::Install);
                }
            }
            // メッセージはボタン群の左端で**末尾省略**する（衝突回避）。`cursor_x` は最後の
            // ボタンぶん進んだ位置ゆえ、間隔の 8.0 を戻して境界にする。
            //
            // クリップではなく省略にするのは、失敗理由（#654）が幅を超えたときに「続きがある」
            // ことを読者へ伝えるため——クリップは文字の途中でぶつ切りにし、切れたことを示さない。
            // `…` は epaint の既定（`TextWrapping` の `overflow_character`）が付ける。
            //
            // **3 variant 共通の描画点である**: Available / Installing の溢れ表現も同時に
            // 省略へ変わる（既定幅では両者 117px 以下で可用幅 532px に収まるため見た目は不変）。
            // 「Failed だけ省略」は分岐を足さないと書けず、その分岐に価値が無い。
            let text_x = rect.left() + 8.0;
            let avail = ((cursor_x + 8.0) - text_x).max(0.0);
            let mut job = egui::text::LayoutJob::single_section(
                line1,
                egui::TextFormat {
                    font_id: egui::FontId::proportional(theme.status_size),
                    color: theme.name_color,
                    ..Default::default()
                },
            );
            job.wrap = egui::text::TextWrapping::truncate_at_width(avail);
            // `single_section` の既定は `break_on_newline: true` だが、置換前の
            // `painter().text()` が使う `simple_singleline` は false。`max_rows: 1` と組むと、
            // 改行を含む失敗理由が**幅と無関係に**そこで切れて `…` になる。挙動を保つため戻す。
            job.break_on_newline = false;
            let galley = ui.painter().layout_job(job);
            ui.painter().galley(
                egui::pos2(text_x, rect.center().y - galley.size().y / 2.0),
                galley,
                theme.name_color,
            );
        }
        if let Some(action) = toast_action {
            self.controller.handle_toast_action(action, &ctx);
        }

        // trailing debounce の poll と再 arm（armed の間は毎フレーム残余を要求し直す）。
        self.controller.poll_search_debounce(&ctx);

        // Enter: 選択項目を起動/実行（Shift は §18.3 のツール選択入場・後置 dispatch は M3 のまま）。
        // TextEdit の changed 処理より後で読む理由は `read_post_widget_input` の doc 参照。
        let post = read_post_widget_input(&ctx);
        if post.enter {
            self.controller.on_enter(post.shift, &ctx);
        }

        // 結果リスト（shouldShowResults 相当）。§4.7: 再インデックス中は plain 結果のみ隠す
        // （instant/folder/tool carve-out・SU6 spec 決定 3）。データと選択は保持——クリアしない
        // （SolidJS parity: setIndexing は結果を触らず派生 memo が非表示を担う）。indexing 中の
        // 案内は空クエリ=hint・非空クエリ=overlay（Task 7・spec 追補 1）が担い、show_results=false
        // では results 窓が hide される（main は bar(+toast)固定高で伸縮しない・#646 PR2 決定 6）。
        // 連言③は**1 フレーム 1 回だけ**読む（#752 F2）。`indexing` は `AtomicBool` の live-read で
        // 同一フレーム内でも変わりうるため、pre/post で 2 回読むと連言③がフレーム内で食い違う。
        // ここで得た値を snapshot 用と `drive_results_window` の両方へ配る。
        let plain_hidden = crate::egui_shell::plain_results_hidden(
            self.controller.state().view_kind(),
            self.controller.instant_rows_query().is_some(),
            self.controller.indexing(),
        );
        // snapshot publish 用は**クリック逆流の消費より前**の値である（#699: publish → 消費の順序）。
        // 一方 `drive_results_window` は件数を消費**後**に読む——この非対称が #752 F2 の要点。
        let show_results = !self.controller.state().results().is_empty() && !plain_hidden;
        // #646 PR2 決定 5: 結果は snapshot として発行し、描画は results 窓(ResultsView)が担う。
        // 変化があったフレームだけ store + wake(毎フレーム wake だと results が常時回る)。
        // 判定は Vec を作る前に行う（/simplify・効率）——無変化フレームで行数ぶんの String
        // 確保を払わないため、`RowsSnapshot::matches` にスライスのまま突き合わせさせる。
        if let Some(shared) = app.try_state::<crate::egui_shell::ResultsShared>() {
            // 表示すべきでないフレームは空スライスを発行する（rows 空 = results 非表示）。
            let rows: &[snotra_core::ui_types::SearchResult] = if show_results {
                self.controller.state().results()
            } else {
                &[]
            };
            let selected = self.controller.state().selected();
            // 旧 view.rs の icon request ゲート `!self.search_debounce.is_armed()`（連打中は
            // icon worker を積まない・perf 最適化）の後継。ResultsView は search_debounce を
            // 持てないため、live 値を snapshot 経由で運ぶ（Task 5 concern 2 の fix・controller 依頼）。
            let settled = !self.controller.is_search_armed();
            {
                let mut guard = shared.snapshot.lock().unwrap();
                if !guard.matches(
                    rows,
                    selected,
                    self.controller.state().rows_generation(),
                    settled,
                ) {
                    *guard = crate::egui_shell::RowsSnapshot {
                        rows: rows.to_vec(),
                        selected,
                        generation: self.controller.state().rows_generation(),
                        settled,
                    };
                    drop(guard);
                    crate::egui_shell::wake_results(&app);
                }
            }
            // クリック逆流の消費(決定 5): 起動ロジックは main の一箇所に保つ。
            // **この消費が snapshot publish の後にある順序は不変条件である**（#699）。
            // 照合に使う世代は、そのフレームで行を差し替えうる全ハンドラ——Escape・
            // index 世代検知・folder drain・launch 完了——より**後**の値でなければ、
            // 「積んだ後・消費する前に総入れ替えが起きた」窓を塞げない。
            match shared.take_clicked_for(self.controller.state().rows_generation()) {
                crate::egui_shell::ClickTake::Current(i) => {
                    self.controller.activate_or_execute(i, &ctx)
                }
                // 破棄は目に見えず手で再現もできないので観測点を残す。**診断用であって
                // 不変条件の担保ではない**（担保は search_state / results_view のユニットテスト）。
                crate::egui_shell::ClickTake::Stale { stamped } => crate::trace_main(
                    "egui_results:click_stale",
                    serde_json::json!({
                        "stamped": stamped,
                        "current": self.controller.state().rows_generation()
                    }),
                ),
                crate::egui_shell::ClickTake::None => {}
            }
        }

        // #646 PR2 決定 6: main は bar(+toast)のみ。結果窓の可視性・サイズ・位置も
        // ここ(毎フレーム走る main)が駆動する——hidden 窓は update() が走らず自分では
        // show できない(SU5 要石)。位置 → サイズ → show の順(main の show と同じ制約)。
        let height = crate::egui_shell::layout::main_window_height(
            metrics.bar_height,
            has_status.then_some(metrics.toast_height),
            has_toast.then_some(metrics.toast_height),
        );
        let width = self.window_width();
        // 判定式の正本は `layout::size_delta_exceeds`（#749）。results 側と**式だけを共有し、
        // memo は共有しない**——main の高さは show 経路の bar_height collapse と
        // `main_window_height` の意図的な 2 導出であり（ADR-0007 却下 1）、その状態を窓の
        // 所有型へ寄せない。
        if crate::egui_shell::layout::size_delta_exceeds(
            (self.last_set_width, self.last_set_height),
            (width, height),
        ) {
            self.last_set_height = height;
            self.last_set_width = width;
            if let Some(window) = app.get_window("main") {
                let _ = window.set_size(tauri::LogicalSize::new(width, height));
            }
            ui.ctx().request_repaint();
        }
        // **`result_count` はここで読む**（#749）——`take_clicked_for`（クリック逆流の消費・
        // 上のブロック）より**後**でなければならない（#752 F2 / ADR-0007）。この式を
        // `plain_hidden` の算出（`show_results` の直前）へ動かすと、行クリック起動フレームで
        // 古い行が 1 フレーム描かれる。`cargo test` では落ちない種類の回帰である。
        crate::egui_shell::drive_results_window(
            &app,
            crate::egui_shell::DriveResultsInputs {
                plain_hidden,
                result_count: self.controller.state().results().len(),
                width,
                row_height: metrics.row_height,
            },
        );

        self.controller.set_focused(pre.focused);
    }
}

#[cfg(test)]
mod tests {
    // `hex_color_parses_and_falls_back` は #673 で `visual.rs` の
    // `hex_parses_valid_and_falls_back_to_config_default` へ移した（hex→Color32 の変換が
    // view から純粋核へ移ったため）。**証明していた命題は 2 つとも移設先で保たれている**
    // ——妥当な hex が期待どおりの色になること、不正文字列が fallback へ落ちること。
    //
    // font_definitions_* 4 件・font_covers_cjk_* 3 件は #666 段 3 タスク 1 で
    // `font_stack::tests` へ移した（フォント解決の実体が `font_stack` へ移設されたため）。
}
