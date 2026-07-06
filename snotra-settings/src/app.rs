use eframe::egui;
use egui::{CornerRadius, Stroke};
use snotra_core::config::{Config, ConfigError, LoadOutcome};
use snotra_core::window_data::{self, WindowPlacement};

use crate::i18n::{Tr, TrKey};
use crate::style;
use crate::style::{
    ACCENT, BANNER_CAUTION_BG, BANNER_CAUTION_FG, CONTENT_BG, FOOTER_BG, SIDEBAR_BG, TAB_HOVER,
    TAB_SELECTED_BG, TEXT_PRIMARY, TEXT_SECONDARY, WIDGET_ACTIVE_BG, WIDGET_BG, WIDGET_BORDER,
};
use crate::tabs;

fn apply_win11_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::light();

    // Panel backgrounds
    visuals.panel_fill = CONTENT_BG;
    visuals.window_fill = CONTENT_BG;

    // Widget styles
    visuals.widgets.noninteractive.bg_fill = CONTENT_BG;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, WIDGET_BORDER);

    visuals.widgets.inactive.bg_fill = WIDGET_BG;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, WIDGET_BORDER);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(4);

    visuals.widgets.hovered.bg_fill = TAB_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(4);

    visuals.widgets.active.bg_fill = WIDGET_ACTIVE_BG;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    visuals.widgets.active.corner_radius = CornerRadius::same(4);

    // Selection color (accent)
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.3);
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);

    // Hyperlink color
    visuals.hyperlink_color = ACCENT;

    ctx.set_visuals(visuals);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabId {
    General,
    Search,
    Index,
    Visual,
    Opener,
    InstantCommand,
    Backup,
}

impl TabId {
    const ALL: &[TabId] = &[
        TabId::General,
        TabId::Search,
        TabId::Index,
        TabId::Visual,
        TabId::Opener,
        TabId::InstantCommand,
        TabId::Backup,
    ];

    fn label(self, tr: &Tr) -> &'static str {
        match self {
            TabId::General => tr.t(TrKey::TabGeneral),
            TabId::Search => tr.t(TrKey::TabSearch),
            TabId::Index => tr.t(TrKey::TabIndex),
            TabId::Visual => tr.t(TrKey::TabVisual),
            TabId::Opener => tr.t(TrKey::TabOpener),
            TabId::InstantCommand => tr.t(TrKey::TabInstantCommand),
            TabId::Backup => tr.t(TrKey::TabBackup),
        }
    }

    fn from_str(s: &str) -> Option<TabId> {
        match s {
            "general" => Some(TabId::General),
            "search" => Some(TabId::Search),
            "index" => Some(TabId::Index),
            "visual" => Some(TabId::Visual),
            "opener" => Some(TabId::Opener),
            "instant_command" => Some(TabId::InstantCommand),
            "backup" => Some(TabId::Backup),
            _ => None,
        }
    }

    /// タブ別ダーティ点（`•`）の判定を `SECTION_TABLE` から導出する。
    /// 新セクション追加時は `SECTION_TABLE` の1箇所だけを更新すればよい
    /// （`section_table_covers_all_config_fields` テストが更新漏れを検出する）。
    fn has_changes(self, draft: &Config, saved: &Config) -> bool {
        SECTION_TABLE.iter().any(|(tab, diff)| *tab == self && diff(draft, saved))
    }
}

/// Config セクション → TabId の対応表（SSOT）。
///
/// タブ別ダーティ点（[`TabId::has_changes`]）はここから導出される。全体判定
/// （[`SettingsApp::has_changes`]）は構造体全体の `PartialEq`（`draft != saved`）を使うため
/// 新フィールド追加時も自動追従するが、この表を更新し忘れるとタブ点だけが無反応になりうる。
/// `section_table_covers_all_config_fields` テストが、この表の合成が `draft != saved` と
/// 一致すること（＝更新漏れがないこと）を Config の全フィールドについて検証する。
type SectionDiff = fn(&Config, &Config) -> bool;

const SECTION_TABLE: &[(TabId, SectionDiff)] = &[
    (TabId::General, |d, s| d.hotkey != s.hotkey),
    (TabId::General, |d, s| d.general != s.general),
    (TabId::Search, |d, s| d.search != s.search),
    (TabId::Index, |d, s| d.paths != s.paths),
    (TabId::Visual, |d, s| d.visual != s.visual),
    (TabId::Visual, |d, s| d.appearance != s.appearance),
    (TabId::Opener, |d, s| d.openers != s.openers),
    (TabId::InstantCommand, |d, s| d.instant_commands != s.instant_commands),
];

struct SettingsApp {
    draft: Config,
    saved: Config,
    active_tab: TabId,
    status: String,
    status_timer: f64,
    index_state: tabs::index::IndexTabState,
    opener_state: tabs::opener::OpenerTabState,
    instant_state: tabs::instant::InstantTabState,
    backup_state: tabs::backup::BackupTabState,
    font_list: Vec<String>,
    hotkey_state: crate::hotkey_input::HotkeyInputState,
    last_position: Option<WindowPlacement>,
    tr: Tr,
    sidebar_focused: bool,
    /// 起動時の読み込みが一時的失敗（`ReadFailed`）で既定値表示中か（finding C）。
    loaded_read_failed: bool,
    /// 上記のとき、既存設定喪失リスクを承知で保存する確認チェック。保存成功でリセット。
    confirm_overwrite_despite_read_failure: bool,
}

impl SettingsApp {
    fn new(
        config: Config,
        first_run: bool,
        initial_tab: Option<String>,
        load_outcome: LoadOutcome,
    ) -> Self {
        let tab = initial_tab
            .as_deref()
            .and_then(TabId::from_str)
            .unwrap_or(if first_run {
                TabId::Index
            } else {
                TabId::General
            });
        let tr = Tr(config.general.language);
        Self {
            draft: config.clone(),
            saved: config,
            active_tab: tab,
            status: String::new(),
            status_timer: 0.0,
            index_state: tabs::index::IndexTabState::default(),
            opener_state: tabs::opener::OpenerTabState::new(),
            instant_state: tabs::instant::InstantTabState::default(),
            backup_state: tabs::backup::BackupTabState::default(),
            font_list: crate::font::list_system_fonts(),
            hotkey_state: Default::default(),
            last_position: None,
            tr,
            sidebar_focused: true,
            loaded_read_failed: load_outcome == LoadOutcome::ReadFailed,
            confirm_overwrite_despite_read_failure: false,
        }
    }

    fn has_changes(&self) -> bool {
        self.draft != self.saved
    }

    fn save(&mut self) {
        // finding C: 一時的 read 失敗起因で既定値を表示している間は、明示確認なしに
        // 保存させない（ロック解除済みの実 config.toml を既定値で上書きする事故を防ぐ）。
        if self.loaded_read_failed && !self.confirm_overwrite_despite_read_failure {
            self.status = self.tr.t(TrKey::StatusReadFailedBlocked).to_string();
            self.status_timer = 5.0;
            return;
        }
        let mut config = self.draft.clone();
        config.paths.normalize_scan_paths();
        config.normalize_openers();
        let errors = config.validate();
        if !errors.is_empty() {
            self.status = format!(
                "{}{}",
                self.tr.t(TrKey::StatusValidationError),
                config_error_message(&errors[0], &self.tr)
            );
            self.status_timer = 5.0;
            return;
        }
        if let Err(e) = config.save() {
            self.status = format!("{}{e}", self.tr.t(TrKey::StatusSaveFailed));
            self.status_timer = 5.0;
            return;
        }
        self.saved = config.clone();
        self.draft = config;
        // 保存成功で read 失敗ガードを解除（実 config.toml がユーザー設定で書かれた）
        self.loaded_read_failed = false;
        self.confirm_overwrite_despite_read_failure = false;
        // Update tr for potential language change
        self.tr = Tr(self.draft.general.language);
        self.status = self.tr.t(TrKey::StatusSaved).to_string();
        self.status_timer = 2.0;
    }

    fn reset_to_default(&mut self) {
        // Config::normalized_default() が正規化（apply_migrations 相当）済みの既定値を返すため、
        // ここで呼び忘れても draft/saved の PartialEq が tab 遷移順序に依存する心配はない。
        self.draft = Config::normalized_default();
        self.tr = Tr(self.draft.general.language);
        self.hotkey_state = Default::default();
        self.index_state = tabs::index::IndexTabState::default();
        self.instant_state = tabs::instant::InstantTabState::default();
        self.opener_state = tabs::opener::OpenerTabState::new();
    }
}

pub(crate) fn config_error_message(error: &ConfigError, tr: &Tr) -> String {
    match error {
        ConfigError::HotkeyModifierEmpty => tr.t(TrKey::ErrHotkeyModifierEmpty).to_string(),
        ConfigError::HotkeyKeyEmpty => tr.t(TrKey::ErrHotkeyKeyEmpty).to_string(),
        ConfigError::HotkeySystemConflict { modifier, key } => tr.t_params(
            TrKey::ErrHotkeySystemConflict,
            &[("value", &format!("{modifier}+{key}"))],
        ),
        ConfigError::VisibleRowsZero => tr.t(TrKey::ErrVisibleRowsZero).to_string(),
        ConfigError::WindowWidthTooSmall(w) => {
            tr.t_params(TrKey::ErrWindowWidthTooSmall, &[("value", &w.to_string())])
        }
        ConfigError::FuzzyCapRatioOutOfRange { value } => {
            tr.t_params(TrKey::ErrFuzzyCapRatioOutOfRange, &[("value", &value.to_string())])
        }
        ConfigError::ScanPathEmpty { index } => {
            tr.t_params(TrKey::ErrScanPathEmpty, &[("n", &(index + 1).to_string())])
        }
        ConfigError::InstantCommandPrefixEmpty => tr.t(TrKey::ErrInstantPrefixEmpty).to_string(),
        ConfigError::InstantCommandPrefixSlash => tr.t(TrKey::ErrInstantPrefixSlash).to_string(),
        ConfigError::InstantCommandDuplicateName { name } => {
            tr.t_params(TrKey::ErrInstantDuplicateName, &[("name", name)])
        }
        ConfigError::InstantCommandUnknownModifier { name, modifier } => tr.t_params(
            TrKey::ErrInstantUnknownModifier,
            &[("name", name), ("modifier", modifier)],
        ),
        ConfigError::MigemoMinCharsZero => tr.t(TrKey::ErrMigemoMinCharsZero).to_string(),
    }
}

impl eframe::App for SettingsApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(pos) = self.last_position {
            window_data::save_settings_placement(pos);
        }
    }

    // eframe 0.35: App::update は logic()/ui() に分割された。全レイアウトを ui() が受け取る
    // ルート Ui 上に置き、各 Panel は show(ui) で描画する（旧 Panel::show(ctx) は 0.35 で削除、
    // show_inside は show へ改名）。ctx はルート Ui から取得する。
    // 本体は `ui_impl` に委譲する（`_frame` 未使用のため挙動不変）。egui_kittest の
    // `Harness::new_ui_state` は `FnMut(&mut Ui, &mut State)` を取り Frame を渡せないため、
    // Frame 非依存の `ui_impl` を切り出してヘッドレステストから直接呼べるようにしている。
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.ui_impl(ui);
    }
}

impl SettingsApp {
    /// eframe の `App::ui` 本体（`eframe::Frame` 非依存）。本番は `ui()` から、テストは
    /// egui_kittest の Harness から呼ぶ。egui 描画コードはここに集約する。
    fn ui_impl(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        // Update window title (language change + dirty indicator)
        let title = if self.has_changes() {
            format!("{}*", self.tr.t(TrKey::WindowTitle))
        } else {
            self.tr.t(TrKey::WindowTitle).to_string()
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        // Sentinel ID: 0-size focusable widget at the top of CentralPanel.
        // Tab order: sidebar (↑↓) → sentinel (transparent) → content widgets → footer buttons
        //            → sentinel → sidebar (wrap-around).
        // The two blocks below must stay in this order: auto-sync first, sentinel detection second.
        // Rationale: when sentinel has focus, sidebar_focused is still false (previous frame),
        // so auto-sync does not fire (its guard is `self.sidebar_focused`). Sentinel detection
        // then sets sidebar_focused=true before surrendering focus, leaving no race window.
        let sidebar_sentinel_id = egui::Id::new("sidebar_sentinel");

        // Single memory read for both focus-sync checks below.
        let focused_id = ctx.memory(|m| m.focused());

        // Auto-sync: any pointer click (including on empty content area) clears sidebar focus.
        // The sidebar tab click handler will re-assert sidebar_focused=true if appropriate.
        // Skip during hotkey capture to avoid clearing sidebar state unexpectedly.
        if !self.hotkey_state.is_capturing()
            && self.sidebar_focused
            && ctx.input(|i| i.pointer.any_click())
        {
            self.sidebar_focused = false;
        }

        // Secondary auto-sync: if a widget takes keyboard focus without a pointer click
        // (e.g. programmatic focus), clear sidebar focus as well.
        if !self.hotkey_state.is_capturing()
            && self.sidebar_focused
            && focused_id.map(|id| id != sidebar_sentinel_id).unwrap_or(false)
        {
            self.sidebar_focused = false;
        }

        // When sentinel gets focus (Shift+Tab from first content widget, or Tab wrap-around),
        // return keyboard control to the sidebar.
        // Skip during hotkey capture for symmetry with the auto-sync guard above.
        if !self.hotkey_state.is_capturing()
            && !self.sidebar_focused
            && focused_id == Some(sidebar_sentinel_id)
        {
            self.sidebar_focused = true;
            ctx.memory_mut(|m| m.surrender_focus(sidebar_sentinel_id));
        }

        if !self.hotkey_state.is_capturing() && self.sidebar_focused {
            // ↑↓: navigate sidebar tabs.
            let tabs = TabId::ALL;
            let current = tabs.iter().position(|&t| t == self.active_tab).unwrap_or(0);
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) && current > 0 {
                self.active_tab = tabs[current - 1];
            }
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) && current + 1 < tabs.len() {
                self.active_tab = tabs[current + 1];
            }
            // Tab: move focus to content.
            // begin_pass already set focus_direction=Next from RawInput, so the sentinel's
            // interested_in_focus() will fire give_to_next=true and transparently hand focus
            // to the first focusable content widget without the sentinel ever appearing focused.
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)) {
                self.sidebar_focused = false;
                ctx.memory_mut(|m| m.request_focus(sidebar_sentinel_id));
            }
        }

        // Close on Escape (skip when hotkey capture is active)
        if !self.hotkey_state.is_capturing() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.has_changes() {
                self.status = self.tr.t(TrKey::StatusUnsaved).to_string();
                self.status_timer = 3.0;
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        // Prevent close via × button / Alt+F4 when there are unsaved changes
        if ctx.input(|i| i.viewport().close_requested()) && self.has_changes() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.status = self.tr.t(TrKey::StatusUnsaved).to_string();
            self.status_timer = 3.0;
        }

        // Decrement status timer
        let dt = ctx.input(|i| i.stable_dt) as f64;
        if self.status_timer > 0.0 {
            self.status_timer -= dt;
            if self.status_timer <= 0.0 {
                self.status.clear();
            }
            ctx.request_repaint();
        }

        // Sidebar
        egui::Panel::left("tabs_panel")
            .resizable(false)
            .exact_size(140.0)
            .frame(egui::Frame::NONE.fill(SIDEBAR_BG).inner_margin(egui::Margin::symmetric(8, 8)))
            .show(ui, |ui| {
                let available_width = ui.available_width();

                // Tab list
                for &tab in TabId::ALL {
                    let selected = self.active_tab == tab;
                    // Sense::CLICK (without FOCUSABLE) keeps sidebar tabs out of the Tab key order.
                    // Sidebar navigation uses ↑↓ keys; Tab should go directly to content.
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(available_width, 28.0),
                        egui::Sense::CLICK,
                    );

                    // Background
                    if selected {
                        ui.painter().rect_filled(rect, CornerRadius::same(4), TAB_SELECTED_BG);
                        // Left accent indicator (WinUI style)
                        let indicator = egui::Rect::from_min_size(
                            rect.left_top() + egui::vec2(0.0, 6.0),
                            egui::vec2(3.0, rect.height() - 12.0),
                        );
                        ui.painter().rect_filled(indicator, CornerRadius::same(2), ACCENT);
                        // Keyboard focus ring: shown when sidebar holds logical focus
                        if self.sidebar_focused {
                            ui.painter().rect_stroke(
                                rect,
                                CornerRadius::same(4),
                                egui::Stroke::new(1.5, ACCENT),
                                egui::StrokeKind::Inside,
                            );
                        }
                    } else if response.hovered() {
                        ui.painter().rect_filled(rect, CornerRadius::same(4), TAB_HOVER);
                    }

                    // Text
                    let text_pos = rect.left_center() + egui::vec2(12.0, 0.0);
                    let label = if tab.has_changes(&self.draft, &self.saved) {
                        format!("{} •", tab.label(&self.tr))
                    } else {
                        tab.label(&self.tr).to_string()
                    };
                    ui.painter().text(
                        text_pos,
                        egui::Align2::LEFT_CENTER,
                        label,
                        egui::TextStyle::Body.resolve(ui.style()),
                        if selected { TEXT_PRIMARY } else { TEXT_SECONDARY },
                    );

                    if response.clicked() {
                        self.active_tab = tab;
                        self.sidebar_focused = true;
                        ctx.memory_mut(|m| m.surrender_focus(sidebar_sentinel_id));
                    }
                }

                // Version info at the bottom of sidebar
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        // Use non-focusable sense so links don't appear in Tab order.
                        let link_sense = egui::Sense::CLICK;
                        if ui.add(egui::Label::new(egui::RichText::new("Web").small().color(ui.visuals().hyperlink_color)).sense(link_sense)).clicked() {
                            let _ = open::that("https://blankrune.sakura.ne.jp/");
                        }
                        if ui.add(egui::Label::new(egui::RichText::new("Mail").small().color(ui.visuals().hyperlink_color)).sense(link_sense)).clicked() {
                            let _ = open::that("mailto:algiz.rune@gmail.com?subject=Snotra%E3%81%AB%E3%81%A4%E3%81%84%E3%81%A6");
                        }
                    });
                    ui.label(egui::RichText::new("Fine Lagusaz").small().color(TEXT_SECONDARY));
                    let version = env!("CARGO_PKG_VERSION");
                    ui.label(egui::RichText::new(format!("v{version}")).small().color(TEXT_SECONDARY));
                    ui.label(egui::RichText::new("Snotra").small().color(TEXT_SECONDARY));
                });
            });

        // Footer (hide action buttons on Backup tab)
        egui::Panel::bottom("footer")
            .exact_size(40.0)
            .frame(egui::Frame::NONE.fill(FOOTER_BG).inner_margin(egui::Margin::symmetric(12, 0)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    // Status (timer message takes priority; otherwise show persistent unsaved indicator)
                    let status_text = if !self.status.is_empty() {
                        Some(&self.status as &str)
                    } else if self.has_changes() {
                        Some(self.tr.t(TrKey::StatusUnsaved))
                    } else {
                        None
                    };
                    if let Some(text) = status_text {
                        ui.label(text);
                        ui.separator();
                    }

                    if self.active_tab != TabId::Backup {
                        // 明示 id でアクションボタン群の auto-id を status ラベルの有無から独立させる。
                        // status ラベルが dirty の切り替わりで出現/消失すると前置ウィジェット数が変わり、
                        // 明示 id がないと RTL（矩形固定）のボタン auto-id がフレーム間で変化して egui の
                        // `warn_if_rect_changes_id`（debug 限定の赤枠）が発火する。`push_id`/`id_salt`
                        // （`IdSource::Child`）は unique_id に親カウンタを混ぜるため解消できず、
                        // `UiBuilder::id`（`IdSource::Explicit`）のみがカウンタ混入を断つ（#456）。
                        // `with_layout` は `scope_builder(UiBuilder::new().layout(..), ..)` の薄いラッパ
                        // なので、`.id()` を足すだけでレイアウト・挙動は不変。
                        let action_group = egui::UiBuilder::new()
                            .id(egui::Id::new("footer_actions"))
                            .layout(egui::Layout::right_to_left(egui::Align::Center));
                        ui.scope_builder(action_group, |ui| {
                            ui.spacing_mut().button_padding = egui::vec2(12.0, 4.0);

                            // Save button (disabled when no changes; finding C: also gated by
                            // the read-failure confirm checkbox so a temporarily-unreadable
                            // config is not silently overwritten with defaults)
                            let can_save = self.has_changes()
                                && (!self.loaded_read_failed
                                    || self.confirm_overwrite_despite_read_failure);
                            if ui
                                .add_enabled(can_save, egui::Button::new(self.tr.t(TrKey::BtnSave)))
                                .clicked()
                            {
                                self.save();
                            }

                            // Discard button (always visible, disabled when no changes)
                            if ui
                                .add_enabled(self.has_changes(), egui::Button::new(self.tr.t(TrKey::BtnDiscard)))
                                .clicked()
                            {
                                self.draft = self.saved.clone();
                                self.tr = Tr(self.draft.general.language);
                            }

                            // Reset to default
                            if ui.button(self.tr.t(TrKey::BtnResetDefault)).clicked() {
                                self.reset_to_default();
                            }
                        });
                    }
                });
            });

        // Main content
        egui::CentralPanel::default().show(ui, |ui| {
            // Sentinel: 0-size focusable widget placed first in Tab order.
            // Shift+Tab from the first content widget lands here (via id_next_frame),
            // which is detected next frame to return focus to the sidebar.
            // ui.interact() does not advance the layout cursor, so rendering is unaffected.
            let sentinel_rect =
                egui::Rect::from_min_size(ui.next_widget_position(), egui::vec2(0.0, 0.0));
            ui.interact(sentinel_rect, sidebar_sentinel_id, egui::Sense::focusable_noninteractive());

            // finding C: read 失敗起因で既定値表示中の警告バナー + 保存確認チェック。
            // チェックを入れるまで Save は無効（下の footer 参照）。
            if self.loaded_read_failed {
                egui::Frame::NONE
                    .fill(BANNER_CAUTION_BG)
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.colored_label(
                            BANNER_CAUTION_FG,
                            self.tr.t(TrKey::ReadFailedBanner),
                        );
                        ui.checkbox(
                            &mut self.confirm_overwrite_despite_read_failure,
                            self.tr.t(TrKey::ReadFailedConfirm),
                        );
                    });
                ui.add_space(8.0);
            }

            match self.active_tab {
                TabId::General => tabs::general::ui(ui, &mut self.draft, &mut self.hotkey_state, &self.tr),
                TabId::Search => tabs::search::ui(ui, &mut self.draft, &self.tr),
                TabId::Index => tabs::index::ui(ui, &ctx, &mut self.draft, &mut self.index_state, &self.tr),
                TabId::Visual => tabs::visual::ui(ui, &mut self.draft, &self.font_list, &self.tr),
                TabId::Opener => tabs::opener::ui(ui, &ctx, &mut self.draft, &mut self.opener_state, &self.tr),
                TabId::InstantCommand => tabs::instant::ui(ui, &ctx, &mut self.draft, &mut self.instant_state, &self.tr),
                TabId::Backup => {
                    if let Some(result) = tabs::backup::ui(ui, &ctx, &mut self.backup_state, &self.tr) {
                        self.draft = result.imported_config.clone();
                        self.saved = result.imported_config;
                        self.tr = Tr(self.draft.general.language);
                        // 明示インポートで良い config を読み込んだので read 失敗ガードは解除
                        // （save() 成功パスと対称に両フラグを戻す）
                        self.loaded_read_failed = false;
                        self.confirm_overwrite_despite_read_failure = false;
                    }
                }
            }
        });

        // Track window position for save on exit (skip when minimized to avoid saving 0,0)
        let minimized = ctx.input(|i| i.viewport().minimized).unwrap_or(false);
        if !minimized
            && let Some(rect) = ctx.input(|i| i.viewport().outer_rect)
        {
            let pos = rect.left_top();
            self.last_position = Some(WindowPlacement {
                x: pos.x as i32,
                y: pos.y as i32,
            });
        }
    }
}

fn load_icon() -> egui::IconData {
    let png = include_bytes!("../../src-tauri/icons/32x32.png");
    let img = image::load_from_memory(png).expect("icon png").into_rgba8();
    let (w, h) = img.dimensions();
    egui::IconData {
        rgba: img.into_raw(),
        width: w,
        height: h,
    }
}

pub fn run(
    config: Config,
    first_run: bool,
    initial_tab: Option<String>,
    load_outcome: LoadOutcome,
) -> eframe::Result {
    let icon = load_icon();
    let title = Tr(config.general.language).t(TrKey::WindowTitle);
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(title)
        .with_inner_size([760.0, 560.0])
        .with_min_inner_size([520.0, 360.0])
        .with_icon(icon);

    if let Some(pos) = window_data::load_settings_placement() {
        viewport = viewport.with_position(egui::pos2(pos.x as f32, pos.y as f32));
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        title,
        options,
        Box::new(move |cc| {
            let heading_semibold = crate::font::configure_fonts(&cc.egui_ctx);
            apply_win11_theme(&cc.egui_ctx);
            style::apply_type_ramp(&cc.egui_ctx, heading_semibold);
            Ok(Box::new(SettingsApp::new(config, first_run, initial_tab, load_outcome)))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use snotra_core::config::{InstantAction, InstantCommand, Language, OpenerRule, OpenerTool, ScanPath};

    /// Config の全トップレベルフィールドを1つずつ変更する mutation の一覧。
    ///
    /// 冒頭の `..` なし destructure が Config のフィールド網羅をコンパイル時に強制する:
    /// Config に新フィールド（新セクション）を追加するとここがコンパイルエラーになり、
    /// mutation と `SECTION_TABLE` の両方に対応を追加するまで検出が続く。
    type FieldMutation = (&'static str, fn(&mut Config));

    fn field_mutations() -> Vec<FieldMutation> {
        // 網羅性ガード: 新フィールド追加でコンパイルエラーにする（`..` を使わないこと）。
        let Config {
            hotkey: _,
            general: _,
            appearance: _,
            visual: _,
            paths: _,
            search: _,
            openers: _,
            instant_commands: _,
        } = Config::normalized_default();
        vec![
            ("hotkey", |c: &mut Config| c.hotkey.key = "Z".to_string()),
            ("general", |c: &mut Config| {
                c.general.show_on_startup = !c.general.show_on_startup;
            }),
            ("appearance", |c: &mut Config| c.appearance.window_width += 10),
            ("visual", |c: &mut Config| {
                c.visual.background_color = "#123456".to_string();
            }),
            ("paths", |c: &mut Config| {
                c.paths.scan.push(ScanPath {
                    path: "C:/mutation".to_string(),
                    extensions: vec![".txt".to_string()],
                    include_folders: false,
                });
            }),
            ("search", |c: &mut Config| {
                c.search.show_hidden_system = !c.search.show_hidden_system;
            }),
            ("openers", |c: &mut Config| {
                c.openers.push(OpenerRule {
                    target: "folder".to_string(),
                    tools: vec![OpenerTool {
                        name: "t".to_string(),
                        exe: "x.exe".to_string(),
                        args: String::new(),
                    }],
                });
            }),
            ("instant_commands", |c: &mut Config| {
                c.instant_commands.push(InstantCommand {
                    name: "mutation".to_string(),
                    description: String::new(),
                    action: InstantAction::Url { url: "https://example.com".to_string() },
                });
            }),
        ]
    }

    // 不変条件: SECTION_TABLE（全 TabId の対応の合成）は全体判定 `draft != saved` と一致する。
    // Config のどのフィールドを変えても、少なくとも1つのタブ点が点灯しなければならない。
    #[test]
    fn section_table_covers_all_config_fields() {
        let saved = Config::normalized_default();
        for (name, mutate) in field_mutations() {
            let mut draft = saved.clone();
            mutate(&mut draft);
            assert_ne!(draft, saved, "mutation `{name}` must actually change the config");
            let any_tab_dirty = TabId::ALL.iter().any(|t| t.has_changes(&draft, &saved));
            assert!(
                any_tab_dirty,
                "SECTION_TABLE lacks a mapping for config field `{name}`: \
                 tab dot would stay off while overall draft != saved is true"
            );
        }
    }

    // 不変条件: draft == saved のとき、どのタブ点も点灯しない（偽陽性なし）。
    #[test]
    fn section_table_no_false_positive_when_unchanged() {
        let saved = Config::normalized_default();
        let draft = saved.clone();
        for &tab in TabId::ALL {
            assert!(!tab.has_changes(&draft, &saved), "{tab:?} must not light when unchanged");
        }
    }

    // 不変条件: Backup タブは draft/saved に参加しないため、どの変更でも点灯しない。
    #[test]
    fn backup_tab_never_shows_dirty_dot() {
        let saved = Config::normalized_default();
        for (name, mutate) in field_mutations() {
            let mut draft = saved.clone();
            mutate(&mut draft);
            assert!(
                !TabId::Backup.has_changes(&draft, &saved),
                "Backup tab must not light for `{name}`"
            );
        }
    }

    // === egui_kittest による AccessKit ヘッドレス操作テスト（#440） ===
    //
    // 対象は「フッターボタン（Save/Discard/Reset）の wiring + draft/saved フロー」——
    // 実 UI を操作して初めて検証できる死角。dirty-dot 導出（`section_table_*`）と
    // モーダル状態機械（`tabs::common::tests`）は純ロジックテスト済みのため重複させない。
    //
    // 実装メモ:
    // - `Harness::new_ui_state` に SettingsApp を state として渡し、Frame 非依存の
    //   `ui_impl` を呼ぶ。`harness.state()` で内部状態（private フィールド含む・同 mod）を観測。
    // - `run()` は「repaint 要求が止まる」収束前提だが、この UI は checkbox 等の
    //   アニメーションで毎フレーム repaint を要求し発散する。観測対象は描画の収束ではなく
    //   draft の内部状態なので、固定ステップ（`settle`）でクリック（press→release）を
    //   処理させれば十分。wgpu 不要 = GPU なし CI で決定的に回る。
    // - 言語は En 固定（`default_language()` は OS 依存でラベルが非決定的になるため）。

    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    /// En 言語の SettingsApp を Harness に載せる（未編集 = clean 状態）。
    fn en_harness(config: Config) -> Harness<'static, SettingsApp> {
        let app = SettingsApp::new(config, false, None, LoadOutcome::Loaded);
        Harness::new_ui_state(|ui, app: &mut SettingsApp| app.ui_impl(ui), app)
    }

    fn en_config() -> Config {
        let mut config = Config::normalized_default();
        config.general.language = Language::En;
        config
    }

    /// クリック（press→release）とアニメーションを固定ステップで処理する。
    fn settle(harness: &mut Harness<'static, SettingsApp>) {
        for _ in 0..4 {
            harness.step();
        }
    }

    // 不変条件: General タブの checkbox クリックで draft != saved（dirty）になる。
    #[test]
    fn kittest_checkbox_click_makes_draft_dirty() {
        let mut harness = en_harness(en_config());
        assert!(!harness.state().has_changes(), "initial state must be clean");

        harness.get_by_label(Tr(Language::En).t(TrKey::CbHotkeyToggle)).click();
        settle(&mut harness);

        assert!(
            harness.state().has_changes(),
            "clicking the checkbox must make draft != saved (dirty)"
        );
    }

    // 不変条件: Discard ボタンで draft が saved に戻る（編集が破棄される）。
    #[test]
    fn kittest_discard_reverts_draft_to_saved() {
        let mut harness = en_harness(en_config());
        let original_toggle = harness.state().saved.general.hotkey_toggle;

        // 編集して dirty にする（Discard は has_changes() で有効化されるため先に編集）。
        harness.get_by_label(Tr(Language::En).t(TrKey::CbHotkeyToggle)).click();
        settle(&mut harness);
        assert!(harness.state().has_changes());
        assert_ne!(harness.state().draft.general.hotkey_toggle, original_toggle);

        // Discard → draft = saved.clone()。
        harness.get_by_label(Tr(Language::En).t(TrKey::BtnDiscard)).click();
        settle(&mut harness);

        assert!(!harness.state().has_changes(), "discard must clear dirty state");
        assert_eq!(
            harness.state().draft.general.hotkey_toggle,
            original_toggle,
            "discard must revert the edited field"
        );
    }

    // 不変条件: Reset to default で draft が normalized_default になり、
    // saved が非既定なら has_changes() が true になる（保存が必要な状態）。
    #[test]
    fn kittest_reset_to_default_makes_dirty() {
        // saved を「既定と1フィールド違う」状態にする（reset で差分が生まれるように）。
        let default_toggle = Config::normalized_default().general.hotkey_toggle;
        let mut config = en_config();
        config.general.hotkey_toggle = !default_toggle;
        let mut harness = en_harness(config);
        assert!(!harness.state().has_changes(), "saved==draft initially = clean");

        harness.get_by_label(Tr(Language::En).t(TrKey::BtnResetDefault)).click();
        settle(&mut harness);

        assert_eq!(
            harness.state().draft.general.hotkey_toggle,
            default_toggle,
            "reset must set draft to normalized_default value"
        );
        assert!(
            harness.state().has_changes(),
            "reset changes draft but not saved → must be dirty (needs Save)"
        );
    }

    // 不変条件: Save ボタンでバリデーションエラーがあると、ステータスにエラーが出て
    // saved は更新されない（不正な draft を disk に書かない）。Save→validate 経路の UI wiring。
    #[test]
    fn kittest_save_with_validation_error_keeps_saved() {
        let mut harness = en_harness(en_config());
        let saved_before = harness.state().saved.clone();

        // draft を「dirty かつ不正」にする（UI の DragValue で不正値まで駆動するのは煩雑なため
        // 中間状態を直接構築。テストが検証するのは Save ボタン→save()→validate 分岐の wiring）。
        // window_width を最小未満にすると ConfigError::WindowWidthTooSmall になる。
        harness.state_mut().draft.appearance.window_width = 1;
        assert!(harness.state().has_changes(), "draft must be dirty so Save is enabled");

        harness.get_by_label(Tr(Language::En).t(TrKey::BtnSave)).click();
        settle(&mut harness);

        assert!(
            !harness.state().status.is_empty(),
            "validation error must surface a status message"
        );
        assert_eq!(
            harness.state().saved, saved_before,
            "saved must not be updated when validation fails"
        );
    }

    // 不変条件: 「破棄」クリックの遷移フレームで egui の id 不安定性警告
    // （`warn_if_rect_changes_id` = 赤枠 2px, debug 限定）が出ないこと（#456）。
    //
    // 破棄で dirty が解消するとフッターの status ラベル（「未保存の変更があります」）が消え、
    // 前置ウィジェット数が変わる。アクションボタン群を RTL（右寄せ・矩形固定）で並べているため、
    // 明示 id を与えないと配下ボタンの auto-id がフレーム間で変化し「同矩形・別 id」警告が出る。
    // ボタン群コンテナの明示 id（`UiBuilder::id`）がこれを防ぐことを保証する。
    //
    // 検出マーカー: `warn_if_rect_changes_id` は赤 2px 枠を `Color32::RED`（egui context.rs で
    // ハードコード）で描く。`check_for_id_clash`（ID 重複）の 1px 枠は `error_fg_color`（既定で
    // 同じ赤）のため、`== Color32::RED` フィルタで両方の回帰を捕捉できる。
    // `.click()` は press+release を 1 step 内で処理し過渡フレームを潰すため、
    // ポインタイベントを `_step` 単位で個別に送り、各フレームで警告矩形 0 件を確認する。
    #[test]
    fn kittest_discard_no_rect_id_instability_warning() {
        fn red_error_rects(harness: &Harness<'static, SettingsApp>) -> usize {
            harness
                .output()
                .shapes
                .iter()
                .filter(|cs| {
                    matches!(&cs.shape, egui::Shape::Rect(r) if r.stroke.color == egui::Color32::RED)
                })
                .count()
        }

        let mut h = en_harness(en_config());
        h.set_size(egui::vec2(760.0, 560.0));

        // dirty 化（status ラベルが出る = フッターの前置ウィジェットが増える）
        h.get_by_label(Tr(Language::En).t(TrKey::CbHotkeyToggle)).click();
        settle(&mut h);
        assert!(h.state().has_changes());

        let center = h.get_by_label(Tr(Language::En).t(TrKey::BtnDiscard)).rect().center();
        let button = |pressed: bool| egui::Event::PointerButton {
            pos: center,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };

        // hover → press → hold → release → 遷移直後（修正前はここで赤枠が出る）→ +1
        let sequence = [
            Some(egui::Event::PointerMoved(center)),
            Some(button(true)),
            None, // hold
            Some(button(false)),
            None, // 遷移フレーム
            None, // 遷移+1
        ];
        for (i, ev) in sequence.into_iter().enumerate() {
            if let Some(ev) = ev {
                h.event(ev);
            }
            h.step();
            assert_eq!(
                red_error_rects(&h),
                0,
                "discard 遷移フレーム{i}で egui の id 不安定性警告（赤枠）が出た（#456 回帰）"
            );
        }
        assert!(!h.state().has_changes(), "discard で dirty が解消しているはず");
    }

    // ピン留めテスト（issue #438）: i18n テーブル駆動化のリファクタで `config_error_message` の
    // 出力文字列がバイト単位で不変であることを保証する。ConfigError 全12 variant × ja/en。
    fn tr_ja() -> Tr {
        Tr(Language::Ja)
    }

    fn tr_en() -> Tr {
        Tr(Language::En)
    }

    #[test]
    fn config_error_message_hotkey_modifier_empty() {
        assert_eq!(
            config_error_message(&ConfigError::HotkeyModifierEmpty, &tr_ja()),
            "ホットキーの修飾キーが未設定です"
        );
        assert_eq!(
            config_error_message(&ConfigError::HotkeyModifierEmpty, &tr_en()),
            "Hotkey modifier is not set"
        );
    }

    #[test]
    fn config_error_message_hotkey_key_empty() {
        assert_eq!(
            config_error_message(&ConfigError::HotkeyKeyEmpty, &tr_ja()),
            "ホットキーのキーが未設定です"
        );
        assert_eq!(
            config_error_message(&ConfigError::HotkeyKeyEmpty, &tr_en()),
            "Hotkey key is not set"
        );
    }

    #[test]
    fn config_error_message_hotkey_system_conflict() {
        let err = ConfigError::HotkeySystemConflict {
            modifier: "Alt".to_string(),
            key: "Space".to_string(),
        };
        assert_eq!(
            config_error_message(&err, &tr_ja()),
            "Alt+Space はシステムショートカットと競合します"
        );
        assert_eq!(
            config_error_message(&err, &tr_en()),
            "Alt+Space conflicts with a system shortcut"
        );
    }

    #[test]
    fn config_error_message_visible_rows_zero() {
        assert_eq!(
            config_error_message(&ConfigError::VisibleRowsZero, &tr_ja()),
            "最大表示件数は1以上にしてください"
        );
        assert_eq!(
            config_error_message(&ConfigError::VisibleRowsZero, &tr_en()),
            "Max results must be at least 1"
        );
    }

    #[test]
    fn config_error_message_window_width_too_small() {
        let err = ConfigError::WindowWidthTooSmall(150);
        assert_eq!(config_error_message(&err, &tr_ja()), "150 は小さすぎます（200以上）");
        assert_eq!(config_error_message(&err, &tr_en()), "150 is too small (min 200)");
    }

    #[test]
    fn config_error_message_fuzzy_cap_ratio_out_of_range() {
        let err = ConfigError::FuzzyCapRatioOutOfRange { value: 1.5 };
        assert_eq!(
            config_error_message(&err, &tr_ja()),
            "1.5 は 0.0〜1.0 の範囲にしてください"
        );
        assert_eq!(
            config_error_message(&err, &tr_en()),
            "1.5 must be between 0.0 and 1.0"
        );
    }

    #[test]
    fn config_error_message_scan_path_empty() {
        let err = ConfigError::ScanPathEmpty { index: 0 };
        assert_eq!(config_error_message(&err, &tr_ja()), "1のパスが空です");
        assert_eq!(config_error_message(&err, &tr_en()), "1 path is empty");
    }

    #[test]
    fn config_error_message_instant_command_prefix_empty() {
        assert_eq!(
            config_error_message(&ConfigError::InstantCommandPrefixEmpty, &tr_ja()),
            "インスタントコマンドのプレフィックスが空です"
        );
        assert_eq!(
            config_error_message(&ConfigError::InstantCommandPrefixEmpty, &tr_en()),
            "Instant command prefix is empty"
        );
    }

    #[test]
    fn config_error_message_instant_command_prefix_slash() {
        assert_eq!(
            config_error_message(&ConfigError::InstantCommandPrefixSlash, &tr_ja()),
            "プレフィックスに / は使用できません（スラッシュコマンドと競合）"
        );
        assert_eq!(
            config_error_message(&ConfigError::InstantCommandPrefixSlash, &tr_en()),
            "Prefix cannot be / (conflicts with slash commands)"
        );
    }

    #[test]
    fn config_error_message_instant_command_duplicate_name() {
        let err = ConfigError::InstantCommandDuplicateName { name: "g".to_string() };
        assert_eq!(config_error_message(&err, &tr_ja()), "g はコマンド名が重複しています");
        assert_eq!(config_error_message(&err, &tr_en()), "g has a duplicate command name");
    }

    #[test]
    fn config_error_message_instant_command_unknown_modifier() {
        let err = ConfigError::InstantCommandUnknownModifier {
            name: "g".to_string(),
            modifier: "xyz".to_string(),
        };
        assert_eq!(
            config_error_message(&err, &tr_ja()),
            "g: 不明な修飾子「xyz」が含まれています"
        );
        assert_eq!(
            config_error_message(&err, &tr_en()),
            "g: contains unknown modifier \"xyz\""
        );
    }

    #[test]
    fn config_error_message_migemo_min_chars_zero() {
        assert_eq!(
            config_error_message(&ConfigError::MigemoMinCharsZero, &tr_ja()),
            "migemo 最小文字数は 1 以上である必要があります"
        );
        assert_eq!(
            config_error_message(&ConfigError::MigemoMinCharsZero, &tr_en()),
            "Migemo min chars must be at least 1"
        );
    }
}
