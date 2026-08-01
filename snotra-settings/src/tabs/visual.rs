//! ビジュアル設定タブ（テーマプリセット・カラーピッカー・フォント）。

use eframe::egui;
use egui::Color32;
use snotra_core::config::{Config, CustomTheme, ThemePreset};

use crate::i18n::{Tr, TrKey};
use crate::style;

struct PresetDef {
    preset: ThemePreset,
    label: &'static str,
    bg: &'static str,
    input_bg: &'static str,
    text: &'static str,
    selected: &'static str,
    hint: &'static str,
}

const PRESETS: &[PresetDef] = &[
    PresetDef {
        preset: ThemePreset::Obsidian,
        label: "Obsidian",
        bg: "#282828",
        input_bg: "#383838",
        text: "#E0E0E0",
        selected: "#505050",
        hint: "#808080",
    },
    PresetDef {
        preset: ThemePreset::Paper,
        label: "Paper",
        bg: "#ffffff",
        input_bg: "#f2f2f2",
        text: "#111111",
        selected: "#d0d0d0",
        hint: "#666666",
    },
    PresetDef {
        preset: ThemePreset::Solarized,
        label: "Solarized",
        bg: "#002b36",
        input_bg: "#073642",
        text: "#839496",
        selected: "#073642",
        hint: "#586e75",
    },
    PresetDef {
        preset: ThemePreset::Monokai,
        label: "Monokai",
        bg: "#272822",
        input_bg: "#3e3d32",
        text: "#f8f8f2",
        selected: "#49483e",
        hint: "#75715e",
    },
];

const SWATCH_SIZE: f32 = 16.0;

pub fn ui(ui: &mut egui::Ui, config: &mut Config, fonts: &[String], tr: &Tr) {
    style::tab_scroll_area(ui, |ui| {
        // -- Theme presets --
        style::section_heading(ui, tr.t(TrKey::HeadingTheme));

        ui.horizontal_wrapped(|ui| {
            for p in PRESETS {
                let is_active = preset_matches(config, p);
                let response = preset_card(ui, p, is_active);
                if response.clicked() {
                    apply_preset(config, p);
                }
            }

            // Custom theme card
            if let Some(ct) = &config.visual.custom_theme {
                let is_active =
                    config.visual.preset == ThemePreset::Custom && custom_theme_matches(config, ct);
                let ct_clone = ct.clone();
                let response = custom_theme_card(ui, &ct_clone, is_active, tr);
                if response.clicked() {
                    apply_custom_theme(config, &ct_clone);
                }
            }
        });

        // Save custom theme button
        let matches_any = PRESETS.iter().any(|p| preset_matches(config, p))
            || config
                .visual
                .custom_theme
                .as_ref()
                .is_some_and(|ct| custom_theme_matches(config, ct));
        if !matches_any && ui.button(tr.t(TrKey::BtnSaveCustomTheme)).clicked() {
            config.visual.custom_theme = Some(CustomTheme {
                background_color: config.visual.background_color.clone(),
                input_background_color: config.visual.input_background_color.clone(),
                text_color: config.visual.text_color.clone(),
                selected_row_color: config.visual.selected_row_color.clone(),
                hint_text_color: config.visual.hint_text_color.clone(),
            });
            config.visual.preset = ThemePreset::Custom;
        }

        style::section_gap(ui);

        // -- Colors --
        style::section_heading(ui, tr.t(TrKey::HeadingColor));

        style::settings_grid("color_grid").show(ui, |ui| {
            color_row(
                ui,
                tr.t(TrKey::LabelBgColor),
                &mut config.visual.background_color,
            );
            color_row(
                ui,
                tr.t(TrKey::LabelInputBg),
                &mut config.visual.input_background_color,
            );
            color_row(
                ui,
                tr.t(TrKey::LabelTextColor),
                &mut config.visual.text_color,
            );
            color_row(
                ui,
                tr.t(TrKey::LabelSelectedRow),
                &mut config.visual.selected_row_color,
            );
            color_row(
                ui,
                tr.t(TrKey::LabelHintText),
                &mut config.visual.hint_text_color,
            );
        });

        style::section_gap(ui);

        // -- Font --
        style::section_heading(ui, tr.t(TrKey::HeadingFont));

        style::settings_grid("font_grid").show(ui, |ui| {
            ui.label(tr.t(TrKey::LabelFontFamily));
            egui::ComboBox::from_id_salt("font_family")
                .selected_text(&config.visual.font_family)
                .show_ui(ui, |ui| {
                    for name in fonts {
                        ui.selectable_value(&mut config.visual.font_family, name.clone(), name);
                    }
                });
            ui.end_row();

            ui.label(tr.t(TrKey::LabelFontSize));
            ui.add_sized(
                [style::FIELD_NUMERIC, ui.spacing().interact_size.y],
                egui::DragValue::new(&mut config.visual.font_size).range(8..=48),
            );
            ui.end_row();
        });

        style::section_gap(ui);

        // -- Window --
        style::section_heading(ui, tr.t(TrKey::HeadingAppearance));

        style::settings_grid("window_grid").show(ui, |ui| {
            ui.label(tr.t(TrKey::LabelVisibleRows));
            let visible_default = config.appearance.effective_visible_rows();
            ui.add_sized(
                [style::FIELD_NUMERIC, ui.spacing().interact_size.y],
                egui::DragValue::new(
                    config
                        .appearance
                        .visible_rows
                        .get_or_insert(visible_default),
                )
                .range(1..=50),
            );
            ui.end_row();

            ui.label(tr.t(TrKey::LabelWindowWidth));
            ui.horizontal(|ui| {
                ui.add_sized(
                    [style::FIELD_NUMERIC, ui.spacing().interact_size.y],
                    egui::DragValue::new(&mut config.appearance.window_width).range(300..=1200),
                );
                ui.label("px");
            });
            ui.end_row();
        });

        ui.checkbox(&mut config.appearance.show_icons, tr.t(TrKey::CbShowIcons));
    });
}

fn color_row(ui: &mut egui::Ui, label: &str, hex: &mut String) {
    ui.label(label);

    ui.horizontal(|ui| {
        // Color swatch button
        let mut color = Color32::from_hex(hex).unwrap_or(Color32::BLACK);
        if egui::widgets::color_picker::color_edit_button_srgba(
            ui,
            &mut color,
            egui::widgets::color_picker::Alpha::Opaque,
        )
        .changed()
        {
            *hex = format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b());
        }

        // Hex text input
        let resp = ui.add(egui::TextEdit::singleline(hex).desired_width(style::FIELD_HEX));
        if resp.lost_focus() {
            // Validate on focus loss
            if Color32::from_hex(hex).is_err() {
                // Revert to black if invalid
                *hex = "#000000".to_string();
            }
        }
    });

    ui.end_row();
}

fn preset_matches(config: &Config, p: &PresetDef) -> bool {
    config.visual.background_color.eq_ignore_ascii_case(p.bg)
        && config
            .visual
            .input_background_color
            .eq_ignore_ascii_case(p.input_bg)
        && config.visual.text_color.eq_ignore_ascii_case(p.text)
        && config
            .visual
            .selected_row_color
            .eq_ignore_ascii_case(p.selected)
        && config.visual.hint_text_color.eq_ignore_ascii_case(p.hint)
}

fn custom_theme_matches(config: &Config, ct: &CustomTheme) -> bool {
    config
        .visual
        .background_color
        .eq_ignore_ascii_case(&ct.background_color)
        && config
            .visual
            .input_background_color
            .eq_ignore_ascii_case(&ct.input_background_color)
        && config
            .visual
            .text_color
            .eq_ignore_ascii_case(&ct.text_color)
        && config
            .visual
            .selected_row_color
            .eq_ignore_ascii_case(&ct.selected_row_color)
        && config
            .visual
            .hint_text_color
            .eq_ignore_ascii_case(&ct.hint_text_color)
}

fn apply_preset(config: &mut Config, p: &PresetDef) {
    config.visual.preset = p.preset;
    config.visual.background_color = p.bg.to_string();
    config.visual.input_background_color = p.input_bg.to_string();
    config.visual.text_color = p.text.to_string();
    config.visual.selected_row_color = p.selected.to_string();
    config.visual.hint_text_color = p.hint.to_string();
}

fn apply_custom_theme(config: &mut Config, ct: &CustomTheme) {
    config.visual.preset = ThemePreset::Custom;
    config.visual.background_color = ct.background_color.clone();
    config.visual.input_background_color = ct.input_background_color.clone();
    config.visual.text_color = ct.text_color.clone();
    config.visual.selected_row_color = ct.selected_row_color.clone();
    config.visual.hint_text_color = ct.hint_text_color.clone();
}

fn preset_card(ui: &mut egui::Ui, p: &PresetDef, active: bool) -> egui::Response {
    let colors = [p.bg, p.input_bg, p.text, p.selected, p.hint];
    theme_card(ui, p.label, &colors, active)
}

fn custom_theme_card(ui: &mut egui::Ui, ct: &CustomTheme, active: bool, tr: &Tr) -> egui::Response {
    let colors = [
        ct.background_color.as_str(),
        ct.input_background_color.as_str(),
        ct.text_color.as_str(),
        ct.selected_row_color.as_str(),
        ct.hint_text_color.as_str(),
    ];
    theme_card(ui, tr.t(TrKey::LabelCustomTheme), &colors, active)
}

fn theme_card(ui: &mut egui::Ui, label: &str, colors: &[&str; 5], active: bool) -> egui::Response {
    let total_width = SWATCH_SIZE * 5.0 + 4.0 * 4.0; // 5 swatches + 4 gaps
    let card_width = total_width + 8.0; // padding
    let card_height = SWATCH_SIZE + 24.0; // swatch + label

    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(card_width, card_height), egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();

        // Card background
        let bg = if active {
            ui.visuals().selection.bg_fill
        } else if response.hovered() {
            ui.visuals().widgets.hovered.bg_fill
        } else {
            ui.visuals().widgets.inactive.bg_fill
        };
        painter.rect_filled(rect, 4.0, bg);

        // Swatches
        let swatch_y = rect.min.y + 4.0;
        for (i, hex) in colors.iter().enumerate() {
            let color = Color32::from_hex(hex).unwrap_or(Color32::BLACK);
            let x = rect.min.x + 4.0 + i as f32 * (SWATCH_SIZE + 4.0);
            let swatch_rect = egui::Rect::from_min_size(
                egui::pos2(x, swatch_y),
                egui::vec2(SWATCH_SIZE, SWATCH_SIZE),
            );
            painter.rect_filled(swatch_rect, 2.0, color);
            painter.rect_stroke(
                swatch_rect,
                2.0,
                (1.0, Color32::GRAY),
                egui::StrokeKind::Outside,
            );
        }

        // Label
        let label_pos = egui::pos2(rect.min.x + 4.0, swatch_y + SWATCH_SIZE + 2.0);
        painter.text(
            label_pos,
            egui::Align2::LEFT_TOP,
            label,
            egui::TextStyle::Body.resolve(ui.style()),
            ui.visuals().text_color(),
        );
    }

    response
}

#[cfg(test)]
mod tests {
    use super::{PRESETS, PresetDef, preset_matches};
    use snotra_core::config::{Config, ThemePreset};

    /// 既定 config は Obsidian プリセットと一致していなければならない（#795 群 10）。
    ///
    /// **置換では消せない写しなのでテストで固定する**（型を変えない判断の理由は
    /// `docs/adr/ADR-config-default-fallback-references.md`）。
    ///
    /// **UI と同じ述語（`preset_matches`）を通すのが要点である。** カードの強調は
    /// `preset_matches` の結果そのもの（`is_active`）で決まり、色 5 本を
    /// `eq_ignore_ascii_case` で比べる。`assert_eq!` で 1 本ずつ比較すると
    /// **UI が守っている不変条件より厳しい主張**になる（大文字小文字の違いだけで落ちる）。
    ///
    /// 壊れると: まっさらな初回起動で**どのプリセットカードも強調されず**、かつ
    /// 「カスタムテーマとして保存」ボタンが最初から出る。
    #[test]
    fn default_config_matches_obsidian_preset() {
        let obsidian = PRESETS
            .iter()
            .find(|p| p.preset == ThemePreset::Obsidian)
            .expect("Obsidian プリセットが PRESETS から消えた");
        assert!(
            preset_matches(&Config::default(), obsidian),
            "既定 config が Obsidian プリセットと一致しない。既定色を変えるなら Obsidian \
             プリセットも同じ変更で直すこと（snotra-core の VisualConfig::default() と \
             本ファイルの PRESETS）"
        );

        // **網羅性ガード**（`..` を使わないこと・`app.rs` の `field_mutations` と同型）:
        // `PresetDef` にフィールドが増えるとここがコンパイルエラーになり、下の変異を
        // 足すまで検出が続く。これが無いと、6 色目を足しても変異が当たらないまま緑になる。
        let PresetDef {
            preset: _,
            label: _,
            bg: _,
            input_bg: _,
            text: _,
            selected: _,
            hint: _,
        } = obsidian;

        // 上の assert が空虚に真でないことを、**複製に変異を当てて**確かめる
        // （`.claude/rules/safety-nets.md`「稼働中のガードを弱めず複製に変異を当てる」）。
        // **5 色すべてに当てる**——1 本だけだと、残り 4 本が述語の `&&` から抜け落ちても緑を通る。
        let drifts = |name: &str, set: fn(&mut Config)| {
            let mut drifted = Config::default();
            set(&mut drifted);
            assert!(
                !preset_matches(&drifted, obsidian),
                "{name} を変えても一致してしまう——preset_matches がその色を見ていない"
            );
        };
        drifts("background_color", |c| {
            c.visual.background_color = "#123456".to_string()
        });
        drifts("input_background_color", |c| {
            c.visual.input_background_color = "#123456".to_string()
        });
        drifts("text_color", |c| {
            c.visual.text_color = "#123456".to_string()
        });
        drifts("selected_row_color", |c| {
            c.visual.selected_row_color = "#123456".to_string()
        });
        drifts("hint_text_color", |c| {
            c.visual.hint_text_color = "#123456".to_string()
        });
    }
}
