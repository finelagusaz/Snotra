#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Issue #532 park-surface 統合スパイク。
//!
//! Tauri managed host（updater / global shortcut Alt+Q）をメインスレッドに、
//! glow_lifecycle_main で採用ゲートを通過した park-surface レンダラーを専用
//! UI スレッドに置き、同一プロセス・同一 Release ビルドで次を計測する。
//!
//! - コールドスタートの内訳（engine / event loop / GL / egui / font / 初回フレーム）
//! - Alt+Q → unpark → focus → Alt 解放 → warm フレームの時間
//! - 反復 park/unpark 耐久（handle / GDI / USER / private bytes の累積）
//! - Tauri Updater との共存

use std::{
    ffi::CString,
    num::NonZeroU32,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use eframe::egui;
use glutin::{
    config::ConfigTemplateBuilder,
    context::{ContextApi, ContextAttributesBuilder, PossiblyCurrentContext},
    display::{GetGlDisplay as _, GlDisplay as _},
    prelude::*,
    surface::{Surface, SwapInterval, WindowSurface},
};
use glutin_winit::{ApiPreference, DisplayBuilder, GlWindow as _};
use raw_window_handle::HasWindowHandle as _;
use snotra_core::{
    config::{AutoUpdateMode, Config},
    engine::Engine,
    history::HistoryStore,
    indexer::AppEntry,
    ui_types::SearchResult,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_updater::UpdaterExt;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::{Window, WindowAttributes, WindowId},
};

const ENTRY_COUNT: usize = 10_000;
const FONT_PATH: &str = "C:/Windows/Fonts/YuGothM.ttc";
const WINDOW_TITLE: &str = "Snotra park-surface host MVP";
const ALT_RELEASE_POLL_MS: u64 = 10;
const ALT_RELEASE_TIMEOUT_MS: u64 = 350;

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProbeConfig {
    /// 0 は対話モード（Alt+Q 駆動のみ）。1 以上で自動 park/unpark 反復。
    cycles: usize,
    visible_wait_ms: u64,
    hidden_wait_ms: u64,
    focus: bool,
    start_visible: bool,
}

impl ProbeConfig {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut config = Self {
            cycles: 0,
            visible_wait_ms: 5_000,
            hidden_wait_ms: 3_000,
            focus: true,
            start_visible: true,
        };
        let mut args = args.into_iter();
        while let Some(option) = args.next() {
            let value = args
                .next()
                .ok_or_else(|| format!("{option} requires a value"))?;
            match option.as_str() {
                "--cycles" => {
                    config.cycles = value
                        .parse()
                        .map_err(|_| format!("{option} requires an integer: {value}"))?;
                }
                "--visible-wait-ms" => config.visible_wait_ms = parse_u64(&option, &value)?,
                "--hidden-wait-ms" => config.hidden_wait_ms = parse_u64(&option, &value)?,
                "--focus" => {
                    config.focus = match value.as_str() {
                        "on" => true,
                        "off" => false,
                        unknown => return Err(format!("unknown focus mode: {unknown}")),
                    };
                }
                "--start" => {
                    config.start_visible = match value.as_str() {
                        "visible" => true,
                        "hidden" => false,
                        unknown => return Err(format!("unknown start mode: {unknown}")),
                    };
                }
                unknown => return Err(format!("unknown option: {unknown}")),
            }
        }
        Ok(config)
    }
}

fn parse_u64(option: &str, value: &str) -> Result<u64, String> {
    value
        .parse()
        .map_err(|_| format!("{option} requires an integer: {value}"))
}

// ---------------------------------------------------------------------------
// ホスト側（Tauri メインスレッド）のホットキー判定
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HotkeyPlan {
    HideNow,
    ShowAfterAltRelease,
    ShowNow,
}

/// Alt+Q 押下時の分岐。製品版 `src-tauri` と同じく、表示中なら即 hide、
/// 非表示中は Alt が押されている限り解放を待ってから show する。
fn plan_hotkey(visible: bool, alt_pressed: bool) -> HotkeyPlan {
    if visible {
        HotkeyPlan::HideNow
    } else if alt_pressed {
        HotkeyPlan::ShowAfterAltRelease
    } else {
        HotkeyPlan::ShowNow
    }
}

// ---------------------------------------------------------------------------
// UI スレッドのライフサイクル状態機械
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LifecycleState {
    Visible,
    Suspended,
    Recreating,
    Exiting,
}

impl LifecycleState {
    fn keeps_gl_context_and_painter(self) -> bool {
        self != Self::Exiting
    }

    fn has_active_window_or_surface(self) -> bool {
        matches!(self, Self::Visible | Self::Recreating)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LifecycleEvent {
    Hide,
    Show,
    FramePresented,
    Exit,
}

fn transition(state: LifecycleState, event: LifecycleEvent) -> Result<LifecycleState, String> {
    match (state, event) {
        (LifecycleState::Visible, LifecycleEvent::Hide) => Ok(LifecycleState::Suspended),
        (LifecycleState::Suspended, LifecycleEvent::Show) => Ok(LifecycleState::Recreating),
        (LifecycleState::Recreating, LifecycleEvent::FramePresented) => Ok(LifecycleState::Visible),
        (LifecycleState::Visible, LifecycleEvent::Exit)
        | (LifecycleState::Suspended, LifecycleEvent::Exit) => Ok(LifecycleState::Exiting),
        _ => Err(format!(
            "invalid lifecycle transition: {state:?} + {event:?}"
        )),
    }
}

#[derive(Clone, Copy, Debug)]
enum HostCommand {
    Show { hotkey_started: Instant },
    Hide,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UiAction {
    Unpark,
    Park,
    Refocus,
    /// unpark 完了（FramePresented）後まで Hide を繰り延べる。
    Defer,
    Ignore,
}

/// ホストコマンドを現在状態へ適用する計画。冪等性（Visible+Show / Suspended+Hide）
/// と、unpark 進行中に届いた Hide の繰り延べをここで一元的に決める。
fn plan_ui_action(state: LifecycleState, command: &HostCommand) -> UiAction {
    match (state, command) {
        (LifecycleState::Visible, HostCommand::Show { .. }) => UiAction::Refocus,
        (LifecycleState::Visible, HostCommand::Hide) => UiAction::Park,
        (LifecycleState::Suspended, HostCommand::Show { .. }) => UiAction::Unpark,
        (LifecycleState::Suspended, HostCommand::Hide) => UiAction::Ignore,
        (LifecycleState::Recreating, HostCommand::Show { .. }) => UiAction::Ignore,
        (LifecycleState::Recreating, HostCommand::Hide) => UiAction::Defer,
        (LifecycleState::Exiting, _) => UiAction::Ignore,
    }
}

// ---------------------------------------------------------------------------
// UI スレッド本体
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum ScheduledAction {
    Hide,
    Show,
    Exit,
}

struct GlResources {
    context: PossiblyCurrentContext,
    surface: Option<Surface<WindowSurface>>,
    window: Option<Window>,
    parked_surface: Option<Surface<WindowSurface>>,
    parked_window: Option<Window>,
    parked_position: Option<PhysicalPosition<i32>>,
    egui_glow: egui_glow::EguiGlow,
}

struct HostProbe {
    config: ProbeConfig,
    state: LifecycleState,
    gl: Option<GlResources>,
    engine: Engine,
    query: String,
    results: Vec<SearchResult>,
    scheduled: Option<(Instant, ScheduledAction)>,
    show_started: Option<Instant>,
    hotkey_started: Option<Instant>,
    pending_hide: bool,
    focus_search: bool,
    completed_cycles: usize,
    warm_times_ms: Vec<f64>,
    started_at: Instant,
    visible_flag: Arc<AtomicBool>,
    error: Option<String>,
}

impl HostProbe {
    fn new(config: ProbeConfig, visible_flag: Arc<AtomicBool>, started_at: Instant) -> Self {
        let engine_started = Instant::now();
        let engine = build_verification_engine(ENTRY_COUNT);
        log_phase(started_at, "engine_ready");
        eprintln!(
            "SNOTRA_PARK_HOST_INFO engine_build_ms={:.3} entries={ENTRY_COUNT}",
            engine_started.elapsed().as_secs_f64() * 1000.0
        );
        let results = engine.recent_history();
        Self {
            config,
            state: LifecycleState::Recreating,
            gl: None,
            engine,
            query: String::new(),
            results,
            scheduled: None,
            show_started: Some(started_at),
            hotkey_started: None,
            pending_hide: false,
            focus_search: true,
            completed_cycles: 0,
            warm_times_ms: Vec::new(),
            started_at,
            visible_flag,
            error: None,
        }
    }

    fn initialize(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let attributes = window_attributes();
        let template = ConfigTemplateBuilder::new()
            .with_depth_size(0)
            .with_stencil_size(0);
        let (window, config) = DisplayBuilder::new()
            .with_preference(ApiPreference::FallbackEgl)
            .with_window_attributes(Some(attributes))
            .build(event_loop, template, |mut configs| {
                configs
                    .next()
                    .expect("glutin returned no matching GL configuration")
            })
            .map_err(|error| format!("failed to create GL display/config: {error}"))?;
        let window =
            window.ok_or_else(|| "GL display did not create a Windows window".to_owned())?;
        log_phase(self.started_at, "gl_window_config_ready");
        let raw_window_handle = window
            .window_handle()
            .map_err(|error| format!("failed to read initial window handle: {error}"))?
            .as_raw();
        let context_attributes = ContextAttributesBuilder::new().build(Some(raw_window_handle));
        let fallback_context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(None))
            .build(Some(raw_window_handle));
        let display = config.display();
        let not_current_context = unsafe {
            display
                .create_context(&config, &context_attributes)
                .or_else(|_| display.create_context(&config, &fallback_context_attributes))
        }
        .map_err(|error| format!("failed to create GL context: {error}"))?;
        let surface_attributes = window
            .build_surface_attributes(Default::default())
            .map_err(|error| format!("failed to build initial surface attributes: {error}"))?;
        let surface = unsafe { display.create_window_surface(&config, &surface_attributes) }
            .map_err(|error| format!("failed to create initial GL surface: {error}"))?;
        let context = not_current_context
            .make_current(&surface)
            .map_err(|error| format!("failed to make initial GL context current: {error}"))?;
        if let Err(error) = surface.set_swap_interval(&context, SwapInterval::Wait(NonZeroU32::MIN))
        {
            eprintln!("SNOTRA_PARK_HOST_WARNING failed_to_set_swap_interval={error}");
        }
        log_phase(self.started_at, "gl_context_current");

        let glow_context = unsafe {
            egui_glow::glow::Context::from_loader_function(|symbol| {
                let symbol = CString::new(symbol).expect("GL symbol contains an interior NUL");
                display.get_proc_address(&symbol) as *const _
            })
        };
        let egui_glow = egui_glow::EguiGlow::new(
            event_loop,
            std::sync::Arc::new(glow_context),
            None,
            Some(window.scale_factor() as f32),
            false,
        );
        log_phase(self.started_at, "egui_ready");
        let font_bytes = configure_static_font(&egui_glow.egui_ctx)?;
        log_phase(self.started_at, "font_configured");
        eprintln!("SNOTRA_PARK_HOST_INFO font_bytes={font_bytes}");
        egui_glow.egui_ctx.request_repaint();

        self.gl = Some(GlResources {
            context,
            surface: Some(surface),
            window: Some(window),
            parked_surface: None,
            parked_window: None,
            parked_position: None,
            egui_glow,
        });
        // visible フラグは「実際の可視性を変えるすべての箇所」（ここ・unpark・park）が
        // 書く。ウィンドウは可視で生成されるため、ここで true にしないとホスト側の
        // plan_hotkey が永遠に false を読み、Alt+Q が Hide へ到達しない（実測）。
        self.visible_flag.store(true, Ordering::SeqCst);
        self.request_redraw();
        Ok(())
    }

    /// park-surface: HWND / Surface / Context を破棄せず、Surface を 1×1 へ縮小して
    /// Window を隠す。Context → Surface/HDC → Window/HWND の順序を守る。
    fn park(&mut self) -> Result<(), String> {
        self.state = transition(self.state, LifecycleEvent::Hide)?;
        if !self.state.keeps_gl_context_and_painter() || self.state.has_active_window_or_surface() {
            return Err("suspended state violates the resource ownership contract".to_owned());
        }
        let gl = self
            .gl
            .as_mut()
            .ok_or_else(|| "GL resources are missing during park".to_owned())?;
        let surface = gl
            .surface
            .as_ref()
            .ok_or_else(|| "GL surface is missing during park".to_owned())?;
        surface.resize(&gl.context, NonZeroU32::MIN, NonZeroU32::MIN);
        if let Some(window) = gl.window.as_ref() {
            window.set_visible(false);
        }
        gl.context
            .make_not_current_in_place()
            .map_err(|error| format!("failed to detach GL context while parking: {error}"))?;
        gl.parked_surface = gl.surface.take();
        let window = gl
            .window
            .take()
            .ok_or_else(|| "window is missing while parking".to_owned())?;
        gl.parked_position = window.outer_position().ok();
        let _ = window.request_inner_size(PhysicalSize::new(1, 1));
        window.set_outer_position(PhysicalPosition::new(-32_000, -32_000));
        gl.parked_window = Some(window);
        self.visible_flag.store(false, Ordering::SeqCst);
        let _ = trim_working_set();
        log_metric("hidden_parked", self.completed_cycles, None);
        if self.config.cycles > 0 {
            self.scheduled = Some((
                Instant::now() + Duration::from_millis(self.config.hidden_wait_ms),
                ScheduledAction::Show,
            ));
        }
        Ok(())
    }

    fn unpark(&mut self, hotkey_started: Option<Instant>) -> Result<(), String> {
        self.state = transition(self.state, LifecycleEvent::Show)?;
        if !self.state.keeps_gl_context_and_painter() || !self.state.has_active_window_or_surface()
        {
            return Err("recreating state violates the resource ownership contract".to_owned());
        }
        self.show_started = Some(Instant::now());
        self.hotkey_started = hotkey_started;
        let gl = self
            .gl
            .as_mut()
            .ok_or_else(|| "GL resources are missing during unpark".to_owned())?;
        if gl.window.is_some() || gl.surface.is_some() {
            return Err("window/surface survived the parked state".to_owned());
        }
        let window = gl
            .parked_window
            .take()
            .ok_or_else(|| "parked window is missing during unpark".to_owned())?;
        if let Some(position) = gl.parked_position.take() {
            window.set_outer_position(position);
        }
        let _ = window.request_inner_size(PhysicalSize::new(640, 360));
        let surface = gl
            .parked_surface
            .take()
            .ok_or_else(|| "parked GL surface is missing during unpark".to_owned())?;
        gl.context
            .make_current(&surface)
            .map_err(|error| format!("failed to reattach parked GL context: {error}"))?;
        surface.resize(
            &gl.context,
            NonZeroU32::new(640).expect("640 is non-zero"),
            NonZeroU32::new(360).expect("360 is non-zero"),
        );
        gl.surface = Some(surface);
        window.set_visible(true);
        gl.window = Some(window);
        self.visible_flag.store(true, Ordering::SeqCst);
        self.focus_search = true;
        if self.config.focus {
            self.spawn_focus_sequence(hotkey_started)?;
        }
        self.request_redraw();
        Ok(())
    }

    /// 製品版と同じ「show → focus → WM_NULL 待ち → 残留 Alt 解除」を、UI スレッドを
    /// ブロックしない helper スレッドで実行する（製品版も helper 側で実行している）。
    fn spawn_focus_sequence(&self, hotkey_started: Option<Instant>) -> Result<(), String> {
        let hwnd = self.active_hwnd()?;
        thread::Builder::new()
            .name("snotra-park-host-focus".to_owned())
            .spawn(move || {
                focus_and_release_alt(hwnd);
                if let Some(started) = hotkey_started {
                    eprintln!(
                        "SNOTRA_PARK_HOST_HOTKEY=input_ready elapsed_ms={:.3}",
                        started.elapsed().as_secs_f64() * 1000.0
                    );
                }
            })
            .map_err(|error| format!("failed to spawn focus sequence: {error}"))?;
        Ok(())
    }

    fn active_hwnd(&self) -> Result<isize, String> {
        let window = self
            .gl
            .as_ref()
            .and_then(|gl| gl.window.as_ref())
            .ok_or_else(|| "window is missing for the focus sequence".to_owned())?;
        let handle = window
            .window_handle()
            .map_err(|error| format!("failed to read window handle for focus: {error}"))?
            .as_raw();
        match handle {
            raw_window_handle::RawWindowHandle::Win32(handle) => Ok(handle.hwnd.get()),
            other => Err(format!("unexpected window handle kind: {other:?}")),
        }
    }

    fn refocus(&mut self) -> Result<(), String> {
        self.focus_search = true;
        let hotkey_started = self.hotkey_started.take();
        if self.config.focus {
            self.spawn_focus_sequence(hotkey_started)?;
        }
        self.request_redraw();
        Ok(())
    }

    fn apply_command(&mut self, command: HostCommand) -> Result<(), String> {
        if self.gl.is_none() {
            eprintln!("SNOTRA_PARK_HOST_WARNING command_before_initialization={command:?}");
            return Ok(());
        }
        match plan_ui_action(self.state, &command) {
            UiAction::Unpark => {
                let hotkey_started = match command {
                    HostCommand::Show { hotkey_started } => Some(hotkey_started),
                    HostCommand::Hide => None,
                };
                self.unpark(hotkey_started)
            }
            UiAction::Park => self.park(),
            UiAction::Refocus => {
                if let HostCommand::Show { hotkey_started } = command {
                    self.hotkey_started = Some(hotkey_started);
                }
                self.refocus()
            }
            UiAction::Defer => {
                self.pending_hide = true;
                Ok(())
            }
            UiAction::Ignore => Ok(()),
        }
    }

    fn render(&mut self) -> Result<(), String> {
        if !self.state.has_active_window_or_surface() {
            return Ok(());
        }
        let gl = self
            .gl
            .as_mut()
            .ok_or_else(|| "GL resources are missing during render".to_owned())?;
        let window = gl
            .window
            .as_ref()
            .ok_or_else(|| "window is missing during render".to_owned())?;
        let surface = gl
            .surface
            .as_ref()
            .ok_or_else(|| "surface is missing during render".to_owned())?;

        let query = &mut self.query;
        let results = &mut self.results;
        let engine = &mut self.engine;
        let focus_search = &mut self.focus_search;
        gl.egui_glow.run(window, |ui| {
            ui.heading("Snotra park-surface 統合スパイク");
            ui.label("Tauri host（updater / Alt+Q）+ park-surface renderer を同一プロセスで実行");
            let response = ui.add(
                egui::TextEdit::singleline(query).hint_text("アプリ、フォルダー、コマンドを検索"),
            );
            if *focus_search {
                response.request_focus();
                *focus_search = false;
            }
            if response.changed() {
                *results = if query.is_empty() {
                    engine.recent_history()
                } else {
                    engine.search(query)
                };
            }
            ui.separator();
            for result in results.iter().take(8) {
                ui.label(&result.name);
            }
            ui.small(format!(
                "{} results / Engine {ENTRY_COUNT} entries / Alt+Q: 表示切替 / Esc: 非表示",
                results.len()
            ));
        });
        gl.egui_glow.painter.clear(
            window.inner_size().into(),
            egui::Rgba::from_gray(0.96).to_array(),
        );
        gl.egui_glow.paint(window);
        surface
            .swap_buffers(&gl.context)
            .map_err(|error| format!("failed to present GL frame: {error}"))?;

        if self.state == LifecycleState::Recreating {
            self.state = transition(self.state, LifecycleEvent::FramePresented)?;
            let elapsed = self
                .show_started
                .take()
                .map(|started| started.elapsed())
                .unwrap_or_default();
            if self.completed_cycles == 0 && self.warm_times_ms.is_empty() {
                log_metric("initial_frame_presented", 0, Some(elapsed));
                log_phase(self.started_at, "first_frame_presented");
            } else {
                self.warm_times_ms.push(elapsed.as_secs_f64() * 1000.0);
                log_metric("warm_frame_presented", self.completed_cycles, Some(elapsed));
            }
            if let Some(started) = self.hotkey_started.take() {
                eprintln!(
                    "SNOTRA_PARK_HOST_HOTKEY=warm_frame_after_hotkey elapsed_ms={:.3}",
                    started.elapsed().as_secs_f64() * 1000.0
                );
            }
            if self.pending_hide {
                self.pending_hide = false;
                log_metric("visible_settled", self.completed_cycles, None);
                return self.park();
            }
            if self.config.cycles > 0 {
                self.scheduled = Some((
                    Instant::now() + Duration::from_millis(self.config.visible_wait_ms),
                    if self.completed_cycles >= self.config.cycles {
                        ScheduledAction::Exit
                    } else {
                        ScheduledAction::Hide
                    },
                ));
            }
        }
        Ok(())
    }

    fn perform_scheduled(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let Some((deadline, action)) = self.scheduled else {
            return Ok(());
        };
        if Instant::now() < deadline {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
            return Ok(());
        }
        self.scheduled = None;
        match action {
            ScheduledAction::Hide => {
                log_metric("visible_settled", self.completed_cycles, None);
                self.park()
            }
            ScheduledAction::Show => {
                self.completed_cycles += 1;
                self.unpark(None)
            }
            ScheduledAction::Exit => {
                log_metric("final_visible_settled", self.completed_cycles, None);
                self.finish(event_loop)
            }
        }
    }

    fn finish(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        self.state = transition(self.state, LifecycleEvent::Exit)?;
        if let Some(gl) = self.gl.as_mut() {
            if !gl.context.is_current() {
                // park 中の終了。Painter の破棄には current な Context が必要なので、
                // parked surface へ付け直してから破棄する。
                if let Some(surface) = gl.parked_surface.as_ref() {
                    gl.context
                        .make_current(surface)
                        .map_err(|error| format!("failed to re-attach context for exit: {error}"))?;
                } else {
                    return Err(
                        "refusing to destroy Painter without a current GL context".to_owned()
                    );
                }
            }
            gl.egui_glow.destroy();
        }
        if !self.warm_times_ms.is_empty() {
            let count = self.warm_times_ms.len();
            let sum: f64 = self.warm_times_ms.iter().sum();
            let min = self.warm_times_ms.iter().cloned().fold(f64::MAX, f64::min);
            let max = self.warm_times_ms.iter().cloned().fold(f64::MIN, f64::max);
            eprintln!(
                "SNOTRA_PARK_HOST_SUMMARY warm_frames={count} warm_ms_min={min:.3} warm_ms_avg={:.3} warm_ms_max={max:.3}",
                sum / count as f64
            );
        }
        event_loop.exit();
        Ok(())
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: String) {
        eprintln!("SNOTRA_PARK_HOST_ERROR {error}");
        self.error = Some(error);
        event_loop.exit();
    }

    fn request_redraw(&self) {
        if let Some(window) = self.gl.as_ref().and_then(|gl| gl.window.as_ref()) {
            window.request_redraw();
        }
    }
}

impl ApplicationHandler<HostCommand> for HostProbe {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gl.is_none() {
            match self.initialize(event_loop) {
                Ok(()) => {
                    if !self.config.start_visible {
                        // 初回フレーム提示後に park する（tray 常駐相当の開始状態）。
                        self.pending_hide = true;
                    }
                }
                Err(error) => self.fail(event_loop, error),
            }
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, command: HostCommand) {
        if let Err(error) = self.apply_command(command) {
            self.fail(event_loop, error);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(current_window_id) = self
            .gl
            .as_ref()
            .and_then(|gl| gl.window.as_ref())
            .map(Window::id)
        else {
            return;
        };
        if current_window_id != window_id {
            return;
        }

        if let Some(gl) = self.gl.as_mut() {
            let window = gl
                .window
                .as_ref()
                .expect("current window id was read immediately before this borrow");
            let response = gl.egui_glow.on_window_event(window, &event);
            if response.repaint {
                window.request_redraw();
            }
        }
        let escape_pressed = matches!(
            &event,
            WindowEvent::KeyboardInput { event, .. }
                if event.state.is_pressed()
                    && event.logical_key
                        == winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape)
        );
        let result = if escape_pressed && self.state == LifecycleState::Visible {
            // 製品版の Esc と同じく非表示化。自動反復中はスケジュールを乱さない。
            if self.config.cycles == 0 {
                log_metric("visible_settled", self.completed_cycles, None);
                self.park()
            } else {
                Ok(())
            }
        } else {
            match event {
                WindowEvent::CloseRequested => self.finish(event_loop),
                WindowEvent::RedrawRequested => self.render(),
                WindowEvent::Resized(size) if size.width > 0 && size.height > 0 => {
                    if let Some(gl) = self.gl.as_mut()
                        && let Some(surface) = gl.surface.as_ref()
                        && gl.context.is_current()
                    {
                        surface.resize(
                            &gl.context,
                            NonZeroU32::new(size.width).unwrap_or(NonZeroU32::MIN),
                            NonZeroU32::new(size.height).unwrap_or(NonZeroU32::MIN),
                        );
                    }
                    self.request_redraw();
                    Ok(())
                }
                _ => Ok(()),
            }
        };
        if let Err(error) = result {
            self.fail(event_loop, error);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.perform_scheduled(event_loop) {
            self.fail(event_loop, error);
        }
    }
}

fn window_attributes() -> WindowAttributes {
    use winit::platform::windows::WindowAttributesExtWindows as _;

    Window::default_attributes()
        .with_title(WINDOW_TITLE)
        .with_inner_size(LogicalSize::new(640.0, 360.0))
        .with_drag_and_drop(false)
        // 隠れた HWND には RedrawRequested の配送保証が無いため、初回フレームは
        // 可視状態で提示し、--start hidden は初回フレーム後に park する。
        .with_visible(true)
}

// ---------------------------------------------------------------------------
// Windows 統合（Alt 検出・フォーカス・Alt 解除）
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn is_alt_pressed() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_LMENU, VK_MENU, VK_RMENU,
    };

    unsafe {
        GetAsyncKeyState(VK_MENU.0 as i32) < 0
            || GetAsyncKeyState(VK_LMENU.0 as i32) < 0
            || GetAsyncKeyState(VK_RMENU.0 as i32) < 0
    }
}

#[cfg(not(windows))]
fn is_alt_pressed() -> bool {
    false
}

fn wait_alt_release_or_timeout() {
    if !is_alt_pressed() {
        return;
    }

    let started = Instant::now();
    let timeout = Duration::from_millis(ALT_RELEASE_TIMEOUT_MS);
    let poll = Duration::from_millis(ALT_RELEASE_POLL_MS);
    while started.elapsed() < timeout {
        if !is_alt_pressed() {
            return;
        }
        thread::sleep(poll);
    }
}

#[cfg(windows)]
fn make_key_input(
    vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY,
    is_up: bool,
) -> windows::Win32::UI::Input::KeyboardAndMouse::INPUT {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP,
    };

    let mut input = INPUT {
        r#type: INPUT_KEYBOARD,
        ..Default::default()
    };
    input.Anonymous.ki = KEYBDINPUT {
        wVk: vk,
        dwFlags: if is_up {
            KEYEVENTF_KEYUP
        } else {
            KEYBD_EVENT_FLAGS::default()
        },
        ..Default::default()
    };
    input
}

/// フォーカス移動後に残留Alt状態を解除する。ダミーvkE8を先行させるのは、
/// bare Alt-upとしてメニューやビープを発火させないためである。
#[cfg(windows)]
fn send_alt_key_up() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, SendInput, VIRTUAL_KEY, VK_LMENU, VK_MENU, VK_RMENU,
    };

    const VK_MASK: VIRTUAL_KEY = VIRTUAL_KEY(0xE8);
    let inputs = [
        make_key_input(VK_MASK, false),
        make_key_input(VK_MASK, true),
        make_key_input(VK_MENU, true),
        make_key_input(VK_LMENU, true),
        make_key_input(VK_RMENU, true),
    ];
    unsafe {
        let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
    thread::sleep(Duration::from_millis(5));
}

#[cfg(not(windows))]
fn send_alt_key_up() {}

/// SetForegroundWindow は部分的に非同期なので、WM_NULL で対象スレッドが
/// activation メッセージを処理し終えるまで待ってから Alt を解放する。
#[cfg(windows)]
fn focus_and_release_alt(hwnd: isize) {
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        SMTO_NORMAL, SendMessageTimeoutW, SetForegroundWindow, WM_NULL,
    };

    let hwnd = HWND(hwnd as *mut _);
    unsafe {
        let _ = SetForegroundWindow(hwnd);
        let mut result = 0usize;
        let _ = SendMessageTimeoutW(
            hwnd,
            WM_NULL,
            WPARAM(0),
            LPARAM(0),
            SMTO_NORMAL,
            100,
            Some(&mut result),
        );
    }
    // 残留 Alt の解除は、物理 Alt が既に離れているときにしか意味を持たない。
    // 押下中に注入すると OS の論理修飾状態だけが解放され、Alt を押し直すまで
    // Alt+Q が発火しない不感帯を作る（タイムアウト show 経路で実測）。
    if is_alt_pressed() {
        eprintln!("SNOTRA_PARK_HOST_HOTKEY=alt_release_skipped_physical_alt_down");
        return;
    }
    send_alt_key_up();
}

#[cfg(not(windows))]
fn focus_and_release_alt(_hwnd: isize) {}

// ---------------------------------------------------------------------------
// Engine・フォント・計測
// ---------------------------------------------------------------------------

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
        "snotra-egui-park-host-mvp-history-{}-unused",
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

static STATIC_FONT: OnceLock<Box<[u8]>> = OnceLock::new();

fn configure_static_font(context: &egui::Context) -> Result<usize, String> {
    if STATIC_FONT.get().is_none() {
        let bytes = std::fs::read(FONT_PATH)
            .map_err(|error| format!("failed to read {FONT_PATH}: {error}"))?
            .into_boxed_slice();
        let _ = STATIC_FONT.set(bytes);
    }
    let bytes: &'static [u8] = STATIC_FONT
        .get()
        .ok_or_else(|| "static font initialization failed".to_owned())?;
    let mut font = egui::FontData::from_static(bytes);
    font.tweak = egui::FontTweak {
        scale: 1.0,
        y_offset_factor: 0.3,
        y_offset: 0.0,
        ..Default::default()
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("jp_font".to_owned(), font.into());
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("jp_font".to_owned());
    }
    context.set_fonts(fonts);
    Ok(bytes.len())
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

fn log_phase(started_at: Instant, name: &str) {
    eprintln!(
        "SNOTRA_PARK_HOST_PHASE name={name} elapsed_ms={:.3}",
        started_at.elapsed().as_secs_f64() * 1000.0
    );
}

fn log_metric(checkpoint: &str, cycle: usize, elapsed: Option<Duration>) {
    let snapshot = process_snapshot();
    let elapsed_ms = elapsed
        .map(|duration| duration.as_secs_f64() * 1_000.0)
        .unwrap_or(-1.0);
    eprintln!(
        "SNOTRA_PARK_HOST_METRIC checkpoint={checkpoint} cycle={cycle} elapsed_ms={elapsed_ms:.3} working_set_bytes={} private_bytes={} handles={} gdi={} user={} windows={} visible_windows={}",
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

// ---------------------------------------------------------------------------
// Tauri ホスト（updater・global shortcut）
// ---------------------------------------------------------------------------

fn parse_update_mode(value: Option<&str>) -> AutoUpdateMode {
    match value {
        Some("check_only") => AutoUpdateMode::CheckOnly,
        Some("disabled") => AutoUpdateMode::Disabled,
        _ => AutoUpdateMode::Full,
    }
}

fn begin_updater_log(handle: tauri::AppHandle) {
    let mode = parse_update_mode(std::env::var("SNOTRA_EGUI_MVP_UPDATE_MODE").ok().as_deref());
    if mode == AutoUpdateMode::Disabled {
        eprintln!("SNOTRA_PARK_HOST_UPDATER=disabled");
        return;
    }
    tauri::async_runtime::spawn(async move {
        let outcome = match handle.updater() {
            Ok(updater) => match updater.check().await {
                Ok(Some(update)) => format!(
                    "available version={} can_install={}",
                    update.version,
                    mode == AutoUpdateMode::Full
                ),
                Ok(None) => "up_to_date".to_owned(),
                Err(error) => format!("failed message={error}"),
            },
            Err(error) => format!("failed message={error}"),
        };
        eprintln!("SNOTRA_PARK_HOST_UPDATER={outcome}");
    });
}

type SharedProxy = Arc<Mutex<Option<EventLoopProxy<HostCommand>>>>;

fn dispatch_hotkey(
    proxy: &EventLoopProxy<HostCommand>,
    visible: &Arc<AtomicBool>,
    generation: &Arc<AtomicU64>,
) {
    let current_generation = generation.fetch_add(1, Ordering::SeqCst) + 1;
    match plan_hotkey(visible.load(Ordering::SeqCst), is_alt_pressed()) {
        HotkeyPlan::HideNow => {
            visible.store(false, Ordering::SeqCst);
            let _ = proxy.send_event(HostCommand::Hide);
            eprintln!("SNOTRA_PARK_HOST_HOTKEY=hide_dispatched");
        }
        // Show 経路では visible を先読みで立てない。真実源は UI スレッドの実際の
        // show/hide である。配送時の楽観 true は「Alt 押下中の再押下」を Hide と
        // 誤読させ、待機中の Show が世代キャンセルされて何も表示されない（実測）。
        // 製品版 `src-tauri` も visible を show 完了時に立てる。
        HotkeyPlan::ShowAfterAltRelease => {
            let hotkey_started = Instant::now();
            let proxy = proxy.clone();
            let generation = Arc::clone(generation);
            thread::spawn(move || {
                wait_alt_release_or_timeout();
                eprintln!(
                    "SNOTRA_PARK_HOST_ALT_WAIT elapsed_ms={:.3}",
                    hotkey_started.elapsed().as_secs_f64() * 1000.0
                );
                if generation.load(Ordering::SeqCst) != current_generation {
                    return;
                }
                let _ = proxy.send_event(HostCommand::Show { hotkey_started });
            });
        }
        HotkeyPlan::ShowNow => {
            let _ = proxy.send_event(HostCommand::Show {
                hotkey_started: Instant::now(),
            });
        }
    }
}

fn run_ui_thread(
    config: ProbeConfig,
    shared_proxy: SharedProxy,
    visible: Arc<AtomicBool>,
    started_at: Instant,
) -> Result<(), String> {
    use winit::platform::windows::EventLoopBuilderExtWindows as _;

    let event_loop = EventLoop::<HostCommand>::with_user_event()
        // Tauri がメインスレッドを所有する。winit のメッセージループはこの専用
        // UI スレッドで動かし、終了前にウィンドウを必ず破棄する。
        .with_any_thread(true)
        .build()
        .map_err(|error| format!("event loop creation failed: {error}"))?;
    event_loop.set_control_flow(ControlFlow::Wait);
    log_phase(started_at, "event_loop_ready");
    if let Ok(mut slot) = shared_proxy.lock() {
        *slot = Some(event_loop.create_proxy());
    }
    let mut probe = HostProbe::new(config, visible, started_at);
    event_loop
        .run_app(&mut probe)
        .map_err(|error| format!("event loop failed: {error}"))?;
    if let Some(error) = probe.error {
        Err(error)
    } else {
        Ok(())
    }
}

fn main() {
    let started_at = Instant::now();
    let config = match ProbeConfig::parse(std::env::args().skip(1)) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("invalid probe arguments: {error}");
            std::process::exit(2);
        }
    };
    eprintln!("SNOTRA_PARK_HOST_CONFIG={config:?}");
    log_metric("process_start", 0, None);

    let shared_proxy: SharedProxy = Arc::new(Mutex::new(None));
    let visible = Arc::new(AtomicBool::new(false));
    let generation = Arc::new(AtomicU64::new(0));
    let ui_thread = Arc::new(Mutex::new(None));
    let ui_thread_for_setup = Arc::clone(&ui_thread);
    let ui_exit_code = Arc::new(AtomicI32::new(0));
    let exit_code_for_setup = Arc::clone(&ui_exit_code);
    let proxy_for_handler = Arc::clone(&shared_proxy);
    let proxy_for_ui = Arc::clone(&shared_proxy);
    let visible_for_handler = Arc::clone(&visible);
    let visible_for_ui = Arc::clone(&visible);

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(move |app| {
            log_phase(started_at, "tauri_setup_entered");
            let alt_q = Shortcut::new(Some(Modifiers::ALT), Code::KeyQ);
            let handler_alt_q = alt_q;
            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(move |_app, shortcut, event| {
                        if shortcut != &handler_alt_q || event.state() != ShortcutState::Pressed {
                            return;
                        }
                        let Some(proxy) = proxy_for_handler
                            .lock()
                            .ok()
                            .and_then(|slot| slot.clone())
                        else {
                            eprintln!("SNOTRA_PARK_HOST_HOTKEY=ignored_before_ui_ready");
                            return;
                        };
                        dispatch_hotkey(&proxy, &visible_for_handler, &generation);
                    })
                    .build(),
            )?;
            app.global_shortcut().register(alt_q)?;
            begin_updater_log(app.handle().clone());

            let app_handle = app.handle().clone();
            let thread = thread::Builder::new()
                .name("snotra-park-host-ui".to_owned())
                .spawn(move || {
                    let exit_code = match run_ui_thread(
                        config,
                        proxy_for_ui,
                        visible_for_ui,
                        started_at,
                    ) {
                        Ok(()) => 0,
                        Err(error) => {
                            eprintln!("SNOTRA_PARK_HOST_ERROR {error}");
                            1
                        }
                    };
                    exit_code_for_setup.store(exit_code, Ordering::Release);
                    app_handle.exit(exit_code);
                })
                .map_err(|error| error.to_string())?;
            log_phase(started_at, "ui_thread_spawned");
            if let Ok(mut slot) = ui_thread_for_setup.lock() {
                *slot = Some(thread);
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build Snotra park-surface host MVP");
    let tauri_exit_code = app.run_return(|_, _| {});
    if let Ok(mut slot) = ui_thread.lock()
        && let Some(thread) = slot.take()
    {
        let _ = thread.join();
    }
    let ui_code = ui_exit_code.load(Ordering::Acquire);
    let exit_code = if ui_code == 0 { tauri_exit_code } else { ui_code };
    log_metric("process_exit", 0, None);
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}

// ---------------------------------------------------------------------------
// テスト
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::{
        HostCommand, HotkeyPlan, LifecycleEvent, LifecycleState, ProbeConfig, UiAction,
        plan_hotkey, plan_ui_action, transition,
    };

    fn show_command() -> HostCommand {
        HostCommand::Show {
            hotkey_started: Instant::now(),
        }
    }

    #[test]
    fn hotkey_hides_immediately_while_visible() {
        assert_eq!(plan_hotkey(true, false), HotkeyPlan::HideNow);
        assert_eq!(plan_hotkey(true, true), HotkeyPlan::HideNow);
    }

    #[test]
    fn hotkey_waits_for_alt_release_before_show() {
        assert_eq!(plan_hotkey(false, true), HotkeyPlan::ShowAfterAltRelease);
    }

    #[test]
    fn hotkey_shows_immediately_without_held_alt() {
        assert_eq!(plan_hotkey(false, false), HotkeyPlan::ShowNow);
    }

    #[test]
    fn ui_show_and_hide_are_idempotent() {
        assert_eq!(
            plan_ui_action(LifecycleState::Visible, &show_command()),
            UiAction::Refocus
        );
        assert_eq!(
            plan_ui_action(LifecycleState::Suspended, &HostCommand::Hide),
            UiAction::Ignore
        );
    }

    #[test]
    fn ui_show_unparks_and_hide_parks_in_stable_states() {
        assert_eq!(
            plan_ui_action(LifecycleState::Suspended, &show_command()),
            UiAction::Unpark
        );
        assert_eq!(
            plan_ui_action(LifecycleState::Visible, &HostCommand::Hide),
            UiAction::Park
        );
    }

    #[test]
    fn ui_hide_during_unpark_is_deferred_until_frame_presented() {
        assert_eq!(
            plan_ui_action(LifecycleState::Recreating, &HostCommand::Hide),
            UiAction::Defer
        );
        assert_eq!(
            plan_ui_action(LifecycleState::Recreating, &show_command()),
            UiAction::Ignore
        );
    }

    #[test]
    fn ui_exiting_state_ignores_all_commands() {
        assert_eq!(
            plan_ui_action(LifecycleState::Exiting, &show_command()),
            UiAction::Ignore
        );
        assert_eq!(
            plan_ui_action(LifecycleState::Exiting, &HostCommand::Hide),
            UiAction::Ignore
        );
    }

    #[test]
    fn lifecycle_park_keeps_context_and_releases_active_slots() {
        let parked = transition(LifecycleState::Visible, LifecycleEvent::Hide).unwrap();
        assert_eq!(parked, LifecycleState::Suspended);
        assert!(parked.keeps_gl_context_and_painter());
        assert!(!parked.has_active_window_or_surface());
    }

    #[test]
    fn lifecycle_unpark_requires_frame_before_visible() {
        let recreating = transition(LifecycleState::Suspended, LifecycleEvent::Show).unwrap();
        assert_eq!(recreating, LifecycleState::Recreating);
        let visible = transition(recreating, LifecycleEvent::FramePresented).unwrap();
        assert_eq!(visible, LifecycleState::Visible);
    }

    #[test]
    fn lifecycle_allows_exit_while_parked() {
        let exiting = transition(LifecycleState::Suspended, LifecycleEvent::Exit).unwrap();
        assert_eq!(exiting, LifecycleState::Exiting);
        assert!(!exiting.keeps_gl_context_and_painter());
    }

    #[test]
    fn lifecycle_rejects_duplicate_hide() {
        assert!(transition(LifecycleState::Suspended, LifecycleEvent::Hide).is_err());
    }

    #[test]
    fn cli_defaults_to_interactive_hotkey_mode() {
        let config = ProbeConfig::parse(Vec::<String>::new()).unwrap();
        assert_eq!(config.cycles, 0);
        assert_eq!(config.visible_wait_ms, 5_000);
        assert_eq!(config.hidden_wait_ms, 3_000);
        assert!(config.focus);
        assert!(config.start_visible);
    }

    #[test]
    fn cli_parses_endurance_overrides() {
        let config = ProbeConfig::parse(
            [
                "--cycles",
                "100",
                "--visible-wait-ms",
                "40",
                "--hidden-wait-ms",
                "40",
                "--focus",
                "off",
                "--start",
                "hidden",
            ]
            .map(str::to_owned),
        )
        .unwrap();
        assert_eq!(config.cycles, 100);
        assert_eq!(config.visible_wait_ms, 40);
        assert_eq!(config.hidden_wait_ms, 40);
        assert!(!config.focus);
        assert!(!config.start_visible);
    }

    #[test]
    fn cli_rejects_unknown_options_and_values() {
        assert!(ProbeConfig::parse(["--focus".to_owned(), "maybe".to_owned()]).is_err());
        assert!(ProbeConfig::parse(["--start".to_owned(), "tray".to_owned()]).is_err());
        assert!(ProbeConfig::parse(["--cycles".to_owned()]).is_err());
        assert!(ProbeConfig::parse(["--wat".to_owned(), "1".to_owned()]).is_err());
    }
}
