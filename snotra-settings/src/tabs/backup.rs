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
    /// Inline message shown on the tab (persists until next operation).
    message: String,
    message_is_error: bool,
}

/// Returned to app.rs only when import succeeds (to update draft/saved).
pub struct BackupResult {
    pub imported_config: Config,
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
                let (msg, is_err) = handle_export_result(picker_path, tr);
                if let Some(m) = msg {
                    state.message = m;
                    state.message_is_error = is_err;
                }
            }
        }
    }

    // Poll import picker
    if state.import_active {
        if let Ok(mut guard) = state.import_result.try_lock() {
            if let Some(picker_path) = guard.take() {
                state.import_active = false;
                let (msg, is_err, config) = handle_import_result(picker_path, tr);
                if let Some(m) = msg {
                    state.message = m;
                    state.message_is_error = is_err;
                }
                if let Some(c) = config {
                    result = Some(BackupResult { imported_config: c });
                }
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
            state.message.clear();
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
            state.message.clear();
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

        // Inline message (persists until next operation)
        if !state.message.is_empty() {
            ui.add_space(16.0);
            ui.separator();
            ui.add_space(8.0);
            let color = if state.message_is_error {
                egui::Color32::from_rgb(196, 43, 28) // Red for errors
            } else {
                egui::Color32::from_rgb(16, 124, 16) // Green for success
            };
            ui.label(egui::RichText::new(&state.message).color(color));
        }
    });

    result
}

fn local_time() -> (u16, u16, u16, u16, u16) {
    let st = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    (st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute)
}

fn start_export(ctx: &egui::Context, state: &mut BackupTabState, tr: &Tr) {
    state.export_active = true;
    let result = Arc::clone(&state.export_result);
    let repaint_ctx = ctx.clone();
    let dialog_title = tr.dialog_export_config().to_string();
    let filter_label = tr.filter_toml().to_string();
    let (y, m, d, h, min) = local_time();
    let default_name = Config::export_filename(y, m, d, h, min);

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

/// Truncate to first line to avoid multi-line status bar overflow.
fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or(s)
}

/// Returns (message, is_error). None message = cancelled.
fn handle_export_result(path: Option<PathBuf>, tr: &Tr) -> (Option<String>, bool) {
    let Some(dest) = path else {
        return (None, false); // Cancelled
    };
    let Some(src) = Config::config_path() else {
        return (Some(format!("{}config dir not found", tr.status_export_failed())), true);
    };
    match std::fs::copy(&src, &dest) {
        Ok(_) => (Some(tr.status_export_success().to_string()), false),
        Err(e) => (Some(format!("{}{}", tr.status_export_failed(), first_line(&e.to_string()))), true),
    }
}

/// Returns (message, is_error, imported_config). None message = cancelled.
fn handle_import_result(path: Option<PathBuf>, tr: &Tr) -> (Option<String>, bool, Option<Config>) {
    let Some(src) = path else {
        return (None, false, None); // Cancelled
    };
    let content = match std::fs::read_to_string(&src) {
        Ok(c) => c,
        Err(e) => {
            return (Some(format!("{}{}", tr.status_import_failed(), first_line(&e.to_string()))), true, None);
        }
    };
    let mut config = match Config::from_toml_str(&content) {
        Ok(c) => c,
        Err(e) => {
            return (Some(format!("{}{}", tr.status_import_failed(), first_line(&e))), true, None);
        }
    };
    // Apply the same migrations as Config::load() (legacy field migration, normalization, etc.)
    config.apply_migrations();
    let errors = config.validate();
    if !errors.is_empty() {
        return (Some(format!("{}{:?}", tr.status_import_validation_error(), errors[0])), true, None);
    }
    if let Err(e) = config.save() {
        return (Some(format!("{}{}", tr.status_import_failed(), first_line(&e))), true, None);
    }
    (Some(tr.status_import_success().to_string()), false, Some(config))
}
