use eframe::egui;
use snotra_core::config::{Config, Language};

use crate::hotkey_input::{self, HotkeyInputState};
use crate::i18n::Tr;

pub fn ui(ui: &mut egui::Ui, config: &mut Config, hotkey_state: &mut HotkeyInputState, tr: &Tr) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        // Consistent row height so checkboxes and DragValue inputs align vertically
        ui.spacing_mut().interact_size.y = 24.0;

        // -- Language --
        ui.heading(tr.heading_language());
        ui.add_space(4.0);

        egui::Grid::new("language_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
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

        ui.add_space(12.0);

        // -- Hotkey --
        ui.heading(tr.heading_hotkey());
        ui.add_space(4.0);

        egui::Grid::new("hotkey_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label(tr.label_hotkey());
            hotkey_input::hotkey_input(ui, &mut config.hotkey, hotkey_state, tr);
            ui.end_row();
        });

        ui.checkbox(
            &mut config.general.hotkey_toggle,
            tr.cb_hotkey_toggle(),
        );

        ui.add_space(12.0);

        // -- Appearance --
        ui.heading(tr.heading_appearance());
        ui.add_space(4.0);

        egui::Grid::new("display_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
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

        ui.add_space(12.0);

        // -- Behavior --
        ui.heading(tr.heading_behavior());
        ui.add_space(4.0);

        ui.checkbox(&mut config.general.show_on_startup, tr.cb_show_on_startup());
        ui.checkbox(
            &mut config.general.auto_hide_on_focus_lost,
            tr.cb_auto_hide(),
        );
        ui.checkbox(&mut config.general.show_tray_icon, tr.cb_tray_icon());
        ui.checkbox(
            &mut config.general.ime_off_on_show,
            tr.cb_ime_off(),
        );
    });
}
