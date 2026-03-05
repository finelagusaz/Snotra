use eframe::egui;
use snotra_core::config::Config;

use crate::tabs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabId {
    General,
    Search,
    Index,
    Visual,
    Opener,
    About,
}

impl TabId {
    const ALL: &[TabId] = &[
        TabId::General,
        TabId::Search,
        TabId::Index,
        TabId::Visual,
        TabId::Opener,
        TabId::About,
    ];

    fn label(self) -> &'static str {
        match self {
            TabId::General => "全般",
            TabId::Search => "検索",
            TabId::Index => "インデックス",
            TabId::Visual => "ビジュアル",
            TabId::Opener => "オープナー",
            TabId::About => "Snotra について",
        }
    }

    fn from_str(s: &str) -> Option<TabId> {
        match s {
            "general" => Some(TabId::General),
            "search" => Some(TabId::Search),
            "index" => Some(TabId::Index),
            "visual" => Some(TabId::Visual),
            "opener" => Some(TabId::Opener),
            "about" => Some(TabId::About),
            _ => None,
        }
    }
}

struct SettingsApp {
    draft: Config,
    saved: Config,
    active_tab: TabId,
    status: String,
    status_timer: f64,
    #[allow(dead_code)]
    first_run: bool,
    index_state: tabs::index::IndexTabState,
    opener_state: tabs::opener::OpenerTabState,
    font_list: Vec<String>,
    hotkey_state: crate::hotkey_input::HotkeyInputState,
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
        Self {
            draft: config.clone(),
            saved: config,
            active_tab: tab,
            status: String::new(),
            status_timer: 0.0,
            first_run,
            index_state: tabs::index::IndexTabState::default(),
            opener_state: tabs::opener::OpenerTabState::default(),
            font_list: crate::font::list_system_fonts(),
            hotkey_state: Default::default(),
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
            self.status = format!("検証エラー: {:?}", errors[0]);
            self.status_timer = 5.0;
            return;
        }
        config.save();
        self.saved = config.clone();
        self.draft = config;
    }

    fn reset_to_default(&mut self) {
        self.draft = Config::default();
    }
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Close on Escape (skip when hotkey capture is active)
        if !self.hotkey_state.is_capturing() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.has_changes() {
                self.status = "未保存の変更があります".to_string();
                self.status_timer = 3.0;
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
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
            .exact_width(120.0)
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.spacing_mut().button_padding = egui::vec2(8.0, 4.0);
                for &tab in TabId::ALL {
                    let selected = self.active_tab == tab;
                    if ui.selectable_label(selected, tab.label()).clicked() {
                        self.active_tab = tab;
                    }
                }
            });

        // Footer (hide action buttons on About tab)
        egui::TopBottomPanel::bottom("footer")
            .exact_height(40.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    // Status
                    if !self.status.is_empty() {
                        ui.label(&self.status);
                        ui.separator();
                    }

                    if self.active_tab != TabId::About {
                        // Spacer
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.spacing_mut().button_padding = egui::vec2(12.0, 4.0);

                            // Save button (always "保存", disabled when no changes)
                            if ui
                                .add_enabled(self.has_changes(), egui::Button::new("保存"))
                                .clicked()
                            {
                                self.save();
                            }

                            // Discard button (always visible, disabled when no changes)
                            if ui
                                .add_enabled(self.has_changes(), egui::Button::new("破棄"))
                                .clicked()
                            {
                                self.draft = self.saved.clone();
                            }

                            // Reset to default
                            if ui.button("初期設定に戻す").clicked() {
                                self.reset_to_default();
                            }
                        });
                    }
                });
            });

        // Main content
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.active_tab {
                TabId::General => tabs::general::ui(ui, &mut self.draft, &mut self.hotkey_state),
                TabId::Search => tabs::search::ui(ui, &mut self.draft),
                TabId::Index => tabs::index::ui(ui, ctx, &mut self.draft, &mut self.index_state),
                TabId::Visual => tabs::visual::ui(ui, &mut self.draft, &self.font_list),
                TabId::Opener => tabs::opener::ui(ui, ctx, &mut self.draft, &mut self.opener_state),
                TabId::About => {
                    ui.vertical_centered(|ui| {
                        ui.add_space(24.0);
                        ui.heading("Snotra");
                        ui.add_space(8.0);

                        let version = env!("CARGO_PKG_VERSION");
                        ui.label(format!("v{version}"));
                        ui.add_space(24.0);

                        ui.label("Fine Lagusaz");
                        ui.add_space(8.0);

                        if ui.link("algiz.rune@gmail.com").clicked() {
                            let _ = open::that("mailto:algiz.rune@gmail.com?subject=Snotra%E3%81%AB%E3%81%A4%E3%81%84%E3%81%A6");
                        }
                        if ui.link("https://blankrune.sakura.ne.jp/").clicked() {
                            let _ = open::that("https://blankrune.sakura.ne.jp/");
                        }
                    });
                }
            }
        });
    }
}

pub fn run(config: Config, first_run: bool, initial_tab: Option<String>) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Snotra 設定")
            .with_inner_size([760.0, 560.0])
            .with_min_inner_size([520.0, 360.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Snotra 設定",
        options,
        Box::new(move |cc| {
            crate::font::configure_fonts(&cc.egui_ctx);
            Ok(Box::new(SettingsApp::new(config, first_run, initial_tab)))
        }),
    )
}
