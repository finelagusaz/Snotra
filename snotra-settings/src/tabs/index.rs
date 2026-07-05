use eframe::egui;
use snotra_core::config::{Config, ScanPath};

use crate::i18n::Tr;
use crate::style;
use crate::tabs::common::{self, ModalState, PickerState};

#[derive(Default)]
pub struct IndexTabState {
    pub picker: PickerState,
    modal: ModalState<ScanPathFields>,
}

/// スキャンパスモーダルのタブ固有編集フィールド。
#[derive(Default)]
struct ScanPathFields {
    path: String,
    extensions: String,
    include_folders: bool,
}

impl ScanPathFields {
    fn from_scan(scan: &ScanPath) -> Self {
        Self {
            path: scan.path.clone(),
            extensions: scan.extensions.join(", "),
            include_folders: scan.include_folders,
        }
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
    if let Some(Some(path)) = state.picker.poll() {
        state.modal.fields.path = path.display().to_string();
    }

    style::tab_scroll_area(ui, |ui| {
        style::section_heading(ui, tr.heading_scan_targets());

        if config.paths.scan.is_empty() {
            style::hint(ui, tr.label_no_scan_paths());
        }

        // List scan paths
        let mut action: Option<ListAction> = None;
        for (i, scan) in config.paths.scan.iter().enumerate() {
            style::list_item(
                ui,
                |ui| {
                    ui.label(&scan.path);
                    let meta = if scan.include_folders {
                        format!("{} {}", scan.extensions.join(", "), tr.label_incl_folders())
                    } else {
                        scan.extensions.join(", ")
                    };
                    style::hint(ui, &meta);
                },
                |ui| {
                    if ui.button(tr.btn_edit()).clicked() {
                        action = Some(ListAction::Edit(i));
                    }
                },
            );
        }

        if ui.button(tr.btn_add()).clicked() {
            action = Some(ListAction::OpenCreate);
        }

        // Apply action after iteration
        match action {
            Some(ListAction::OpenCreate) => state.modal.open_create(),
            Some(ListAction::Edit(i)) => {
                let fields = ScanPathFields::from_scan(&config.paths.scan[i]);
                state.modal.open_edit(i, fields);
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
    let title = if state.modal.is_edit() {
        tr.modal_edit_scan_path()
    } else {
        tr.modal_add_scan_path()
    };

    let modal = egui::Modal::new(egui::Id::new("index_modal"));

    let resp = modal.show(ctx, |ui| {
        style::modal_header(ui, title);

        // Path input + browse button
        ui.label(tr.label_path());
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut state.modal.fields.path);
            if ui
                .add_enabled(!state.picker.active, egui::Button::new(tr.btn_browse()))
                .clicked()
            {
                let dialog_title = tr.dialog_select_folder().to_string();
                state.picker.launch(ctx, move || {
                    rfd::FileDialog::new().set_title(&dialog_title).pick_folder()
                });
            }
        });

        ui.add_space(style::SPACE_HINT);
        ui.label(tr.label_extensions());
        ui.text_edit_singleline(&mut state.modal.fields.extensions);

        ui.add_space(style::SPACE_HINT);
        ui.checkbox(&mut state.modal.fields.include_folders, tr.cb_include_folders());

        ui.add_space(style::SPACE_GROUP);
        ui.separator();

        ui.horizontal(|ui| {
            // Delete button (edit mode only)
            if state.modal.is_edit() && style::danger_button(ui, tr.btn_delete()).clicked() {
                common::delete_entry(&mut config.paths.scan, state.modal.editing);
                state.modal.close();
            }

            let buttons = style::modal_buttons(ui, tr);
            if buttons.cancel {
                state.modal.close();
            }
            if buttons.save {
                save_scan_path(config, &state.modal);
                state.modal.close();
            }
        });
    });

    if resp.should_close() {
        state.modal.close();
    }
}

fn save_scan_path(config: &mut Config, modal: &ModalState<ScanPathFields>) {
    let new_entry = ScanPath {
        path: modal.fields.path.clone(),
        extensions: parse_extensions(&modal.fields.extensions),
        include_folders: modal.fields.include_folders,
    };
    common::save_entry(&mut config.paths.scan, modal.editing, new_entry);
}
