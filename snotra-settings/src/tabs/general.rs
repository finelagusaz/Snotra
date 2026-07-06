use eframe::egui;
use snotra_core::config::{Config, Language};

use crate::hotkey_input::{self, HotkeyInputState};
use crate::i18n::{Tr, TrKey};
use crate::style;

pub fn ui(ui: &mut egui::Ui, config: &mut Config, hotkey_state: &mut HotkeyInputState, tr: &Tr) {
    style::tab_scroll_area(ui, |ui| {
        // -- Language --
        style::section_heading(ui, tr.t(TrKey::HeadingLanguage));

        style::settings_grid("language_grid").show(ui, |ui| {
            ui.label(tr.t(TrKey::LabelLanguage));
            let lang_label = match config.general.language {
                Language::Ja => tr.t(TrKey::LanguageJa),
                Language::En => tr.t(TrKey::LanguageEn),
            };
            egui::ComboBox::from_id_salt("language")
                .selected_text(lang_label)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut config.general.language, Language::Ja, tr.t(TrKey::LanguageJa));
                    ui.selectable_value(&mut config.general.language, Language::En, tr.t(TrKey::LanguageEn));
                });
            ui.end_row();
        });

        style::section_gap(ui);

        // -- Hotkey --
        style::section_heading(ui, tr.t(TrKey::HeadingHotkey));

        style::settings_grid("hotkey_grid").show(ui, |ui| {
            ui.label(tr.t(TrKey::LabelHotkey));
            hotkey_input::hotkey_input(ui, &mut config.hotkey, hotkey_state, tr);
            ui.end_row();
        });

        ui.checkbox(&mut config.general.hotkey_toggle, tr.t(TrKey::CbHotkeyToggle));

        style::section_gap(ui);

        // -- Behavior --
        style::section_heading(ui, tr.t(TrKey::HeadingBehavior));

        ui.checkbox(&mut config.general.show_on_startup, tr.t(TrKey::CbShowOnStartup));
        ui.checkbox(&mut config.general.auto_hide_on_focus_lost, tr.t(TrKey::CbAutoHide));
        ui.checkbox(&mut config.general.show_tray_icon, tr.t(TrKey::CbTrayIcon));
        ui.checkbox(&mut config.general.ime_off_on_show, tr.t(TrKey::CbImeOff));
        ui.checkbox(
            &mut config.general.follow_cursor_monitor,
            tr.t(TrKey::CbFollowCursorMonitor),
        );

        style::section_gap(ui);

        // -- Auto Update --
        style::section_heading(ui, tr.t(TrKey::HeadingAutoUpdate));

        style::settings_grid("auto_update_grid").show(ui, |ui| {
            use snotra_core::config::AutoUpdateMode;
            let selected_label = match config.general.auto_update {
                AutoUpdateMode::Full => tr.t(TrKey::AutoUpdateFull),
                AutoUpdateMode::CheckOnly => tr.t(TrKey::AutoUpdateCheckOnly),
                AutoUpdateMode::Disabled => tr.t(TrKey::AutoUpdateDisabled),
            };
            egui::ComboBox::from_id_salt("auto_update_mode")
                .selected_text(selected_label)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut config.general.auto_update,
                        AutoUpdateMode::Full,
                        tr.t(TrKey::AutoUpdateFull),
                    );
                    ui.selectable_value(
                        &mut config.general.auto_update,
                        AutoUpdateMode::CheckOnly,
                        tr.t(TrKey::AutoUpdateCheckOnly),
                    );
                    ui.selectable_value(
                        &mut config.general.auto_update,
                        AutoUpdateMode::Disabled,
                        tr.t(TrKey::AutoUpdateDisabled),
                    );
                });
            ui.end_row();
        });
    });
}
