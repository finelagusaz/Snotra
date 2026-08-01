//! オープナー設定タブ（ツール/ルール管理・プリセット検出/追加）。

use eframe::egui;
use snotra_core::config::{
    self, Config, OpenerRule, OpenerTool, extract_path_condition, opener_specificity_order,
};

use crate::i18n::{Tr, TrKey};
use crate::style;
use crate::tabs::common::{ModalState, PickerState};

pub struct OpenerTabState {
    pub exe_picker: PickerState,
    /// ネストした `(rule_idx, tool_idx)` を編集スナップショットに持つ（フラットリストの
    /// index / instant と異なり、Save/Delete の境界チェックは opener 固有ロジックで行う）。
    modal: ModalState<OpenerFields, (usize, usize)>,
    presets: Vec<config::OpenerPreset>,
}

impl OpenerTabState {
    pub fn new() -> Self {
        Self {
            exe_picker: PickerState::default(),
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

/// オープナーモーダルのタブ固有編集フィールド。
#[derive(Default)]
struct OpenerFields {
    target_kind: TargetKind,
    target_ext: String,
    target_path: String,
    tool_name: String,
    tool_exe: String,
    tool_args: String,
}

impl OpenerFields {
    fn from_rule_tool(rule: &OpenerRule, tool: &OpenerTool) -> Self {
        let mut fields = Self {
            // パス条件の抽出
            target_path: extract_path_condition(&rule.target)
                .unwrap_or("")
                .to_string(),
            tool_name: tool.name.clone(),
            tool_exe: tool.exe.clone(),
            tool_args: tool.args.clone(),
            ..Self::default()
        };
        if rule.target.starts_with("ext:") {
            fields.target_kind = TargetKind::Extension;
            fields.target_ext = config::extract_ext_part(&rule.target).to_string();
        } else {
            fields.target_kind = TargetKind::Folder;
        }
        fields
    }
}

pub fn ui(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    config: &mut Config,
    state: &mut OpenerTabState,
    tr: &Tr,
) {
    // Poll exe picker result
    if let Some(Some(path)) = state.exe_picker.poll() {
        state.modal.fields.tool_exe = path.display().to_string();
    }

    style::tab_scroll_area(ui, |ui| {
        style::section_heading(ui, tr.t(TrKey::HeadingOpenerRules));
        style::hint(ui, tr.t(TrKey::OpenerDescription));
        ui.add_space(style::SPACE_GROUP);

        if config.openers.is_empty() {
            style::hint(ui, tr.t(TrKey::LabelNoRules));
        }

        // Flatten rules into (rule_idx, tool_idx, target, tool) for display
        // Sort by specificity: path-qualified folder (longest first), generic folder,
        // path-qualified ext (longest first), generic ext
        let mut flat: Vec<(usize, usize, String, OpenerTool)> = Vec::new();
        for (ri, rule) in config.openers.iter().enumerate() {
            for (ti, tool) in rule.tools.iter().enumerate() {
                flat.push((ri, ti, rule.target.clone(), tool.clone()));
            }
        }
        flat.sort_by_key(|a| opener_specificity_order(&a.2));

        let mut action: Option<OpenerAction> = None;
        for (ri, ti, target, tool) in &flat {
            style::list_item(
                ui,
                |ui| {
                    let target_label = format_target_label(target, tr);
                    ui.label(format!(
                        "[{}] {}",
                        target_label,
                        if tool.name.is_empty() {
                            tr.t(TrKey::LabelNoName)
                        } else {
                            &tool.name
                        }
                    ));
                    style::hint(ui, &tool.exe);
                },
                |ui| {
                    if ui.button(tr.t(TrKey::BtnEdit)).clicked() {
                        action = Some(OpenerAction::Edit(*ri, *ti));
                    }
                    // 同一ルール内のツール並び替えボタン
                    let tool_count = config.openers[*ri].tools.len();
                    if tool_count > 1 {
                        match style::reorder_controls(ui, *ti > 0, *ti + 1 < tool_count) {
                            Some(style::ReorderDir::Up) => {
                                action = Some(OpenerAction::MoveUp(*ri, *ti));
                            }
                            Some(style::ReorderDir::Down) => {
                                action = Some(OpenerAction::MoveDown(*ri, *ti));
                            }
                            None => {}
                        }
                    }
                },
            );
        }

        if ui.button(tr.t(TrKey::BtnAdd)).clicked() {
            action = Some(OpenerAction::OpenCreate);
        }

        // Presets section
        if !state.presets.is_empty() {
            style::section_gap(ui);
            style::section_heading(ui, tr.t(TrKey::HeadingPresets));
            style::hint(ui, tr.t(TrKey::PresetDescription));
            ui.add_space(style::SPACE_HINT);

            let mut preset_action: Option<usize> = None;
            for (i, preset) in state.presets.iter().enumerate() {
                let already_added = config::is_preset_already_added(&config.openers, &preset.exe);
                ui.horizontal(|ui| {
                    ui.label(preset.name);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if already_added {
                            ui.add_enabled(
                                false,
                                egui::Button::new(tr.t(TrKey::LabelAlreadyAdded)),
                            );
                        } else if ui.button(tr.t(TrKey::BtnAddPreset)).clicked() {
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
                let fields = OpenerFields::from_rule_tool(rule, &rule.tools[ti]);
                state.modal.open_edit((ri, ti), fields);
            }
            Some(OpenerAction::MoveUp(ri, ti))
                if ti > 0 && ri < config.openers.len() && ti < config.openers[ri].tools.len() =>
            {
                config.openers[ri].tools.swap(ti, ti - 1);
            }
            Some(OpenerAction::MoveUp(_, _)) => {}
            Some(OpenerAction::MoveDown(ri, ti))
                if ri < config.openers.len() && ti + 1 < config.openers[ri].tools.len() =>
            {
                config.openers[ri].tools.swap(ti, ti + 1);
            }
            Some(OpenerAction::MoveDown(_, _)) => {}
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
    MoveUp(usize, usize),
    MoveDown(usize, usize),
}

fn show_modal(ctx: &egui::Context, config: &mut Config, state: &mut OpenerTabState, tr: &Tr) {
    let title = if state.modal.is_edit() {
        tr.t(TrKey::ModalEditRule)
    } else {
        tr.t(TrKey::ModalAddRule)
    };

    let modal = egui::Modal::new(egui::Id::new("opener_modal"));

    let resp = modal.show(ctx, |ui| {
        style::modal_header(ui, title);

        // Target
        ui.label(tr.t(TrKey::LabelTarget));
        egui::ComboBox::from_id_salt("target_kind")
            .selected_text(match state.modal.fields.target_kind {
                TargetKind::Folder => tr.t(TrKey::TargetKindFolder),
                TargetKind::Extension => tr.t(TrKey::TargetKindExtension),
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut state.modal.fields.target_kind,
                    TargetKind::Folder,
                    tr.t(TrKey::TargetKindFolder),
                );
                ui.selectable_value(
                    &mut state.modal.fields.target_kind,
                    TargetKind::Extension,
                    tr.t(TrKey::TargetKindExtension),
                );
            });

        if state.modal.fields.target_kind == TargetKind::Extension {
            ui.label(tr.t(TrKey::LabelExtension));
            ui.text_edit_singleline(&mut state.modal.fields.target_ext);
            style::hint(ui, tr.t(TrKey::HintExtensionFormat));
        }

        ui.add_space(style::SPACE_HINT);

        // Path condition (optional, applies to both folder and extension)
        ui.label(tr.t(TrKey::LabelPathCondition));
        ui.text_edit_singleline(&mut state.modal.fields.target_path);
        style::hint(ui, tr.t(TrKey::HintPathCondition));

        ui.add_space(style::SPACE_HINT);

        // Tool name
        ui.label(tr.t(TrKey::LabelToolName));
        ui.text_edit_singleline(&mut state.modal.fields.tool_name);

        ui.add_space(style::SPACE_HINT);

        // Tool exe + browse
        ui.label(tr.t(TrKey::LabelExecutable));
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut state.modal.fields.tool_exe);
            if ui
                .add_enabled(
                    !state.exe_picker.active,
                    egui::Button::new(tr.t(TrKey::BtnBrowse)),
                )
                .clicked()
            {
                let dialog_title = tr.t(TrKey::DialogSelectExe).to_string();
                let filter_label = tr.t(TrKey::FilterExecutables).to_string();
                state.exe_picker.launch(ctx, move || {
                    rfd::FileDialog::new()
                        .set_title(&dialog_title)
                        .add_filter(&filter_label, &["exe", "bat", "cmd"])
                        .pick_file()
                });
            }
        });

        ui.add_space(style::SPACE_HINT);

        // Tool args
        ui.label(tr.t(TrKey::LabelArguments));
        ui.text_edit_singleline(&mut state.modal.fields.tool_args);
        style::hint(ui, tr.t(TrKey::HintPathPlaceholder));

        ui.add_space(style::SPACE_GROUP);
        ui.separator();

        ui.horizontal(|ui| {
            // Delete (edit mode only)
            if state.modal.is_edit() && style::danger_button(ui, tr.t(TrKey::BtnDelete)).clicked() {
                if let Some((ri, ti)) = state.modal.editing
                    && ri < config.openers.len()
                    && ti < config.openers[ri].tools.len()
                {
                    config.openers[ri].tools.remove(ti);
                    if config.openers[ri].tools.is_empty() {
                        config.openers.remove(ri);
                    }
                }
                state.modal.close();
            }

            let buttons = style::modal_buttons(ui, tr);
            if buttons.cancel {
                state.modal.close();
            }
            if buttons.save {
                save_opener(config, &state.modal);
                state.modal.close();
            }
        });
    });

    if resp.should_close() {
        state.modal.close();
    }
}

fn save_opener(config: &mut Config, modal: &ModalState<OpenerFields, (usize, usize)>) {
    let path_trimmed = modal.fields.target_path.trim();
    let target = match modal.fields.target_kind {
        TargetKind::Folder => {
            if path_trimmed.is_empty() {
                "folder".to_string()
            } else {
                format!("folder:{path_trimmed}")
            }
        }
        TargetKind::Extension => {
            let ext = modal.fields.target_ext.trim();
            if path_trimmed.is_empty() {
                format!("ext:{ext}")
            } else {
                format!("ext:{ext}:{path_trimmed}")
            }
        }
    };
    let tool = OpenerTool {
        name: modal.fields.tool_name.clone(),
        exe: modal.fields.tool_exe.clone(),
        args: modal.fields.tool_args.clone(),
    };

    if let Some((ri, ti)) = modal.editing {
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

/// ターゲット文字列の表示ラベルを生成する。
fn format_target_label(target: &str, tr: &Tr) -> String {
    let path_cond = extract_path_condition(target);
    if target == "folder" {
        tr.t(TrKey::LabelAllFolders).to_string()
    } else if target.starts_with("folder:") {
        path_cond.unwrap_or("").to_string()
    } else if target.starts_with("ext:") {
        let ext_part = config::extract_ext_part(target);
        if let Some(path) = path_cond {
            format!("{ext_part} ({path})")
        } else {
            ext_part.to_string()
        }
    } else {
        target.to_string()
    }
}
