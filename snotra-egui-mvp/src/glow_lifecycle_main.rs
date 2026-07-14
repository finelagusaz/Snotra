use std::{
    ffi::CString,
    num::NonZeroU32,
    sync::OnceLock,
    time::{Duration, Instant},
};

use eframe::egui;
use egui_glow::glow::HasContext as _;
use glutin::{
    config::{Config as GlConfig, ConfigTemplateBuilder},
    context::{ContextApi, ContextAttributesBuilder, PossiblyCurrentContext},
    display::{GetGlDisplay as _, GlDisplay as _},
    prelude::*,
    surface::{Surface, SwapInterval, WindowSurface},
};
use glutin_winit::{ApiPreference, DisplayBuilder, GlWindow as _};
use raw_window_handle::HasWindowHandle as _;
use snotra_core::{
    config::Config, engine::Engine, history::HistoryStore, indexer::AppEntry,
    ui_types::SearchResult,
};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowAttributes, WindowId},
};

const ENTRY_COUNT: usize = 10_000;
const FONT_PATH: &str = "C:/Windows/Fonts/YuGothM.ttc";
const WINDOW_TITLE: &str = "Snotra glow lifecycle MVP";

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

    fn has_window_or_surface(self) -> bool {
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
        (LifecycleState::Visible, LifecycleEvent::Exit) => Ok(LifecycleState::Exiting),
        _ => Err(format!(
            "invalid lifecycle transition: {state:?} + {event:?}"
        )),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EguiStateMode {
    Recreate,
    Reuse,
    Skip,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UiMode {
    Search,
    LabelOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FrameMode {
    Full,
    PaintNoSwap,
    RunOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GpuDrainMode {
    None,
    Finish,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VsyncMode {
    Wait,
    Off,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContextMode {
    Reuse,
    SharedChild,
    ParkWindow,
    ParkSurface,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProbeConfig {
    visible_wait_ms: u64,
    hidden_wait_ms: u64,
    cycles: usize,
    egui_state: EguiStateMode,
    ui: UiMode,
    frame: FrameMode,
    gpu_drain: GpuDrainMode,
    vsync: VsyncMode,
    context: ContextMode,
}

impl ProbeConfig {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut config = Self {
            visible_wait_ms: 5_000,
            hidden_wait_ms: 3_000,
            cycles: 3,
            egui_state: EguiStateMode::Reuse,
            ui: UiMode::Search,
            frame: FrameMode::Full,
            gpu_drain: GpuDrainMode::None,
            vsync: VsyncMode::Wait,
            context: ContextMode::ParkSurface,
        };
        let mut args = args.into_iter();
        while let Some(option) = args.next() {
            let value = args
                .next()
                .ok_or_else(|| format!("{option} requires a value"))?;
            match option.as_str() {
                "--visible-wait-ms" => config.visible_wait_ms = parse_u64(&option, &value)?,
                "--hidden-wait-ms" => config.hidden_wait_ms = parse_u64(&option, &value)?,
                "--cycles" => {
                    config.cycles = value
                        .parse()
                        .map_err(|_| format!("{option} requires an integer: {value}"))?;
                    if config.cycles == 0 {
                        return Err("--cycles must be greater than zero".to_owned());
                    }
                }
                "--egui-state" => {
                    config.egui_state = match value.as_str() {
                        "recreate" => EguiStateMode::Recreate,
                        "reuse" => EguiStateMode::Reuse,
                        "skip" => EguiStateMode::Skip,
                        unknown => return Err(format!("unknown egui state mode: {unknown}")),
                    };
                }
                "--ui" => {
                    config.ui = match value.as_str() {
                        "search" => UiMode::Search,
                        "label-only" => UiMode::LabelOnly,
                        unknown => return Err(format!("unknown UI mode: {unknown}")),
                    };
                }
                "--frame" => {
                    config.frame = match value.as_str() {
                        "full" => FrameMode::Full,
                        "paint-no-swap" => FrameMode::PaintNoSwap,
                        "run-only" => FrameMode::RunOnly,
                        unknown => return Err(format!("unknown frame mode: {unknown}")),
                    };
                }
                "--gpu-drain" => {
                    config.gpu_drain = match value.as_str() {
                        "none" => GpuDrainMode::None,
                        "finish" => GpuDrainMode::Finish,
                        unknown => return Err(format!("unknown GPU drain mode: {unknown}")),
                    };
                }
                "--vsync" => {
                    config.vsync = match value.as_str() {
                        "wait" => VsyncMode::Wait,
                        "off" => VsyncMode::Off,
                        unknown => return Err(format!("unknown VSync mode: {unknown}")),
                    };
                }
                "--context" => {
                    config.context = match value.as_str() {
                        "reuse" => ContextMode::Reuse,
                        "shared-child" => ContextMode::SharedChild,
                        "park-window" => ContextMode::ParkWindow,
                        "park-surface" => ContextMode::ParkSurface,
                        unknown => return Err(format!("unknown context mode: {unknown}")),
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

#[derive(Clone, Copy, Debug)]
enum ScheduledAction {
    Hide,
    Show,
    Exit,
}

struct GlResources {
    config: GlConfig,
    current_context: Option<PossiblyCurrentContext>,
    master_context: Option<PossiblyCurrentContext>,
    master_surface: Option<Surface<WindowSurface>>,
    master_window: Option<Window>,
    child_context: Option<PossiblyCurrentContext>,
    parked_window: Option<Window>,
    parked_surface: Option<Surface<WindowSurface>>,
    parked_position: Option<PhysicalPosition<i32>>,
    surface: Option<Surface<WindowSurface>>,
    window: Option<Window>,
    egui_glow: egui_glow::EguiGlow,
}

impl GlResources {
    fn active_context(&self) -> Option<&PossiblyCurrentContext> {
        self.child_context
            .as_ref()
            .or(self.current_context.as_ref())
    }
}

struct LifecycleProbe {
    config: ProbeConfig,
    state: LifecycleState,
    gl: Option<GlResources>,
    engine: Engine,
    query: String,
    results: Vec<SearchResult>,
    scheduled: Option<(Instant, ScheduledAction)>,
    show_started: Option<Instant>,
    completed_cycles: usize,
    error: Option<String>,
}

impl LifecycleProbe {
    fn new(config: ProbeConfig) -> Self {
        let engine = build_verification_engine(ENTRY_COUNT);
        let results = engine.recent_history();
        Self {
            config,
            state: LifecycleState::Recreating,
            gl: None,
            engine,
            query: String::new(),
            results,
            scheduled: None,
            show_started: Some(Instant::now()),
            completed_cycles: 0,
            error: None,
        }
    }

    fn initialize(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let started = Instant::now();
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
        let current_context = not_current_context
            .make_current(&surface)
            .map_err(|error| format!("failed to make initial GL context current: {error}"))?;
        set_swap_interval(&surface, &current_context, self.config.vsync);

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
        let font_bytes = configure_static_font(&egui_glow.egui_ctx)?;
        egui_glow.egui_ctx.request_repaint();

        self.show_started = Some(started);
        self.gl = Some(GlResources {
            config,
            current_context: Some(current_context),
            master_context: None,
            master_surface: None,
            master_window: None,
            child_context: None,
            parked_window: None,
            parked_surface: None,
            parked_position: None,
            surface: Some(surface),
            window: Some(window),
            egui_glow,
        });
        eprintln!(
            "SNOTRA_GLOW_LIFECYCLE_INFO font_bytes={font_bytes} entries={ENTRY_COUNT} cycles={} egui_state={:?} ui={:?} frame={:?} gpu_drain={:?} vsync={:?} context={:?}",
            self.config.cycles,
            self.config.egui_state,
            self.config.ui,
            self.config.frame,
            self.config.gpu_drain,
            self.config.vsync,
            self.config.context
        );
        self.request_redraw();
        Ok(())
    }

    fn suspend(&mut self) -> Result<(), String> {
        self.state = transition(self.state, LifecycleEvent::Hide)?;
        if !self.state.keeps_gl_context_and_painter() || self.state.has_window_or_surface() {
            return Err("suspended state violates the resource ownership contract".to_owned());
        }
        let gl = self
            .gl
            .as_mut()
            .ok_or_else(|| "GL resources are missing during suspend".to_owned())?;

        // WGL owns a device context through the surface. Detach the rendering
        // context first, release the surface/DC second, and destroy the HWND last.
        if self.config.gpu_drain == GpuDrainMode::Finish {
            unsafe { gl.egui_glow.painter.gl().finish() };
        }
        match self.config.context {
            ContextMode::Reuse => {
                let current = gl
                    .current_context
                    .as_ref()
                    .ok_or_else(|| "current GL context is missing during suspend".to_owned())?;
                current
                    .make_not_current_in_place()
                    .map_err(|error| format!("failed to detach GL context: {error}"))?;
                gl.surface = None;
                gl.window = None;
            }
            ContextMode::ParkWindow => {
                let current = gl.current_context.as_ref().ok_or_else(|| {
                    "current GL context is missing while parking window".to_owned()
                })?;
                if let Some(window) = gl.window.as_ref() {
                    window.set_visible(false);
                }
                current.make_not_current_in_place().map_err(|error| {
                    format!("failed to detach GL context while parking window: {error}")
                })?;
                gl.surface = None;
                let window = gl
                    .window
                    .take()
                    .ok_or_else(|| "window is missing while parking".to_owned())?;
                gl.parked_position = window.outer_position().ok();
                let _ = window.request_inner_size(PhysicalSize::new(1, 1));
                window.set_outer_position(PhysicalPosition::new(-32_000, -32_000));
                gl.parked_window = Some(window);
            }
            ContextMode::ParkSurface => {
                let current = gl.current_context.as_ref().ok_or_else(|| {
                    "current GL context is missing while parking surface".to_owned()
                })?;
                let surface = gl
                    .surface
                    .as_ref()
                    .ok_or_else(|| "GL surface is missing while parking surface".to_owned())?;
                surface.resize(current, NonZeroU32::MIN, NonZeroU32::MIN);
                if let Some(window) = gl.window.as_ref() {
                    window.set_visible(false);
                }
                current.make_not_current_in_place().map_err(|error| {
                    format!("failed to detach GL context while parking surface: {error}")
                })?;
                gl.parked_surface = gl.surface.take();
                let window = gl
                    .window
                    .take()
                    .ok_or_else(|| "window is missing while parking surface".to_owned())?;
                gl.parked_position = window.outer_position().ok();
                let _ = window.request_inner_size(PhysicalSize::new(1, 1));
                window.set_outer_position(PhysicalPosition::new(-32_000, -32_000));
                gl.parked_window = Some(window);
            }
            ContextMode::SharedChild if gl.master_context.is_none() => {
                let current = gl.current_context.as_ref().ok_or_else(|| {
                    "initial GL context is missing before master promotion".to_owned()
                })?;
                let surface = gl.surface.as_ref().ok_or_else(|| {
                    "initial GL surface is missing before master promotion".to_owned()
                })?;
                surface.resize(current, NonZeroU32::MIN, NonZeroU32::MIN);
                if let Some(window) = gl.window.as_ref() {
                    let _ = window.request_inner_size(PhysicalSize::new(1, 1));
                    window.set_outer_position(PhysicalPosition::new(-32_000, -32_000));
                    window.set_visible(false);
                }
                current.make_not_current_in_place().map_err(|error| {
                    format!("failed to detach promoted master context: {error}")
                })?;
                gl.master_context = gl.current_context.take();
                gl.master_surface = gl.surface.take();
                gl.master_window = gl.window.take();
            }
            ContextMode::SharedChild => {
                let child = gl.child_context.as_ref().ok_or_else(|| {
                    "shared child GL context is missing during suspend".to_owned()
                })?;
                child
                    .make_not_current_in_place()
                    .map_err(|error| format!("failed to detach shared child context: {error}"))?;
                // Delete the child HGLRC before releasing its HDC/HWND. GL
                // objects survive in the master context's share group.
                gl.child_context = None;
                gl.surface = None;
                gl.window = None;
            }
        }
        let _ = trim_working_set();
        self.scheduled = Some((
            Instant::now() + Duration::from_millis(self.config.hidden_wait_ms),
            ScheduledAction::Show,
        ));
        Ok(())
    }

    fn recreate(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        self.state = transition(self.state, LifecycleEvent::Show)?;
        if !self.state.keeps_gl_context_and_painter() || !self.state.has_window_or_surface() {
            return Err("recreating state violates the resource ownership contract".to_owned());
        }
        self.show_started = Some(Instant::now());
        let gl = self
            .gl
            .as_mut()
            .ok_or_else(|| "GL resources are missing during recreation".to_owned())?;
        if gl.window.is_some() || gl.surface.is_some() {
            return Err("window/surface survived the suspended state".to_owned());
        }

        // `glutin::Config` on WGL retains the initial HDC. `finalize_window`
        // re-queries `supports_transparency()` through that stale HDC after the
        // original HWND has gone away. This probe never requests transparency,
        // so create the HWND directly and let `create_window_surface` apply the
        // saved pixel format to the new native window.
        let window = if matches!(
            self.config.context,
            ContextMode::ParkWindow | ContextMode::ParkSurface
        ) {
            let window = gl
                .parked_window
                .take()
                .ok_or_else(|| "parked window is missing during recreation".to_owned())?;
            if let Some(position) = gl.parked_position.take() {
                window.set_outer_position(position);
            }
            let _ = window.request_inner_size(PhysicalSize::new(640, 360));
            window
        } else {
            event_loop
                .create_window(window_attributes())
                .map_err(|error| format!("failed to recreate window: {error}"))?
        };
        let raw_window_handle = window
            .window_handle()
            .map_err(|error| format!("failed to read recreated window handle: {error}"))?
            .as_raw();
        let surface = if self.config.context == ContextMode::ParkSurface {
            gl.parked_surface
                .take()
                .ok_or_else(|| "parked GL surface is missing during recreation".to_owned())?
        } else {
            let surface_attributes = window
                .build_surface_attributes(Default::default())
                .map_err(|error| {
                    format!("failed to build recreated surface attributes: {error}")
                })?;
            unsafe {
                gl.config
                    .display()
                    .create_window_surface(&gl.config, &surface_attributes)
            }
            .map_err(|error| format!("failed to recreate GL surface: {error}"))?
        };
        match self.config.context {
            ContextMode::Reuse | ContextMode::ParkWindow => {
                let current = gl
                    .current_context
                    .as_ref()
                    .ok_or_else(|| "detached GL context is missing during recreation".to_owned())?;
                current
                    .make_current(&surface)
                    .map_err(|error| format!("failed to reattach GL context: {error}"))?;
                set_swap_interval(&surface, current, self.config.vsync);
            }
            ContextMode::ParkSurface => {
                let current = gl.current_context.as_ref().ok_or_else(|| {
                    "detached GL context is missing during parked recreation".to_owned()
                })?;
                current
                    .make_current(&surface)
                    .map_err(|error| format!("failed to reattach parked GL context: {error}"))?;
                surface.resize(
                    current,
                    NonZeroU32::new(640).expect("640 is non-zero"),
                    NonZeroU32::new(360).expect("360 is non-zero"),
                );
            }
            ContextMode::SharedChild => {
                let master = gl.master_context.as_ref().ok_or_else(|| {
                    "master GL context is missing during child creation".to_owned()
                })?;
                let context_attributes = ContextAttributesBuilder::new()
                    .with_sharing(master)
                    .build(Some(raw_window_handle));
                let fallback_context_attributes = ContextAttributesBuilder::new()
                    .with_context_api(ContextApi::Gles(None))
                    .with_sharing(master)
                    .build(Some(raw_window_handle));
                let display = gl.config.display();
                let not_current_child = unsafe {
                    display
                        .create_context(&gl.config, &context_attributes)
                        .or_else(|_| {
                            display.create_context(&gl.config, &fallback_context_attributes)
                        })
                }
                .map_err(|error| format!("failed to create shared child context: {error}"))?;
                let child = not_current_child.make_current(&surface).map_err(|error| {
                    format!("failed to make shared child context current: {error}")
                })?;
                set_swap_interval(&surface, &child, self.config.vsync);
                gl.child_context = Some(child);
            }
        }

        if self.config.egui_state == EguiStateMode::Recreate {
            gl.egui_glow.egui_winit = egui_glow::egui_winit::State::new(
                gl.egui_glow.egui_ctx.clone(),
                egui::ViewportId::ROOT,
                event_loop,
                Some(window.scale_factor() as f32),
                event_loop.system_theme(),
                Some(gl.egui_glow.painter.max_texture_side()),
            );
        }
        gl.surface = Some(surface);
        gl.window = Some(window);
        if matches!(
            self.config.context,
            ContextMode::ParkWindow | ContextMode::ParkSurface
        ) && let Some(window) = gl.window.as_ref()
        {
            // Windows does not guarantee RedrawRequested for a hidden HWND.
            // Reveal before requesting the first frame; timing still ends at
            // the completed buffer swap.
            window.set_visible(true);
        }
        if self.config.egui_state == EguiStateMode::Skip {
            self.state = transition(self.state, LifecycleEvent::FramePresented)?;
            let elapsed = self
                .show_started
                .take()
                .map(|started| started.elapsed())
                .unwrap_or_default();
            log_metric(
                "surface_only_attached",
                self.completed_cycles,
                Some(elapsed),
            );
            self.scheduled = Some((
                Instant::now() + Duration::from_millis(self.config.visible_wait_ms),
                if self.completed_cycles >= self.config.cycles {
                    ScheduledAction::Exit
                } else {
                    ScheduledAction::Hide
                },
            ));
        } else {
            self.request_redraw();
        }
        Ok(())
    }

    fn render(&mut self) -> Result<(), String> {
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
        let current = gl
            .child_context
            .as_ref()
            .or(gl.current_context.as_ref())
            .ok_or_else(|| "active GL context is missing during render".to_owned())?;

        let query = &mut self.query;
        let results = &mut self.results;
        let engine = &mut self.engine;
        let ui_mode = self.config.ui;
        gl.egui_glow.run(window, |ui| {
            ui.heading("Snotra glow lifecycle 検証");
            ui.label("GL Context / Painter / egui Context / Engine は非表示中も保持");
            if ui_mode == UiMode::Search {
                let response = ui.add(
                    egui::TextEdit::singleline(query)
                        .hint_text("アプリ、フォルダー、コマンドを検索"),
                );
                if response.changed() {
                    *results = if query.is_empty() {
                        engine.recent_history()
                    } else {
                        engine.search(query)
                    };
                }
            } else {
                ui.label("IMEを発生させないlabel-only診断");
            }
            ui.separator();
            for result in results.iter().take(8) {
                ui.label(&result.name);
            }
            ui.small(format!(
                "{} results / Engine {ENTRY_COUNT} entries / cycle {} of {}",
                results.len(),
                self.completed_cycles,
                self.config.cycles
            ));
        });
        if self.config.frame != FrameMode::RunOnly || self.completed_cycles == 0 {
            gl.egui_glow.painter.clear(
                window.inner_size().into(),
                egui::Rgba::from_gray(0.96).to_array(),
            );
            gl.egui_glow.paint(window);
        }
        if self.config.frame == FrameMode::Full || self.completed_cycles == 0 {
            surface
                .swap_buffers(current)
                .map_err(|error| format!("failed to present GL frame: {error}"))?;
        }
        window.set_visible(true);

        if self.state == LifecycleState::Recreating {
            self.state = transition(self.state, LifecycleEvent::FramePresented)?;
            let elapsed = self
                .show_started
                .take()
                .map(|started| started.elapsed())
                .unwrap_or_default();
            if self.completed_cycles == 0 {
                log_metric("initial_frame_presented", 0, Some(elapsed));
            } else {
                log_metric("warm_frame_presented", self.completed_cycles, Some(elapsed));
            }
            self.scheduled = Some((
                Instant::now() + Duration::from_millis(self.config.visible_wait_ms),
                if self.completed_cycles >= self.config.cycles {
                    ScheduledAction::Exit
                } else {
                    ScheduledAction::Hide
                },
            ));
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
                self.suspend()
            }
            ScheduledAction::Show => {
                log_metric("hidden_window_dropped", self.completed_cycles, None);
                self.completed_cycles += 1;
                self.recreate(event_loop)
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
            if !gl
                .active_context()
                .is_some_and(|context| context.is_current())
            {
                return Err(
                    "refusing to destroy Painter while GL context is not current".to_owned(),
                );
            }
            gl.egui_glow.destroy();
        }
        event_loop.exit();
        Ok(())
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: String) {
        eprintln!("SNOTRA_GLOW_LIFECYCLE_ERROR {error}");
        self.error = Some(error);
        event_loop.exit();
    }

    fn request_redraw(&self) {
        if let Some(window) = self.gl.as_ref().and_then(|gl| gl.window.as_ref()) {
            window.request_redraw();
        }
    }
}

impl ApplicationHandler for LifecycleProbe {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gl.is_none()
            && let Err(error) = self.initialize(event_loop)
        {
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
        let result = match event {
            WindowEvent::CloseRequested => self.finish(event_loop),
            WindowEvent::RedrawRequested
                if self.config.egui_state == EguiStateMode::Skip && self.completed_cycles > 0 =>
            {
                Ok(())
            }
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::Resized(size) if size.width > 0 && size.height > 0 => {
                if let Some(gl) = self.gl.as_mut()
                    && let (Some(surface), Some(context)) =
                        (gl.surface.as_ref(), gl.current_context.as_ref())
                {
                    surface.resize(
                        context,
                        NonZeroU32::new(size.width).unwrap_or(NonZeroU32::MIN),
                        NonZeroU32::new(size.height).unwrap_or(NonZeroU32::MIN),
                    );
                }
                self.request_redraw();
                Ok(())
            }
            _ => Ok(()),
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
        // winit enables OLE file drop for every HWND by default. This search
        // window never accepts files, so avoid the unused OLE/drop-target fixed
        // resources while comparing lifecycle strategies.
        .with_drag_and_drop(false)
        // A hidden Windows window does not guarantee delivery of a requested
        // redraw. The measured endpoint is the completed buffer swap, so the
        // probe creates the window visible and keeps that timing definition.
        .with_visible(true)
}

fn set_swap_interval(
    surface: &Surface<WindowSurface>,
    context: &PossiblyCurrentContext,
    vsync: VsyncMode,
) {
    let interval = match vsync {
        VsyncMode::Wait => SwapInterval::Wait(NonZeroU32::MIN),
        VsyncMode::Off => SwapInterval::DontWait,
    };
    if let Err(error) = surface.set_swap_interval(context, interval) {
        eprintln!("SNOTRA_GLOW_LIFECYCLE_WARNING failed_to_set_swap_interval={error}");
    }
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
        "snotra-egui-glow-lifecycle-mvp-history-{}-unused",
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

fn log_metric(checkpoint: &str, cycle: usize, elapsed: Option<Duration>) {
    let snapshot = process_snapshot();
    let elapsed_ms = elapsed
        .map(|duration| duration.as_secs_f64() * 1_000.0)
        .unwrap_or(-1.0);
    eprintln!(
        "SNOTRA_GLOW_LIFECYCLE_METRIC checkpoint={checkpoint} cycle={cycle} elapsed_ms={elapsed_ms:.3} working_set_bytes={} private_bytes={} handles={} gdi={} user={} windows={} visible_windows={}",
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

fn run() -> Result<(), String> {
    let config = ProbeConfig::parse(std::env::args().skip(1))?;
    let event_loop =
        EventLoop::new().map_err(|error| format!("event loop creation failed: {error}"))?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = LifecycleProbe::new(config);
    event_loop
        .run_app(&mut app)
        .map_err(|error| format!("event loop failed: {error}"))?;
    if let Some(error) = app.error {
        Err(error)
    } else {
        Ok(())
    }
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LifecycleEvent, LifecycleState, ProbeConfig, transition};

    #[test]
    fn lifecycle_releases_only_window_and_surface_while_hidden() {
        let hidden = transition(LifecycleState::Visible, LifecycleEvent::Hide).unwrap();
        assert_eq!(hidden, LifecycleState::Suspended);
        assert!(hidden.keeps_gl_context_and_painter());
        assert!(!hidden.has_window_or_surface());
    }

    #[test]
    fn lifecycle_recreates_surface_before_first_warm_frame() {
        let recreating = transition(LifecycleState::Suspended, LifecycleEvent::Show).unwrap();
        assert_eq!(recreating, LifecycleState::Recreating);
        let visible = transition(recreating, LifecycleEvent::FramePresented).unwrap();
        assert_eq!(visible, LifecycleState::Visible);
    }

    #[test]
    fn lifecycle_rejects_duplicate_hide() {
        assert!(transition(LifecycleState::Suspended, LifecycleEvent::Hide).is_err());
    }

    #[test]
    fn cli_defaults_measure_three_complete_cycles() {
        let config = ProbeConfig::parse(Vec::<String>::new()).unwrap();
        assert_eq!(config.visible_wait_ms, 5_000);
        assert_eq!(config.hidden_wait_ms, 3_000);
        assert_eq!(config.cycles, 3);
        assert_eq!(config.context, super::ContextMode::ParkSurface);
        assert_eq!(config.egui_state, super::EguiStateMode::Reuse);
        assert_eq!(config.frame, super::FrameMode::Full);
        assert_eq!(config.vsync, super::VsyncMode::Wait);
    }

    #[test]
    fn cli_rejects_zero_cycles() {
        assert!(ProbeConfig::parse(["--cycles".to_owned(), "0".to_owned()]).is_err());
    }

    #[test]
    fn cli_can_reuse_egui_state_for_component_isolation() {
        let config = ProbeConfig::parse(["--egui-state".to_owned(), "reuse".to_owned()]).unwrap();
        assert_eq!(config.egui_state, super::EguiStateMode::Reuse);
    }

    #[test]
    fn cli_can_skip_egui_after_first_frame_for_surface_isolation() {
        let config = ProbeConfig::parse(["--egui-state".to_owned(), "skip".to_owned()]).unwrap();
        assert_eq!(config.egui_state, super::EguiStateMode::Skip);
    }

    #[test]
    fn cli_can_remove_text_edit_for_ime_isolation() {
        let config = ProbeConfig::parse(["--ui".to_owned(), "label-only".to_owned()]).unwrap();
        assert_eq!(config.ui, super::UiMode::LabelOnly);
    }

    #[test]
    fn cli_can_skip_paint_and_swap_for_output_isolation() {
        let config = ProbeConfig::parse(["--frame".to_owned(), "run-only".to_owned()]).unwrap();
        assert_eq!(config.frame, super::FrameMode::RunOnly);
    }

    #[test]
    fn cli_can_paint_without_swap_for_present_isolation() {
        let config =
            ProbeConfig::parse(["--frame".to_owned(), "paint-no-swap".to_owned()]).unwrap();
        assert_eq!(config.frame, super::FrameMode::PaintNoSwap);
    }

    #[test]
    fn cli_can_drain_gpu_before_surface_drop() {
        let config = ProbeConfig::parse(["--gpu-drain".to_owned(), "finish".to_owned()]).unwrap();
        assert_eq!(config.gpu_drain, super::GpuDrainMode::Finish);
    }

    #[test]
    fn cli_can_disable_vsync_for_swap_isolation() {
        let config = ProbeConfig::parse(["--vsync".to_owned(), "off".to_owned()]).unwrap();
        assert_eq!(config.vsync, super::VsyncMode::Off);
    }

    #[test]
    fn cli_can_use_disposable_shared_child_contexts() {
        let config =
            ProbeConfig::parse(["--context".to_owned(), "shared-child".to_owned()]).unwrap();
        assert_eq!(config.context, super::ContextMode::SharedChild);
    }

    #[test]
    fn cli_can_park_one_hwnd_while_surface_is_dropped() {
        let config =
            ProbeConfig::parse(["--context".to_owned(), "park-window".to_owned()]).unwrap();
        assert_eq!(config.context, super::ContextMode::ParkWindow);
    }

    #[test]
    fn cli_can_park_one_hwnd_and_surface_at_one_pixel() {
        let config =
            ProbeConfig::parse(["--context".to_owned(), "park-surface".to_owned()]).unwrap();
        assert_eq!(config.context, super::ContextMode::ParkSurface);
    }
}
