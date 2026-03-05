use eframe::egui;
use snotra_core::config::{Config, SearchHistoryNormalizationConfig, SearchModeConfig};

pub fn ui(ui: &mut egui::Ui, config: &mut Config) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.spacing_mut().interact_size.y = 24.0;

        // -- Search mode --
        ui.heading("検索モード");
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("通常モード:");
            search_mode_combo(ui, "normal_mode", &mut config.search.normal_mode);
        });

        ui.horizontal(|ui| {
            ui.label("フォルダモード:");
            search_mode_combo(ui, "folder_mode", &mut config.search.folder_mode);
        });

        ui.add_space(12.0);

        // -- Visibility --
        ui.heading("表示");
        ui.add_space(4.0);

        ui.checkbox(
            &mut config.search.show_hidden_system,
            "隠しファイル・システムファイルを表示",
        );

        ui.add_space(12.0);

        // -- History --
        ui.heading("履歴");
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("履歴保持件数:");
            ui.add(egui::DragValue::new(&mut config.appearance.top_n_history).range(10..=1000));
        });

        ui.add_space(12.0);

        // -- History score --
        ui.heading("履歴スコア");
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("正規化:");
            history_normalization_combo(ui, &mut config.search.history_normalization);
        });

        let cap_enabled =
            config.search.history_normalization != SearchHistoryNormalizationConfig::Disabled;
        ui.horizontal(|ui| {
            ui.label("Fuzzy 履歴キャップ比率:");
            ui.add_enabled(
                cap_enabled,
                egui::DragValue::new(&mut config.search.fuzzy_history_cap_ratio)
                    .range(0.0..=1.0)
                    .speed(0.05),
            );
        });
    });
}

fn search_mode_combo(ui: &mut egui::Ui, id: &str, mode: &mut SearchModeConfig) {
    let label = match mode {
        SearchModeConfig::Prefix => "前方一致",
        SearchModeConfig::Substring => "部分一致",
        SearchModeConfig::Fuzzy => "あいまい",
    };
    egui::ComboBox::from_id_salt(id)
        .selected_text(label)
        .show_ui(ui, |ui| {
            ui.selectable_value(mode, SearchModeConfig::Prefix, "前方一致");
            ui.selectable_value(mode, SearchModeConfig::Substring, "部分一致");
            ui.selectable_value(mode, SearchModeConfig::Fuzzy, "あいまい");
        });
}

fn history_normalization_combo(
    ui: &mut egui::Ui,
    norm: &mut SearchHistoryNormalizationConfig,
) {
    let label = match norm {
        SearchHistoryNormalizationConfig::Disabled => "無効",
        SearchHistoryNormalizationConfig::FuzzyRelativeCap => "Fuzzy 相対キャップ",
    };
    egui::ComboBox::from_id_salt("history_normalization")
        .selected_text(label)
        .show_ui(ui, |ui| {
            ui.selectable_value(norm, SearchHistoryNormalizationConfig::Disabled, "無効");
            ui.selectable_value(
                norm,
                SearchHistoryNormalizationConfig::FuzzyRelativeCap,
                "Fuzzy 相対キャップ",
            );
        });
}
