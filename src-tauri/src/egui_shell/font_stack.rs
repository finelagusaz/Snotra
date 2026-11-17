//! フォント解決（config font_family → システムフォント検索）と `set_fonts` 登録。
//! 消費者は `view.rs` と `results_view.rs` の両方（main/results は別 Context を持つため
//! それぞれが自分の Context に対して呼ぶ）——ゆえにどちらにも属させず独立モジュールにする。
//! フォントのバイト列はファイルをメモリマップして持ち（メモリマップできなければ読んで写す・[`FontFiles`]）、
//! ファイルのパスごとに 1 回だけ開く（**never-clear**。消すと、次の解決が同じファイルを
//! もう一度開く）。メモリマップしたページはヒープに無いので、private commit に載らない（#1449）。
//! フォント登録の分岐は次のとおり（#532・#689）: config font_family が解決し CJK を**カバーしない**なら user_font 先頭 + jp_font fallback（WebView2 CSS スタック parity）、解決し**カバーするなら user_font 単一**（jp_font を積まない・#689）、解決失敗時は jp_font 単一。いずれも index 0 へ `insert` する（#579 の元不変条件）。判定は `font_covers_cjk`。**分岐を列挙するのは、この index 0 の不変条件が分岐ごとに同じであることを一目で見せるためである**（正本の代わりの写しではない）。
//!
//! **index 0 を破ると Latin と CJK が混在する行のベースラインがずれる**（2 フォントで積むとずれること自体は
//! `snotra-settings/CLAUDE.md` の #399）。softbuffer の `fill_mesh` はカバレッジ AA を持たず、2 フォント間の
//! 分数 px の差を整数 px へ丸めて目に見える段差にする——glow / wgpu 期は sub-pixel AA が同じ差を隠していた。
//! 規則は `font_definitions_*` テストが固定するが、見え方を測るのは目視だけである。**フォント登録に触る
//! 変更では `docs/build-commands.md` カテゴリ D の目視スモークを省かない。**

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use egui::epaint::text::Blob;

/// メモリマップしたフォントファイル。開いたハンドルを、メモリマップと同じ値の中に持つ。
///
/// フィールドは `map` を先に置く（drop は宣言順なので、メモリマップを解いてからハンドルを閉じる）。
struct MappedFont {
    map: memmap2::Mmap,
    /// 書き込みを拒む共有モードで開いたハンドル。メモリマップの中身を変えさせない根拠（[`map_font_file`]）。
    _file: File,
}

impl AsRef<[u8]> for MappedFont {
    fn as_ref(&self) -> &[u8] {
        &self.map
    }
}

/// [`FontFiles::load`] がバイト列をどう持ったか。メモリマップの経路を通ったことをテストが確かめるための値。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Loaded {
    /// ファイルをメモリマップした（[`MappedFont`]）。
    Mapped,
    /// メモリマップできず、読んでヒープへ写した。
    Copied,
}

/// フォントファイルのキャッシュ。キーは正規化したパスで、**同じファイルは 1 回だけ開く**——
/// 別名の family が同じファイルへ解決したときも、user_font と jp_font が同じファイルのときも。
/// 製品は [`font_files`] の 1 つを never-clear で使い、テストは自分で作る。
/// **読めなかったパスはキャッシュしない**（次の呼び出しで開き直す）。
#[derive(Default)]
struct FontFiles {
    files: Mutex<HashMap<PathBuf, (Blob, Loaded)>>,
}

impl FontFiles {
    /// `path` のフォントファイルを、メモリマップして（できなければ読んで写して）返す。読めなければ `None`。
    ///
    /// 錠は開く・メモリマップする間も持つ——起動時の先読み（[`prefetch`]）と view の `setup` が
    /// 同時に引いても、1 回だけ開く（#1452）。
    fn load(&self, path: &Path) -> Option<(Blob, Loaded)> {
        // 綴りの違い（fontdb の `C:\WINDOWS\Fonts\…` と `C:/Windows/Fonts/…`）を 1 つのキーに寄せる。
        // 正規化に失敗しても、開けるなら開く（キーが綴りどおりになるだけである）。
        let key = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        // poison からは回復する（理由は `resolve_font_family` と同じ）。
        let mut files = self.files.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(hit) = files.get(&key) {
            return Some(hit.clone());
        }
        let loaded = match map_font_file(&key) {
            Some(mapped) => (Arc::new(mapped) as Blob, Loaded::Mapped),
            None => (Arc::new(std::fs::read(&key).ok()?) as Blob, Loaded::Copied),
        };
        files.insert(key, loaded.clone());
        Some(loaded)
    }
}

/// 製品が使う [`FontFiles`]。
fn font_files() -> &'static FontFiles {
    static FONT_FILES: OnceLock<FontFiles> = OnceLock::new();
    FONT_FILES.get_or_init(FontFiles::default)
}

/// `path` を、書き込みを拒む共有モードで開いてメモリマップする。メモリマップしないときは `None`
/// を返し、呼び出し側が読んで写す。
fn map_font_file(path: &Path) -> Option<MappedFont> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use windows::Win32::Storage::FileSystem::{FILE_SHARE_DELETE, FILE_SHARE_READ};

    let file = std::fs::OpenOptions::new()
        .read(true)
        .share_mode((FILE_SHARE_READ | FILE_SHARE_DELETE).0)
        .open(path)
        .ok()?;
    let meta = file.metadata().ok()?;
    if meta.len() == 0 || !maps_on(drive_type_of(&file)?, meta.file_attributes()) {
        return None;
    }
    // SAFETY: メモリマップの背後のファイルは、`FILE_SHARE_WRITE` を含まない共有モードで開いた `file` が
    // 生きている間、他のハンドルから書き込みのために開けない（OS が拒む）。書き込みのために開いている
    // 他のハンドルが既に在れば、`file` の open 自体が共有違反で失敗する。削除と名前の変更は許す。
    // どちらもディレクトリの項目を外すだけで、メモリマップが指す中身は閉じるまで残る（#1449 で実測）。
    // `file` はメモリマップと同じ値（`MappedFont`）に入り、メモリマップより先に閉じない。ページの読み込みに失敗しうる
    // 場所（ネットワーク・クラウドのプレースホルダ）は `maps_on` が除く。
    let map = unsafe { memmap2::Mmap::map(&file) }.ok()?;
    Some(MappedFont { map, _file: file })
}

/// 開いたハンドルが指すボリュームの種類（`GetDriveTypeW` の値）。パスの文字列ではなく
/// ハンドルから引く——ドライブ文字の付け替え・ジャンクション・UNC を綴りでは見分けられないため。
fn drive_type_of(file: &File) -> Option<u32> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        GetDriveTypeW, GetFinalPathNameByHandleW, VOLUME_NAME_GUID,
    };

    let handle = HANDLE(file.as_raw_handle());
    let mut buf = vec![0u16; 512];
    loop {
        // SAFETY: `handle` は `file` が生きている間有効で、`buf` は呼び出しの間だけ貸す。
        let n = unsafe { GetFinalPathNameByHandleW(handle, &mut buf, VOLUME_NAME_GUID) } as usize;
        if n == 0 {
            return None;
        }
        if n < buf.len() {
            buf.truncate(n);
            break;
        }
        // 足りなければ、戻り値が終端を含む必要な長さを示す。
        buf.resize(n, 0);
    }
    // `\\?\Volume{GUID}\…` の、ボリュームのルート（`}\` まで）だけを渡す。
    let path = String::from_utf16_lossy(&buf);
    let root_end = path.find("}\\")? + 2;
    let root: Vec<u16> = path[..root_end].encode_utf16().chain([0]).collect();
    // SAFETY: `root` は NUL で終わり、呼び出しの間生きている。
    Some(unsafe { GetDriveTypeW(windows::core::PCWSTR(root.as_ptr())) })
}

/// メモリマップしてよい場所か。ローカルの固定ドライブで、クラウドのプレースホルダ・オフラインでないこと。
///
/// メモリマップのページを読み込めないとプロセスが落ちる（`EXCEPTION_IN_PAGE_ERROR`。release は
/// panic=abort で捕まえられない）。読み込みが失敗しうる場所を除き、判定できないときもメモリマップしない。
/// USB で繋いだ外付けのディスクも固定ドライブと報告されることがあり、起動中に抜くと落ちうる（受容する残余）。
fn maps_on(drive_type: u32, attributes: u32) -> bool {
    use windows::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_OFFLINE, FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS, FILE_ATTRIBUTE_RECALL_ON_OPEN,
    };
    // `DRIVE_FIXED`（`GetDriveTypeW` の値）。定数を持つ feature を有効にしていないので値で書く。
    const DRIVE_FIXED: u32 = 3;
    let remote = (FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS
        | FILE_ATTRIBUTE_RECALL_ON_OPEN
        | FILE_ATTRIBUTE_OFFLINE)
        .0;
    drive_type == DRIVE_FIXED && attributes & remote == 0
}

/// user_font が CJK をカバーするかを判定するプローブ文字。**全点に glyph が無ければカバーしているとみなさない**。
///
/// かな + JIS 第1水準に加え、**第2水準を混ぜているのが要点**——かなと常用漢字だけ持つ
/// 中途半端な和文フォントを弾くため。判定が緩いと jp_font を落としたあとで第2水準が
/// 豆腐（□）になり、クラッシュしない字単位の静かな欠落として残る。
/// 判定は「厳しすぎて jp_font を残す」方向にする（安全側）。
const CJK_PROBE: &[char] = &[
    // かな（必須）
    'あ', 'ん', 'ア', 'ヴ', 'ー', // JIS 第1水準（常用寄り）
    '日', '本', '語', '漢', '字', '検', '索', // JIS 第2水準・互換漢字
    '彁', '﨑', '槇', '遙', '瑤', '兪',
];

/// `bytes` の `face_index` 面が [`CJK_PROBE`] を全点持つかを cmap 実測で判定する。
///
/// 真なら jp_font fallback は不要で、`YuGothM.ttc` のメモリマップと egui のグリフ機構を
/// 丸ごと積まずに済む（#687 の実測: user_font 分だけでアイドル 20.6 MiB）。
/// パース不能なバイト列は `false`（= jp_font を積む安全側）にする。
fn font_covers_cjk(bytes: &[u8], face_index: u32) -> bool {
    let Ok(face) = ttf_parser::Face::parse(bytes, face_index) else {
        return false;
    };
    CJK_PROBE.iter().all(|&ch| face.glyph_index(ch).is_some())
}

/// `jp = None` は「user_font が CJK をカバーするので jp_font を積まない」を表す。
/// user・jp とも None なら egui 既定フォントのままの `FontDefinitions` を返す
/// （呼び出し側が `set_fonts` 自体を避ける）。
///
/// **両フォントとも [`Blob`] で受け取り [`egui::FontData::from_blob`] で積む。**
/// 守っているのは「フォントのバイト列が 1 本しか常駐しない」ことである。egui 0.36 までは
/// `FontData.font` が `Cow` で、`from_owned`（`Cow::Owned`）だと epaint が Blob を作るときに
/// フォント全体を深くコピーし、2 本が常駐した（実測: user_font は `from_owned` で 20.6 MiB ≒
/// 2 × ファイル 10.21 MiB、jp_font は `from_static` で 12.9 MiB ≒ 1 × 13.26 MiB・#689）。
/// 0.37 は `FontData.font` を `Arc` にし、epaint は内部でもその `Arc` を共有する
/// （`face_store.rs` / `font_face.rs` が `Arc::clone`）ので、渡したメモリマップをそのまま使う。
fn font_definitions(jp: Option<Blob>, user: Option<(Blob, u32)>) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    let has_jp = jp.is_some();
    if let Some(jp) = jp {
        let mut jp = egui::FontData::from_blob(jp, 0);
        jp.tweak = egui::FontTweak {
            scale: 1.0,
            y_offset_factor: 0.3,
            y_offset: 0.0,
            ..Default::default()
        };
        fonts.font_data.insert("jp_font".to_owned(), jp.into());
    }
    match user {
        Some((blob, face_index)) => {
            // face_index は TTC の face 指定（settings font.rs:138 と同型）。
            let uf = egui::FontData::from_blob(blob, face_index);
            fonts.font_data.insert("user_font".to_owned(), uf.into());
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                // user_font 先頭（font_family 優先）+ jp_font fallback（CJK をカバー）= CSS スタック parity。
                // カバー済みなら jp_font は積まれず user_font 単一・先頭になる。
                let list = fonts.families.entry(family).or_default();
                if has_jp {
                    list.insert(0, "jp_font".to_owned());
                }
                list.insert(0, "user_font".to_owned());
            }
        }
        None => {
            if has_jp {
                for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                    // 解決失敗時は jp_font 単一・先頭（#579: push=末尾だとベースラインずれ再発）。
                    fonts
                        .families
                        .entry(family)
                        .or_default()
                        .insert(0, "jp_font".to_owned());
                }
            }
        }
    }
    fonts
}

/// 解決済み user_font の (バイト列, face index)。[`USER_FONTS`] の値型。
type ResolvedFont = (Blob, u32);

/// 解決済み user_font の family 名 → バイト列。**成功したものだけ**を保持する。
///
/// `configure_japanese_font` は main と results の 2 Context から別々に呼ばれ、font_family を
/// 切り替えて戻すこともある。family 名をキーにして、fontdb の照会（システムフォントの全走査）を
/// family ごとに 1 回にする。バイト列は [`FontFiles`] の `Arc` を共有するので、別名の family が
/// 同じファイルへ解決してもファイルは 1 回だけ開く。[`FontFiles`] と同じく解放されない。
///
/// **失敗（None）はキャッシュしない。** するとフォントを後から導入して同じ名前に
/// 戻したとき、解決できるのに拒み続ける。再解決のコストは `applied_font_family` ゲートに
/// より font_family 変更時の一度きりで、現行挙動と同じである。
static USER_FONTS: OnceLock<Mutex<HashMap<String, ResolvedFont>>> = OnceLock::new();

/// config font_family をシステムから解決して (バイト列, face index) を返す。
/// 見つからなければ None（呼び出し側が jp_font 単一へフォールバック）。Database は
/// 解決後に drop（非常駐・列挙コストはフォント設定時の一度きり）。
///
/// fontdb はファイルのパスを選ぶところまで使い、開くのは [`FontFiles`] が行う——fontdb の
/// メモリマップ（`make_shared_face_data`）は共有モードを絞らずに開くため使わない。
fn resolve_font_family(name: &str) -> Option<ResolvedFont> {
    let cache = USER_FONTS.get_or_init(|| Mutex::new(HashMap::new()));
    // poison からは回復する（release は panic=abort ゆえ実際には起きないが、
    // ここで panic すると起動経路ごと落ちるため中身を取り出して続行する）。
    let mut map = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(hit) = map.get(name) {
        return Some(hit.clone());
    }

    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    let query = fontdb::Query {
        families: &[fontdb::Family::Name(name)],
        weight: fontdb::Weight::NORMAL,
        stretch: fontdb::Stretch::Normal,
        style: fontdb::Style::Normal,
    };
    let id = db.query(&query)?;
    let (source, face_index) = db.face_source(id)?;
    let blob = match source {
        fontdb::Source::File(path) => font_files().load(&path)?.0,
        // 既にメモリにあるデータは、その `Arc` をそのまま積む（写さない）。
        fontdb::Source::Binary(data) | fontdb::Source::SharedFile(_, data) => data,
    };
    let resolved = (blob, face_index);
    map.insert(name.to_owned(), resolved.clone());
    Some(resolved)
}

/// jp_font（CJK fallback）のバイト列を遅延ロードして返す。
///
/// CJK をカバーするフォントを使う限りこの関数は呼ばれず、jp_font のファイルは開かれない。
fn jp_font_bytes() -> Option<Blob> {
    const CANDIDATES: [&str; 4] = [
        "C:/Windows/Fonts/YuGothM.ttc",
        "C:/Windows/Fonts/yugothic.ttf",
        "C:/Windows/Fonts/msgothic.ttc",
        "C:/Windows/Fonts/meiryo.ttc",
    ];
    // 見つけたファイルは [`FontFiles`] がキャッシュするので、2 回目以降は開き直さない。
    CANDIDATES
        .iter()
        .find_map(|path| font_files().load(Path::new(path)))
        .map(|(blob, _)| blob)
}

/// `font_family` から、積むフォントの組（jp_font, user_font）を解決する。結果は
/// [`USER_FONTS`] と [`FontFiles`] にキャッシュされる。
fn resolve_stack(font_family: &str) -> (Option<Blob>, Option<ResolvedFont>) {
    // **user を先に解決し、カバー判定が済むまで jp_font のファイルを開かない**（順序が要点）。
    // jp を先に開くと、CJK をカバーするフォントでも jp_font を開き、egui がそれを解析する
    // （メモリマップできない場所では写しも持つ）。
    let user = resolve_font_family(font_family);
    let need_jp = match &user {
        Some((blob, face_index)) => !font_covers_cjk((**blob).as_ref(), *face_index),
        None => true,
    };
    let jp = if need_jp { jp_font_bytes() } else { None };
    (jp, user)
}

/// フォントの解決を別スレッドで先に始める（#1452）。`main()` が config を読んだ直後に呼び、
/// 索引のロードなど main スレッドの後続と重ねる。後で view の `setup` が
/// [`configure_japanese_font`] を呼ぶと、キャッシュに当たるか、解決が終わるのを錠で待つ。
///
/// **解決の結果も、ファイルを開く回数も変わらない**——どちらの経路も [`resolve_stack`] を通り、
/// キャッシュは family 名・ファイルのパスと `OnceLock` で 1 つに決まる。スレッドを起こせなければ
/// 何もしない（`setup` がその場で解決する）。
pub(crate) fn prefetch(font_family: String) {
    let _ = std::thread::Builder::new()
        .name("snotra-font-prefetch".to_string())
        .spawn(move || {
            let _ = resolve_stack(&font_family);
        });
}

pub(super) fn configure_japanese_font(context: &egui::Context, font_family: &str) {
    let (jp, user) = resolve_stack(font_family);
    if jp.is_none() && user.is_none() {
        // 積むフォントが 1 つも無い。egui 既定のままにする。
        return;
    }
    context.set_fonts(font_definitions(jp, user));
}

/// `AppState` から現在の config font_family を読む。`AppState` が未 manage（起動極初期）
/// なら既定の font_family（正本は `visual::default_visual()`）へ落ちる。`view.rs` の `setup` と
/// `results_view.rs` の `setup` が共有する読みであり、それぞれへ手書きしない（#666）。
///
/// **既定値のリテラルをここへ再手打ちしない**（#795）——`"Segoe UI"` を書くと `config.rs` の
/// `default_font_family()` と乖離しうる 2 つ目の表現になる。
pub(super) fn font_family_from_config(app: &tauri::AppHandle) -> String {
    super::read_config(
        app,
        |c| c.visual.font_family.clone(),
        || super::visual::default_visual().font_family.clone(),
    )
}

#[cfg(test)]
mod tests {
    use super::{Blob, FontFiles, Loaded, font_definitions, maps_on};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    fn blob(bytes: &[u8]) -> Blob {
        Arc::new(bytes.to_vec())
    }

    /// テストごとの一時ディレクトリ（`%TEMP%` の下。実 `%APPDATA%\Snotra` には触れない）。
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("snotra-font-stack-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("一時ディレクトリを作れない");
            Self(dir)
        }

        fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.0.join(name);
            std::fs::write(&path, bytes).expect("一時ファイルを書けない");
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn sample_bytes() -> Vec<u8> {
        (0..64 * 1024u32).map(|i| (i * 7 + 3) as u8).collect()
    }

    #[test]
    fn font_definitions_fallback_is_jp_single_stack() {
        // user=None（font_family 解決失敗）: jp_font 単一・両ファミリ index 0（#579 の元不変条件）。
        let fonts = font_definitions(Some(blob(&[0u8; 4])), None);
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            let list = fonts.families.get(&family).expect("family present");
            assert_eq!(
                list.first().map(String::as_str),
                Some("jp_font"),
                "解決失敗時は jp_font 単一・先頭（#579 再発防止）"
            );
        }
    }

    #[test]
    fn font_definitions_honor_puts_user_first_jp_fallback() {
        // user=Some かつ **CJK をカバーしていない**（jp=Some）: user_font 先頭・jp_font は fallback（index 1）
        // ＝ WebView2 CSS スタック parity。カバー判定を入れても、この経路の不変条件は不変である。
        let fonts = font_definitions(Some(blob(&[0u8; 4])), Some((blob(&[0u8; 4]), 0)));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            let list = fonts.families.get(&family).expect("family present");
            assert_eq!(
                list.first().map(String::as_str),
                Some("user_font"),
                "honor 時は user_font 先頭（font_family 優先）"
            );
            assert_eq!(
                list.get(1).map(String::as_str),
                Some("jp_font"),
                "honor 時も jp_font は fallback として残す（CJK をカバー）"
            );
        }
    }

    #[test]
    fn font_definitions_covered_user_font_omits_jp_entirely() {
        // user が CJK をカバー（jp=None）: jp_font は**スタックにもデータにも現れない**。
        // font_data に残ると egui が eager parse してメモリを食うため、両方を検査する。
        let fonts = font_definitions(None, Some((blob(&[0u8; 4]), 0)));
        assert!(
            !fonts.font_data.contains_key("jp_font"),
            "カバー済みなら jp_font のバイト列自体を積まない（削減の実体）"
        );
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            let list = fonts.families.get(&family).expect("family present");
            assert_eq!(
                list.first().map(String::as_str),
                Some("user_font"),
                "カバー済みでも user_font 先頭（#579 のベースラインずれ防止）"
            );
            assert!(
                !list.iter().any(|n| n == "jp_font"),
                "カバー済みなら jp_font をスタックに残さない"
            );
        }
    }

    #[test]
    fn font_definitions_registers_both_fonts_without_copy() {
        // **常駐 2 倍化に対する退行テスト。** 積んだバイト列が、渡した Blob そのものを
        // 指すことを見る（どこかで複製されれば別の番地になる）。複製は**数値でしか気づけない**
        // （描画も型検査も通る）ので番地で縛る。経緯は `font_definitions` の doc。
        let jp = blob(&[0u8; 4]);
        let user = blob(&[1u8; 4]);
        let fonts = font_definitions(Some(jp.clone()), Some((user.clone(), 0)));
        for (key, src) in [("jp_font", &jp), ("user_font", &user)] {
            let data = fonts.font_data.get(key).expect("font registered");
            assert!(
                std::ptr::eq(data.bytes().as_ptr(), (**src).as_ref().as_ptr()),
                "{key} のバイト列が複製されて積まれている（常駐が 2 倍になる）"
            );
        }
    }

    #[test]
    fn mapped_file_refuses_writes_but_survives_delete_and_replace() {
        // 書き込みを拒む共有モードの帰結（`map_font_file` の SAFETY）。書き込みでは開けず、
        // 退避・同じ名前での置き換え・削除は通り、そのあともメモリマップの中身は元のままである。
        let dir = TempDir::new("share");
        let bytes = sample_bytes();
        let path = dir.write("font.bin", &bytes);
        let (loaded, how) = FontFiles::default().load(&path).expect("読める");
        // **経路を assert する。** 判定が常に「メモリマップしない」を返しても、以下は写しで全部通る。
        assert_eq!(
            how,
            Loaded::Mapped,
            "ローカルの固定ドライブのファイルはメモリマップする"
        );

        assert!(
            std::fs::OpenOptions::new().write(true).open(&path).is_err(),
            "メモリマップ中のファイルを書き込みのために開けてしまう（中身を書き換えられる）"
        );
        let moved = dir.0.join("moved.bin");
        std::fs::rename(&path, &moved).expect("メモリマップ中でも名前を変えられる");
        std::fs::write(&path, b"replaced").expect("同じ名前へ新しい中身を書ける");
        std::fs::remove_file(&moved).expect("メモリマップ中でも削除できる");
        assert_eq!(
            (*loaded).as_ref(),
            &bytes[..],
            "削除・置き換えの後にメモリマップの中身が変わった"
        );
    }

    #[test]
    fn load_copies_empty_file_and_rejects_missing_path() {
        let dir = TempDir::new("edge");
        let path = dir.write("empty.bin", &[]);
        let (loaded, how) = FontFiles::default().load(&path).expect("長さ 0 も読める");
        assert_eq!(how, Loaded::Copied, "長さ 0 はメモリマップせず読んで写す");
        assert!((*loaded).as_ref().is_empty());
        assert!(
            FontFiles::default()
                .load(&dir.0.join("missing.bin"))
                .is_none()
        );
    }

    #[test]
    fn load_opens_the_same_file_once_across_spellings() {
        // fontdb は `C:\WINDOWS\Fonts\…`、jp_font の候補は `C:/Windows/Fonts/…` と綴る。同じファイルなら
        // 同じ Arc を返す。**大文字小文字も変える**——Windows の `PathBuf` は区切りの `\` と `/` を同じに
        // 比べるので、区切りだけを変えても、正規化を外した実装との差が出ない（変異注入で実測）。
        let dir = TempDir::new("spell");
        let path = dir.write("font.bin", &sample_bytes());
        let files = FontFiles::default();
        let (a, _) = files.load(&path).expect("読める");
        let respelled = PathBuf::from(path.to_string_lossy().replace('\\', "/").to_uppercase());
        assert_ne!(respelled, path, "綴りを変えられていない");
        let (b, _) = files.load(&respelled).expect("読める");
        assert!(Arc::ptr_eq(&a, &b), "同じファイルを 2 回開いた");
    }

    #[test]
    fn maps_on_accepts_only_local_fixed_and_present_files() {
        use windows::Win32::Storage::FileSystem::{
            FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_OFFLINE, FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS,
            FILE_ATTRIBUTE_RECALL_ON_OPEN,
        };
        // `GetDriveTypeW` の値: 0 不明・1 ルートなし・2 リムーバブル・3 固定・4 リモート・5 CD-ROM・6 RAM ディスク。
        assert!(maps_on(3, 0));
        assert!(maps_on(3, FILE_ATTRIBUTE_NORMAL.0));
        for drive in [0, 1, 2, 4, 5, 6] {
            assert!(
                !maps_on(drive, 0),
                "drive type {drive} をメモリマップしてしまう"
            );
        }
        for attr in [
            FILE_ATTRIBUTE_OFFLINE,
            FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS,
            FILE_ATTRIBUTE_RECALL_ON_OPEN,
        ] {
            assert!(
                !maps_on(3, attr.0),
                "属性 {:#x} のファイルをメモリマップしてしまう",
                attr.0
            );
        }
    }

    #[test]
    fn load_maps_japanese_system_font() {
        // 実フォントでもメモリマップの経路を通ることを見る。不在なら skip する（理由は下の positive 検査と同じ）。
        let path = Path::new("C:/Windows/Fonts/YuGothM.ttc");
        let Ok(bytes) = std::fs::read(path) else {
            eprintln!("skip: YuGothM.ttc が無いためメモリマップの検査を実施していない");
            return;
        };
        let (loaded, how) = FontFiles::default().load(path).expect("読める");
        assert_eq!(
            how,
            Loaded::Mapped,
            "システムフォントをメモリマップできず写した"
        );
        assert_eq!((*loaded).as_ref(), &bytes[..]);
    }

    #[test]
    fn font_covers_cjk_rejects_unparsable_bytes() {
        // パース不能は「カバーしていない」＝ jp_font を積む安全側にする。
        assert!(!super::font_covers_cjk(&[], 0));
        assert!(!super::font_covers_cjk(&[0u8; 64], 0));
    }

    #[test]
    fn font_covers_cjk_rejects_latin_only_font() {
        // egui 同梱の既定フォントは Latin/emoji のみ。システムに依存しない決定論的な negative。
        let defaults = egui::FontDefinitions::default();
        let mut checked = 0usize;
        for (name, data) in &defaults.font_data {
            assert!(
                !super::font_covers_cjk(data.bytes(), data.index),
                "egui 同梱フォント {name} が CJK をカバーすると誤判定された（判定が緩すぎる）"
            );
            checked += 1;
        }
        assert!(
            checked > 0,
            "egui 既定フォントが 0 件では negative を検査できていない"
        );
    }

    #[test]
    fn font_covers_cjk_accepts_japanese_system_font() {
        // positive はシステムフォント依存ゆえ、不在なら skip する（CI の runner には
        // 和文フォントが無いことがある）。**沈黙させず理由を出す**——「検査しなかった」と
        // 「合格した」を出力から区別できるようにするため。
        let Ok(bytes) = std::fs::read("C:/Windows/Fonts/YuGothM.ttc") else {
            eprintln!("skip: YuGothM.ttc が無いため positive 検査を実施していない");
            return;
        };
        assert!(
            super::font_covers_cjk(&bytes, 0),
            "和文フォント YuGothM が CJK をカバーしないと判定された（判定が厳しすぎる）"
        );
    }
}
