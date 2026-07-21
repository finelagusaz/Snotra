#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Issue #532 softbuffer 最小スパイク。
//!
//! WebView2 も GPU ランタイム（GL/wgpu）も持たず、egui を CPU でラスタライズして
//! softbuffer（Windows では GDI 転送)で提示する構成の床を測る。
//!
//! - private bytes / working set / handle / GDI / USER の床
//! - コールドスタート内訳と初回フレーム時間
//! - hide/show 反復の warm フレーム時間（present 完了まで）
//!
//! GPU 固定費（AMD OpenGL DLL 群 ~141 MiB / wgpu ~469 MiB）と WebView2 固定費の
//! 双方を外したとき、Engine + フォントだけでどこまで下がるかが問い。

use std::{
    collections::HashMap,
    num::NonZeroU32,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use eframe::egui;
use egui_glow::egui_winit;
use snotra_core::{
    config::Config, engine::Engine, history::HistoryStore, indexer::AppEntry,
    ui_types::SearchResult,
};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowAttributes, WindowId},
};

const ENTRY_COUNT: usize = 10_000;
const FONT_PATH: &str = "C:/Windows/Fonts/YuGothM.ttc";
const WINDOW_TITLE: &str = "Snotra softbuffer MVP";
const CLEAR_COLOR: u32 = 0x00F5_F5F5;

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProbeConfig {
    cycles: usize,
    visible_wait_ms: u64,
    hidden_wait_ms: u64,
}

impl ProbeConfig {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut config = Self {
            cycles: 3,
            visible_wait_ms: 5_000,
            hidden_wait_ms: 3_000,
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
                    if config.cycles == 0 {
                        return Err("--cycles must be greater than zero".to_owned());
                    }
                }
                "--visible-wait-ms" => config.visible_wait_ms = parse_u64(&option, &value)?,
                "--hidden-wait-ms" => config.hidden_wait_ms = parse_u64(&option, &value)?,
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
// CPU ラスタライザの中核（純関数）
// ---------------------------------------------------------------------------

/// 2D エッジ関数（外積）。正なら点 p は a→b の左側にある。
/// 三角形の 3 辺すべてで符号が面積と一致する点が内部である。
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

// ---------------------------------------------------------------------------
// テクスチャ管理
// ---------------------------------------------------------------------------

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
// プローブ本体
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum ScheduledAction {
    Hide,
    Show,
    Exit,
}

struct SoftProbe {
    config: ProbeConfig,
    engine: Engine,
    query: String,
    results: Vec<SearchResult>,
    window: Option<Arc<Window>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    egui_ctx: egui::Context,
    egui_winit: Option<egui_winit::State>,
    textures: HashMap<egui::TextureId, CpuTexture>,
    visible: bool,
    scheduled: Option<(Instant, ScheduledAction)>,
    show_started: Option<Instant>,
    completed_cycles: usize,
    warm_times_ms: Vec<f64>,
    raster_times_ms: Vec<f64>,
    started_at: Instant,
    focus_search: bool,
    error: Option<String>,
}

impl SoftProbe {
    fn new(config: ProbeConfig, started_at: Instant) -> Self {
        let engine_started = Instant::now();
        let engine = build_verification_engine(ENTRY_COUNT);
        log_phase(started_at, "engine_ready");
        eprintln!(
            "SNOTRA_SOFT_INFO engine_build_ms={:.3} entries={ENTRY_COUNT}",
            engine_started.elapsed().as_secs_f64() * 1000.0
        );
        let results = engine.recent_history();
        Self {
            config,
            engine,
            query: String::new(),
            results,
            window: None,
            surface: None,
            egui_ctx: egui::Context::default(),
            egui_winit: None,
            textures: HashMap::new(),
            visible: true,
            scheduled: None,
            show_started: Some(started_at),
            completed_cycles: 0,
            warm_times_ms: Vec::new(),
            raster_times_ms: Vec::new(),
            started_at,
            focus_search: true,
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
        eprintln!("SNOTRA_SOFT_INFO font_bytes={font_bytes}");
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
        let window = self
            .window
            .as_ref()
            .ok_or_else(|| "window is missing during hide".to_owned())?;
        window.set_visible(false);
        self.visible = false;
        let _ = trim_working_set();
        log_metric("hidden_settled", self.completed_cycles, None);
        self.scheduled = Some((
            Instant::now() + Duration::from_millis(self.config.hidden_wait_ms),
            ScheduledAction::Show,
        ));
        Ok(())
    }

    fn show(&mut self) -> Result<(), String> {
        let window = self
            .window
            .as_ref()
            .ok_or_else(|| "window is missing during show".to_owned())?;
        self.show_started = Some(Instant::now());
        window.set_visible(true);
        self.visible = true;
        self.focus_search = true;
        window.request_redraw();
        Ok(())
    }

    fn render(&mut self) -> Result<(), String> {
        if !self.visible {
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
                    ui.heading("Snotra softbuffer 検証");
                    ui.label("WebView2 なし / GPU ランタイムなし / CPU ラスタ + GDI 転送");
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
                        "{} results / Engine {ENTRY_COUNT} entries",
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

        if let Some(started) = self.show_started.take() {
            let elapsed = started.elapsed();
            if self.completed_cycles == 0 && self.warm_times_ms.is_empty() {
                log_metric("initial_frame_presented", 0, Some(elapsed));
                log_phase(self.started_at, "first_frame_presented");
                eprintln!("SNOTRA_SOFT_INFO first_raster_ms={raster_ms:.3}");
            } else {
                self.warm_times_ms.push(elapsed.as_secs_f64() * 1000.0);
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
                self.hide()
            }
            ScheduledAction::Show => {
                self.completed_cycles += 1;
                self.show()
            }
            ScheduledAction::Exit => {
                log_metric("final_visible_settled", self.completed_cycles, None);
                self.finish(event_loop);
                Ok(())
            }
        }
    }

    fn finish(&mut self, event_loop: &ActiveEventLoop) {
        if !self.warm_times_ms.is_empty() {
            let count = self.warm_times_ms.len();
            let sum: f64 = self.warm_times_ms.iter().sum();
            let min = self.warm_times_ms.iter().cloned().fold(f64::MAX, f64::min);
            let max = self.warm_times_ms.iter().cloned().fold(f64::MIN, f64::max);
            eprintln!(
                "SNOTRA_SOFT_SUMMARY warm_frames={count} warm_ms_min={min:.3} warm_ms_avg={:.3} warm_ms_max={max:.3}",
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
                "SNOTRA_SOFT_SUMMARY raster_frames={count} raster_ms_avg={:.3} raster_ms_max={max:.3}",
                sum / count as f64
            );
        }
        event_loop.exit();
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: String) {
        eprintln!("SNOTRA_SOFT_ERROR {error}");
        self.error = Some(error);
        event_loop.exit();
    }
}

impl ApplicationHandler for SoftProbe {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none()
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
        let Some(window) = self.window.as_ref().map(Arc::clone) else {
            return;
        };
        if window.id() != window_id {
            return;
        }
        // RedrawRequested を egui_winit へ渡すと repaint 応答が再描画要求を生み、
        // 描画が自己永続ループになる（実測: 15 秒で約 2,000 フレーム）。入力イベント
        // だけを egui へ渡し、描画イベントは render へ直行させる。
        if !matches!(event, WindowEvent::RedrawRequested)
            && let Some(egui_winit) = self.egui_winit.as_mut()
        {
            let response = egui_winit.on_window_event(&window, &event);
            if response.repaint && self.visible {
                window.request_redraw();
            }
        }
        let result = match event {
            WindowEvent::CloseRequested => {
                self.finish(event_loop);
                Ok(())
            }
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::Resized(size) if size.width > 0 && size.height > 0 && self.visible => {
                window.request_redraw();
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
        .with_drag_and_drop(false)
        .with_visible(true)
}

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
        "snotra-egui-soft-mvp-history-{}-unused",
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
        // #399/#579: jp_font を fallback（push=末尾）にすると Latin は egui 既定
        // フォント・CJK は Yu Gothic の 2 フォントに分かれ、混在行のベースラインが
        // ずれる。被覆 AA の無い softbuffer ラスタライザは分数差を整数 px に丸めて
        // 顕在化させる。Yu Gothic は Latin も持つので先頭に置き、両スクリプトを
        // 単一フォントで描いて 1 ベースラインに統一する（soft_host と同修正）。
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, "jp_font".to_owned());
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
        "SNOTRA_SOFT_PHASE name={name} elapsed_ms={:.3}",
        started_at.elapsed().as_secs_f64() * 1000.0
    );
}

fn log_metric(checkpoint: &str, cycle: usize, elapsed: Option<Duration>) {
    let snapshot = process_snapshot();
    let elapsed_ms = elapsed
        .map(|duration| duration.as_secs_f64() * 1_000.0)
        .unwrap_or(-1.0);
    eprintln!(
        "SNOTRA_SOFT_METRIC checkpoint={checkpoint} cycle={cycle} elapsed_ms={elapsed_ms:.3} working_set_bytes={} private_bytes={} handles={} gdi={} user={} windows={} visible_windows={}",
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
    let started_at = Instant::now();
    let config = ProbeConfig::parse(std::env::args().skip(1))?;
    eprintln!("SNOTRA_SOFT_CONFIG={config:?}");
    log_metric("process_start", 0, None);
    let event_loop =
        EventLoop::new().map_err(|error| format!("event loop creation failed: {error}"))?;
    event_loop.set_control_flow(ControlFlow::Wait);
    log_phase(started_at, "event_loop_ready");
    let mut probe = SoftProbe::new(config, started_at);
    event_loop
        .run_app(&mut probe)
        .map_err(|error| format!("event loop failed: {error}"))?;
    log_metric("process_exit", 0, None);
    if let Some(error) = probe.error {
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

// ---------------------------------------------------------------------------
// テスト
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{CpuTexture, ProbeConfig, blend_premultiplied, edge, fill_mesh, modulate};
    use eframe::egui;

    #[test]
    fn cli_defaults_match_lifecycle_probe_conventions() {
        let config = ProbeConfig::parse(Vec::<String>::new()).unwrap();
        assert_eq!(config.cycles, 3);
        assert_eq!(config.visible_wait_ms, 5_000);
        assert_eq!(config.hidden_wait_ms, 3_000);
    }

    #[test]
    fn cli_rejects_zero_cycles_and_unknown_options() {
        assert!(ProbeConfig::parse(["--cycles".to_owned(), "0".to_owned()]).is_err());
        assert!(ProbeConfig::parse(["--wat".to_owned(), "1".to_owned()]).is_err());
    }

    #[test]
    fn edge_function_signs_separate_inside_from_outside() {
        // 反時計回り三角形 (0,0)-(4,0)-(0,4)。内部点は正、外部点は負。
        assert!(edge(0.0, 0.0, 4.0, 0.0, 1.0, 1.0) > 0.0);
        assert!(edge(0.0, 0.0, 4.0, 0.0, 1.0, -1.0) < 0.0);
        // 線上は 0。
        assert_eq!(edge(0.0, 0.0, 4.0, 0.0, 2.0, 0.0), 0.0);
    }

    #[test]
    fn opaque_source_replaces_and_transparent_source_keeps_destination() {
        assert_eq!(blend_premultiplied(0x0000_0000, [255, 0, 0, 255]), 0x00FF_0000);
        assert_eq!(blend_premultiplied(0x0012_3456, [0, 0, 0, 0]), 0x0012_3456);
    }

    #[test]
    fn half_alpha_source_blends_toward_source() {
        // premultiplied: src = (128,0,0,128) を白 dst に over → dst' ≈ src + dst*(1-a)
        let blended = blend_premultiplied(0x00FF_FFFF, [128, 0, 0, 128]);
        let r = (blended >> 16) & 0xFF;
        let g = (blended >> 8) & 0xFF;
        assert!((r as i32 - 255).abs() <= 2, "r={r}");
        assert!((g as i32 - 127).abs() <= 2, "g={g}");
    }

    #[test]
    fn modulate_by_white_texture_is_identity() {
        assert_eq!(
            modulate([200, 100, 50, 255], [255, 255, 255, 255]),
            [200, 100, 50, 255]
        );
        assert_eq!(modulate([200, 100, 50, 255], [0, 0, 0, 0]), [0, 0, 0, 0]);
    }

    #[test]
    fn fill_mesh_rasterizes_triangle_inside_only() {
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
        // 対角線の左上は塗られ、右下は塗られない。
        assert_eq!(buffer[8 + 1], 0x00FF_FFFF);
        assert_eq!(buffer[7 * 8 + 7], 0);
    }
}
