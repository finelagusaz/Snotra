use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::binfmt::{deserialize_with_header, serialize_with_header};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, SelectObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_SMALLICON};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, DestroyIcon, DrawIconEx, GetIconInfo, DI_NORMAL, HICON, ICONINFO,
};

use crate::config::Config;
use crate::indexer::AppEntry;

const ICON_SIZE: i32 = 16;
const ICON_MAGIC: [u8; 4] = *b"ICON";
const ICON_VERSION: u32 = 1;

#[derive(Clone, Serialize, Deserialize)]
pub struct IconData {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

#[derive(Serialize, Deserialize, Default)]
struct IconCacheData {
    icons: HashMap<String, IconData>,
}

pub struct IconCache {
    data: IconCacheData,
    runtime: HashMap<String, HICON>,
}

impl IconCache {
    pub fn build(entries: &[AppEntry]) -> Self {
        let mut data = IconCacheData {
            icons: HashMap::new(),
        };

        for entry in entries {
            if let Some(icon_data) = extract_icon(&entry.target_path) {
                data.icons.insert(entry.target_path.clone(), icon_data);
            }
        }

        let mut cache = Self {
            data,
            runtime: HashMap::new(),
        };
        cache.build_runtime_icons();
        cache
    }

    pub fn load() -> Option<Self> {
        let path = cache_path()?;
        let bytes = std::fs::read(&path).ok()?;
        let data: IconCacheData = deserialize_with_header(&bytes, ICON_MAGIC, ICON_VERSION)?;

        let mut cache = Self {
            data,
            runtime: HashMap::new(),
        };
        cache.build_runtime_icons();
        Some(cache)
    }

    pub fn save(&self) {
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
        }
    }

    pub fn rebuild_cache(entries: &[AppEntry]) {
        let cache = Self::build(entries);
        cache.save();
    }

    pub fn draw(&self, target_path: &str, hdc: windows::Win32::Graphics::Gdi::HDC, x: i32, y: i32) {
        if let Some(&hicon) = self.runtime.get(target_path) {
            unsafe {
                let _ = DrawIconEx(hdc, x, y, hicon, ICON_SIZE, ICON_SIZE, 0, None, DI_NORMAL);
            }
        }
    }

    fn build_runtime_icons(&mut self) {
        for (path, icon_data) in &self.data.icons {
            if let Some(hicon) = create_hicon_from_data(icon_data) {
                self.runtime.insert(path.clone(), hicon);
            }
        }
    }
}

impl Drop for IconCache {
    fn drop(&mut self) {
        for (_, hicon) in self.runtime.drain() {
            unsafe {
                let _ = DestroyIcon(hicon);
            }
        }
    }
}

fn cache_path() -> Option<PathBuf> {
    Config::config_dir().map(|p| p.join("icons.bin"))
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

        // Clean up bitmaps from GetIconInfo
        let _cleanup = scopeguard_bitmaps(&icon_info);

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
                biHeight: -(height as i32), // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0 as u32,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut pixels = vec![0u8; (width * height * 4) as usize];

        // Get color bitmap data
        if !icon_info.hbmColor.is_invalid() {
            let old = SelectObject(hdc_screen, icon_info.hbmColor);
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

        // Verify we got actual pixel data (not all zeros)
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

fn scopeguard_bitmaps(icon_info: &ICONINFO) -> impl Drop + '_ {
    struct BitmapCleanup<'a>(&'a ICONINFO);
    impl Drop for BitmapCleanup<'_> {
        fn drop(&mut self) {
            unsafe {
                if !self.0.hbmColor.is_invalid() {
                    let _ = DeleteObject(self.0.hbmColor);
                }
                if !self.0.hbmMask.is_invalid() {
                    let _ = DeleteObject(self.0.hbmMask);
                }
            }
        }
    }
    BitmapCleanup(icon_info)
}

fn create_hicon_from_data(data: &IconData) -> Option<HICON> {
    unsafe {
        let hdc = CreateCompatibleDC(None);
        if hdc.is_invalid() {
            return None;
        }

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: data.width as i32,
                biHeight: -(data.height as i32), // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0 as u32,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut bits_ptr = std::ptr::null_mut();
        let hbm_color = windows::Win32::Graphics::Gdi::CreateDIBSection(
            hdc,
            &bmi,
            DIB_RGB_COLORS,
            &mut bits_ptr,
            None,
            0,
        )
        .ok()?;

        // Validate pixel data length before unsafe copy
        let expected_len = (data.width * data.height * 4) as usize;
        if data.bgra.len() != expected_len {
            let _ = DeleteObject(hbm_color);
            let _ = DeleteDC(hdc);
            return None;
        }

        // Copy pixel data
        std::ptr::copy_nonoverlapping(data.bgra.as_ptr(), bits_ptr as *mut u8, data.bgra.len());

        // Create mask bitmap (all zeros = fully opaque)
        let mask_size = ((data.width + 31) / 32 * 4 * data.height) as usize;
        let mask_bits = vec![0u8; mask_size];

        let hbm_mask = windows::Win32::Graphics::Gdi::CreateBitmap(
            data.width as i32,
            data.height as i32,
            1,
            1,
            Some(mask_bits.as_ptr() as *const _),
        );

        let icon_info = ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: hbm_mask,
            hbmColor: hbm_color,
        };

        let hicon = CreateIconIndirect(&icon_info).ok();

        let _ = DeleteObject(hbm_color);
        let _ = DeleteObject(hbm_mask);
        let _ = DeleteDC(hdc);

        hicon
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_data_bincode_roundtrip() {
        let data = IconData {
            width: 16,
            height: 16,
            bgra: vec![0xFF; 16 * 16 * 4],
        };

        let bytes = serialize_with_header(ICON_MAGIC, ICON_VERSION, &data).expect("serialize");
        let restored: IconData =
            deserialize_with_header(&bytes, ICON_MAGIC, ICON_VERSION).expect("deserialize");

        assert_eq!(restored.width, 16);
        assert_eq!(restored.height, 16);
        assert_eq!(restored.bgra.len(), 16 * 16 * 4);
    }

    #[test]
    fn icon_cache_data_roundtrip() {
        let mut icons = HashMap::new();
        icons.insert(
            "C:\\test.exe".to_string(),
            IconData {
                width: 16,
                height: 16,
                bgra: vec![0xAB; 16 * 16 * 4],
            },
        );

        let cache_data = IconCacheData { icons };
        let bytes =
            serialize_with_header(ICON_MAGIC, ICON_VERSION, &cache_data).expect("serialize");
        let restored: IconCacheData =
            deserialize_with_header(&bytes, ICON_MAGIC, ICON_VERSION).expect("deserialize");

        assert!(restored.icons.contains_key("C:\\test.exe"));
        assert_eq!(restored.icons["C:\\test.exe"].bgra.len(), 16 * 16 * 4);
    }
}
