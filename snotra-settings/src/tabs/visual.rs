use eframe::egui;
use egui::Color32;
use snotra_core::config::{Config, CustomTheme, ThemePreset};

use crate::i18n::Tr;

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
    egui::ScrollArea::vertical().scroll_source(egui::scroll_area::ScrollSource { drag: false, ..Default::default() }).show(ui, |ui| {
        ui.spacing_mut().interact_size.y = 24.0;

        // -- Theme presets --
        ui.heading(tr.heading_theme());
        ui.add_space(4.0);

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
                let is_active = config.visual.preset == ThemePreset::Custom
                    && custom_theme_matches(config, ct);
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
        if !matches_any && ui.button(tr.btn_save_custom_theme()).clicked() {
            config.visual.custom_theme = Some(CustomTheme {
                background_color: config.visual.background_color.clone(),
                input_background_color: config.visual.input_background_color.clone(),
                text_color: config.visual.text_color.clone(),
                selected_row_color: config.visual.selected_row_color.clone(),
                hint_text_color: config.visual.hint_text_color.clone(),
            });
            config.visual.preset = ThemePreset::Custom;
        }

        ui.add_space(12.0);

        // -- Colors --
        ui.heading(tr.heading_color());
        ui.add_space(4.0);

        egui::Grid::new("color_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            color_row(ui, tr.label_bg_color(), &mut config.visual.background_color);
            color_row(ui, tr.label_input_bg(), &mut config.visual.input_background_color);
            color_row(ui, tr.label_text_color(), &mut config.visual.text_color);
            color_row(ui, tr.label_selected_row(), &mut config.visual.selected_row_color);
            color_row(ui, tr.label_hint_text(), &mut config.visual.hint_text_color);
        });

        ui.add_space(12.0);

        // -- Font --
        ui.heading(tr.heading_font());
        ui.add_space(4.0);

        egui::Grid::new("font_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label(tr.label_font_family());
            egui::ComboBox::from_id_salt("font_family")
                .selected_text(&config.visual.font_family)
                .show_ui(ui, |ui| {
                    for name in fonts {
                        ui.selectable_value(&mut config.visual.font_family, name.clone(), name);
                    }
                });
            ui.end_row();

            ui.label(tr.label_font_size());
            ui.add_sized([60.0, ui.spacing().interact_size.y], egui::DragValue::new(&mut config.visual.font_size).range(8..=48));
            ui.end_row();
        });

        ui.add_space(12.0);

        // -- Window --
        ui.heading(tr.heading_appearance());
        ui.add_space(4.0);

        egui::Grid::new("window_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label(tr.label_max_results());
            ui.add_sized([60.0, ui.spacing().interact_size.y], egui::DragValue::new(&mut config.appearance.max_results).range(1..=50));
            ui.end_row();

            ui.label(tr.label_window_width());
            ui.horizontal(|ui| {
                ui.add_sized([60.0, ui.spacing().interact_size.y], egui::DragValue::new(&mut config.appearance.window_width).range(300..=1200));
                ui.label("px");
            });
            ui.end_row();
        });

        ui.checkbox(&mut config.appearance.show_icons, tr.cb_show_icons());
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
        let resp = ui.add(egui::TextEdit::singleline(hex).desired_width(80.0));
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
    theme_card(ui, tr.label_custom_theme(), &colors, active)
}

fn theme_card(ui: &mut egui::Ui, label: &str, colors: &[&str; 5], active: bool) -> egui::Response {
    let total_width = SWATCH_SIZE * 5.0 + 4.0 * 4.0; // 5 swatches + 4 gaps
    let card_width = total_width + 8.0; // padding
    let card_height = SWATCH_SIZE + 24.0; // swatch + label

    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(card_width, card_height),
        egui::Sense::click(),
    );

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
            let swatch_rect =
                egui::Rect::from_min_size(egui::pos2(x, swatch_y), egui::vec2(SWATCH_SIZE, SWATCH_SIZE));
            painter.rect_filled(swatch_rect, 2.0, color);
            painter.rect_stroke(swatch_rect, 2.0, (1.0, Color32::GRAY), egui::StrokeKind::Outside);
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
