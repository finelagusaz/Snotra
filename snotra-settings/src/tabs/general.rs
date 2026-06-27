use eframe::egui;
use snotra_core::config::{Config, Language};

use crate::hotkey_input::{self, HotkeyInputState};
use crate::i18n::Tr;
use crate::style;

pub fn ui(ui: &mut egui::Ui, config: &mut Config, hotkey_state: &mut HotkeyInputState, tr: &Tr) {
    style::tab_scroll_area(ui, |ui| {
        // -- Language --
        style::section_heading(ui, tr.heading_language());

        style::settings_grid("language_grid").show(ui, |ui| {
            ui.label(tr.label_language());
            let lang_label = match config.general.language {
                Language::Ja => tr.language_ja(),
                Language::En => tr.language_en(),
            };
            egui::ComboBox::from_id_salt("language")
                .selected_text(lang_label)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut config.general.language, Language::Ja, tr.language_ja());
                    ui.selectable_value(&mut config.general.language, Language::En, tr.language_en());
                });
            ui.end_row();
        });

        style::section_gap(ui);

        // -- Hotkey --
        style::section_heading(ui, tr.heading_hotkey());

        style::settings_grid("hotkey_grid").show(ui, |ui| {
            ui.label(tr.label_hotkey());
            hotkey_input::hotkey_input(ui, &mut config.hotkey, hotkey_state, tr);
            ui.end_row();
        });

        ui.checkbox(&mut config.general.hotkey_toggle, tr.cb_hotkey_toggle());

        style::section_gap(ui);

        // -- Behavior --
        style::section_heading(ui, tr.heading_behavior());

        ui.checkbox(&mut config.general.show_on_startup, tr.cb_show_on_startup());
        ui.checkbox(&mut config.general.auto_hide_on_focus_lost, tr.cb_auto_hide());
        ui.checkbox(&mut config.general.show_tray_icon, tr.cb_tray_icon());
        ui.checkbox(&mut config.general.ime_off_on_show, tr.cb_ime_off());
        ui.checkbox(
            &mut config.general.follow_cursor_monitor,
            tr.cb_follow_cursor_monitor(),
        );

        style::section_gap(ui);

        // -- Auto Update --
        style::section_heading(ui, tr.heading_auto_update());

        style::settings_grid("auto_update_grid").show(ui, |ui| {
            use snotra_core::config::AutoUpdateMode;
            let selected_label = match config.general.auto_update {
                AutoUpdateMode::Full => tr.auto_update_full(),
                AutoUpdateMode::CheckOnly => tr.auto_update_check_only(),
                AutoUpdateMode::Disabled => tr.auto_update_disabled(),
            };
            egui::ComboBox::from_id_salt("auto_update_mode")
                .selected_text(selected_label)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut config.general.auto_update,
                        AutoUpdateMode::Full,
                        tr.auto_update_full(),
                    );
                    ui.selectable_value(
                        &mut config.general.auto_update,
                        AutoUpdateMode::CheckOnly,
                        tr.auto_update_check_only(),
                    );
                    ui.selectable_value(
                        &mut config.general.auto_update,
                        AutoUpdateMode::Disabled,
                        tr.auto_update_disabled(),
                    );
                });
            ui.end_row();
        });
    });
}
