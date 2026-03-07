use eframe::egui;

/// Configure Japanese font support by loading a system font.
pub fn configure_fonts(ctx: &egui::Context) {
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
            },
        ),
        (
            "C:\\Windows\\Fonts\\yugothic.ttf",
            egui::FontTweak {
                scale: 1.0,
                y_offset_factor: 0.3,
                y_offset: 0.0,
            },
        ),
        (
            "C:\\Windows\\Fonts\\msgothic.ttc",
            egui::FontTweak {
                scale: 1.0,
                y_offset_factor: 0.3,
                y_offset: 0.0,
            },
        ),
        (
            "C:\\Windows\\Fonts\\meiryo.ttc",
            egui::FontTweak {
                scale: 1.0,
                y_offset_factor: 0.3,
                y_offset: 0.0,
            },
        ),
    ];

    let mut found = false;
    for &(path, ref tweak) in jp_font_candidates {
        if let Ok(font_data) = std::fs::read(path) {
            let mut data = egui::FontData::from_owned(font_data);
            data.tweak = *tweak;
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

    ctx.set_fonts(fonts);
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
