use std::time::{Duration, Instant};

use tauri_runtime_wry::tao::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent},
    keyboard::ModifiersState,
};

pub(crate) struct InputState {
    raw: egui::RawInput,
    native_pixels_per_point: f32,
    pointer_pos: egui::Pos2,
    started_at: Instant,
    /// 現在の修飾キー状態。**egui 0.36 で `RawInput::modifiers` が消え、保持側がこちらへ移った**
    /// （upstream の `Remove Modifiers from RawInput and make it a egui::Event`）。egui へは
    /// `Event::ModifiersChanged` として流しつつ、`PointerButton` / `MouseWheel` / `Key` の
    /// `modifiers` にはここの値を載せる——**両方要る**。イベントだけでは各イベントに載る値が
    /// 埋まらず、保持だけでは egui 側の `InputState::modifiers` が更新されない。
    modifiers: egui::Modifiers,
    /// `take()` を通った回数＝egui へフレームを渡した回数（診断のみ・#872/#936）。
    frame_count: u64,
    /// 直近で `take` 行を出した時刻。**心拍を間引くためだけに持つ**——runner では
    /// stderr 1 行が 17〜56ms かかると実測されており（run 30963445957）、全フレームに
    /// 出すと計器が系を乱す側へ回る。
    last_take_trace: Option<Instant>,
}

/// フレームの心拍を出す最短間隔（#872/#936）。**入力を積んだフレームはこの間引きを受けない**
/// ——落としてよいのは「何も起きていないことの証拠」だけである。
const TAKE_TRACE_HEARTBEAT: Duration = Duration::from_millis(100);

/// 打鍵の到達計器が有効か（`SNOTRA_EGUI_INPUT_TRACE`・#872/#936）。
/// 判定を `input_trace` の内側だけに置くと、呼び出し側が `format!` の割り当てを
/// 無条件に払う。呼ぶ前に問えるよう外へ出す。
pub(crate) fn input_trace_enabled() -> bool {
    // **一度だけ読む**。この述語はウィンドウイベントごと（マウス移動を含む）とフレームごとに問われる
    // ため、env の読み出し（`crate::env::trace_hatch_enabled` の中の `var_os`）の割り当てを
    // 毎回払うと、計器を**切っていても**出荷バイナリのホットパスに費用が残る。
    // キャッシュの形は `src-tauri/src/trace.rs` の `trace_enabled` と同じ。
    //
    // **キャッシュを持つのはこの 1 箇所だけである**（残り 6 箇所は呼び出しごとに読む）。
    // 実害があるのは「実行中に env が変わる」場合だけで、Windows のプロセス env では通常
    // 起きない。ここだけ持つ理由は上の頻度であって、他が持たないことの正当化ではない。
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| crate::env::trace_hatch_enabled("SNOTRA_EGUI_INPUT_TRACE"))
}

/// 到達した入力を 1 件 1 行で残す（#872/#936 の型 B＝打鍵の喪失を、層ごとに割るための計器）。
///
/// **`[trace]` を名乗らない。** あちらの `seq` は `src-tauri/src/trace.rs` の単一 `AtomicU64` が
/// 持つ全順序であり、この crate からは触れない。seq を欠いた `[trace]` 行は
/// `SnotraTraceInvariants.psm1` が「捨てた行」に数えて smoke を degrade させる（`$seq` が
/// null の行は順序にも区間にも載せられないため）。既存の `SNOTRA_EGUI_WAKE_TRACE` と同じ
/// 流儀で独自の接頭辞を使い、**`ts_ms`（epoch ms）は `[trace]` と同じ時計**なので
/// 事後に 1 本の時系列へ並べられる。
///
/// **文字の内容は出さない**——出すのは件数だけである（`on_input_changed` が入力文字列を
/// 残さず文字数だけを残すのと同じ方針）。物理キーコードは出す: 「どのキーが消えたか」が
/// この計器の問いそのもので、それ無しでは 5 打鍵のうち何番目が落ちたか分からない。
pub(crate) fn input_trace(kind: &str, detail: &str) {
    if !input_trace_enabled() {
        return;
    }
    let ts_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    eprintln!("SNOTRA_EGUI_INPUT {kind} ts_ms={ts_ms} {detail}");
}

impl InputState {
    pub(crate) fn new(native_pixels_per_point: f32) -> Self {
        Self {
            raw: egui::RawInput::default(),
            native_pixels_per_point,
            pointer_pos: egui::Pos2::ZERO,
            started_at: Instant::now(),
            modifiers: egui::Modifiers::default(),
            frame_count: 0,
            last_take_trace: None,
        }
    }

    /// live な size/ppp（`Window::inner_size`/`scale_factor`）を毎フレーム受け取る。
    /// イベント駆動で保持していた値との乖離（#532 SU1）を絶たすため、screen_rect と
    /// native_pixels_per_point はここで渡された値だけから作る。
    pub(crate) fn take(
        &mut self,
        max_texture_side: usize,
        size: PhysicalSize<u32>,
        native_pixels_per_point: f32,
    ) -> egui::RawInput {
        // **フレームが走ったこと自体を残す**（#872/#936 の最後の分岐）。ここより下流の
        // trace（`egui_input:changed` 等）は「入力が実った」ときしか出ないため、
        // 「フレームが 1 枚も走っていない」と「走ったが TextEdit が受け取らない」が
        // 同じ沈黙へ潰れている。**この 1 行だけが両者を分ける。**
        //
        // **全フレームには出さない**——runner では stderr 1 行が 17〜56ms かかると実測した
        // （run 30963445957・隣接する 2 行の間隔）。全フレームに出すと計器が系を乱す側へ回る。
        // 入力を積んだフレームは必ず出し、それ以外は 100ms ごとの心拍に間引く：
        // 「走っていない」は**行の不在**ではなく**心拍の途切れ**として見えるので、間引いても
        // 判定は壊れない。
        self.frame_count += 1;
        if input_trace_enabled() {
            let now = Instant::now();
            let pending = self.raw.events.len();
            let due = self
                .last_take_trace
                .is_none_or(|last| now.duration_since(last) >= TAKE_TRACE_HEARTBEAT);
            if pending > 0 || due {
                self.last_take_trace = Some(now);
                input_trace(
                    "take",
                    &format!(
                        "frame={} events={pending} focused={}",
                        self.frame_count, self.raw.focused
                    ),
                );
            }
        }

        self.native_pixels_per_point = native_pixels_per_point;
        let size_points = egui::vec2(
            size.width as f32 / native_pixels_per_point,
            size.height as f32 / native_pixels_per_point,
        );
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, size_points);

        self.raw.screen_rect = Some(screen_rect);
        self.raw.max_texture_side = Some(max_texture_side);
        self.raw.time = Some(self.started_at.elapsed().as_secs_f64());
        // **イベント駆動ゆえ「次フレームは vsync 後に来る」前提を持たない**（#628）。
        // egui は `request_repaint_after(d)` を `d - predicted_dt` へ切り詰める
        // （`context.rs:148-151`）。既定値 1/60 秒のままだと、`d < 16.7ms` の要求が
        // ZERO へ飽和して「即時再描画」に化け、次パスがさらに小さい `d` を要求して
        // 再び飽和する——キャレット点滅の遷移ごとに約 26ms スピンする（#628 実測:
        // 遷移 1 回につき平均 7.8 フレーム・可視アイドルが 2fps ではなく 11.5fps）。
        // 0 を渡して要求どおりの時刻に起こす（本 runtime は先取りせず、起きてから
        // 約 1.4ms で描く）。egui 側の他の用途はアニメーションの半フレーム先読みと
        // sleep 明けの `stable_dt` フォールバックのみで、いずれも `at_most()` 付きの
        // 乗算ゆえ 0 で壊れない（除算は無い）。
        self.raw.predicted_dt = 0.0;

        let viewport = self
            .raw
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default();
        viewport.native_pixels_per_point = Some(native_pixels_per_point);
        viewport.inner_rect = Some(screen_rect);
        viewport.focused = Some(self.raw.focused);

        self.raw.take()
    }

    pub(crate) fn native_pixels_per_point(&self) -> f32 {
        self.native_pixels_per_point
    }

    /// subclass から届いた 1 件を egui のイベントへ積む（#1289）。
    pub(crate) fn push_native_input(&mut self, input: crate::keyboard::NativeInput) {
        use crate::keyboard::NativeInput;
        match input {
            NativeInput::Ime(event) => self.raw.events.push(egui::Event::Ime(event)),
            NativeInput::Key {
                key,
                physical_key,
                pressed,
                repeat,
            } => self.push_key(key, physical_key, pressed, repeat),
            NativeInput::Text(text) => self
                .raw
                .events
                .push(egui::Event::Ime(egui::ImeEvent::Commit(text))),
        }
    }

    /// キー 1 件を積む。**修飾はここで載せる**。載せるのは tao の `ModifiersChanged` の保持値
    /// （`self.modifiers`）で、AltGr のときは Ctrl・Alt が外れている（tao の `get_agnostic_mods`）。
    /// subclass の `GetKeyState` で取ると、AltGr+C/X/V が Copy/Cut/Paste になる。
    fn push_key(
        &mut self,
        key: egui::Key,
        physical_key: Option<egui::Key>,
        pressed: bool,
        repeat: bool,
    ) {
        if pressed && self.modifiers.command {
            match key {
                egui::Key::C => {
                    self.raw.events.push(egui::Event::Copy);
                    return;
                }
                egui::Key::X => {
                    self.raw.events.push(egui::Event::Cut);
                    return;
                }
                egui::Key::V => {
                    if let Ok(mut clipboard) = arboard::Clipboard::new()
                        && let Ok(text) = clipboard.get_text()
                        && !text.is_empty()
                    {
                        self.raw
                            .events
                            .push(egui::Event::Paste(text.replace("\r\n", "\n")));
                    }
                    return;
                }
                _ => {}
            }
        }

        self.raw.events.push(egui::Event::Key {
            key,
            physical_key,
            pressed,
            repeat,
            modifiers: self.modifiers,
        });
    }

    pub(crate) fn on_window_event(&mut self, event: &WindowEvent<'_>) -> bool {
        match event {
            // 実際の size/ppp は render() が毎フレーム window から live に取る
            // （input.rs Step 1 #532 SU1）。ここでは repaint 要求のトリガーとしてのみ残す。
            WindowEvent::Resized(_) => {}
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                // ポインタ座標変換（物理px→point）に使うため live 同期は残す。
                self.native_pixels_per_point = *scale_factor as f32;
            }
            WindowEvent::Focused(focused) => {
                // focus 獲得時の抑止（#927）はここでは扱わない——subclass が `WM_SETFOCUS` /
                // `WM_KILLFOCUS` で持つ（`keyboard.rs` の `KeyboardTranslator`）。
                self.raw.focused = *focused;
                self.raw.events.push(egui::Event::WindowFocused(*focused));
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                // **保持とイベント送出の両方を行う**（egui 0.36 で `RawInput::modifiers` が
                // 消えた・`self.modifiers` の doc）。片方だけでは、各イベントに載る値か
                // egui 側の `InputState::modifiers` のどちらかが更新されない。
                self.modifiers = modifiers_from_tao(*modifiers);
                self.raw
                    .events
                    .push(egui::Event::ModifiersChanged(self.modifiers));
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer_pos =
                    physical_position_to_points(*position, self.native_pixels_per_point);
                self.raw
                    .events
                    .push(egui::Event::PointerMoved(self.pointer_pos));
            }
            WindowEvent::CursorLeft { .. } => {
                self.raw.events.push(egui::Event::PointerGone);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(button) = pointer_button(*button) {
                    self.raw.events.push(egui::Event::PointerButton {
                        pos: self.pointer_pos,
                        button,
                        pressed: *state == ElementState::Pressed,
                        modifiers: self.modifiers,
                    });
                }
            }
            WindowEvent::MouseWheel { delta, phase, .. } => {
                let (unit, delta) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        (egui::MouseWheelUnit::Line, egui::vec2(*x, *y))
                    }
                    MouseScrollDelta::PixelDelta(position) => (
                        egui::MouseWheelUnit::Point,
                        egui::vec2(position.x as f32, position.y as f32)
                            / self.native_pixels_per_point,
                    ),
                    _ => return false,
                };
                self.raw.events.push(egui::Event::MouseWheel {
                    unit,
                    delta,
                    phase: touch_phase(*phase),
                    // egui-winit 0.37 も macOS 以外では `Unknown` を渡す（tao は駆動元を伝えない）。
                    source: egui::MouseWheelSource::Unknown,
                    modifiers: self.modifiers,
                });
            }
            // **キーボード由来は tao から受け取らない**（#1289）。打鍵・文字・IME 確定はすべて
            // `windows_ime.rs` の subclass が Win32 メッセージから作り、`push_native_input` で積む。
            // tao の `KeyboardInput` は従来どおり発火するが、egui へ渡すと二重に入る。到着は
            // `runtime.rs` の `rx_key` / `rx_text` が引き当ての前に残す。`ReceivedImeText` は
            // Windows では 0 件が正常（subclass が `WM_IME_ENDCOMPOSITION` を tao へ通さない）で、
            // `rx_text` に現れたらその抑止が破れている tripwire である。
            WindowEvent::KeyboardInput { .. } | WindowEvent::ReceivedImeText(_) => return false,
            WindowEvent::CloseRequested => {
                self.raw
                    .viewports
                    .entry(egui::ViewportId::ROOT)
                    .or_default()
                    .events
                    .push(egui::ViewportEvent::Close);
            }
            _ => return false,
        }

        true
    }
}

fn physical_position_to_points(position: PhysicalPosition<f64>, scale_factor: f32) -> egui::Pos2 {
    egui::pos2(
        position.x as f32 / scale_factor,
        position.y as f32 / scale_factor,
    )
}

fn pointer_button(button: MouseButton) -> Option<egui::PointerButton> {
    match button {
        MouseButton::Left => Some(egui::PointerButton::Primary),
        MouseButton::Right => Some(egui::PointerButton::Secondary),
        MouseButton::Middle => Some(egui::PointerButton::Middle),
        MouseButton::Other(1) => Some(egui::PointerButton::Extra1),
        MouseButton::Other(2) => Some(egui::PointerButton::Extra2),
        _ => None,
    }
}

fn touch_phase(phase: TouchPhase) -> egui::TouchPhase {
    match phase {
        TouchPhase::Started => egui::TouchPhase::Start,
        TouchPhase::Moved => egui::TouchPhase::Move,
        TouchPhase::Ended => egui::TouchPhase::End,
        TouchPhase::Cancelled => egui::TouchPhase::Cancel,
        _ => egui::TouchPhase::Cancel,
    }
}

pub fn modifiers_from_tao(modifiers: ModifiersState) -> egui::Modifiers {
    let ctrl = modifiers.control_key();
    let mac_cmd = cfg!(target_os = "macos") && modifiers.super_key();

    egui::Modifiers {
        alt: modifiers.alt_key(),
        ctrl,
        shift: modifiers.shift_key(),
        mac_cmd,
        command: if cfg!(target_os = "macos") {
            mac_cmd
        } else {
            ctrl
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_preserve_windows_shortcut_semantics() {
        let modifiers = modifiers_from_tao(
            ModifiersState::CONTROL | ModifiersState::SHIFT | ModifiersState::ALT,
        );

        assert!(modifiers.ctrl);
        assert!(modifiers.command);
        assert!(modifiers.shift);
        assert!(modifiers.alt);
        assert!(!modifiers.mac_cmd);
    }

    fn native_key(key: egui::Key, pressed: bool) -> crate::keyboard::NativeInput {
        crate::keyboard::NativeInput::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat: false,
        }
    }

    fn drain_events(input: &mut InputState) -> Vec<egui::Event> {
        input.take(4096, PhysicalSize::new(100, 100), 1.0).events
    }

    /// #1289: subclass のキーには修飾が無く、drain の時点で tao の `ModifiersChanged` の保持値を載せる。
    #[test]
    fn native_key_carries_modifiers_held_from_tao() {
        let mut input = InputState::new(1.0);
        input.on_window_event(&WindowEvent::ModifiersChanged(ModifiersState::SHIFT));
        drain_events(&mut input);

        input.push_native_input(native_key(egui::Key::A, true));
        let events = drain_events(&mut input);
        assert_eq!(
            events,
            vec![egui::Event::Key {
                key: egui::Key::A,
                physical_key: Some(egui::Key::A),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::SHIFT,
            }]
        );
    }

    /// Ctrl+C / Ctrl+X は Copy / Cut になり、Key としては積まない（#1289 で subclass へ移す前の tao 経路と同じ）。
    #[test]
    fn command_c_and_x_become_copy_and_cut() {
        let mut input = InputState::new(1.0);
        input.on_window_event(&WindowEvent::ModifiersChanged(ModifiersState::CONTROL));
        drain_events(&mut input);

        input.push_native_input(native_key(egui::Key::C, true));
        input.push_native_input(native_key(egui::Key::X, true));
        assert_eq!(
            drain_events(&mut input),
            vec![egui::Event::Copy, egui::Event::Cut]
        );
    }

    /// AltGr のとき tao は Ctrl・Alt を外した修飾を送る（`get_agnostic_mods`）。その状態の C は
    /// Copy にならない——subclass の `GetKeyState` で修飾を取らない理由（`ADR-keyboard-input-owned-by-subclass`）。
    /// AltGr の除去そのものは tao の中にあり、ここでは再現できない。固定するのは「tao が送った
    /// 修飾をそのまま載せる」ほうである。
    #[test]
    fn altgr_c_is_not_copy() {
        let mut input = InputState::new(1.0);
        input.on_window_event(&WindowEvent::ModifiersChanged(ModifiersState::empty()));
        drain_events(&mut input);

        input.push_native_input(native_key(egui::Key::C, true));
        assert_eq!(
            drain_events(&mut input),
            vec![egui::Event::Key {
                key: egui::Key::C,
                physical_key: Some(egui::Key::C),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            "tao が外した Ctrl・Alt を、キーに載せる修飾でも外したままにする"
        );
    }

    #[test]
    fn native_text_and_ime_become_ime_events() {
        let mut input = InputState::new(1.0);
        input.push_native_input(crate::keyboard::NativeInput::Text("a".to_owned()));
        input.push_native_input(crate::keyboard::NativeInput::Ime(egui::ImeEvent::Commit(
            "日本".to_owned(),
        )));
        assert_eq!(
            drain_events(&mut input),
            vec![
                egui::Event::Ime(egui::ImeEvent::Commit("a".to_owned())),
                egui::Event::Ime(egui::ImeEvent::Commit("日本".to_owned())),
            ]
        );
    }

    #[test]
    fn take_uses_live_size_and_ppp_not_stored_event_values() {
        // イベント駆動で古い値を持たせる。
        let mut input = InputState::new(1.0);
        // live に新しい size/ppp を渡すと screen_rect は live 値から作られる。
        let raw = input.take(4096, PhysicalSize::new(200, 400), 2.0);
        let rect = raw.screen_rect.expect("screen_rect");
        // 200x400 physical / ppp 2.0 = 100x200 points。
        assert_eq!(rect.width(), 100.0);
        assert_eq!(rect.height(), 200.0);
        let vp = raw.viewports.get(&egui::ViewportId::ROOT).unwrap();
        assert_eq!(vp.native_pixels_per_point, Some(2.0));
    }

    #[test]
    fn take_reports_zero_predicted_dt_so_repaint_delays_are_not_truncated() {
        // 既定値（1/60 秒）に戻ると egui が短い repaint 予約を ZERO へ飽和させ、
        // 即時再描画のスピンになる（#628: キャレット点滅の遷移ごとに約 26ms・
        // 可視アイドルが 2fps → 11.5fps）。値そのものを固定して回帰を検出する。
        let mut input = InputState::new(1.0);
        let raw = input.take(4096, PhysicalSize::new(100, 100), 1.0);
        assert_eq!(
            raw.predicted_dt, 0.0,
            "先取りしない（要求どおりの時刻に起きる）"
        );
        // 2 フレーム目以降も持ち越しで戻らないこと（RawInput::take は値を保持する）。
        let raw2 = input.take(4096, PhysicalSize::new(100, 100), 1.0);
        assert_eq!(raw2.predicted_dt, 0.0);
    }
}
