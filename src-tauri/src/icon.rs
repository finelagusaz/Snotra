use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

use snotra_core::binfmt::BinFile;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDIBits, SelectObject,
};
use windows::Win32::Storage::FileSystem::{FILE_FLAGS_AND_ATTRIBUTES, SearchPathW};
use windows::Win32::UI::Shell::{SHFILEINFOW, SHGFI_ICON, SHGFI_SMALLICON, SHGetFileInfoW};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

const ICON_SIZE: i32 = 16;
const ICON_MAGIC: [u8; 4] = *b"ICON";
const ICON_VERSION: u32 = 5; // v4: base64 String, v5: raw PNG bytes

#[derive(Serialize, Deserialize, Default)]
struct IconCacheData {
    png: HashMap<String, Vec<u8>>,
}

pub struct IconCache {
    data: IconCacheData,
    dirty: bool,
}

impl IconCache {
    /// Try to load persisted cache, or return empty cache. Never blocks on icon extraction.
    pub fn load() -> Self {
        let loaded = icon_bin_file().and_then(|bf| bf.load::<IconCacheData>());
        match loaded {
            Some(data) => Self { data, dirty: false },
            None => Self {
                data: IconCacheData::default(),
                dirty: false,
            },
        }
    }

    /// Get cached PNG bytes for a path (read-only, no extraction).
    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.data.png.get(path).map(|v| v.as_slice())
    }

    /// Insert extracted PNG bytes into the cache and mark dirty.
    pub fn insert(&mut self, path: String, png: Vec<u8>) {
        self.data.png.insert(path, png);
        self.dirty = true;
    }

    /// Save to disk if there are new entries since last save.
    pub fn save_if_dirty(&mut self) {
        if !self.dirty {
            return;
        }
        if let Some(bf) = icon_bin_file()
            && bf.save(&self.data)
        {
            self.dirty = false;
        }
    }

    /// 現在のインデックスに存在するパスのみ残し、不要なエントリを除去する。
    /// `clear()` と異なり有効なアイコンを再利用するため、再構築後の再抽出コストを削減する。
    /// 1 件でも除去した場合は dirty フラグを立てる。
    pub fn retain_paths(&mut self, valid_paths: &std::collections::HashSet<String>) {
        let before = self.data.png.len();
        self.data.png.retain(|k, _| valid_paths.contains(k));
        if self.data.png.len() < before {
            self.dirty = true;
        }
    }
}

/// Encode a batch of optional PNG slices into a length-prefixed binary frame.
///
/// Format:
///   [count: u32 LE]
///   For each icon (in request order):
///     [status: u8] (0 = None, 1 = Some)
///     If status == 1:
///       [png_len: u32 LE][png_bytes: len bytes]
pub fn encode_batch_binary(results: &[Option<&[u8]>]) -> Vec<u8> {
    let total_data: usize = results
        .iter()
        .map(|r| match r {
            Some(png) => 1 + 4 + png.len(), // status + len + data
            None => 1,                       // status only
        })
        .sum();
    let mut buf = Vec::with_capacity(4 + total_data);
    buf.extend_from_slice(&(results.len() as u32).to_le_bytes());
    for r in results {
        match r {
            Some(png) => {
                buf.push(1);
                buf.extend_from_slice(&(png.len() as u32).to_le_bytes());
                buf.extend_from_slice(png);
            }
            None => {
                buf.push(0);
            }
        }
    }
    buf
}

/// Extract PNG bytes for a path without holding any lock.
pub fn extract_png(path: &str) -> Option<Vec<u8>> {
    let icon_data = extract_icon(path)?;
    bgra_to_png(&icon_data)
}

/// Managed state for icon cache
pub type IconCacheState = Mutex<Option<IconCache>>;

fn icon_bin_file() -> Option<BinFile> {
    BinFile::new(ICON_MAGIC, ICON_VERSION, "icons.bin")
}

/// アイコンキャッシュをメモリ内・ディスク両方で無効化する。
/// 背景再スキャンでエントリ集合が変わったときに呼ぶ。メモリ内 `IconCacheState` を
/// `None` にし、`icons.bin` を削除する。両方やらないと、ファイルだけ消しても
/// メモリ内の古いアイコンが終了時の `save_if_dirty` で再永続化される。
pub fn invalidate_icon_cache(icons: &IconCacheState) {
    invalidate_icon_cache_with(icons, icon_bin_file());
}

/// テスト可能な内部実装。`bin_file` を `None` で渡すとファイル削除をスキップする。
fn invalidate_icon_cache_with(icons: &IconCacheState, bin_file: Option<BinFile>) {
    // メモリ内キャッシュをクリア（次の get_icons_batch で icons.bin から再ロードされる）。
    *icons.lock().unwrap() = None;
    // ディスク上の icons.bin も削除（再ロード時に古いデータを読まないため）。
    if let Some(bf) = bin_file {
        bf.remove();
    }
}

struct IconData {
    width: u32,
    height: u32,
    bgra: Vec<u8>,
}

/// bare name ("explorer.exe") を PATH から検索してフルパスに解決する。
/// パス区切り文字やドライブレターを含む場合はそのまま返す。
fn resolve_to_full_path(path: &str) -> String {
    if path.contains('\\') || path.contains('/') || path.contains(':') {
        return path.to_string();
    }
    unsafe {
        let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        let mut buffer = vec![0u16; 512];
        let len = SearchPathW(
            windows::core::PCWSTR::null(),
            windows::core::PCWSTR(wide.as_ptr()),
            windows::core::PCWSTR::null(),
            Some(&mut buffer),
            None,
        );
        if len > 0 {
            String::from_utf16_lossy(&buffer[..len as usize])
        } else {
            path.to_string()
        }
    }
}

fn extract_icon(path: &str) -> Option<IconData> {
    let resolved = resolve_to_full_path(path);
    unsafe {
        let wide_path: Vec<u16> = resolved.encode_utf16().chain(std::iter::once(0)).collect();

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

fn bgra_to_png(data: &IconData) -> Option<Vec<u8>> {
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

    Some(png_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_batch_binary_empty() {
        let buf = encode_batch_binary(&[]);
        assert_eq!(buf.len(), 4);
        assert_eq!(u32::from_le_bytes(buf[0..4].try_into().unwrap()), 0);
    }

    #[test]
    fn encode_batch_binary_mixed() {
        let png1 = vec![0x89, 0x50, 0x4E, 0x47]; // fake PNG header
        let results: Vec<Option<&[u8]>> = vec![Some(&png1), None, Some(&png1)];
        let buf = encode_batch_binary(&results);

        let mut offset = 0;
        let count = u32::from_le_bytes(buf[offset..offset + 4].try_into().unwrap());
        offset += 4;
        assert_eq!(count, 3);

        // item 0: Some
        assert_eq!(buf[offset], 1);
        offset += 1;
        let len = u32::from_le_bytes(buf[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;
        assert_eq!(&buf[offset..offset + len], &png1);
        offset += len;

        // item 1: None
        assert_eq!(buf[offset], 0);
        offset += 1;

        // item 2: Some
        assert_eq!(buf[offset], 1);
        offset += 1;
        let len = u32::from_le_bytes(buf[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;
        assert_eq!(&buf[offset..offset + len], &png1);
        offset += len;

        assert_eq!(offset, buf.len());
    }

    #[test]
    fn invalidate_icon_cache_clears_in_memory_state() {
        // メモリ内にキャッシュがある状態で invalidate すると None になる
        // （次の get_icons_batch で icons.bin から再ロードされる）。
        // bin_file=None でファイル削除はスキップ（テストは実 icons.bin に触れない）。
        let state: IconCacheState = Mutex::new(Some(IconCache {
            data: IconCacheData::default(),
            dirty: false,
        }));
        invalidate_icon_cache_with(&state, None);
        assert!(
            state.lock().unwrap().is_none(),
            "invalidate_icon_cache must clear the in-memory IconCacheState to None"
        );
    }
}
