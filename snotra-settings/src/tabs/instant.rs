use std::sync::Arc;

use eframe::egui;
use snotra_core::config::{Config, InstantAction, InstantCommand};

use crate::i18n::Tr;
use crate::style;
use crate::tabs::opener::ExePickerState;

#[derive(Default)]
pub struct InstantTabState {
    modal: ModalState,
    exe_picker: ExePickerState,
}

#[derive(Default, PartialEq, Clone, Copy)]
enum EditKind {
    #[default]
    Url,
    Program,
}

#[derive(Default)]
struct ModalState {
    open: bool,
    mode: ModalMode,
    editing_index: Option<usize>,
    edit_name: String,
    edit_description: String,
    edit_kind: EditKind,
    edit_url: String,
    edit_exe: String,
    edit_args: String,
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
        self.edit_name.clear();
        self.edit_description.clear();
        self.edit_kind = EditKind::Url;
        self.edit_url.clear();
        self.edit_exe.clear();
        self.edit_args.clear();
    }

    fn open_create_from(&mut self, cmd: &InstantCommand) {
        self.open = true;
        self.mode = ModalMode::Create;
        self.editing_index = None;
        self.load_action(cmd);
    }

    fn open_edit(&mut self, index: usize, cmd: &InstantCommand) {
        self.open = true;
        self.mode = ModalMode::Edit;
        self.editing_index = Some(index);
        self.load_action(cmd);
    }

    fn load_action(&mut self, cmd: &InstantCommand) {
        self.edit_name = cmd.name.clone();
        self.edit_description = cmd.description.clone();
        self.edit_url.clear();
        self.edit_exe.clear();
        self.edit_args.clear();
        match &cmd.action {
            InstantAction::Url { url } => {
                self.edit_kind = EditKind::Url;
                self.edit_url = url.clone();
            }
            InstantAction::Exec { exe, args } => {
                self.edit_kind = EditKind::Program;
                self.edit_exe = exe.clone();
                self.edit_args = args.clone();
            }
            InstantAction::Legacy { command } => {
                self.edit_kind = EditKind::Url;
                self.edit_url = command.clone();
            }
        }
    }

    fn close(&mut self) {
        self.open = false;
        self.editing_index = None;
    }
}

pub fn ui(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    config: &mut Config,
    state: &mut InstantTabState,
    tr: &Tr,
) {
    // Poll exe picker result（opener タブと同型の非同期ピッカーパターン）
    if state.exe_picker.active
        && let Ok(mut guard) = state.exe_picker.result.try_lock()
        && let Some(result) = guard.take()
    {
        state.exe_picker.active = false;
        if let Some(path) = result {
            state.modal.edit_exe = path.display().to_string();
        }
    }

    style::tab_scroll_area(ui, |ui| {
        // Prefix setting
        style::section_heading(ui, tr.heading_instant_prefix());
        style::settings_grid("instant_prefix_grid").show(ui, |ui| {
            ui.label(tr.label_instant_prefix());
            ui.add(
                egui::TextEdit::singleline(&mut config.search.instant_command_prefix)
                    .desired_width(style::FIELD_NUMERIC),
            );
            ui.end_row();
        });
        style::hint(ui, tr.hint_instant_prefix());

        style::section_gap(ui);

        // Command list
        style::section_heading(ui, tr.heading_instant_commands());
        style::hint(ui, tr.instant_description());
        ui.add_space(style::SPACE_GROUP);

        if config.instant_commands.is_empty() {
            style::hint(ui, tr.label_no_instant_commands());
        }

        let mut action: Option<RowAction> = None;
        let len = config.instant_commands.len();
        for (i, cmd) in config.instant_commands.iter().enumerate() {
            style::list_item(
                ui,
                |ui| {
                    ui.label(if cmd.name.is_empty() { tr.label_no_name() } else { &cmd.name });
                    if !cmd.description.is_empty() {
                        style::hint(ui, &cmd.description);
                    }
                    let (display, suspect_legacy) = match &cmd.action {
                        InstantAction::Url { url } => (
                            url.clone(),
                            !url.starts_with("http://")
                                && !url.starts_with("https://")
                                && url.contains(' '),
                        ),
                        InstantAction::Exec { exe, args } => (
                            if args.is_empty() {
                                exe.clone()
                            } else {
                                format!("{exe} {args}")
                            },
                            false,
                        ),
                        InstantAction::Legacy { command } => (
                            command.clone(),
                            !command.starts_with("http://")
                                && !command.starts_with("https://")
                                && command.contains(' '),
                        ),
                    };
                    style::hint(ui, &display);
                    if suspect_legacy {
                        ui.label(
                            egui::RichText::new(format!("⚠ {}", tr.hint_instant_migrate()))
                                .small()
                                .color(style::STATUS_WARNING),
                        );
                    }
                },
                |ui| {
                    if ui.button(tr.btn_edit()).clicked() {
                        action = Some(RowAction::Edit(i));
                    }
                    if ui.button(tr.btn_duplicate()).clicked() {
                        action = Some(RowAction::Duplicate(i));
                    }
                    match style::reorder_controls(ui, i > 0, i + 1 < len) {
                        Some(style::ReorderDir::Up) => action = Some(RowAction::MoveUp(i)),
                        Some(style::ReorderDir::Down) => action = Some(RowAction::MoveDown(i)),
                        None => {}
                    }
                },
            );
        }

        if ui.button(tr.btn_add()).clicked() {
            action = Some(RowAction::OpenCreate);
        }

        match action {
            Some(RowAction::OpenCreate) => state.modal.open_create(),
            Some(RowAction::Edit(i)) => {
                let cmd = &config.instant_commands[i];
                state.modal.open_edit(i, cmd);
            }
            Some(RowAction::Duplicate(i)) => {
                let cmd = &config.instant_commands[i];
                state.modal.open_create_from(cmd);
            }
            Some(RowAction::MoveUp(i)) if i > 0 => {
                config.instant_commands.swap(i, i - 1);
            }
            Some(RowAction::MoveUp(_)) => {}
            Some(RowAction::MoveDown(i)) if i < len - 1 => {
                config.instant_commands.swap(i, i + 1);
            }
            Some(RowAction::MoveDown(_)) => {}
            None => {}
        }
    });

    if state.modal.open {
        show_modal(ctx, config, state, tr);
    }
}

enum RowAction {
    OpenCreate,
    Edit(usize),
    Duplicate(usize),
    MoveUp(usize),
    MoveDown(usize),
}

fn show_modal(
    ctx: &egui::Context,
    config: &mut Config,
    state: &mut InstantTabState,
    tr: &Tr,
) {
    let title = if state.modal.mode == ModalMode::Edit {
        tr.modal_edit_instant()
    } else {
        tr.modal_add_instant()
    };

    let modal = egui::Modal::new(egui::Id::new("instant_modal"));

    let resp = modal.show(ctx, |ui| {
        style::modal_header(ui, title);

        // Name
        ui.label(tr.label_instant_name());
        ui.text_edit_singleline(&mut state.modal.edit_name);
        style::hint(ui, tr.hint_instant_name());

        ui.add_space(style::SPACE_HINT);

        // Description
        ui.label(tr.label_instant_description());
        ui.text_edit_singleline(&mut state.modal.edit_description);
        style::hint(ui, tr.hint_instant_description());

        ui.add_space(style::SPACE_HINT);

        // Kind
        ui.label(tr.label_instant_kind());
        ui.horizontal(|ui| {
            ui.radio_value(&mut state.modal.edit_kind, EditKind::Url, tr.radio_instant_url());
            ui.radio_value(
                &mut state.modal.edit_kind,
                EditKind::Program,
                tr.radio_instant_program(),
            );
        });
        ui.add_space(style::SPACE_HINT);

        match state.modal.edit_kind {
            EditKind::Url => {
                ui.label(tr.label_instant_command());
                ui.text_edit_singleline(&mut state.modal.edit_url);
                style::hint(ui, tr.hint_instant_command());
                if !state.modal.edit_url.is_empty() {
                    let preview = snotra_core::instant::expand_instant_command(
                        &state.modal.edit_url,
                        "example",
                        "(clipboard)",
                    );
                    ui.add_space(style::SPACE_HINT);
                    ui.label(tr.label_instant_preview());
                    style::hint(ui, &preview);
                }
            }
            EditKind::Program => {
                ui.label(tr.label_instant_exe());
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut state.modal.edit_exe);
                    if ui
                        .add_enabled(!state.exe_picker.active, egui::Button::new(tr.btn_browse()))
                        .clicked()
                    {
                        state.exe_picker.active = true;
                        let result = Arc::clone(&state.exe_picker.result);
                        let repaint_ctx = ctx.clone();
                        let dialog_title = tr.dialog_select_exe().to_string();
                        let exe_label = tr.filter_executables().to_string();
                        let all_label = tr.filter_all_files().to_string();
                        std::thread::spawn(move || {
                            // exe を既定フィルタで誘導しつつ、全ファイル選択も許す（.com/拡張子なし/cmd.exe 等）。
                            // 最初の add_filter が既定フィルタになる（rfd 0.17 の set_default_extension）。
                            let path = rfd::FileDialog::new()
                                .set_title(&dialog_title)
                                .add_filter(&exe_label, &["exe"])
                                .add_filter(&all_label, &["*"])
                                .pick_file();
                            *result.lock().unwrap() = Some(path);
                            repaint_ctx.request_repaint();
                        });
                    }
                });
                ui.label(tr.label_instant_args());
                ui.text_edit_singleline(&mut state.modal.edit_args);
                style::hint(ui, tr.hint_instant_program());
                if !state.modal.edit_exe.is_empty() {
                    let tokens = snotra_core::instant::expand_exec_args(
                        &state.modal.edit_args,
                        "example",
                        "(clipboard)",
                        |s| s.to_string(),
                    );
                    let preview = format!("{} {}", state.modal.edit_exe, tokens.join(" "));
                    ui.add_space(style::SPACE_HINT);
                    ui.label(tr.label_instant_preview());
                    style::hint(ui, preview.trim());
                }
            }
        }

        ui.add_space(style::SPACE_GROUP);
        ui.separator();

        ui.horizontal(|ui| {
            // Delete (edit mode only)
            if state.modal.mode == ModalMode::Edit && style::danger_button(ui, tr.btn_delete()).clicked()
            {
                if let Some(idx) = state.modal.editing_index
                    && idx < config.instant_commands.len()
                {
                    config.instant_commands.remove(idx);
                }
                state.modal.close();
            }

            let buttons = style::modal_buttons(ui, tr);
            if buttons.cancel {
                state.modal.close();
            }
            if buttons.save {
                save_instant_command(config, &state.modal);
                state.modal.close();
            }
        });
    });

    if resp.should_close() {
        state.modal.close();
    }
}

fn save_instant_command(config: &mut Config, modal: &ModalState) {
    let action = match modal.edit_kind {
        EditKind::Url => InstantAction::Url { url: modal.edit_url.clone() },
        EditKind::Program => InstantAction::Exec {
            exe: modal.edit_exe.clone(),
            args: modal.edit_args.clone(),
        },
    };
    let cmd = InstantCommand {
        name: modal.edit_name.clone(),
        description: modal.edit_description.clone(),
        action,
    };

    if let Some(idx) = modal.editing_index {
        if idx < config.instant_commands.len() {
            config.instant_commands[idx] = cmd;
        }
    } else {
        config.instant_commands.push(cmd);
    }
}
