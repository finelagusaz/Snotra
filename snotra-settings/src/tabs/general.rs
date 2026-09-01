//! 全般設定タブ（起動時表示・トレイ・IME・ホットキー・スタートアップ登録）。
//!
//! **「スタートアップ」節だけが `Config` に属さない。** そこは Windows のログオン時自動起動
//! （`HKCU\...\Run`）の状態を直接読み書きし、Save を経由せず即時に効く。判断と却下した代替案は
//! `SPEC.md` §7.7、機構は [`snotra_core::autostart`]。

use eframe::egui;
use snotra_core::autostart::{self, AutostartError};
use snotra_core::config::{Config, Language};

use crate::hotkey_input::{self, HotkeyInputState};
use crate::i18n::{Tr, TrKey};
use crate::style;

/// 全般タブのうち `Config` に属さない状態（＝スタートアップ節）。
///
/// `enabled` は**レジストリの写しである**。正本は OS 側にあり、ここは表示用のキャッシュにすぎない
/// ——毎フレームのレジストリ読みを避けるため、起動時（`app::run`）と操作直後にだけ更新する。
#[derive(Default)]
pub struct GeneralTabState {
    /// 直近に観測したスタートアップ登録の有無。
    enabled: bool,
    /// 操作結果のインラインメッセージ（次の操作まで表示を維持する）。
    message: String,
    message_is_error: bool,
}

impl GeneralTabState {
    /// 実 OS 状態から初期化する。**呼ぶのは `app::run` だけである**——`SettingsApp::new` から
    /// 呼ぶと `en_harness` 経由でヘッドレス UI テストが開発機のレジストリを読む（#963 と同型）。
    pub fn new(autostart_enabled: bool) -> Self {
        Self {
            enabled: autostart_enabled,
            ..Default::default()
        }
    }
}

/// スタートアップ登録を `desired` の状態にし、**その直後に実 OS 状態を読み直す**。
///
/// 読み直すのは、失敗したときに UI が嘘をつかないためである（チェックが入ったのに登録されて
/// いない、を残さない）。
fn apply_autostart(state: &mut GeneralTabState, desired: bool, tr: &Tr) {
    let result = if desired {
        autostart::enable()
    } else {
        autostart::disable()
    };
    match result {
        Ok(()) => {
            state.message_is_error = false;
            state.message = if desired {
                tr.t(TrKey::StatusAutostartEnabled).to_string()
            } else {
                tr.t(TrKey::StatusAutostartDisabled).to_string()
            };
        }
        Err(AutostartError::MainExeNotFound) => {
            state.message_is_error = true;
            state.message = tr.t(TrKey::ErrAutostartExeNotFound).to_string();
        }
        Err(AutostartError::Registry(code)) => {
            state.message_is_error = true;
            state.message =
                tr.t_params(TrKey::ErrAutostartRegistry, &[("code", &code.to_string())]);
        }
    }
    state.enabled = autostart::is_enabled();
}

pub fn ui(
    ui: &mut egui::Ui,
    config: &mut Config,
    hotkey_state: &mut HotkeyInputState,
    state: &mut GeneralTabState,
    tr: &Tr,
) {
    style::tab_scroll_area(ui, |ui| {
        // -- Language --
        style::section_heading(ui, tr.t(TrKey::HeadingLanguage));

        style::settings_grid("language_grid").show(ui, |ui| {
            ui.label(tr.t(TrKey::LabelLanguage));
            let lang_label = match config.general.language {
                Language::Ja => tr.t(TrKey::LanguageJa),
                Language::En => tr.t(TrKey::LanguageEn),
            };
            egui::ComboBox::from_id_salt("language")
                .selected_text(lang_label)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut config.general.language,
                        Language::Ja,
                        tr.t(TrKey::LanguageJa),
                    );
                    ui.selectable_value(
                        &mut config.general.language,
                        Language::En,
                        tr.t(TrKey::LanguageEn),
                    );
                });
            ui.end_row();
        });

        style::section_gap(ui);

        // -- Hotkey --
        style::section_heading(ui, tr.t(TrKey::HeadingHotkey));

        style::settings_grid("hotkey_grid").show(ui, |ui| {
            ui.label(tr.t(TrKey::LabelHotkey));
            hotkey_input::hotkey_input(ui, &mut config.hotkey, hotkey_state, tr);
            ui.end_row();
        });

        ui.checkbox(
            &mut config.general.hotkey_toggle,
            tr.t(TrKey::CbHotkeyToggle),
        );

        style::section_gap(ui);

        // -- Behavior --
        style::section_heading(ui, tr.t(TrKey::HeadingBehavior));

        ui.checkbox(
            &mut config.general.show_on_startup,
            tr.t(TrKey::CbShowOnStartup),
        );
        ui.checkbox(
            &mut config.general.auto_hide_on_focus_lost,
            tr.t(TrKey::CbAutoHide),
        );
        ui.checkbox(&mut config.general.show_tray_icon, tr.t(TrKey::CbTrayIcon));
        ui.checkbox(&mut config.general.ime_off_on_show, tr.t(TrKey::CbImeOff));
        ui.checkbox(
            &mut config.general.follow_cursor_monitor,
            tr.t(TrKey::CbFollowCursorMonitor),
        );

        style::section_gap(ui);

        // -- Startup --
        //
        // **この節だけ Save を経由しない。** 状態の正本は `Config` ではなくレジストリなので、
        // draft/saved の二重状態モデルに載せようがない（`SECTION_TABLE` の型は `Config` の外を
        // 表現できない）。切り替えた時点で OS へ書き、結果をその場のインラインメッセージで返す。
        style::section_heading(ui, tr.t(TrKey::HeadingStartup));

        let mut desired = state.enabled;
        if ui
            .checkbox(&mut desired, tr.t(TrKey::CbLaunchAtLogon))
            .changed()
        {
            apply_autostart(state, desired, tr);
        }

        if !state.message.is_empty() {
            let color = if state.message_is_error {
                style::STATUS_ERROR
            } else {
                style::STATUS_SUCCESS
            };
            ui.label(egui::RichText::new(&state.message).color(color));
        }

        style::section_gap(ui);

        // -- Auto Update --
        style::section_heading(ui, tr.t(TrKey::HeadingAutoUpdate));

        style::settings_grid("auto_update_grid").show(ui, |ui| {
            use snotra_core::config::AutoUpdateMode;
            let selected_label = match config.general.auto_update {
                AutoUpdateMode::Full => tr.t(TrKey::AutoUpdateFull),
                AutoUpdateMode::CheckOnly => tr.t(TrKey::AutoUpdateCheckOnly),
                AutoUpdateMode::Disabled => tr.t(TrKey::AutoUpdateDisabled),
            };
            egui::ComboBox::from_id_salt("auto_update_mode")
                .selected_text(selected_label)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut config.general.auto_update,
                        AutoUpdateMode::Full,
                        tr.t(TrKey::AutoUpdateFull),
                    );
                    ui.selectable_value(
                        &mut config.general.auto_update,
                        AutoUpdateMode::CheckOnly,
                        tr.t(TrKey::AutoUpdateCheckOnly),
                    );
                    ui.selectable_value(
                        &mut config.general.auto_update,
                        AutoUpdateMode::Disabled,
                        tr.t(TrKey::AutoUpdateDisabled),
                    );
                });
            ui.end_row();
        });
    });
}
