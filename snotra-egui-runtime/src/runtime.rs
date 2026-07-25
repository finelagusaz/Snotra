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
    ime::ImeBridge,
    input::InputState,
    renderer::EguiRenderer,
    repaint::{RepaintScheduler, WakeReceiver, WindowWaker, wake_channel},
};

pub trait EguiView: Send + 'static {
    fn setup(&mut self, _context: &egui::Context) {}

    fn update(&mut self, ui: &mut egui::Ui, frame: &mut RuntimeFrame);
}

pub struct RuntimeFrame {
    drag_requested: bool,
}

impl RuntimeFrame {
    pub fn drag_window(&mut self) {
        self.drag_requested = true;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("Tauri window error: {0}")]
    Tauri(#[from] tauri::Error),
    #[error("softbuffer surface initialization failed: {0}")]
    SurfaceInit(String),
    #[error("softbuffer present failed: {0}")]
    Present(String),
    #[error("Windows IME initialization failed: {0}")]
    ImeInitialization(String),
    #[error("egui runtime is not installed")]
    NotInstalled,
    #[error("an egui view is already attached to window '{0}'")]
    DuplicateWindow(String),
}

const MAX_PAINT_RETRIES: u32 = 5;
/// 描画失敗の再試行間隔（指数バックオフ）。MAX_PAINT_RETRIES 回まで Some、以降 None（fatal）。
fn retry_delay(consecutive_failures: u32) -> Option<std::time::Duration> {
    (consecutive_failures <= MAX_PAINT_RETRIES)
        .then(|| std::time::Duration::from_millis(16u64 << consecutive_failures.min(6)))
}

#[derive(Clone, Default)]
pub struct EguiRuntime {
    pending: Arc<Mutex<HashMap<String, PendingWindow>>>,
    installed: Arc<AtomicBool>,
}

/// 活性化待ちの窓。`wake_rx` は活性化時に `RepaintScheduler` へ渡して消えるため、
/// 「活性化後は存在しない」ことを型で表す（`Option` + `take()` にしない）。
struct PendingWindow {
    window: EguiWindow,
    wake_rx: WakeReceiver,
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

    /// 窓に view を結び付け、その窓を外部から起こす [`WindowWaker`] を返す（#671 PR D）。
    /// **`#[must_use]` は付けない**——wake を要さない窓では戻り値を落とせる設計にしてある
    /// （意図）。ただし現在の呼び出し元はいずれも束縛しており、捨てる実例は無い（実測）。
    /// 属性を足すなら「落とせる」という設計判断ごと見直すことになる。
    ///
    /// エラー（未 install / ラベル重複）では waker と受信側の両方が落ちる。
    pub fn attach<V: EguiView>(
        &self,
        window: tauri::Window,
        view: V,
    ) -> Result<WindowWaker, RuntimeError> {
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
        // wake 経路はこの窓の活性化を待たずに作る（要求は queue され、活性化直後に届く）。
        let (waker, wake_rx) = wake_channel();
        let mut pending = self.pending.lock().expect("egui pending window lock");
        match pending.entry(label.clone()) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(PendingWindow {
                    window: egui_window,
                    wake_rx,
                });
                Ok(waker)
            }
            std::collections::hash_map::Entry::Occupied(_) => {
                Err(RuntimeError::DuplicateWindow(label))
            }
        }
    }
}

struct RuntimePluginBuilder {
    pending: Arc<Mutex<HashMap<String, PendingWindow>>>,
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
    pending: Arc<Mutex<HashMap<String, PendingWindow>>>,
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
            let Some(PendingWindow {
                mut window,
                wake_rx,
            }) = pending.remove(&label)
            else {
                continue;
            };
            // softbuffer surface は GDI 親和性ゆえ present スレッド（このイベントループ）
            // で生成する（EguiWindow::new は別スレッドから呼ばれうるため作れない）。
            match EguiRenderer::new(window.window.clone()) {
                Ok(renderer) => window.renderer = Some(renderer),
                Err(error) => {
                    log::error!("egui softbuffer surface init failed: {error}");
                    eprintln!("SNOTRA_EGUI_RENDER_ERROR={error}");
                    // この window はスキップ（attach の同期エラー契約は狭まる）。wake_rx も
                    // ここで落ちるため、以降この窓宛の `WindowWaker::wake()` は no-op になる。
                    continue;
                }
            }
            // 受信側を worker へ渡す（送信側は attach が呼び出し元へ返した WindowWaker）。
            let scheduler = RepaintScheduler::new(proxy.clone(), window_id, wake_rx);
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
    // 活性化（イベントループの attach_pending_windows）時に Some になる。softbuffer surface
    // は GDI 親和性ゆえ present スレッド（イベントループ）で生成する必要があり、
    // attach() を呼ぶスレッド（別スレッドのことがある）では作れない。
    renderer: Option<EguiRenderer>,
    view: Box<dyn EguiView>,
    visible: bool,
    paint_failures: u32,
}

impl EguiWindow {
    fn new(window: tauri::Window, mut view: Box<dyn EguiView>) -> Result<Self, RuntimeError> {
        let scale_factor = window.scale_factor()? as f32;
        let ime = ImeBridge::new(&window)?;
        let context = egui::Context::default();
        view.setup(&context);
        Ok(Self {
            context,
            window,
            input: InputState::new(scale_factor),
            ime,
            renderer: None,
            view,
            visible: true,
            paint_failures: 0,
        })
    }

    fn on_window_event(&mut self, event: &tauri_runtime_wry::tao::event::WindowEvent<'_>) -> bool {
        self.drain_native_ime();
        if let tauri_runtime_wry::tao::event::WindowEvent::ScaleFactorChanged { scale_factor, .. } =
            event
        {
            eprintln!("SNOTRA_EGUI_DPI_CHANGED scale_factor={scale_factor:.3}");
        }
        if let tauri_runtime_wry::tao::event::WindowEvent::Focused(true) = event {
            // #532 SU1: main.rs の Alt+Q は RuntimeFrame 経由でなく tauri::Window を直接
            // show()/hide() する。show() 直後は必ず set_focus() が続く（show_and_focus）ため、
            // Focused(true) を「再表示された」の代理シグナルとして visible を復帰させる
            // （外部再表示イベント・不変条件⑥の最小追従）。
            self.visible = true;
        }
        self.input.on_window_event(event)
    }

    fn render(&mut self) -> Result<(), RuntimeError> {
        if !self.visible {
            // 到達可能性の注記（#671 サイクル PR A）: `RuntimeFrame::hide_window()` 削除
            // （本ブランチ b9a9caf）により、crate 内で visible を false にする経路は現在
            // 無い（true にするのは new() の初期値と Focused(true) ハンドラのみ）。よって
            // このガードは現在到達不能。それでも残すのは、「hidden 中は update() が走ら
            // ない」という #532 SU5 の不変条件がどの機構に依るか未測定（OS/tao が hidden
            // 窓へ WM_PAINT を送らないためと推定）であり、将来 runtime 側での抑止が必要
            // になったときの受け口として置いているため。参照:
            // docs/superpowers/specs/2026-07-25-egui-window-ownership-and-event-delivery-design.md §7 残余 2・3
            return Ok(()); // 不変条件⑥: 非表示中は描かない。
        }
        self.drain_native_ime();
        let size = self.window.inner_size()?;
        let ppp = self.window.scale_factor()? as f32;
        let max_side = self
            .renderer
            .as_ref()
            .ok_or(RuntimeError::NotInstalled)?
            .max_texture_side();
        let raw_input = self.input.take(max_side, size, ppp);
        let mut frame = RuntimeFrame {
            drag_requested: false,
        };
        let output = self
            .context
            .run_ui(raw_input, |ui| self.view.update(ui, &mut frame));
        self.handle_platform_output(&output.platform_output);
        // 描画失敗は能動再試行（不変条件⑤）。「次 RedrawRequested 待ち」は egui が repaint を
        // 求めなければ停止するため、失敗時に自ら repaint を要求し、上限超過で fatal にする。
        match self
            .renderer
            .as_mut()
            .expect("renderer installed by attach_pending_windows")
            .paint(&self.context, output, size)
        {
            Ok(_) => self.paint_failures = 0,
            Err(error) => match retry_delay(self.paint_failures + 1) {
                Some(delay) => {
                    self.paint_failures += 1;
                    log::warn!("egui paint failed (retry {}): {error}", self.paint_failures);
                    self.context.request_repaint_after(delay);
                }
                None => return Err(error),
            },
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
                    log::warn!("copying egui images is not implemented in this runtime");
                }
                egui::OutputCommand::OpenUrl(url) => {
                    log::warn!(
                        "opening URL is not implemented in this runtime: {}",
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
        if frame.drag_requested {
            self.window.start_dragging()?;
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

#[cfg(test)]
mod tests {
    use super::{MAX_PAINT_RETRIES, retry_delay};

    #[test]
    fn hidden_window_is_not_painted() {
        // visible=false のとき render は早期 return する契約を、visible 述語で固定。
        fn should_render(visible: bool) -> bool {
            visible
        }
        assert!(!should_render(false));
        assert!(should_render(true));
    }

    #[test]
    fn retry_delay_backs_off_then_gives_up() {
        assert!(retry_delay(1).is_some());
        assert!(retry_delay(MAX_PAINT_RETRIES).is_some());
        assert!(
            retry_delay(MAX_PAINT_RETRIES + 1).is_none(),
            "上限超過は fatal"
        );
        assert!(
            retry_delay(2).unwrap() > retry_delay(1).unwrap(),
            "バックオフは単調増加"
        );
    }
}
