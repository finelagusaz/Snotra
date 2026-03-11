use eframe::egui;
use egui::{Color32, CornerRadius, Stroke};
use snotra_core::config::{Config, ConfigError, Language};
use snotra_core::window_data::{self, WindowPlacement};

use crate::i18n::Tr;
use crate::tabs;

// Windows 11 Settings-inspired color palette
const SIDEBAR_BG: Color32 = Color32::from_rgb(243, 243, 243); // WinUI NavigationView pane
const CONTENT_BG: Color32 = Color32::from_rgb(249, 249, 249); // WinUI content area
const FOOTER_BG: Color32 = Color32::from_rgb(243, 243, 243);
const ACCENT: Color32 = Color32::from_rgb(0, 103, 192); // Windows 11 default blue
const TAB_HOVER: Color32 = Color32::from_rgb(232, 232, 232);
const TAB_SELECTED_BG: Color32 = Color32::from_rgb(255, 255, 255);
const TEXT_PRIMARY: Color32 = Color32::from_rgb(26, 26, 26);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(96, 96, 96);
const WIDGET_BG: Color32 = Color32::from_rgb(255, 255, 255);
const WIDGET_BORDER: Color32 = Color32::from_rgb(210, 210, 210);

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

    visuals.widgets.active.bg_fill = Color32::from_rgb(220, 220, 220);
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
            TabId::General => tr.tab_general(),
            TabId::Search => tr.tab_search(),
            TabId::Index => tr.tab_index(),
            TabId::Visual => tr.tab_visual(),
            TabId::Opener => tr.tab_opener(),
            TabId::InstantCommand => tr.tab_instant_command(),
            TabId::Backup => tr.tab_backup(),
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

    fn has_changes(self, draft: &Config, saved: &Config) -> bool {
        match self {
            TabId::General => draft.general != saved.general || draft.hotkey != saved.hotkey,
            TabId::Search => draft.search != saved.search,
            TabId::Index => draft.paths != saved.paths,
            TabId::Visual => draft.visual != saved.visual || draft.appearance != saved.appearance,
            TabId::Opener => draft.openers != saved.openers,
            TabId::InstantCommand => draft.instant_commands != saved.instant_commands,
            TabId::Backup => false,
        }
    }
}

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
}

impl SettingsApp {
    fn new(config: Config, first_run: bool, initial_tab: Option<String>) -> Self {
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
        }
    }

    fn has_changes(&self) -> bool {
        self.draft != self.saved
    }

    fn save(&mut self) {
        let mut config = self.draft.clone();
        config.paths.normalize_scan_paths();
        config.normalize_openers();
        let errors = config.validate();
        if !errors.is_empty() {
            self.status = format!(
                "{}{}",
                self.tr.status_validation_error(),
                config_error_message(&errors[0], &self.tr)
            );
            self.status_timer = 5.0;
            return;
        }
        if let Err(e) = config.save() {
            self.status = format!("{}{e}", self.tr.status_save_failed());
            self.status_timer = 5.0;
            return;
        }
        self.saved = config.clone();
        self.draft = config;
        // Update tr for potential language change
        self.tr = Tr(self.draft.general.language);
        self.status = self.tr.status_saved().to_string();
        self.status_timer = 2.0;
    }

    fn reset_to_default(&mut self) {
        self.draft = Config::default();
    }
}

fn config_error_message(error: &ConfigError, tr: &Tr) -> String {
    match error {
        ConfigError::HotkeyModifierEmpty => tr.err_hotkey_modifier_empty().to_string(),
        ConfigError::HotkeyKeyEmpty => tr.err_hotkey_key_empty().to_string(),
        ConfigError::HotkeySystemConflict { modifier, key } => {
            format!("{}+{}{}", modifier, key, tr.err_hotkey_system_conflict())
        }
        ConfigError::MaxResultsZero => tr.err_max_results_zero().to_string(),
        ConfigError::WindowWidthTooSmall(w) => {
            format!("{}{}", w, tr.err_window_width_too_small())
        }
        ConfigError::FuzzyCapRatioOutOfRange { value } => {
            format!("{}{}", value, tr.err_fuzzy_cap_ratio_out_of_range())
        }
        ConfigError::ScanPathEmpty { index } => {
            format!("{}{}", index + 1, tr.err_scan_path_empty())
        }
        ConfigError::InstantCommandPrefixEmpty => tr.err_instant_prefix_empty().to_string(),
        ConfigError::InstantCommandPrefixSlash => tr.err_instant_prefix_slash().to_string(),
        ConfigError::InstantCommandDuplicateName { name } => {
            format!("{}{}", name, tr.err_instant_duplicate_name())
        }
        ConfigError::MigemoMinCharsZero => tr.err_migemo_min_chars_zero().to_string(),
    }
}

impl eframe::App for SettingsApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(pos) = self.last_position {
            window_data::save_settings_placement(pos);
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Update window title (language change + dirty indicator)
        let title = if self.has_changes() {
            format!("{}*", self.tr.window_title())
        } else {
            self.tr.window_title().to_string()
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        // Close on Escape (skip when hotkey capture is active)
        if !self.hotkey_state.is_capturing() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.has_changes() {
                self.status = self.tr.status_unsaved().to_string();
                self.status_timer = 3.0;
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        // Prevent close via × button / Alt+F4 when there are unsaved changes
        if ctx.input(|i| i.viewport().close_requested()) && self.has_changes() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.status = self.tr.status_unsaved().to_string();
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
        egui::SidePanel::left("tabs_panel")
            .resizable(false)
            .exact_width(140.0)
            .frame(egui::Frame::NONE.fill(SIDEBAR_BG).inner_margin(egui::Margin::symmetric(8, 8)))
            .show(ctx, |ui| {
                let available_width = ui.available_width();

                // Tab list
                for &tab in TabId::ALL {
                    let selected = self.active_tab == tab;
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(available_width, 28.0),
                        egui::Sense::click(),
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
                    }
                }

                // Version info at the bottom of sidebar
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.link(egui::RichText::new("Web").small()).clicked() {
                            let _ = open::that("https://blankrune.sakura.ne.jp/");
                        }
                        if ui.link(egui::RichText::new("Mail").small()).clicked() {
                            let _ = open::that("mailto:algiz.rune@gmail.com?subject=Snotra%E3%81%AB%E3%81%A4%E3%81%84%E3%81%A6");
                        }
                    });
                    ui.label(egui::RichText::new("Fine Lagusaz").small().color(TEXT_SECONDARY));
                    let version = env!("CARGO_PKG_VERSION");
                    ui.label(egui::RichText::new(format!("v{version}")).small().color(TEXT_SECONDARY));
                    ui.label(egui::RichText::new("Snotra").small().color(TEXT_SECONDARY));
                });
            });

        // Footer (hide action buttons on About tab)
        egui::TopBottomPanel::bottom("footer")
            .exact_height(40.0)
            .frame(egui::Frame::NONE.fill(FOOTER_BG).inner_margin(egui::Margin::symmetric(12, 0)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    // Status (timer message takes priority; otherwise show persistent unsaved indicator)
                    let status_text = if !self.status.is_empty() {
                        Some(&self.status as &str)
                    } else if self.has_changes() {
                        Some(self.tr.status_unsaved())
                    } else {
                        None
                    };
                    if let Some(text) = status_text {
                        ui.label(text);
                        ui.separator();
                    }

                    if self.active_tab != TabId::Backup {
                        // Spacer
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.spacing_mut().button_padding = egui::vec2(12.0, 4.0);

                            // Save button (always "保存", disabled when no changes)
                            if ui
                                .add_enabled(self.has_changes(), egui::Button::new(self.tr.btn_save()))
                                .clicked()
                            {
                                self.save();
                            }

                            // Discard button (always visible, disabled when no changes)
                            if ui
                                .add_enabled(self.has_changes(), egui::Button::new(self.tr.btn_discard()))
                                .clicked()
                            {
                                self.draft = self.saved.clone();
                            }

                            // Reset to default
                            if ui.button(self.tr.btn_reset_default()).clicked() {
                                self.reset_to_default();
                            }
                        });
                    }
                });
            });

        // Main content
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.active_tab {
                TabId::General => tabs::general::ui(ui, &mut self.draft, &mut self.hotkey_state, &self.tr),
                TabId::Search => tabs::search::ui(ui, &mut self.draft, &self.tr),
                TabId::Index => tabs::index::ui(ui, ctx, &mut self.draft, &mut self.index_state, &self.tr),
                TabId::Visual => tabs::visual::ui(ui, &mut self.draft, &self.font_list, &self.tr),
                TabId::Opener => tabs::opener::ui(ui, ctx, &mut self.draft, &mut self.opener_state, &self.tr),
                TabId::InstantCommand => tabs::instant::ui(ui, ctx, &mut self.draft, &mut self.instant_state, &self.tr),
                TabId::Backup => {
                    if let Some(result) = tabs::backup::ui(ui, ctx, &mut self.backup_state, &self.tr) {
                        self.draft = result.imported_config.clone();
                        self.saved = result.imported_config;
                        self.tr = Tr(self.draft.general.language);
                    }
                }
            }
        });

        // Track window position for save on exit
        if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
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

pub fn run(config: Config, first_run: bool, initial_tab: Option<String>) -> eframe::Result {
    let icon = load_icon();
    let title = match config.general.language {
        Language::Ja => "Snotra 設定",
        Language::En => "Snotra Settings",
    };
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
            crate::font::configure_fonts(&cc.egui_ctx);
            apply_win11_theme(&cc.egui_ctx);
            Ok(Box::new(SettingsApp::new(config, first_run, initial_tab)))
        }),
    )
}
