use std::sync::Arc;

use eframe::egui;

use crate::app::PickerState;

/// PoC: rfd folder picker integration test.
/// This will be replaced with the full Index tab UI in Phase 2.
pub fn ui(ui: &mut egui::Ui, ctx: &egui::Context, picker: &mut PickerState) {
    ui.heading("インデックス (PoC: フォルダピッカー)");
    ui.add_space(8.0);

    // Poll for picker result
    if picker.active {
        if let Ok(mut guard) = picker.result.try_lock() {
            if let Some(result) = guard.take() {
                picker.active = false;
                if let Some(path) = result {
                    ui.ctx().memory_mut(|mem| {
                        mem.data
                            .insert_temp(egui::Id::new("picked_path"), path.display().to_string());
                    });
                }
            }
        }
    }

    // Show picked path if any
    let picked: String = ui
        .ctx()
        .memory(|mem| mem.data.get_temp(egui::Id::new("picked_path")))
        .unwrap_or_default();

    if !picked.is_empty() {
        ui.label(format!("選択されたフォルダ: {picked}"));
        ui.add_space(8.0);
    }

    // Picker button (disabled while active)
    if ui
        .add_enabled(!picker.active, egui::Button::new("フォルダを選択…"))
        .clicked()
    {
        picker.active = true;
        let result = Arc::clone(&picker.result);
        let repaint_ctx = ctx.clone();
        std::thread::spawn(move || {
            let path = rfd::FileDialog::new()
                .set_title("フォルダを選択")
                .pick_folder();
            *result.lock().unwrap() = Some(path);
            repaint_ctx.request_repaint();
        });
    }

    if picker.active {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("フォルダを選択中…");
        });
    }
}
