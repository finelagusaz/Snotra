use std::collections::HashSet;
use std::time::{Duration, Instant};

use tauri_runtime_wry::tao::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent},
    keyboard::{Key, KeyCode, ModifiersState},
};

pub(crate) struct InputState {
    raw: egui::RawInput,
    native_pixels_per_point: f32,
    pointer_pos: egui::Pos2,
    started_at: Instant,
    /// focus を獲得した瞬間に既に押されていたキー（#927）。**press を release まで抑止する**
    /// ——判定は `admit_key`、そこに機序と一次資料を書いた。
    ///
    /// **`Focused(false)` で全消去し、focus セッションを越えない。** 抑止したキーの release が
    /// 届かない経路（release 時に窓が focus を失っている）で抑止が持ち越されると、以後
    /// Escape が永久に効かなくなる——fail-closed の側へ倒れるため、消去点を持つことが
    /// 抑止そのものと同じだけ重要である。
    held_since_focus_gain: HashSet<KeyCode>,
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
    // **一度だけ読む**。この述語は窓イベントごと（マウス移動を含む）とフレームごとに問われる
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

/// このキーイベントを egui へ渡すか（#927）。`true` = 渡す。`held` は副作用として更新する。
///
/// **tao は `WM_SETFOCUS` を受けたとき、その瞬間に押されている全キーの `Pressed` を合成する**
/// （`tao-0.35.3/src/platform_impl/windows/keyboard.rs:87-93` の `get_async_kbd_state()` →
/// `synthesize_kbd_state(ElementState::Pressed, …)`）。設定ウィンドウを Escape の **down** で
/// 閉じると、本体が focus を取り戻した瞬間にこの合成 press が届き、Escape ラダーが走って
/// **1 回の押下で 2 つの窓が閉じる**（#927 の症状。実測: 本体が受けた press は `synthetic=true`）。
///
/// ゆえに **focus 獲得時に押されていたキーは、release されるまで press を渡さない**。
/// 抑止は Escape に限らない——`Z` を押しっぱなしで設定を閉じると検索欄へ合成 press が届く（実測）。
///
/// **非合成の press も抑止対象に含める**のは、物理キーボードのオートリピートが focus 移行を
/// 跨いで新しい前面窓へ届く可能性を塞ぐためである（`keybd_event` はリピートを生まないので
/// 注入では測れない。#927 の (A)）。
///
/// **release は常に渡す**——落とすと egui の `keys_down` に押しっぱなしが残る。egui へ渡らな
/// かった press に対応する release が渡ることは無害である（`key_released` / `keys_down` を読む
/// 消費者はこの crate にも `src-tauri/src/egui_shell/` にも無い・grep 実測）。
fn admit_key(
    is_synthetic: bool,
    pressed: bool,
    physical: KeyCode,
    held: &mut HashSet<KeyCode>,
) -> bool {
    if !pressed {
        held.remove(&physical);
        return true;
    }
    if is_synthetic {
        held.insert(physical);
        return false;
    }
    !held.contains(&physical)
}

impl InputState {
    pub(crate) fn new(native_pixels_per_point: f32) -> Self {
        Self {
            raw: egui::RawInput::default(),
            native_pixels_per_point,
            pointer_pos: egui::Pos2::ZERO,
            started_at: Instant::now(),
            held_since_focus_gain: HashSet::new(),
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
                // **focus を失ったら抑止を全消去する**（#927）。tao は `WM_KILLFOCUS` でも
                // 合成 release を先に送る（`tao-0.35.3/src/platform_impl/windows/event_loop.rs`
                // の `lose_active_focus` は `keyboard_callback` より後の `match msg` で走る）ので、
                // 到着順に関わらずここで空になる。**`Focused(true)` では消さない**——合成 press は
                // `Focused(true)` より**先**に届くため（同 `event_loop.rs` の `keyboard_callback`
                // が `match msg` の前にある）、ここで消すと直前に立てた抑止を自分で捨てる。
                if !*focused {
                    self.held_since_focus_gain.clear();
                }
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
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                let pressed = event.state == ElementState::Pressed;
                if !admit_key(
                    *is_synthetic,
                    pressed,
                    event.physical_key,
                    &mut self.held_since_focus_gain,
                ) {
                    // **落とした側も残す**（#872/#936）——`push_key` はここより下流ゆえ、
                    // 抑止した打鍵は残さなければ沈黙になる。
                    if input_trace_enabled() {
                        input_trace(
                            "drop_key",
                            &format!(
                                "state={} physical={:?} synthetic={is_synthetic} reason=held_since_focus_gain",
                                if pressed { "down" } else { "up" },
                                event.physical_key,
                            ),
                        );
                    }
                    return true;
                }
                self.on_keyboard_event(event)
            }
            WindowEvent::ReceivedImeText(text) => {
                let committed = committed_text_event(text);
                // **落とした側も残す**（#872/#936）。`committed_text_event` は制御文字と空を
                // 弾くので、「届いたのに egui へ渡らなかった」経路がここにも在る。
                input_trace(
                    "push_text",
                    &format!(
                        "chars={} committed={}",
                        text.chars().count(),
                        committed.is_some()
                    ),
                );
                if let Some(event) = committed {
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

        // **`mapped` を残す**（#872/#936）: `key_from_tao` / `key_from_key_code` が両方 None なら
        // この打鍵は egui へ 1 件も積まれずに消える。「届いたが実らなかった」の最も内側の経路で
        // あり、外からは沈黙としか見えない。
        if input_trace_enabled() {
            input_trace(
                "push_key",
                &format!(
                    "state={} physical={:?} repeat={} mapped={}",
                    if pressed { "down" } else { "up" },
                    event.physical_key,
                    event.repeat,
                    active_key.is_some()
                ),
            );
        }

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

    /// #927: focus 獲得時の合成 press は egui へ渡さず、抑止対象として記録する。
    /// **この 1 件が「1 回の押下で設定窓と本体の 2 つが閉じる」を止めている唯一の判定である。**
    #[test]
    fn synthetic_press_at_focus_gain_is_not_admitted() {
        let mut held = HashSet::new();

        assert!(!admit_key(true, true, KeyCode::Escape, &mut held));
        assert!(held.contains(&KeyCode::Escape));
    }

    /// #927 (A): 抑止中は**非合成**の press も渡さない（物理オートリピートが focus 移行を
    /// 跨いで届く経路。`keybd_event` では生成できないため注入では測れない）。
    #[test]
    fn physical_repeat_while_held_is_not_admitted() {
        let mut held = HashSet::new();
        admit_key(true, true, KeyCode::Escape, &mut held);

        assert!(!admit_key(false, true, KeyCode::Escape, &mut held));
    }

    /// release は渡し、抑止を解く。**渡す側も外す側も落とせない**——渡さなければ egui の
    /// `keys_down` に押しっぱなしが残り、外さなければ次の press が永久に届かない。
    #[test]
    fn release_is_admitted_and_lifts_suppression() {
        let mut held = HashSet::new();
        admit_key(true, true, KeyCode::Escape, &mut held);

        assert!(admit_key(false, false, KeyCode::Escape, &mut held));
        assert!(!held.contains(&KeyCode::Escape));
    }

    /// **抑止は 1 回の押下で終わる**（fail-closed の防止）: release 後の press は通常どおり渡る。
    /// これが破れると Escape が永久に効かなくなる——症状が #927 より重い。
    #[test]
    fn press_after_release_is_admitted_again() {
        let mut held = HashSet::new();
        admit_key(true, true, KeyCode::Escape, &mut held);
        admit_key(false, false, KeyCode::Escape, &mut held);

        assert!(admit_key(false, true, KeyCode::Escape, &mut held));
    }

    /// 抑止はキーごとである（Escape を抑止していても他のキーの打鍵は届く）。
    #[test]
    fn other_keys_are_unaffected_by_suppression() {
        let mut held = HashSet::new();
        admit_key(true, true, KeyCode::Escape, &mut held);

        assert!(admit_key(false, true, KeyCode::KeyA, &mut held));
    }

    /// focus 喪失時の合成 release（tao が `WM_KILLFOCUS` で作る）も渡す——押しっぱなしのまま
    /// focus を失ったキーを egui 側で開放するのがこのイベントの役目である。
    #[test]
    fn synthetic_release_is_admitted() {
        let mut held = HashSet::new();
        admit_key(true, true, KeyCode::Escape, &mut held);

        assert!(admit_key(true, false, KeyCode::Escape, &mut held));
        assert!(held.is_empty());
    }

    /// #927: 抑止は focus セッションを越えない。**release が届かない経路**（release 時に窓が
    /// focus を失っている）で持ち越されると、以後 Escape が効かなくなる。
    #[test]
    fn losing_focus_clears_suppression() {
        let mut input = InputState::new(1.0);
        input.held_since_focus_gain.insert(KeyCode::Escape);

        input.on_window_event(&WindowEvent::Focused(false));

        assert!(input.held_since_focus_gain.is_empty());
    }

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
        assert_eq!(
            raw.predicted_dt, 0.0,
            "先取りしない（要求どおりの時刻に起きる）"
        );
        // 2 フレーム目以降も持ち越しで戻らないこと（RawInput::take は値を保持する）。
        let raw2 = input.take(4096, PhysicalSize::new(100, 100), 1.0);
        assert_eq!(raw2.predicted_dt, 0.0);
    }
}
