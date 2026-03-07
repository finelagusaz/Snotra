use eframe::egui;
use snotra_core::config::{is_system_shortcut, HotkeyConfig};

/// Hotkey capture widget state
#[derive(Default)]
pub struct HotkeyInputState {
    capturing: bool,
}

impl HotkeyInputState {
    pub fn is_capturing(&self) -> bool {
        self.capturing
    }
}

/// Render a hotkey input widget. Returns true if the hotkey was changed.
pub fn hotkey_input(
    ui: &mut egui::Ui,
    config: &mut HotkeyConfig,
    state: &mut HotkeyInputState,
) -> bool {
    let mut changed = false;

    let display = if state.capturing {
        "キーを押してください...".to_string()
    } else if config.modifier.is_empty() && config.key.is_empty() {
        "(未設定)".to_string()
    } else if config.modifier.is_empty() {
        config.key.clone()
    } else {
        format!("{} + {}", config.modifier, config.key)
    };

    let button = egui::Button::new(&display).min_size(egui::vec2(160.0, 24.0));
    let response = ui.add(button);

    if response.clicked() {
        state.capturing = !state.capturing;
    }

    if state.capturing {
        ui.ctx().request_repaint();

        let result = ui.input(capture_hotkey);

        match result {
            CaptureResult::None => {}
            CaptureResult::Cancel => {
                state.capturing = false;
            }
            CaptureResult::Clear => {
                config.modifier.clear();
                config.key.clear();
                state.capturing = false;
                changed = true;
            }
            CaptureResult::Captured { modifier, key } => {
                config.modifier = modifier;
                config.key = key;
                state.capturing = false;
                changed = true;
            }
        }
    }

    changed
}

enum CaptureResult {
    None,
    Cancel,
    Clear,
    Captured { modifier: String, key: String },
}

fn capture_hotkey(input: &egui::InputState) -> CaptureResult {
    // Escape cancels capture
    if input.key_pressed(egui::Key::Escape) {
        return CaptureResult::Cancel;
    }

    // Backspace/Delete clears the hotkey
    if input.key_pressed(egui::Key::Backspace) || input.key_pressed(egui::Key::Delete) {
        return CaptureResult::Clear;
    }

    // Find any non-modifier key that was pressed this frame
    for &key in egui::Key::ALL {
        if is_modifier_key(key) {
            continue;
        }
        if input.key_pressed(key) {
            let key_name = egui_key_to_config_name(key);
            if let Some(name) = key_name {
                let modifier = build_modifier_string(&input.modifiers);
                // Require at least one modifier for safety
                if modifier.is_empty() {
                    continue;
                }
                // Reject system shortcuts immediately (save-time validate is a backstop)
                if is_system_shortcut(&modifier, &name) {
                    continue;
                }
                return CaptureResult::Captured {
                    modifier,
                    key: name,
                };
            }
        }
    }

    CaptureResult::None
}

fn is_modifier_key(_key: egui::Key) -> bool {
    // egui doesn't have separate Ctrl/Alt/Shift key variants;
    // modifiers are tracked in InputState::modifiers, not as Key presses.
    false
}

fn build_modifier_string(mods: &egui::Modifiers) -> String {
    let mut parts = Vec::new();
    if mods.ctrl {
        parts.push("Ctrl");
    }
    if mods.alt {
        parts.push("Alt");
    }
    if mods.shift {
        parts.push("Shift");
    }
    parts.join("+")
}

fn egui_key_to_config_name(key: egui::Key) -> Option<String> {
    use egui::Key;
    match key {
        // Letters
        Key::A => Some("A".into()),
        Key::B => Some("B".into()),
        Key::C => Some("C".into()),
        Key::D => Some("D".into()),
        Key::E => Some("E".into()),
        Key::F => Some("F".into()),
        Key::G => Some("G".into()),
        Key::H => Some("H".into()),
        Key::I => Some("I".into()),
        Key::J => Some("J".into()),
        Key::K => Some("K".into()),
        Key::L => Some("L".into()),
        Key::M => Some("M".into()),
        Key::N => Some("N".into()),
        Key::O => Some("O".into()),
        Key::P => Some("P".into()),
        Key::Q => Some("Q".into()),
        Key::R => Some("R".into()),
        Key::S => Some("S".into()),
        Key::T => Some("T".into()),
        Key::U => Some("U".into()),
        Key::V => Some("V".into()),
        Key::W => Some("W".into()),
        Key::X => Some("X".into()),
        Key::Y => Some("Y".into()),
        Key::Z => Some("Z".into()),
        // Digits
        Key::Num0 => Some("0".into()),
        Key::Num1 => Some("1".into()),
        Key::Num2 => Some("2".into()),
        Key::Num3 => Some("3".into()),
        Key::Num4 => Some("4".into()),
        Key::Num5 => Some("5".into()),
        Key::Num6 => Some("6".into()),
        Key::Num7 => Some("7".into()),
        Key::Num8 => Some("8".into()),
        Key::Num9 => Some("9".into()),
        // Function keys
        Key::F1 => Some("F1".into()),
        Key::F2 => Some("F2".into()),
        Key::F3 => Some("F3".into()),
        Key::F4 => Some("F4".into()),
        Key::F5 => Some("F5".into()),
        Key::F6 => Some("F6".into()),
        Key::F7 => Some("F7".into()),
        Key::F8 => Some("F8".into()),
        Key::F9 => Some("F9".into()),
        Key::F10 => Some("F10".into()),
        Key::F11 => Some("F11".into()),
        Key::F12 => Some("F12".into()),
        // Special keys
        Key::Space => Some("Space".into()),
        Key::Enter => Some("Enter".into()),
        Key::Tab => Some("Tab".into()),
        Key::Home => Some("Home".into()),
        Key::End => Some("End".into()),
        Key::PageUp => Some("PageUp".into()),
        Key::PageDown => Some("PageDown".into()),
        Key::Insert => Some("Insert".into()),
        // Not usable as hotkey
        _ => None,
    }
}
