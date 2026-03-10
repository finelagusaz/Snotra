use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui;
use snotra_core::config::Config;

use crate::app::TEXT_SECONDARY;
use crate::i18n::Tr;

/// Result type for file picker threads.
/// None = still running, Some(None) = cancelled, Some(Some(path)) = selected.
type PickerResult = Arc<Mutex<Option<Option<PathBuf>>>>;

#[derive(Default)]
pub struct BackupTabState {
    export_active: bool,
    export_result: PickerResult,
    import_active: bool,
    import_result: PickerResult,
}

/// Returned to app.rs to update status (and optionally draft/saved after import).
pub struct BackupResult {
    /// If Some, update draft and saved to this config (import success).
    pub imported_config: Option<Config>,
    pub status: String,
    pub status_timer: f64,
}

pub fn ui(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &mut BackupTabState,
    tr: &Tr,
) -> Option<BackupResult> {
    let mut result = None;

    // Poll export picker
    if state.export_active {
        if let Ok(mut guard) = state.export_result.try_lock() {
            if let Some(picker_path) = guard.take() {
                state.export_active = false;
                result = handle_export_result(picker_path, tr);
            }
        }
    }

    // Poll import picker
    if state.import_active {
        if let Ok(mut guard) = state.import_result.try_lock() {
            if let Some(picker_path) = guard.take() {
                state.import_active = false;
                result = handle_import_result(picker_path, tr);
            }
        }
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.spacing_mut().interact_size.y = 24.0;

        // Export section
        ui.heading(tr.heading_export());
        ui.add_space(4.0);
        ui.label(egui::RichText::new(tr.label_export_description()).color(TEXT_SECONDARY));
        ui.add_space(8.0);
        if ui
            .add_enabled(!state.export_active, egui::Button::new(tr.btn_export()))
            .clicked()
        {
            start_export(ctx, state, tr);
        }

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(8.0);

        // Import section
        ui.heading(tr.heading_import());
        ui.add_space(4.0);
        ui.label(egui::RichText::new(tr.label_import_description()).color(TEXT_SECONDARY));
        ui.add_space(8.0);
        if ui
            .add_enabled(!state.import_active, egui::Button::new(tr.btn_import()))
            .clicked()
        {
            start_import(ctx, state, tr);
        }

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(8.0);

        // Open folder section
        ui.heading(tr.heading_data_folder());
        ui.add_space(4.0);
        ui.label(egui::RichText::new(tr.label_data_folder_description()).color(TEXT_SECONDARY));
        if let Some(dir) = Config::config_dir() {
            ui.add_space(2.0);
            ui.label(egui::RichText::new(dir.display().to_string()).color(TEXT_SECONDARY));
        }
        ui.add_space(8.0);
        if ui.button(tr.btn_open_folder()).clicked() {
            if let Some(dir) = Config::config_dir() {
                let _ = open::that(dir);
            }
        }
    });

    result
}

fn start_export(ctx: &egui::Context, state: &mut BackupTabState, tr: &Tr) {
    state.export_active = true;
    let result = Arc::clone(&state.export_result);
    let repaint_ctx = ctx.clone();
    let dialog_title = tr.dialog_export_config().to_string();
    let filter_label = tr.filter_toml().to_string();
    let default_name = Config::default_export_filename();

    std::thread::spawn(move || {
        let path = rfd::FileDialog::new()
            .set_title(&dialog_title)
            .add_filter(&filter_label, &["toml"])
            .set_file_name(&default_name)
            .save_file();
        *result.lock().unwrap() = Some(path);
        repaint_ctx.request_repaint();
    });
}

fn start_import(ctx: &egui::Context, state: &mut BackupTabState, tr: &Tr) {
    state.import_active = true;
    let result = Arc::clone(&state.import_result);
    let repaint_ctx = ctx.clone();
    let dialog_title = tr.dialog_import_config().to_string();
    let filter_label = tr.filter_toml().to_string();

    std::thread::spawn(move || {
        let path = rfd::FileDialog::new()
            .set_title(&dialog_title)
            .add_filter(&filter_label, &["toml"])
            .pick_file();
        *result.lock().unwrap() = Some(path);
        repaint_ctx.request_repaint();
    });
}

fn handle_export_result(path: Option<PathBuf>, tr: &Tr) -> Option<BackupResult> {
    let Some(dest) = path else {
        return None; // Cancelled
    };
    let Some(src) = Config::config_path() else {
        return Some(BackupResult {
            imported_config: None,
            status: format!("{}config dir not found", tr.status_export_failed()),
            status_timer: 5.0,
        });
    };
    match std::fs::copy(&src, &dest) {
        Ok(_) => Some(BackupResult {
            imported_config: None,
            status: tr.status_export_success().to_string(),
            status_timer: 2.0,
        }),
        Err(e) => Some(BackupResult {
            imported_config: None,
            status: format!("{}{e}", tr.status_export_failed()),
            status_timer: 5.0,
        }),
    }
}

fn handle_import_result(path: Option<PathBuf>, tr: &Tr) -> Option<BackupResult> {
    let Some(src) = path else {
        return None; // Cancelled
    };
    let content = match std::fs::read_to_string(&src) {
        Ok(c) => c,
        Err(e) => {
            return Some(BackupResult {
                imported_config: None,
                status: format!("{}{e}", tr.status_import_failed()),
                status_timer: 5.0,
            });
        }
    };
    let config = match Config::from_toml_str(&content) {
        Ok(c) => c,
        Err(e) => {
            return Some(BackupResult {
                imported_config: None,
                status: format!("{}{e}", tr.status_import_failed()),
                status_timer: 5.0,
            });
        }
    };
    let errors = config.validate();
    if !errors.is_empty() {
        return Some(BackupResult {
            imported_config: None,
            status: format!("{}{:?}", tr.status_import_validation_error(), errors[0]),
            status_timer: 5.0,
        });
    }
    if let Err(e) = config.save() {
        return Some(BackupResult {
            imported_config: None,
            status: format!("{}{e}", tr.status_import_failed()),
            status_timer: 5.0,
        });
    }
    Some(BackupResult {
        imported_config: Some(config),
        status: tr.status_import_success().to_string(),
        status_timer: 2.0,
    })
}
