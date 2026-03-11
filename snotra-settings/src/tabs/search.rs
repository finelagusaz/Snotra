use eframe::egui;
use eframe::egui::RichText;
use snotra_core::config::{Config, SearchHistoryNormalizationConfig, SearchModeConfig};

use crate::app::TEXT_SECONDARY;
use crate::i18n::Tr;

pub fn ui(ui: &mut egui::Ui, config: &mut Config, tr: &Tr) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.spacing_mut().interact_size.y = 24.0;

        // -- Search mode --
        ui.heading(tr.heading_search_mode());
        ui.add_space(4.0);

        egui::Grid::new("search_mode_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label(tr.label_normal_mode());
            search_mode_combo(ui, "normal_mode", &mut config.search.normal_mode, tr);
            ui.end_row();

            ui.label(tr.label_folder_mode());
            search_mode_combo(ui, "folder_mode", &mut config.search.folder_mode, tr);
            ui.end_row();
        });

        ui.add_space(12.0);

        // -- Visibility --
        ui.heading(tr.heading_visibility());
        ui.add_space(4.0);

        ui.checkbox(
            &mut config.search.show_hidden_system,
            tr.cb_show_hidden_system(),
        );

        ui.add_space(12.0);

        // -- History --
        ui.heading(tr.heading_history());
        ui.add_space(4.0);

        egui::Grid::new("history_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label(tr.label_max_history());
            ui.add_sized([60.0, ui.spacing().interact_size.y], egui::DragValue::new(&mut config.search.top_n_history).range(10..=1000));
            ui.end_row();
            ui.label(tr.label_max_history_display());
            ui.add_sized([60.0, ui.spacing().interact_size.y], egui::DragValue::new(&mut config.search.max_history_display).range(1..=50));
            ui.end_row();
        });

        ui.add_space(12.0);

        // -- History score --
        ui.heading(tr.heading_history_score());
        ui.add_space(4.0);

        let cap_enabled =
            config.search.history_normalization != SearchHistoryNormalizationConfig::Disabled;
        egui::Grid::new("history_score_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label(tr.label_normalization());
            history_normalization_combo(ui, &mut config.search.history_normalization, tr);
            ui.end_row();

            ui.label(tr.label_fuzzy_cap_ratio());
            ui.add_enabled_ui(cap_enabled, |ui| {
                ui.add_sized(
                    [60.0, ui.spacing().interact_size.y],
                    egui::DragValue::new(&mut config.search.fuzzy_history_cap_ratio)
                        .range(0.0..=1.0)
                        .speed(0.05)
                        .min_decimals(2),
                );
            });
            ui.end_row();
        });

        ui.add_space(12.0);

        // -- Migemo 検索 --
        ui.heading(tr.heading_migemo());
        ui.add_space(4.0);

        ui.checkbox(&mut config.search.migemo_enabled, tr.cb_migemo_enabled());
        ui.label(RichText::new(tr.hint_migemo()).small().color(TEXT_SECONDARY));

        ui.add_space(4.0);

        egui::Grid::new("migemo_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label(tr.label_migemo_min_chars());
            ui.add_enabled_ui(config.search.migemo_enabled, |ui| {
                ui.add_sized(
                    [60.0, ui.spacing().interact_size.y],
                    egui::DragValue::new(&mut config.search.migemo_min_chars).range(1..=10),
                );
            });
            ui.end_row();
        });
    });
}

fn search_mode_combo(ui: &mut egui::Ui, id: &str, mode: &mut SearchModeConfig, tr: &Tr) {
    let label = match mode {
        SearchModeConfig::Prefix => tr.search_mode_prefix(),
        SearchModeConfig::Substring => tr.search_mode_substring(),
        SearchModeConfig::Fuzzy => tr.search_mode_fuzzy(),
    };
    egui::ComboBox::from_id_salt(id)
        .selected_text(label)
        .show_ui(ui, |ui| {
            ui.selectable_value(mode, SearchModeConfig::Prefix, tr.search_mode_prefix());
            ui.selectable_value(mode, SearchModeConfig::Substring, tr.search_mode_substring());
            ui.selectable_value(mode, SearchModeConfig::Fuzzy, tr.search_mode_fuzzy());
        });
}

fn history_normalization_combo(
    ui: &mut egui::Ui,
    norm: &mut SearchHistoryNormalizationConfig,
    tr: &Tr,
) {
    let label = match norm {
        SearchHistoryNormalizationConfig::Disabled => tr.normalization_disabled(),
        SearchHistoryNormalizationConfig::FuzzyRelativeCap => tr.normalization_fuzzy_relative_cap(),
    };
    egui::ComboBox::from_id_salt("history_normalization")
        .selected_text(label)
        .show_ui(ui, |ui| {
            ui.selectable_value(norm, SearchHistoryNormalizationConfig::Disabled, tr.normalization_disabled());
            ui.selectable_value(
                norm,
                SearchHistoryNormalizationConfig::FuzzyRelativeCap,
                tr.normalization_fuzzy_relative_cap(),
            );
        });
}
