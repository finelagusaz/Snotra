use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui;
use snotra_core::config::{Config, OpenerRule, OpenerTool};

/// Non-blocking file picker state for exe selection
#[derive(Clone, Default)]
pub struct ExePickerState {
    pub result: Arc<Mutex<Option<Option<PathBuf>>>>,
    pub active: bool,
}

#[derive(Default)]
pub struct OpenerTabState {
    pub exe_picker: ExePickerState,
    modal: ModalState,
}

#[derive(Default)]
struct ModalState {
    open: bool,
    mode: ModalMode,
    editing_rule: Option<usize>,
    editing_tool: Option<usize>,
    edit_target: String,
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
        self.edit_target = "folder".to_string();
        self.edit_tool_name.clear();
        self.edit_tool_exe.clear();
        self.edit_tool_args.clear();
    }

    fn open_edit(&mut self, rule_idx: usize, tool_idx: usize, rule: &OpenerRule, tool: &OpenerTool) {
        self.open = true;
        self.mode = ModalMode::Edit;
        self.editing_rule = Some(rule_idx);
        self.editing_tool = Some(tool_idx);
        self.edit_target = rule.target.clone();
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

pub fn ui(ui: &mut egui::Ui, ctx: &egui::Context, config: &mut Config, state: &mut OpenerTabState) {
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

        ui.heading("オープナールール");
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("ファイル種別ごとに起動するアプリケーションを設定します。")
                .small()
                .color(egui::Color32::GRAY),
        );
        ui.add_space(8.0);

        if config.openers.is_empty() {
            ui.label("ルールが設定されていません。");
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
                        "フォルダ".to_string()
                    } else {
                        target.clone()
                    };
                    ui.label(format!(
                        "[{}] {}",
                        target_label,
                        if tool.name.is_empty() {
                            "(名前なし)"
                        } else {
                            &tool.name
                        }
                    ));
                    ui.label(
                        egui::RichText::new(&tool.exe)
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("編集").clicked() {
                        action = Some(OpenerAction::Edit(*ri, *ti));
                    }
                });
            });
            ui.separator();
        }

        if ui.button("追加…").clicked() {
            action = Some(OpenerAction::OpenCreate);
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
        show_modal(ctx, config, state);
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

fn show_modal(ctx: &egui::Context, config: &mut Config, state: &mut OpenerTabState) {
    let title = if state.modal.mode == ModalMode::Edit {
        "ルールを編集"
    } else {
        "ルールを追加"
    };

    let modal = egui::Modal::new(egui::Id::new("opener_modal"));

    let resp = modal.show(ctx, |ui| {
        ui.heading(title);
        ui.separator();
        ui.add_space(4.0);

        // Target
        ui.label("ターゲット:");
        ui.text_edit_singleline(&mut state.modal.edit_target);
        ui.label(
            egui::RichText::new("\"folder\" またはカンマ区切り拡張子 (例: ext:.png,.jpg)")
                .small()
                .color(egui::Color32::GRAY),
        );

        ui.add_space(4.0);

        // Tool name
        ui.label("ツール名:");
        ui.text_edit_singleline(&mut state.modal.edit_tool_name);

        ui.add_space(4.0);

        // Tool exe + browse
        ui.label("実行ファイル:");
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut state.modal.edit_tool_exe);
            if ui
                .add_enabled(!state.exe_picker.active, egui::Button::new("参照…"))
                .clicked()
            {
                state.exe_picker.active = true;
                let result = Arc::clone(&state.exe_picker.result);
                let repaint_ctx = ctx.clone();
                std::thread::spawn(move || {
                    let path = rfd::FileDialog::new()
                        .set_title("実行ファイルを選択")
                        .add_filter("実行ファイル", &["exe", "bat", "cmd"])
                        .pick_file();
                    *result.lock().unwrap() = Some(path);
                    repaint_ctx.request_repaint();
                });
            }
        });

        ui.add_space(4.0);

        // Tool args
        ui.label("引数:");
        ui.text_edit_singleline(&mut state.modal.edit_tool_args);
        ui.label(
            egui::RichText::new("{path} でファイルパスを埋め込み")
                .small()
                .color(egui::Color32::GRAY),
        );

        ui.add_space(8.0);
        ui.separator();

        ui.horizontal(|ui| {
            // Delete (edit mode only)
            if state.modal.mode == ModalMode::Edit && ui.button("削除").clicked() {
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
                if ui.button("保存").clicked() {
                    save_opener(config, &state.modal);
                    state.modal.close();
                }
                if ui.button("キャンセル").clicked() {
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
    let target = modal.edit_target.clone();
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
