use eframe::egui;
use snotra_core::config::{Config, InstantCommand};

use crate::i18n::Tr;

#[derive(Default)]
pub struct InstantTabState {
    modal: ModalState,
}

#[derive(Default)]
struct ModalState {
    open: bool,
    mode: ModalMode,
    editing_index: Option<usize>,
    edit_name: String,
    edit_command: String,
    edit_description: String,
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
        self.edit_command.clear();
        self.edit_description.clear();
    }

    fn open_create_from(&mut self, cmd: &InstantCommand) {
        self.open = true;
        self.mode = ModalMode::Create;
        self.editing_index = None;
        self.edit_name = cmd.name.clone();
        self.edit_command = cmd.command.clone();
        self.edit_description = cmd.description.clone();
    }

    fn open_edit(&mut self, index: usize, cmd: &InstantCommand) {
        self.open = true;
        self.mode = ModalMode::Edit;
        self.editing_index = Some(index);
        self.edit_name = cmd.name.clone();
        self.edit_command = cmd.command.clone();
        self.edit_description = cmd.description.clone();
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

        let mut action: Option<InstantAction> = None;
        let len = config.instant_commands.len();
        for (i, cmd) in config.instant_commands.iter().enumerate() {
            ui.horizontal(|ui| {
                // Move up/down
                ui.vertical(|ui| {
                    if ui
                        .add_enabled(i > 0, egui::Button::new("▲").small())
                        .clicked()
                    {
                        action = Some(InstantAction::MoveUp(i));
                    }
                    if ui
                        .add_enabled(i < len - 1, egui::Button::new("▼").small())
                        .clicked()
                    {
                        action = Some(InstantAction::MoveDown(i));
                    }
                });

                ui.vertical(|ui| {
                    ui.label(if cmd.name.is_empty() {
                        tr.label_no_name()
                    } else {
                        &cmd.name
                    });
                    if !cmd.description.is_empty() {
                        ui.label(
                            egui::RichText::new(&cmd.description)
                                .small()
                                .color(crate::app::TEXT_SECONDARY),
                        );
                    }
                    ui.label(
                        egui::RichText::new(&cmd.command)
                            .small()
                            .color(crate::app::TEXT_SECONDARY),
                    );
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tr.btn_edit()).clicked() {
                        action = Some(InstantAction::Edit(i));
                    }
                    if ui.button(tr.btn_duplicate()).clicked() {
                        action = Some(InstantAction::Duplicate(i));
                    }
                });
            });
            ui.separator();
        }

        if ui.button(tr.btn_add()).clicked() {
            action = Some(InstantAction::OpenCreate);
        }

        match action {
            Some(InstantAction::OpenCreate) => state.modal.open_create(),
            Some(InstantAction::Edit(i)) => {
                let cmd = &config.instant_commands[i];
                state.modal.open_edit(i, cmd);
            }
            Some(InstantAction::Duplicate(i)) => {
                let cmd = &config.instant_commands[i];
                state.modal.open_create_from(cmd);
            }
            Some(InstantAction::MoveUp(i)) => {
                if i > 0 {
                    config.instant_commands.swap(i, i - 1);
                }
            }
            Some(InstantAction::MoveDown(i)) => {
                if i < len - 1 {
                    config.instant_commands.swap(i, i + 1);
                }
            }
            None => {}
        }
    });

    if state.modal.open {
        show_modal(ctx, config, state, tr);
    }
}

enum InstantAction {
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

        // Command
        ui.label(tr.label_instant_command());
        ui.text_edit_singleline(&mut state.modal.edit_command);
        ui.label(
            egui::RichText::new(tr.hint_instant_command())
                .small()
                .color(crate::app::TEXT_SECONDARY),
        );

        // Preview
        if !state.modal.edit_command.is_empty() {
            ui.add_space(4.0);
            let preview = snotra_core::instant::expand_instant_command(
                &state.modal.edit_command,
                "example",
                "(clipboard)",
            );
            ui.label(tr.label_instant_preview());
            ui.label(
                egui::RichText::new(&preview)
                    .small()
                    .color(crate::app::TEXT_SECONDARY),
            );
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
                if let Some(idx) = state.modal.editing_index {
                    if idx < config.instant_commands.len() {
                        config.instant_commands.remove(idx);
                    }
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
    let cmd = InstantCommand {
        name: modal.edit_name.clone(),
        command: modal.edit_command.clone(),
        description: modal.edit_description.clone(),
    };

    if let Some(idx) = modal.editing_index {
        if idx < config.instant_commands.len() {
            config.instant_commands[idx] = cmd;
        }
    } else {
        config.instant_commands.push(cmd);
    }
}
