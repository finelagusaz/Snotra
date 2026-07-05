use eframe::egui;
use snotra_core::config::{Config, InstantAction, InstantCommand};

use crate::i18n::Tr;
use crate::style;
use crate::tabs::common::{self, ModalState, PickerState};

#[derive(Default)]
pub struct InstantTabState {
    modal: ModalState<InstantFields>,
    exe_picker: PickerState,
}

#[derive(Default, PartialEq, Clone, Copy)]
enum EditKind {
    #[default]
    Url,
    Program,
}

/// インスタントコマンドモーダルのタブ固有編集フィールド。
#[derive(Default)]
struct InstantFields {
    name: String,
    description: String,
    kind: EditKind,
    url: String,
    exe: String,
    args: String,
}

impl InstantFields {
    fn from_command(cmd: &InstantCommand) -> Self {
        let mut fields = Self {
            name: cmd.name.clone(),
            description: cmd.description.clone(),
            ..Self::default()
        };
        match &cmd.action {
            InstantAction::Url { url } => {
                fields.kind = EditKind::Url;
                fields.url = url.clone();
            }
            InstantAction::Exec { exe, args } => {
                fields.kind = EditKind::Program;
                fields.exe = exe.clone();
                fields.args = args.clone();
            }
            InstantAction::Legacy { command } => {
                fields.kind = EditKind::Url;
                fields.url = command.clone();
            }
        }
        fields
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
    if let Some(Some(path)) = state.exe_picker.poll() {
        state.modal.fields.exe = path.display().to_string();
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
                let fields = InstantFields::from_command(&config.instant_commands[i]);
                state.modal.open_edit(i, fields);
            }
            Some(RowAction::Duplicate(i)) => {
                let fields = InstantFields::from_command(&config.instant_commands[i]);
                state.modal.open_create_with(fields);
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
    let title = if state.modal.is_edit() {
        tr.modal_edit_instant()
    } else {
        tr.modal_add_instant()
    };

    let modal = egui::Modal::new(egui::Id::new("instant_modal"));

    let resp = modal.show(ctx, |ui| {
        style::modal_header(ui, title);

        // Name
        ui.label(tr.label_instant_name());
        ui.text_edit_singleline(&mut state.modal.fields.name);
        style::hint(ui, tr.hint_instant_name());

        ui.add_space(style::SPACE_HINT);

        // Description
        ui.label(tr.label_instant_description());
        ui.text_edit_singleline(&mut state.modal.fields.description);
        style::hint(ui, tr.hint_instant_description());

        ui.add_space(style::SPACE_HINT);

        // Kind
        ui.label(tr.label_instant_kind());
        ui.horizontal(|ui| {
            ui.radio_value(&mut state.modal.fields.kind, EditKind::Url, tr.radio_instant_url());
            ui.radio_value(
                &mut state.modal.fields.kind,
                EditKind::Program,
                tr.radio_instant_program(),
            );
        });
        ui.add_space(style::SPACE_HINT);

        match state.modal.fields.kind {
            EditKind::Url => {
                ui.label(tr.label_instant_command());
                ui.text_edit_singleline(&mut state.modal.fields.url);
                style::hint(ui, tr.hint_instant_command());
                if !state.modal.fields.url.is_empty() {
                    let preview = snotra_core::instant::expand_instant_command(
                        &state.modal.fields.url,
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
                    ui.text_edit_singleline(&mut state.modal.fields.exe);
                    if ui
                        .add_enabled(!state.exe_picker.active, egui::Button::new(tr.btn_browse()))
                        .clicked()
                    {
                        let dialog_title = tr.dialog_select_exe().to_string();
                        let exe_label = tr.filter_executables().to_string();
                        let all_label = tr.filter_all_files().to_string();
                        state.exe_picker.launch(ctx, move || {
                            // exe を既定フィルタで誘導しつつ、全ファイル選択も許す（.com/拡張子なし/cmd.exe 等）。
                            // 最初の add_filter が既定フィルタになる（rfd 0.17 の set_default_extension）。
                            rfd::FileDialog::new()
                                .set_title(&dialog_title)
                                .add_filter(&exe_label, &["exe"])
                                .add_filter(&all_label, &["*"])
                                .pick_file()
                        });
                    }
                });
                ui.label(tr.label_instant_args());
                ui.text_edit_singleline(&mut state.modal.fields.args);
                style::hint(ui, tr.hint_instant_program());
                if !state.modal.fields.exe.is_empty() {
                    let tokens = snotra_core::instant::expand_exec_args(
                        &state.modal.fields.args,
                        "example",
                        "(clipboard)",
                        |s| s.to_string(),
                    );
                    let preview = format!("{} {}", state.modal.fields.exe, tokens.join(" "));
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
            if state.modal.is_edit() && style::danger_button(ui, tr.btn_delete()).clicked() {
                common::delete_entry(&mut config.instant_commands, state.modal.editing);
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

fn save_instant_command(config: &mut Config, modal: &ModalState<InstantFields>) {
    let action = match modal.fields.kind {
        EditKind::Url => InstantAction::Url { url: modal.fields.url.clone() },
        EditKind::Program => InstantAction::Exec {
            exe: modal.fields.exe.clone(),
            args: modal.fields.args.clone(),
        },
    };
    let cmd = InstantCommand {
        name: modal.fields.name.clone(),
        description: modal.fields.description.clone(),
        action,
    };
    common::save_entry(&mut config.instant_commands, modal.editing, cmd);
}
