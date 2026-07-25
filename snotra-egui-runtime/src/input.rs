use std::time::Instant;

use tauri_runtime_wry::tao::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent},
    keyboard::{Key, ModifiersState},
};

pub(crate) struct InputState {
    raw: egui::RawInput,
    native_pixels_per_point: f32,
    pointer_pos: egui::Pos2,
    started_at: Instant,
}

impl InputState {
    pub(crate) fn new(native_pixels_per_point: f32) -> Self {
        Self {
            raw: egui::RawInput::default(),
            native_pixels_per_point,
            pointer_pos: egui::Pos2::ZERO,
            started_at: Instant::now(),
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

    pub(crate) fn push_ime_event(&mut self, event: egui::ImeEvent) {
        self.raw.events.push(egui::Event::Ime(event));
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
                self.raw.focused = *focused;
                self.raw.events.push(egui::Event::WindowFocused(*focused));
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.raw.modifiers = modifiers_from_tao(*modifiers);
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
                        modifiers: self.raw.modifiers,
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
                    modifiers: self.raw.modifiers,
                });
            }
            WindowEvent::KeyboardInput { event, .. } => self.on_keyboard_event(event),
            WindowEvent::ReceivedImeText(text) => {
                if let Some(event) = committed_text_event(text) {
                    self.raw.events.push(event);
                }
            }
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

    fn on_keyboard_event(&mut self, event: &tauri_runtime_wry::tao::event::KeyEvent) {
        let pressed = event.state == ElementState::Pressed;
        let active_key =
            key_from_tao(&event.logical_key).or_else(|| key_from_key_code(event.physical_key));

        if let Some(key) = active_key {
            if pressed && self.raw.modifiers.command {
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
                physical_key: key_from_key_code(event.physical_key),
                pressed,
                repeat: event.repeat,
                modifiers: self.raw.modifiers,
            });
        }

        // Tao emits normal WM_CHAR text and IME commits through ReceivedImeText.
        // Routing KeyEvent::text too would insert every printable character twice.
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

fn is_printable_char(character: char) -> bool {
    !character.is_ascii_control() && character != '\u{7f}'
}

fn committed_text_event(text: &str) -> Option<egui::Event> {
    (!text.is_empty() && text.chars().all(is_printable_char))
        .then(|| egui::Event::Ime(egui::ImeEvent::Commit(text.to_owned())))
}

fn key_from_key_code(key: tauri_runtime_wry::tao::keyboard::KeyCode) -> Option<egui::Key> {
    use egui::Key;
    use tauri_runtime_wry::tao::keyboard::KeyCode;

    Some(match key {
        KeyCode::KeyA => Key::A,
        KeyCode::KeyB => Key::B,
        KeyCode::KeyC => Key::C,
        KeyCode::KeyD => Key::D,
        KeyCode::KeyE => Key::E,
        KeyCode::KeyF => Key::F,
        KeyCode::KeyG => Key::G,
        KeyCode::KeyH => Key::H,
        KeyCode::KeyI => Key::I,
        KeyCode::KeyJ => Key::J,
        KeyCode::KeyK => Key::K,
        KeyCode::KeyL => Key::L,
        KeyCode::KeyM => Key::M,
        KeyCode::KeyN => Key::N,
        KeyCode::KeyO => Key::O,
        KeyCode::KeyP => Key::P,
        KeyCode::KeyQ => Key::Q,
        KeyCode::KeyR => Key::R,
        KeyCode::KeyS => Key::S,
        KeyCode::KeyT => Key::T,
        KeyCode::KeyU => Key::U,
        KeyCode::KeyV => Key::V,
        KeyCode::KeyW => Key::W,
        KeyCode::KeyX => Key::X,
        KeyCode::KeyY => Key::Y,
        KeyCode::KeyZ => Key::Z,
        _ => return None,
    })
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

pub fn key_from_tao(key: &Key<'_>) -> Option<egui::Key> {
    use egui::Key as EguiKey;

    match key {
        Key::ArrowDown => Some(EguiKey::ArrowDown),
        Key::ArrowLeft => Some(EguiKey::ArrowLeft),
        Key::ArrowRight => Some(EguiKey::ArrowRight),
        Key::ArrowUp => Some(EguiKey::ArrowUp),
        Key::Escape => Some(EguiKey::Escape),
        Key::Tab => Some(EguiKey::Tab),
        Key::Backspace => Some(EguiKey::Backspace),
        Key::Enter => Some(EguiKey::Enter),
        Key::Insert => Some(EguiKey::Insert),
        Key::Delete => Some(EguiKey::Delete),
        Key::Home => Some(EguiKey::Home),
        Key::End => Some(EguiKey::End),
        Key::PageUp => Some(EguiKey::PageUp),
        Key::PageDown => Some(EguiKey::PageDown),
        Key::Space => Some(EguiKey::Space),
        Key::F1 => Some(EguiKey::F1),
        Key::F2 => Some(EguiKey::F2),
        Key::F3 => Some(EguiKey::F3),
        Key::F4 => Some(EguiKey::F4),
        Key::F5 => Some(EguiKey::F5),
        Key::F6 => Some(EguiKey::F6),
        Key::F7 => Some(EguiKey::F7),
        Key::F8 => Some(EguiKey::F8),
        Key::F9 => Some(EguiKey::F9),
        Key::F10 => Some(EguiKey::F10),
        Key::F11 => Some(EguiKey::F11),
        Key::F12 => Some(EguiKey::F12),
        Key::F13 => Some(EguiKey::F13),
        Key::F14 => Some(EguiKey::F14),
        Key::F15 => Some(EguiKey::F15),
        Key::F16 => Some(EguiKey::F16),
        Key::F17 => Some(EguiKey::F17),
        Key::F18 => Some(EguiKey::F18),
        Key::F19 => Some(EguiKey::F19),
        Key::F20 => Some(EguiKey::F20),
        Key::Character(value) => key_from_character(value),
        _ => None,
    }
}

fn key_from_character(value: &str) -> Option<egui::Key> {
    use egui::Key;

    Some(match value.to_ascii_lowercase().as_str() {
        "0" => Key::Num0,
        "1" => Key::Num1,
        "2" => Key::Num2,
        "3" => Key::Num3,
        "4" => Key::Num4,
        "5" => Key::Num5,
        "6" => Key::Num6,
        "7" => Key::Num7,
        "8" => Key::Num8,
        "9" => Key::Num9,
        "a" => Key::A,
        "b" => Key::B,
        "c" => Key::C,
        "d" => Key::D,
        "e" => Key::E,
        "f" => Key::F,
        "g" => Key::G,
        "h" => Key::H,
        "i" => Key::I,
        "j" => Key::J,
        "k" => Key::K,
        "l" => Key::L,
        "m" => Key::M,
        "n" => Key::N,
        "o" => Key::O,
        "p" => Key::P,
        "q" => Key::Q,
        "r" => Key::R,
        "s" => Key::S,
        "t" => Key::T,
        "u" => Key::U,
        "v" => Key::V,
        "w" => Key::W,
        "x" => Key::X,
        "y" => Key::Y,
        "z" => Key::Z,
        _ => return None,
    })
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

    #[test]
    fn navigation_keys_are_mapped() {
        assert_eq!(key_from_tao(&Key::ArrowUp), Some(egui::Key::ArrowUp));
        assert_eq!(key_from_tao(&Key::Enter), Some(egui::Key::Enter));
        assert_eq!(key_from_tao(&Key::Escape), Some(egui::Key::Escape));
    }

    #[test]
    fn latin_shortcut_characters_are_mapped_case_insensitively() {
        assert_eq!(key_from_tao(&Key::Character("a")), Some(egui::Key::A));
        assert_eq!(key_from_tao(&Key::Character("V")), Some(egui::Key::V));
    }

    #[test]
    fn received_ime_text_filters_control_characters() {
        assert_eq!(
            committed_text_event("日本語"),
            Some(egui::Event::Ime(egui::ImeEvent::Commit(
                "日本語".to_owned()
            )))
        );
        assert_eq!(committed_text_event("\r"), None);
        assert_eq!(committed_text_event(""), None);
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
        assert_eq!(raw.predicted_dt, 0.0, "先取りしない（要求どおりの時刻に起きる）");
        // 2 フレーム目以降も持ち越しで戻らないこと（RawInput::take は値を保持する）。
        let raw2 = input.take(4096, PhysicalSize::new(100, 100), 1.0);
        assert_eq!(raw2.predicted_dt, 0.0);
    }
}
