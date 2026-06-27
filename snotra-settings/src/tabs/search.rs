use eframe::egui;
use snotra_core::config::{Config, SearchHistoryNormalizationConfig, SearchModeConfig};

use crate::i18n::Tr;
use crate::style;

pub fn ui(ui: &mut egui::Ui, config: &mut Config, tr: &Tr) {
    style::tab_scroll_area(ui, |ui| {
        // -- Search mode --
        style::section_heading(ui, tr.heading_search_mode());

        style::settings_grid("search_mode_grid").show(ui, |ui| {
            ui.label(tr.label_normal_mode());
            search_mode_combo(ui, "normal_mode", &mut config.search.normal_mode, tr);
            ui.end_row();

            ui.label(tr.label_folder_mode());
            search_mode_combo(ui, "folder_mode", &mut config.search.folder_mode, tr);
            ui.end_row();
        });

        style::section_gap(ui);

        // -- Visibility --
        style::section_heading(ui, tr.heading_visibility());
        ui.checkbox(&mut config.search.show_hidden_system, tr.cb_show_hidden_system());

        style::section_gap(ui);

        // -- PATH executables --
        style::section_heading(ui, tr.heading_path_env());
        ui.checkbox(&mut config.search.include_path_env, tr.cb_include_path_env());

        style::section_gap(ui);

        // -- History --
        style::section_heading(ui, tr.heading_history());

        style::settings_grid("history_grid").show(ui, |ui| {
            ui.label(tr.label_result_limit());
            let result_limit_default = config.search.effective_result_limit();
            ui.add_sized(
                [style::FIELD_NUMERIC, ui.spacing().interact_size.y],
                egui::DragValue::new(config.search.result_limit.get_or_insert(result_limit_default))
                    .range(10..=1000),
            );
            ui.end_row();
            ui.label(tr.label_recent_limit());
            let recent_limit_default = config.search.effective_recent_limit();
            ui.add_sized(
                [style::FIELD_NUMERIC, ui.spacing().interact_size.y],
                egui::DragValue::new(config.search.recent_limit.get_or_insert(recent_limit_default))
                    .range(1..=50),
            );
            ui.end_row();
        });

        style::section_gap(ui);

        // -- History score --
        style::section_heading(ui, tr.heading_history_score());

        let cap_enabled =
            config.search.history_normalization != SearchHistoryNormalizationConfig::Disabled;
        style::settings_grid("history_score_grid").show(ui, |ui| {
            ui.label(tr.label_normalization());
            history_normalization_combo(ui, &mut config.search.history_normalization, tr);
            ui.end_row();

            ui.label(tr.label_fuzzy_cap_ratio());
            ui.add_enabled_ui(cap_enabled, |ui| {
                ui.add_sized(
                    [style::FIELD_NUMERIC, ui.spacing().interact_size.y],
                    egui::DragValue::new(&mut config.search.fuzzy_history_cap_ratio)
                        .range(0.0..=1.0)
                        .speed(0.05)
                        .min_decimals(2),
                );
            });
            ui.end_row();
        });

        style::section_gap(ui);

        // -- Migemo 検索 --
        style::section_heading(ui, tr.heading_migemo());

        ui.checkbox(&mut config.search.migemo_enabled, tr.cb_migemo_enabled());
        style::hint(ui, tr.hint_migemo());

        ui.add_space(style::SPACE_HINT);

        style::settings_grid("migemo_grid").show(ui, |ui| {
            ui.label(tr.label_migemo_min_chars());
            ui.add_enabled_ui(config.search.migemo_enabled, |ui| {
                ui.add_sized(
                    [style::FIELD_NUMERIC, ui.spacing().interact_size.y],
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
