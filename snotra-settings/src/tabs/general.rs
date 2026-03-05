use eframe::egui;
use snotra_core::config::Config;

use crate::hotkey_input::{self, HotkeyInputState};

pub fn ui(ui: &mut egui::Ui, config: &mut Config, hotkey_state: &mut HotkeyInputState) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        // Consistent row height so checkboxes and DragValue inputs align vertically
        ui.spacing_mut().interact_size.y = 24.0;
        // -- Hotkey --
        ui.heading("ホットキー");
        ui.add_space(4.0);

        egui::Grid::new("hotkey_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("ホットキー:");
            hotkey_input::hotkey_input(ui, &mut config.hotkey, hotkey_state);
            ui.end_row();
        });

        ui.checkbox(
            &mut config.general.hotkey_toggle,
            "ホットキーで表示中のウィンドウを非表示にする",
        );

        ui.add_space(12.0);

        // -- Appearance --
        ui.heading("外観");
        ui.add_space(4.0);

        egui::Grid::new("display_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("最大表示件数:");
            ui.add_sized([60.0, ui.spacing().interact_size.y], egui::DragValue::new(&mut config.appearance.max_results).range(1..=50));
            ui.end_row();

            ui.label("ウィンドウ幅:");
            ui.horizontal(|ui| {
                ui.add_sized([60.0, ui.spacing().interact_size.y], egui::DragValue::new(&mut config.appearance.window_width).range(300..=1200));
                ui.label("px");
            });
            ui.end_row();
        });

        ui.checkbox(&mut config.appearance.show_icons, "アイコンを表示");

        ui.add_space(12.0);

        // -- Behavior --
        ui.heading("動作");
        ui.add_space(4.0);

        ui.checkbox(&mut config.general.show_on_startup, "起動時にウィンドウを表示");
        ui.checkbox(
            &mut config.general.auto_hide_on_focus_lost,
            "フォーカス喪失時に非表示",
        );
        ui.checkbox(&mut config.general.show_tray_icon, "トレイアイコンを表示");
        ui.checkbox(
            &mut config.general.ime_off_on_show,
            "表示時に IME をオフにする",
        );
    });
}
