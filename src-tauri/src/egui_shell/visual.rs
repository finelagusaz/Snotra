//! テーマ（config `[visual]` のうち**色・font・padding** + `appearance.show_icons`）の
//! 1 フレーム分の読み取り値と、その純粋な導出（#673 spec 決定 4）。
//!
//! **`[visual]` 全体ではない。`window_gap` は含まない**——results 窓の配置
//! （`window_coordinator::position_results_below_main`）は main の `update()` と `Moved` リスナーの
//! 両方から呼ばれ、**フレームに閉じない**読みだからである。ゆえに `window_gap` だけは
//! 1 フレーム内で別 lock から読まれうる（`window_gap` と `font_size` を同時に変更した
//! 1 フレームだけ新 gap × 旧行高になりうるが、次フレームで自然に直る cosmetic）。
//!
//! **なぜ束ねるか**: 従来は 1 フレームの中でテーマ色（文字色・font_size）と `read_metrics`
//! （font_size・padding）が別々に lock を取っていた。その間に `config_watcher` の
//! `apply_config_change` が挟まると、**新しい font_size を旧い行高で描く 1 フレーム**が生じる。
//! 次フレームで自然に直る cosmetic な窓だが、同じ値を 2 度読む構造そのものが窓の原因である。
//!
//! **保持しないこと**: `VisualSnapshot` の寿命は 1 フレームである。`self.` へ持つと config 変更が
//! 反映されなくなる（毎フレーム live-read 方針・#576 / #646 決定 2）。
//!
//! **色のパーサは 1 本である**（spec 決定 4 で統合）: `egui::Color32::from_hex`（`#RGB` / `#RGBA` /
//! `#RRGGBB` / `#RRGGBBAA` を受理）。tao のネイティブ背景ブラシへは `native_brush_color` が
//! `Color32` から変換する——**alpha は捨てる**（softbuffer の clear color が持てないため）。
//! 旧 `config_watcher::parse_hex_color`（`#RRGGBB` 厳格）と 2 本立てだった頃は、`#FFF` を書くと
//! 描画色だけが白になり下地は既定色へ落ちていた（#680 の 1）。統合で消えたのはこの非対称である。

use std::sync::LazyLock;

use snotra_core::config::VisualConfig;

use crate::egui_shell::layout::{self, Metrics};

/// フォールバックの正本（`config.rs` の `default_*`）を **1 度だけ**構築して使い回す。
///
/// `VisualConfig::default()` は色 5 本 + font_family の `String` を毎回ヒープ確保する。
/// `visual_snapshot` は engine の guard 内で呼ばれるため、呼び出しごとに作ると
/// **mutex を握ったまま毎フレーム 6 本確保する**——「guard 内は parse と算術と `&str` 比較
/// まで」という本 PR 自身の制約に反する（レビュー Important 1）。
static DEFAULT_VISUAL: LazyLock<VisualConfig> = LazyLock::new(VisualConfig::default);

/// AppState 不在時に渡す既定 config（確保ゼロ・`DEFAULT_VISUAL` の doc 参照）。
pub(crate) fn default_visual() -> &'static VisualConfig {
    &DEFAULT_VISUAL
}

/// 1 結果行の描画テーマ。main のバー行・toast 行も同じ型を使う（#646 PR2 で results 側に
/// あったものを #673 で純粋核へ移設）。
pub(crate) struct RowTheme {
    pub name_color: egui::Color32,
    pub path_color: egui::Color32,
    pub selection: egui::Color32,
    pub name_size: f32,
    pub path_size: f32,
    /// status 行・updater トーストの本文サイズ（#672）。**両者は同じ「バー直下の 1 行」で
    /// あり、揃っていないと別種の UI に見える**（#700）——1 つの値を両方が読むことで揃え続ける。
    pub status_size: f32,
    /// トーストのボタンサイズ。本文より一回り小さい（#700 の意図を比率で保つ）。
    pub button_size: f32,
}

/// 呼び出し側が既に適用済みの値。**guard 内で比較し、変化したときだけ clone する**ための入力。
pub(crate) struct VisualApplied<'a> {
    /// 各窓の `applied_font_family`（ctx は窓ごとに独立ゆえ main と results で別々に持つ）。
    pub font_family: &'a str,
}

/// 1 フレーム分のテーマ値。main と results の要求の**和集合**である（窓ごとの projection に
/// 分けない——分けると導出式が再び 2 箇所になる・spec 決定 4）。
pub(crate) struct VisualSnapshot {
    /// パネル背景（main のみ使用）。
    pub background: egui::Color32,
    /// TextEdit / overlay 背景（main のみ使用）。
    pub input_bg: egui::Color32,
    /// 選択色（main の `visuals.selection.bg_fill`）。行の選択帯は `row.selection` を使う。
    pub selection: egui::Color32,
    /// ヒント文字色（main の overlay）。行の path 色は `row.path_color` を使う。
    pub hint: egui::Color32,
    /// 行テーマ（main のバー・toast、results の各行）。
    pub row: RowTheme,
    /// 行高・バー高・toast 高・バー内余白。
    pub metrics: Metrics,
    /// アイコン枠を描くか（results のみ使用）。
    pub show_icons: bool,
    /// `applied.font_family` と異なるときだけ `Some`。呼び出し側は Some のときだけ
    /// `font_stack::configure_japanese_font` を呼び、`applied` を更新する。
    pub font_family_changed: Option<String>,
}

/// `VisualConfig` から 1 フレーム分の値を導く**純関数**（lock を持たない・テスト対象）。
///
/// フォールバックは `VisualConfig::default()` が正本である（リテラルを再手打ちしない・
/// `read_metrics` と同方針）。hex の parse に失敗した色だけが既定色へ落ちる。
pub(crate) fn visual_snapshot(
    v: &VisualConfig,
    show_icons: bool,
    applied: VisualApplied<'_>,
) -> VisualSnapshot {
    let d = &*DEFAULT_VISUAL;
    let text = hex_or(&v.text_color, &d.text_color);
    let hint = hex_or(&v.hint_text_color, &d.hint_text_color);
    let selection = hex_or(&v.selected_row_color, &d.selected_row_color);
    VisualSnapshot {
        background: hex_or(&v.background_color, &d.background_color),
        input_bg: hex_or(&v.input_background_color, &d.input_background_color),
        selection,
        hint,
        row: RowTheme {
            name_color: text,
            path_color: hint,
            selection,
            name_size: v.font_size as f32,
            // 正本は layout（行高が同じ係数で積算するため。二重定義は行高と描画の不一致）。
            path_size: layout::path_size(v.font_size) as f32,
            status_size: layout::status_size(v.font_size) as f32,
            button_size: (layout::status_size(v.font_size) as f32 * 0.92).max(10.0),
        },
        // 導出式を書き写さない——行高の正本は Metrics::from_config である。
        metrics: Metrics::from_config(v.font_size, v.row_padding, v.bar_padding),
        show_icons,
        font_family_changed: (v.font_family != applied.font_family).then(|| v.font_family.clone()),
    }
}

/// 背景色 hex を `Color32` へ。**フォールバックの正本は `VisualConfig::default()`** であり、
/// `#282828` のリテラルをここへ再手打ちしない（`hex_or` の doc と同方針）。
pub(crate) fn background_color(hex: &str) -> egui::Color32 {
    hex_or(hex, &DEFAULT_VISUAL.background_color)
}

/// `Color32` を tao のネイティブ背景ブラシ色へ（spec 決定 4）。
///
/// **alpha は捨てて 255 にする**——softbuffer の clear color が `0x00RRGGBB` で alpha を持てず、
/// 下地と定常の背景が食い違うと show の一瞬だけ色が変わって見えるためである。
/// **パーサは `Color32::from_hex` の 1 本だけ**になった（旧 `config_watcher::parse_hex_color` は
/// `#RRGGBB` 厳格で、`#FFF` を書くと下地だけ既定色へ落ちていた・#680 の 1）。
pub(crate) fn native_brush_color(color: egui::Color32) -> tauri::window::Color {
    tauri::window::Color(color.r(), color.g(), color.b(), 0xff)
}

/// `#RRGGBB` 等を `Color32` へ。parse 失敗時は既定値（の parse 結果）へ落ちる。
/// 既定値まで parse に失敗することは無い（`config.rs` の `default_*` は妥当な 6 桁 hex）が、
/// release は `panic="abort"` ゆえ unwrap しない——最後は黒へ落とす。
fn hex_or(s: &str, default_hex: &str) -> egui::Color32 {
    egui::Color32::from_hex(s)
        .or_else(|_| egui::Color32::from_hex(default_hex))
        .unwrap_or(egui::Color32::BLACK)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> VisualConfig {
        VisualConfig::default()
    }

    fn applied_none(font_family: &str) -> VisualApplied<'_> {
        VisualApplied { font_family }
    }

    /// **本 PR が存在する理由の不変条件**（spec 決定 4 の効果）: 1 つの snapshot の中で、
    /// 行高（metrics）と文字サイズ（row）が**同じ font_size** から導かれる。別々に lock を
    /// 取っていた頃は、間に config 適用が挟まると新 font を旧行高で描く 1 フレームが生じえた。
    #[test]
    fn metrics_and_row_sizes_derive_from_the_same_font_size() {
        let mut v = cfg();
        v.font_size = 24;
        let s = visual_snapshot(&v, true, applied_none(&v.font_family));
        assert_eq!(s.row.name_size, 24.0);
        assert_eq!(s.row.path_size, layout::path_size(24) as f32);
        assert_eq!(s.row.status_size, layout::status_size(24) as f32);
        let m = Metrics::from_config(24, v.row_padding, v.bar_padding);
        assert_eq!(s.metrics.row_height, m.row_height);
        assert_eq!(s.metrics.bar_height, m.bar_height);
    }

    /// 妥当な hex は期待どおりの色になり、parse 失敗は既定色へ落ちる（既定リテラルの
    /// 再手打ちを持たない）。**`view.rs` の `hex_color_parses_and_falls_back` の後継**——
    /// 変換が view から純粋核へ移ったため、証明する命題ごとここへ移した（#673）。
    #[test]
    fn hex_parses_valid_and_falls_back_to_config_default() {
        let mut v = cfg();
        v.text_color = "#E0E0E0".into();
        let ok = visual_snapshot(&v, true, applied_none(""));
        assert_eq!(ok.row.name_color, egui::Color32::from_rgb(0xE0, 0xE0, 0xE0));

        v.text_color = "not-a-color".into();
        let bad = visual_snapshot(&v, true, applied_none(""));
        let d = VisualConfig::default();
        assert_eq!(bad.row.name_color, egui::Color32::from_hex(&d.text_color).unwrap());
    }

    /// font_family は変化フレームだけ clone する（毎フレームの String 確保を避ける）。
    #[test]
    fn font_family_reported_only_when_changed() {
        let v = cfg();
        let same = visual_snapshot(&v, true, applied_none(&v.font_family));
        assert!(same.font_family_changed.is_none());
        let diff = visual_snapshot(&v, true, applied_none("Other"));
        assert_eq!(diff.font_family_changed.as_deref(), Some(v.font_family.as_str()));
    }

    /// 背景色の parse は **1 本**（`Color32::from_hex`）になり、`#RGB` も通る（#680 の 1）。
    /// 旧 `config_watcher::parse_hex_color` は `#RRGGBB` 厳格で、`#FFF` を書くと下地だけ
    /// 既定色へ落ちていた——その非対称がここで消える。
    #[test]
    fn background_color_accepts_short_hex_and_falls_back_to_config_default() {
        assert_eq!(background_color("#FFF"), egui::Color32::WHITE);
        assert_eq!(background_color("#4A2B5C"), egui::Color32::from_rgb(0x4A, 0x2B, 0x5C));
        assert_eq!(background_color("#ffffff"), egui::Color32::WHITE);
        // 旧 `config_watcher::parse_hex_color` の 5 テストが固定していた命題（撤去に伴い移設）:
        // 不正形はいずれも既定色へ落ちる。**リテラルを再手打ちせず既定から導く**。
        let fallback = egui::Color32::from_hex(&VisualConfig::default().background_color).unwrap();
        for bad in ["282828", "#28282", "#gggggg", "", "#", "not-a-color"] {
            assert_eq!(background_color(bad), fallback, "input: {bad:?}");
        }
    }

    /// ネイティブブラシは alpha を 255 に固定する。softbuffer の clear color が alpha を
    /// 持てないため、**下地と定常の背景が食い違わないよう捨てる側へ揃える**（spec 決定 4）。
    #[test]
    fn native_brush_forces_opaque_alpha() {
        let c = native_brush_color(egui::Color32::from_rgba_premultiplied(0x12, 0x34, 0x56, 0x80));
        assert_eq!((c.0, c.1, c.2, c.3), (0x12, 0x34, 0x56, 0xff));
    }
}
