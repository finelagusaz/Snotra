use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Color32, ComboBox, FontId, RichText, ScrollArea, TextStyle, TextureHandle,
    TextureOptions, ViewportCommand,
};

use crate::config::{Config, ScanPath, SearchModeConfig, ThemePreset, VisualConfig};
use crate::folder;
use crate::history::HistoryStore;
use crate::icon;
use crate::indexer::{self, AppEntry};
use crate::launcher;
use crate::platform_win32::{PlatformBridge, PlatformCommand, PlatformEvent};
use crate::query;
use crate::search::{SearchEngine, SearchMode};
use crate::ui_types::{FolderExpansionState, SearchResult};
use crate::window_data;

const INPUT_HEIGHT: f32 = 36.0;
const ITEM_HEIGHT: f32 = 42.0;
const WINDOW_PADDING: f32 = 8.0;
const SPINNER_FRAMES: [char; 4] = ['|', '/', '-', '\\'];

#[derive(Clone, Copy)]
struct RuntimeSettings {
    max_results: usize,
    max_history_display: usize,
    normal_mode: SearchMode,
    folder_mode: SearchMode,
    show_hidden_system: bool,
    hotkey_toggle: bool,
    auto_hide_on_focus_lost: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    General,
    Search,
    Index,
    Visual,
}

enum InternalEvent {
    RebuildDone {
        entries: Vec<AppEntry>,
        reload_icons: bool,
    },
    RebuildFailed,
}

pub struct AppInit {
    pub config: Config,
    pub engine: SearchEngine,
    pub history: HistoryStore,
    pub icon_cache: Option<Rc<icon::IconCache>>,
    pub platform: PlatformBridge,
}

pub struct SnotraApp {
    config: Config,
    runtime: RuntimeSettings,
    engine: SearchEngine,
    history: HistoryStore,
    icon_cache: Option<Rc<icon::IconCache>>,
    icon_textures: HashMap<String, TextureHandle>,

    query: String,
    results: Vec<SearchResult>,
    selected: usize,
    folder_state: Option<FolderExpansionState>,

    show_search_window: bool,
    request_focus_input: bool,
    initial_window_applied: bool,

    settings_open: bool,
    settings_tab: SettingsTab,
    settings_draft: Config,
    settings_status: String,
    settings_scan_path: String,
    settings_scan_ext: String,
    settings_scan_include_folders: bool,
    selected_scan_index: Option<usize>,
    show_rebuild_confirm: bool,
    pending_rebuild_config: Option<Config>,
    rebuild_in_progress: bool,
    spinner_index: usize,
    last_spinner_tick: Instant,

    search_window_pos: Option<egui::Pos2>,
    settings_window_pos: Option<egui::Pos2>,

    platform: PlatformBridge,
    internal_tx: Sender<InternalEvent>,
    internal_rx: Receiver<InternalEvent>,

    should_exit: bool,
    exit_sent: bool,
    minimize_on_settings_close: bool,
}

impl SnotraApp {
    pub fn new(_cc: &eframe::CreationContext<'_>, init: AppInit) -> Self {
        let mut config = init.config;
        config.appearance.max_history_display = config
            .appearance
            .max_history_display
            .min(config.appearance.max_results);

        let (internal_tx, internal_rx) = mpsc::channel();

        let mut app = Self {
            runtime: runtime_from_config(&config),
            settings_draft: config.clone(),
            show_search_window: config.general.show_on_startup,
            request_focus_input: config.general.show_on_startup,
            initial_window_applied: false,
            settings_open: false,
            settings_tab: SettingsTab::General,
            settings_status: String::new(),
            settings_scan_path: String::new(),
            settings_scan_ext: String::new(),
            settings_scan_include_folders: false,
            selected_scan_index: None,
            show_rebuild_confirm: false,
            pending_rebuild_config: None,
            rebuild_in_progress: false,
            spinner_index: 0,
            last_spinner_tick: Instant::now(),
            query: String::new(),
            results: Vec::new(),
            selected: 0,
            folder_state: None,
            icon_textures: HashMap::new(),
            search_window_pos: window_data::load_search_placement()
                .map(|p| egui::pos2(p.x as f32, p.y as f32)),
            settings_window_pos: window_data::load_settings_placement()
                .map(|p| egui::pos2(p.x as f32, p.y as f32)),
            platform: init.platform,
            internal_tx,
            internal_rx,
            should_exit: false,
            exit_sent: false,
            minimize_on_settings_close: false,
            engine: init.engine,
            history: init.history,
            icon_cache: init.icon_cache,
            config,
        };

        app.refresh_results();
        app
    }

    fn handle_platform_events(&mut self, ctx: &egui::Context) {
        while let Some(event) = self.platform.try_recv_event() {
            match event {
                PlatformEvent::HotkeyPressed => {
                    if self.runtime.hotkey_toggle {
                        if self.show_search_window {
                            self.hide_search_window(ctx);
                        } else {
                            self.show_search_window(ctx);
                        }
                    } else {
                        self.show_search_window(ctx);
                    }
                }
                PlatformEvent::OpenSettings => {
                    self.open_settings_from_anywhere(ctx);
                }
                PlatformEvent::ExitRequested => {
                    self.should_exit = true;
                }
                PlatformEvent::InitialHotkeyFailed => {
                    self.show_search_window(ctx);
                    self.settings_status =
                        "ホットキー登録に失敗したため、ウィンドウを表示しています".to_string();
                }
            }
        }
    }

    fn handle_internal_events(&mut self) {
        while let Ok(event) = self.internal_rx.try_recv() {
            match event {
                InternalEvent::RebuildDone {
                    entries,
                    reload_icons,
                } => {
                    self.engine = SearchEngine::new(entries);
                    if reload_icons {
                        self.icon_cache = icon::IconCache::load().map(Rc::new);
                    } else {
                        self.icon_cache = None;
                    }
                    self.icon_textures.clear();
                    self.rebuild_in_progress = false;
                    self.spinner_index = 0;
                    self.settings_status = "インデックス再構築が完了しました".to_string();
                    self.refresh_results();
                }
                InternalEvent::RebuildFailed => {
                    self.rebuild_in_progress = false;
                    self.spinner_index = 0;
                    self.settings_status = "インデックス再構築に失敗しました".to_string();
                }
            }
        }
    }

    fn show_search_window(&mut self, ctx: &egui::Context) {
        self.show_search_window = true;
        self.request_focus_input = true;
        self.query.clear();
        self.selected = 0;
        self.folder_state = None;
        self.refresh_results();

        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);

        if self.config.general.ime_off_on_show {
            self.platform
                .send_command(PlatformCommand::TurnOffImeForForeground);
        }
    }

    fn hide_search_window(&mut self, ctx: &egui::Context) {
        self.show_search_window = false;
        self.persist_search_placement();
        ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
    }

    fn open_settings(&mut self) {
        self.settings_open = true;
        self.settings_draft = self.config.clone();
        self.settings_status.clear();
        self.settings_scan_path.clear();
        self.settings_scan_ext.clear();
        self.settings_scan_include_folders = false;
        self.selected_scan_index = None;
    }

    fn open_settings_from_anywhere(&mut self, ctx: &egui::Context) {
        self.open_settings();
        self.show_search_window = false;
        self.minimize_on_settings_close = true;
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    fn close_settings(&mut self, ctx: &egui::Context) {
        self.settings_open = false;
        self.show_rebuild_confirm = false;
        self.pending_rebuild_config = None;
        self.persist_settings_placement();
        if self.minimize_on_settings_close {
            ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
        }
        self.minimize_on_settings_close = false;
    }

    fn refresh_results(&mut self) {
        if let Some(folder_state) = self.folder_state.as_ref() {
            self.results = folder::list_folder(
                Path::new(&folder_state.current_dir),
                &self.query,
                self.runtime.folder_mode,
                self.runtime.show_hidden_system,
                &self.history,
                self.runtime.max_results,
            );
        } else if self.query.trim().is_empty() {
            self.results = self
                .engine
                .recent_history(&self.history, self.runtime.max_history_display);
        } else {
            self.results = self.engine.search(
                &self.query,
                self.runtime.max_results,
                &self.history,
                self.runtime.normal_mode,
            );
        }

        if self.results.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.results.len() {
            self.selected = self.results.len() - 1;
        }
    }

    fn move_selection_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    fn move_selection_down(&mut self) {
        if !self.results.is_empty() && self.selected < self.results.len() - 1 {
            self.selected += 1;
        }
    }

    fn enter_folder_expansion(&mut self, folder_path: &str) {
        let current_query = self.query.clone();

        if self.folder_state.is_none() {
            self.folder_state = Some(FolderExpansionState {
                current_dir: folder_path.to_string(),
                saved_results: std::mem::take(&mut self.results),
                saved_selected: self.selected,
                saved_query: current_query,
            });
        } else if let Some(fs) = self.folder_state.as_mut() {
            fs.current_dir = folder_path.to_string();
        }

        self.query.clear();
        self.results = folder::list_folder(
            Path::new(folder_path),
            "",
            self.runtime.folder_mode,
            self.runtime.show_hidden_system,
            &self.history,
            self.runtime.max_results,
        );
        self.selected = 0;
    }

    fn navigate_folder_up(&mut self) {
        let Some(fs) = self.folder_state.as_mut() else {
            return;
        };
        let Some(parent) = crate::folder::parent_for_navigation(&fs.current_dir) else {
            return;
        };

        fs.current_dir = parent.to_string_lossy().to_string();
        self.query.clear();
        self.results = folder::list_folder(
            Path::new(&fs.current_dir),
            "",
            self.runtime.folder_mode,
            self.runtime.show_hidden_system,
            &self.history,
            self.runtime.max_results,
        );
        self.selected = 0;
    }

    fn exit_folder_expansion(&mut self) -> bool {
        let Some(fs) = self.folder_state.take() else {
            return false;
        };

        self.query = fs.saved_query;
        self.results = fs.saved_results;
        self.selected = fs.saved_selected;
        true
    }

    fn activate_selected(&mut self, ctx: &egui::Context) {
        if query::normalize_query(&self.query) == "/o" {
            self.open_settings_from_anywhere(ctx);
            return;
        }

        let Some(result) = self.results.get(self.selected).cloned() else {
            return;
        };

        if result.is_error {
            return;
        }

        launcher::launch(&result.path);
        if !result.is_folder {
            self.history.record_launch(&result.path, &self.query);
        }
        self.hide_search_window(ctx);
    }

    fn handle_search_keyboard(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if !self.exit_folder_expansion() {
                self.hide_search_window(ctx);
            }
            return;
        }

        if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
            self.move_selection_up();
        }

        if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
            self.move_selection_down();
        }

        if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
            if let Some(result) = self.results.get(self.selected) {
                if result.is_folder {
                    let folder_path = result.path.clone();
                    self.history.record_folder_expansion(&folder_path);
                    self.enter_folder_expansion(&folder_path);
                }
            }
        }

        if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
            if self.folder_state.is_some() {
                self.navigate_folder_up();
            }
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            self.activate_selected(ctx);
        }
    }

    fn apply_visual_style(&self, ctx: &egui::Context) {
        let mut style = (*ctx.style()).clone();

        let bg = parse_hex_color(&self.config.visual.background_color, Color32::from_rgb(40, 40, 40));
        let input_bg = parse_hex_color(
            &self.config.visual.input_background_color,
            Color32::from_rgb(56, 56, 56),
        );
        let text = parse_hex_color(&self.config.visual.text_color, Color32::from_rgb(224, 224, 224));
        let selected = parse_hex_color(
            &self.config.visual.selected_row_color,
            Color32::from_rgb(80, 80, 80),
        );
        let hint = parse_hex_color(&self.config.visual.hint_text_color, Color32::from_rgb(128, 128, 128));

        style.visuals.panel_fill = bg;
        style.visuals.window_fill = bg;
        style.visuals.extreme_bg_color = input_bg;
        style.visuals.override_text_color = Some(text);
        style.visuals.selection.bg_fill = selected;
        style.visuals.widgets.noninteractive.fg_stroke.color = text;
        style.visuals.widgets.inactive.fg_stroke.color = text;
        style.visuals.widgets.hovered.fg_stroke.color = text;
        style.visuals.widgets.active.fg_stroke.color = text;
        style.visuals.weak_text_color = hint;

        let size = self.config.visual.font_size.clamp(8, 48) as f32;
        let family = normalize_visual_font_family(&self.config.visual.font_family);
        style
            .text_styles
            .insert(TextStyle::Body, FontId::new(size, family.clone()));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::new(size, family.clone()));
        style
            .text_styles
            .insert(TextStyle::Heading, FontId::new(size + 2.0, family));

        ctx.set_style(style);
    }

    fn sync_search_viewport_pos(&mut self, ctx: &egui::Context) {
        let pos = ctx.input(|i| i.viewport().outer_rect.map(|rect| rect.left_top()));
        if let Some(pos) = pos {
            self.search_window_pos = Some(pos);
        }
    }

    fn handle_auto_hide_on_focus_lost(&mut self, ctx: &egui::Context) {
        if !self.runtime.auto_hide_on_focus_lost || !self.show_search_window || self.settings_open {
            return;
        }

        if ctx.input(|i| i.viewport().focused) == Some(false) {
            self.hide_search_window(ctx);
        }
    }

    fn ensure_icon_texture(&mut self, ctx: &egui::Context, path: &str) -> Option<egui::TextureId> {
        if let Some(texture) = self.icon_textures.get(path) {
            return Some(texture.id());
        }

        let data = self.icon_cache.as_ref()?.icon_data(path)?;

        let mut rgba = data.bgra.clone();
        for px in rgba.chunks_exact_mut(4) {
            px.swap(0, 2);
        }

        let image = egui::ColorImage::from_rgba_unmultiplied(
            [data.width as usize, data.height as usize],
            &rgba,
        );

        let texture = ctx.load_texture(format!("icon:{}", path), image, TextureOptions::LINEAR);
        let id = texture.id();
        self.icon_textures.insert(path.to_string(), texture);
        Some(id)
    }

    fn draw_search_ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(WINDOW_PADDING);

            let input = egui::TextEdit::singleline(&mut self.query)
                .desired_width(f32::INFINITY)
                .hint_text("検索...");
            let input_response = ui.add_sized([ui.available_width(), INPUT_HEIGHT], input);
            if self.request_focus_input {
                input_response.request_focus();
                self.request_focus_input = false;
            }
            if input_response.changed() {
                self.selected = 0;
                self.refresh_results();
            }

            ui.add_space(6.0);

            ScrollArea::vertical().show(ui, |ui| {
                let rows = self.results.clone();
                for (idx, result) in rows.iter().enumerate() {
                    let selected = idx == self.selected;
                    let row_text = if result.is_folder {
                        format!("{}\n[DIR] {}", result.name, result.path)
                    } else {
                        format!("{}\n{}", result.name, result.path)
                    };

                    ui.horizontal(|ui| {
                        if self.config.appearance.show_icons {
                            if let Some(texture_id) = self.ensure_icon_texture(ctx, &result.path) {
                                ui.image((texture_id, egui::vec2(16.0, 16.0)));
                            } else {
                                ui.add_space(18.0);
                            }
                        }

                        let resp = ui.add_sized(
                            [ui.available_width(), ITEM_HEIGHT],
                            egui::SelectableLabel::new(selected, row_text),
                        );

                        if resp.clicked() {
                            self.selected = idx;
                        }
                        if resp.double_clicked() {
                            self.selected = idx;
                            self.activate_selected(ctx);
                        }
                    });
                }
            });
        });
    }

    fn draw_settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }

        let mut open = self.settings_open;
        let mut window = egui::Window::new("Snotra 設定")
            .open(&mut open)
            .resizable(true)
            .default_size([760.0, 560.0]);

        if let Some(pos) = self.settings_window_pos {
            window = window.default_pos(pos);
        }

        if let Some(resp) = window.show(ctx, |ui| self.draw_settings_contents(ui, ctx)) {
            self.settings_window_pos = Some(resp.response.rect.left_top());
        }

        if !open {
            self.close_settings(ctx);
        }
    }

    fn draw_settings_contents(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.settings_tab, SettingsTab::General, "全般");
            ui.selectable_value(&mut self.settings_tab, SettingsTab::Search, "検索");
            ui.selectable_value(&mut self.settings_tab, SettingsTab::Index, "インデックス");
            ui.selectable_value(&mut self.settings_tab, SettingsTab::Visual, "ビジュアル");
        });

        ui.separator();

        match self.settings_tab {
            SettingsTab::General => self.draw_settings_general(ui),
            SettingsTab::Search => self.draw_settings_search(ui),
            SettingsTab::Index => self.draw_settings_index(ui),
            SettingsTab::Visual => self.draw_settings_visual(ui),
        }

        ui.separator();

        ui.horizontal(|ui| {
            if ui.button("保存").clicked() && !self.rebuild_in_progress {
                self.save_settings(ctx);
            }
            if ui.button("再構築").clicked() && !self.rebuild_in_progress {
                self.pending_rebuild_config = Some(self.settings_draft.clone());
                self.show_rebuild_confirm = true;
            }
            if ui.button("閉じる").clicked() {
                self.close_settings(ctx);
            }
        });

        if !self.settings_status.is_empty() {
            ui.label(self.settings_status.clone());
        }

        if self.show_rebuild_confirm {
            egui::Window::new("インデックス再構築")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("インデックス再構築を開始しますか？");
                    ui.horizontal(|ui| {
                        if ui.button("開始").clicked() {
                            if let Some(cfg) = self.pending_rebuild_config.clone() {
                                self.start_rebuild(cfg);
                            }
                            self.pending_rebuild_config = None;
                            self.show_rebuild_confirm = false;
                        }
                        if ui.button("キャンセル").clicked() {
                            self.pending_rebuild_config = None;
                            self.show_rebuild_confirm = false;
                        }
                    });
                });
        }
    }

    fn draw_settings_general(&mut self, ui: &mut egui::Ui) {
        ui.label("ホットキー修飾キー (例: Alt, Ctrl+Shift)");
        ui.text_edit_singleline(&mut self.settings_draft.hotkey.modifier);
        ui.label("ホットキーキー (例: Q, Space)");
        ui.text_edit_singleline(&mut self.settings_draft.hotkey.key);

        ui.checkbox(
            &mut self.settings_draft.general.hotkey_toggle,
            "呼び出しキーで表示/非表示トグル",
        );
        ui.checkbox(
            &mut self.settings_draft.general.show_on_startup,
            "起動時にウィンドウ表示",
        );
        ui.checkbox(
            &mut self.settings_draft.general.auto_hide_on_focus_lost,
            "フォーカス喪失時の自動非表示",
        );
        ui.checkbox(
            &mut self.settings_draft.general.show_tray_icon,
            "タスクトレイアイコン表示",
        );
        ui.checkbox(
            &mut self.settings_draft.general.ime_off_on_show,
            "入力ウィンドウ表示時にIMEをオフ",
        );
        ui.checkbox(
            &mut self.settings_draft.general.show_title_bar,
            "タイトルバー表示",
        );
    }

    fn draw_settings_search(&mut self, ui: &mut egui::Ui) {
        ComboBox::from_label("通常時検索方式")
            .selected_text(search_mode_label(self.settings_draft.search.normal_mode))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.settings_draft.search.normal_mode,
                    SearchModeConfig::Prefix,
                    "prefix",
                );
                ui.selectable_value(
                    &mut self.settings_draft.search.normal_mode,
                    SearchModeConfig::Substring,
                    "substring",
                );
                ui.selectable_value(
                    &mut self.settings_draft.search.normal_mode,
                    SearchModeConfig::Fuzzy,
                    "fuzzy",
                );
            });

        ComboBox::from_label("フォルダ展開時検索方式")
            .selected_text(search_mode_label(self.settings_draft.search.folder_mode))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.settings_draft.search.folder_mode,
                    SearchModeConfig::Prefix,
                    "prefix",
                );
                ui.selectable_value(
                    &mut self.settings_draft.search.folder_mode,
                    SearchModeConfig::Substring,
                    "substring",
                );
                ui.selectable_value(
                    &mut self.settings_draft.search.folder_mode,
                    SearchModeConfig::Fuzzy,
                    "fuzzy",
                );
            });

        ui.horizontal(|ui| {
            ui.label("最大表示件数");
            ui.add(
                egui::DragValue::new(&mut self.settings_draft.appearance.max_results)
                    .clamp_range(1..=50),
            );
        });

        ui.horizontal(|ui| {
            ui.label("履歴表示最大件数");
            ui.add(
                egui::DragValue::new(&mut self.settings_draft.appearance.max_history_display)
                    .clamp_range(1..=50),
            );
        });

        ui.checkbox(
            &mut self.settings_draft.search.show_hidden_system,
            "隠し/システム項目を表示",
        );
    }

    fn draw_settings_index(&mut self, ui: &mut egui::Ui) {
        ui.label("スキャン条件一覧");
        ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
            for (idx, sp) in self.settings_draft.paths.scan.iter().enumerate() {
                let line = format!(
                    "{} | {} | folder={}",
                    sp.path,
                    sp.extensions.join(","),
                    if sp.include_folders { 1 } else { 0 }
                );
                let selected = self.selected_scan_index == Some(idx);
                if ui.selectable_label(selected, line).clicked() {
                    self.selected_scan_index = Some(idx);
                    self.settings_scan_path = sp.path.clone();
                    self.settings_scan_ext = sp.extensions.join(",");
                    self.settings_scan_include_folders = sp.include_folders;
                }
            }
        });

        ui.separator();
        ui.label("パス");
        ui.text_edit_singleline(&mut self.settings_scan_path);
        ui.label("拡張子 (,区切り)");
        ui.text_edit_singleline(&mut self.settings_scan_ext);
        ui.checkbox(&mut self.settings_scan_include_folders, "フォルダ含む");

        ui.horizontal(|ui| {
            if ui.button("追加").clicked() {
                let path = self.settings_scan_path.trim();
                let exts = parse_extensions(&self.settings_scan_ext);
                if path.is_empty() {
                    self.settings_status = "パスを入力してください".to_string();
                } else if exts.is_empty() {
                    self.settings_status = "拡張子を1つ以上入力してください".to_string();
                } else {
                    self.settings_draft.paths.scan.push(ScanPath {
                        path: path.to_string(),
                        extensions: exts,
                        include_folders: self.settings_scan_include_folders,
                    });
                    self.settings_status = "スキャン条件を追加しました".to_string();
                }
            }

            if ui.button("更新").clicked() {
                if let Some(idx) = self.selected_scan_index {
                    if idx < self.settings_draft.paths.scan.len() {
                        let path = self.settings_scan_path.trim();
                        let exts = parse_extensions(&self.settings_scan_ext);
                        if path.is_empty() || exts.is_empty() {
                            self.settings_status =
                                "パスと拡張子を入力してください".to_string();
                        } else {
                            self.settings_draft.paths.scan[idx] = ScanPath {
                                path: path.to_string(),
                                extensions: exts,
                                include_folders: self.settings_scan_include_folders,
                            };
                            self.settings_status = "スキャン条件を更新しました".to_string();
                        }
                    }
                } else {
                    self.settings_status = "更新対象を選択してください".to_string();
                }
            }

            if ui.button("削除").clicked() {
                if let Some(idx) = self.selected_scan_index {
                    if idx < self.settings_draft.paths.scan.len() {
                        self.settings_draft.paths.scan.remove(idx);
                        self.selected_scan_index = None;
                        self.settings_status = "スキャン条件を削除しました".to_string();
                    }
                } else {
                    self.settings_status = "削除対象を選択してください".to_string();
                }
            }
        });

        ui.separator();
        ui.horizontal(|ui| {
            ui.label("履歴保存上位 N");
            ui.add(
                egui::DragValue::new(&mut self.settings_draft.appearance.top_n_history)
                    .clamp_range(10..=5000),
            );
        });
        ui.checkbox(&mut self.settings_draft.appearance.show_icons, "アイコン表示");
    }

    fn draw_settings_visual(&mut self, ui: &mut egui::Ui) {
        let mut preset = self.settings_draft.visual.preset;
        ComboBox::from_label("プリセット")
            .selected_text(theme_preset_label(preset))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut preset, ThemePreset::Obsidian, "obsidian");
                ui.selectable_value(&mut preset, ThemePreset::Paper, "paper");
                ui.selectable_value(&mut preset, ThemePreset::Solarized, "solarized");
            });

        if preset != self.settings_draft.visual.preset {
            self.settings_draft.visual.preset = preset;
            apply_visual_preset(&mut self.settings_draft.visual, preset);
        }

        ui.label("背景色 (#RRGGBB)");
        ui.text_edit_singleline(&mut self.settings_draft.visual.background_color);
        ui.label("入力背景色 (#RRGGBB)");
        ui.text_edit_singleline(&mut self.settings_draft.visual.input_background_color);
        ui.label("文字色 (#RRGGBB)");
        ui.text_edit_singleline(&mut self.settings_draft.visual.text_color);
        ui.label("選択行色 (#RRGGBB)");
        ui.text_edit_singleline(&mut self.settings_draft.visual.selected_row_color);
        ui.label("ヒント文字色 (#RRGGBB)");
        ui.text_edit_singleline(&mut self.settings_draft.visual.hint_text_color);

        ui.label("フォントファミリー");
        let mut family = normalize_visual_font_family(&self.settings_draft.visual.font_family);
        ComboBox::from_id_source("visual_font_family")
            .selected_text(visual_font_family_label(&family))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut family,
                    egui::FontFamily::Proportional,
                    visual_font_family_label(&egui::FontFamily::Proportional),
                );
                ui.selectable_value(
                    &mut family,
                    egui::FontFamily::Monospace,
                    visual_font_family_label(&egui::FontFamily::Monospace),
                );
            });
        self.settings_draft.visual.font_family = visual_font_family_value(&family).to_string();
        ui.horizontal(|ui| {
            ui.label("フォントサイズ");
            ui.add(
                egui::DragValue::new(&mut self.settings_draft.visual.font_size).clamp_range(8..=48),
            );
        });
    }

    fn save_settings(&mut self, ctx: &egui::Context) {
        let old = self.config.clone();
        let mut next = self.settings_draft.clone();

        next.appearance.max_history_display = next
            .appearance
            .max_history_display
            .min(next.appearance.max_results)
            .clamp(1, 50);
        next.appearance.max_results = next.appearance.max_results.clamp(1, 50);
        next.appearance.top_n_history = next.appearance.top_n_history.clamp(10, 5000);
        next.visual.background_color =
            normalize_hex_color(&next.visual.background_color, &old.visual.background_color);
        next.visual.input_background_color = normalize_hex_color(
            &next.visual.input_background_color,
            &old.visual.input_background_color,
        );
        next.visual.text_color = normalize_hex_color(&next.visual.text_color, &old.visual.text_color);
        next.visual.selected_row_color = normalize_hex_color(
            &next.visual.selected_row_color,
            &old.visual.selected_row_color,
        );
        next.visual.hint_text_color =
            normalize_hex_color(&next.visual.hint_text_color, &old.visual.hint_text_color);
        next.visual.font_family = normalize_stored_font_family(&next.visual.font_family);
        next.visual.font_size = next.visual.font_size.clamp(8, 48);

        let mut hotkey_ok = true;
        if old.hotkey != next.hotkey {
            let (reply_tx, reply_rx) = mpsc::channel();
            self.platform.send_command(PlatformCommand::SetHotkey {
                config: next.hotkey.clone(),
                reply: reply_tx,
            });
            match reply_rx.recv_timeout(Duration::from_secs(2)) {
                Ok(true) => {}
                _ => {
                    hotkey_ok = false;
                    next.hotkey = old.hotkey.clone();
                }
            }
        }

        next.save();
        self.apply_config(ctx, &old, &next);
        self.settings_draft = next.clone();

        let rebuild_needed = needs_rebuild(&old, &next);
        if rebuild_needed {
            self.pending_rebuild_config = Some(next);
            self.show_rebuild_confirm = true;
        }

        if hotkey_ok {
            self.settings_status = "保存しました".to_string();
        } else {
            self.settings_status =
                "保存しました（ホットキー再登録に失敗したため旧設定を維持）".to_string();
        }
    }

    fn apply_config(&mut self, ctx: &egui::Context, old: &Config, next: &Config) {
        self.config = next.clone();
        self.runtime = runtime_from_config(next);
        self.history = HistoryStore::load(
            next.appearance.top_n_history,
            next.appearance.max_history_display,
        );

        if old.general.show_tray_icon != next.general.show_tray_icon {
            self.platform
                .send_command(PlatformCommand::SetTrayVisible(next.general.show_tray_icon));
        }

        if old.general.show_title_bar != next.general.show_title_bar {
            ctx.send_viewport_cmd(ViewportCommand::Decorations(next.general.show_title_bar));
        }

        if old.appearance.max_results != next.appearance.max_results
            || old.appearance.window_width != next.appearance.window_width
        {
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(
                next.appearance.window_width as f32,
                search_window_height(next.appearance.max_results),
            )));
        }

        if next.appearance.show_icons {
            let cache = icon::IconCache::load().unwrap_or_else(|| {
                let cache = icon::IconCache::build(self.engine.entries());
                cache.save();
                cache
            });
            self.icon_cache = Some(Rc::new(cache));
        } else {
            self.icon_cache = None;
        }
        self.icon_textures.clear();

        self.apply_visual_style(ctx);
        self.refresh_results();
    }

    fn start_rebuild(&mut self, cfg: Config) {
        if self.rebuild_in_progress {
            return;
        }

        self.rebuild_in_progress = true;
        self.spinner_index = 0;
        self.last_spinner_tick = Instant::now();
        self.settings_status = "インデックス再構築中... |".to_string();

        let tx = self.internal_tx.clone();
        let additional = cfg.paths.additional.clone();
        let scan = cfg.paths.scan.clone();
        let show_hidden = cfg.search.show_hidden_system;
        let reload_icons = cfg.appearance.show_icons;

        let spawn = std::thread::Builder::new()
            .name("snotra-manual-rebuild".to_string())
            .spawn(move || {
                let entries = indexer::rebuild_and_save(&additional, &scan, show_hidden);
                if reload_icons {
                    icon::IconCache::rebuild_cache(&entries);
                }
                let _ = tx.send(InternalEvent::RebuildDone {
                    entries,
                    reload_icons,
                });
            });

        if spawn.is_err() {
            let _ = self.internal_tx.send(InternalEvent::RebuildFailed);
        }
    }

    fn persist_search_placement(&self) {
        if let Some(pos) = self.search_window_pos {
            window_data::save_search_placement(window_data::WindowPlacement {
                x: pos.x.round() as i32,
                y: pos.y.round() as i32,
            });
        }
    }

    fn persist_settings_placement(&self) {
        if let Some(pos) = self.settings_window_pos {
            window_data::save_settings_placement(window_data::WindowPlacement {
                x: pos.x.round() as i32,
                y: pos.y.round() as i32,
            });
        }
    }

    fn tick_spinner(&mut self) {
        if !self.rebuild_in_progress {
            return;
        }
        if self.last_spinner_tick.elapsed() >= Duration::from_millis(120) {
            self.spinner_index = (self.spinner_index + 1) % SPINNER_FRAMES.len();
            self.settings_status = format!(
                "インデックス再構築中... {}",
                SPINNER_FRAMES[self.spinner_index]
            );
            self.last_spinner_tick = Instant::now();
        }
    }
}

impl eframe::App for SnotraApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.initial_window_applied {
            ctx.send_viewport_cmd(ViewportCommand::Decorations(
                self.config.general.show_title_bar,
            ));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(
                self.config.appearance.window_width as f32,
                search_window_height(self.config.appearance.max_results),
            )));
            if !self.show_search_window {
                ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
            }
            self.initial_window_applied = true;
        }

        self.apply_visual_style(ctx);
        self.sync_search_viewport_pos(ctx);
        self.handle_platform_events(ctx);
        self.handle_internal_events();
        self.tick_spinner();
        self.handle_auto_hide_on_focus_lost(ctx);

        if self.should_exit {
            if !self.exit_sent {
                self.platform.send_command(PlatformCommand::Exit);
                self.exit_sent = true;
            }
            self.persist_search_placement();
            self.persist_settings_placement();
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }

        if self.show_search_window {
            self.handle_search_keyboard(ctx);
            self.draw_search_ui(ctx);
        } else {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(24.0);
                    ui.label(RichText::new("Snotra はトレイで動作中です"));
                });
            });
        }

        self.draw_settings_window(ctx);

        ctx.request_repaint_after(Duration::from_millis(80));
    }
}

impl Drop for SnotraApp {
    fn drop(&mut self) {
        self.persist_search_placement();
        self.persist_settings_placement();
        if !self.exit_sent {
            self.platform.send_command(PlatformCommand::Exit);
            self.exit_sent = true;
        }
    }
}

pub fn search_window_height(max_results: usize) -> f32 {
    INPUT_HEIGHT + (ITEM_HEIGHT * max_results as f32) + WINDOW_PADDING * 2.0
}

fn runtime_from_config(config: &Config) -> RuntimeSettings {
    RuntimeSettings {
        max_results: config.appearance.max_results,
        max_history_display: config.appearance.max_history_display,
        normal_mode: to_search_mode(config.search.normal_mode),
        folder_mode: to_search_mode(config.search.folder_mode),
        show_hidden_system: config.search.show_hidden_system,
        hotkey_toggle: config.general.hotkey_toggle,
        auto_hide_on_focus_lost: config.general.auto_hide_on_focus_lost,
    }
}

fn to_search_mode(mode: SearchModeConfig) -> SearchMode {
    match mode {
        SearchModeConfig::Prefix => SearchMode::Prefix,
        SearchModeConfig::Substring => SearchMode::Substring,
        SearchModeConfig::Fuzzy => SearchMode::Fuzzy,
    }
}

fn parse_hex_color(input: &str, fallback: Color32) -> Color32 {
    let s = input.trim();
    let hex = s.strip_prefix('#').unwrap_or(s);
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return fallback;
    }

    let Ok(v) = u32::from_str_radix(hex, 16) else {
        return fallback;
    };

    let r = ((v >> 16) & 0xFF) as u8;
    let g = ((v >> 8) & 0xFF) as u8;
    let b = (v & 0xFF) as u8;
    Color32::from_rgb(r, g, b)
}

fn normalize_hex_color(input: &str, fallback: &str) -> String {
    let trimmed = input.trim();
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return fallback.to_string();
    }
    format!("#{}", hex.to_uppercase())
}

fn normalize_stored_font_family(input: &str) -> String {
    let family = normalize_visual_font_family(input);
    visual_font_family_value(&family).to_string()
}

fn normalize_visual_font_family(input: &str) -> egui::FontFamily {
    let normalized = input.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "monospace" | "consolas" => egui::FontFamily::Monospace,
        _ => egui::FontFamily::Proportional,
    }
}

fn visual_font_family_value(family: &egui::FontFamily) -> &'static str {
    match family {
        egui::FontFamily::Proportional => "proportional",
        egui::FontFamily::Monospace => "monospace",
        egui::FontFamily::Name(_) => "proportional",
    }
}

fn visual_font_family_label(family: &egui::FontFamily) -> &'static str {
    match family {
        egui::FontFamily::Proportional => "proportional",
        egui::FontFamily::Monospace => "monospace",
        egui::FontFamily::Name(_) => "proportional",
    }
}

fn needs_rebuild(old: &Config, new: &Config) -> bool {
    old.paths.scan != new.paths.scan
        || old.search.show_hidden_system != new.search.show_hidden_system
        || old.appearance.show_icons != new.appearance.show_icons
}

fn parse_extensions(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| {
            if s.starts_with('.') {
                s.to_lowercase()
            } else {
                format!(".{}", s.to_lowercase())
            }
        })
        .collect()
}

fn search_mode_label(mode: SearchModeConfig) -> &'static str {
    match mode {
        SearchModeConfig::Prefix => "prefix",
        SearchModeConfig::Substring => "substring",
        SearchModeConfig::Fuzzy => "fuzzy",
    }
}

fn theme_preset_label(preset: ThemePreset) -> &'static str {
    match preset {
        ThemePreset::Obsidian => "obsidian",
        ThemePreset::Paper => "paper",
        ThemePreset::Solarized => "solarized",
    }
}

fn apply_visual_preset(visual: &mut VisualConfig, preset: ThemePreset) {
    let (bg, input_bg, text, selected, hint, family, size) = match preset {
        ThemePreset::Obsidian => (
            "#282828",
            "#383838",
            "#E0E0E0",
            "#505050",
            "#808080",
            "proportional",
            15,
        ),
        ThemePreset::Paper => (
            "#FFFFFF",
            "#F2F2F2",
            "#141414",
            "#DADADA",
            "#707070",
            "proportional",
            15,
        ),
        ThemePreset::Solarized => (
            "#002B36",
            "#073642",
            "#839496",
            "#586E75",
            "#93A1A1",
            "monospace",
            15,
        ),
    };

    visual.background_color = bg.to_string();
    visual.input_background_color = input_bg.to_string();
    visual.text_color = text.to_string();
    visual.selected_row_color = selected.to_string();
    visual.hint_text_color = hint.to_string();
    visual.font_family = family.to_string();
    visual.font_size = size;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_stored_font_family_maps_legacy_names() {
        assert_eq!(normalize_stored_font_family("Consolas"), "monospace");
        assert_eq!(normalize_stored_font_family("Yu Gothic UI"), "proportional");
        assert_eq!(normalize_stored_font_family("monospace"), "monospace");
    }

    #[test]
    fn runtime_from_config_reflects_auto_hide_on_focus_lost() {
        let mut cfg = Config::default();
        cfg.general.auto_hide_on_focus_lost = false;
        let runtime = runtime_from_config(&cfg);
        assert!(!runtime.auto_hide_on_focus_lost);
    }
}
