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
    clear_color: Option<egui::Color32>,
    /// このフレームがイベントループスレッド上で走っていることの証人。
    /// `render()` は wry plugin の `on_event` からしか呼ばれないため、この値が存在すること
    /// 自体がその証明である（`proof.rs` の `//!`）。
    proof: crate::proof::EventLoopProof,
}

impl RuntimeFrame {
    pub fn drag_window(&mut self) {
        self.drag_requested = true;
    }

    /// このフレームの背景色（softbuffer が present 前に塗る下地）。
    ///
    /// **毎フレーム呼ぶこと。** `render()` は `run_ui` → `paint` の順に進むため、`update()` の中で
    /// 決めた色は**同じフレームの `buffer.fill` に間に合う**（遅れは原理的に生じない）。
    /// 呼ばなかったフレームは `renderer.rs` の `CLEAR_COLOR` へ落ちる——**ビルドでも自動テストでも
    /// 落ちない**。検知するのは `npm run check:colors` と目視で、どちらも CI には無い
    /// （`docs/build-commands.md`「`[visual]` の色を変える変更は、**非既定色で**目視する」）。
    pub fn set_clear_color(&mut self, color: egui::Color32) {
        self.clear_color = Some(color);
    }

    /// このフレームがイベントループスレッド上にいることの証人を貸す。
    ///
    /// **`&self` で貸すのは意図である**——証人は `!Send` ゆえ、借りた側もこのフレームの
    /// 寿命を越えて持ち出せない。
    pub fn event_loop(&self) -> &crate::proof::EventLoopProof {
        &self.proof
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
    /// 窓が載っている HMONITOR（isize・#737）。`Moved` はドラッグ中に連発するため、
    /// 変化検知（安価な `window_monitor`）を挟んでからリフレッシュレート再取得
    /// （`EnumDisplaySettingsW` 込み・安くない）へ進むためのキャッシュ。
    last_monitor: Option<isize>,
}

/// モニターのリフレッシュレートを取得して配送の下限間隔へ供給する（契約②・#737）。
/// 返り値は現在の HMONITOR（変化検知キャッシュの更新用）。取得できない環境では
/// `set_min_interval(None)` が 60Hz フォールバックへ倒す。
fn refresh_min_interval(window: &tauri::Window, scheduler: &RepaintScheduler) -> Option<isize> {
    let hwnd = window.hwnd().ok().map(|h| h.0 as isize);
    let monitor = hwnd.and_then(crate::monitor::window_monitor);
    scheduler.set_min_interval(hwnd.and_then(crate::monitor::monitor_refresh_hz));
    monitor
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
                use tauri_runtime_wry::tao::event::WindowEvent as TaoWindowEvent;
                // **引き当ての前に残す**（#872/#936 の型 B）。この下の 2 つの引き当て
                // （`window_id_map` と `self.active`）はどちらも失敗すると `return false` で
                // **黙って捨てる**ため、ここより後ろに置くと「そもそも配送されなかった」と
                // 「配送されたが捨てた」が同じ沈黙へ潰れる。判定は先に問う——`format!` の
                // 割り当てを打鍵のたびに無条件で払わないため。
                if crate::input::input_trace_enabled() {
                    match event {
                        TaoWindowEvent::KeyboardInput { event: key, .. } => {
                            crate::input::input_trace(
                                "rx_key",
                                &format!(
                                    "window_id={window_id:?} state={:?} physical={:?}",
                                    key.state, key.physical_key
                                ),
                            )
                        }
                        TaoWindowEvent::ReceivedImeText(text) => crate::input::input_trace(
                            "rx_text",
                            &format!("window_id={window_id:?} chars={}", text.chars().count()),
                        ),
                        _ => {}
                    }
                }
                let Some(runtime_id) = context.window_id_map.get(window_id) else {
                    return false;
                };
                if matches!(event, TaoWindowEvent::Destroyed) {
                    self.active.remove(&runtime_id);
                    return false;
                }
                // 契約②（#737）: 配送の下限間隔の backstop。窓が静止したまま OS 設定で
                // リフレッシュレートが変わる・モニターが抜き差しされる経路は Moved でも
                // ScaleFactorChanged でも捕まらないため、show のたびに必ず来る Focused(true)
                // で再取得する（頻度は低い）。**全窓**に適用するのは、従属窓（results）が
                // focusable(false) + SW_SHOWNOACTIVATE で Focused を一度も受けないため
                // ——両窓は設計上同じモニターに載る（results は main 直下へ追従・/symmetric-check #737）。
                if matches!(event, TaoWindowEvent::Focused(true)) {
                    for active in self.active.values_mut() {
                        active.last_monitor =
                            refresh_min_interval(&active.window.window, &active.scheduler);
                    }
                }
                if let Some(active) = self.active.get_mut(&runtime_id) {
                    // 契約②（#737）: モニター跨ぎの追従。Moved はドラッグ中に連発するため、
                    // 安価な window_monitor の比較を挟み、変わったときだけ再取得する。
                    if matches!(
                        event,
                        TaoWindowEvent::Moved(_) | TaoWindowEvent::ScaleFactorChanged { .. }
                    ) {
                        let monitor = active
                            .window
                            .window
                            .hwnd()
                            .ok()
                            .and_then(|h| crate::monitor::window_monitor(h.0 as isize));
                        if monitor != active.last_monitor {
                            active.last_monitor =
                                refresh_min_interval(&active.window.window, &active.scheduler);
                        }
                    }
                    if active.window.on_window_event(event) {
                        active.scheduler.request(std::time::Duration::ZERO);
                    }
                }
            }
            Event::RedrawRequested(window_id) => {
                // hidden 中の抑止点の切り分け計器（#697）。送信側は repaint.rs の worker。
                // 引き当て前に置く——引き当て失敗で握りつぶされる経路も観測対象。
                // runtime_id は送信側の window_id と同じ ID 空間ゆえ、窓の帰属を直接照合できる。
                if std::env::var_os("SNOTRA_EGUI_WAKE_TRACE").is_some() {
                    eprintln!(
                        "SNOTRA_EGUI_WAKE_RECV window_id={window_id:?} runtime_id={:?}",
                        context.window_id_map.get(window_id)
                    );
                }
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
            // 契約②（#737）: 活性化時にモニターのリフレッシュレートを配送の下限間隔へ供給する。
            let last_monitor = refresh_min_interval(&window.window, &scheduler);
            self.active.insert(
                window_id,
                ActiveWindow {
                    window,
                    scheduler,
                    last_monitor,
                },
            );
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
    /// 直前フレームの時刻（`SNOTRA_EGUI_REPAINT_TRACE` 専用・#628）。表示にも制御にも
    /// 影響しない。env 未設定なら一度も書かれず `None` のまま。
    repaint_trace_prev: Option<std::time::Instant>,
    /// 最後に OS へ適用したカーソル（`handle_platform_output` の変化検出用）。
    /// `SetCursor` はスレッド共通ゆえ毎フレーム撃つと 2 窓が上書きし合う（同関数のコメント）。
    applied_cursor: Option<egui::CursorIcon>,
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
            repaint_trace_prev: None,
            applied_cursor: None,
        })
    }

    fn on_window_event(&mut self, event: &tauri_runtime_wry::tao::event::WindowEvent<'_>) -> bool {
        self.drain_native_ime();
        if let tauri_runtime_wry::tao::event::WindowEvent::ScaleFactorChanged {
            scale_factor, ..
        } = event
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
            // （PR #677）により、crate 内で visible を false にする経路は現在
            // 無い（true にするのは new() の初期値と Focused(true) ハンドラのみ）。よって
            // このガードは現在到達不能。それでも残すのは、「hidden 中は update() が走ら
            // ない」という #532 SU5 の不変条件が runtime の外＝tao/OS 層に依っており
            // （2026-07-26 実測・#697: worker は RequestRedraw を送信するが、hidden な
            // 窓には RedrawRequested が配送されない）、将来 runtime 側での抑止が必要
            // になったときの受け口として置いているため。参照:
            // docs/superpowers/specs/2026-07-25-egui-window-ownership-and-event-delivery-design.md §7 残余 2・3（#697 の errata で解消）
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
            clear_color: None,
            proof: crate::proof::EventLoopProof::new(),
        };
        let output = self
            .context
            .run_ui(raw_input, |ui| self.view.update(ui, &mut frame));
        // 可視アイドルの周期 repaint 源を名指しする計器（#628）。env 未設定なら Instant も
        // causes の clone も取らない——計測器が測定対象を汚さないため（renderer.rs と同規範）。
        //
        // 読むときの 3 点:
        // - `repaint_causes()` が返すのは `prev_causes`（egui 0.35 `context.rs:98-105` の
        //   pass 冒頭 swap）。ゆえに **1 パス前**に積まれた原因である
        // - 遅延ゼロの `request_repaint()` は 2 フレーム生む（`outstanding` の記帳）。
        //   「1 要求 = 1 フレーム」で件数を照合しない
        // - `focused` は必須項目である。egui はフォーカスがあるときだけキャレット点滅の
        //   repaint を出すため、非フォーカスの行を混ぜると「眠っている」に見える
        if std::env::var_os("SNOTRA_EGUI_REPAINT_TRACE").is_some() {
            let now = std::time::Instant::now();
            // 初回は None ゆえ NaN（0 と紛れない）。hide をまたぐと巨大値になる（眠っていた証拠）。
            let since = self
                .repaint_trace_prev
                .replace(now)
                .map(|prev| (now - prev).as_secs_f64() * 1000.0)
                .unwrap_or(f64::NAN);
            eprintln!(
                "SNOTRA_EGUI_REPAINT window={} focused={} since_prev_ms={since:.1} causes={}",
                self.window.label(),
                self.context.input(|i| i.focused),
                crate::repaint::format_repaint_causes(&self.context.repaint_causes()),
            );
        }
        self.handle_platform_output(&output.platform_output);
        // 描画失敗は能動再試行（不変条件⑤）。「次 RedrawRequested 待ち」は egui が repaint を
        // 求めなければ停止するため、失敗時に自ら repaint を要求し、上限超過で fatal にする。
        match self
            .renderer
            .as_mut()
            .expect("renderer installed by attach_pending_windows")
            .paint(&self.context, output, size, frame.clear_color)
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

    fn handle_platform_output(&mut self, output: &egui::PlatformOutput) {
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

        // **値が変わったときだけ OS へ書く。** tao の `set_cursor_icon` は窓に紐づかない
        // `SetCursor` を直接撃つ（tao 0.35.3 `platform_impl/windows/window.rs:460-466`）——
        // 最後に呼んだ者が勝ち、マウス静止中は `WM_SETCURSOR` が来ないので OS の復元も
        // 入らない。毎フレーム無条件に撃つと、ポインタを持つ窓（Text）と持たない窓
        // （Default）が交互に上書きし合ってカーソルが点滅する（#628 の計測中に実機で
        // 発見。main 1194 / results 1339 フレームが撃ち合っていた）。
        //
        // 変化検出は egui 側の値で行う（`tauri::CursorIcon` の等値性に依存しない）。
        // 残余: 別窓のアイコンが実際に変わった瞬間だけ、静止中のポインタ下のカーソルが
        // 一度ずれうる。マウスを動かせば `WM_SETCURSOR` で窓ごとの値へ復帰する。
        if self.applied_cursor != Some(output.cursor_icon) {
            self.applied_cursor = Some(output.cursor_icon);
            if let Some(cursor) = cursor_icon(output.cursor_icon) {
                let _ = self.window.set_cursor_icon(cursor);
            }
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
