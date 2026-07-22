//! egui メインウィンドウの placeholder view（#532 SU2）。show/hide/focus/位置を視覚検証できる
//! 最小 chrome を描く。検索本体は SU3。font-first（jp_font を index 0）は SU1 申し送りの義務。

use std::sync::OnceLock;
use std::time::Instant;

use snotra_egui_runtime::{EguiView, RuntimeFrame};

static JP_FONT_BYTES: OnceLock<Box<[u8]>> = OnceLock::new();

fn japanese_font_definitions(bytes: &'static [u8]) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    let mut font = egui::FontData::from_static(bytes);
    font.tweak = egui::FontTweak {
        scale: 1.0,
        y_offset_factor: 0.3,
        y_offset: 0.0,
        ..Default::default()
    };
    fonts.font_data.insert("jp_font".to_owned(), font.into());
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        // insert(0)＝先頭。jp_font を最優先にして単一フォント化する。push（末尾 fallback）だと
        // Latin=egui 既定 / CJK=Yu Gothic に分離し、被覆 AA 無の softbuffer でベースラインずれ（#579/#399）。
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, "jp_font".to_owned());
    }
    fonts
}

fn configure_japanese_font(context: &egui::Context) {
    let candidates = [
        "C:/Windows/Fonts/YuGothM.ttc",
        "C:/Windows/Fonts/yugothic.ttf",
        "C:/Windows/Fonts/msgothic.ttc",
        "C:/Windows/Fonts/meiryo.ttc",
    ];
    if JP_FONT_BYTES.get().is_none() {
        for path in candidates {
            if let Ok(bytes) = std::fs::read(path) {
                let _ = JP_FONT_BYTES.set(bytes.into_boxed_slice());
                break;
            }
        }
    }
    if let Some(bytes) = JP_FONT_BYTES.get() {
        // OnceLock の中身は以後不変ゆえ 'static として安全に借用できる。
        let static_bytes: &'static [u8] =
            unsafe { std::mem::transmute::<&[u8], &'static [u8]>(bytes) };
        context.set_fonts(japanese_font_definitions(static_bytes));
    }
}

#[allow(dead_code)] // フィールドは Task 5（blur/focus 配線）で読む。Task 5 で除去する。
pub(crate) struct SearchWindowView {
    app_handle: tauri::AppHandle,
    was_focused: bool,
    unfocus_at: Option<Instant>,
    // emit dedup は共有 EguiShellState.hide_pending（show がクリア・codex #8）。view-local には持たない。
}

impl SearchWindowView {
    #[allow(dead_code)] // Task 3（create）が呼ぶまで未使用。Task 3 で除去する。
    pub(crate) fn new(app_handle: tauri::AppHandle) -> Self {
        Self {
            app_handle,
            was_focused: false,
            unfocus_at: None,
        }
    }
}

impl EguiView for SearchWindowView {
    fn setup(&mut self, context: &egui::Context) {
        configure_japanese_font(context);
    }

    fn update(&mut self, ui: &mut egui::Ui, _frame: &mut RuntimeFrame) {
        // placeholder: SU3 が検索 UI で置き換える。混在行（Latin+CJK）で font-first を視覚検証。
        // focus 観測 + blur emit は Task 5 で本体を実装する。
        ui.label("Snotra — 検索ウィンドウ（C:/Program Files/example）");
    }
}

#[cfg(test)]
mod tests {
    use super::japanese_font_definitions;

    #[test]
    fn jp_font_is_registered_at_index_zero_for_both_families() {
        let dummy: &'static [u8] = &[0u8; 4];
        let fonts = japanese_font_definitions(dummy);
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            let list = fonts.families.get(&family).expect("family present");
            assert_eq!(
                list.first().map(String::as_str),
                Some("jp_font"),
                "jp_font must be index 0 for {family:?}（push=末尾だと #579 再発）"
            );
        }
    }
}
