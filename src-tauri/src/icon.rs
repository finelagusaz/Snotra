use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use base64::Engine;
use snotra_core::binfmt::{deserialize_with_header, serialize_with_header};
use snotra_core::config::Config;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, SelectObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_SMALLICON};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

const ICON_SIZE: i32 = 16;
const ICON_MAGIC: [u8; 4] = *b"ICON";
const ICON_VERSION: u32 = 3;

#[derive(Serialize, Deserialize, Default)]
struct IconCacheData {
    base64: HashMap<String, String>,
}

pub struct IconCache {
    data: IconCacheData,
    dirty: bool,
}

impl IconCache {
    /// Try to load persisted cache, or return empty cache. Never blocks on icon extraction.
    pub fn load() -> Self {
        let loaded = (|| {
            let path = cache_path()?;
            let bytes = std::fs::read(&path).ok()?;
            deserialize_with_header::<IconCacheData>(&bytes, ICON_MAGIC, ICON_VERSION)
        })();
        match loaded {
            Some(data) => Self { data, dirty: false },
            None => Self {
                data: IconCacheData::default(),
                dirty: false,
            },
        }
    }

    /// Get base64 icon for a path, extracting on-demand if not cached.
    pub fn get_or_extract(&mut self, path: &str) -> Option<String> {
        if let Some(b64) = self.data.base64.get(path) {
            return Some(b64.clone());
        }
        let icon_data = extract_icon(path)?;
        let b64 = bgra_to_png_base64(&icon_data)?;
        self.data.base64.insert(path.to_string(), b64.clone());
        self.dirty = true;
        Some(b64)
    }

    /// Batch version of get_or_extract.
    pub fn get_or_extract_batch(&mut self, paths: &[String]) -> HashMap<String, String> {
        let mut result = HashMap::new();
        for path in paths {
            if let Some(b64) = self.get_or_extract(path) {
                result.insert(path.clone(), b64);
            }
        }
        result
    }

    /// Save to disk if there are new entries since last save.
    pub fn save_if_dirty(&mut self) {
        if !self.dirty {
            return;
        }
        let Some(path) = cache_path() else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let Some(bytes) = serialize_with_header(ICON_MAGIC, ICON_VERSION, &self.data) else {
            return;
        };
        let tmp_path = path.with_extension("bin.tmp");
        if std::fs::write(&tmp_path, &bytes).is_ok() {
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::rename(&tmp_path, &path);
            self.dirty = false;
        }
    }

    /// Clear all cached icons (used after index rebuild).
    pub fn clear(&mut self) {
        self.data.base64.clear();
        self.dirty = false;
        // Also remove persisted file so stale data is not reloaded
        if let Some(path) = cache_path() {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Managed state for icon cache
pub type IconCacheState = Mutex<Option<IconCache>>;

fn cache_path() -> Option<PathBuf> {
    Config::config_dir().map(|p| p.join("icons.bin"))
}

struct IconData {
    width: u32,
    height: u32,
    bgra: Vec<u8>,
}

fn extract_icon(path: &str) -> Option<IconData> {
    unsafe {
        let wide_path: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();

        let mut shfi = SHFILEINFOW::default();
        let result = SHGetFileInfoW(
            windows::core::PCWSTR(wide_path.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut shfi),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_SMALLICON,
        );

        if result == 0 || shfi.hIcon.is_invalid() {
            return None;
        }

        let icon_data = hicon_to_bgra(shfi.hIcon);
        let _ = DestroyIcon(shfi.hIcon);
        icon_data
    }
}

fn hicon_to_bgra(hicon: HICON) -> Option<IconData> {
    unsafe {
        let mut icon_info = ICONINFO::default();
        if GetIconInfo(hicon, &mut icon_info).is_err() {
            return None;
        }

        let _cleanup = BitmapCleanup(&icon_info);

        let hdc_screen = CreateCompatibleDC(None);
        if hdc_screen.is_invalid() {
            return None;
        }

        let width = ICON_SIZE as u32;
        let height = ICON_SIZE as u32;

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut pixels = vec![0u8; (width * height * 4) as usize];

        if !icon_info.hbmColor.is_invalid() {
            let old = SelectObject(hdc_screen, icon_info.hbmColor.into());
            GetDIBits(
                hdc_screen,
                icon_info.hbmColor,
                0,
                height,
                Some(pixels.as_mut_ptr() as *mut _),
                &mut bmi,
                DIB_RGB_COLORS,
            );
            SelectObject(hdc_screen, old);
        }

        let _ = DeleteDC(hdc_screen);

        let has_data = pixels.iter().any(|&b| b != 0);
        if !has_data {
            return None;
        }

        Some(IconData {
            width,
            height,
            bgra: pixels,
        })
    }
}

struct BitmapCleanup<'a>(&'a ICONINFO);
impl Drop for BitmapCleanup<'_> {
    fn drop(&mut self) {
        unsafe {
            if !self.0.hbmColor.is_invalid() {
                let _ = DeleteObject(self.0.hbmColor.into());
            }
            if !self.0.hbmMask.is_invalid() {
                let _ = DeleteObject(self.0.hbmMask.into());
            }
        }
    }
}

fn bgra_to_png_base64(data: &IconData) -> Option<String> {
    let w = data.width as usize;
    let h = data.height as usize;
    if data.bgra.len() != w * h * 4 {
        return None;
    }

    // Convert BGRA to RGBA
    let mut rgba = Vec::with_capacity(w * h * 4);
    for chunk in data.bgra.chunks_exact(4) {
        rgba.push(chunk[2]); // R
        rgba.push(chunk[1]); // G
        rgba.push(chunk[0]); // B
        rgba.push(chunk[3]); // A
    }

    // Encode as PNG
    let mut png_buf = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_buf, data.width, data.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&rgba).ok()?;
    }

    // Base64 encode
    Some(base64::engine::general_purpose::STANDARD.encode(&png_buf))
}
