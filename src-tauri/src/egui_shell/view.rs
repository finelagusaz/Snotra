//! egui メインウィンドウの main 窓 1 フレーム（入力の読みと描画・OS 窓への適用）。
//! 検索セッションの状態と遷移は `launcher_controller`（`LauncherController`。責務詳細は
//! そちらの `//!`）が持つ（#666 段 3。依存は一方向——`launcher_controller` はこの型を
//! 見ない）。
//!
//! **入力変換は pre/post の 2 段である**（`read_pre_widget_input` / `read_post_widget_input`）。
//! 1 段にまとめられない理由は各関数の doc にあり、正本は `read_pre_widget_input` の doc。
//!
//! **反映境界は 5 つ（`ui.visuals_mut()` / `ctx.set_visuals` / `ctx.set_fonts` /
//! `frame.set_clear_color` / `window.set_background_color`）あり、1 つの名前に畳んでいない**
//! ——このうち本ファイルが直接呼ぶのは `ui.visuals_mut()` と `frame.set_clear_color` の 2 つ。
//! **前者の呼び出し点は `search_input_ui` の入口 1 か所である**（#949 で `update()` から移設。
//! 順序不変条件と、それを縛る検査はその関数の doc が正本）。
//! `window.set_background_color` は**リサイズ時に間接呼び出し**である
//! （`window_coordinator::apply_native_background` 経由）。フォント登録は
//! `font_stack::configure_japanese_font` の**呼び出し点** 2 箇所（`setup` と `update` の
//! font_family 差分の分岐）として持つ。**`ctx.set_fonts` 自体の呼び出しは `font_stack.rs` に
//! あり本ファイルには無い**（#666 段 3 タスク 1 で移設）。**`ctx.set_visuals` は
//! `src-tauri/src/` の全域 grep で 0 件である**（#751 で撤去・現在の pass に届かないため。
//! #900 以降は `src-tauri/clippy.toml` の `disallowed-methods` が機構で禁じる）。
//!
//! **この crate では `panel_fill` / `window_fill` を書かない**——main 窓には読む egui コンテナ
//! （`CentralPanel` / `egui::Window` 等）が 1 つも無く、消費者ゼロの死んだ書き込みだった
//! （spec 決定 2 で撤去）。**揃えるために書き足さないこと**——ただし
//! **`snotra-settings` には当てはまらない**（あちらは `CentralPanel` を使うので実消費者が在り、
//! `ctx.set_visuals` こそが正しい API である。`src-tauri/clippy.toml` の禁止が crate スコープに
//! 閉じているのも同じ理由）。
//!
//! **style を経由する 3 値（`extreme_bg_color` / `selection.bg_fill` / `weak_text_color`）と
//! 背景色は、いまはどちらも同じフレームに届く**（#751 で揃えた・経路は別のまま）。背景色は
//! `frame.set_clear_color` が `run_ui` → `paint` の順序に乗るため（spec 決定 1）、3 値は
//! `ui.visuals_mut()` が root `Ui` の style を copy-on-write するためである。**`ctx.set_visuals`
//! へ戻すとこの対称が壊れる**——root `Ui` は pass 冒頭で `ctx.global_style()` を `Arc` snapshot
//! するので、そこへの書き込みは次の pass からしか効かない。
//!
//! フォント解決と登録は `font_stack`（独立モジュールへ切り出した理由は `font_stack.rs` の
//! `//!`・#666 段 3 タスク 1）。

use std::time::Instant;

use snotra_egui_runtime::{EguiView, RuntimeFrame};
use tauri::Manager;

use crate::egui_shell::launcher_controller::{LauncherController, ToastAction};
use crate::egui_shell::{RowTheme, ViewKind};

/// hint の内幅を出すために `ui.available_width()` から引く量（#870）。
///
/// egui 0.35 の `TextEdit` は既定 margin が `Margin::symmetric(4, 2)`（`builder.rs:135`）で、
/// 空バッファのとき hint の最初の text atom に `atom_shrink(true)` が付き、AtomLayout が
/// `TextWrapMode::Truncate` で elide する（`:592-599` / `:678-688`）。その閾値は
/// `available_size - frame.total_margin().sum()`（`atom_layout.rs:318`）＝ **こちらが
/// `ui.available_width()` から引くべき量は margin の水平合計 8.0 ちょうど**である。
///
/// **`text_edit_width`（既定 280）の上限は効かない**——`add_sized` は
/// `Layout::centered_and_justified` の子 ui を作り、AtomLayout は justified なら
/// `max_size.x = f32::INFINITY` へ倒す（`atom_layout.rs:304-312`）。`builder.rs:478` の
/// `allocate_width = desired_width.at_most(available_width)` はそこで捨てられる。
///
/// **frame expansion（`builder.rs:721-725`）も効かない**——差し替わるのは `.allocate(ui)` の
/// **後**で、既に決まった atom の省略判定ではなく描画位置だけを動かす。
///
/// ゆえに `fit_middle_by_measure` の予算と egui の省略閾値は**同じ量を同じ font・同じ
/// painter で測ったもの**になり、収まると測れた候補が egui に再度削られる経路は無い。
/// **見積もりが足りなければ `…` が中間と末尾へ二重に付き、leaf が再び削れて #870 が直って
/// いない状態へ静かに戻る**——カテゴリ D のキャプチャ目視が、その退行に対する検知点である。
const TEXT_EDIT_HINT_H_MARGIN: f32 = 8.0;

/// 入力欄の hint に何を出すか（#836 の優先度ラダー tool > folder > results）。
///
/// **値ではなく「どれか」だけを持つのは、書式化を後段へ送るためである**（#870）。フォルダ
/// 現在地は入力欄の幅に合わせて中間省略する必要があり、幅（`ui.available_width()`）と
/// 測定器（`ui.painter()`）は `TextEdit` を組む内側の `ui` でしか手に入らない。一方
/// `folder_current_dir()` の**読み取り点は動かせない**（下の長文コメント）。ゆえに
/// 「読み取りは外・書式化と省略は内」で切る。
///
/// **ラダーを純粋核へ切り出したのではない**——腕・順序・条件は元の 3 分岐のままである
/// （切り出しは `ADR-folder-location-display-surface` 却下 4 で退けられている。
/// `view_kind()` の 2 重導出になるため）。
enum HintPlan<'a> {
    Tool,
    Folder(&'a str),
    Search,
}

pub(crate) struct SearchWindowView {
    /// 検索セッション層（show を跨ぐ状態・結果・選択・起動・履歴・期限）の所有者
    /// （#666 段 3）。**依存は一方向である**——`launcher_controller` からこの型は見えない。
    /// `AppHandle` もこちらが単独所有し、view は毎フレーム冒頭で 1 回 clone して使う。
    controller: LauncherController,
    /// SU6 spec 決定 2: 適用済み font_family。config 値と毎フレーム比較し差分で再ロード。
    /// **解決の成否に依らず config 値へ無条件更新する**——未解決名（typo・未インストール）で
    /// 毎フレーム load_system_fonts（数十 ms）が走る perf cliff を避ける（並行性レビュー）。
    applied_font_family: String,
    /// 直近に適用した下地の色。**`ResultsWindow::last_background` の main 版**（理由はそちらの doc）。
    ///
    /// **撤去した `applied_background_hex` とは別物である**——あちらは「config の hex と比較して
    /// 変化を検出する」エッジ検出で、hidden 中の変化に居合わせられず取りこぼした。これは
    /// 「実際に Win32 へ撃った色」の memo であり、いつ比較しても差分が正しく出る。
    ///
    /// **リサイズ経路だけが持つ**: show 経路（`show_egui_main`）は状態を持たない関数で、頻度も
    /// ホットキーのトグル程度ゆえ無条件で撃つ。打鍵ごとに走るのはこちらだけである。
    applied_background: Option<egui::Color32>,
    /// SU6 spec 決定 2: 直近 set_size の幅。main（本 view）が両窓（main・results）の唯一の
    /// size writer に一意化されている（幅は config live-read・#646 PR2 決定 6）。
    last_set_width: f64,
    last_set_height: f64,
    /// 起動直後の数フレームだけ「入力欄が打鍵を受け取れる状態か」を残すための残数
    /// （#872/#936）。**打ち切りを持つのは費用のためではなく意味のため**——知りたいのは
    /// 最初のフレームであって、定常状態ではない。
    focus_state_traces_left: u8,
    // results 窓のサイズデルタガードは `ResultsWindow` が持つ（#749 で移設）。**`last_set_*`
    // （main 用）を流用してはならない**という当時の不変条件（Important 1）は、memo が別の型に
    // 分かれたことで構造的に保たれる——同一フレーム内で main のブロックが先に
    // `last_set_width` を更新するため、共有すると results が幅の live-reload に追従しなくなる。
    /// フレーム所要と間隔の計器（#1004 PR 1）。`SNOTRA_TRACE` 無効時も進めてよい（`Instant` 差だけ）。
    frame_timer: crate::egui_shell::FrameTimer,
}

impl SearchWindowView {
    pub(crate) fn new(app_handle: tauri::AppHandle) -> Self {
        Self {
            controller: LauncherController::new(app_handle),
            applied_font_family: String::new(),
            applied_background: None,
            last_set_width: 0.0,
            last_set_height: 52.0,
            focus_state_traces_left: 5,
            frame_timer: Default::default(),
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
        super::window_coordinator::read_window_width(self.controller.app())
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
    let color = if enabled {
        theme.name_color
    } else {
        theme.path_color
    };
    ui.painter().rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0, color),
        egui::StrokeKind::Inside,
    );
    ui.painter().galley(
        egui::pos2(rect.left() + 8.0, center_y - galley.size().y / 2.0),
        galley,
        color,
    );
    enabled && response.clicked()
}

fn move_text_cursor_to_end(ctx: &egui::Context, id: egui::Id, text: &str) {
    if let Some(mut state) = egui::TextEdit::load_state(ctx, id) {
        let end = egui::text::CCursor::new(text.chars().count());
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(end)));
        egui::TextEdit::store_state(ctx, id, state);
    }
}

/// `search_input_ui` が**この関数の中で `ui` へ適用する**テーマ 3 値（#949）。
///
/// **`SearchInputParams` と分けてある**——あちらは `TextEdit` へ**渡す**値、こちらは `ui` へ
/// **適用する**値であり、順序の不変条件を負うのはこちらだけである（理由は `search_input_ui` の doc）。
///
/// **3 フィールドは同型（`Color32`）ゆえ、構築時に取り違えても型は通る。** `search_input_ui` の
/// **中**での取り違えは同関数の検査が捕まえるが、**呼び出し側で `VisualSnapshot` から詰めるとき**の
/// 取り違えは型でもテストでも捕まらない——区別できる観測は非既定色での目視だけである
/// （`docs/build-commands.md`「`[visual]` の色を変える変更は、**非既定色で**目視する」）。
pub(crate) struct InputVisuals {
    /// `TextEdit` の背景（`Visuals::extreme_bg_color`）。
    pub(crate) input_bg: egui::Color32,
    /// 選択帯（`Visuals::selection.bg_fill`）。
    pub(crate) selection: egui::Color32,
    /// hint 文字色（`Visuals::weak_text_color`）。**TextEdit の hint はこれだけが効く**
    /// （egui が `RichText::color()` を無条件に上書きする・#654。機序は TextEdit 構築部のコメント）。
    pub(crate) hint: egui::Color32,
}

/// `search_input_ui` が要る 1 フレーム分の値（`RuntimeFrame` も controller も含まない）。
pub(crate) struct SearchInputParams {
    pub(crate) input_id: egui::Id,
    /// フォルダ絞り込みから展開前 query へバッファ全体を復元したフレームか（#840）。
    pub(crate) restored_search: bool,
    /// 窓が OS の focus を持つか（`RawInput::focused`）。**focus 要求の条件である。**
    pub(crate) window_focused: bool,
    pub(crate) input_editable: bool,
    pub(crate) inset: f32,
    pub(crate) field_height: f32,
    pub(crate) font: egui::FontId,
    pub(crate) text_color: egui::Color32,
}

/// 検索入力欄の widget 合成。**`RuntimeFrame` にも `LauncherController` にも触らない**ので、
/// `egui_kittest` の `Harness` から実コードのまま駆動できる（`mod tests` の kittest 検査）。
///
/// **この関数の内容は 4 つの順序そのものである。** テーマ 3 値の適用（#751/#949）・キャレットの
/// 末尾同期（#840）・focus の要求（#872/#936）は、いずれも後ろへ動かすと同じフレームに効かない
///（前者は子 `Ui` の生成より、後 2 者は `TextEdit` の構築より前でなければならない）。#938 が
/// 入れた単体検査は egui の意味論だけを縛り、`update()` の並びが後ろへ戻っても通る受容残余を
/// 持っていた——**この関数を丸ごと走らせる検査でその残余が閉じる。**
///
/// **どの順序をどの検査が縛るかは分かれている**（ここが本 doc の正本）:
/// - テーマ 3 値 → `search_input_ui_applies_theme_values_to_child_ui_in_the_first_pass`
/// - キャレット → `kittest_restored_frame_appends_same_frame_input_at_end`
/// - focus → `kittest_first_frame_requests_focus_before_text_edit`（判定フレームの開始時点で
///   焦点を持たない状態を作る。作らないと `!has_focus` ガードで要求が走らず縛れない・実測）
/// - 対照（縛れているかの検算）→ `kittest_without_focus_request_the_same_input_is_dropped`
///
/// **適用先は `ctx` ではなくこの `ui` である**（#751・ここが機序の正本）。egui 0.35.0 の
/// `Context::run_ui` は user callback より前に root `Ui` を作り（`context.rs:780-807`）、`Ui::new`
/// はそこで `ctx.global_style()` を `Arc<Style>` として掴む（`ui.rs:108-136`）。ゆえに
/// `ctx.set_visuals` は**現在の pass に届かない**——色だけを変えた config 適用フレームは
/// 「次のフレームが来る保証の無い状況」（設定 UI で色を編集中）と一致するため、入力欄だけが
/// 旧色で取り残された。`ui.visuals_mut()` は copy-on-write でこの `Ui` と**以後に作られる子 Ui**
///（`ui.rs:236` の `Arc::clone`）に効くので、同じフレームに届く。`Context` 経由の書き込みは
/// `src-tauri/clippy.toml` の `disallowed-methods` が禁じる（#900）。
///
/// **この位置は `update()` にあった**（#751）——規範だけで守られ、破っても検知されなかった。
/// #949 で 3 値の唯一の消費者（下の `TextEdit`）を描くこの関数へ吸収し、位置を関数の入口に
/// 固定した。**検査が守るのは「この関数の中」だけである**——`update()` 側でこの関数の呼び出し
/// より前に visuals を読むウィジェットを足す退行は、検査から**原理的に見えない**（受容する残余・
/// 却下した代替案とあわせて `ADR-visuals-order-detector-at-choke-point`）。
///
/// **観測点は `hint` クロージャである**——実コードのクロージャが子 `Ui` を受け取るのを流用する。
/// 代価として、本来必要な「子 `Ui` の生成より前」より 1 段強い「`hint` の呼び出しより前」まで
/// 縛る。適用を関数の入口に置く限り偽陽性は出ない。
///
/// `hint` をクロージャで受けるのは、`HintPlan::Folder` の分岐が `ui.available_width()` と
/// `ui.painter()` を**内側の `Frame` の中で**読むためである。文字列を先に作って渡すと
/// `available_width` が変わり、中間省略の結果が動く。
pub(crate) fn search_input_ui(
    ui: &mut egui::Ui,
    input_visuals: InputVisuals,
    buf: &mut String,
    params: &SearchInputParams,
    hint: impl FnOnce(&mut egui::Ui) -> String,
) -> egui::Response {
    // **子 `Ui` を作る前に適用する**（上の doc の順序）。`Frame::show` より後ろへ動かすと、
    // 子は生成時に親の style を `Arc::clone` 済みで、この書き込みは届かない（実測）。
    //
    // **この書き込みは呼び出し元の `ui` に残る**（`&mut` ゆえ関数を出ても戻らない）。移設前の適用は
    // この呼び出しより前（`update()` 前半・`frame.set_clear_color` の直後）に在ったので、
    // **呼び出しより後ろの範囲は変わっていない**——**前の区間は失った**（上の doc と
    // `ADR-visuals-order-detector-at-choke-point`）。`search_input_ui` より後の status 行・toast は
    // raw painter へ色を明示渡しするため 3 値を読まない。`ui.scope()` で閉じないのは、その
    // **後方の**同値性を保つためである。
    let visuals = ui.visuals_mut();
    visuals.extreme_bg_color = input_visuals.input_bg;
    visuals.selection.bg_fill = input_visuals.selection;
    visuals.weak_text_color = Some(input_visuals.hint);
    // **IME 変換中を「選択帯」ではなく「下線」で描く。**
    //
    // **上の 3 値とは性質が違う**——あちらは config 由来の色（`InputVisuals`）で毎フレーム
    // live-read するが、これは固定の表示方式であり config には現れない。同じ `visuals` へ
    // 書くので位置だけを共有している。
    //
    // **egui はこれを Windows でだけ `true`（＝旧表示）にする。** 理由は「`winit` が韓国語 IME
    // で誤ったカーソル位置を報告する」ことである（egui の `Visuals::ime_composition` の doc）。
    // **その理由は Snotra に当たらない**——この窓は winit を使わず、tao + 自前の IMM32 処理
    //（`snotra-egui-runtime/src/windows_ime.rs`）で preedit を取る。
    //
    // **旧表示のままだと、変換対象の節が分からない。** `windows_ime.rs` は `GCS_COMPATTR` から
    // 変換対象を取り出し `ime::active_range_chars` で文字範囲へ直して `Preedit` へ載せているが、
    // `legacy_visuals` が真だと egui は `cursor_purpose` を `Selection` に固定し、**その範囲を
    // 一度も参照しない**（`egui/src/widgets/text_edit/builder.rs`）。切り替えて初めて、未確定の
    // 全体に細い下線・変換対象の節に太い下線が出る（実機で確認・`ime::active_range_chars` の doc）。
    visuals.ime_composition.legacy_visuals = false;

    egui::Frame::new()
        .inner_margin(egui::Margin::same(params.inset.round() as i8))
        .show(ui, |ui| {
            // **clone せず借りる。** ctx の用途 3 つはいずれも `hint(ui)` より前で終わるので、
            // NLL 上ここで借りれば足りる（呼び出し側は既に clone を 1 本持っている）。
            let ctx = ui.ctx();
            // #840: folder filter から展開前 query へバッファ全体を復元するフレームでは、
            // 同じ widget id に残る egui のキャレットも query 末尾へ同期する。TextEdit の
            // 構築前に行うことで、同一フレームの文字イベントも復元後の末尾から処理される。
            // tool は入力不可の一時表示で元の編集位置を保つため、この経路へは入れない。
            if params.restored_search {
                move_text_cursor_to_end(ctx, params.input_id, buf);
            }
            // **入力欄の focus は TextEdit の構築より前に要求する**（#872/#936）。
            // `TextEdit` は自分が走る時点の focus でイベントを消費するか決めるため、
            // 構築の**後**に要求すると、そのフレームに載っていた文字は焦点の無い widget の
            // 横を素通りして捨てられる。**プロセス起動後の最初のフレームがまさにその形
            // だった**（実測: frame 1 が `has_focus=false`、frame 2 から真。再 show では
            // `Memory` に残るので初回だけ）。窓は可視・前面・focus 済みで「打てるはず」に
            // 見えるのに、ローカルで 50ms・CI runner で 1.4〜19 秒、打った文字が消えていた
            // ——これが #872 の間欠失敗の正体である。
            //
            // **直前の `move_text_cursor_to_end`（#840）が構築前に置かれているのと同じ理由**
            // であり、同一フレームの文字イベントに効かせるには構築前でなければならない。
            //
            // **blur 猶予の状態を読まない**——読む形（例: `blur_grace == Focused` を条件に
            // 足す）にすると、reset-on-show 直後は `NeverFocused` なのに窓は focus を持ちうる
            // ため、show 直後に打鍵できなくなる（SU2 が入れた当の挙動が消える）。条件は
            // `interactive` と同じ `input_editable` を読む（同変数の doc）。
            if params.window_focused
                && params.input_editable
                && !ctx.memory(|m| m.has_focus(params.input_id))
            {
                ctx.memory_mut(|m| m.request_focus(params.input_id));
            }
            let hint_text = hint(ui);
            ui.add_sized(
                egui::vec2(ui.available_width(), params.field_height),
                egui::TextEdit::singleline(buf)
                    .id(params.input_id)
                    // §18.5 ツール選択中の入力は無効化。add_enabled（全体グレーアウト）でなく
                    // interactive(false)（通常描画のまま読み取り専用・changed 不発火）——外観維持。
                    // launching 中も同様に打鍵を止める（Escape/blur/Alt+Q・↑↓は従来どおり通す・
                    // spec 決定 3・4。↑↓は空リストゆえ自然 no-op）。
                    .interactive(params.input_editable)
                    // **文字を欄の縦中央へ置く。** egui の `TextEdit` は `Align2::LEFT_TOP` を
                    // 既定に持ち、`add_sized` が欄を `field_height` へ引き伸ばしても galley は
                    // 上端に留まる——余りが全部下へ落ち、既定 config で上 2px / 下 10px の偏りに
                    // なっていた（`Metrics::bar_inset` の doc が宣言する「文字の上下に 7」が
                    // そこで破れる）。**hint・キャレット・IME 帯もこの 1 行で一緒に動く**
                    //（いずれも galley の位置を起点に描かれるため）——理由と実測は
                    // `input_text_sits_vertically_centered_for_both_body_and_hint`。
                    //
                    // **キャレットと IME 帯には実ピクセルで 1px の非対称が残る**（受容する残余）。
                    // 論理座標は完全対称だが、キャレットの高さは `galley 高 + expand(1.5)×2` ゆえ
                    // フォント次第で偶数行になり、欄の内側（既定 27 行）と偶奇が合わないと余りを
                    // 等分できない。**フォント依存であり、既定 Segoe UI で 3/2・HackGen Console で
                    // 4/4 になることを実機のスクリーンショットで実測した**。`raster.rs` は
                    // カバレッジ AA を持たず、feathering の帯の端がピクセル中心と一致するため
                    // 中間濃度も生まれない——0.5px ずらす実験は**向きが反転しただけ**だった（実測）。
                    // 消すには 0.25px ずらすか欄高を font 連動で ±1 する必要があり、どちらも
                    // 幅いっぱいの枠線を半端な位置へ動かす。**細い線の端 1px より枠線 1px の方が
                    // 目立つ**ため採らない。修正前の偏りは 8px だった。
                    .vertical_align(egui::Align::Center)
                    .font(params.font.clone())
                    .text_color(params.text_color)
                    // 色を付けない——付けても egui が weak_text_color で上書きする。
                    // hint の色は `ui.visuals_mut()` の `weak_text_color` が正本。
                    .hint_text(egui::RichText::new(hint_text).font(params.font.clone())),
            )
        })
        .inner
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
    // 消費は無条件に行う: 単一行入力欄で ↑↓ にキャレット移動の用途は無く（SPEC §4.9）、
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

    PreWidgetInput {
        focused,
        escape,
        nav_down,
        nav_up,
        right,
        left,
    }
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

/// main 窓の当たり判定を、**描かれた矩形そのもの**に一致させる（`setup` から 1 回だけ）。
///
/// egui の既定は `interaction.interact_radius = 5.0`——矩形の**外 5px** までを近傍として
/// その widget の当たりに含める。`hit_test` は「大きな drag 背景の上に載った小さな click
/// widget」を助ける枝を持ち、そこで**背景のドラッグを捨てて**近傍の widget へ click も drag も
/// 渡す。main 窓はまさにその形（背景 = `Sense::drag()` の `max_rect`・入力欄 =
/// `Sense::click_and_drag()`）ゆえ、バー帯の余白 `Metrics::bar_inset`（既定 7.0）のうち
/// **内側 5px が入力欄に食われ**、入力欄以外の全域をドラッグして移動可能という
/// `SPEC.md`「8.2 ウィンドウ位置」の定めが、外側 2px まで痩せていた（実測: 余白 7px のうち
/// 掴めるのは 2px、残りは I ビーム + クリックで入力欄が focus）。
///
/// **書き込み先は global style でなければならない。** ヒットテストが読むのは `Context` の
/// `memory.options.style()` であり、`ui.style_mut()` の copy-on-write は届かない
/// （`ui.visuals_mut()` で 3 値を渡す `search_input_ui` とは経路が違う）。
///
/// **`all_styles_mut` であって `global_style_mut` ではない**——後者が書くのは**現在テーマの
/// style だけ**である（`Options::style()` が `dark_style` / `light_style` を選ぶ）。この窓は
/// テーマ設定に触れないので `theme_preference` は既定の `System` のまま、`system_theme` が
/// `None` の間は `fallback_theme`（Dark）へ落ちる——**そこへ OS が Light を報せた瞬間、
/// 書いていない側の style が現役になり修正が黙って消える**（実測: `RawInput.system_theme =
/// Some(Light)` を 1 フレーム流すだけで 0.0 → 5.0 へ戻る）。現状の `input.rs` は
/// `system_theme` を積まないため到達しないが、**積む変更は色の追従を足すつもりの誰かが書く**
/// ものであり、当たり判定が道連れになることを予測できない。両テーマへ書けば費用ゼロで塞がる。
///
/// **`setup` は `run_ui` の外**（`EguiWindow::new`）**から呼ばれる**ため、この書き込みは第 1
/// pass から効く——`src-tauri/clippy.toml` が禁じる #751 の欠陥（root `Ui` が pass 冒頭で掴む
/// `Arc<Style>` に間に合わない）を、この地点は原理的に持たない。同ファイルが sanctioned な
/// 解消手段として指定する `#[allow]` + 理由コメントで開ける。
///
/// **射程は main 窓の全 widget である**——toast ボタンの当たりも見た目の矩形に一致する
/// （近傍 5px の助けを失う。矩形自体は変えていない）。results 窓は別 `Context` ゆえ無関係。
///
/// **検査が縛るのは適用の帰結だけである**——`bar_margin_belongs_to_the_window_drag_not_the_input_field`
/// はこの関数を直接呼ぶため、`setup` からの**呼び出しを落とす退行には届かない**
/// （`SearchWindowView` の構築に `AppHandle` が要り、テストから組めない）。受容する残余であり、
/// 検知点は実機の目視だけである。
fn apply_exact_hit_test_style(context: &egui::Context) {
    // 上の doc のとおり、ここは run_ui の外なので #751 の「当該 pass に届かない」欠陥を持たない。
    #[allow(clippy::disallowed_methods)]
    context.all_styles_mut(|style| style.interaction.interact_radius = 0.0);
}

impl EguiView for SearchWindowView {
    fn setup(&mut self, context: &egui::Context) {
        // `AppHandle` は controller が単独所有する（不変条件 13）。ここで 1 回 clone して
        // ローカルへ置く——`update()` 冒頭と同じ理由（`tauri::State<'_, T>` の借用元問題）。
        let app = self.controller.app().clone();
        // 当たり判定を描画矩形に一致させる（理由と射程は関数 doc）。**`update()` へ移してはならない**
        // ——書き込み先は global style であり、root `Ui` が pass 冒頭で掴む snapshot に間に合わない。
        apply_exact_hit_test_style(context);
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

        let frame_started = Instant::now();
        let frame_interval = self.frame_timer.begin(frame_started);

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
        // **ここで読むのは値だけである**——**適用**は別の位置に散る: 3 値は `search_input_ui` の
        // 入口（#949 で `update()` から移設）、font は `configure_japanese_font`、背景色は
        // `frame.set_clear_color`。ネイティブ背景ブラシだけは**フレーム冒頭に無い**——show 直前
        //（`update()` の外）と、サイズを変えたときの `applied_background` 分岐（`update()` の末尾・
        // spec 決定 3）の 2 か所である。理由の正本は下の「ここに無い」の段落。
        let visual = crate::egui_shell::read_visual(&app, &self.applied_font_family);
        let metrics = &visual.metrics;

        // show 直後の resetForShow の消費（検索セッション側のクリアは controller が行う）。
        // **真偽をローカルへ残す**——このフレームが「show 直後の最初のフレーム」であることは、
        // 不変条件検出器（レビュー是正 4・下の height 計算点）が突き合わせの発火条件に使う。
        let was_reset_frame = self.controller.consume_reset_pending();
        if was_reset_frame {
            // **show ごとに観測の予算を張り直す**（#872/#936）。egui の widget focus は
            // `Memory` に残るため、**2 回目以降の show で入力欄が focus を保つのか、初回と
            // 同じく最初のフレームで失うのかは、初回だけの計測では言えない**——前者なら
            // 脆弱な窓はプロセス起動時に限られ、後者なら Alt+Q のたびに開く。
            // ここが「show 直後の最初のフレーム」の唯一の判定点である。
            self.focus_state_traces_left = 5;
            // results 窓の **サイズ**デルタガードを初期値へ戻す（#646 PR2 決定 6・memo 自体は
            // #749 で `ResultsWindow` へ移設）。これは冗長な set_size を避ける性能上のガードで
            // あり、可視性のような correctness のフラグではない（#671 spec 決定 2 の意図的な分割）。
            // 0 へ戻すことで再 show 後に必ず 1 度は現行 metrics で set_size させる。
            // 可視フラグはここに無い——`ResultsWindow` が所有し、hide_egui_main と
            // drive_results_window の 2 経路が同じ型を通るため後始末が要らない（PR A′）。
            //
            // **呼び出し点をここに保つ**（#749）: この reset は同一フレームの
            // `drive_results_window`（update 末尾）より**前**でなければならない。show 経路
            // （`show_egui_main`）は証人型（`EventLoopProof`）の導入で同じイベントループ
            // スレッドに閉じたが、**フレームの中ではない**——そちらへ移すと「同一フレーム」
            // というこの前提が消える。
            //
            // **この位置を要求する理由は #749 だけではない**（#745）: 同じ
            // `consume_reset_pending` が `BlurGrace::reset()` を呼んでおり、それが段 16–17
            // （`on_focus_changed`）より**前**であることが blur 猶予の backstop を成立させて
            // いる。#749 の制約が将来失効しても、**#745 の制約は残る**——片方だけを根拠に
            // この呼び出しを動かすと、沈黙で #745 が再発する。
            if let Some(results) = app.try_state::<crate::egui_shell::ResultsWindow>() {
                results.reset_size_guard();
            }

            // **main 窓のサイズ memo も 0 へ戻す**（results と対称・#755）。show 経路は
            // OS のサイズを直接書き、この memo を更新しない。戻さないと「memo == 導出値」の
            // 一致で補正が握り潰され、**導出がずれた瞬間に固着する**。
            //
            // 戻すことの代価は show ごとの同値 `set_size` 1 回だけである（show 経路が既に
            // 同じ高さを設定しているため見た目は変わらない）。得るのは fail-safe である
            // ——導出がずれても 1 フレームで実際に描く高さへ直る。
            //
            // **この 1 手を単独で入れてはならない**: show 経路が実高を導出しない状態でこれを
            // 入れると、補正が必ず撃たれて #801 が全ての show で起きる（実測で確認済み）。
            self.last_set_width = 0.0;
            self.last_set_height = 0.0;
        }

        let ctx = ui.ctx().clone();

        // 外部から届いた pending の消費（index build 完了世代 → 再検索・hotkey 登録失敗の通知）。
        // どちらも reset_pending 消費の後でなければならない（順序の理由は controller 側の本文）。
        self.controller.consume_external_pending(&ctx);

        // 背景色は **style を経由しない**（spec 決定 1）。`render()` が `run_ui` → `paint` の順に
        // 進むため、ここで決めた色は同じフレームの `buffer.fill` に届く。**style を経由する 3 値も
        // #751 以降は同じフレームに届く**（`ui.visuals_mut()` へ移したため）——経路は別のままだが、
        // 到達フレームの非対称はもう無い。
        frame.set_clear_color(visual.background);

        // §11: 入力欄/選択色は config テーマから取る（ハードコード撤廃）。**適用はここに無い**
        // ——3 値（`extreme_bg_color` / `selection.bg_fill` / `weak_text_color`）は
        // `search_input_ui` の入口が `ui.visuals_mut()` で適用する（#949 で `update()` から移設。
        // 機序と順序不変条件の正本はその doc）。**唯一の消費者はその関数が描く `TextEdit` である**
        // ——この view の egui ウィジェットは他に無く、status 行と toast は raw painter へ色を
        // 明示渡しする。results 窓は別 Context ゆえ影響外。
        // font_family のエッジ検出も同一 lock で読む（SU6 spec 決定 2・lock 1 回/フレーム）。
        // 値はフレーム冒頭の `visual` から取る（#673）。

        // SU6 spec 決定 2: font_family hot-reload（WebView2 の --font-family CSS 変数即時反映 parity）。
        // applied は解決成否に依らず無条件更新（フィールド doc 参照）。
        if let Some(name) = &visual.font_family_changed {
            self.applied_font_family = name.clone();
            super::font_stack::configure_japanese_font(&ctx, name);
            ctx.request_repaint(); // set_fonts は次フレーム適用——欠くと新フォントが 1 イベント遅れる
        }

        // ネイティブ背景ブラシの追従はここに無い（spec 決定 3 で撤去）——エッジ検出は変化の
        // 瞬間に居合わせることを要求するが、hidden 中は update() が走らないため居合わせられない。
        // 適用点は show 直前（`show_egui_main` / `ResultsWindow::show`）とサイズ変更時へ移した。

        // 非同期の到着物の回収（起動結果 → 通知期限 → folder 列挙）。**reset_pending 消費の後**
        // に置くこと（spec C 節 不変条件 2）。3 者が同一フレームで走ることも不変条件である。
        self.controller.poll_async(&ctx);

        let pre = read_pre_widget_input(&ctx);

        let restored_search = if pre.escape {
            self.controller.on_escape_pressed(&ctx)
        } else {
            false
        };
        // 段 16–17: blur 猶予。**旧・段 14（focus 復帰で猶予を捨てる）と旧・段 34（フレーム
        // 末尾で focus を畳む）はここへ合流した**（#745）——前フレームとの比較は `BlurGrace`
        // が状態として持つ。段番号は振り直さない（`read_pre_widget_input` の doc が既に
        // 「旧・段 14〜20 相当の位置」と歴史的番号で書いている）。
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
        // **ここから下（TextEdit 構築まで）の検索状態の読みを、この位置より前へ寄せてはならない**
        // （#836）。`update()` の冒頭からここまでには `&mut self.controller` を取る呼び出しが
        // 挟まっており、そのいくつかは `view_kind()` と folder の現在地を書き換える。前寄せすると
        // hint が**遷移前**のディレクトリを描き、「`←` を打った瞬間にラベルが変わる」という
        // #836 の要件（#743 の誤読を防ぐ当のもの）が 1 フレーム遅れて壊れる。
        //
        // **件数は書かない**（数えるたびに変わり、古い数が読者を安心させるため）。**検算は
        // 手続きで行う**: 選んだ位置から TextEdit 構築までの間に `self.controller.` の
        // `&mut` メソッド呼び出しが 1 本も無いことを grep で列挙して確かめる（現在位置では空）。
        //
        // **禁じているのは前寄せだけである**（#870）。hint の**書式化**は下の closure 内へ
        // 移してあるが、それは読んだ値を後ろで使うだけで、読み取り点は動いていない
        // ——遷移前のディレクトリを描く経路は生まれない。
        //
        // **#870 以降、この不変条件のうち folder 現在地の分は借用が構造的に守る**——`HintPlan`
        // が `folder_current_dir()` の `&str` を closure まで持つため、間に
        // `&mut self.controller` を挟むとコンパイルが通らない（ライフタイムは enum の型に
        // 載るので、実行時にどの腕を取るかに依らず領域が生きる）。**grep 検算がなお要るのは
        // `view_kind()` / `is_launching()` / `folder_filter()` のように値をコピーして渡す
        // 読みのほうである**——そちらは借用が残らないので、間に `&mut` を書けてしまう。
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
        // **フォルダ展開中の現在地は「案内」ではなくプレースホルダである**（#836・SPEC §6.7）。
        // 上の #700 の規範（案内の描画面は status 行ただ 1 つ）に抵触しない——status 行が担うのは
        // 「いま何が起きているか」の**お知らせ**（indexing / 起動中 / 一時通知）で、こちらは
        // 「いま入力するとどこが絞り込まれるか」という**入力欄本来の説明**である。現在地の
        // 描画面はこの hint ただ 1 つで、status 行にも results 窓にも出さない。
        //
        // **`indexing_hint()` は名前に反して status 行の文言である**（#700 で移設された際に
        // 関数名だけが残った）。`hint` で grep してここへ辿り着いた編集者が、現在地を
        // `overlay_kind` のラダーへ配線しないこと——それは却下した代替案 B であり、排他ラダー
        // ゆえ indexing 中に現在地が黙って消える（#700 が是正した失敗様態そのもの）。
        //
        // **ここで決めるのは「どれを出すか」だけである**（#870）。書式化と、フォルダ現在地の
        // 幅に合わせた中間省略は下の closure 内で行う——幅と測定器が内側の `ui` にしか
        // 無いためで、**読み取り点はこの位置に据え置かれる**（上の前寄せ禁止がそのまま効く）。
        let hint_plan = if in_tool {
            // SolidJS placeholder.tool_select parity（egui の hint は buf が空のときだけ描かれる＝
            // HTML placeholder と同条件。表示されるのは対象パスが区切り終端等でファイル名が空のとき）
            HintPlan::Tool
        } else if let Some(dir) = self.controller.state().folder_current_dir() {
            // **`in_folder` ではなく `Option` を直接分岐させる。** `in_folder`（= `view_kind()`
            // == `Folder`）で分岐すると「Folder なのに dir が無い」到達不能な else 側を
            // `unwrap_or` 等で埋めることになる。`Option` で分岐すればその腕が**構造的に存在せず**、
            // 到達不能な行を「検出器」に見せかけずに済む。
            //
            // **同値は片側だけである**: `view_kind() == Folder ⟹ folder.is_some()` は成り立つが
            // 逆は成り立たない——tool が folder の上に積まれた状態（`enter_tool` は folder frame を
            // 残す）では `folder.is_some()` かつ `view_kind() == Tool` である（`search_state.rs` の
            // `view_kind` が tool を先に見る。`escape_ladder_tool_then_folder_then_hide` がその
            // 状態を実際に構成している）。**この分岐が正しいのは `in_tool` を先に見ているから
            // であって、同値だからではない。**
            //
            // **溢れたパスは自前で中間省略する**（#870・下の closure 内）。egui へ任せると
            // `hint_text` が `TextWrapMode::Truncate` で**末尾を `…` に**するため
            //（`RichText::new` は単一の text atom ゆえ省略は組み立て後の文字列の末尾に当たる）、
            // 深い階層で**いま居るフォルダ名から削れて、異なる 2 つのディレクトリが同一表示に
            // 潰れる**（#836 のカテゴリ D 実測で第 3 階層と leaf のキャプチャが SHA256 一致）。
            HintPlan::Folder(dir)
        } else {
            HintPlan::Search
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
        //   `RichText::color()` は届かない（#643 の指定は dead だった）。適用は
        //   `search_input_ui` の入口（`ui.visuals_mut()`・#949 で `update()` から移設。
        //   #751 以前は `ctx.set_visuals` で、同じフレームに届かなかった）
        //
        // **どちらも「色リテラルを書かない」だけでは守れない**（#654 で 2 様態とも実在した。
        // SPEC §11 は「指定したつもりで届かない経路」とだけ述べ、機序は本コメントを正本に
        // 指す・#888）。片方を直してもう片方を放置しない。
        let bar_theme = &visual.row;
        let bar_font = egui::FontId::proportional(bar_theme.name_size);
        // 入力欄はバー帯の内側に四辺一様の余白（`Metrics::bar_inset`）を残して置く
        //（#646 PR2・実機目視で追加）。egui の既定配置では上と左が詰まり余りが下だけに
        // 溜まっていた。Frame の inner_margin で四辺の枠を作り、中身の高さを
        // `bar_height - 2*inset` に固定することで帯をちょうど埋める（窓高は bar_height
        // ゆえ、下に取り残しも溢れも出ない）。余白部はドラッグ掴み領域になる（決定 10）。
        let inset = metrics.bar_inset as f32;
        let field_height = (metrics.bar_height as f32 - 2.0 * inset).max(1.0);
        let input_id = ui.make_persistent_id("search_input");
        // **hint の分岐が読む `buf` は、`search_input_ui` が `&mut` で借りるので先に測る。**
        // hint が計算されるのは `TextEdit` の構築より前ゆえ、ここで測った値と同じである。
        let buf_is_empty = buf.is_empty();
        let params = SearchInputParams {
            input_id,
            restored_search,
            window_focused: pre.focused,
            input_editable,
            inset,
            field_height,
            font: bar_font.clone(),
            text_color: bar_theme.name_color,
        };
        // **テーマ 3 値の適用点はこの下——`search_input_ui` の入口である**（#949 で移設）。
        // **ここより前で新しいウィジェット・子 `Ui` を作るなら、visuals を読まないことを
        // 確かめるか、その `Ui` へ自分で visuals を渡すこと。** `update()` の冒頭からここまでは
        // 3 値が未適用の区間であり、適用が `update()` の前半に在った #751 当時より**広い**。
        // **この区間の退行を捕まえる自動検査は無い**——移設で入った検査は `search_input_ui` を
        // 単独で駆動するので、ここには届かない。残るのは**カテゴリ D の非既定色目視**だけである
        //（受容する残余・`ADR-visuals-order-detector-at-choke-point`）。現時点でこの区間に visuals を読む描画は
        // 無い: `ui.interact` は `create_widget` を呼ぶが（`ui.rs:906` → `:920`）ヒットテストの
        // 矩形を積むだけで visuals を読まず、status 行と toast は raw painter へ色を明示渡しする。
        let input_visuals = InputVisuals {
            input_bg: visual.input_bg,
            selection: visual.selection,
            hint: visual.hint,
        };
        let response = search_input_ui(ui, input_visuals, &mut buf, &params, |ui| {
            // hint の書式化（#870）。**フォルダ現在地だけが幅を要る**——収まらないパスを
            // 中間省略し、ドライブと leaf の両方を残す。`add_sized` に渡すのと同じ
            // `ui.available_width()` から `TextEdit` の左右 margin を引いたものが、
            // egui が hint を elide する内幅である（`TEXT_EDIT_HINT_H_MARGIN` の doc）。
            //
            // 測定は「候補を書式へ埋めた文字列の実幅」を返す形で注入する。**固定部
            //（日本語の接尾辞・英語の接頭辞 + 接尾辞）の幅を別に推定しなくて済む**のが
            // 要点で、省略は `dir` にだけ当たるため接尾辞は必ず残る。
            //
            // **測る font と描く font は同じ `bar_font` でなければならない。** 片方だけ
            // 替えると、広く測れば egui が末尾を削って `…` が二重に付き（leaf が消える）、
            // 狭く測れば要らぬ省略が入る——**どちらも検出器を持たない**（型でも
            // テストでも捕まらず、カテゴリ D の目視だけが見る受容残余である）。
            // 色（`name_color`）は galley の寸法に効かないので、描画側の
            // `weak_text_color`（egui が無条件に上書きする）との食い違いは無害である。
            let hint: String = match hint_plan {
                HintPlan::Tool => crate::egui_shell::ui_strings::tool_select_hint(l).to_string(),
                HintPlan::Search => crate::egui_shell::ui_strings::search_hint(l).to_string(),
                // **egui 自身の描画条件と同じ述語でガードする**: `hint_text` はバッファが
                // 空のときだけ描かれる（`builder.rs:592`）。フォルダ展開中の `buf` は
                // `folder_filter()` なので、1 文字でも絞り込むと hint は描かれない——
                // その間まで測ると `folder_hint` の String 確保と `layout_no_wrap` が
                // 毎フレーム最大 9 回ずつ空回りする。描かれない文字列の中身は観測されない
                // ので、素通しでも見え方は変わらない。
                HintPlan::Folder(dir) if !buf_is_empty => {
                    crate::egui_shell::ui_strings::folder_hint(l, dir)
                }
                HintPlan::Folder(dir) => {
                    let avail = (ui.available_width() - TEXT_EDIT_HINT_H_MARGIN).max(0.0);
                    let shown =
                        crate::egui_shell::layout::fit_middle_by_measure(dir, avail, |cand| {
                            let text = crate::egui_shell::ui_strings::folder_hint(l, cand);
                            ui.painter()
                                .layout_no_wrap(text, bar_font.clone(), bar_theme.name_color)
                                .size()
                                .x
                        });
                    crate::egui_shell::ui_strings::folder_hint(l, &shown)
                }
            };
            hint
        });
        if response.changed() {
            self.controller.on_input_changed(buf, in_folder, &ctx);
        }
        // **かつてここに「窓に focus があるのに入力欄が持たないなら移す」があった**（#872/#936
        // で TextEdit の構築前へ移設）。ここに置くと、そのフレームに載っていた文字は既に
        // 捨てられた後であり、効くのは次のフレームからだった。**移設は挙動を 1 フレーム
        // 早めるだけで、回復の速さは変わらない**——このフレームで焦点を失った場合（Escape 等）、
        // 旧: このフレームの末尾で要求 → 次フレームの widget が持つ / 新: 次フレームの構築前に
        // 要求 → 同じ widget が持つ、で一致する。**2 か所で要求しない**（#700 と同じ理由）。
        // **show ごとに最初の数フレームだけ、入力欄が打鍵を受け取れる状態だったかを残す**
        // （#872/#936）。**`response.has_focus()` は widget が走った時点の値**なので、
        // 「そのフレームの文字イベントを受け取れたか」をそのまま言う——上の移設が効いていれば
        // 最初のフレームから真であり、**この行は移設の回帰検出器になる**（偽に戻れば、
        // 起動直後の打鍵が再び捨てられている）。
        //
        // **`input_editable` を併記する**——両者は同じ沈黙を作るため、片方だけを見ると
        // 当たっていなくても説明が通ってしまう（実測でこちらは初回から真と判った）。
        if self.focus_state_traces_left > 0 {
            self.focus_state_traces_left -= 1;
            crate::trace_main(
                "egui_input:focus_state",
                serde_json::json!({
                    "window_focused": pre.focused,
                    "input_editable": input_editable,
                    "has_focus": response.has_focus(),
                    "in_tool": in_tool,
                    "launching": self.controller.is_launching(),
                }),
            );
        }

        // status 行（#532 SU5 の一時 overlay・#700 で位置を変更）: 「起動中…」/ 失敗・結果不明通知/
        // indexing 案内を**検索バーの直下に独立した行として**描く。かつてはこれを TextEdit の
        // rect への重ね描きで賄っていた（#700 で撤回・下のブロック参照）。
        // 優先順は WebView2 SearchWindow.tsx の Switch 先頭一致 parity: indexing > 起動中 > 通知。
        //
        // **クエリの空/非空では切り替えない**（#700）。「空クエリのときは hint が描く」という
        // 2 面構成は撤回済みであり、`notify::overlay_kind` は空/非空を入力に取らない
        // （`indexing_overlay_does_not_depend_on_query_emptiness` が固定）。**`indexing_hint()` は
        // 名前に反してこの status 行の文言であって、TextEdit の hint_text へは一度も渡らない**
        // （上の hint 構築部を参照）。
        // 4 入力を 1 度だけ読み、`overlay_kind`（文言の導出）と `status_row_present`（行の
        // 有無・下）の両方へ**同じローカル**を渡す（レビュー是正 2）。`indexing` はスレッドを
        // またぐ `AtomicBool` のため、独立に 2 回読むと index build スレッドの
        // `finish_index_build()` が 2 つの読みの間に割り込み、status 行を描いたフレームが
        // バー高だけの `set_size` を撃つ（案内が切り取られる）——この diff の前は
        // `has_status = overlay_text.is_some()` で一致が構造的に保証されていた。
        //
        // **`indexing_raw` はこのフレームで `indexing` を読む唯一の点である**（#1077 で
        // 射程が status 行の外まで広がった）。**配り先は数えない**——数えれば足すたびにこの行が
        // 腐る。正本は `indexing_raw` の参照そのもの（`launcher_controller.rs` の
        // `activation_uses_frame_values_not_live_reads` が、起動側で読み直しが
        // 復活しないことを固定する）。**唯一でないものが 1 つある**: `run_search_with` の
        // `indexing` 読みは用途が違い（行をクリアするか）、到達経路ごとにその時点で判断するのが
        // 正しいので live のまま残してある。
        let indexing = self.controller.indexing();
        let indexing_raw = indexing.get();
        // **連言④（`SPEC.md`「4.5 最大列挙数」）の値もこのフレームで 1 回だけ読む**（#1106）。
        // `indexing` と理由は同じだが機構が違う——こちらは `AtomicBool` ではなく config の
        // live-read で、変わる契機は `config_watcher` の適用である。読みが 2 つあると
        // 「表示は隠し、起動は通す」並びが同一フレーム内に構築できる（実機で測った症状）。
        // **配り先は数えない**（`indexing` と同じ理由——上の行）。要点は表示側と起動側が
        // 同じこの値を見ることであって、いま何か所へ渡っているかではない。
        //
        // **受容する残余**（`indexing` の (1) と同型）: 凍結ゆえ、`config_watcher` がこの
        // フレームの途中で適用した新しい値は次フレームまで効かない（最大 1 フレーム古い）。
        // **表示と起動が同じ値を見ること**がこの凍結の目的であり、遅れは `config-applied` の
        // wake が起こす次フレームが回復する（`SPEC.md`「4.7 結果表示制御（2 窓構成）」の反映機構）。
        let visible_rows = super::window_coordinator::read_visible_rows(&app);
        let is_results = self.controller.state().view_kind() == ViewKind::Results;
        let launching_now = self.controller.is_launching();
        let notice_now = self.controller.notice_message().map(|m| m.to_string());
        let has_notice_now = notice_now.is_some();
        let overlay_text: Option<String> = match crate::egui_shell::overlay_kind(
            indexing_raw && is_results,
            launching_now,
            has_notice_now,
        ) {
            Some(crate::egui_shell::OverlayKind::Indexing) => Some(
                crate::egui_shell::ui_strings::indexing_hint(self.controller.lang()).to_string(),
            ),
            Some(crate::egui_shell::OverlayKind::Launching) => {
                Some(crate::egui_shell::ui_strings::launching(self.controller.lang()).to_string())
            }
            Some(crate::egui_shell::OverlayKind::Notice) => notice_now,
            None => None,
        };
        // #700 発見 C: **入力欄に重ねず、バー直下の独立した行へ描く。** 以前は
        // `response.rect` を不透明に塗り潰していたため、入力欄は編集可能なまま
        // 「打った文字が見えない」状態になり、実際に「検索ワードを編集できない」と
        // 報告された。launching 中は入力欄が非対話（`input_editable`）で整合していたが、
        // indexing（数分に及びうる）と notice（数秒）は編集可能なまま覆われていた。
        // 行の高さは toast と同じ `metrics.toast_height`（= bar_height・#646 決定 2）で、
        // 窓高は `main_window_height` の `status_height` が積む。
        // **`overlay_text.is_some()` と同値である**（上で 1 度だけ読んだ同じ 4 入力を
        // 同じローカルとして `overlay_kind` / 本関数の両方へ通すため——読み直した入力では
        // ない・レビュー是正 2）。それでも述語を経由するのは、**show 経路が同じ関数を呼ぶ**
        // からである（`window_coordinator::show_egui_main`）。共有の実体の正本は
        // `src-tauri/CLAUDE.md`「モジュール構成」の `window_coordinator.rs` の項（#755 / #801）。
        let has_status = crate::egui_shell::status_row_present(
            indexing_raw,
            is_results,
            launching_now,
            has_notice_now,
        );
        if let Some(text) = overlay_text {
            let status_h = metrics.toast_height as f32;
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), status_h),
                egui::Sense::hover(),
            );
            // 色はフレーム冒頭の `visual` から（#673）。**ここは style を読まない**——config の値を
            // painter へ直接渡すので、3 値の適用との前後関係に意味は無い（監査 #4。#751 で適用先が
            // ctx から ui へ、#949 で `update()` から `search_input_ui` へ移った後も無関係のまま）。
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
            if draw_toast_button(
                ui,
                &mut cursor_x,
                btn_y,
                dismiss_label,
                row.buttons_enabled,
                theme,
            ) {
                toast_action = Some(ToastAction::Dismiss);
            }
            if row.show_install {
                let install_label = crate::egui_shell::ui_strings::update_install_now(l);
                if draw_toast_button(
                    ui,
                    &mut cursor_x,
                    btn_y,
                    install_label,
                    row.buttons_enabled,
                    theme,
                ) {
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

        // worker の結果を採り込む（#1004）。**行の差し替えはクリック消費より前でなければならない**（#699）。`poll_search_debounce` より前に置くのは、同じフレームで trailing 発火が新しい要求を出す前に、届いた結果を採るためである。
        self.controller.drain_search();

        // trailing debounce の poll と再 arm（armed の間は毎フレーム残余を要求し直す）。
        self.controller.poll_search_debounce(&ctx);

        // Enter: 選択項目を起動/実行（Shift は §18.3 のツール選択入場・後置 dispatch は M3 のまま）。
        // TextEdit の changed 処理より後で読む理由は `read_post_widget_input` の doc 参照。
        let post = read_post_widget_input(&ctx);
        if post.enter {
            // `indexing` は上で 1 回だけ読んだ `indexing_raw` を渡す（#1077）——起動の判定と
            // 下の表示ゲートが**同一フレームで同じ値**を見ることが受け入れ条件である。
            self.controller
                .on_enter(post.shift, indexing, visible_rows, &ctx);
        }

        // 結果リスト（shouldShowResults 相当）。§4.7: 再インデックス中は plain 結果のみ隠す
        // （instant/folder/tool carve-out・SU6 spec 決定 3）。データと選択は保持——クリアしない
        // （SolidJS parity: setIndexing は結果を触らず派生 memo が非表示を担う）。indexing 中の
        // 案内は status 行が担い（#700 で一本化・空/非空で切り替えない）、show_results=false
        // では results 窓が hide される（main は bar(+toast)固定高で伸縮しない・#646 PR2 決定 6）。
        // 連言③は**1 フレーム 1 回だけ**読む（#752 F2）。`indexing` は `AtomicBool` の live-read で
        // 同一フレーム内でも変わりうるため、pre/post で 2 回読むと連言③がフレーム内で食い違う。
        // ここで得た値を snapshot 用と `drive_results_window` の両方へ配る。
        //
        // **`indexing` は上の `indexing_raw` を使う**（#1077 でここの読み直しを止めた）。同じ値が
        // status 行・この表示ゲート・`on_enter` の起動判定とクリック逆流へ配られ、
        // 「画面には出ていないが Enter は起動する」の類が構築できなくなる。**凍結してよいのは
        // `indexing` だけである**——`view_kind` と `instant_rows_query` はここで読むのが正しい
        // （`on_enter` が Tool ビューへ入る等、この行より前で正当に変わる）。
        //
        // **受容する残余が 2 つある。** (1) この値は `indexing_raw` を読んだ時点のもので、
        // 表示ゲートとしては最大 1 フレーム古い——`on_enter` の同期 `engine.search` は engine lock を
        // 40〜95 ms 握る（#1032 実測）ので、その間に立つ余地がある。帰結は results 窓が隠れるのが
        // 1 フレーム遅れることだけで、**起動と表示は同じ値を見たまま**である。(2)
        // `run_search_with` の `indexing` 読みは live のままである（用途が違う——行をクリアするか。
        // **到達経路は数えない**——凍結より前に走るものも後に走るものも在り、足すたびに腐る）。
        // 食い違うと「Enter が 1 フレーム飲まれる」か「行が空で何も起きない」になり、どちらも
        // 次フレームの再検索が回復する。
        let plain_hidden = crate::egui_shell::plain_results_hidden(
            self.controller.state().view_kind(),
            self.controller.instant_rows_query().is_some(),
            indexing_raw,
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
            let input_idle = !self.controller.is_search_armed();
            {
                let mut guard = shared.snapshot.lock().unwrap();
                if !guard.matches(
                    rows,
                    selected,
                    self.controller.state().rows_generation(),
                    input_idle,
                ) {
                    *guard = crate::egui_shell::RowsSnapshot {
                        rows: rows.to_vec(),
                        selected,
                        generation: self.controller.state().rows_generation(),
                        input_idle,
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
                    // クリックも Enter と同じ `indexing` / `visible_rows` を見る（#1077 / #1106）
                    // ——`rows_generation` の照合は「行が差し替わったか」だけを見ており、
                    // **その行が画面に出ているか**は見ない。判定は `activate_or_execute` の中の
                    // 2 つの表示ゲート（連言③と④）が持つ。
                    self.controller
                        .activate_or_execute(i, indexing, visible_rows, &ctx)
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

        // #646 PR2 決定 6: main は bar(+status/toast)のみで結果件数には伸縮しない。結果窓の可視性・サイズ・位置も
        // ここ(毎フレーム走る main)が駆動する——hidden 窓は update() が走らず自分では
        // show できない(SU5 要石)。位置 → サイズ → show の順(main の show と同じ制約)。
        let height = crate::egui_shell::layout::main_window_height(
            metrics.bar_height,
            has_status.then_some(metrics.toast_height),
            has_toast.then_some(metrics.toast_height),
        );
        // 不変条件検出器（レビュー是正 4）: 仕様は「高さは『いま描く行』で決まり、高さの変化は
        // 行の出没と 1 対 1 で対応する。行が変わっていないのに高さが変わったら欠陥である。
        // 行が変わったなら通知が届いた——正常」。reset-on-show の memo リセット（fail-safe）は
        // show 側の導出が退行しても最初のフレームで動的高さ算出が直すため、外から観測する
        // smoke の高さ断言を無力化する（過渡は 1 フレーム=約16ms、`Wait-SnotraWindow` は
        // 200ms ポーリング + 100ms 間隔では捕まらない）。ここは in-process で
        // 「起きてはならないことが起きていないか」を突き合わせる
        // （`src-tauri/CLAUDE.md`「モジュール構成」の trace 規範）。
        //
        // 発火条件は 4 つの連言——1 つでも崩れていれば「（indexing/toast が変わった、または
        // launching/notice が新たに立った）通知が届いただけ」で正常なので何も出さない。
        if was_reset_frame
            && let Some(sh) = app.try_state::<crate::egui_shell::EguiShellState>()
            && sh
                .show_read_indexing
                .load(std::sync::atomic::Ordering::SeqCst)
                == indexing_raw
            && sh.show_read_toast.load(std::sync::atomic::Ordering::SeqCst) == has_toast
            && !launching_now
            && !has_notice_now
        {
            let show_h = f64::from_bits(
                sh.show_applied_height_bits
                    .load(std::sync::atomic::Ordering::SeqCst),
            );
            if (show_h - height).abs() > 0.5 {
                crate::trace_main(
                    "egui_main:height_mismatch",
                    serde_json::json!({
                        "show_h": show_h,
                        "frame_h": height,
                        "indexing": indexing_raw,
                        "toast": has_toast,
                    }),
                );
            }
        }
        let width = self.window_width();
        // 判定式の正本は `layout::size_delta_exceeds`（#749）。results 側と**式だけを共有し、
        // memo は共有しない**（ADR-results-presentation-two-stage 却下 1: `main_size` を results の
        // 導出へ入れない）。高さの導出が show 経路と共有される事実の正本は
        // `src-tauri/CLAUDE.md`「モジュール構成」の `window_coordinator.rs` の項（#755 / #801）。
        // 共有するのは導出であって memo ではない。
        if crate::egui_shell::layout::size_delta_exceeds(
            (self.last_set_width, self.last_set_height),
            (width, height),
        ) {
            self.last_set_height = height;
            self.last_set_width = width;
            if let Some(window) = app.get_window("main") {
                let _ = window.set_size(tauri::LogicalSize::new(width, height));
                // **リサイズでも下地が露出する**（SU6 spec 決定 2 の codex 反証。show の一瞬だけ
                // ではない）。ゆえに show 直前（spec 決定 3）に加えてここでも合わせる。
                // **色が変わったときだけ撃つ**——サイズのデルタガードの内側ではあるが、それは
                // 件数変化のたび＝ほぼ打鍵ごとに開く。同じ色を撃ち直しても得るものは無く、
                // `InvalidateRect(erase=true)` + `UpdateWindow` の代価だけがかかる。
                if self.applied_background != Some(visual.background) {
                    self.applied_background = Some(visual.background);
                    super::window_coordinator::apply_native_background(&window, visual.background);
                }
            }
            ui.ctx().request_repaint();
        }
        // #738: バー矩形を作業領域の内側へ戻す。**ポインタが押されていないフレームだけ**である
        // ——ドラッグ中も戻すと横並びモニター間の移動が封鎖される（機序と作業例、および
        // `was_reset_frame` を OR で足す backstop を実測で却下した経緯は
        // `clamp_main_into_work_area` の doc と `ADR-main-window-clamp-on-pointer-release`）。
        //
        // **呼び出し側にしか無い制約はこちら**: **`drive_results_window` より前**でなければ
        // ならない。`position_results_below_main` は main の位置を OS から読み直すため、後ろへ
        // 置くと results が 1 フレームだけクランプ前の位置へ追従する。`set_position` は
        // `SetWindowPos` を同期で撃つので、ここで戻せば直後の drive は新しい位置を読む
        // （`Moved` イベントの配送を待たない——それは `update()` の終了後になる）。
        if !ui.input(|i| i.pointer.any_down()) {
            crate::egui_shell::clamp_main_into_work_area(&app, metrics.bar_height);
        }
        // **`result_count` はここで読む**（#749）——`take_clicked_for`（クリック逆流の消費・
        // 上のブロック）より**後**でなければならない（#752 F2 / ADR-results-presentation-two-stage）。この式を
        // `plain_hidden` の算出（`show_results` の直前）へ動かすと、行クリック起動フレームで
        // 古い行が 1 フレーム描かれる。`cargo test` では落ちない種類の回帰である。
        crate::egui_shell::drive_results_window(
            &app,
            frame.event_loop(),
            crate::egui_shell::DriveResultsInputs {
                plain_hidden,
                result_count: self.controller.state().results().len(),
                width,
                row_height: metrics.row_height,
                // 起動の入口へ配ったのと**同じ 1 回の読み**である（#1106・同フィールドの doc）
                visible_rows,
                // `row_height` と同じフレーム冒頭の snapshot から取る（別 lock にしない）
                background: visual.background,
            },
        );

        crate::trace::trace(
            "egui_frame",
            serde_json::json!({
                "update_us": frame_started.elapsed().as_micros() as u64,
                "interval_us": frame_interval.map(|d| d.as_micros() as u64),
            }),
        );
    }
}

#[cfg(test)]
mod tests {
    use egui::text::{CCursor, CCursorRange};
    use egui::text_edit::TextEditState;

    use super::{InputVisuals, SearchInputParams, move_text_cursor_to_end, search_input_ui};

    /// `indexing` をこのファイルが読むのは**フレームで 1 回だけ**であることを固定する（#1077）。
    ///
    /// `AppState.indexing` は `AtomicBool` の live-read で、同一フレーム内でも index build
    /// スレッドが値を変えうる。独立に 2 回読むと、status 行・表示ゲート・起動判定のうち
    /// **どの 2 つかが同じフレームで食い違う**——#752 F2 が status 行と窓高について踏み、
    /// #1077 が表示ゲートと Enter の起動判定について踏んだ。どちらも「片方だけ古い値で
    /// 描く／判断する」形で、**挙動テストは通り抜ける**（行は正しく出る）。
    ///
    /// [`crate::egui_shell::FrameIndexing`] の構築子は `window_coordinator` に閉じているので、
    /// **偽の値を作る**書き方はコンパイルが塞ぐ。この検査が塞ぐのは残りの一手——
    /// **本物をもう 1 回読む**ことである。
    ///
    /// **残る死角**: 母集団はこのファイルのソーステキストだけである。読みを別のヘルパーへ
    /// 移すと母集団の外になる。
    #[test]
    fn indexing_is_read_exactly_once_per_frame() {
        assert_read_once_in_this_file(
            // **リテラルを割って組み立てる**——このソースに綴りが逐語で現れないようにするため
            // であり、それが母集団をファイル全体にできる理由である（#1112。理由の正本は
            // [`assert_read_once_in_this_file`]）。
            concat!(".controller.", "indexing()"),
            "1 回だけ読み、FrameIndexing のまま配ること（#752 F2 / #1077）",
        );
    }

    /// 連言④の値（`visible_rows`）をこのファイルが読むのは**フレームで 1 回だけ**である
    /// ことを固定する（#1106）。上の `indexing` の検査と**別に置く**——射程が違うものを
    /// 1 つの名前へ束ねると、名前と実体がずれる。
    ///
    /// **こちらは唯一性が構造でも支えられている**——`window_coordinator::drive_results_window`
    /// の内側にあった直読み（`max_results(app)`）を撤去し、`read_visible_rows` の呼び出し点を
    /// このファイルの 1 か所にした。ゆえにこの検査が塞ぐのは「**view.rs の中で**もう 1 回読む」
    /// 形だけであり、他モジュールに読みが復活する形は母集団の外である（`fn max_results` の doc が
    /// 呼び出し点の唯一性を規範として持つ）。
    #[test]
    fn visible_rows_is_read_exactly_once_per_frame() {
        assert_read_once_in_this_file(
            // 上と同じ理由で割る（#1112）。
            concat!("read_visible", "_rows("),
            "1 回だけ読み、FrameVisibleRows のまま表示側と起動側の両方へ配ること（#1106）",
        );
    }

    /// 上の 2 検査が共有する骨格——**このファイル全体で** `needle` がちょうど 1 回現れる
    /// ことを測る。
    ///
    /// **母集団を切り出さないのが要点である**（#1112）。`assert_eq!(reads, 1)` は
    /// 「≧ 1」と「≦ 1」の連言であり、後半は否定形——**禁止語の不在を測る形**である。
    /// 否定形の検査は母集団が本体を取りこぼすと**緑のまま沈黙する**（canary は「母集団が
    /// 空でない」しか塞がず、「途中で切れる」を塞がない）。かつてはここが
    /// `split_once("#[cfg(test)]")` で production 側を切り出しており、その切れ目が前へ
    /// 動けば 2 回目の読みが母集団の外へ落ちて黙って通った。ファイル全体を数えれば
    /// 切り詰めが起こりえない。
    ///
    /// **切り出していた理由は呼び出し側で解いた**——検査は `needle` のリテラルを自分の
    /// ソースへ書くため、ファイル全体を数えると自分を勘定に入れて必ず 2 になっていた。
    /// 呼び出し側が `concat!` でリテラルを割って組み立てるので、綴りはこのファイルに
    /// 逐語で現れず、production の 1 回だけが数えられる。
    ///
    /// **これは `literal-grep-misses-constructed-strings` の失敗類型へ意図して踏み込んで
    /// いる**——「リテラルを狙った探索は組み立てられた値を落とす」形そのものである。
    /// 根拠は**生成点が 1 か所であること**: `needle` を組むのは上の 2 つの呼び出しだけで、
    /// どちらもこの関数へ直に渡す。ゆえに「綴りが現れない」ことを保つ責任は同じ画面の
    /// 中に閉じている。
    ///
    /// **`assert_eq!(reads, 1)` の 2 側で失敗方向が違う**（#1112 のレビューで射程を訂正した）:
    ///
    /// - **≦ 1 側**（読みが増える）は安全側である。doc コメント等に綴りが現れれば過剰計数で
    ///   **赤**になる（沈黙しない）
    /// - **≧ 1 側**（production の読みが消える）は**この検査だけでは担保されない**。母集団が
    ///   ファイル全体なので、production 0 と、このファイルのどこか（doc コメント・`concat!` を
    ///   解いたリテラル）に現れた綴り 1 件が釣り合えば緑が成立する。切り出していた頃は
    ///   production 側だけを数えたので、この釣り合いは起こりえなかった——**ここは新設計が
    ///   引き受けた交換である**。**その組み合わせは 2026-08-17 時点では成立しない**:
    ///   production は各 1 件で、呼び出し側は `concat!` で割ってある
    ///
    /// **≧ 1 側をいま支えているのは rustc である。** `update()` は読みの結果を `indexing` /
    /// `visible_rows` へ束縛し、[`crate::egui_shell::FrameIndexing`] /
    /// [`crate::egui_shell::FrameVisibleRows`] のまま表示側と起動側へ渡す。読みを消せば束縛が
    /// 未定義になってコンパイルが通らない。**この支えは束縛の形に依っている**——値を別経路で
    /// 作れるようにする、消費点ごと落とす、といった変更で外れる（構築子が `window_coordinator`
    /// に閉じていることが、いま前者を難しくしている）。
    fn assert_read_once_in_this_file(needle: &str, lost: &str) {
        let src = include_str!("view.rs");
        let reads = src.matches(needle).count();
        assert_eq!(
            reads, 1,
            "view.rs が `{needle}` を {reads} 回書いている。{lost}"
        );
    }

    /// 検査用のテーマ 3 値。**3 値とも非既定色にする**——既定色と偶然一致すると、適用が
    /// 落ちていても通ってしまう。
    fn test_visuals() -> InputVisuals {
        InputVisuals {
            input_bg: egui::Color32::from_rgb(0x80, 0x30, 0x20),
            selection: egui::Color32::from_rgb(0x20, 0x70, 0x40),
            hint: egui::Color32::from_rgb(0x10, 0x20, 0xF0),
        }
    }

    /// テストで 1 pass 走らせ、**`textures_delta` を消費してから `FullOutput` を返す**。
    ///
    /// epaint 0.36 の `TexturesDelta` は未適用の delta を持ったまま drop されると `Drop` の
    /// `debug_assert!` で落ちる。**製品の消費経路は `snotra-egui-runtime` の `renderer.rs`**
    /// であり、テストはそこを通さない（`Context` だけを回して shapes や状態を見る）ので、
    /// 捨てることをここで明示する。**`ctx.run_ui` を直に呼ばず必ずこれを通すこと**——
    /// 直呼びは egui のフォントアトラスが更新されたフレームでだけ落ち、**入力に依存して
    /// 落ちたり落ちなかったりする**。
    fn run_pass(
        ctx: &egui::Context,
        input: egui::RawInput,
        f: impl FnMut(&mut egui::Ui),
    ) -> egui::FullOutput {
        let mut out = ctx.run_ui(input, f);
        out.textures_delta.clear();
        out
    }

    /// IME 変換中のフレームを描き、**水平な**線分を `(x0, x1, 不透明か)` で集める。
    ///
    /// **向きで選ぶ**——キャレットも同じ色・太さの `LineSegment` を描くが、あちらは垂直である。
    /// 色や太さで分けると `Visuals` の既定値に依存するが、向きは表示方式そのものに属する。
    ///
    /// **不透明かどうかが「変換対象の節か」を表す**。egui は `inactive_underline_stroke` を
    /// `active` の `linear_multiply(0.5)` として作るため、未確定の残りは半透明で描かれる。
    ///
    /// **preedit を載せた次のフレームを見る**——`TextEdit` がその内容を描くのは 1 フレーム後で、
    /// 同じフレームの `shapes` にはまだ現れない（実測）。
    fn preedit_underlines(active: std::ops::Range<usize>) -> Vec<(f32, f32, bool)> {
        fn walk(shape: &egui::Shape, out: &mut Vec<(f32, f32, bool)>) {
            match shape {
                egui::Shape::LineSegment { points, stroke }
                    if (points[0].y - points[1].y).abs() < f32::EPSILON =>
                {
                    out.push((points[0].x, points[1].x, stroke.color.a() == 255));
                }
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }

        let params = SearchInputParams {
            input_id: egui::Id::new("search_input"),
            restored_search: false,
            window_focused: true,
            input_editable: true,
            inset: 7.0,
            field_height: 29.0,
            font: egui::FontId::proportional(15.0),
            text_color: egui::Color32::WHITE,
        };
        let ctx = egui::Context::default();
        let mut buf = String::new();
        let mut found = Vec::new();
        for frame in 0..4 {
            let events = if frame == 2 {
                vec![egui::Event::Ime(egui::ImeEvent::Preedit {
                    text: "へんかんちゅう".to_owned(),
                    active_range_chars: Some(active.clone()),
                })]
            } else {
                vec![]
            };
            let out = run_pass(
                &ctx,
                egui::RawInput {
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    let _ =
                        search_input_ui(ui, test_visuals(), &mut buf, &params, |_| String::new());
                },
            );
            if frame == 3 {
                for clipped in &out.shapes {
                    walk(&clipped.shape, &mut found);
                }
            }
        }
        found
    }

    /// IME 変換中は**選択帯ではなく下線**で描かれ、**変換対象の節が濃い下線で示される**。
    ///
    /// `search_input_ui` の入口で `ime_composition.legacy_visuals` を偽へ倒していることが前提で、
    /// **戻すと下線は 1 本も引かれない**（egui は `cursor_purpose` を `Selection` に固定して
    /// 選択帯で描く）——変異注入で実測した。egui はこの値を **Windows で既定 `true`** にするため、
    /// **バージョンを上げた拍子に既定へ戻る類の退行**がありうる。この検査がその検知点である。
    ///
    /// **同時に `snotra-egui-runtime` の `ime::active_range_chars` の出口を縛る。** あちらの
    /// テストは「IMM32 の属性配列 → 文字範囲」の変換だけを測り、**その値が描画へ届くかは見ない**
    /// ——実際、`legacy_visuals` が既定のままだった間、あの計算は一度も画面に現れていなかった。
    #[test]
    fn ime_preedit_paints_underlines_and_marks_the_active_clause() {
        let head = preedit_underlines(0..3);
        let tail = preedit_underlines(4..7);

        for (label, lines) in [("先頭", &head), ("末尾", &tail)] {
            assert!(
                !lines.is_empty(),
                "{label}: 下線が 1 本も引かれていない（`legacy_visuals` が真に戻ると選択帯で描かれる）"
            );
            assert!(
                lines.iter().any(|&(_, _, opaque)| opaque),
                "{label}: 変換対象を示す濃い下線が無い（active_range が届いていない）"
            );
        }

        let active_x = |lines: &[(f32, f32, bool)]| {
            lines
                .iter()
                .find(|&&(_, _, opaque)| opaque)
                .expect("濃い下線は上の assert で存在を確かめてある")
                .0
        };
        let (head_x, tail_x) = (active_x(&head), active_x(&tail));
        assert!(
            head_x < tail_x,
            "変換対象の下線が `active_range_chars` に追随していない（先頭指定 x={head_x} / 末尾指定 x={tail_x}）"
        );
    }

    // `hex_color_parses_and_falls_back` は #673 で `visual.rs` の
    // `hex_parses_valid_and_falls_back_to_config_default` へ移した（hex→Color32 の変換が
    // view から純粋核へ移ったため）。**証明していた命題は 2 つとも移設先で保たれている**
    // ——妥当な hex が期待どおりの色になること、不正文字列が fallback へ落ちること。
    //
    // font_definitions_* 4 件・font_covers_cjk_* 3 件は #666 段 3 タスク 1 で
    // `font_stack::tests` へ移した（フォント解決の実体が `font_stack` へ移設されたため）。

    // CCursor は文字単位であり、UTF-8 のバイト長を渡すと非 ASCII クエリで末尾を越える。
    #[test]
    fn restored_search_cursor_uses_character_count() {
        let ctx = egui::Context::default();
        let id = egui::Id::new("restored_non_ascii_search_input");
        let mut state = TextEditState::default();
        state
            .cursor
            .set_char_range(Some(CCursorRange::one(CCursor::new(1))));
        egui::TextEdit::store_state(&ctx, id, state);

        move_text_cursor_to_end(&ctx, id, "日本語");

        let restored = egui::TextEdit::load_state(&ctx, id).expect("stored TextEdit state");
        assert_eq!(
            restored.cursor.char_range(),
            Some(CCursorRange::one(CCursor::new(3)))
        );
    }

    // 回帰テスト: #840 の可視症状を、次の Event::Text が作る文字列まで通して検査する。
    #[test]
    fn restored_search_inserts_next_input_at_query_end() {
        let ctx = egui::Context::default();
        let id = egui::Id::new("restored_search_input_event");
        let mut state = TextEditState::default();
        state
            .cursor
            .set_char_range(Some(CCursorRange::one(CCursor::new(2))));
        egui::TextEdit::store_state(&ctx, id, state);
        ctx.memory_mut(|memory| memory.request_focus(id));

        let mut text = "alpha".to_string();
        move_text_cursor_to_end(&ctx, id, &text);
        let input = egui::RawInput {
            events: vec![egui::Event::Text("z".to_string())],
            ..Default::default()
        };
        run_pass(&ctx, input, |ui| {
            ui.add(egui::TextEdit::singleline(&mut text).id(id));
        });

        assert_eq!(text, "alphaz");
    }

    /// 回帰テスト: #872/#936。**focus の要求が `TextEdit` の構築より後だと、そのフレームに
    /// 載っていた文字は捨てられる。**
    ///
    /// `update()` はかつて widget を追加した**後**に `response.request_focus()` を撃っており、
    /// プロセス起動後の最初のフレーム（`has_focus=false`）に届いた文字が丸ごと消えていた。
    /// ローカルではその窓が 50ms しか開かないが、CI runner では 1.4〜19 秒開き、#872 の
    /// 間欠失敗（失敗率 12.5%・7 か月）の正体がこれだった。
    ///
    /// **両方の並びを 1 フレームずつ走らせて差まで測る**——「構築前なら入る」だけでは、
    /// 後ろに戻したときに落ちる保証にならない（当時の並びが本当に落とすことを固定する）。
    ///
    /// ⚠️ **これが縛るのは egui の意味論であって `update()` の並びではない。** 本体側の
    /// 呼び出し位置が後ろへ戻っても、このテストは通る。**この受容残余は #872 で閉じた**——
    /// 経緯と、どの検査が何を縛るかは `search_input_ui` の doc が正本。
    #[test]
    fn focus_requested_before_text_edit_applies_same_frame_input() {
        // 文字の届き方は本体と同じ経路にする——runtime は WM_CHAR / IME 確定を
        // `Event::Ime(Commit)` として渡す（`snotra-egui-runtime/src/input.rs`）。
        let typed = || egui::RawInput {
            events: vec![egui::Event::Ime(egui::ImeEvent::Commit("a".to_owned()))],
            ..Default::default()
        };

        // 現在の並び: 構築より前に要求する。
        let before = egui::Context::default();
        let before_id = egui::Id::new("focus_before_widget");
        let mut before_text = String::new();
        run_pass(&before, typed(), |ui| {
            ui.ctx().memory_mut(|m| m.request_focus(before_id));
            ui.add(egui::TextEdit::singleline(&mut before_text).id(before_id));
        });
        assert_eq!(
            before_text, "a",
            "構築前に focus を要求すれば、同じフレームの文字が入る"
        );

        // かつての並び: 構築より後に要求する。
        let after = egui::Context::default();
        let after_id = egui::Id::new("focus_after_widget");
        let mut after_text = String::new();
        run_pass(&after, typed(), |ui| {
            let response = ui.add(egui::TextEdit::singleline(&mut after_text).id(after_id));
            response.request_focus();
        });
        assert_eq!(
            after_text, "",
            "構築後に要求すると、そのフレームの文字は焦点の無い widget を素通りして捨てられる"
        );
    }

    /// kittest の state。**復元フラグをフレームごとに切り替える**ために buf と束ねる。
    struct CaretState {
        buf: String,
        restored: bool,
        window_focused: bool,
    }

    fn caret_harness(id: egui::Id, focused: bool) -> egui_kittest::Harness<'static, CaretState> {
        egui_kittest::Harness::new_ui_state(
            move |ui, st: &mut CaretState| {
                let params = SearchInputParams {
                    input_id: id,
                    restored_search: st.restored,
                    window_focused: st.window_focused,
                    input_editable: true,
                    inset: 0.0,
                    field_height: 20.0,
                    font: egui::FontId::proportional(12.0),
                    text_color: egui::Color32::WHITE,
                };
                let _ =
                    search_input_ui(ui, test_visuals(), &mut st.buf, &params, |_| String::new());
            },
            CaretState {
                buf: "alpha".to_owned(),
                restored: false,
                window_focused: focused,
            },
        )
    }

    /// 復元フレームで、**同一フレームに載っていた**文字が末尾へ入る（#840/#872）。
    ///
    /// **縛るのは `move_text_cursor_to_end` の位置だけである**——それが `TextEdit` の後ろへ
    /// 動けば落ちる。**focus 要求の位置はこの検査では縛れない**（判定フレームの開始時点で
    /// 既に焦点が立っており、`!has_focus` ガードで要求が走らないため・実測）。そちらは
    /// `kittest_first_frame_requests_focus_before_text_edit` が持つ。
    ///
    /// **`step()` を使う（`run()` ではない）。** `run()` は再描画要求が尽きるまで複数フレーム
    /// 回すため、文字が 2 フレーム目で入っても通ってしまい、この検査の主題（同一フレーム）が
    /// 骨抜きになる。`step()` は「キューされた各イベントにつき 1 フレーム、イベントが無ければ
    /// 1 フレーム」である（`egui_kittest` 0.35 の `Harness::step` の doc）。
    #[test]
    fn kittest_restored_frame_appends_same_frame_input_at_end() {
        let id = egui::Id::new("search_input");
        let mut harness = caret_harness(id, true);
        // 復元より前に 2 フレーム回して focus と `TextEdit` の state を確立する
        //（`move_text_cursor_to_end` は `TextEdit::load_state` が None の間は何もしない）。
        harness.step();
        harness.step();

        // **キャレットを先頭へ置く。** これが無いと検査は discriminate しない——focus 直後の
        // キャレットは既に末尾に在り、`move_text_cursor_to_end` が no-op になるので、
        // 呼び出しを `TextEdit` の後ろへ動かしても結果が変わらない（故障注入で実測した）。
        // 実経路の `restored_search` は**バッファ全体が置き換わった**フレームであり、
        // 残るキャレットは古い（短い）テキストの位置を指す。その状態をここで作る。
        let mut state = TextEditState::load(&harness.ctx, id).expect("2 フレーム後に state が在る");
        state
            .cursor
            .set_char_range(Some(CCursorRange::one(CCursor::new(0))));
        state.store(&harness.ctx, id);

        // 復元フレーム: `restored=true` と文字を**同じフレーム**へ載せる。
        // 文字は本体と同じ経路で渡す（runtime は WM_CHAR / IME 確定を `Ime(Commit)` にする）。
        harness.state_mut().restored = true;
        harness
            .input_mut()
            .events
            .push(egui::Event::Ime(egui::ImeEvent::Commit("z".to_owned())));
        harness.step();

        assert_eq!(
            harness.state().buf.as_str(),
            "alphaz",
            "復元フレームに載った打鍵は復元クエリの末尾へ入る"
        );
    }

    /// **プロセス起動後の最初のフレーム**を再現する（#872/#936 の実害そのもの）。
    ///
    /// **上の検査は focus の並びを縛れない**（実測）。判定フレームより前に `step()` を回すため、
    /// その時点で widget は既に焦点を持ち、`!has_focus` ガードで focus 要求が**そもそも走らない**
    /// ——ゆえに要求を `TextEdit` の後ろへ動かしても通る。縛れているのはキャレットの側だけである。
    ///
    /// こちらは事前の `step()` を置かない。widget がまだ焦点を持たないフレームに文字が載る形は
    /// **起動直後の初回 show そのもの**であり、要求が構築の後ろにあれば文字は焦点の無い widget の
    /// 横を素通りして捨てられる。
    #[test]
    fn kittest_first_frame_requests_focus_before_text_edit() {
        let id = egui::Id::new("search_input");
        let mut harness = caret_harness(id, true);

        // **判定フレームの開始時点で焦点を持っていない状態を作る。** `Harness` は構築の時点で
        // 既にフレームを 1 枚走らせるため、「`step()` を呼ばない」だけでは足りない（実測: それだけ
        // では焦点が立っていて、要求を後ろへ動かしても通ってしまった）。
        harness.ctx.memory_mut(|m| m.surrender_focus(id));

        harness
            .input_mut()
            .events
            .push(egui::Event::Ime(egui::ImeEvent::Commit("z".to_owned())));
        harness.step();

        assert_ne!(
            harness.state().buf.as_str(),
            "alpha",
            "最初のフレームに載った打鍵が捨てられている（focus 要求が TextEdit の後ろにある）"
        );
    }

    /// focus を要求しなければ同じ文字が捨てられる。
    /// **この検査が本当に focus を見ていることの対照**であり、無いと上の検査が
    /// 「何をしても通る」形に退化したことに気づけない。
    #[test]
    fn kittest_without_focus_request_the_same_input_is_dropped() {
        let id = egui::Id::new("search_input");
        let mut harness = caret_harness(id, false); // focus 要求の条件（`window_focused`）を落とす
        harness.step();
        harness.step();

        harness.state_mut().restored = true;
        harness
            .input_mut()
            .events
            .push(egui::Event::Ime(egui::ImeEvent::Commit("z".to_owned())));
        harness.step();

        assert_eq!(
            harness.state().buf.as_str(),
            "alpha",
            "焦点が無ければ文字は入らない"
        );
    }

    /// #751: テーマ 3 値の適用は**同じ pass の子 Ui へ届かなければならない**。
    ///
    /// egui 0.35.0 の `Context::run_ui` は user callback より**前**に root `Ui` を作り
    /// （`context.rs:780-807`）、`Ui::new` はそこで `ctx.global_style()` を `Arc<Style>` として
    /// 掴む（`ui.rs:108-136`）。ゆえに callback 内の `ctx.set_visuals` は現在の pass に届かず、
    /// 色だけを変えた config 適用フレームで入力欄だけが旧色で残った。`ui.visuals_mut()` は
    /// copy-on-write でこの `Ui` と以後の子 Ui（`ui.rs:236` の `Arc::clone`）に効く——
    /// `update()` の適用がそちらを使う理由である。
    ///
    /// **`ctx.set_visuals` が届かないことを固定する対のテストは意図的に置かない。** それは
    /// egui の現在の制限を固定する主張であり、上流が直した日に緑のビルドが赤くなる。
    ///
    /// **このテストが守るのは egui の伝播であって、製品関数の適用位置ではない。**
    /// 「子 `Ui` の生成より前で適用する」という #751 の順序不変条件を縛るのは、下の
    /// `search_input_ui_applies_theme_values_to_child_ui_in_the_first_pass` である（#949）。
    /// **両者を併存させるのは切り分けのためである**——あちらが落ちたときに**こちらが緑なら、
    /// 原因は egui ではなく製品コードの側**だと言える。
    #[test]
    fn ui_visuals_mut_reaches_child_ui_in_the_same_pass() {
        let ctx = egui::Context::default();
        // 既定色と偶然一致して通ることが無いよう、3 値とも非既定の色を使う。
        let input_bg = egui::Color32::from_rgb(0x80, 0x30, 0x20);
        let selection = egui::Color32::from_rgb(0x20, 0x70, 0x40);
        let hint = egui::Color32::from_rgb(0x10, 0x20, 0xF0);

        let seen = std::cell::RefCell::new(None);
        // **測るのは最初の pass である。** 2 回目以降は global style 経由でも通ってしまうため、
        // 1 pass しか走らせないことがこのテストの要点である（症状の成立条件そのもの）。
        run_pass(&ctx, egui::RawInput::default(), |ui| {
            let visuals = ui.visuals_mut();
            visuals.extreme_bg_color = input_bg;
            visuals.selection.bg_fill = selection;
            visuals.weak_text_color = Some(hint);

            // TextEdit が置かれるのと同じ形の子 Ui（`update()` の `egui::Frame::new().show`）。
            // 読む側も TextEdit と同じ getter を使う——`text_edit_bg_color()` は
            // `text_edit_bg_color.unwrap_or(extreme_bg_color)`、`weak_text_color()` は
            // `Option` を解決する（`style.rs:1135-1148`）。生フィールドを見ると、TextEdit が
            // 実際に読む経路を素通りする。
            egui::Frame::new().show(ui, |child| {
                *seen.borrow_mut() = Some((
                    child.visuals().text_edit_bg_color(),
                    child.visuals().selection.bg_fill,
                    child.visuals().weak_text_color(),
                ));
            });
        });

        assert_eq!(seen.into_inner(), Some((input_bg, selection, hint)));
    }

    /// #949: **テーマ 3 値の適用は `search_input_ui` の入口に在り、同じ pass の子 `Ui` へ届く。**
    ///
    /// **上のテストとの分担**（消さないこと）: あちらが測るのは **egui の伝播**であって、製品関数の
    /// どこで適用しているかは縛らない。こちらは `search_input_ui` を**実コードのまま** 1 pass
    /// 走らせ、`hint` クロージャが受け取る子 `Ui` の値を突き合わせる。**新テストが落ちたとき、
    /// 上のテストが緑なら原因は製品コード側だと切り分けられる**——併存はその対照のためである。
    ///
    /// #751 が「コンパイラもユニットテストも `check:colors` も smoke も捕まえない受容残余」と
    /// 記録した順序不変条件は、#949 の移設とこの検査で**検知手段を持つ**。
    ///
    /// **`Harness` ではなく素の `run_ui` を使う。** 初回 pass であることが症状の成立条件そのもの
    /// であり、`egui_kittest::Harness` は構築の時点で既に 1 フレーム走らせる
    ///（`kittest_first_frame_requests_focus_before_text_edit` の doc に実測が在る）。
    ///
    /// **この検査が捕まえないもの**は `search_input_ui` の doc（`update()` 側の未適用区間）。
    #[test]
    fn search_input_ui_applies_theme_values_to_child_ui_in_the_first_pass() {
        let ctx = egui::Context::default();
        let expected = test_visuals();
        let mut buf = String::new();
        let params = SearchInputParams {
            input_id: egui::Id::new("search_input"),
            restored_search: false,
            window_focused: true,
            input_editable: true,
            inset: 0.0,
            field_height: 20.0,
            font: egui::FontId::proportional(12.0),
            text_color: egui::Color32::WHITE,
        };

        let seen = std::cell::RefCell::new(None);
        run_pass(&ctx, egui::RawInput::default(), |ui| {
            // 読む側は `TextEdit` と同じ解決 getter を使う（生フィールドを見ると実経路を
            // 素通りする——理由は上のテストの doc）。
            let _ = search_input_ui(ui, test_visuals(), &mut buf, &params, |child| {
                *seen.borrow_mut() = Some((
                    child.visuals().text_edit_bg_color(),
                    child.visuals().selection.bg_fill,
                    child.visuals().weak_text_color(),
                ));
                String::new()
            });
        });

        assert_eq!(
            seen.into_inner(),
            Some((expected.input_bg, expected.selection, expected.hint)),
            "3 値が子 Ui へ届いていない（適用が子 Ui の生成より後ろに在るか、値が欠けている）"
        );
    }

    /// バー余白の 1 点を突いて `(背景がドラッグを取ったか, 入力欄が hover したか)` を返す。
    ///
    /// **`update()` の登録順を再現する**——背景ドラッグ（`Sense::drag()` の `max_rect`）が先、
    /// 入力欄が後。`hit_test` の「大きな drag 背景の上に載った小さな click widget を助ける」枝は
    /// 背景があって初めて通り、その枝こそが食い込みの実体だからである。
    ///
    /// 幾何は既定 config の実値（`Metrics::from_config(15, 6, 28)`: 余白 7・欄高 29）。
    /// `offset` は入力欄の矩形からの符号付き距離（正 = 外側の余白、負 = 欄の内側）。
    fn probe_bar_margin(apply_style: bool, offset: f32) -> (bool, bool) {
        const W: f32 = 600.0;
        const INSET: f32 = 7.0;
        const FIELD_H: f32 = 29.0;
        const BAR_H: f32 = FIELD_H + 2.0 * INSET;

        let ctx = egui::Context::default();
        if apply_style {
            super::apply_exact_hit_test_style(&ctx);
        }
        let mut buf = String::new();
        let pos = egui::pos2(W / 2.0, INSET - offset);
        let mut got = (false, false);

        // frame 0-1: widget を登録（ヒットテストは前フレームの矩形を見る）/ 2: 押下 / 3: 移動
        for frame in 0..4 {
            let mut events = vec![egui::Event::PointerMoved(pos)];
            match frame {
                2 => events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                }),
                3 => events = vec![egui::Event::PointerMoved(pos + egui::vec2(10.0, 0.0))],
                _ => {}
            }
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(W, BAR_H),
                )),
                focused: true,
                events,
                ..Default::default()
            };
            run_pass(&ctx, input, |ui| {
                let bg = ui.interact(
                    ui.max_rect(),
                    egui::Id::new("main-window-drag"),
                    egui::Sense::drag(),
                );
                let params = SearchInputParams {
                    input_id: egui::Id::new("search_input"),
                    restored_search: false,
                    window_focused: true,
                    input_editable: true,
                    inset: INSET,
                    field_height: FIELD_H,
                    font: egui::FontId::proportional(15.0),
                    text_color: egui::Color32::WHITE,
                };
                let te = search_input_ui(ui, test_visuals(), &mut buf, &params, |_| String::new());
                if frame == 3 {
                    got = (bg.dragged(), te.hovered());
                }
            });
        }
        got
    }

    /// 入力欄以外の全域をドラッグして移動可能という `SPEC.md`「8.2 ウィンドウ位置」の定めを、
    /// 余白の実点で測る。
    ///
    /// egui 既定の `interaction.interact_radius = 5.0` は入力欄の当たりを矩形の外 5px へ広げ、
    /// 余白 7px のうち内側 5px を食っていた（掴めるのは外側 2px だけ・実測）。
    /// `apply_exact_hit_test_style` はそれを 0 にして判定を描画矩形へ一致させる。
    ///
    /// **対照（未適用）を同じ検査に置く**——適用の有無で結果が変わらなければ、この検査は
    /// 何も縛っていない。**欄の内側**も併せて測る: 判定を縮めた結果、入力欄自身が当たらなく
    /// なっていないことまで見る。
    #[test]
    fn bar_margin_belongs_to_the_window_drag_not_the_input_field() {
        // **欄の外 1px を突く**——SPEC が言う「入力欄以外」は余白の全域であり、境界の隣から
        // 掴めなければならない。`interact_radius` がわずかでも残れば落ちる（1.0 で既に食う）。
        assert_eq!(
            probe_bar_margin(true, 1.0),
            (true, false),
            "余白（欄の外 1px）が窓ドラッグへ渡っていない"
        );
        assert_eq!(
            probe_bar_margin(false, 1.0),
            (false, true),
            "対照が成立しない: style 未適用でも食い込みが起きないなら、この検査は何も縛っていない"
        );
        assert_eq!(
            probe_bar_margin(true, -5.0),
            (false, true),
            "欄の内側 5px が入力欄に当たらない（判定を縮めすぎている）"
        );
    }

    /// OS のテーマが切り替わっても当たり判定が既定へ戻らない（`all_styles_mut` の要）。
    ///
    /// `global_style_mut` は**現在テーマの style だけ**を書くため、`system_theme` が Light で
    /// 届いた瞬間に書いていない側が現役になり、修正が黙って消える。**`theme_preference` は
    /// 既定の `System` のままで起きる**——この窓はテーマ設定に触れないので、コード側は何も
    /// していないのに OS の報せだけで倒れる経路である。
    ///
    /// **両テーマを直接読む**（`global_style()` は現在テーマしか返さないので、切替を経ずに
    /// 「書いていない側」を見られない）。
    #[test]
    fn hit_test_style_survives_a_system_theme_change() {
        let ctx = egui::Context::default();
        super::apply_exact_hit_test_style(&ctx);

        for theme in [egui::Theme::Dark, egui::Theme::Light] {
            assert_eq!(
                ctx.style_of(theme).interaction.interact_radius,
                0.0,
                "{theme:?} の style へ届いていない（現在テーマだけを書く API を使っている）"
            );
        }

        // OS からの報せを 1 フレーム流す（`RawInput.system_theme`）。実経路と同じ形で、
        // `theme_preference` は `System` のまま現役の style だけが入れ替わる。
        let input = egui::RawInput {
            system_theme: Some(egui::Theme::Light),
            ..Default::default()
        };
        run_pass(&ctx, input, |_| {});
        assert_eq!(
            ctx.global_style().interaction.interact_radius,
            0.0,
            "system_theme=Light が届いた後に既定へ戻っている"
        );
    }

    /// 入力欄を実コードのまま 1 pass 描き、**空でない galley** の
    /// `(欄の上端からの余り, 欄の下端までの余り)` を返す。
    ///
    /// 幾何は既定 config の導出（`Metrics::from_config(font_size, 6, 28)`）から取る——
    /// 欄高を検査に書き写すと、`bar_padding` の導出が変わったときにここだけが腐る。
    ///
    /// **空 galley を除くのは、text が空のフレームで `TextEdit` がキャレット用に 0 幅の galley を
    /// 積むためである**（hint の galley と同じ位置に重なる・実測）。除かないと 1 件のはずの
    /// 測定対象が 2 件になる。
    fn galley_margins(font_size: u32, text: &str) -> Vec<(f32, f32)> {
        fn walk(shape: &egui::Shape, out: &mut Vec<(egui::Pos2, f32)>) {
            match shape {
                egui::Shape::Text(t) if !t.galley.text().is_empty() => {
                    out.push((t.pos, t.galley.size().y));
                }
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }

        let metrics = crate::egui_shell::layout::Metrics::from_config(font_size, 6, 28);
        let inset = metrics.bar_inset as f32;
        let params = SearchInputParams {
            input_id: egui::Id::new("search_input"),
            restored_search: false,
            window_focused: true,
            input_editable: true,
            inset,
            field_height: (metrics.bar_height as f32 - 2.0 * inset).max(1.0),
            font: egui::FontId::proportional(font_size as f32),
            text_color: egui::Color32::WHITE,
        };

        let ctx = egui::Context::default();
        let mut buf = text.to_owned();
        let mut field = egui::Rect::NOTHING;
        let out = run_pass(&ctx, egui::RawInput::default(), |ui| {
            field = search_input_ui(ui, test_visuals(), &mut buf, &params, |_| {
                "検索...".to_owned()
            })
            .rect;
        });

        let mut texts = Vec::new();
        for clipped in &out.shapes {
            walk(&clipped.shape, &mut texts);
        }
        texts
            .into_iter()
            .map(|(pos, height)| (pos.y - field.top(), field.bottom() - (pos.y + height)))
            .collect()
    }

    /// 入力欄の文字は欄の**縦中央**に置かれる（本文・hint とも）。
    ///
    /// egui 0.35 の `TextEdit` は `Align2::LEFT_TOP` を既定に持つため（`builder.rs` の
    /// `TextEdit::default`）、`add_sized` が欄を `field_height` へ引き伸ばしても galley は上端に
    /// 留まり、余りが全部下へ落ちる。既定 config での修正前の実測は**上 2px / 下 10px** で、
    /// 上余りは `TextEdit` 既定 `Margin::symmetric(4, 2)` の top ちょうどだった——
    /// `Metrics::bar_inset` の doc が宣言する「文字の上下に 7」の設計意図がそこで破れていた。
    ///
    /// **font_size を 2 通り測る。** 中央は `field_height` と galley 高の差から導かれるので、
    /// 固定 px を足す実装（`TextEdit::margin` の非対称化）へ差し替わると片方の font で落ちる
    /// ——`SPEC.md` §11「文字サイズに固定値を書かない」を検査の側から縛る。
    ///
    /// **hint も測る。** 本文だけが中央へ寄れば、打鍵の瞬間に文字が上下へ飛ぶ。egui の hint は
    /// `Align2::LEFT_TOP` を明示して積まれるが、singleline ではその align が効くのは自分の
    /// cell（= galley 高ちょうど）の中だけで、ブロック全体を動かすのは `align2` の側である
    ///（`AtomLayout` の `align_size_within_rect`）——ゆえに 1 つの指定で両方が動く。
    #[test]
    fn input_text_sits_vertically_centered_for_both_body_and_hint() {
        for font_size in [15, 24] {
            for (label, text) in [("本文", "aqgILあいう"), ("hint", "")] {
                let margins = galley_margins(font_size, text);
                assert_eq!(
                    margins.len(),
                    1,
                    "{label}（font {font_size}）: 測る galley は 1 つのはず（実際 {margins:?}）"
                );
                let (top, bottom) = margins[0];
                assert!(
                    (top - bottom).abs() <= 0.5,
                    "{label}（font {font_size}）: 縦中央から外れている（上 {top} / 下 {bottom}）\
                     ——`vertical_align` が落ちると egui 既定の上寄せへ戻る"
                );
            }
        }
    }
}
