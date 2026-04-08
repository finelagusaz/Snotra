use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui;
use snotra_core::config::{Config, ScanPath};

use crate::i18n::Tr;

/// Non-blocking file/folder picker state (rfd + thread spawn pattern)
#[derive(Clone, Default)]
pub struct PickerState {
    pub result: Arc<Mutex<Option<Option<PathBuf>>>>,
    pub active: bool,
}

#[derive(Default)]
pub struct IndexTabState {
    pub picker: PickerState,
    modal: ModalState,
}

#[derive(Default)]
struct ModalState {
    open: bool,
    mode: ModalMode,
    editing_index: Option<usize>,
    edit_path: String,
    edit_extensions: String,
    edit_include_folders: bool,
}

#[derive(Default, PartialEq)]
enum ModalMode {
    #[default]
    Create,
    Edit,
}

impl ModalState {
    fn open_create(&mut self) {
        self.open = true;
        self.mode = ModalMode::Create;
        self.editing_index = None;
        self.edit_path.clear();
        self.edit_extensions.clear();
        self.edit_include_folders = false;
    }

    fn open_edit(&mut self, index: usize, scan: &ScanPath) {
        self.open = true;
        self.mode = ModalMode::Edit;
        self.editing_index = Some(index);
        self.edit_path = scan.path.clone();
        self.edit_extensions = scan.extensions.join(", ");
        self.edit_include_folders = scan.include_folders;
    }

    fn close(&mut self) {
        self.open = false;
        self.editing_index = None;
    }
}

fn parse_extensions(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn ui(ui: &mut egui::Ui, ctx: &egui::Context, config: &mut Config, state: &mut IndexTabState, tr: &Tr) {
    // Poll picker result
    if state.picker.active
        && let Ok(mut guard) = state.picker.result.try_lock()
        && let Some(result) = guard.take()
    {
        state.picker.active = false;
        if let Some(path) = result {
            state.modal.edit_path = path.display().to_string();
        }
    }

    egui::ScrollArea::vertical().auto_shrink([false, false]).scroll_source(egui::scroll_area::ScrollSource { drag: false, ..Default::default() }).show(ui, |ui| {
        ui.spacing_mut().interact_size.y = 24.0;

        ui.heading(tr.heading_scan_targets());
        ui.add_space(4.0);

        if config.paths.scan.is_empty() {
            ui.label(tr.label_no_scan_paths());
        }

        // List scan paths
        let mut action: Option<ListAction> = None;
        for (i, scan) in config.paths.scan.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(&scan.path);
                    let meta = if scan.include_folders {
                        format!("{} {}", scan.extensions.join(", "), tr.label_incl_folders())
                    } else {
                        scan.extensions.join(", ")
                    };
                    ui.label(egui::RichText::new(meta).small().color(crate::app::TEXT_SECONDARY));
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tr.btn_edit()).clicked() {
                        action = Some(ListAction::Edit(i));
                    }
                });
            });
            ui.separator();
        }

        if ui.button(tr.btn_add()).clicked() {
            action = Some(ListAction::OpenCreate);
        }

        // Apply action after iteration
        match action {
            Some(ListAction::OpenCreate) => state.modal.open_create(),
            Some(ListAction::Edit(i)) => {
                let scan = config.paths.scan[i].clone();
                state.modal.open_edit(i, &scan);
            }
            None => {}
        }
    });

    // Modal
    if state.modal.open {
        show_modal(ctx, config, state, tr);
    }
}

enum ListAction {
    OpenCreate,
    Edit(usize),
}

fn show_modal(ctx: &egui::Context, config: &mut Config, state: &mut IndexTabState, tr: &Tr) {
    let title = if state.modal.mode == ModalMode::Edit {
        tr.modal_edit_scan_path()
    } else {
        tr.modal_add_scan_path()
    };

    let modal = egui::Modal::new(egui::Id::new("index_modal"));

    let resp = modal.show(ctx, |ui| {
        ui.heading(title);
        ui.separator();
        ui.add_space(4.0);

        // Path input + browse button
        ui.label(tr.label_path());
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut state.modal.edit_path);
            if ui
                .add_enabled(!state.picker.active, egui::Button::new(tr.btn_browse()))
                .clicked()
            {
                state.picker.active = true;
                let result = Arc::clone(&state.picker.result);
                let repaint_ctx = ctx.clone();
                let dialog_title = tr.dialog_select_folder().to_string();
                std::thread::spawn(move || {
                    let path = rfd::FileDialog::new()
                        .set_title(&dialog_title)
                        .pick_folder();
                    *result.lock().unwrap() = Some(path);
                    repaint_ctx.request_repaint();
                });
            }
        });

        ui.add_space(4.0);
        ui.label(tr.label_extensions());
        ui.text_edit_singleline(&mut state.modal.edit_extensions);

        ui.add_space(4.0);
        ui.checkbox(&mut state.modal.edit_include_folders, tr.cb_include_folders());

        ui.add_space(8.0);
        ui.separator();

        ui.horizontal(|ui| {
            // Delete button (edit mode only)
            if state.modal.mode == ModalMode::Edit
                && ui
                    .add(egui::Button::new(
                        egui::RichText::new(tr.btn_delete()).color(egui::Color32::from_rgb(196, 43, 28)),
                    ))
                    .clicked()
            {
                if let Some(idx) = state.modal.editing_index
                    && idx < config.paths.scan.len()
                {
                    config.paths.scan.remove(idx);
                }
                state.modal.close();
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr.btn_cancel()).clicked() {
                    state.modal.close();
                }
                if ui.button(tr.btn_save()).clicked() {
                    save_scan_path(config, &state.modal);
                    state.modal.close();
                }
            });
        });
    });

    if resp.should_close() {
        state.modal.close();
    }
}

fn save_scan_path(config: &mut Config, modal: &ModalState) {
    let path = modal.edit_path.clone();
    let extensions = parse_extensions(&modal.edit_extensions);
    let include_folders = modal.edit_include_folders;

    let new_entry = ScanPath {
        path,
        extensions,
        include_folders,
    };

    if let Some(idx) = modal.editing_index {
        if idx < config.paths.scan.len() {
            config.paths.scan[idx] = new_entry;
        }
    } else {
        config.paths.scan.push(new_entry);
    }
}
