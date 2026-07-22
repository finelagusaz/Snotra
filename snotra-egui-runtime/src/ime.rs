use std::ops::Range;

pub(crate) fn active_range_chars(
    text: &str,
    attributes: &[u8],
    cursor_utf16: Option<usize>,
) -> Option<Range<usize>> {
    const ATTR_TARGET_CONVERTED: u8 = 1;
    const ATTR_TARGET_NOT_CONVERTED: u8 = 3;

    let mut utf16_offset = 0;
    let mut first_target = None;
    let mut end_target = None;

    for (char_index, character) in text.chars().enumerate() {
        let next_utf16_offset = utf16_offset + character.len_utf16();
        let targeted = attributes
            .get(utf16_offset..next_utf16_offset)
            .is_some_and(|span| {
                span.iter().any(|attribute| {
                    matches!(
                        *attribute,
                        ATTR_TARGET_CONVERTED | ATTR_TARGET_NOT_CONVERTED
                    )
                })
            });

        if targeted {
            first_target.get_or_insert(char_index);
            end_target = Some(char_index + 1);
        } else if first_target.is_some() {
            break;
        }
        utf16_offset = next_utf16_offset;
    }

    if let (Some(start), Some(end)) = (first_target, end_target) {
        return Some(start..end);
    }

    cursor_utf16.map(|cursor| {
        let mut consumed_utf16 = 0;
        let mut char_index = 0;
        for character in text.chars() {
            let next = consumed_utf16 + character.len_utf16();
            if next > cursor {
                break;
            }
            consumed_utf16 = next;
            char_index += 1;
        }
        char_index..char_index
    })
}

pub(crate) fn logical_ime_rect_to_physical(
    rect: egui::Rect,
    scale_factor: f32,
) -> ([i32; 2], [i32; 4]) {
    let left = (rect.left() * scale_factor).round() as i32;
    let top = (rect.top() * scale_factor).round() as i32;
    let right = (rect.right() * scale_factor).round() as i32;
    let bottom = (rect.bottom() * scale_factor).round() as i32;
    ([left, bottom], [left, top, right, bottom])
}

#[cfg(windows)]
#[path = "windows_ime.rs"]
mod platform;

#[cfg(not(windows))]
mod platform {
    pub(crate) struct PlatformIme;

    impl PlatformIme {
        pub(crate) fn new(_window: &tauri::Window) -> Result<Self, crate::RuntimeError> {
            Ok(Self)
        }

        pub(crate) fn drain(&self) -> Vec<egui::ImeEvent> {
            Vec::new()
        }

        pub(crate) fn update(&self, _output: Option<egui::output::IMEOutput>, _scale_factor: f32) {}
    }
}

pub(crate) struct ImeBridge {
    platform: platform::PlatformIme,
}

impl ImeBridge {
    pub(crate) fn new(window: &tauri::Window) -> Result<Self, crate::RuntimeError> {
        Ok(Self {
            platform: platform::PlatformIme::new(window)?,
        })
    }

    pub(crate) fn drain(&self) -> Vec<egui::ImeEvent> {
        self.platform.drain()
    }

    pub(crate) fn update(&self, output: Option<egui::output::IMEOutput>, scale_factor: f32) {
        self.platform.update(output, scale_factor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ATTR_INPUT: u8 = 0;
    const ATTR_TARGET_CONVERTED: u8 = 1;

    #[test]
    fn target_attributes_map_utf16_units_to_character_range() {
        let text = "A𠮷B";
        let attributes = [
            ATTR_INPUT,
            ATTR_TARGET_CONVERTED,
            ATTR_TARGET_CONVERTED,
            ATTR_INPUT,
        ];

        assert_eq!(active_range_chars(text, &attributes, Some(3)), Some(1..2));
    }

    #[test]
    fn cursor_fallback_maps_utf16_offset_to_character_index() {
        assert_eq!(active_range_chars("A𠮷B", &[], Some(3)), Some(2..2));
        assert_eq!(active_range_chars("日本語", &[], Some(1)), Some(1..1));
    }

    #[test]
    fn ime_rect_uses_physical_pixels_at_fractional_scale() {
        let rect = egui::Rect::from_min_max(egui::pos2(10.0, 20.0), egui::pos2(30.0, 32.0));

        assert_eq!(
            logical_ime_rect_to_physical(rect, 1.25),
            ([13, 40], [13, 25, 38, 40])
        );
    }
}
