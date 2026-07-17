#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Issue #532 Tauri host + softbuffer 統合スパイク。
//!
//! glow_park_host_main と同じホスト骨格（Tauri managed host: updater / global
//! shortcut Alt+Q をメインスレッド、UI を専用 winit スレッド）に、GL/wgpu を
//! 持たない softbuffer CPU レンダラーを載せる。GL context の park/unpark 管理は
//! 丸ごと不要になり、hide/show は `set_visible` + working set trim だけになる。
//!
//! - コールドスタート内訳と統合時の床（private bytes）
//! - Alt+Q → show → focus → Alt 解除 → warm フレーム
//! - 反復耐久（handle / GDI / USER / private bytes）
//! - Tauri Updater との共存

use std::{
    collections::HashMap,
    num::NonZeroU32,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use eframe::egui;
use egui_glow::egui_winit;
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
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::{Window, WindowAttributes, WindowId},
};

const ENTRY_COUNT: usize = 10_000;
const FONT_PATH: &str = "C:/Windows/Fonts/YuGothM.ttc";
const WINDOW_TITLE: &str = "Snotra softbuffer host MVP";
const CLEAR_COLOR: u32 = 0x00F5_F5F5;
const ALT_RELEASE_POLL_MS: u64 = 10;
const ALT_RELEASE_TIMEOUT_MS: u64 = 350;

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProbeConfig {
    /// 0 は対話モード（Alt+Q 駆動のみ）。1 以上で自動 hide/show 反復。
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
// ホスト側ホットキー判定と UI コマンド計画（glow_park_host_main と同型）
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LifecycleState {
    Visible,
    Suspended,
    /// show 済みで最初のフレーム提示を待っている。
    Recreating,
    Exiting,
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
    Show,
    Hide,
    Refocus,
    /// show 完了（FramePresented）後まで Hide を繰り延べる。
    Defer,
    Ignore,
}

/// ホストコマンドを現在状態へ適用する計画。冪等性（Visible+Show / Suspended+Hide）
/// と、show 進行中に届いた Hide の繰り延べをここで一元的に決める。
fn plan_ui_action(state: LifecycleState, command: &HostCommand) -> UiAction {
    match (state, command) {
        (LifecycleState::Visible, HostCommand::Show { .. }) => UiAction::Refocus,
        (LifecycleState::Visible, HostCommand::Hide) => UiAction::Hide,
        (LifecycleState::Suspended, HostCommand::Show { .. }) => UiAction::Show,
        (LifecycleState::Suspended, HostCommand::Hide) => UiAction::Ignore,
        (LifecycleState::Recreating, HostCommand::Show { .. }) => UiAction::Ignore,
        (LifecycleState::Recreating, HostCommand::Hide) => UiAction::Defer,
        (LifecycleState::Exiting, _) => UiAction::Ignore,
    }
}

// ---------------------------------------------------------------------------
// CPU ラスタライザの中核（soft_main と同型・テストで固定）
// ---------------------------------------------------------------------------

/// 2D エッジ関数（外積）。正なら点 p は a→b の左側にある。
fn edge(ax: f32, ay: f32, bx: f32, by: f32, px: f32, py: f32) -> f32 {
    (bx - ax) * (py - ay) - (by - ay) * (px - ax)
}

/// premultiplied sRGB 同士の over 合成。dst は 0x00RRGGBB、src は [r,g,b,a]。
fn blend_premultiplied(dst: u32, src: [u8; 4]) -> u32 {
    let inverse = 255 - src[3] as u32;
    let dst_r = (dst >> 16) & 0xFF;
    let dst_g = (dst >> 8) & 0xFF;
    let dst_b = dst & 0xFF;
    let r = (src[0] as u32 + dst_r * inverse / 255).min(255);
    let g = (src[1] as u32 + dst_g * inverse / 255).min(255);
    let b = (src[2] as u32 + dst_b * inverse / 255).min(255);
    (r << 16) | (g << 8) | b
}

/// 頂点色 × テクスチャの変調。どちらも premultiplied で、(c*t + 127) / 255。
fn modulate(color: [u8; 4], texel: [u8; 4]) -> [u8; 4] {
    let channel = |c: u8, t: u8| ((c as u16 * t as u16 + 127) / 255) as u8;
    [
        channel(color[0], texel[0]),
        channel(color[1], texel[1]),
        channel(color[2], texel[2]),
        channel(color[3], texel[3]),
    ]
}

struct CpuTexture {
    width: usize,
    height: usize,
    /// premultiplied sRGB RGBA。
    pixels: Vec<[u8; 4]>,
}

impl CpuTexture {
    fn sample_nearest(&self, u: f32, v: f32) -> [u8; 4] {
        if self.width == 0 || self.height == 0 {
            return [255, 255, 255, 255];
        }
        let x = ((u * self.width as f32) as isize).clamp(0, self.width as isize - 1) as usize;
        let y = ((v * self.height as f32) as isize).clamp(0, self.height as isize - 1) as usize;
        self.pixels[y * self.width + x]
    }
}

/// 1 つの epaint Mesh を framebuffer へ描く。pos は物理ピクセル座標へ変換済みであること。
#[allow(clippy::too_many_arguments)]
fn fill_mesh(
    buffer: &mut [u32],
    width: usize,
    height: usize,
    vertices: &[egui::epaint::Vertex],
    indices: &[u32],
    texture: &CpuTexture,
    clip_min: (usize, usize),
    clip_max: (usize, usize),
    pixels_per_point: f32,
) {
    for triangle in indices.chunks_exact(3) {
        let v0 = &vertices[triangle[0] as usize];
        let v1 = &vertices[triangle[1] as usize];
        let v2 = &vertices[triangle[2] as usize];
        let (x0, y0) = (v0.pos.x * pixels_per_point, v0.pos.y * pixels_per_point);
        let (x1, y1) = (v1.pos.x * pixels_per_point, v1.pos.y * pixels_per_point);
        let (x2, y2) = (v2.pos.x * pixels_per_point, v2.pos.y * pixels_per_point);
        let area = edge(x0, y0, x1, y1, x2, y2);
        if area.abs() < f32::EPSILON {
            continue;
        }
        let min_x = x0.min(x1).min(x2).floor().max(clip_min.0 as f32) as usize;
        let min_y = y0.min(y1).min(y2).floor().max(clip_min.1 as f32) as usize;
        let max_x = (x0.max(x1).max(x2).ceil() as usize).min(clip_max.0).min(width);
        let max_y = (y0.max(y1).max(y2).ceil() as usize).min(clip_max.1).min(height);
        for y in min_y..max_y {
            let py = y as f32 + 0.5;
            for x in min_x..max_x {
                let px = x as f32 + 0.5;
                let w0 = edge(x1, y1, x2, y2, px, py);
                let w1 = edge(x2, y2, x0, y0, px, py);
                let w2 = edge(x0, y0, x1, y1, px, py);
                let inside = if area > 0.0 {
                    w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0
                } else {
                    w0 <= 0.0 && w1 <= 0.0 && w2 <= 0.0
                };
                if !inside {
                    continue;
                }
                let (b0, b1, b2) = (w0 / area, w1 / area, w2 / area);
                let u = v0.uv.x * b0 + v1.uv.x * b1 + v2.uv.x * b2;
                let v = v0.uv.y * b0 + v1.uv.y * b1 + v2.uv.y * b2;
                let c0 = v0.color.to_array();
                let c1 = v1.color.to_array();
                let c2 = v2.color.to_array();
                let color = [
                    (c0[0] as f32 * b0 + c1[0] as f32 * b1 + c2[0] as f32 * b2) as u8,
                    (c0[1] as f32 * b0 + c1[1] as f32 * b1 + c2[1] as f32 * b2) as u8,
                    (c0[2] as f32 * b0 + c1[2] as f32 * b1 + c2[2] as f32 * b2) as u8,
                    (c0[3] as f32 * b0 + c1[3] as f32 * b1 + c2[3] as f32 * b2) as u8,
                ];
                let src = modulate(color, texture.sample_nearest(u, v));
                if src[3] == 0 && src[0] == 0 && src[1] == 0 && src[2] == 0 {
                    continue;
                }
                let index = y * width + x;
                buffer[index] = blend_premultiplied(buffer[index], src);
            }
        }
    }
}

fn image_to_pixels(image: &egui::epaint::image::ImageData) -> (usize, usize, Vec<[u8; 4]>) {
    match image {
        egui::epaint::image::ImageData::Color(color) => (
            color.size[0],
            color.size[1],
            color.pixels.iter().map(|c| c.to_array()).collect(),
        ),
    }
}

fn apply_texture_delta(
    textures: &mut HashMap<egui::TextureId, CpuTexture>,
    id: egui::TextureId,
    delta: &egui::epaint::image::ImageDelta,
) {
    let (width, height, pixels) = image_to_pixels(&delta.image);
    match delta.pos {
        None => {
            textures.insert(
                id,
                CpuTexture {
                    width,
                    height,
                    pixels,
                },
            );
        }
        Some([x, y]) => {
            if let Some(existing) = textures.get_mut(&id) {
                for row in 0..height {
                    for column in 0..width {
                        let dst_x = x + column;
                        let dst_y = y + row;
                        if dst_x < existing.width && dst_y < existing.height {
                            existing.pixels[dst_y * existing.width + dst_x] =
                                pixels[row * width + column];
                        }
                    }
                }
            }
        }
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

struct SoftHostProbe {
    config: ProbeConfig,
    state: LifecycleState,
    engine: Engine,
    query: String,
    results: Vec<SearchResult>,
    window: Option<Arc<Window>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    egui_ctx: egui::Context,
    egui_winit: Option<egui_winit::State>,
    textures: HashMap<egui::TextureId, CpuTexture>,
    scheduled: Option<(Instant, ScheduledAction)>,
    show_started: Option<Instant>,
    hotkey_started: Option<Instant>,
    pending_hide: bool,
    focus_search: bool,
    completed_cycles: usize,
    warm_times_ms: Vec<f64>,
    raster_times_ms: Vec<f64>,
    started_at: Instant,
    visible_flag: Arc<AtomicBool>,
    error: Option<String>,
}

impl SoftHostProbe {
    fn new(config: ProbeConfig, visible_flag: Arc<AtomicBool>, started_at: Instant) -> Self {
        let engine_started = Instant::now();
        let engine = build_verification_engine(ENTRY_COUNT);
        log_phase(started_at, "engine_ready");
        eprintln!(
            "SNOTRA_SOFT_HOST_INFO engine_build_ms={:.3} entries={ENTRY_COUNT}",
            engine_started.elapsed().as_secs_f64() * 1000.0
        );
        let results = engine.recent_history();
        Self {
            config,
            state: LifecycleState::Recreating,
            engine,
            query: String::new(),
            results,
            window: None,
            surface: None,
            egui_ctx: egui::Context::default(),
            egui_winit: None,
            textures: HashMap::new(),
            scheduled: None,
            show_started: Some(started_at),
            hotkey_started: None,
            pending_hide: false,
            focus_search: true,
            completed_cycles: 0,
            warm_times_ms: Vec::new(),
            raster_times_ms: Vec::new(),
            started_at,
            visible_flag,
            error: None,
        }
    }

    fn initialize(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let window = Arc::new(
            event_loop
                .create_window(window_attributes())
                .map_err(|error| format!("failed to create window: {error}"))?,
        );
        log_phase(self.started_at, "window_ready");
        let context = softbuffer::Context::new(Arc::clone(&window))
            .map_err(|error| format!("failed to create softbuffer context: {error}"))?;
        let surface = softbuffer::Surface::new(&context, Arc::clone(&window))
            .map_err(|error| format!("failed to create softbuffer surface: {error}"))?;
        log_phase(self.started_at, "softbuffer_ready");
        let font_bytes = configure_static_font(&self.egui_ctx)?;
        log_phase(self.started_at, "font_configured");
        eprintln!("SNOTRA_SOFT_HOST_INFO font_bytes={font_bytes}");
        let egui_winit = egui_winit::State::new(
            self.egui_ctx.clone(),
            egui::ViewportId::ROOT,
            event_loop,
            Some(window.scale_factor() as f32),
            event_loop.system_theme(),
            None,
        );
        log_phase(self.started_at, "egui_ready");
        self.window = Some(Arc::clone(&window));
        self.surface = Some(surface);
        self.egui_winit = Some(egui_winit);
        window.request_redraw();
        Ok(())
    }

    fn hide(&mut self) -> Result<(), String> {
        self.state = transition(self.state, LifecycleEvent::Hide)?;
        let window = self
            .window
            .as_ref()
            .ok_or_else(|| "window is missing during hide".to_owned())?;
        window.set_visible(false);
        self.visible_flag.store(false, Ordering::SeqCst);
        let _ = trim_working_set();
        log_metric("hidden_settled", self.completed_cycles, None);
        if self.config.cycles > 0 {
            self.scheduled = Some((
                Instant::now() + Duration::from_millis(self.config.hidden_wait_ms),
                ScheduledAction::Show,
            ));
        }
        Ok(())
    }

    fn show(&mut self, hotkey_started: Option<Instant>) -> Result<(), String> {
        self.state = transition(self.state, LifecycleEvent::Show)?;
        self.show_started = Some(Instant::now());
        self.hotkey_started = hotkey_started;
        let window = self
            .window
            .as_ref()
            .ok_or_else(|| "window is missing during show".to_owned())?;
        window.set_visible(true);
        self.visible_flag.store(true, Ordering::SeqCst);
        self.focus_search = true;
        if self.config.focus {
            self.spawn_focus_sequence(hotkey_started)?;
        }
        window.request_redraw();
        Ok(())
    }

    /// 製品版と同じ「show → focus → WM_NULL 待ち → 残留 Alt 解除」を、UI スレッドを
    /// ブロックしない helper スレッドで実行する。
    fn spawn_focus_sequence(&self, hotkey_started: Option<Instant>) -> Result<(), String> {
        let hwnd = self.active_hwnd()?;
        thread::Builder::new()
            .name("snotra-soft-host-focus".to_owned())
            .spawn(move || {
                focus_and_release_alt(hwnd);
                if let Some(started) = hotkey_started {
                    eprintln!(
                        "SNOTRA_SOFT_HOST_HOTKEY=input_ready elapsed_ms={:.3}",
                        started.elapsed().as_secs_f64() * 1000.0
                    );
                }
            })
            .map_err(|error| format!("failed to spawn focus sequence: {error}"))?;
        Ok(())
    }

    fn active_hwnd(&self) -> Result<isize, String> {
        let window = self
            .window
            .as_ref()
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
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
        Ok(())
    }

    fn apply_command(&mut self, command: HostCommand) -> Result<(), String> {
        if self.window.is_none() {
            eprintln!("SNOTRA_SOFT_HOST_WARNING command_before_initialization={command:?}");
            return Ok(());
        }
        match plan_ui_action(self.state, &command) {
            UiAction::Show => {
                let hotkey_started = match command {
                    HostCommand::Show { hotkey_started } => Some(hotkey_started),
                    HostCommand::Hide => None,
                };
                self.show(hotkey_started)
            }
            UiAction::Hide => self.hide(),
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
        if self.state == LifecycleState::Suspended || self.state == LifecycleState::Exiting {
            return Ok(());
        }
        let window = self
            .window
            .as_ref()
            .ok_or_else(|| "window is missing during render".to_owned())?
            .clone();
        let egui_winit = self
            .egui_winit
            .as_mut()
            .ok_or_else(|| "egui state is missing during render".to_owned())?;
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }

        let raw_input = egui_winit.take_egui_input(&window);
        let query = &mut self.query;
        let results = &mut self.results;
        let engine = &mut self.engine;
        let focus_search = &mut self.focus_search;
        let full_output = self.egui_ctx.run_ui(raw_input, |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(egui::Color32::from_rgb(245, 245, 245)))
                .show(ctx, |ui| {
                    ui.heading("Snotra softbuffer 統合スパイク");
                    ui.label("Tauri host（updater / Alt+Q）+ CPU ラスタ + GDI 転送");
                    let response = ui.add(
                        egui::TextEdit::singleline(query)
                            .hint_text("アプリ、フォルダー、コマンドを検索"),
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
        });
        egui_winit.handle_platform_output(&window, full_output.platform_output);
        for (id, delta) in &full_output.textures_delta.set {
            apply_texture_delta(&mut self.textures, *id, delta);
        }
        let clipped = self
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);

        let surface = self
            .surface
            .as_mut()
            .ok_or_else(|| "softbuffer surface is missing during render".to_owned())?;
        let (width, height) = (size.width as usize, size.height as usize);
        surface
            .resize(
                NonZeroU32::new(size.width).expect("width was checked to be non-zero"),
                NonZeroU32::new(size.height).expect("height was checked to be non-zero"),
            )
            .map_err(|error| format!("failed to resize softbuffer surface: {error}"))?;
        let mut buffer = surface
            .buffer_mut()
            .map_err(|error| format!("failed to borrow softbuffer frame: {error}"))?;
        let raster_started = Instant::now();
        buffer.fill(CLEAR_COLOR);
        let white = CpuTexture {
            width: 0,
            height: 0,
            pixels: Vec::new(),
        };
        let ppp = full_output.pixels_per_point;
        for primitive in &clipped {
            let egui::epaint::Primitive::Mesh(mesh) = &primitive.primitive else {
                continue;
            };
            let texture = self.textures.get(&mesh.texture_id).unwrap_or(&white);
            let clip = primitive.clip_rect;
            let clip_min = (
                (clip.min.x * ppp).floor().max(0.0) as usize,
                (clip.min.y * ppp).floor().max(0.0) as usize,
            );
            let clip_max = (
                (clip.max.x * ppp).ceil().min(width as f32) as usize,
                (clip.max.y * ppp).ceil().min(height as f32) as usize,
            );
            fill_mesh(
                &mut buffer,
                width,
                height,
                &mesh.vertices,
                &mesh.indices,
                texture,
                clip_min,
                clip_max,
                ppp,
            );
        }
        let raster_ms = raster_started.elapsed().as_secs_f64() * 1000.0;
        self.raster_times_ms.push(raster_ms);
        buffer
            .present()
            .map_err(|error| format!("failed to present softbuffer frame: {error}"))?;
        for id in &full_output.textures_delta.free {
            self.textures.remove(id);
        }

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
                eprintln!("SNOTRA_SOFT_HOST_INFO first_raster_ms={raster_ms:.3}");
            } else {
                self.warm_times_ms.push(elapsed.as_secs_f64() * 1000.0);
                log_metric("warm_frame_presented", self.completed_cycles, Some(elapsed));
            }
            if let Some(started) = self.hotkey_started.take() {
                eprintln!(
                    "SNOTRA_SOFT_HOST_HOTKEY=warm_frame_after_hotkey elapsed_ms={:.3}",
                    started.elapsed().as_secs_f64() * 1000.0
                );
            }
            if self.pending_hide {
                self.pending_hide = false;
                log_metric("visible_settled", self.completed_cycles, None);
                return self.hide();
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
                self.hide()
            }
            ScheduledAction::Show => {
                self.completed_cycles += 1;
                self.show(None)
            }
            ScheduledAction::Exit => {
                log_metric("final_visible_settled", self.completed_cycles, None);
                self.finish(event_loop)
            }
        }
    }

    fn finish(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        self.state = transition(self.state, LifecycleEvent::Exit)?;
        if !self.warm_times_ms.is_empty() {
            let count = self.warm_times_ms.len();
            let sum: f64 = self.warm_times_ms.iter().sum();
            let min = self.warm_times_ms.iter().cloned().fold(f64::MAX, f64::min);
            let max = self.warm_times_ms.iter().cloned().fold(f64::MIN, f64::max);
            eprintln!(
                "SNOTRA_SOFT_HOST_SUMMARY warm_frames={count} warm_ms_min={min:.3} warm_ms_avg={:.3} warm_ms_max={max:.3}",
                sum / count as f64
            );
        }
        if !self.raster_times_ms.is_empty() {
            let count = self.raster_times_ms.len();
            let sum: f64 = self.raster_times_ms.iter().sum();
            let max = self
                .raster_times_ms
                .iter()
                .cloned()
                .fold(f64::MIN, f64::max);
            eprintln!(
                "SNOTRA_SOFT_HOST_SUMMARY raster_frames={count} raster_ms_avg={:.3} raster_ms_max={max:.3}",
                sum / count as f64
            );
        }
        event_loop.exit();
        Ok(())
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: String) {
        eprintln!("SNOTRA_SOFT_HOST_ERROR {error}");
        self.error = Some(error);
        event_loop.exit();
    }
}

impl ApplicationHandler<HostCommand> for SoftHostProbe {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            match self.initialize(event_loop) {
                Ok(()) => {
                    if !self.config.start_visible {
                        // 初回フレーム提示後に hide する（tray 常駐相当の開始状態）。
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
        let Some(window) = self.window.as_ref().map(Arc::clone) else {
            return;
        };
        if window.id() != window_id {
            return;
        }
        // RedrawRequested を egui_winit へ渡すと repaint 応答が再描画要求を生み、
        // 描画が自己永続ループになる。入力イベントだけを egui へ渡す。
        if !matches!(event, WindowEvent::RedrawRequested)
            && let Some(egui_winit) = self.egui_winit.as_mut()
        {
            let response = egui_winit.on_window_event(&window, &event);
            if response.repaint && self.state == LifecycleState::Visible {
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
                self.hide()
            } else {
                Ok(())
            }
        } else {
            match event {
                WindowEvent::CloseRequested => self.finish(event_loop),
                WindowEvent::RedrawRequested => self.render(),
                WindowEvent::Resized(size)
                    if size.width > 0
                        && size.height > 0
                        && self.state == LifecycleState::Visible =>
                {
                    window.request_redraw();
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
        // 可視状態で提示し、--start hidden は初回フレーム後に hide する。
        .with_visible(true)
}

// ---------------------------------------------------------------------------
// Windows 統合（Alt 検出・フォーカス・Alt 解除。glow_park_host_main と同型）
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
    send_alt_key_up();
}

#[cfg(not(windows))]
fn focus_and_release_alt(_hwnd: isize) {}

// ---------------------------------------------------------------------------
// Engine・フォント・計測（他プローブと同型）
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
        "snotra-egui-soft-host-mvp-history-{}-unused",
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
        "SNOTRA_SOFT_HOST_PHASE name={name} elapsed_ms={:.3}",
        started_at.elapsed().as_secs_f64() * 1000.0
    );
}

fn log_metric(checkpoint: &str, cycle: usize, elapsed: Option<Duration>) {
    let snapshot = process_snapshot();
    let elapsed_ms = elapsed
        .map(|duration| duration.as_secs_f64() * 1_000.0)
        .unwrap_or(-1.0);
    eprintln!(
        "SNOTRA_SOFT_HOST_METRIC checkpoint={checkpoint} cycle={cycle} elapsed_ms={elapsed_ms:.3} working_set_bytes={} private_bytes={} handles={} gdi={} user={} windows={} visible_windows={}",
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
// Tauri ホスト（updater・global shortcut。glow_park_host_main と同型）
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
        eprintln!("SNOTRA_SOFT_HOST_UPDATER=disabled");
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
        eprintln!("SNOTRA_SOFT_HOST_UPDATER={outcome}");
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
            eprintln!("SNOTRA_SOFT_HOST_HOTKEY=hide_dispatched");
        }
        HotkeyPlan::ShowAfterAltRelease => {
            visible.store(true, Ordering::SeqCst);
            let hotkey_started = Instant::now();
            let proxy = proxy.clone();
            let generation = Arc::clone(generation);
            thread::spawn(move || {
                wait_alt_release_or_timeout();
                eprintln!(
                    "SNOTRA_SOFT_HOST_ALT_WAIT elapsed_ms={:.3}",
                    hotkey_started.elapsed().as_secs_f64() * 1000.0
                );
                if generation.load(Ordering::SeqCst) != current_generation {
                    return;
                }
                let _ = proxy.send_event(HostCommand::Show { hotkey_started });
            });
        }
        HotkeyPlan::ShowNow => {
            visible.store(true, Ordering::SeqCst);
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
    let mut probe = SoftHostProbe::new(config, visible, started_at);
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
    eprintln!("SNOTRA_SOFT_HOST_CONFIG={config:?}");
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
                            eprintln!("SNOTRA_SOFT_HOST_HOTKEY=ignored_before_ui_ready");
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
                .name("snotra-soft-host-ui".to_owned())
                .spawn(move || {
                    let exit_code = match run_ui_thread(
                        config,
                        proxy_for_ui,
                        visible_for_ui,
                        started_at,
                    ) {
                        Ok(()) => 0,
                        Err(error) => {
                            eprintln!("SNOTRA_SOFT_HOST_ERROR {error}");
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
        .expect("failed to build Snotra softbuffer host MVP");
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
        CpuTexture, HostCommand, HotkeyPlan, LifecycleEvent, LifecycleState, ProbeConfig,
        UiAction, blend_premultiplied, edge, fill_mesh, modulate, plan_hotkey, plan_ui_action,
        transition,
    };
    use eframe::egui;

    fn show_command() -> HostCommand {
        HostCommand::Show {
            hotkey_started: Instant::now(),
        }
    }

    #[test]
    fn hotkey_branches_match_product_semantics() {
        assert_eq!(plan_hotkey(true, false), HotkeyPlan::HideNow);
        assert_eq!(plan_hotkey(true, true), HotkeyPlan::HideNow);
        assert_eq!(plan_hotkey(false, true), HotkeyPlan::ShowAfterAltRelease);
        assert_eq!(plan_hotkey(false, false), HotkeyPlan::ShowNow);
    }

    #[test]
    fn ui_commands_are_idempotent_and_defer_hide_while_showing() {
        assert_eq!(
            plan_ui_action(LifecycleState::Visible, &show_command()),
            UiAction::Refocus
        );
        assert_eq!(
            plan_ui_action(LifecycleState::Suspended, &HostCommand::Hide),
            UiAction::Ignore
        );
        assert_eq!(
            plan_ui_action(LifecycleState::Suspended, &show_command()),
            UiAction::Show
        );
        assert_eq!(
            plan_ui_action(LifecycleState::Visible, &HostCommand::Hide),
            UiAction::Hide
        );
        assert_eq!(
            plan_ui_action(LifecycleState::Recreating, &HostCommand::Hide),
            UiAction::Defer
        );
        assert_eq!(
            plan_ui_action(LifecycleState::Recreating, &show_command()),
            UiAction::Ignore
        );
        assert_eq!(
            plan_ui_action(LifecycleState::Exiting, &show_command()),
            UiAction::Ignore
        );
    }

    #[test]
    fn lifecycle_requires_frame_before_visible_and_allows_hidden_exit() {
        let showing = transition(LifecycleState::Suspended, LifecycleEvent::Show).unwrap();
        assert_eq!(showing, LifecycleState::Recreating);
        let visible = transition(showing, LifecycleEvent::FramePresented).unwrap();
        assert_eq!(visible, LifecycleState::Visible);
        assert!(transition(LifecycleState::Suspended, LifecycleEvent::Hide).is_err());
        assert_eq!(
            transition(LifecycleState::Suspended, LifecycleEvent::Exit).unwrap(),
            LifecycleState::Exiting
        );
    }

    #[test]
    fn cli_defaults_to_interactive_and_parses_endurance_overrides() {
        let default = ProbeConfig::parse(Vec::<String>::new()).unwrap();
        assert_eq!(default.cycles, 0);
        assert!(default.focus);
        assert!(default.start_visible);
        let endurance = ProbeConfig::parse(
            ["--cycles", "100", "--focus", "off", "--start", "hidden"].map(str::to_owned),
        )
        .unwrap();
        assert_eq!(endurance.cycles, 100);
        assert!(!endurance.focus);
        assert!(!endurance.start_visible);
        assert!(ProbeConfig::parse(["--focus".to_owned(), "maybe".to_owned()]).is_err());
    }

    #[test]
    fn raster_core_matches_soft_probe_invariants() {
        // soft_main と同じ中核を持つことを、代表入力の再実行で固定する。
        assert!(edge(0.0, 0.0, 4.0, 0.0, 1.0, 1.0) > 0.0);
        assert_eq!(blend_premultiplied(0x0000_0000, [255, 0, 0, 255]), 0x00FF_0000);
        assert_eq!(blend_premultiplied(0x0012_3456, [0, 0, 0, 0]), 0x0012_3456);
        assert_eq!(
            modulate([200, 100, 50, 255], [255, 255, 255, 255]),
            [200, 100, 50, 255]
        );

        let mut buffer = vec![0u32; 8 * 8];
        let vertex = |x: f32, y: f32| egui::epaint::Vertex {
            pos: egui::pos2(x, y),
            uv: egui::pos2(0.0, 0.0),
            color: egui::Color32::from_rgba_premultiplied(255, 255, 255, 255),
        };
        let vertices = [vertex(0.0, 0.0), vertex(8.0, 0.0), vertex(0.0, 8.0)];
        let white = CpuTexture {
            width: 0,
            height: 0,
            pixels: Vec::new(),
        };
        fill_mesh(
            &mut buffer,
            8,
            8,
            &vertices,
            &[0, 1, 2],
            &white,
            (0, 0),
            (8, 8),
            1.0,
        );
        assert_eq!(buffer[8 + 1], 0x00FF_FFFF);
        assert_eq!(buffer[7 * 8 + 7], 0);
    }
}
