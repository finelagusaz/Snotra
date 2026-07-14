#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicI32, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant},
};

use eframe::egui;
use snotra_core::{
    config::Config, engine::Engine, history::HistoryStore, indexer::AppEntry,
    ui_types::SearchResult,
};

const ENTRY_COUNT: usize = 10_000;
const WINDOW_TITLE: &str = "Snotra eframe/glow MVP";
const FRAME_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HostMode {
    Tauri,
    Standalone,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HardwareMode {
    Preferred,
    Off,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FontChoice {
    None,
    YuGothic,
    MsGothic,
    Meiryo,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FontStorage {
    Owned,
    Static,
}

impl FontChoice {
    fn path(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::YuGothic => Some("C:/Windows/Fonts/YuGothM.ttc"),
            Self::MsGothic => Some("C:/Windows/Fonts/msgothic.ttc"),
            Self::Meiryo => Some("C:/Windows/Fonts/meiryo.ttc"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Scenario {
    KeepWindow,
    DropFontOnHide,
    RecreateWindow,
    DeferredViewport,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProbeConfig {
    host: HostMode,
    hardware: HardwareMode,
    font: FontChoice,
    font_storage: FontStorage,
    scenario: Scenario,
    visible_wait_ms: u64,
    hidden_wait_ms: u64,
}

impl ProbeConfig {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut config = Self {
            host: HostMode::Tauri,
            hardware: HardwareMode::Preferred,
            font: FontChoice::YuGothic,
            font_storage: FontStorage::Owned,
            scenario: Scenario::KeepWindow,
            visible_wait_ms: 5_000,
            hidden_wait_ms: 3_000,
        };
        let mut args = args.into_iter();
        while let Some(option) = args.next() {
            let mut value = || {
                args.next()
                    .ok_or_else(|| format!("{option} requires a value"))
            };
            match option.as_str() {
                "--host" => {
                    config.host = match value()?.as_str() {
                        "tauri" => HostMode::Tauri,
                        "standalone" => HostMode::Standalone,
                        unknown => return Err(format!("unknown host: {unknown}")),
                    };
                }
                "--hardware" => {
                    config.hardware = match value()?.as_str() {
                        "preferred" => HardwareMode::Preferred,
                        "off" => HardwareMode::Off,
                        unknown => return Err(format!("unknown hardware mode: {unknown}")),
                    };
                }
                "--font" => {
                    config.font = match value()?.as_str() {
                        "none" => FontChoice::None,
                        "yugothic" => FontChoice::YuGothic,
                        "msgothic" => FontChoice::MsGothic,
                        "meiryo" => FontChoice::Meiryo,
                        unknown => return Err(format!("unknown font: {unknown}")),
                    };
                }
                "--font-storage" => {
                    config.font_storage = match value()?.as_str() {
                        "owned" => FontStorage::Owned,
                        "static" => FontStorage::Static,
                        unknown => return Err(format!("unknown font storage: {unknown}")),
                    };
                }
                "--scenario" => {
                    config.scenario = match value()?.as_str() {
                        "keep-window" => Scenario::KeepWindow,
                        "drop-font" => Scenario::DropFontOnHide,
                        "recreate" => Scenario::RecreateWindow,
                        "deferred-viewport" => Scenario::DeferredViewport,
                        unknown => return Err(format!("unknown scenario: {unknown}")),
                    };
                }
                "--visible-wait-ms" => {
                    config.visible_wait_ms = parse_millis(&option, &value()?)?;
                }
                "--hidden-wait-ms" => {
                    config.hidden_wait_ms = parse_millis(&option, &value()?)?;
                }
                unknown => return Err(format!("unknown option: {unknown}")),
            }
        }
        Ok(config)
    }
}

fn parse_millis(option: &str, value: &str) -> Result<u64, String> {
    value
        .parse()
        .map_err(|_| format!("{option} requires an integer: {value}"))
}

struct ProbeApp {
    engine: Arc<Mutex<Engine>>,
    query: String,
    results: Vec<SearchResult>,
    first_frame: bool,
    frame_tx: Sender<Instant>,
    config: ProbeConfig,
}

impl ProbeApp {
    fn new(engine: Arc<Mutex<Engine>>, frame_tx: Sender<Instant>, config: ProbeConfig) -> Self {
        let results = engine
            .lock()
            .map(|engine| engine.recent_history())
            .unwrap_or_default();
        Self {
            engine,
            query: String::new(),
            results,
            first_frame: true,
            frame_tx,
            config,
        }
    }

    fn refresh_results(&mut self) {
        if let Ok(mut engine) = self.engine.lock() {
            self.results = if self.query.is_empty() {
                engine.recent_history()
            } else {
                engine.search(&self.query)
            };
        }
    }

    fn render(&mut self, ui: &mut egui::Ui) {
        let now = Instant::now();
        let _ = self.frame_tx.send(now);
        if self.first_frame {
            self.first_frame = false;
            log_metric("first_frame", self.config);
        }

        ui.heading("Snotra eframe/glow 検証");
        ui.label(format!(
            "font={:?} storage={:?} scenario={:?}",
            self.config.font, self.config.font_storage, self.config.scenario
        ));
        let response = ui.add(
            egui::TextEdit::singleline(&mut self.query)
                .hint_text("アプリ、フォルダー、コマンドを検索"),
        );
        if response.changed() {
            self.refresh_results();
        }
        ui.separator();
        for result in self.results.iter().take(8) {
            ui.label(&result.name);
        }
        ui.small(format!(
            "{} results / Engine {ENTRY_COUNT} entries",
            self.results.len()
        ));
    }
}

impl eframe::App for ProbeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.render(ui);
    }
}

struct DeferredViewportApp {
    child_visible: Arc<AtomicBool>,
    child_app: Arc<Mutex<ProbeApp>>,
    root_state_tx: Sender<bool>,
}

impl eframe::App for DeferredViewportApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let child_visible_now = self.child_visible.load(Ordering::Acquire);
        let _ = self.root_state_tx.send(child_visible_now);
        if !child_visible_now {
            return;
        }

        let child_visible = Arc::clone(&self.child_visible);
        let child_app = Arc::clone(&self.child_app);
        ui.ctx().show_viewport_deferred(
            child_viewport_id(),
            search_viewport_builder(),
            move |ui, _class| {
                if ui.ctx().input(|input| input.viewport().close_requested()) {
                    child_visible.store(false, Ordering::Release);
                    ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
                    return;
                }
                if let Ok(mut app) = child_app.lock() {
                    app.render(ui);
                }
            },
        );
    }
}

fn child_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("snotra-egui-glow-search")
}

fn search_viewport_builder() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title(WINDOW_TITLE)
        .with_inner_size([640.0, 360.0])
        .with_visible(true)
}

fn build_verification_engine(entry_count: usize) -> Engine {
    let mut entries = vec![
        AppEntry {
            name: "Visual Studio Code".to_owned(),
            target_path: "C:/Program Files/Microsoft VS Code/Code.exe".to_owned(),
            is_folder: false,
        },
        AppEntry {
            name: "Windows Terminal".to_owned(),
            target_path: "C:/Program Files/WindowsApps/wt.exe".to_owned(),
            is_folder: false,
        },
        AppEntry {
            name: "ドキュメント".to_owned(),
            target_path: "C:/Users/User/Documents".to_owned(),
            is_folder: true,
        },
        AppEntry {
            name: "ダウンロード".to_owned(),
            target_path: "C:/Users/User/Downloads".to_owned(),
            is_folder: true,
        },
        AppEntry {
            name: "日本語入力テスト".to_owned(),
            target_path: "C:/Snotra/Verification/JapaneseInput.exe".to_owned(),
            is_folder: false,
        },
    ];
    for index in entries.len()..entry_count {
        entries.push(AppEntry {
            name: format!("Benchmark App {index:05}"),
            target_path: format!("C:/Snotra/Verification/App{index:05}.exe"),
            is_folder: false,
        });
    }

    let history_path = std::env::temp_dir().join(format!(
        "snotra-egui-glow-mvp-history-{}-unused",
        std::process::id()
    ));
    let history = HistoryStore::load_in(&history_path);
    let mut engine = Engine::new(entries, history, Config::normalized_default());
    for path in [
        "C:/Program Files/Microsoft VS Code/Code.exe",
        "C:/Program Files/WindowsApps/wt.exe",
        "C:/Users/User/Documents",
        "C:/Users/User/Downloads",
        "C:/Snotra/Verification/JapaneseInput.exe",
    ] {
        engine.record_launch(path, "");
    }
    engine
}

struct StaticFontBytes {
    path: &'static str,
    bytes: Box<[u8]>,
}

static STATIC_PROBE_FONT: OnceLock<StaticFontBytes> = OnceLock::new();

fn configure_font(
    context: &egui::Context,
    choice: FontChoice,
    storage: FontStorage,
) -> Result<usize, String> {
    let mut fonts = egui::FontDefinitions::default();
    let Some(path) = choice.path() else {
        context.set_fonts(fonts);
        return Ok(0);
    };
    let (mut font, byte_len) = match storage {
        FontStorage::Owned => {
            let bytes = read_font(path)?;
            let byte_len = bytes.len();
            (egui::FontData::from_owned(bytes), byte_len)
        }
        FontStorage::Static => {
            if STATIC_PROBE_FONT.get().is_none() {
                let loaded = StaticFontBytes {
                    path,
                    bytes: read_font(path)?.into_boxed_slice(),
                };
                let _ = STATIC_PROBE_FONT.set(loaded);
            }
            let cached = STATIC_PROBE_FONT
                .get()
                .ok_or_else(|| "static font initialization failed".to_owned())?;
            if cached.path != path {
                return Err(format!(
                    "static font cache already contains {} instead of {path}",
                    cached.path
                ));
            }
            let bytes: &'static [u8] = cached.bytes.as_ref();
            (egui::FontData::from_static(bytes), bytes.len())
        }
    };
    font.tweak = egui::FontTweak {
        scale: 1.0,
        y_offset_factor: 0.3,
        y_offset: 0.0,
        ..Default::default()
    };
    fonts.font_data.insert("jp_font".to_owned(), font.into());
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .push("jp_font".to_owned());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .push("jp_font".to_owned());
    context.set_fonts(fonts);
    Ok(byte_len)
}

fn read_font(path: &str) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|error| format!("failed to read {path}: {error}"))
}

fn native_options(deferred_root: bool, hardware: HardwareMode) -> eframe::NativeOptions {
    let viewport = if deferred_root {
        // This root owns the GL context for the process lifetime. Keeping a 1x1
        // off-screen window lets child viewports be destroyed independently.
        egui::ViewportBuilder::default()
            .with_title("Snotra eframe/glow root")
            .with_inner_size([1.0, 1.0])
            .with_position([-32_000.0, -32_000.0])
            .with_decorations(false)
            .with_resizable(false)
            .with_taskbar(false)
    } else {
        search_viewport_builder()
    };
    eframe::NativeOptions {
        viewport,
        glow_options: eframe::egui_glow::GlowConfiguration {
            hardware_acceleration: match hardware {
                HardwareMode::Preferred => eframe::egui_glow::HardwareAcceleration::Preferred,
                HardwareMode::Off => eframe::egui_glow::HardwareAcceleration::Off,
            },
            ..Default::default()
        },
        event_loop_builder: Some(Box::new(|builder| {
            use winit::platform::windows::EventLoopBuilderExtWindows as _;
            // Tauri must own the main thread. Windows supports a winit message loop on
            // this dedicated UI thread, and every window is destroyed before it exits.
            builder.with_any_thread(true);
        })),
        ..Default::default()
    }
}

fn run_probe(config: ProbeConfig) -> Result<(), String> {
    eprintln!("SNOTRA_EFRAME_PROBE_CONFIG={config:?}");
    log_metric("host_ready", config);

    let engine_started = Instant::now();
    let engine = Arc::new(Mutex::new(build_verification_engine(ENTRY_COUNT)));
    eprintln!(
        "SNOTRA_EFRAME_PROBE_PHASE=engine_ready elapsed_ms={:.3}",
        engine_started.elapsed().as_secs_f64() * 1000.0
    );
    log_metric("engine_ready", config);

    let context = egui::Context::default();
    let font_started = Instant::now();
    let font_bytes = configure_font(&context, config.font, config.font_storage)?;
    eprintln!(
        "SNOTRA_EFRAME_PROBE_PHASE=font_configured font={:?} storage={:?} bytes={} elapsed_ms={:.3}",
        config.font,
        config.font_storage,
        font_bytes,
        font_started.elapsed().as_secs_f64() * 1000.0
    );
    log_metric("font_configured", config);

    match config.scenario {
        Scenario::KeepWindow | Scenario::DropFontOnHide => {
            run_persistent_window(config, engine, context)
        }
        Scenario::RecreateWindow => run_recreate_window(config, engine, context),
        Scenario::DeferredViewport => run_deferred_viewport(config, engine, context),
    }
}

fn run_persistent_window(
    config: ProbeConfig,
    engine: Arc<Mutex<Engine>>,
    context: egui::Context,
) -> Result<(), String> {
    let (frame_tx, frame_rx) = mpsc::channel();
    let controller_context = context.clone();
    let controller = thread::Builder::new()
        .name("eframe-glow-controller".to_owned())
        .spawn(move || persistent_window_controller(config, controller_context, frame_rx))
        .map_err(|error| format!("failed to spawn probe controller: {error}"))?;

    let opened_at = Instant::now();
    let app_engine = Arc::clone(&engine);
    let result = eframe::run_native_ext(
        "snotra-egui-glow-mvp",
        native_options(false, config.hardware),
        Some(context),
        Box::new(move |_| {
            eprintln!(
                "SNOTRA_EFRAME_PROBE_PHASE=app_created elapsed_ms={:.3}",
                opened_at.elapsed().as_secs_f64() * 1000.0
            );
            Ok(Box::new(ProbeApp::new(app_engine, frame_tx, config)))
        }),
    );
    let controller_result = controller
        .join()
        .map_err(|_| "probe controller panicked".to_owned())?;
    result.map_err(|error| format!("eframe failed: {error}"))?;
    controller_result
}

fn persistent_window_controller(
    config: ProbeConfig,
    context: egui::Context,
    frame_rx: Receiver<Instant>,
) -> Result<(), String> {
    let result = (|| {
        let first_frame = frame_rx
            .recv_timeout(FRAME_TIMEOUT)
            .map_err(|error| format!("first frame was not rendered: {error}"))?;
        eprintln!("SNOTRA_EFRAME_PROBE_PHASE=first_frame_seen at={first_frame:?}");
        thread::sleep(Duration::from_millis(config.visible_wait_ms));
        log_metric("visible", config);

        context.send_viewport_cmd_to(
            egui::ViewportId::ROOT,
            egui::ViewportCommand::Visible(false),
        );
        context.request_repaint_of(egui::ViewportId::ROOT);
        if config.scenario == Scenario::DropFontOnHide {
            let started = Instant::now();
            context.set_fonts(egui::FontDefinitions::default());
            context.request_repaint();
            eprintln!(
                "SNOTRA_EFRAME_PROBE_PHASE=font_dropped elapsed_ms={:.3}",
                started.elapsed().as_secs_f64() * 1000.0
            );
        }
        thread::sleep(Duration::from_millis(250));
        let trimmed = trim_working_set();
        eprintln!("SNOTRA_EFRAME_PROBE_PHASE=working_set_trimmed success={trimmed}");
        thread::sleep(Duration::from_millis(config.hidden_wait_ms));
        log_metric("hidden", config);

        let show_started = Instant::now();
        if config.scenario == Scenario::DropFontOnHide {
            let font_started = Instant::now();
            let font_bytes = configure_font(&context, config.font, config.font_storage)?;
            eprintln!(
                "SNOTRA_EFRAME_PROBE_PHASE=font_restored bytes={} elapsed_ms={:.3}",
                font_bytes,
                font_started.elapsed().as_secs_f64() * 1000.0
            );
        }
        context.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Visible(true));
        context.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus);
        context.request_repaint_of(egui::ViewportId::ROOT);
        let shown_frame = wait_for_frame_after(&frame_rx, show_started)?;
        eprintln!(
            "SNOTRA_EFRAME_PROBE_PHASE=warm_shown elapsed_ms={:.3}",
            shown_frame.duration_since(show_started).as_secs_f64() * 1000.0
        );
        thread::sleep(Duration::from_millis(500));
        log_metric("reshown", config);
        Ok(())
    })();
    close_root_viewport(&context);
    result
}

fn run_deferred_viewport(
    config: ProbeConfig,
    engine: Arc<Mutex<Engine>>,
    context: egui::Context,
) -> Result<(), String> {
    let (frame_tx, frame_rx) = mpsc::channel();
    let (root_state_tx, root_state_rx) = mpsc::channel();
    let child_visible = Arc::new(AtomicBool::new(true));
    let controller_visible = Arc::clone(&child_visible);
    let controller_context = context.clone();
    let controller = thread::Builder::new()
        .name("eframe-glow-deferred-controller".to_owned())
        .spawn(move || {
            deferred_viewport_controller(
                config,
                controller_context,
                controller_visible,
                frame_rx,
                root_state_rx,
            )
        })
        .map_err(|error| format!("failed to spawn deferred viewport controller: {error}"))?;

    let opened_at = Instant::now();
    let child_app = Arc::new(Mutex::new(ProbeApp::new(engine, frame_tx, config)));
    let result = eframe::run_native_ext(
        "snotra-egui-glow-mvp",
        native_options(true, config.hardware),
        Some(context),
        Box::new(move |_| {
            eprintln!(
                "SNOTRA_EFRAME_PROBE_PHASE=deferred_root_created elapsed_ms={:.3}",
                opened_at.elapsed().as_secs_f64() * 1000.0
            );
            Ok(Box::new(DeferredViewportApp {
                child_visible,
                child_app,
                root_state_tx,
            }))
        }),
    );
    let controller_result = controller
        .join()
        .map_err(|_| "deferred viewport controller panicked".to_owned())?;
    result.map_err(|error| format!("eframe failed: {error}"))?;
    controller_result
}

fn deferred_viewport_controller(
    config: ProbeConfig,
    context: egui::Context,
    child_visible: Arc<AtomicBool>,
    frame_rx: Receiver<Instant>,
    root_state_rx: Receiver<bool>,
) -> Result<(), String> {
    let result = (|| {
        let first_frame = frame_rx
            .recv_timeout(FRAME_TIMEOUT)
            .map_err(|error| format!("deferred child frame was not rendered: {error}"))?;
        eprintln!("SNOTRA_EFRAME_PROBE_PHASE=deferred_first_frame_seen at={first_frame:?}");
        thread::sleep(Duration::from_millis(config.visible_wait_ms));
        log_metric("visible", config);

        while root_state_rx.try_recv().is_ok() {}
        // First hide the child through a parent output pass. Deferred children
        // sleep independently, so relying on a child repaint here can deadlock.
        context.send_viewport_cmd_to(child_viewport_id(), egui::ViewportCommand::Visible(false));
        context.request_repaint_of(egui::ViewportId::ROOT);
        wait_for_root_state(&root_state_rx, true)?;
        thread::sleep(Duration::from_millis(50));
        // A second root pass omits the child and lets eframe prune its window and
        // GL surface while retaining the root GL context.
        child_visible.store(false, Ordering::Release);
        context.request_repaint_of(egui::ViewportId::ROOT);
        wait_for_root_state(&root_state_rx, false)?;
        thread::sleep(Duration::from_millis(250));
        let trimmed = trim_working_set();
        eprintln!("SNOTRA_EFRAME_PROBE_PHASE=working_set_trimmed success={trimmed}");
        thread::sleep(Duration::from_millis(config.hidden_wait_ms));
        log_metric("child_destroyed", config);

        let show_started = Instant::now();
        while frame_rx.try_recv().is_ok() {}
        child_visible.store(true, Ordering::Release);
        context.request_repaint_of(egui::ViewportId::ROOT);
        wait_for_root_state(&root_state_rx, true)?;
        let shown_frame = wait_for_frame_after(&frame_rx, show_started)?;
        eprintln!(
            "SNOTRA_EFRAME_PROBE_PHASE=warm_shown elapsed_ms={:.3}",
            shown_frame.duration_since(show_started).as_secs_f64() * 1000.0
        );
        thread::sleep(Duration::from_millis(500));
        log_metric("reshown", config);
        Ok(())
    })();
    close_root_viewport(&context);
    result
}

fn wait_for_root_state(root_state_rx: &Receiver<bool>, expected: bool) -> Result<(), String> {
    let deadline = Instant::now() + FRAME_TIMEOUT;
    loop {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return Err(format!("root did not apply child_visible={expected}"));
        };
        let actual = root_state_rx
            .recv_timeout(remaining)
            .map_err(|error| format!("root state wait failed: {error}"))?;
        if actual == expected {
            eprintln!("SNOTRA_EFRAME_PROBE_PHASE=root_applied child_visible={expected}");
            return Ok(());
        }
    }
}

fn run_recreate_window(
    config: ProbeConfig,
    engine: Arc<Mutex<Engine>>,
    context: egui::Context,
) -> Result<(), String> {
    for iteration in 1..=2 {
        let (frame_tx, frame_rx) = mpsc::channel();
        let controller_context = context.clone();
        let opened_at = Instant::now();
        let controller = thread::Builder::new()
            .name(format!("eframe-glow-controller-{iteration}"))
            .spawn(move || {
                recreate_window_controller(
                    config,
                    iteration,
                    opened_at,
                    controller_context,
                    frame_rx,
                )
            })
            .map_err(|error| format!("failed to spawn recreate controller: {error}"))?;
        let app_engine = Arc::clone(&engine);
        let result = eframe::run_native_ext(
            "snotra-egui-glow-mvp",
            native_options(false, config.hardware),
            Some(context.clone()),
            Box::new(move |_| {
                eprintln!(
                    "SNOTRA_EFRAME_PROBE_PHASE=app_created iteration={} elapsed_ms={:.3}",
                    iteration,
                    opened_at.elapsed().as_secs_f64() * 1000.0
                );
                Ok(Box::new(ProbeApp::new(app_engine, frame_tx, config)))
            }),
        );
        let controller_result = controller
            .join()
            .map_err(|_| "recreate controller panicked".to_owned())?;
        result.map_err(|error| format!("eframe iteration {iteration} failed: {error}"))?;
        controller_result?;

        if iteration == 1 {
            thread::sleep(Duration::from_millis(250));
            let trimmed = trim_working_set();
            eprintln!("SNOTRA_EFRAME_PROBE_PHASE=working_set_trimmed success={trimmed}");
            thread::sleep(Duration::from_millis(config.hidden_wait_ms));
            log_metric("window_destroyed", config);
        }
    }
    Ok(())
}

fn recreate_window_controller(
    config: ProbeConfig,
    iteration: usize,
    opened_at: Instant,
    context: egui::Context,
    frame_rx: Receiver<Instant>,
) -> Result<(), String> {
    let result = (|| {
        let first_frame = frame_rx
            .recv_timeout(FRAME_TIMEOUT)
            .map_err(|error| format!("iteration {iteration} frame was not rendered: {error}"))?;
        eprintln!(
            "SNOTRA_EFRAME_PROBE_PHASE=window_first_frame iteration={} elapsed_ms={:.3}",
            iteration,
            first_frame.duration_since(opened_at).as_secs_f64() * 1000.0
        );
        let wait_ms = if iteration == 1 {
            config.visible_wait_ms
        } else {
            500
        };
        thread::sleep(Duration::from_millis(wait_ms));
        log_metric(
            if iteration == 1 {
                "visible"
            } else {
                "recreated_visible"
            },
            config,
        );
        Ok(())
    })();
    close_root_viewport(&context);
    result
}

fn close_root_viewport(context: &egui::Context) {
    // The controller runs outside winit's UI thread. Sending the command only
    // queues it, so explicitly wake the event loop to guarantee paired cleanup.
    context.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Close);
    context.request_repaint_of(egui::ViewportId::ROOT);
}

fn wait_for_frame_after(
    frame_rx: &Receiver<Instant>,
    threshold: Instant,
) -> Result<Instant, String> {
    let deadline = Instant::now() + FRAME_TIMEOUT;
    loop {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return Err("frame wait timed out".to_owned());
        };
        let frame = frame_rx
            .recv_timeout(remaining)
            .map_err(|error| format!("frame wait failed: {error}"))?;
        if frame >= threshold {
            return Ok(frame);
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ProcessSnapshot {
    handles: u32,
    gdi_objects: u32,
    user_objects: u32,
    windows: u32,
    visible_windows: u32,
    working_set: usize,
    private_bytes: usize,
}

fn log_metric(checkpoint: &str, config: ProbeConfig) {
    let snapshot = process_snapshot();
    eprintln!(
        "SNOTRA_EFRAME_PROBE_METRIC checkpoint={} host={:?} hardware={:?} font={:?} font_storage={:?} scenario={:?} working_set_bytes={} private_bytes={} handles={} gdi={} user={} windows={} visible_windows={}",
        checkpoint,
        config.host,
        config.hardware,
        config.font,
        config.font_storage,
        config.scenario,
        snapshot.working_set,
        snapshot.private_bytes,
        snapshot.handles,
        snapshot.gdi_objects,
        snapshot.user_objects,
        snapshot.windows,
        snapshot.visible_windows
    );
}

#[cfg(windows)]
fn process_snapshot() -> ProcessSnapshot {
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::System::{
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::{
            GR_GDIOBJECTS, GR_USEROBJECTS, GetCurrentProcess, GetCurrentProcessId, GetGuiResources,
            GetProcessHandleCount,
        },
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsWindowVisible,
    };
    use windows::core::BOOL;

    struct WindowCounts {
        process_id: u32,
        total: u32,
        visible: u32,
    }

    unsafe extern "system" fn count_process_windows(hwnd: HWND, state: LPARAM) -> BOOL {
        let counts = unsafe { &mut *(state.0 as *mut WindowCounts) };
        let mut window_process_id = 0;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut window_process_id)) };
        if window_process_id == counts.process_id {
            counts.total += 1;
            if unsafe { IsWindowVisible(hwnd).as_bool() } {
                counts.visible += 1;
            }
        }
        BOOL(1)
    }

    let process = unsafe { GetCurrentProcess() };
    let mut handles = 0;
    let mut memory = PROCESS_MEMORY_COUNTERS {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    let mut windows = WindowCounts {
        process_id: unsafe { GetCurrentProcessId() },
        total: 0,
        visible: 0,
    };
    unsafe {
        let _ = GetProcessHandleCount(process, &mut handles);
        let _ = GetProcessMemoryInfo(process, &mut memory, memory.cb);
        let _ = EnumWindows(
            Some(count_process_windows),
            LPARAM((&mut windows as *mut WindowCounts) as isize),
        );
    }
    ProcessSnapshot {
        handles,
        gdi_objects: unsafe { GetGuiResources(process, GR_GDIOBJECTS) },
        user_objects: unsafe { GetGuiResources(process, GR_USEROBJECTS) },
        windows: windows.total,
        visible_windows: windows.visible,
        working_set: memory.WorkingSetSize,
        private_bytes: memory.PagefileUsage,
    }
}

#[cfg(not(windows))]
fn process_snapshot() -> ProcessSnapshot {
    ProcessSnapshot::default()
}

#[cfg(windows)]
fn trim_working_set() -> bool {
    use windows::Win32::System::{ProcessStatus::EmptyWorkingSet, Threading::GetCurrentProcess};
    unsafe { EmptyWorkingSet(GetCurrentProcess()).is_ok() }
}

#[cfg(not(windows))]
fn trim_working_set() -> bool {
    false
}

fn execute_probe(config: ProbeConfig) -> i32 {
    match run_probe(config) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("SNOTRA_EFRAME_PROBE_ERROR={error}");
            1
        }
    }
}

fn main() {
    let config = match ProbeConfig::parse(std::env::args().skip(1)) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("invalid probe arguments: {error}");
            std::process::exit(2);
        }
    };
    if config.host == HostMode::Standalone {
        let exit_code = execute_probe(config);
        if exit_code != 0 {
            std::process::exit(exit_code);
        }
        return;
    }
    let ui_thread = Arc::new(Mutex::new(None));
    let ui_thread_for_setup = Arc::clone(&ui_thread);
    let process_exit_code = Arc::new(AtomicI32::new(0));
    let exit_code_for_setup = Arc::clone(&process_exit_code);
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let thread = thread::Builder::new()
                .name("snotra-eframe-glow-ui".to_owned())
                .spawn(move || {
                    let exit_code = execute_probe(config);
                    exit_code_for_setup.store(exit_code, Ordering::Release);
                    app_handle.exit(exit_code);
                })
                .map_err(|error| error.to_string())?;
            if let Ok(mut slot) = ui_thread_for_setup.lock() {
                *slot = Some(thread);
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build Snotra eframe/glow MVP");
    let tauri_exit_code = app.run_return(|_, _| {});
    if let Ok(mut slot) = ui_thread.lock()
        && let Some(thread) = slot.take()
    {
        let _ = thread.join();
    }
    let probe_exit_code = process_exit_code.load(Ordering::Acquire);
    let exit_code = if probe_exit_code == 0 {
        tauri_exit_code
    } else {
        probe_exit_code
    };
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_adoption_gate_baseline() {
        let config = ProbeConfig::parse(Vec::new()).expect("default probe config");
        assert_eq!(config.host, HostMode::Tauri);
        assert_eq!(config.hardware, HardwareMode::Preferred);
        assert_eq!(config.font, FontChoice::YuGothic);
        assert_eq!(config.font_storage, FontStorage::Owned);
        assert_eq!(config.scenario, Scenario::KeepWindow);
        assert_eq!(config.visible_wait_ms, 5_000);
        assert_eq!(config.hidden_wait_ms, 3_000);
    }

    #[test]
    fn parses_font_scenario_and_wait_overrides() {
        let config = ProbeConfig::parse([
            "--font".to_owned(),
            "msgothic".to_owned(),
            "--font-storage".to_owned(),
            "static".to_owned(),
            "--scenario".to_owned(),
            "drop-font".to_owned(),
            "--visible-wait-ms".to_owned(),
            "250".to_owned(),
            "--hidden-wait-ms".to_owned(),
            "750".to_owned(),
        ])
        .expect("explicit probe config");
        assert_eq!(config.font, FontChoice::MsGothic);
        assert_eq!(config.font_storage, FontStorage::Static);
        assert_eq!(config.scenario, Scenario::DropFontOnHide);
        assert_eq!(config.visible_wait_ms, 250);
        assert_eq!(config.hidden_wait_ms, 750);
    }

    #[test]
    fn rejects_unknown_options_and_values() {
        assert!(ProbeConfig::parse(["--font".to_owned(), "unknown".to_owned()]).is_err());
        assert!(ProbeConfig::parse(["--font-storage".to_owned(), "unknown".to_owned()]).is_err());
        assert!(ProbeConfig::parse(["--wat".to_owned()]).is_err());
        assert!(ProbeConfig::parse(["--hidden-wait-ms".to_owned()]).is_err());
    }

    #[test]
    fn parses_deferred_viewport_scenario() {
        let config = ProbeConfig::parse(["--scenario".to_owned(), "deferred-viewport".to_owned()])
            .expect("deferred viewport probe config");
        assert_eq!(config.scenario, Scenario::DeferredViewport);
    }

    #[test]
    fn parses_standalone_host() {
        let config = ProbeConfig::parse([
            "--host".to_owned(),
            "standalone".to_owned(),
            "--hardware".to_owned(),
            "off".to_owned(),
        ])
        .expect("standalone software probe config");
        assert_eq!(config.host, HostMode::Standalone);
        assert_eq!(config.hardware, HardwareMode::Off);
    }
}
