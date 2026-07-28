//! フォント解決（config font_family → システムフォント検索）と `set_fonts` 登録。
//! 消費者は `view.rs` と `results_view.rs` の両方（main/results は別 Context を持つため
//! それぞれが自分の Context に対して呼ぶ）——ゆえにどちらにも属させず独立モジュールにする。
//! [`JP_FONT_BYTES`] は `OnceLock`（**set-once・never-clear**）を厳守する: [`jp_font_bytes`]
//! が返す参照は `transmute` で `'static` 化しており、その健全性はこの不変条件だけを根拠に
//! 成り立つ。再 set・クリアの経路を足してはならない。
//! フォント登録は **3 枝**（#532 SU4 の 2 枝に #689 が 1 枝を足した）: config font_family が
//! 解決し CJK 非被覆なら user_font 先頭 + jp_font fallback（WebView2 CSS スタック parity）、
//! 解決し**被覆するなら user_font 単一**（jp_font を積まない・#689）、解決失敗時は jp_font
//! 単一。いずれも index 0 へ `insert` する（#579 の元不変条件）。判定は `font_covers_cjk`。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use tauri::Manager;

static JP_FONT_BYTES: OnceLock<Box<[u8]>> = OnceLock::new();

/// user_font が CJK をカバーするかを判定するプローブ文字。**全点に glyph が無ければカバーしているとみなさない**。
///
/// かな + JIS 第1水準に加え、**第2水準を混ぜているのが要点**——かなと常用漢字だけ持つ
/// 中途半端な和文フォントを弾くため。判定が緩いと jp_font を落としたあとで第2水準が
/// 豆腐（□）になり、クラッシュしない字単位の静かな欠落として残る。
/// 判定は「厳しすぎて jp_font を残す」方向へ倒す（安全側）。
const CJK_PROBE: &[char] = &[
    // かな（必須）
    'あ', 'ん', 'ア', 'ヴ', 'ー',
    // JIS 第1水準（常用寄り）
    '日', '本', '語', '漢', '字', '検', '索',
    // JIS 第2水準・互換漢字
    '彁', '﨑', '槇', '遙', '瑤', '兪',
];

/// `bytes` の `face_index` 面が [`CJK_PROBE`] を全点持つかを cmap 実測で判定する。
///
/// 真なら jp_font fallback は不要で、`YuGothM.ttc`（13.26 MiB）+ egui のグリフ機構を
/// 丸ごと積まずに済む（#687 の実測: user_font 分だけでアイドル 20.6 MiB）。
/// パース不能なバイト列は `false`（= jp_font を積む安全側）へ倒す。
fn font_covers_cjk(bytes: &[u8], face_index: u32) -> bool {
    let Ok(face) = ttf_parser::Face::parse(bytes, face_index) else {
        return false;
    };
    CJK_PROBE.iter().all(|&ch| face.glyph_index(ch).is_some())
}

/// `jp_bytes = None` は「user_font が CJK をカバーするので jp_font を積まない」を表す。
/// user・jp とも None なら egui 既定フォントのままの `FontDefinitions` を返す
/// （呼び出し側が `set_fonts` 自体を避ける）。
///
/// **両フォントとも `&'static [u8]` で受け取り [`egui::FontData::from_static`] で積む。**
/// epaint の `blob_from_font_data` は `data.clone().font` を match の**前**に評価するため、
/// `Cow::Owned`（= `from_owned`）ではフォント全体が深くコピーされ、`FontDefinitions` 側と
/// Blob 側の 2 本が常駐する。`Cow::Borrowed` なら複製されるのは参照だけである。
/// 実測がこれを二重に裏づける: user_font は `from_owned` で 20.6 MiB ≒ 2 × ファイル 10.21 MiB、
/// jp_font は `from_static` で 12.9 MiB ≒ 1 × ファイル 13.26 MiB だった（#689）。
fn font_definitions(
    jp_bytes: Option<&'static [u8]>,
    user: Option<(&'static [u8], u32)>,
) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    let has_jp = jp_bytes.is_some();
    if let Some(jp_bytes) = jp_bytes {
        let mut jp = egui::FontData::from_static(jp_bytes);
        jp.tweak = egui::FontTweak {
            scale: 1.0,
            y_offset_factor: 0.3,
            y_offset: 0.0,
            ..Default::default()
        };
        fonts.font_data.insert("jp_font".to_owned(), jp.into());
    }
    match user {
        Some((bytes, face_index)) => {
            let mut uf = egui::FontData::from_static(bytes);
            uf.index = face_index; // TTC face 指定（settings font.rs:138 と同型）
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
                    fonts.families.entry(family).or_default().insert(0, "jp_font".to_owned());
                }
            }
        }
    }
    fonts
}

/// 解決済み user_font の (`'static` バイト列, face index)。[`USER_FONTS`] の値型。
type ResolvedFont = (&'static [u8], u32);

/// 解決済み user_font の family 名 → `'static` バイト列。**成功したものだけ**を保持する。
///
/// `Box::leak` を素で置くと 1 回の font_family 変更で 2 回漏れる——
/// `configure_japanese_font` は main と results の 2 Context から別々に呼ばれるため。
/// family 名をキーにして両者と再切替に 1 本の leak を共有させる（漏れはセッション中に
/// 使われた **distinct family 数**で頭打ち）。`JP_FONT_BYTES` と同じく解放されない。
///
/// ただし never-clear を要求する理由は `JP_FONT_BYTES` とは別である。あちらは
/// `transmute` による `'static` 化の**健全性**が OnceLock の不変性に依存する。こちらの
/// `Box::leak` はそれ自体が `'static` を生むため、never-clear は leak の重複を避ける
/// 一意性の要請であって memory safety の要請ではない。
///
/// **失敗（None）はキャッシュしない。** するとフォントを後から導入して同じ名前に
/// 戻したとき、解決できるのに拒み続ける。再解決のコストは `applied_font_family` ゲートに
/// より font_family 変更時の一度きりで、現行挙動と同じである。
static USER_FONTS: OnceLock<Mutex<HashMap<String, ResolvedFont>>> = OnceLock::new();

/// config font_family をシステムから解決して (`'static` バイト列, face index) を返す。
/// 見つからなければ None（呼び出し側が jp_font 単一へフォールバック）。Database は
/// 解決後に drop（非常駐・列挙コストはフォント設定時の一度きり）。
///
/// バイト列を `'static` へ leak するのは [`font_definitions`] が `from_static` で積むため。
/// `from_owned` だと epaint が Blob 生成時にフォント全体を深くコピーし、常駐が 2 倍になる。
fn resolve_font_family(name: &str) -> Option<(&'static [u8], u32)> {
    let cache = USER_FONTS.get_or_init(|| Mutex::new(HashMap::new()));
    // poison からは回復する（release は panic=abort ゆえ実際には起きないが、
    // ここで panic すると起動経路ごと落ちるため中身を取り出して続行する）。
    let mut map = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(hit) = map.get(name) {
        return Some(*hit);
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
    let resolved: (&'static [u8], u32) =
        db.with_face_data(id, |data, face_index| {
            (&*Box::leak(data.to_vec().into_boxed_slice()), face_index)
        })?;
    map.insert(name.to_owned(), resolved);
    Some(resolved)
}

/// jp_font（CJK fallback）のバイト列を遅延ロードして `'static` 借用を返す。
///
/// **`OnceLock` は set-once・never-clear を厳守する**——下の `transmute` による `'static`
/// 化はその不変条件だけを根拠に健全である。再 set・クリアの経路を足してはならない。
/// CJK をカバーするフォントを使う限りこの関数は呼ばれず、13.26 MiB は確保されない。
fn jp_font_bytes() -> Option<&'static [u8]> {
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
    // OnceLock の中身は以後不変ゆえ 'static として安全に借用できる。
    JP_FONT_BYTES
        .get()
        .map(|bytes| unsafe { std::mem::transmute::<&[u8], &'static [u8]>(&**bytes) })
}

pub(super) fn configure_japanese_font(context: &egui::Context, font_family: &str) {
    // **user を先に解決し、カバー判定が済むまで jp_font のファイルを読まない**（順序が要点）。
    // 旧実装は無条件に jp を読んでから user を解決していたため、CJK をカバーするフォントでも
    // 13.26 MiB が常駐した。ここを逆にすることが削減の実体である。
    let user = resolve_font_family(font_family);
    let need_jp = match &user {
        Some((bytes, face_index)) => !font_covers_cjk(bytes, *face_index),
        None => true,
    };
    let jp = if need_jp { jp_font_bytes() } else { None };
    if jp.is_none() && user.is_none() {
        // 積むフォントが 1 つも無い。egui 既定のままにする（旧実装と同じ挙動）。
        return;
    }
    context.set_fonts(font_definitions(jp, user));
}

/// `AppState` から現在の config font_family を読む。`AppState` が未 manage（起動極初期）
/// なら既定の font_family（正本は `visual::default_visual()`）へ落ちる。`view.rs` の `setup` と
/// `results_view.rs` の `setup` が同一の 4 行を持っていた重複をここへ寄せる
/// （#666 段 3 タスク 1・dry-check）。
///
/// **既定値のリテラルをここへ再手打ちしない**（#795）——`"Segoe UI"` を書くと `config.rs` の
/// `default_font_family()` と乖離しうる 2 つ目の表現になる。
pub(super) fn font_family_from_config(app: &tauri::AppHandle) -> String {
    app.try_state::<crate::AppState>()
        .map(|s| s.engine.lock().unwrap().config().visual.font_family.clone())
        .unwrap_or_else(|| super::visual::default_visual().font_family.clone())
}

#[cfg(test)]
mod tests {
    use super::font_definitions;

    #[test]
    fn font_definitions_fallback_is_jp_single_stack() {
        // user=None（font_family 解決失敗）: jp_font 単一・両ファミリ index 0（#579 の元不変条件）。
        let dummy: &'static [u8] = &[0u8; 4];
        let fonts = font_definitions(Some(dummy), None);
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            let list = fonts.families.get(&family).expect("family present");
            assert_eq!(list.first().map(String::as_str), Some("jp_font"),
                "解決失敗時は jp_font 単一・先頭（#579 再発防止）");
        }
    }

    #[test]
    fn font_definitions_honor_puts_user_first_jp_fallback() {
        // user=Some かつ **CJK をカバーしていない**（jp=Some）: user_font 先頭・jp_font は fallback（index 1）
        // ＝ WebView2 CSS スタック parity。カバー判定を入れても、この経路の不変条件は不変である。
        let dummy: &'static [u8] = &[0u8; 4];
        let user: &'static [u8] = &[0u8; 4];
        let fonts = font_definitions(Some(dummy), Some((user, 0)));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            let list = fonts.families.get(&family).expect("family present");
            assert_eq!(list.first().map(String::as_str), Some("user_font"),
                "honor 時は user_font 先頭（font_family 優先）");
            assert_eq!(list.get(1).map(String::as_str), Some("jp_font"),
                "honor 時も jp_font は fallback として残す（CJK をカバー）");
        }
    }

    #[test]
    fn font_definitions_covered_user_font_omits_jp_entirely() {
        // user が CJK をカバー（jp=None）: jp_font は**スタックにもデータにも現れない**。
        // font_data に残ると egui が eager parse してメモリを食うため、両方を検査する。
        let user: &'static [u8] = &[0u8; 4];
        let fonts = font_definitions(None, Some((user, 0)));
        assert!(!fonts.font_data.contains_key("jp_font"),
            "カバー済みなら jp_font のバイト列自体を積まない（削減の実体）");
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            let list = fonts.families.get(&family).expect("family present");
            assert_eq!(list.first().map(String::as_str), Some("user_font"),
                "カバー済みでも user_font 先頭（#579 のベースラインずれ防止）");
            assert!(!list.iter().any(|n| n == "jp_font"),
                "カバー済みなら jp_font をスタックに残さない");
        }
    }

    #[test]
    fn font_definitions_registers_both_fonts_as_borrowed() {
        // **常駐 2 倍化に対する退行テスト。** epaint の `blob_from_font_data` は
        // `data.clone().font` を match の前に評価するため、`Cow::Owned`（= `from_owned`）
        // だとフォント全体が深くコピーされ FontDefinitions 側と Blob 側の 2 本が残る。
        // `from_static` → `Cow::Borrowed` なら複製されるのは参照だけ。
        // ここが Owned に戻ると**数値でしか気づけない**（描画も型検査も通る）ので型で縛る。
        use std::borrow::Cow;
        let jp: &'static [u8] = &[0u8; 4];
        let user: &'static [u8] = &[0u8; 4];
        let fonts = font_definitions(Some(jp), Some((user, 0)));
        for key in ["jp_font", "user_font"] {
            let data = fonts.font_data.get(key).expect("font registered");
            assert!(matches!(data.font, Cow::Borrowed(_)),
                "{key} が Cow::Owned で積まれている（from_owned だと epaint が全体を複製し常駐が 2 倍になる）");
        }
    }

    #[test]
    fn font_covers_cjk_rejects_unparsable_bytes() {
        // パース不能は「カバーしていない」＝ jp_font を積む安全側へ倒す。
        assert!(!super::font_covers_cjk(&[], 0));
        assert!(!super::font_covers_cjk(&[0u8; 64], 0));
    }

    #[test]
    fn font_covers_cjk_rejects_latin_only_font() {
        // egui 同梱の既定フォントは Latin/emoji のみ。システムに依存しない決定論的な negative。
        let defaults = egui::FontDefinitions::default();
        let mut checked = 0usize;
        for (name, data) in &defaults.font_data {
            assert!(!super::font_covers_cjk(&data.font, data.index),
                "egui 同梱フォント {name} が CJK をカバーすると誤判定された（判定が緩すぎる）");
            checked += 1;
        }
        assert!(checked > 0, "egui 既定フォントが 0 件では negative を検査できていない");
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
        assert!(super::font_covers_cjk(&bytes, 0),
            "和文フォント YuGothM が CJK をカバーしないと判定された（判定が厳しすぎる）");
    }
}
