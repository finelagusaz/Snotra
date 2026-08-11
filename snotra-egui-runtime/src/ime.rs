use std::ops::Range;

/// IMM32 の変換属性（`GCS_COMPATTR`）から、**変換対象の節**の文字範囲を取り出す。
///
/// `attributes` は UTF-16 単位で 1 バイトずつ属性が並ぶ。返すのは egui が要求する
/// **文字単位**の範囲で、単位の読み替えがこの関数の主な仕事である（サロゲートペアを含む
/// テキストで両者はずれる・下のテストが実測で固定する）。
///
/// **この戻り値を消費するのは egui の `paint_ime_preedit_text_visuals` である**——未確定の
/// 全体に細い下線を、ここで返した範囲に太い下線を引く。`windows_ime.rs` が
/// `ImeEvent::Preedit { active_range_chars, .. }` に載せて渡す。
///
/// **消費されるかは `Visuals::ime_composition.legacy_visuals` に懸かっている。** 真だと egui は
/// `cursor_purpose` を `Selection` に固定し、**この値を一度も参照せず**変換中を選択帯で描く
/// ——テストが緑でも画面には何も現れない状態になる。egui はこれを **Windows で既定 `true`**
/// にするため、**Snotra は `search_input_ui` の入口で偽へ倒している**（理由と経緯は同関数の
/// 当該コメント）。**その 1 行を戻すと、この関数は計算されるだけで描画へ届かなくなる。**
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
