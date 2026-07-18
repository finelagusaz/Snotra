//! 検索設定タブ（検索モード・履歴・隠しファイル）。

use eframe::egui;
use snotra_core::config::{Config, SearchHistoryNormalizationConfig, SearchModeConfig};

use crate::i18n::{Tr, TrKey};
use crate::style;

pub fn ui(ui: &mut egui::Ui, config: &mut Config, tr: &Tr) {
    style::tab_scroll_area(ui, |ui| {
        // -- Search mode --
        style::section_heading(ui, tr.t(TrKey::HeadingSearchMode));

        style::settings_grid("search_mode_grid").show(ui, |ui| {
            ui.label(tr.t(TrKey::LabelNormalMode));
            search_mode_combo(ui, "normal_mode", &mut config.search.normal_mode, tr);
            ui.end_row();

            ui.label(tr.t(TrKey::LabelFolderMode));
            search_mode_combo(ui, "folder_mode", &mut config.search.folder_mode, tr);
            ui.end_row();
        });

        style::section_gap(ui);

        // -- Visibility --
        style::section_heading(ui, tr.t(TrKey::HeadingVisibility));
        ui.checkbox(&mut config.search.show_hidden_system, tr.t(TrKey::CbShowHiddenSystem));

        style::section_gap(ui);

        // -- PATH executables --
        style::section_heading(ui, tr.t(TrKey::HeadingPathEnv));
        ui.checkbox(&mut config.search.include_path_env, tr.t(TrKey::CbIncludePathEnv));

        style::section_gap(ui);

        // -- History --
        style::section_heading(ui, tr.t(TrKey::HeadingHistory));

        style::settings_grid("history_grid").show(ui, |ui| {
            ui.label(tr.t(TrKey::LabelResultLimit));
            let result_limit_default = config.search.effective_result_limit();
            ui.add_sized(
                [style::FIELD_NUMERIC, ui.spacing().interact_size.y],
                egui::DragValue::new(config.search.result_limit.get_or_insert(result_limit_default))
                    .range(10..=1000),
            );
            ui.end_row();
            ui.label(tr.t(TrKey::LabelRecentLimit));
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
        style::section_heading(ui, tr.t(TrKey::HeadingHistoryScore));

        let cap_enabled =
            config.search.history_normalization != SearchHistoryNormalizationConfig::Disabled;
        style::settings_grid("history_score_grid").show(ui, |ui| {
            ui.label(tr.t(TrKey::LabelNormalization));
            history_normalization_combo(ui, &mut config.search.history_normalization, tr);
            ui.end_row();

            ui.label(tr.t(TrKey::LabelFuzzyCapRatio));
            ui.add_enabled_ui(cap_enabled, |ui| {
                ui.add_sized(
                    [style::FIELD_NUMERIC, ui.spacing().interact_size.y],
                    egui::DragValue::new(&mut config.search.fuzzy_history_cap_ratio)
                        .range(0.0..=1.0)
                        .speed(0.05)
                        .min_decimals(2),
                )
            })
            .inner
            .on_disabled_hover_text(tr.t(TrKey::TooltipFuzzyCapDisabled));
            ui.end_row();
        });

        style::section_gap(ui);

        // -- Migemo 検索 --
        style::section_heading(ui, tr.t(TrKey::HeadingMigemo));

        ui.checkbox(&mut config.search.migemo_enabled, tr.t(TrKey::CbMigemoEnabled));
        style::hint(ui, tr.t(TrKey::HintMigemo));

        ui.add_space(style::SPACE_HINT);

        style::settings_grid("migemo_grid").show(ui, |ui| {
            ui.label(tr.t(TrKey::LabelMigemoMinChars));
            ui.add_enabled_ui(config.search.migemo_enabled, |ui| {
                ui.add_sized(
                    [style::FIELD_NUMERIC, ui.spacing().interact_size.y],
                    egui::DragValue::new(&mut config.search.migemo_min_chars).range(1..=10),
                )
            })
            .inner
            .on_disabled_hover_text(tr.t(TrKey::TooltipMigemoDisabled));
            ui.end_row();
        });
    });
}

fn search_mode_combo(ui: &mut egui::Ui, id: &str, mode: &mut SearchModeConfig, tr: &Tr) {
    let label = match mode {
        SearchModeConfig::Prefix => tr.t(TrKey::SearchModePrefix),
        SearchModeConfig::Substring => tr.t(TrKey::SearchModeSubstring),
        SearchModeConfig::Fuzzy => tr.t(TrKey::SearchModeFuzzy),
    };
    egui::ComboBox::from_id_salt(id)
        .selected_text(label)
        .show_ui(ui, |ui| {
            ui.selectable_value(mode, SearchModeConfig::Prefix, tr.t(TrKey::SearchModePrefix));
            ui.selectable_value(mode, SearchModeConfig::Substring, tr.t(TrKey::SearchModeSubstring));
            ui.selectable_value(mode, SearchModeConfig::Fuzzy, tr.t(TrKey::SearchModeFuzzy));
        });
}

fn history_normalization_combo(
    ui: &mut egui::Ui,
    norm: &mut SearchHistoryNormalizationConfig,
    tr: &Tr,
) {
    let label = match norm {
        SearchHistoryNormalizationConfig::Disabled => tr.t(TrKey::NormalizationDisabled),
        SearchHistoryNormalizationConfig::FuzzyRelativeCap => tr.t(TrKey::NormalizationFuzzyRelativeCap),
    };
    egui::ComboBox::from_id_salt("history_normalization")
        .selected_text(label)
        .show_ui(ui, |ui| {
            ui.selectable_value(norm, SearchHistoryNormalizationConfig::Disabled, tr.t(TrKey::NormalizationDisabled));
            ui.selectable_value(
                norm,
                SearchHistoryNormalizationConfig::FuzzyRelativeCap,
                tr.t(TrKey::NormalizationFuzzyRelativeCap),
            );
        });
}
