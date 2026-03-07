use eframe::egui;
use snotra_core::config::{Config, SearchHistoryNormalizationConfig, SearchModeConfig};

pub fn ui(ui: &mut egui::Ui, config: &mut Config) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.spacing_mut().interact_size.y = 24.0;

        // -- Search mode --
        ui.heading("検索モード");
        ui.add_space(4.0);

        egui::Grid::new("search_mode_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("通常モード:");
            search_mode_combo(ui, "normal_mode", &mut config.search.normal_mode);
            ui.end_row();

            ui.label("フォルダモード:");
            search_mode_combo(ui, "folder_mode", &mut config.search.folder_mode);
            ui.end_row();
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

        egui::Grid::new("history_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("最大列挙数:");
            ui.add_sized([60.0, ui.spacing().interact_size.y], egui::DragValue::new(&mut config.appearance.top_n_history).range(10..=1000));
            ui.end_row();
        });

        ui.add_space(12.0);

        // -- History score --
        ui.heading("履歴スコア");
        ui.add_space(4.0);

        let cap_enabled =
            config.search.history_normalization != SearchHistoryNormalizationConfig::Disabled;
        egui::Grid::new("history_score_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("正規化:");
            history_normalization_combo(ui, &mut config.search.history_normalization);
            ui.end_row();

            ui.label("Fuzzy 履歴キャップ比率:");
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
