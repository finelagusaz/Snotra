//! インデックス設定タブ（スキャンパスの追加/削除/管理）。

use eframe::egui;
use snotra_core::config::{Config, ScanPath};

use crate::i18n::{Tr, TrKey};
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

/// UNIX 秒をローカル時刻の「YYYY-MM-DD HH:MM」へ整形する。
///
/// **`Local::now()` を内部で呼ばない。** 時間帯と実行時刻でテストが揺れるため、
/// 変換済みの `DateTime<Local>` を受け取る（`snotra_core::instant` の `format_date`
/// と同じ形）。秒は落とす——ユーザーが知りたいのは「いつ更新したか」であって、
/// 秒の精度は要らない。
fn format_built_at(dt: &chrono::DateTime<chrono::Local>) -> String {
    dt.format("%Y-%m-%d %H:%M").to_string()
}

/// 表示する文字列を決める。**不在・読めない・壊れているを区別しない**——
/// ユーザーにとってはどれも「まだ構築していない」と同じである。
fn built_at_text(built_at: Option<u64>, tr: &Tr) -> String {
    use chrono::TimeZone;
    let Some(secs) = built_at else {
        return tr.t(TrKey::LabelIndexNotBuilt).to_string();
    };
    match chrono::Local.timestamp_opt(secs as i64, 0).single() {
        Some(dt) => format_built_at(&dt),
        // 範囲外の値（壊れたファイル）も「未構築」へ倒す。
        None => tr.t(TrKey::LabelIndexNotBuilt).to_string(),
    }
}

fn parse_extensions(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn ui(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    config: &mut Config,
    state: &mut IndexTabState,
    tr: &Tr,
) {
    // Poll picker result
    if let Some(Some(path)) = state.picker.poll() {
        state.modal.fields.path = path.display().to_string();
    }

    style::tab_scroll_area(ui, |ui| {
        // **索引がいつのものかを示すだけである。** 再構築のボタンは置かない——
        // 設定アプリは別プロセスで、本体との通信路は config.toml と config_watcher
        // しかない（ボタンは通信路の新設を要する・ADR-rescan-explicit-only）。
        let built_at = snotra_core::config::Config::config_dir()
            .and_then(|dir| snotra_core::indexer::index_built_at_in(&dir));
        style::hint(
            ui,
            &format!(
                "{} {}",
                tr.t(TrKey::LabelIndexLastBuilt),
                built_at_text(built_at, tr)
            ),
        );

        style::section_heading(ui, tr.t(TrKey::HeadingScanTargets));

        if config.paths.scan.is_empty() {
            style::hint(ui, tr.t(TrKey::LabelNoScanPaths));
        }

        // List scan paths
        let mut action: Option<ListAction> = None;
        for (i, scan) in config.paths.scan.iter().enumerate() {
            style::list_item(
                ui,
                |ui| {
                    ui.label(&scan.path);
                    let meta = if scan.include_folders {
                        tr.t_params(
                            TrKey::IndexScanExtensionsWithFolders,
                            &[("extensions", &scan.extensions.join(", "))],
                        )
                    } else {
                        scan.extensions.join(", ")
                    };
                    style::hint(ui, &meta);
                },
                |ui| {
                    if ui.button(tr.t(TrKey::BtnEdit)).clicked() {
                        action = Some(ListAction::Edit(i));
                    }
                },
            );
        }

        if ui.button(tr.t(TrKey::BtnAdd)).clicked() {
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
        tr.t(TrKey::ModalEditScanPath)
    } else {
        tr.t(TrKey::ModalAddScanPath)
    };

    let modal = egui::Modal::new(egui::Id::new("index_modal"));

    let resp = modal.show(ctx, |ui| {
        style::modal_header(ui, title);

        // Path input + browse button
        ui.label(tr.t(TrKey::LabelPath));
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut state.modal.fields.path);
            if ui
                .add_enabled(
                    !state.picker.active,
                    egui::Button::new(tr.t(TrKey::BtnBrowse)),
                )
                .clicked()
            {
                let dialog_title = tr.t(TrKey::DialogSelectFolder).to_string();
                state.picker.launch(ctx, move || {
                    rfd::FileDialog::new()
                        .set_title(&dialog_title)
                        .pick_folder()
                });
            }
        });

        ui.add_space(style::SPACE_HINT);
        ui.label(tr.t(TrKey::LabelExtensions));
        ui.text_edit_singleline(&mut state.modal.fields.extensions);

        ui.add_space(style::SPACE_HINT);
        ui.checkbox(
            &mut state.modal.fields.include_folders,
            tr.t(TrKey::CbIncludeFolders),
        );

        ui.add_space(style::SPACE_GROUP);
        ui.separator();

        ui.horizontal(|ui| {
            // Delete button (edit mode only)
            if state.modal.is_edit() && style::danger_button(ui, tr.t(TrKey::BtnDelete)).clicked() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use snotra_core::config::Language;

    #[test]
    fn built_at_is_rendered_as_a_local_datetime() {
        let dt = chrono::Local
            .with_ymd_and_hms(2026, 8, 4, 9, 12, 0)
            .unwrap();
        assert_eq!(format_built_at(&dt), "2026-08-04 09:12");
    }

    /// **不在は「未構築」へ倒す。** `index.bin` が無い・読めない・壊れているを
    /// 区別しない——ユーザーにとってはどれも「まだ構築していない」と同じである。
    #[test]
    fn an_absent_index_renders_as_not_built() {
        let tr = Tr(Language::Ja);
        assert_eq!(built_at_text(None, &tr), tr.t(TrKey::LabelIndexNotBuilt));
    }
}
