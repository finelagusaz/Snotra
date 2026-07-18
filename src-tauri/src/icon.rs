//! ファイルアイコンのオンデマンド抽出（`SHGetFileInfoW` → PNG バイト列）とキャッシュ永続化
//! （`icons.bin`）。
//!
//! 検索結果表示時に遅延ロードする。`icons.bin` は src-tauri の資源（snotra-core は触れない）。
//! `invalidate_icon_cache` はメモリ内キャッシュと `icons.bin` を単一ロック内で原子的に無効化し、
//! 並行ロードが旧ファイルをメモリへ戻す TOCTOU を防ぐ（#522）。

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
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

// `png` は挿入順を保持する `IndexMap`。HashMap と postcard の wire 形式は byte 互換
// （どちらも serde の `serialize_map`）なので `ICON_VERSION` バンプ不要で、旧 v5 `icons.bin`
// も読める。挿入順保持により FIFO 退避（最古から pop）と順序の永続化が自然に成立する。
#[derive(Serialize, Deserialize, Default)]
struct IconCacheData {
    png: IndexMap<String, Vec<u8>>,
}

pub struct IconCache {
    data: IconCacheData,
    /// 最大保持件数。超過時は挿入順で最古から退避する。永続化しない runtime config
    /// （`load(cap)` で注入。値は `Config::icon_cache_cap()` が表示ワーキングセットから派生する）。
    cap: usize,
    dirty: bool,
}

impl IconCache {
    /// 永続化済みキャッシュのロードを試み、失敗時は空キャッシュを返す。アイコン抽出では決してブロックしない。
    /// `cap` 超過の既存 `icons.bin` はロード時点で切り詰め、常駐メモリを即時に頭打ちにする。
    pub fn load(cap: usize) -> Self {
        let data = icon_bin_file()
            .and_then(|bf| bf.load::<IconCacheData>())
            .unwrap_or_default();
        let mut cache = Self {
            data,
            cap,
            dirty: false,
        };
        // ロード時点で cap を適用。切り詰めたら dirty を立て、次回 save で永続側も頭打ちにする。
        cache.enforce_cap();
        cache
    }

    /// パスに対応するキャッシュ済み PNG バイト列を返す（読み取り専用・抽出はしない）。
    /// **read-only を厳守**: アクセス順を更新しない（更新すると検索 Step1 の read lock が
    /// write lock に変質し性能退行する）。退避は `insert` / `load`（`&mut self`）に限定する。
    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.data.png.get(path).map(|v| v.as_slice())
    }

    /// Insert extracted PNG bytes into the cache, enforce the cap, and mark dirty.
    pub fn insert(&mut self, path: String, png: Vec<u8>) {
        self.data.png.insert(path, png);
        self.enforce_cap();
        self.dirty = true;
    }

    /// 件数上限を適用し、超過分を挿入順で最古から一括退避する。退避した件数を返す。
    /// 退避が発生したときのみ dirty を立てる（無駄な save を避ける）。
    fn enforce_cap(&mut self) -> usize {
        let excess = self.data.png.len().saturating_sub(self.cap);
        if excess > 0 {
            // 先頭（最古挿入）から excess 件を一括退避（O(n) 一括）。
            self.data.png.drain(0..excess);
            self.dirty = true;
        }
        excess
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
/// 両操作は**単一 lock 内で原子的に**行う（→ `invalidate_icon_cache_with` の doc、#522）。
pub fn invalidate_icon_cache(icons: &IconCacheState) {
    invalidate_icon_cache_with(icons, icon_bin_file());
}

/// テスト可能な内部実装。`bin_file` を `None` で渡すとファイル削除をスキップする。
///
/// **ファイル削除まで lock 保持中に行う**（#522）。旧実装の「None 化 → unlock →
/// 削除」では、unlock〜削除の窓で `ensure_icon_cache_loaded_if_enabled`（同じ lock で
/// None 検知 → `icons.bin` ロード）が削除直前の旧ファイルをメモリへ戻せた
/// （実測 17/2000 回）。削除 → None 化を同一 critical section に置くことで、
/// **`remove()` が成功した場合**「None の観測 = `icons.bin` は削除済み」が成立し、
/// 旧データの再ロード・終了時 `save_if_dirty` での再永続化が構造的に起こらない。
/// `remove()` 失敗時（AV の sharing violation 等）は次ロードが旧ファイルを読む —
/// これは修正前から存在する既知の残余で #522 のスコープ外。
/// `remove()` は `%APPDATA%` へのローカル `remove_file` 1 回で、lock 内 I/O として軽量。
fn invalidate_icon_cache_with(icons: &IconCacheState, bin_file: Option<BinFile>) {
    let mut guard = icons.lock().unwrap();
    if let Some(bf) = bin_file {
        bf.remove();
    }
    *guard = None;
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

    /// issue #522 の回帰テスト: invalidate（ファイル削除 + メモリ None 化）と
    /// 並行ロード（None 検知 → icons.bin ロード）を並走させ、「icons.bin 不在なのに
    /// メモリへ旧データが残存する」interleaving が存在しないことを確認する。
    /// 修正前は「None 化 → unlock → 削除」の窓で 17/2000 回再現した（issue 実測）。
    /// loader は ensure_icon_cache_loaded_if_enabled と同手順を temp BinFile 上で
    /// 再構成する（本物は icon_bin_file() 固定パス依存のため temp 注入が効かない）。
    #[test]
    fn invalidate_is_atomic_with_concurrent_load() {
        use std::sync::Arc;

        // dir 名に process id を含め、並列テスト・過去の残骸との衝突を避ける
        let dir = std::env::temp_dir().join(format!("snotra_icon_522_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut hits = 0;
        const ITERS: usize = 1000;
        for _ in 0..ITERS {
            let make_bf = || BinFile::new_in(&dir, ICON_MAGIC, ICON_VERSION, "icons.bin");
            let mut old = IconCacheData::default();
            old.png.insert("old.exe".into(), vec![1, 2, 3]);
            assert!(make_bf().save(&old));

            let state: Arc<IconCacheState> = Arc::new(Mutex::new(Some(IconCache {
                data: IconCacheData::default(),
                cap: 100,
                dirty: false,
            })));
            let s2 = Arc::clone(&state);
            let dir2 = dir.clone();
            let loader = std::thread::spawn(move || {
                loop {
                    let mut g = s2.lock().unwrap();
                    if g.is_none() {
                        let bf = BinFile::new_in(&dir2, ICON_MAGIC, ICON_VERSION, "icons.bin");
                        let data: IconCacheData = bf.load().unwrap_or_default();
                        *g = Some(IconCache { data, cap: 100, dirty: false });
                        break;
                    }
                    drop(g);
                    std::hint::spin_loop();
                }
            });

            invalidate_icon_cache_with(&state, Some(make_bf()));
            loader.join().unwrap();

            let file_exists = dir.join("icons.bin").exists();
            let mem_has_old = state
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|c| c.get("old.exe").is_some());
            if !file_exists && mem_has_old {
                hits += 1;
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            hits, 0,
            "TOCTOU: {}/{} 回、削除済み icons.bin の内容がメモリに残存した（None 観測 = 削除済み の不変条件違反）",
            hits, ITERS
        );
    }

    /// issue #522: 無効化後の事後条件（ファイル不在 かつ メモリ None）の決定論検証。
    #[test]
    fn invalidate_removes_file_and_clears_memory() {
        let dir = std::env::temp_dir()
            .join(format!("snotra_icon_522_det_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let bf = BinFile::new_in(&dir, ICON_MAGIC, ICON_VERSION, "icons.bin");
        let mut old = IconCacheData::default();
        old.png.insert("old.exe".into(), vec![1, 2, 3]);
        assert!(bf.save(&old));

        let state: IconCacheState = Mutex::new(Some(IconCache {
            data: IconCacheData::default(),
            cap: 100,
            dirty: false,
        }));
        invalidate_icon_cache_with(
            &state,
            Some(BinFile::new_in(&dir, ICON_MAGIC, ICON_VERSION, "icons.bin")),
        );

        assert!(!dir.join("icons.bin").exists(), "icons.bin が削除されている");
        assert!(state.lock().unwrap().is_none(), "メモリキャッシュが None");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalidate_icon_cache_clears_in_memory_state() {
        // メモリ内にキャッシュがある状態で invalidate すると None になる
        // （次の get_icons_batch で icons.bin から再ロードされる）。
        // bin_file=None でファイル削除はスキップ（テストは実 icons.bin に触れない）。
        let state: IconCacheState = Mutex::new(Some(IconCache {
            data: IconCacheData::default(),
            cap: 1000,
            dirty: false,
        }));
        invalidate_icon_cache_with(&state, None);
        assert!(
            state.lock().unwrap().is_none(),
            "invalidate_icon_cache must clear the in-memory IconCacheState to None"
        );
    }

    /// テスト用に cap 指定で空キャッシュを構築する（ファイル I/O を伴わない）。
    fn empty_cache_with_cap(cap: usize) -> IconCache {
        IconCache {
            data: IconCacheData::default(),
            cap,
            dirty: false,
        }
    }

    #[test]
    fn insert_evicts_oldest_when_over_cap() {
        let mut cache = empty_cache_with_cap(2);
        cache.insert("a".into(), vec![1]);
        cache.insert("b".into(), vec![2]);
        cache.insert("c".into(), vec![3]); // cap 超過 → 最古 "a" を退避

        assert_eq!(cache.data.png.len(), 2, "cap を超えない");
        assert!(cache.get("a").is_none(), "最古挿入の a が退避される");
        assert_eq!(cache.get("b"), Some(&[2][..]));
        assert_eq!(cache.get("c"), Some(&[3][..]));
        // 残存は挿入順 b, c
        let keys: Vec<&String> = cache.data.png.keys().collect();
        assert_eq!(keys, vec![&"b".to_string(), &"c".to_string()]);
    }

    #[test]
    fn insert_within_cap_keeps_all() {
        let mut cache = empty_cache_with_cap(3);
        cache.insert("a".into(), vec![1]);
        cache.insert("b".into(), vec![2]);
        assert_eq!(cache.data.png.len(), 2);
        assert!(cache.get("a").is_some());
        assert!(cache.get("b").is_some());
    }

    #[test]
    fn enforce_cap_trims_when_over_cap_and_marks_dirty() {
        // load 経由の切り詰めをエミュレート: 3 件持つ cap=2 のキャッシュで enforce_cap。
        let mut cache = empty_cache_with_cap(2);
        cache.data.png.insert("a".into(), vec![1]);
        cache.data.png.insert("b".into(), vec![2]);
        cache.data.png.insert("c".into(), vec![3]);
        cache.dirty = false; // 直接挿入では立てていない

        let evicted = cache.enforce_cap();
        assert_eq!(evicted, 1, "超過 1 件を退避");
        assert_eq!(cache.data.png.len(), 2);
        assert!(cache.get("a").is_none(), "最古 a が退避");
        assert!(cache.dirty, "退避したら dirty を立てる（永続側も頭打ちにする）");
    }

    #[test]
    fn enforce_cap_noop_keeps_dirty_unchanged() {
        let mut cache = empty_cache_with_cap(5);
        cache.data.png.insert("a".into(), vec![1]);
        cache.dirty = false;
        let evicted = cache.enforce_cap();
        assert_eq!(evicted, 0, "cap 以内なら退避なし");
        assert!(!cache.dirty, "退避なしなら dirty を立てない（無駄な save を避ける）");
    }

    #[test]
    fn get_does_not_mutate() {
        let mut cache = empty_cache_with_cap(3);
        cache.insert("a".into(), vec![1]);
        cache.insert("b".into(), vec![2]);
        let before: Vec<String> = cache.data.png.keys().cloned().collect();
        let _ = cache.get("a"); // アクセスしても順序・件数は不変
        let _ = cache.get("missing");
        let after: Vec<String> = cache.data.png.keys().cloned().collect();
        assert_eq!(before, after, "get は read-only（順序・件数を変えない）");
    }

    #[test]
    fn retain_paths_preserves_cap_invariant() {
        let mut cache = empty_cache_with_cap(2);
        cache.insert("a".into(), vec![1]);
        cache.insert("b".into(), vec![2]); // len == cap

        let valid: std::collections::HashSet<String> = ["b".to_string()].into_iter().collect();
        cache.retain_paths(&valid);
        assert!(cache.data.png.len() <= cache.cap, "retain 後も cap 不変条件を満たす");
        assert!(cache.get("a").is_none());
        assert!(cache.get("b").is_some());
    }

    #[test]
    fn wire_compat_hashmap_format_loads() {
        // 受け入れ条件7: 旧 v5 icons.bin（HashMap 書き込み）が IndexMap 化後も読める。
        // HashMap を持つヘルパー struct でバイト列化し、IndexMap 版 IconCacheData で読み戻す。
        use snotra_core::binfmt::{try_deserialize_with_header, try_serialize_with_header};
        use std::collections::HashMap;

        #[derive(serde::Serialize)]
        struct LegacyIconCacheData {
            png: HashMap<String, Vec<u8>>,
        }

        let mut legacy = HashMap::new();
        legacy.insert("c:/a.exe".to_string(), vec![1u8, 2, 3]);
        legacy.insert("c:/b.exe".to_string(), vec![4u8, 5]);
        let bytes = try_serialize_with_header(
            ICON_MAGIC,
            ICON_VERSION,
            &LegacyIconCacheData { png: legacy },
        )
        .expect("serialize legacy");

        let restored: IconCacheData =
            try_deserialize_with_header(&bytes, ICON_MAGIC, ICON_VERSION)
                .expect("deserialize into IndexMap-backed IconCacheData");
        assert_eq!(restored.png.len(), 2);
        assert_eq!(restored.png.get("c:/a.exe"), Some(&vec![1, 2, 3]));
        assert_eq!(restored.png.get("c:/b.exe"), Some(&vec![4, 5]));
    }
}
