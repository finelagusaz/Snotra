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
use windows::Win32::Foundation::GetLastError;
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

    /// 現在保持しているパスの snapshot。
    ///
    /// **owned で返すのは、呼び出し側が lock を離してから判定するためである**——判定（`IndexTree::absent_paths`）は索引の全件走査ゆえ、lock の中で回すと表示中のアイコン取得がその間ずっと待たされる。**判定に使う集合はこの snapshot であって現在のキャッシュではない**——そのずれが安全である理由は [`Self::remove_paths`] の doc が正本とする。
    ///
    /// **確保はここへ移っただけで消えていない。** キャッシュの全キー（高々 `Config::icon_cache_cap()`・既定 1,000 件）を clone する。lock 保持を 36 ms 削る対価であって、確保回数の削減ではない。
    pub fn keys(&self) -> Vec<String> {
        self.data.png.keys().cloned().collect()
    }

    /// 渡した集合のパスを除去する。
    /// `clear()` と異なり有効なアイコンを再利用するため、再構築後の再抽出コストを削減する。
    /// 1 件でも除去した場合は dirty フラグを立てる。
    ///
    /// **`dead_paths` の意味は「索引に無い」であって「消えた」ではない。** 呼び出し側（[`sync_with_index`]）が渡すのは `IndexTree::absent_paths` の結果で、そこには**索引に載るとは限らないパス**——フォルダを掘って表示した行のアイコン——も入る（経路は同メソッドの doc）。**名前を「死」と読むと事実より強い。**
    ///
    /// **「残す集合」ではなく「落とす集合」を受け取るのが要石である。** 判定を lock の外で行う以上、渡される集合は [`Self::keys`] を取った時点の snapshot から導かれており、その後に挿入されたキーを知らない。**残す集合で書くと、その新しいキーは「残す集合に無い」ゆえ落ちる**——落とす集合で書けば、知らないキーは落とす集合にも居ないので残る。
    ///
    /// **旧実装（lock を握ったままの剪定）との異同を、ここでは主張しない。** 3 巡のレビューで**軸ごとに違う答えになり、書くたびに強すぎるか弱すぎるかへ振れた**——挿入の着地順・世代交代・FIFO 退避・終了時 flush はそれぞれ別の答えを持つ。読者が要るのは比較ではなく、**今この関数が何を保証し、何を保証しないか**である。
    ///
    /// # 受容する残余（どれも構造では塞いでいない）
    ///
    /// **1. 判定の窓を跨いだキーは剪定を素通りする。** snapshot を取った後に挿入されたキーは `dead_paths` に居ないので残る（それが上の要石の裏返しである）。**片づける契機は複数あるが**（次の索引再構築・`invalidate_icon_cache`・`show_icons` を偽にする設定変更）、**どれも来ない期間はいくらでも長くなりうる**——`enforce_cap` は FIFO ゆえ索引を見ず、`save_if_dirty` はむしろ `icons.bin` へ書く。害は cap（既定 1,000）件に有界。**ただし「索引に無いから読まれない」とは書けない**——フォルダ階層モードの行は索引に無いパスを表示してアイコンを引くので、生き延びた古い PNG がそこで返りうる。
    ///
    /// **2. 窓の間にキャッシュが世代交代することがある。** `invalidate_icon_cache` が `None` と `icons.bin` 削除を撃つと、次の表示が `IconCache::load` で**別の世代**を建てる——呼び出し側の `as_mut()` は `None` を弾くだけで、世代の違いは見ない。**消えるものは正しいが、時期が早い**: `dead_paths` の要素は (a) 古い世代に在り (b) 呼び出し側がこれから差し替える索引に無いパスなので、消してよいものである。代償は再抽出で、**それがいつ走るかは表示側の都合で決まる**（`results_view` は既にテクスチャを持つ行を弾く一方、結果集合が変われば `retain_visible` がそのテクスチャを捨てる）。
    ///
    /// **3. 終了時の flush と競合しうる。** `main::flush_persistent_state` が窓の間に lock を取ると剪定前の `icons.bin` を書いて `dirty` を落とし、その後の除去は `exit(0)` に追われて保存されない。帰結は残余 1 と同じ（索引に無いエントリが 1 セッション分残る）。
    ///
    /// **4. lock の構造そのものを守る検知器は無い。** 2 回の lock 取得を 1 回へ戻す退行——この関数が買ったものを丸ごと失い、かつ「残す集合」の形へ戻る入口——は全テスト緑のまま通る。検知器（`concurrent_insert_during_prune_window_survives`）が測るのは述語の向きだけで、その射程は同テストの doc が持つ。
    pub fn remove_paths(&mut self, dead_paths: &std::collections::HashSet<String>) {
        let before = self.data.png.len();
        self.data.png.retain(|k, _| !dead_paths.contains(k));
        if self.data.png.len() < before {
            self.dirty = true;
        }
    }
}

/// アイコン抽出が失敗した段階（#692）。**`None` に潰すと「アイコンが無い」と
/// 「一時的に取れなかった」が区別できず、呼び出し側が恒久的な欠落として latch する。**
/// 失敗の 3 分類は `is_transient` が担う（呼び出し側が再試行の可否を決める材料）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconFailure {
    /// `SHGetFileInfoW` が 0 を返した（パス不在・アクセス不可を含む）。
    ShellQueryFailed(u32),
    /// 戻り値は非 0 だが HICON が無効。
    NoIconHandle,
    /// `GetIconInfo` 失敗。
    IconInfoFailed(u32),
    /// `CreateCompatibleDC` 失敗（GDI リソース枯渇の疑い）。
    CreateDcFailed(u32),
    /// カラービットマップが無い（モノクロアイコン）。
    NoColorBitmap,
    /// `GetDIBits` が 0 行を返した（**従来は戻り値を捨てていた**）。
    GetDiBitsFailed(u32),
    /// 取得できたが全画素ゼロ。
    AllPixelsZero,
    /// PNG エンコード失敗。
    PngEncodeFailed,
}

impl IconFailure {
    /// 再試行で解消しうるか。`false` は「このパスにアイコンは無い」に近い恒久的失敗。
    pub fn is_transient(self) -> bool {
        match self {
            // GDI/DC 系は資源枯渇で一時的に落ちうる。shell 問い合わせも同様（不在は
            // 呼び出し側が Path::exists() で切り分ける）。
            Self::CreateDcFailed(_) | Self::GetDiBitsFailed(_) | Self::IconInfoFailed(_) => true,
            Self::ShellQueryFailed(_) | Self::NoIconHandle => true,
            // モノクロ・全ゼロ・エンコード失敗はそのアイコン固有で、再試行しても同じ。
            Self::NoColorBitmap | Self::AllPixelsZero | Self::PngEncodeFailed => false,
        }
    }
}

/// Extract PNG bytes for a path without holding any lock.
pub fn extract_png(path: &str) -> Result<Vec<u8>, IconFailure> {
    let icon_data = extract_icon(path)?;
    bgra_to_png(&icon_data).ok_or(IconFailure::PngEncodeFailed)
}

/// Managed state for icon cache
pub type IconCacheState = Mutex<Option<IconCache>>;

/// 索引の再構築に合わせてアイコンキャッシュを揃える（`indexing::drain_index` の 1 手順）。
///
/// **判定は lock の外で行う。** 索引の全件走査（312,625 件・実測 36 ms）を lock の中で回すと、表示中のアイコン取得（`commands::icon::load_icon_pngs` の Step 1/3）がその間ずっと待つ。lock を持つのは snapshot と除去の一瞬だけにする。**その窓が安全である理由と受容残余は [`IconCache::remove_paths`] の doc が正本とする。**
///
/// **`show_icons` が偽なら丸ごと捨てる**（snapshot も判定も要らない）。
pub fn sync_with_index(
    icons: &IconCacheState,
    show_icons: bool,
    tree: &snotra_core::index_tree::IndexTree,
) {
    if !show_icons {
        *icons.lock().unwrap() = None;
        return;
    }
    let Some(keys) = icons.lock().unwrap().as_ref().map(IconCache::keys) else {
        return;
    };
    let dead = tree.absent_paths(keys);
    // **消すものが無ければ lock を取らない。** 空になるのは、キャッシュのキーが 1 件残らず新しい索引に在るときだけである（**頻度は書かない**——キャッシュにはフォルダ階層モードの行のような索引に載るとは限らないパスも入り、それが索引と一致するかは掘った場所で決まる）。それでも置くのは、空のときに払うのがこの関数が避けている当の lock だからである。
    //
    // 窓の間に `invalidate_icon_cache` / show_icons=false が `None` へ落としていることがあるので、取り直して確かめる。
    if !dead.is_empty()
        && let Some(c) = icons.lock().unwrap().as_mut()
    {
        c.remove_paths(&dead);
    }
}

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
/// `remove()` は既定では `%APPDATA%` へのローカル `remove_file` 1 回で、lock 内 I/O として軽量
/// （`SNOTRA_CONFIG_DIR` で保存先を遠い場所へ向けた場合はこの前提が崩れる・`SPEC.md` §13）。
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

/// アイコンを抽出する。**`NoIconHandle` は 1 回だけ即時リトライする**（#692）。
///
/// `SHGetFileInfoW` は**成功（非 0）を返しながら HICON を返さない**ことがある——プロセス
/// ごとに冷えたシェルのアイコンキャッシュに対する初回要求で起きる。実測（#692）:
///
/// - 実機 1 セッションで 17 件が `NoIconHandle`（いずれも `exists=true`）
/// - 396 パス × 6 周のうち**失敗するのは周 1 だけ**（16 件 → 以降 0 件）。新しいプロセスで
///   再び周 1 が失敗する＝**キャッシュはプロセスごと**
/// - **0ms の即時リトライで 15/15 回復**（待ち時間は不要。回復しない残り 2 件はパスが実在
///   しないもので、これは恒久的失敗として正しい）
///
/// リトライしないと呼び出し側が「アイコンが無い」と解釈して恒久的に latch し、その行は
/// グレーのプレースホルダのままになる（本 issue の症状）。
fn extract_icon(path: &str) -> Result<IconData, IconFailure> {
    // 3 回まで（= リトライ 2 回）。実測では 2 回目でほぼ全て成功するが、並列負荷下の
    // run によっては 1 リトライ後も数件残ったため 1 回ぶん余裕を持たせる。**待ち時間は
    // 置かない**（0ms リトライで回復することを実測済み・待っても回復率は変わらない）。
    // リトライするのは `NoIconHandle` だけ——`ShellQueryFailed`（パス不在）は何度呼んでも
    // 同じで、無駄なシェル問い合わせを増やすだけである（実測: 6 回試しても回復しない）。
    let mut last = extract_icon_once(path);
    for _ in 0..2 {
        if !matches!(last, Err(IconFailure::NoIconHandle)) {
            break;
        }
        last = extract_icon_once(path);
    }
    last
}

fn extract_icon_once(path: &str) -> Result<IconData, IconFailure> {
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

        if result == 0 {
            return Err(IconFailure::ShellQueryFailed(GetLastError().0));
        }
        if shfi.hIcon.is_invalid() {
            return Err(IconFailure::NoIconHandle);
        }

        let icon_data = hicon_to_bgra(shfi.hIcon);
        let _ = DestroyIcon(shfi.hIcon);
        icon_data
    }
}

fn hicon_to_bgra(hicon: HICON) -> Result<IconData, IconFailure> {
    unsafe {
        let mut icon_info = ICONINFO::default();
        if GetIconInfo(hicon, &mut icon_info).is_err() {
            return Err(IconFailure::IconInfoFailed(GetLastError().0));
        }

        let _cleanup = BitmapCleanup(&icon_info);

        let hdc_screen = CreateCompatibleDC(None);
        if hdc_screen.is_invalid() {
            return Err(IconFailure::CreateDcFailed(GetLastError().0));
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

        if icon_info.hbmColor.is_invalid() {
            let _ = DeleteDC(hdc_screen);
            return Err(IconFailure::NoColorBitmap);
        }
        let old = SelectObject(hdc_screen, icon_info.hbmColor.into());
        // **戻り値（コピーできた走査行数）を捨てない**（#692）。0 は失敗であり、
        // 捨てると pixels が初期値ゼロのまま「全画素ゼロ」に化けて理由が消える。
        let scanlines = GetDIBits(
            hdc_screen,
            icon_info.hbmColor,
            0,
            height,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );
        let dib_error = if scanlines == 0 {
            Some(GetLastError().0)
        } else {
            None
        };
        SelectObject(hdc_screen, old);

        let _ = DeleteDC(hdc_screen);

        if let Some(code) = dib_error {
            return Err(IconFailure::GetDiBitsFailed(code));
        }

        let has_data = pixels.iter().any(|&b| b != 0);
        if !has_data {
            return Err(IconFailure::AllPixelsZero);
        }

        Ok(IconData {
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

    /// 計測列のパーセンタイル（`p` は 0.0..=1.0）。空なら 0。
    /// **両プローブが共有する**——測る対象（全区間 vs 区間別）は別だが、この算法は
    /// 測定対象に依存しない。
    fn pctl(mut v: Vec<u128>, p: f64) -> u128 {
        if v.is_empty() {
            return 0;
        }
        v.sort_unstable();
        let idx = ((v.len() as f64 - 1.0) * p).round() as usize;
        v[idx]
    }

    /// #692 の再現ハーネス（`#[ignore]`・環境依存ゆえ CI では走らせない）。
    ///
    /// シェルのアイコンキャッシュは**プロセスごとに冷えており**、初回要求で
    /// `SHGetFileInfoW` が成功を返しながら HICON を返さないことがある（`NoIconHandle`）。
    /// 修正（`extract_icon` の即時リトライ）が効いていることは、**冷えたプロセスで
    /// 1 周目に `NoIconHandle` が出ないこと**で確かめる——温まった後では区別が付かない。
    ///
    /// ```text
    /// $env:SNOTRA_ICON_DIAG_PATHS = "C:\path\a;C:\path\b"   # 省略時は既定の 5 件
    /// cargo test -p snotra diagnose_icon_cold_start -- --ignored --nocapture
    /// ```
    ///
    /// 実測（2026-07-26・396 パス）: 修正前は 1 周目に 16〜18 件の `NoIconHandle`、
    /// 修正後は 0 件（残る `ShellQueryFailed` はパス不在で、恒久的失敗として正しい）。
    #[test]
    #[ignore]
    fn diagnose_icon_cold_start() {
        use rayon::prelude::*;
        use std::collections::BTreeMap;

        let default = [
            r"C:\Windows\explorer.exe",
            r"C:\Windows\notepad.exe",
            r"C:\Windows\System32\cmd.exe",
            r"C:\Windows",
            r"C:\Program Files",
        ]
        .join(";");
        let paths: Vec<String> = std::env::var("SNOTRA_ICON_DIAG_PATHS")
            .unwrap_or(default)
            .split(';')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_owned())
            .collect();
        println!("対象 {} 件", paths.len());

        // **最初に測る**——先に別の pass を走らせるとシェルのキャッシュが温まり、
        // 「冷えた初回」の観測ができなくなる（計測順序そのものが観測対象を変える）。
        for round in 1..=2 {
            let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
            let failures: Vec<(String, IconFailure)> = paths
                .par_iter()
                .filter_map(|p| extract_png(p).err().map(|e| (p.clone(), e)))
                .collect();
            for (_, e) in &failures {
                let label = format!("{e:?}");
                let kind = label.split('(').next().unwrap_or(&label).to_owned();
                *kinds.entry(kind).or_default() += 1;
            }
            println!("  周 {round}: 失敗 {} 件 {kinds:?}", failures.len());
            for (p, e) in failures.iter().take(5) {
                println!("    {e:?} exists={} {p}", std::path::Path::new(p).exists());
            }
        }
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
                        *g = Some(IconCache {
                            data,
                            cap: 100,
                            dirty: false,
                        });
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
        let dir = std::env::temp_dir().join(format!("snotra_icon_522_det_{}", std::process::id()));
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

        assert!(
            !dir.join("icons.bin").exists(),
            "icons.bin が削除されている"
        );
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
        assert!(
            cache.dirty,
            "退避したら dirty を立てる（永続側も頭打ちにする）"
        );
    }

    /// `sync_with_index` の分岐を固定する。**空の木を渡すと全キーが `dead` になる**ので、
    /// 判定そのものを通したうえで各分岐を数行で押さえられる。
    #[test]
    fn sync_with_index_drops_the_cache_when_icons_are_disabled() {
        let state: IconCacheState = Mutex::new(Some(empty_cache_with_cap(4)));
        state
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .insert("a".into(), vec![1]);

        sync_with_index(&state, false, &snotra_core::index_tree::IndexTree::empty());

        assert!(
            state.lock().unwrap().is_none(),
            "show_icons=false ではキャッシュごと捨てる（剪定しない）"
        );
    }

    #[test]
    fn sync_with_index_is_a_noop_when_the_cache_is_absent() {
        let state: IconCacheState = Mutex::new(None);
        sync_with_index(&state, true, &snotra_core::index_tree::IndexTree::empty());
        assert!(
            state.lock().unwrap().is_none(),
            "遅延ロード前は何も起こさない（ここでロードしない）"
        );
    }

    #[test]
    fn sync_with_index_removes_keys_absent_from_the_tree() {
        let state: IconCacheState = Mutex::new(Some(empty_cache_with_cap(4)));
        state
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .insert("a".into(), vec![1]);

        sync_with_index(&state, true, &snotra_core::index_tree::IndexTree::empty());

        let guard = state.lock().unwrap();
        let cache = guard
            .as_ref()
            .expect("キャッシュは残る（捨てるのは disabled 側だけ）");
        assert!(cache.get("a").is_none(), "空の木では全キーが索引に無い");
    }

    #[test]
    fn enforce_cap_noop_keeps_dirty_unchanged() {
        let mut cache = empty_cache_with_cap(5);
        cache.data.png.insert("a".into(), vec![1]);
        cache.dirty = false;
        let evicted = cache.enforce_cap();
        assert_eq!(evicted, 0, "cap 以内なら退避なし");
        assert!(
            !cache.dirty,
            "退避なしなら dirty を立てない（無駄な save を避ける）"
        );
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
    fn remove_paths_preserves_cap_invariant() {
        let mut cache = empty_cache_with_cap(2);
        cache.insert("a".into(), vec![1]);
        cache.insert("b".into(), vec![2]); // len == cap

        let dead: std::collections::HashSet<String> = ["a".to_string()].into_iter().collect();
        cache.remove_paths(&dead);
        assert!(
            cache.data.png.len() <= cache.cap,
            "除去後も cap 不変条件を満たす"
        );
        assert!(cache.get("a").is_none());
        assert!(cache.get("b").is_some());
    }

    /// **剪定の判定が lock の外で走る窓を検査する。** `keys` を取ってから `remove_paths` までの間に挿入されたキーは、判定の入力に居ない——それでも落ちてはならない（正本は `remove_paths` の doc）。
    ///
    /// **これは「落とす集合」で書いたことだけが与える性質である。** 下の `cache.remove_paths(&dead)` を「残す集合」の形（`retain(|k, _| alive.contains(k))`・＝候補表が否定した変種）へ差し替えると、`late` の断言で落ちる——**実際に注入して確かめてある**（2026-08-09 実測）。
    ///
    /// **ただし変異させたのはこのテストの中であって production ではない。** production 側だけを壊す形（`remove_paths` の述語反転）では先行する 2 つの断言が先に落ちるので、`late` の行は**この検知器が守る性質を書き留めてはいるが、production の退行で発火する保証は無い**——実際の退行経路（呼び出し側が `alive` を組んで渡す形へ戻す）はこのテストの書き換えを伴う。**受容する残余である。**
    #[test]
    fn concurrent_insert_during_prune_window_survives() {
        let mut cache = empty_cache_with_cap(8);
        cache.insert("alive".into(), vec![1]);
        cache.insert("gone".into(), vec![2]);

        // lock の外で判定する側が見る snapshot。
        let snapshot = cache.keys();
        assert_eq!(snapshot.len(), 2);

        // 窓の間に別スレッドが挿入した（＝判定の入力に居ない）キー。
        cache.insert("late".into(), vec![3]);

        // 索引には "alive" しか無かった、という判定結果。
        let dead: std::collections::HashSet<String> = snapshot
            .into_iter()
            .filter(|k| k != "alive")
            .collect::<std::collections::HashSet<_>>();
        cache.remove_paths(&dead);

        assert!(cache.get("gone").is_none(), "索引に無いキーは落ちる");
        assert!(cache.get("alive").is_some(), "索引に在るキーは残る");
        assert!(
            cache.get("late").is_some(),
            "判定の窓の間に挿入されたキーは、判定の入力に居なくても落ちてはならない"
        );
    }

    /// #532 SU4 Probe 1: アイコン抽出（`SHGetFileInfoW`→BGRA→PNG 全区間）の実コスト計測。
    /// `cargo test -p snotra --release icon_extract_cost_probe -- --ignored --nocapture` で実行。
    /// 判定: 8 件バッチ warm 合計が 1 フレーム予算 16.7ms に十分収まるなら update() 内同期が
    /// 候補（worker 不要）。dead-UNC 論（下記 note）はこの数字と別に評価する。
    #[test]
    #[ignore = "計測プローブ（実機・release 実行専用）"]
    fn icon_extract_cost_probe() {
        use std::time::Instant;

        // 代表パス: exe（大半のヒット）・folder・doc・.lnk（対象解決）。
        // SHGetFileInfoW はファイルパスにバックスラッシュを要求する（indexer 由来の実パスと同形）。
        let exes = [
            r"C:\Windows\System32\notepad.exe",
            r"C:\Windows\System32\calc.exe",
            r"C:\Windows\System32\cmd.exe",
            r"C:\Windows\explorer.exe",
        ];
        let folders = [r"C:\Windows", r"C:\Windows\System32"];
        let doc = r"C:\Windows\System32\drivers\etc\hosts";
        let lnk = r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\AdGuard.lnk";

        // 型別 warm 計測: 各パスを 1 回 prime してから N 回測る（shell キャッシュ warm）。
        let measure = |label: &str, paths: &[&str]| {
            for p in paths {
                let _ = extract_png(p); // prime（cold を除外）
            }
            let mut us = Vec::new();
            for _ in 0..50 {
                for p in paths {
                    let t = Instant::now();
                    let out = extract_png(p);
                    us.push(t.elapsed().as_micros());
                    assert!(
                        out.is_ok(),
                        "{p} の抽出が失敗（テスト前提の実在パス）: {out:?}"
                    );
                }
            }
            println!(
                "[{label}] warm per-call  p50={}us p95={}us max={}us  (n={})",
                pctl(us.clone(), 0.5),
                pctl(us.clone(), 0.95),
                pctl(us.clone(), 1.0),
                us.len(),
            );
        };

        measure("exe", &exes);
        measure("folder", &folders);
        measure("doc", &[doc]);
        measure("lnk", &[lnk]);

        // 1 結果集合ぶん（8 件）の cold バッチ: 各パス first-touch の合計（shell 未 prime）。
        // 注: 一度触れた shell は同プロセス内でキャッシュされ真の cold は初回のみ。参考値。
        let batch: Vec<&str> = exes
            .iter()
            .chain(folders.iter())
            .chain([doc, lnk].iter())
            .copied()
            .collect();
        // 別プロセス起動での「真 cold」は測れないため、ここでは warm バッチ合計を主指標にする。
        let mut batch_us = Vec::new();
        for _ in 0..30 {
            let t = Instant::now();
            for p in &batch {
                let _ = extract_png(p);
            }
            batch_us.push(t.elapsed().as_micros());
        }
        println!(
            "[batch x{}] warm total     p50={}us p95={}us max={}us  (frame budget=16700us)",
            batch.len(),
            pctl(batch_us.clone(), 0.5),
            pctl(batch_us.clone(), 0.95),
            pctl(batch_us.clone(), 1.0),
        );
    }

    /// アイコンパイプラインの**区間別**コスト計測（#532 SU4 Probe 1 の続き）。`icon_extract_cost_probe` が「抽出全区間」を 1 つの数字で見るのに対し、こちらは**どの区間に伸びしろがあるか**を分けて測る。`cargo test -p snotra --release icon_pipeline_cost_probe -- --ignored --nocapture` で実行。
    ///
    /// 測る区間は 4 つ:
    /// - `shell+gdi`: `extract_icon`（`SHGetFileInfoW` → GDI → BGRA）
    /// - `encode`: `bgra_to_png`（BGRA → RGBA → PNG）。**`icons.bin` 永続化に必要**
    /// - `decode`: `png_to_color_image`（PNG → RGBA → `Color32`）。**miss 経路では
    ///   `encode` の直後に走る往復であり、抽出時の RGBA を渡せば省ける**
    /// - `batch(N) parallel`: 実際の `load_icon_pngs` と同じ rayon 並列で N 件を一気に抽出した
    ///   実時間。N は既定で `effective_result_limit`（200）に合わせる
    ///
    /// 対象パスは既定で**本番と同じ母集団**（`Config::default_scan_paths()` = common Start Menu
    /// ＋ Desktop の `.lnk`）から先頭 200 件を採る。**自前でディレクトリを歩いてはならない**
    /// ——`default_scan_paths` は User Start Menu を意図的に除外しており（同関数の doc）、
    /// 手書きの走査はインデックスに入らないパスを測ってしまう。同じ組み合わせで本番相当の
    /// 母集団を作る先例は `snotra_core::search::tests::performance` の
    /// `measure_lower_name_footprint_report`。`SNOTRA_ICON_DIAG_PATHS` で明示指定もできる
    /// （`icon_extract_cost_probe` と同じ規約）。
    #[test]
    #[ignore = "計測プローブ（実機・release 実行専用）"]
    fn icon_pipeline_cost_probe() {
        use rayon::prelude::*;
        use std::time::Instant;

        let paths: Vec<String> = match std::env::var("SNOTRA_ICON_DIAG_PATHS") {
            Ok(s) => s
                .split(';')
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect(),
            Err(_) => snotra_core::indexer::scan_all(
                &snotra_core::config::Config::default_scan_paths(),
                false,
            )
            .into_iter()
            .map(|e| e.target_path)
            .take(200)
            .collect(),
        };
        assert!(
            !paths.is_empty(),
            "対象パスが 0 件（スタートメニューが空なら SNOTRA_ICON_DIAG_PATHS で明示指定する）"
        );
        println!(
            "対象 {} 件（既定は result_limit=200 に合わせる）",
            paths.len()
        );

        // 温めついでに PNG を回収する（**warm-up と hit 経路の材料を兼ねる**——分けると
        // 最も重い shell+gdi を全件ぶん 1 周余計に払う）。cold 初回は `icon_extract_cost_probe`
        // の担当ゆえ、ここで温まっていることが後続の計測の前提である。
        let pngs: Vec<Vec<u8>> = paths
            .par_iter()
            .filter_map(|p| extract_png(p).ok())
            .collect();

        // 区間別 per-call。**3 区間を 1 本のタプル列に持つ**——別々の Vec に push すると
        // 途中で失敗した path が片方にだけ残り、「同一のアイコンに対する 3 区間の比」という
        // この計測の前提が黙って崩れる。3 区間すべて成功した path だけを積む。
        let mut per_call: Vec<(u128, u128, u128)> = Vec::new();
        for p in &paths {
            let t = Instant::now();
            let Ok(icon) = extract_icon(p) else { continue };
            let shell = t.elapsed().as_micros();

            let t = Instant::now();
            let Some(png) = bgra_to_png(&icon) else {
                continue;
            };
            let encode = t.elapsed().as_micros();

            let t = Instant::now();
            let img = crate::egui_shell::png_to_color_image(&png);
            let decode = t.elapsed().as_micros();
            assert!(
                img.is_some(),
                "自前エンコードの PNG は必ず decode できる: {p}"
            );
            per_call.push((shell, encode, decode));
        }
        for (label, project) in [
            ("shell+gdi", (|t: &(u128, u128, u128)| t.0) as fn(_) -> _),
            ("encode", |t: &(u128, u128, u128)| t.1),
            ("decode", |t: &(u128, u128, u128)| t.2),
        ] {
            let v: Vec<u128> = per_call.iter().map(project).collect();
            println!(
                "[{label:<9}] per-call  p50={}us p95={}us  合計={}us  (n={})",
                pctl(v.clone(), 0.5),
                pctl(v.clone(), 0.95),
                v.iter().sum::<u128>(),
                v.len(),
            );
        }

        // 実バッチ: load_icon_pngs の Step 2 と同じ rayon 並列で全件抽出した実時間。
        // **キャッシュミス時に 1 回の settle が払う実額**である。
        let mut batch_us = Vec::new();
        for _ in 0..5 {
            let t = Instant::now();
            let n = paths.par_iter().filter(|p| extract_png(p).is_ok()).count();
            batch_us.push(t.elapsed().as_micros());
            assert!(
                n > 0,
                "1 件も抽出できていない（対象パスの前提が崩れている）"
            );
        }
        println!(
            "[batch x{} parallel] warm 実時間 p50={}us p95={}us  (frame budget=16700us)",
            paths.len(),
            pctl(batch_us.clone(), 0.5),
            pctl(batch_us.clone(), 0.95),
        );

        // キャッシュヒット経路の decode 単体: icons.bin ヒット時は shell+gdi/encode を払わず
        // decode + load_texture だけになる。load_texture は egui ctx 依存でここでは測れないため、
        // decode 合計を「1 settle ぶんのヒット経路 CPU」の下限として出す。
        // 材料は warm-up で回収済みの `pngs` を使い回す（再抽出しない）。
        let t = Instant::now();
        for png in &pngs {
            let _ = crate::egui_shell::png_to_color_image(png);
        }
        println!(
            "[hit path decode x{}] 直列合計={}us（load_texture 別途）",
            pngs.len(),
            t.elapsed().as_micros(),
        );
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

        let restored: IconCacheData = try_deserialize_with_header(&bytes, ICON_MAGIC, ICON_VERSION)
            .expect("deserialize into IndexMap-backed IconCacheData");
        assert_eq!(restored.png.len(), 2);
        assert_eq!(restored.png.get("c:/a.exe"), Some(&vec![1, 2, 3]));
        assert_eq!(restored.png.get("c:/b.exe"), Some(&vec![4, 5]));
    }
}
