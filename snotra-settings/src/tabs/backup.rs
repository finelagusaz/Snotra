use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui;
use snotra_core::config::Config;

use crate::app::{config_error_message, TEXT_SECONDARY};
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

    egui::ScrollArea::vertical().auto_shrink([false, false]).scroll_source(egui::scroll_area::ScrollSource { drag: false, ..Default::default() }).show(ui, |ui| {
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

/// Truncate to first line. Used for I/O errors which never contain newlines.
fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or(s)
}

/// Format a toml parse error string for display.
/// - En: returns the full error string as-is (already contains line numbers and context).
/// - Ja: prepends a Japanese summary (with line number) before the full English original.
fn localize_toml_error(msg: &str, tr: &crate::i18n::Tr) -> String {
    use snotra_core::config::Language;
    if tr.0 == Language::En {
        return msg.to_string();
    }

    // Extract line number from first line: "TOML parse error at line N, column M"
    let line_num: Option<u32> = msg
        .lines()
        .next()
        .and_then(|first| first.split("at line ").nth(1))
        .and_then(|rest| rest.split([',', ' ']).next())
        .and_then(|n| n.trim().parse().ok());

    // Last non-empty, non-visual-context line is the error description.
    // Visual context lines start with '|' (after trimming); numbered source lines
    // like "3 | key value" do not, but they appear before the description.
    let desc = msg
        .lines()
        .rfind(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with('|') && !t.starts_with('^')
        })
        .unwrap_or(msg);

    let ja_summary: String = if desc.contains("missing field") {
        match extract_backtick(desc) {
            Some(field) => tr.err_toml_missing_field(field),
            None => tr.err_toml_parse_error().to_string(),
        }
    } else if desc.contains("invalid type") {
        tr.err_toml_invalid_type().to_string()
    } else {
        tr.err_toml_parse_error().to_string()
    };

    match line_num {
        Some(n) => format!("行 {}: {}\n\n{}", n, ja_summary, msg),
        None => format!("{}\n\n{}", ja_summary, msg),
    }
}

/// Extract the first backtick-quoted word: `` `target` `` → `"target"`.
fn extract_backtick(s: &str) -> Option<&str> {
    let start = s.find('`')? + 1;
    let end = s[start..].find('`')? + start;
    Some(&s[start..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use snotra_core::config::Language;

    fn tr_ja() -> crate::i18n::Tr {
        crate::i18n::Tr(Language::Ja)
    }

    fn tr_en() -> crate::i18n::Tr {
        crate::i18n::Tr(Language::En)
    }

    #[test]
    fn localize_toml_error_en_returns_input_unchanged() {
        let msg = "TOML parse error at line 3, column 5\n  |\n3 | bad\n  | ^\nexpected '='";
        assert_eq!(localize_toml_error(msg, &tr_en()), msg);
    }

    #[test]
    fn localize_toml_error_ja_missing_field_extracts_name_and_line() {
        let msg = "TOML parse error at line 1, column 1\n  |\n1 | [search]\n  | ^\nmissing field `hotkey`";
        let out = localize_toml_error(msg, &tr_ja());
        assert!(out.starts_with("行 1:"), "should start with line number: {}", out);
        assert!(out.contains("\"hotkey\" が必要です"), "should contain field name: {}", out);
        assert!(out.contains(msg), "should include original English: {}", out);
    }

    #[test]
    fn localize_toml_error_ja_invalid_type_with_line() {
        let msg = "TOML parse error at line 2, column 15\n  |\n2 | max_results = \"ten\"\n  |               ^^^^^\ninvalid type: string \"ten\", expected u32";
        let out = localize_toml_error(msg, &tr_ja());
        assert!(out.starts_with("行 2:"), "should start with line number: {}", out);
        assert!(out.contains("値の型が違います"), "should contain type error message: {}", out);
        assert!(out.contains(msg), "should include original English: {}", out);
    }

    #[test]
    fn localize_toml_error_ja_syntax_error_fallback() {
        let msg = "TOML parse error at line 5, column 3\n  |\n5 | key value\n  |   ^\nexpected '='";
        let out = localize_toml_error(msg, &tr_ja());
        assert!(out.starts_with("行 5:"), "should start with line number: {}", out);
        assert!(out.contains("構文エラー"), "should contain generic message: {}", out);
    }

    #[test]
    fn localize_toml_error_ja_no_line_number_falls_back_gracefully() {
        let msg = "some unknown error without line info";
        let out = localize_toml_error(msg, &tr_ja());
        assert!(!out.starts_with("行"), "should not have line prefix: {}", out);
        assert!(out.contains("構文エラー"), "should contain generic message: {}", out);
        assert!(out.contains(msg), "should include original: {}", out);
    }

    #[test]
    fn extract_backtick_extracts_first_word() {
        assert_eq!(extract_backtick("missing field `hotkey`"), Some("hotkey"));
        assert_eq!(extract_backtick("missing field `target`"), Some("target"));
        assert_eq!(extract_backtick("no backticks here"), None);
    }
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
            return (Some(format!("{}{}", tr.status_import_failed(), localize_toml_error(&e, tr))), true, None);
        }
    };
    // Apply the same migrations as Config::load() (legacy field migration, normalization, etc.)
    config.apply_migrations();
    let errors = config.validate();
    if !errors.is_empty() {
        return (Some(format!("{}{}", tr.status_import_validation_error(), config_error_message(&errors[0], tr))), true, None);
    }
    if let Err(e) = config.save() {
        return (Some(format!("{}{}", tr.status_import_failed(), first_line(&e))), true, None);
    }
    (Some(tr.status_import_success().to_string()), false, Some(config))
}
