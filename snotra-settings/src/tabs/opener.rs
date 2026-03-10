use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui;
use snotra_core::config::{self, Config, OpenerRule, OpenerTool};

use crate::i18n::Tr;

/// Non-blocking file picker state for exe selection
#[derive(Clone, Default)]
pub struct ExePickerState {
    pub result: Arc<Mutex<Option<Option<PathBuf>>>>,
    pub active: bool,
}

pub struct OpenerTabState {
    pub exe_picker: ExePickerState,
    modal: ModalState,
    presets: Vec<config::OpenerPreset>,
}

impl OpenerTabState {
    pub fn new() -> Self {
        Self {
            exe_picker: ExePickerState::default(),
            modal: ModalState::default(),
            presets: config::detect_opener_presets(),
        }
    }
}

#[derive(Default, PartialEq)]
enum TargetKind {
    #[default]
    Folder,
    Extension,
}

#[derive(Default)]
struct ModalState {
    open: bool,
    mode: ModalMode,
    editing_rule: Option<usize>,
    editing_tool: Option<usize>,
    edit_target_kind: TargetKind,
    edit_target_ext: String,
    edit_tool_name: String,
    edit_tool_exe: String,
    edit_tool_args: String,
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
        self.editing_rule = None;
        self.editing_tool = None;
        self.edit_target_kind = TargetKind::Folder;
        self.edit_target_ext.clear();
        self.edit_tool_name.clear();
        self.edit_tool_exe.clear();
        self.edit_tool_args.clear();
    }

    fn open_edit(&mut self, rule_idx: usize, tool_idx: usize, rule: &OpenerRule, tool: &OpenerTool) {
        self.open = true;
        self.mode = ModalMode::Edit;
        self.editing_rule = Some(rule_idx);
        self.editing_tool = Some(tool_idx);
        if let Some(ext) = rule.target.strip_prefix("ext:") {
            self.edit_target_kind = TargetKind::Extension;
            self.edit_target_ext = ext.to_string();
        } else {
            self.edit_target_kind = TargetKind::Folder;
            self.edit_target_ext.clear();
        }
        self.edit_tool_name = tool.name.clone();
        self.edit_tool_exe = tool.exe.clone();
        self.edit_tool_args = tool.args.clone();
    }

    fn close(&mut self) {
        self.open = false;
        self.editing_rule = None;
        self.editing_tool = None;
    }
}

pub fn ui(ui: &mut egui::Ui, ctx: &egui::Context, config: &mut Config, state: &mut OpenerTabState, tr: &Tr) {
    // Poll exe picker result
    if state.exe_picker.active {
        if let Ok(mut guard) = state.exe_picker.result.try_lock() {
            if let Some(result) = guard.take() {
                state.exe_picker.active = false;
                if let Some(path) = result {
                    state.modal.edit_tool_exe = path.display().to_string();
                }
            }
        }
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.spacing_mut().interact_size.y = 24.0;

        ui.heading(tr.heading_opener_rules());
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(tr.opener_description())
                .small()
                .color(crate::app::TEXT_SECONDARY),
        );
        ui.add_space(8.0);

        if config.openers.is_empty() {
            ui.label(tr.label_no_rules());
        }

        // Flatten rules into (rule_idx, tool_idx, target, tool) for display
        let mut flat: Vec<(usize, usize, String, OpenerTool)> = Vec::new();
        for (ri, rule) in config.openers.iter().enumerate() {
            for (ti, tool) in rule.tools.iter().enumerate() {
                flat.push((ri, ti, rule.target.clone(), tool.clone()));
            }
        }

        let mut action: Option<OpenerAction> = None;
        let len = flat.len();
        for (fi, (ri, ti, target, tool)) in flat.iter().enumerate() {
            ui.horizontal(|ui| {
                // Move up/down
                ui.vertical(|ui| {
                    if ui
                        .add_enabled(fi > 0, egui::Button::new("▲").small())
                        .clicked()
                    {
                        action = Some(OpenerAction::MoveUp(fi));
                    }
                    if ui
                        .add_enabled(fi < len - 1, egui::Button::new("▼").small())
                        .clicked()
                    {
                        action = Some(OpenerAction::MoveDown(fi));
                    }
                });

                ui.vertical(|ui| {
                    let target_label = if target == "folder" {
                        tr.label_target_folder().to_string()
                    } else {
                        target.clone()
                    };
                    ui.label(format!(
                        "[{}] {}",
                        target_label,
                        if tool.name.is_empty() {
                            tr.label_no_name()
                        } else {
                            &tool.name
                        }
                    ));
                    ui.label(
                        egui::RichText::new(&tool.exe)
                            .small()
                            .color(crate::app::TEXT_SECONDARY),
                    );
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tr.btn_edit()).clicked() {
                        action = Some(OpenerAction::Edit(*ri, *ti));
                    }
                });
            });
            ui.separator();
        }

        if ui.button(tr.btn_add()).clicked() {
            action = Some(OpenerAction::OpenCreate);
        }

        // Presets section
        if !state.presets.is_empty() {
            ui.add_space(12.0);
            ui.heading(tr.heading_presets());
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(tr.preset_description())
                    .small()
                    .color(crate::app::TEXT_SECONDARY),
            );
            ui.add_space(4.0);

            let mut preset_action: Option<usize> = None;
            for (i, preset) in state.presets.iter().enumerate() {
                let already_added = config::is_preset_already_added(&config.openers, &preset.exe);
                ui.horizontal(|ui| {
                    ui.label(preset.name);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if already_added {
                            ui.add_enabled(false, egui::Button::new(tr.label_already_added()));
                        } else if ui.button(tr.btn_add_preset()).clicked() {
                            preset_action = Some(i);
                        }
                    });
                });
            }

            if let Some(idx) = preset_action {
                let preset = &state.presets[idx];
                let tool = OpenerTool {
                    name: preset.name.to_string(),
                    exe: preset.exe.clone(),
                    args: preset.args.to_string(),
                };
                let target = preset.target.to_string();
                if let Some(rule) = config.openers.iter_mut().find(|r| r.target == target) {
                    rule.tools.push(tool);
                } else {
                    config.openers.push(OpenerRule {
                        target,
                        tools: vec![tool],
                    });
                }
            }
        }

        // Apply actions (must be after iteration)
        match action {
            Some(OpenerAction::OpenCreate) => state.modal.open_create(),
            Some(OpenerAction::Edit(ri, ti)) => {
                let rule = &config.openers[ri];
                let tool = &rule.tools[ti];
                state.modal.open_edit(ri, ti, rule, tool);
            }
            Some(OpenerAction::MoveUp(fi)) => {
                if fi > 0 {
                    swap_flat_entries(config, &flat, fi, fi - 1);
                }
            }
            Some(OpenerAction::MoveDown(fi)) => {
                if fi < len - 1 {
                    swap_flat_entries(config, &flat, fi, fi + 1);
                }
            }
            None => {}
        }
    });

    // Modal
    if state.modal.open {
        show_modal(ctx, config, state, tr);
    }
}

enum OpenerAction {
    OpenCreate,
    Edit(usize, usize),
    MoveUp(usize),
    MoveDown(usize),
}

/// Swap two flat entries by rebuilding the openers list.
fn swap_flat_entries(
    config: &mut Config,
    flat: &[(usize, usize, String, OpenerTool)],
    a: usize,
    b: usize,
) {
    // Rebuild openers from flat list with swapped positions
    let mut entries: Vec<(String, OpenerTool)> = flat
        .iter()
        .map(|(_, _, target, tool)| (target.clone(), tool.clone()))
        .collect();
    entries.swap(a, b);

    // Rebuild OpenerRule[] by grouping consecutive entries with same target
    let mut rules: Vec<OpenerRule> = Vec::new();
    for (target, tool) in entries {
        if let Some(last) = rules.last_mut() {
            if last.target == target {
                last.tools.push(tool);
                continue;
            }
        }
        rules.push(OpenerRule {
            target,
            tools: vec![tool],
        });
    }
    config.openers = rules;
}

fn show_modal(ctx: &egui::Context, config: &mut Config, state: &mut OpenerTabState, tr: &Tr) {
    let title = if state.modal.mode == ModalMode::Edit {
        tr.modal_edit_rule()
    } else {
        tr.modal_add_rule()
    };

    let modal = egui::Modal::new(egui::Id::new("opener_modal"));

    let resp = modal.show(ctx, |ui| {
        ui.heading(title);
        ui.separator();
        ui.add_space(4.0);

        // Target
        ui.label(tr.label_target());
        egui::ComboBox::from_id_salt("target_kind")
            .selected_text(match state.modal.edit_target_kind {
                TargetKind::Folder => tr.target_kind_folder(),
                TargetKind::Extension => tr.target_kind_extension(),
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut state.modal.edit_target_kind, TargetKind::Folder, tr.target_kind_folder());
                ui.selectable_value(&mut state.modal.edit_target_kind, TargetKind::Extension, tr.target_kind_extension());
            });

        if state.modal.edit_target_kind == TargetKind::Extension {
            ui.label(tr.label_extension());
            ui.text_edit_singleline(&mut state.modal.edit_target_ext);
            ui.label(
                egui::RichText::new(tr.hint_extension_format())
                    .small()
                    .color(crate::app::TEXT_SECONDARY),
            );
        }

        ui.add_space(4.0);

        // Tool name
        ui.label(tr.label_tool_name());
        ui.text_edit_singleline(&mut state.modal.edit_tool_name);

        ui.add_space(4.0);

        // Tool exe + browse
        ui.label(tr.label_executable());
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut state.modal.edit_tool_exe);
            if ui
                .add_enabled(!state.exe_picker.active, egui::Button::new(tr.btn_browse()))
                .clicked()
            {
                state.exe_picker.active = true;
                let result = Arc::clone(&state.exe_picker.result);
                let repaint_ctx = ctx.clone();
                let dialog_title = tr.dialog_select_exe().to_string();
                let filter_label = tr.filter_executables().to_string();
                std::thread::spawn(move || {
                    let path = rfd::FileDialog::new()
                        .set_title(&dialog_title)
                        .add_filter(&filter_label, &["exe", "bat", "cmd"])
                        .pick_file();
                    *result.lock().unwrap() = Some(path);
                    repaint_ctx.request_repaint();
                });
            }
        });

        ui.add_space(4.0);

        // Tool args
        ui.label(tr.label_arguments());
        ui.text_edit_singleline(&mut state.modal.edit_tool_args);
        ui.label(
            egui::RichText::new(tr.hint_path_placeholder())
                .small()
                .color(crate::app::TEXT_SECONDARY),
        );

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
                if let (Some(ri), Some(ti)) = (state.modal.editing_rule, state.modal.editing_tool) {
                    if ri < config.openers.len() && ti < config.openers[ri].tools.len() {
                        config.openers[ri].tools.remove(ti);
                        if config.openers[ri].tools.is_empty() {
                            config.openers.remove(ri);
                        }
                    }
                }
                state.modal.close();
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr.btn_cancel()).clicked() {
                    state.modal.close();
                }
                if ui.button(tr.btn_save()).clicked() {
                    save_opener(config, &state.modal);
                    state.modal.close();
                }
            });
        });
    });

    if resp.should_close() {
        state.modal.close();
    }
}

fn save_opener(config: &mut Config, modal: &ModalState) {
    let target = match modal.edit_target_kind {
        TargetKind::Folder => "folder".to_string(),
        TargetKind::Extension => format!("ext:{}", modal.edit_target_ext.trim()),
    };
    let tool = OpenerTool {
        name: modal.edit_tool_name.clone(),
        exe: modal.edit_tool_exe.clone(),
        args: modal.edit_tool_args.clone(),
    };

    if let (Some(ri), Some(ti)) = (modal.editing_rule, modal.editing_tool) {
        // Edit existing
        if ri < config.openers.len() && ti < config.openers[ri].tools.len() {
            if config.openers[ri].target == target {
                // Same target: just update the tool in place
                config.openers[ri].tools[ti] = tool;
            } else {
                // Target changed: remove from old rule, add to new
                config.openers[ri].tools.remove(ti);
                if config.openers[ri].tools.is_empty() {
                    config.openers.remove(ri);
                }
                if let Some(rule) = config.openers.iter_mut().find(|r| r.target == target) {
                    rule.tools.push(tool);
                } else {
                    config.openers.push(OpenerRule {
                        target,
                        tools: vec![tool],
                    });
                }
            }
        }
    } else {
        // Create new: find existing rule with same target or create new
        if let Some(rule) = config.openers.iter_mut().find(|r| r.target == target) {
            rule.tools.push(tool);
        } else {
            config.openers.push(OpenerRule {
                target,
                tools: vec![tool],
            });
        }
    }
}
