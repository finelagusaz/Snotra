use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use tauri_runtime::{UserEvent, window::WindowId};
use tauri_runtime_wry::{
    Context, EventLoopIterationContext, Message, Plugin, PluginBuilder, WebContextStore,
    tao::{event::Event, event_loop::ControlFlow},
};

use crate::{
    gpu::GpuFaultInjection,
    ime::ImeBridge,
    input::InputState,
    renderer::{EguiRenderer, PaintOutcome},
    repaint::RepaintScheduler,
};

pub trait EguiView: Send + 'static {
    fn setup(&mut self, _context: &egui::Context) {}

    fn update(&mut self, ui: &mut egui::Ui, frame: &mut RuntimeFrame);
}

pub struct RuntimeFrame {
    close_requested: bool,
    hide_requested: bool,
    drag_requested: bool,
    gpu_fault_requested: Option<GpuFaultInjection>,
}

impl RuntimeFrame {
    pub fn close_window(&mut self) {
        self.close_requested = true;
    }

    pub fn hide_window(&mut self) {
        self.hide_requested = true;
    }

    pub fn drag_window(&mut self) {
        self.drag_requested = true;
    }

    /// Issue #532の障害復旧検証専用。製品UIからは呼び出さない。
    pub fn inject_gpu_fault(&mut self, fault: GpuFaultInjection) {
        self.gpu_fault_requested = Some(fault);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("Tauri window error: {0}")]
    Tauri(#[from] tauri::Error),
    #[error("wgpu initialization failed: {0}")]
    GpuInitialization(String),
    #[error("wgpu surface validation failed")]
    SurfaceValidation,
    #[error("wgpu reported out of memory")]
    GpuOutOfMemory,
    #[error("Windows IME initialization failed: {0}")]
    ImeInitialization(String),
    #[error("egui runtime is not installed")]
    NotInstalled,
    #[error("an egui view is already attached to window '{0}'")]
    DuplicateWindow(String),
}

#[derive(Clone, Default)]
pub struct EguiRuntime {
    pending: Arc<Mutex<HashMap<String, EguiWindow>>>,
    installed: Arc<AtomicBool>,
}

impl EguiRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn install(&self, app: &mut tauri::App<tauri::Wry>) {
        app.wry_plugin(RuntimePluginBuilder {
            pending: Arc::clone(&self.pending),
        });
        self.installed.store(true, Ordering::Release);
    }

    pub fn attach<V: EguiView>(&self, window: tauri::Window, view: V) -> Result<(), RuntimeError> {
        if !self.installed.load(Ordering::Acquire) {
            return Err(RuntimeError::NotInstalled);
        }

        let label = window.label().to_owned();
        {
            let pending = self.pending.lock().expect("egui pending window lock");
            if pending.contains_key(&label) {
                return Err(RuntimeError::DuplicateWindow(label));
            }
        }

        let egui_window = EguiWindow::new(window, Box::new(view))?;
        let mut pending = self.pending.lock().expect("egui pending window lock");
        match pending.entry(label.clone()) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(egui_window);
                Ok(())
            }
            std::collections::hash_map::Entry::Occupied(_) => {
                Err(RuntimeError::DuplicateWindow(label))
            }
        }
    }
}

struct RuntimePluginBuilder {
    pending: Arc<Mutex<HashMap<String, EguiWindow>>>,
}

impl<T: UserEvent> PluginBuilder<T> for RuntimePluginBuilder {
    type Plugin = RuntimePlugin<T>;

    fn build(self, _context: Context<T>) -> Self::Plugin {
        RuntimePlugin::<T> {
            pending: self.pending,
            active: HashMap::new(),
            event_type: std::marker::PhantomData,
        }
    }
}

struct RuntimePlugin<T: UserEvent> {
    pending: Arc<Mutex<HashMap<String, EguiWindow>>>,
    active: HashMap<WindowId, ActiveWindow>,
    event_type: std::marker::PhantomData<T>,
}

struct ActiveWindow {
    window: EguiWindow,
    scheduler: RepaintScheduler,
}

impl<T: UserEvent> Plugin<T> for RuntimePlugin<T> {
    fn on_event(
        &mut self,
        event: &Event<Message<T>>,
        _event_loop: &tauri_runtime_wry::tao::event_loop::EventLoopWindowTarget<Message<T>>,
        proxy: &tauri_runtime_wry::tao::event_loop::EventLoopProxy<Message<T>>,
        _control_flow: &mut ControlFlow,
        context: EventLoopIterationContext<'_, T>,
        _web_context: &WebContextStore,
    ) -> bool {
        self.attach_pending_windows(proxy, &context);

        match event {
            Event::WindowEvent {
                window_id, event, ..
            } => {
                let Some(runtime_id) = context.window_id_map.get(window_id) else {
                    return false;
                };
                if matches!(event, tauri_runtime_wry::tao::event::WindowEvent::Destroyed) {
                    self.active.remove(&runtime_id);
                    return false;
                }
                if let Some(active) = self.active.get_mut(&runtime_id)
                    && active.window.on_window_event(event)
                {
                    active.scheduler.request(std::time::Duration::ZERO);
                }
            }
            Event::RedrawRequested(window_id) => {
                let Some(runtime_id) = context.window_id_map.get(window_id) else {
                    return false;
                };
                if let Some(active) = self.active.get_mut(&runtime_id)
                    && let Err(error) = active.window.render()
                {
                    log::error!("egui render failed: {error}");
                    eprintln!("SNOTRA_EGUI_RENDER_ERROR={error}");
                }
            }
            _ => {}
        }

        false
    }
}

impl<T: UserEvent> RuntimePlugin<T> {
    fn attach_pending_windows(
        &mut self,
        proxy: &tauri_runtime_wry::tao::event_loop::EventLoopProxy<Message<T>>,
        context: &EventLoopIterationContext<'_, T>,
    ) {
        let known_windows: Vec<(WindowId, String)> = context
            .windows
            .0
            .borrow()
            .iter()
            .map(|(id, window)| (*id, window.label().to_owned()))
            .collect();
        let mut pending = self.pending.lock().expect("egui pending window lock");

        for (window_id, label) in known_windows {
            let Some(window) = pending.remove(&label) else {
                continue;
            };
            let scheduler = RepaintScheduler::new(proxy.clone(), window_id);
            let callback_scheduler = scheduler.clone();
            window.context.set_request_repaint_callback(move |info| {
                callback_scheduler.request(info.delay);
            });
            scheduler.request(std::time::Duration::ZERO);
            self.active
                .insert(window_id, ActiveWindow { window, scheduler });
        }
    }
}

struct EguiWindow {
    context: egui::Context,
    window: tauri::Window,
    input: InputState,
    ime: ImeBridge,
    renderer: EguiRenderer,
    view: Box<dyn EguiView>,
}

impl EguiWindow {
    fn new(window: tauri::Window, mut view: Box<dyn EguiView>) -> Result<Self, RuntimeError> {
        let size = window.inner_size()?;
        let scale_factor = window.scale_factor()? as f32;
        let renderer = EguiRenderer::new(window.clone())?;
        let ime = ImeBridge::new(&window)?;
        let context = egui::Context::default();
        view.setup(&context);
        Ok(Self {
            context,
            window,
            input: InputState::new(size, scale_factor),
            ime,
            renderer,
            view,
        })
    }

    fn on_window_event(&mut self, event: &tauri_runtime_wry::tao::event::WindowEvent<'_>) -> bool {
        self.drain_native_ime();
        if let tauri_runtime_wry::tao::event::WindowEvent::Resized(size) = event {
            if let Err(error) = self.renderer.configure(size.width, size.height) {
                log::error!("egui resize failed: {error}");
            }
        } else if let tauri_runtime_wry::tao::event::WindowEvent::ScaleFactorChanged {
            new_inner_size,
            scale_factor,
        } = event
        {
            eprintln!(
                "SNOTRA_EGUI_DPI_CHANGED scale_factor={scale_factor:.3} physical={}x{}",
                new_inner_size.width, new_inner_size.height
            );
            if let Err(error) = self
                .renderer
                .configure(new_inner_size.width, new_inner_size.height)
            {
                log::error!("egui DPI resize failed: {error}");
            }
        }
        self.input.on_window_event(event)
    }

    fn render(&mut self) -> Result<(), RuntimeError> {
        self.drain_native_ime();
        let raw_input = self.input.take(self.renderer.max_texture_side());
        let mut frame = RuntimeFrame {
            close_requested: false,
            hide_requested: false,
            drag_requested: false,
            gpu_fault_requested: None,
        };
        let output = self
            .context
            .run_ui(raw_input, |ui| self.view.update(ui, &mut frame));
        self.handle_platform_output(&output.platform_output);
        let paint_outcome = self.renderer.paint(&self.context, output)?;
        if paint_outcome == PaintOutcome::DeviceRecovered {
            // The new egui-wgpu renderer has no copy of textures owned by the
            // old device. Recreate Context so setup re-registers fonts and the
            // next pass emits a complete texture delta; view state stays owned
            // by EguiView and therefore survives the GPU reset.
            self.context = egui::Context::default();
            self.view.setup(&self.context);
            self.context.request_repaint();
        }
        self.apply_frame_commands(frame)?;
        Ok(())
    }

    fn handle_platform_output(&self, output: &egui::PlatformOutput) {
        for command in &output.commands {
            match command {
                egui::OutputCommand::CopyText(text) => {
                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                        let _ = clipboard.set_text(text.clone());
                    }
                }
                egui::OutputCommand::CopyImage(_) => {
                    log::warn!("copying egui images is not implemented in the MVP runtime");
                }
                egui::OutputCommand::OpenUrl(url) => {
                    log::warn!(
                        "opening URL is not implemented in the MVP runtime: {}",
                        url.url
                    );
                }
            }
        }

        if let Some(cursor) = cursor_icon(output.cursor_icon) {
            let _ = self.window.set_cursor_icon(cursor);
        }
        self.ime
            .update(output.ime, self.input.native_pixels_per_point());
    }

    fn drain_native_ime(&mut self) {
        for event in self.ime.drain() {
            self.input.push_ime_event(event);
        }
    }

    fn apply_frame_commands(&mut self, frame: RuntimeFrame) -> Result<(), RuntimeError> {
        if let Some(fault) = frame.gpu_fault_requested {
            self.renderer.inject_fault(fault);
            self.context.request_repaint();
        }
        if frame.drag_requested {
            self.window.start_dragging()?;
        }
        if frame.hide_requested {
            self.window.hide()?;
        }
        if frame.close_requested {
            self.window.close()?;
        }
        Ok(())
    }
}

fn cursor_icon(icon: egui::CursorIcon) -> Option<tauri::CursorIcon> {
    Some(match icon {
        egui::CursorIcon::Default => tauri::CursorIcon::Default,
        egui::CursorIcon::None => return None,
        egui::CursorIcon::ContextMenu => tauri::CursorIcon::ContextMenu,
        egui::CursorIcon::Help => tauri::CursorIcon::Help,
        egui::CursorIcon::PointingHand => tauri::CursorIcon::Hand,
        egui::CursorIcon::Progress => tauri::CursorIcon::Progress,
        egui::CursorIcon::Wait => tauri::CursorIcon::Wait,
        egui::CursorIcon::Cell => tauri::CursorIcon::Cell,
        egui::CursorIcon::Crosshair => tauri::CursorIcon::Crosshair,
        egui::CursorIcon::Text => tauri::CursorIcon::Text,
        egui::CursorIcon::VerticalText => tauri::CursorIcon::VerticalText,
        egui::CursorIcon::Alias => tauri::CursorIcon::Alias,
        egui::CursorIcon::Copy => tauri::CursorIcon::Copy,
        egui::CursorIcon::Move => tauri::CursorIcon::Move,
        egui::CursorIcon::NoDrop => tauri::CursorIcon::NoDrop,
        egui::CursorIcon::NotAllowed => tauri::CursorIcon::NotAllowed,
        egui::CursorIcon::Grab => tauri::CursorIcon::Grab,
        egui::CursorIcon::Grabbing => tauri::CursorIcon::Grabbing,
        egui::CursorIcon::AllScroll => tauri::CursorIcon::AllScroll,
        egui::CursorIcon::ResizeHorizontal => tauri::CursorIcon::EwResize,
        egui::CursorIcon::ResizeNeSw => tauri::CursorIcon::NeswResize,
        egui::CursorIcon::ResizeNwSe => tauri::CursorIcon::NwseResize,
        egui::CursorIcon::ResizeVertical => tauri::CursorIcon::NsResize,
        egui::CursorIcon::ResizeEast => tauri::CursorIcon::EResize,
        egui::CursorIcon::ResizeSouthEast => tauri::CursorIcon::SeResize,
        egui::CursorIcon::ResizeSouth => tauri::CursorIcon::SResize,
        egui::CursorIcon::ResizeSouthWest => tauri::CursorIcon::SwResize,
        egui::CursorIcon::ResizeWest => tauri::CursorIcon::WResize,
        egui::CursorIcon::ResizeNorthWest => tauri::CursorIcon::NwResize,
        egui::CursorIcon::ResizeNorth => tauri::CursorIcon::NResize,
        egui::CursorIcon::ResizeNorthEast => tauri::CursorIcon::NeResize,
        egui::CursorIcon::ResizeColumn => tauri::CursorIcon::ColResize,
        egui::CursorIcon::ResizeRow => tauri::CursorIcon::RowResize,
        egui::CursorIcon::ZoomIn => tauri::CursorIcon::ZoomIn,
        egui::CursorIcon::ZoomOut => tauri::CursorIcon::ZoomOut,
    })
}
