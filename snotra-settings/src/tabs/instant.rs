use eframe::egui;
use snotra_core::config::{Config, InstantAction, InstantCommand};

use crate::i18n::Tr;

#[derive(Default)]
pub struct InstantTabState {
    modal: ModalState,
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
    egui::ScrollArea::vertical().auto_shrink([false, false]).scroll_source(egui::scroll_area::ScrollSource { drag: false, ..Default::default() }).show(ui, |ui| {
        ui.spacing_mut().interact_size.y = 24.0;

        // Prefix setting
        ui.heading(tr.heading_instant_prefix());
        ui.add_space(4.0);
        egui::Grid::new("instant_prefix_grid")
            .num_columns(2)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                ui.label(tr.label_instant_prefix());
                ui.add(
                    egui::TextEdit::singleline(&mut config.search.instant_command_prefix)
                        .desired_width(60.0),
                );
                ui.end_row();
            });
        ui.label(
            egui::RichText::new(tr.hint_instant_prefix())
                .small()
                .color(crate::app::TEXT_SECONDARY),
        );

        ui.add_space(16.0);

        // Command list
        ui.heading(tr.heading_instant_commands());
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(tr.instant_description())
                .small()
                .color(crate::app::TEXT_SECONDARY),
        );
        ui.add_space(8.0);

        if config.instant_commands.is_empty() {
            ui.label(tr.label_no_instant_commands());
        }

        let mut action: Option<RowAction> = None;
        let len = config.instant_commands.len();
        for (i, cmd) in config.instant_commands.iter().enumerate() {
            ui.horizontal(|ui| {
                // Move up/down
                ui.vertical(|ui| {
                    if ui
                        .add_enabled(i > 0, egui::Button::new("▲").small())
                        .clicked()
                    {
                        action = Some(RowAction::MoveUp(i));
                    }
                    if ui
                        .add_enabled(i < len - 1, egui::Button::new("▼").small())
                        .clicked()
                    {
                        action = Some(RowAction::MoveDown(i));
                    }
                });

                ui.vertical(|ui| {
                    ui.label(if cmd.name.is_empty() { tr.label_no_name() } else { &cmd.name });
                    if !cmd.description.is_empty() {
                        ui.label(
                            egui::RichText::new(&cmd.description)
                                .small()
                                .color(crate::app::TEXT_SECONDARY),
                        );
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
                    ui.label(
                        egui::RichText::new(&display).small().color(crate::app::TEXT_SECONDARY),
                    );
                    if suspect_legacy {
                        ui.label(
                            egui::RichText::new(format!("⚠ {}", tr.hint_instant_migrate()))
                                .small()
                                .color(egui::Color32::from_rgb(196, 120, 28)),
                        );
                    }
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tr.btn_edit()).clicked() {
                        action = Some(RowAction::Edit(i));
                    }
                    if ui.button(tr.btn_duplicate()).clicked() {
                        action = Some(RowAction::Duplicate(i));
                    }
                });
            });
            ui.separator();
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
        ui.heading(title);
        ui.separator();
        ui.add_space(4.0);

        // Name
        ui.label(tr.label_instant_name());
        ui.text_edit_singleline(&mut state.modal.edit_name);
        ui.label(
            egui::RichText::new(tr.hint_instant_name())
                .small()
                .color(crate::app::TEXT_SECONDARY),
        );

        ui.add_space(4.0);

        // Description
        ui.label(tr.label_instant_description());
        ui.text_edit_singleline(&mut state.modal.edit_description);
        ui.label(
            egui::RichText::new(tr.hint_instant_description())
                .small()
                .color(crate::app::TEXT_SECONDARY),
        );

        ui.add_space(4.0);

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
        ui.add_space(4.0);

        match state.modal.edit_kind {
            EditKind::Url => {
                ui.label(tr.label_instant_command());
                ui.text_edit_singleline(&mut state.modal.edit_url);
                ui.label(
                    egui::RichText::new(tr.hint_instant_command())
                        .small()
                        .color(crate::app::TEXT_SECONDARY),
                );
                if !state.modal.edit_url.is_empty() {
                    let preview = snotra_core::instant::expand_instant_command(
                        &state.modal.edit_url,
                        "example",
                        "(clipboard)",
                    );
                    ui.add_space(4.0);
                    ui.label(tr.label_instant_preview());
                    ui.label(
                        egui::RichText::new(&preview)
                            .small()
                            .color(crate::app::TEXT_SECONDARY),
                    );
                }
            }
            EditKind::Program => {
                ui.label(tr.label_instant_exe());
                ui.text_edit_singleline(&mut state.modal.edit_exe);
                ui.label(tr.label_instant_args());
                ui.text_edit_singleline(&mut state.modal.edit_args);
                ui.label(
                    egui::RichText::new(tr.hint_instant_program())
                        .small()
                        .color(crate::app::TEXT_SECONDARY),
                );
                if !state.modal.edit_exe.is_empty() {
                    let tokens = snotra_core::instant::expand_exec_args(
                        &state.modal.edit_args,
                        "example",
                        "(clipboard)",
                        |s| s.to_string(),
                    );
                    let preview = format!("{} {}", state.modal.edit_exe, tokens.join(" "));
                    ui.add_space(4.0);
                    ui.label(tr.label_instant_preview());
                    ui.label(
                        egui::RichText::new(preview.trim())
                            .small()
                            .color(crate::app::TEXT_SECONDARY),
                    );
                }
            }
        }

        ui.add_space(8.0);
        ui.separator();

        ui.horizontal(|ui| {
            // Delete (edit mode only)
            if state.modal.mode == ModalMode::Edit
                && ui
                    .add(egui::Button::new(
                        egui::RichText::new(tr.btn_delete()).color(egui::Color32::from_rgb(196, 43, 28)),
                    ))
                    .clicked()
            {
                if let Some(idx) = state.modal.editing_index
                    && idx < config.instant_commands.len()
                {
                    config.instant_commands.remove(idx);
                }
                state.modal.close();
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr.btn_cancel()).clicked() {
                    state.modal.close();
                }
                if ui.button(tr.btn_save()).clicked() {
                    save_instant_command(config, &state.modal);
                    state.modal.close();
                }
            });
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
