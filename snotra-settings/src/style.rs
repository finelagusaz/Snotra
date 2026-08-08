//! 設定 UI のデザイントークンと共有スタイルヘルパー。
//!
//! 色・余白・フォントサイズ・リスト行/モーダルのレイアウトを 1 箇所に集約し、
//! 全タブがこのモジュール経由で描画する。直書きの `Color32::from_rgb` / `add_space` /
//! `ScrollArea` を各タブに散らさないことで、タブ間のデザインを統一する。
//!
//! 規約とトークンの意図は `SETTINGS-DESIGN.md`（クレート直下）を SSOT とする。

use eframe::egui;
use egui::Color32;

use crate::i18n::{Tr, TrKey};

// ---- タイポグラフィ（Windows 11 Fluent タイプランプ epx）----
// 出典: Microsoft Learn「Typography in Windows」。`apply_type_ramp` で TextStyle に登録する。
/// セクション見出し・モーダル題（Fluent: Body Large 18）。
pub const FONT_HEADING: f32 = 18.0;
/// ラベル・値・項目主行・ボタン（Fluent: Body 14。判読最小 14px Regular）。
pub const FONT_BODY: f32 = 14.0;
/// ヒント・メタ・説明文 = `.small()`（Fluent: Caption 12。判読最小 12px Regular）。
pub const FONT_CAPTION: f32 = 12.0;

// ---- 縦リズム（spacing scale）----
/// `interact_size.y`。本文 14px 化に伴い 24→28。チェックボックス/DragValue の縦整列の基準。
pub const ROW_HEIGHT: f32 = 28.0;
/// 見出し → 最初のウィジェット。
pub const SPACE_HEADING: f32 = 4.0;
/// ウィジェット → ヒント等の小間隔。
pub const SPACE_HINT: f32 = 4.0;
/// 説明文 → リスト / モーダル内フィールド群。
pub const SPACE_GROUP: f32 = 8.0;
/// セクション間。
pub const SPACE_SECTION: f32 = 12.0;

// ---- フィールド幅 ----
/// DragValue / プレフィックス入力。
pub const FIELD_NUMERIC: f32 = 60.0;
/// hex カラー入力。
pub const FIELD_HEX: f32 = 80.0;

// ---- 色パレット（Windows 11 Settings インスパイア。app.rs から移設した SSOT）----
/// WinUI NavigationView pane。
pub const SIDEBAR_BG: Color32 = Color32::from_rgb(243, 243, 243);
/// WinUI content area。
pub const CONTENT_BG: Color32 = Color32::from_rgb(249, 249, 249);
pub const FOOTER_BG: Color32 = Color32::from_rgb(243, 243, 243);
/// Windows 11 default blue。
pub const ACCENT: Color32 = Color32::from_rgb(0, 103, 192);
pub const TAB_HOVER: Color32 = Color32::from_rgb(232, 232, 232);
pub const TAB_SELECTED_BG: Color32 = Color32::from_rgb(255, 255, 255);
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(26, 26, 26);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(96, 96, 96);
pub const WIDGET_BG: Color32 = Color32::from_rgb(255, 255, 255);
pub const WIDGET_BORDER: Color32 = Color32::from_rgb(210, 210, 210);
/// アクティブ（押下）ウィジェットの背景。
pub const WIDGET_ACTIVE_BG: Color32 = Color32::from_rgb(220, 220, 220);

// ---- セマンティック色（散在していた意味色の SSOT）----
/// 削除・エラー（赤）。
pub const STATUS_ERROR: Color32 = Color32::from_rgb(196, 43, 28);
/// 成功（緑）。
pub const STATUS_SUCCESS: Color32 = Color32::from_rgb(16, 124, 16);
/// 警告（オレンジ。legacy 移行 ⚠ 等）。
pub const STATUS_WARNING: Color32 = Color32::from_rgb(196, 120, 28);
/// 注意バナーの背景。
pub const BANNER_CAUTION_BG: Color32 = Color32::from_rgb(255, 244, 206);
/// 注意バナーの文字。
pub const BANNER_CAUTION_FG: Color32 = Color32::from_rgb(140, 90, 0);

/// カスタム TextStyle ランプ（Fluent 準拠）を `ctx` に登録する。
///
/// `ctx.all_styles_mut` で各 TextStyle の `size` を書き換えるため、`.small()` /
/// `ui.heading()` / 既定ラベルが新サイズを自動継承する（タイポグラフィの SSOT）。
/// `heading_semibold` が true のとき `Heading` のファミリを Semibold
/// （[`crate::font::SEMIBOLD_FAMILY`]）へ切り替える（Fluent: 見出しは Semibold）。
/// false（Semibold フォント不在）なら Heading は Regular のまま据え置く。
/// Monospace は egui 既定を温存し、Body/Button/Small のファミリも変更しない。
/// `run()` で `configure_fonts` → `apply_win11_theme` の後に呼ぶ。
pub fn apply_type_ramp(ctx: &egui::Context, heading_semibold: bool) {
    use egui::TextStyle;
    ctx.all_styles_mut(|style| {
        if let Some(f) = style.text_styles.get_mut(&TextStyle::Heading) {
            f.size = FONT_HEADING;
            if heading_semibold {
                f.family = egui::FontFamily::Name(crate::font::SEMIBOLD_FAMILY.into());
            }
        }
        if let Some(f) = style.text_styles.get_mut(&TextStyle::Body) {
            f.size = FONT_BODY;
        }
        if let Some(f) = style.text_styles.get_mut(&TextStyle::Button) {
            f.size = FONT_BODY;
        }
        if let Some(f) = style.text_styles.get_mut(&TextStyle::Small) {
            f.size = FONT_CAPTION;
        }
    });
}

/// タブ本体を包む標準スクロール領域。`interact_size.y` を [`ROW_HEIGHT`] に設定する。
///
/// 全タブで完全同一だった `ScrollArea::vertical().auto_shrink([false,false]).scroll_source(drag:false)` ボイラープレートを 1 箇所に集約する。
pub fn tab_scroll_area<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .scroll_source(egui::scroll_area::ScrollSource {
            drag: egui::scroll_area::DragScroll::Never,
            ..Default::default()
        })
        .show(ui, |ui| {
            ui.spacing_mut().interact_size.y = ROW_HEIGHT;
            add_contents(ui)
        })
        .inner
}

/// セクション見出し + 標準余白。
pub fn section_heading(ui: &mut egui::Ui, text: &str) {
    ui.heading(text);
    ui.add_space(SPACE_HEADING);
}

/// セクション間の余白。
pub fn section_gap(ui: &mut egui::Ui) {
    ui.add_space(SPACE_SECTION);
}

/// 副次テキスト行（Caption + [`TEXT_SECONDARY`]）。説明文・メタ・ヒントの規範。
pub fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).small().color(TEXT_SECONDARY));
}

/// 2 カラムのラベル/値グリッド（`num_columns(2).spacing([8,4])`）。
pub fn settings_grid(id_salt: &str) -> egui::Grid {
    egui::Grid::new(id_salt).num_columns(2).spacing([8.0, 4.0])
}

/// モーダルのヘッダ（題 + 区切り線 + 余白）。
pub fn modal_header(ui: &mut egui::Ui, title: &str) {
    ui.heading(title);
    ui.separator();
    ui.add_space(SPACE_HEADING);
}

/// 破壊的操作のボタン（[`STATUS_ERROR`] 文字色）。削除ボタンの規範。
pub fn danger_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(
        egui::RichText::new(text).color(STATUS_ERROR),
    ))
}

/// リスト行の規範スキャフォールド: 本文を左、アクションを右寄せ、末尾に区切り線。
///
/// 中身は `body` / `actions` クロージャに委ね、構造（horizontal + 左 vertical +
/// 右 `right_to_left` + 末尾 separator）だけを統一する。`actions` の戻り値を返す。
pub fn list_item<R>(
    ui: &mut egui::Ui,
    body: impl FnOnce(&mut egui::Ui),
    actions: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let result = ui
        .horizontal(|ui| {
            ui.vertical(body);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), actions)
                .inner
        })
        .inner;
    ui.separator();
    result
}

/// 並び替えの方向。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ReorderDir {
    Up,
    Down,
}

/// 並び替えボタン ▲▼ の規範。クリックされた方向を返す。
///
/// **親レイアウトが `right_to_left`（[`list_item`] の actions クラスタ）であることを前提**に、
/// ▼ → ▲ の順で add して視覚的に ▲（上）▼（下）の左右順に揃える。通常サイズ。
/// `can_up` / `can_down` は呼び出し側が算出する（端で disabled）。
pub fn reorder_controls(ui: &mut egui::Ui, can_up: bool, can_down: bool) -> Option<ReorderDir> {
    let mut dir = None;
    if ui.add_enabled(can_down, egui::Button::new("▼")).clicked() {
        dir = Some(ReorderDir::Down);
    }
    if ui.add_enabled(can_up, egui::Button::new("▲")).clicked() {
        dir = Some(ReorderDir::Up);
    }
    dir
}

/// モーダルフッタの Cancel/Save 対（3 モーダルで完全同一）。どちらが押されたかを返す。
///
/// 削除ボタン（[`danger_button`]）と Vec 変更ロジックは保存処理がタブごとに異なるため
/// 各モーダルに残す。この対のみを共有する。
pub struct ModalButtons {
    pub cancel: bool,
    pub save: bool,
}

pub fn modal_buttons(ui: &mut egui::Ui, tr: &Tr) -> ModalButtons {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let cancel = ui.button(tr.t(TrKey::BtnCancel)).clicked();
        let save = ui.button(tr.t(TrKey::BtnSave)).clicked();
        ModalButtons { cancel, save }
    })
    .inner
}
