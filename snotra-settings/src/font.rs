//! 日本語フォントの読み込みとシステムフォント列挙。
//!
//! Regular（`jp_font`）と見出し用 Semibold ファミリ（`SEMIBOLD_FAMILY`）を登録する。
//! Semibold が無い環境では未登録のまま graceful degrade する（見出しは Regular にフォールバック）。

use eframe::egui;

/// `FontFamily::Name` key for the Semibold heading family.
///
/// Registered by [`configure_fonts`] and referenced by `style::apply_type_ramp`.
pub const SEMIBOLD_FAMILY: &str = "semibold";

/// Configure Japanese font support by loading a system font.
///
/// Returns `true` if a Semibold heading family was registered (≥1 Semibold face
/// found). Callers thread this into `style::apply_type_ramp` so headings only
/// repoint to the Semibold family when it exists — otherwise headings stay on the
/// Regular Proportional family (graceful degrade; no panic on undefined family).
#[must_use]
pub fn configure_fonts(ctx: &egui::Context) -> bool {
    let mut fonts = egui::FontDefinitions::default();

    // Try loading Japanese font from Windows system fonts (priority order).
    // Each font has a FontTweak to align vertical position with the default Latin font.
    // Tweak values are provisional — adjust after visual confirmation.
    let jp_font_candidates: &[(&str, egui::FontTweak)] = &[
        (
            "C:\\Windows\\Fonts\\YuGothM.ttc",
            egui::FontTweak {
                scale: 1.0,
                y_offset_factor: 0.3,
                y_offset: 0.0,
                ..Default::default()
            },
        ),
        (
            "C:\\Windows\\Fonts\\yugothic.ttf",
            egui::FontTweak {
                scale: 1.0,
                y_offset_factor: 0.3,
                y_offset: 0.0,
                ..Default::default()
            },
        ),
        (
            "C:\\Windows\\Fonts\\msgothic.ttc",
            egui::FontTweak {
                scale: 1.0,
                y_offset_factor: 0.3,
                y_offset: 0.0,
                ..Default::default()
            },
        ),
        (
            "C:\\Windows\\Fonts\\meiryo.ttc",
            egui::FontTweak {
                scale: 1.0,
                y_offset_factor: 0.3,
                y_offset: 0.0,
                ..Default::default()
            },
        ),
    ];

    let mut found = false;
    for &(path, ref tweak) in jp_font_candidates {
        if let Ok(font_data) = std::fs::read(path) {
            let mut data = egui::FontData::from_owned(font_data);
            data.tweak = tweak.clone();
            fonts.font_data.insert("jp_font".to_owned(), data.into());
            // Append Japanese font as fallback for proportional text
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .push("jp_font".to_owned());
            // Also for monospace
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("jp_font".to_owned());
            found = true;
            break;
        }
    }

    if !found {
        eprintln!("warning: no Japanese font found; UI text may not render correctly");
    }

    let semibold = register_semibold_family(&mut fonts);

    ctx.set_fonts(fonts);
    semibold
}

/// Register a Semibold [`egui::FontFamily::Name`] for headings (Fluent: headings
/// are Semibold).
///
/// Uses Yu Gothic UI Semibold (`YuGothB.ttc` face 2) for both Latin and Japanese
/// — a single font keeps mixed-script headings on one baseline. It is followed by
/// the existing Proportional fallback chain so any glyph it lacks still renders
/// (in Regular weight) — no missing glyphs. Returns `false` (family not registered)
/// when no Semibold face is found, so the caller can leave headings on the Regular
/// family.
fn register_semibold_family(fonts: &mut egui::FontDefinitions) -> bool {
    // (path, ttc face index, tweak). A single font for both scripts: Yu Gothic UI
    // Semibold covers Latin + Japanese, so a mixed heading (e.g. "PATH 実行ファイル")
    // shares one baseline. Routing Latin through a separate font (Segoe UI Semibold)
    // misaligned the two scripts within a heading. The y_offset_factor matches the
    // Regular jp_font so headings sit like the all-Japanese ones.
    let semibold_candidates: &[(&str, u32, egui::FontTweak)] = &[(
        "C:\\Windows\\Fonts\\YuGothB.ttc",
        2,
        egui::FontTweak {
            scale: 1.0,
            y_offset_factor: 0.3,
            y_offset: 0.0,
            ..Default::default()
        },
    )];

    let mut names = Vec::new();
    for &(path, index, ref tweak) in semibold_candidates {
        let Ok(font_data) = std::fs::read(path) else {
            continue;
        };
        // egui parses every registered font eagerly in `set_fonts` and panics
        // (→ abort in release, where `panic = "abort"`) on an out-of-range ttc
        // face index. Validate the face before inserting so a non-standard font
        // layout (e.g. a `YuGothB.ttc` lacking the Semibold face) degrades to
        // Regular headings instead of aborting the settings process.
        if !face_index_valid(&font_data, index) {
            continue;
        }
        let mut data = egui::FontData::from_owned(font_data);
        data.index = index;
        data.tweak = tweak.clone();
        let key = format!("semibold_{}", names.len());
        fonts.font_data.insert(key.clone(), data.into());
        names.push(key);
    }

    if names.is_empty() {
        return false;
    }

    // Append the Regular Proportional fallback chain so non-covered glyphs still render.
    if let Some(proportional) = fonts.families.get(&egui::FontFamily::Proportional) {
        names.extend(proportional.iter().cloned());
    }
    fonts
        .families
        .insert(egui::FontFamily::Name(SEMIBOLD_FAMILY.into()), names);
    true
}

/// Returns `true` if `index` is a usable face index for the font bytes.
///
/// Face 0 is always valid. A TrueType Collection (`ttcf`) stores its face count as
/// a big-endian `u32` at offset 8; any index ≥ that count is absent. A
/// non-collection file (single TTF/OTF) only has face 0. egui parses every
/// registered font eagerly in [`egui::Context::set_fonts`] and panics (→ abort in
/// release) on an out-of-range face index, so this pre-check lets a non-standard
/// font layout degrade to Regular headings instead of aborting the process.
fn face_index_valid(bytes: &[u8], index: u32) -> bool {
    if index == 0 {
        return true;
    }
    // TrueType Collection header: "ttcf" tag, then numFonts (u32 BE) at offset 8.
    if bytes.len() >= 12 && &bytes[0..4] == b"ttcf" {
        let num_fonts = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        return index < num_fonts;
    }
    false
}

/// List available system font family names (Windows only).
/// Used by the Visual tab (Phase 2).
#[cfg(windows)]
#[allow(dead_code)]
pub fn list_system_fonts() -> Vec<String> {
    use std::collections::BTreeSet;
    use windows::Win32::Foundation::LPARAM;
    use windows::Win32::Graphics::Gdi::*;

    unsafe extern "system" fn enum_callback(
        logfont: *const LOGFONTW,
        _text_metric: *const TEXTMETRICW,
        _font_type: u32,
        lparam: LPARAM,
    ) -> i32 {
        unsafe {
            let fonts = &mut *(lparam.0 as *mut BTreeSet<String>);
            let lf = &*logfont;
            let name_len = lf.lfFaceName.iter().position(|&c| c == 0).unwrap_or(32);
            let name = String::from_utf16_lossy(&lf.lfFaceName[..name_len]);
            if !name.starts_with('@') {
                fonts.insert(name);
            }
            1
        }
    }

    let mut fonts = BTreeSet::<String>::new();
    let hdc = unsafe { GetDC(None) };
    let mut lf: LOGFONTW = unsafe { std::mem::zeroed() };
    lf.lfCharSet = DEFAULT_CHARSET;

    unsafe {
        EnumFontFamiliesExW(
            hdc,
            &lf,
            Some(enum_callback),
            LPARAM(&mut fonts as *mut _ as isize),
            0,
        );
        ReleaseDC(None, hdc);
    }

    fonts.into_iter().collect()
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn list_system_fonts() -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::face_index_valid;

    /// Minimal TrueType Collection header advertising `num_fonts` faces.
    fn ttc_header(num_fonts: u32) -> Vec<u8> {
        let mut b = b"ttcf".to_vec();
        b.extend_from_slice(&[0, 1, 0, 0]); // version 1.0
        b.extend_from_slice(&num_fonts.to_be_bytes()); // numFonts at offset 8
        b
    }

    #[test]
    fn face_zero_always_valid() {
        // Face 0 is the single-face default; never gated even on non-font bytes.
        assert!(face_index_valid(&[], 0));
        assert!(face_index_valid(b"not a font", 0));
    }

    #[test]
    fn ttc_face_within_count_is_valid() {
        // YuGothB.ttc ships 3 faces; face 2 (Yu Gothic UI Semibold) is present.
        assert!(face_index_valid(&ttc_header(3), 2));
    }

    #[test]
    fn ttc_face_beyond_count_degrades() {
        // A YuGothB.ttc variant with only 2 faces must reject face 2 (no panic path).
        assert!(!face_index_valid(&ttc_header(2), 2));
    }

    #[test]
    fn non_collection_rejects_nonzero_face() {
        // A single TTF (no "ttcf" tag) has only face 0.
        assert!(!face_index_valid(b"\x00\x01\x00\x00 single-face ttf body", 1));
    }

    #[test]
    fn truncated_collection_header_degrades() {
        // Claims to be a collection but is too short to hold numFonts → reject.
        assert!(!face_index_valid(b"ttcf", 2));
    }
}
