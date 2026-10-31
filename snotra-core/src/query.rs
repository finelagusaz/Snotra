//! クエリと履歴キーの正規化、および文字存在ビットマスク計算。
//!
//! 照合用の正規化の唯一の入口 `normalize_for_match`（クエリとエントリ名の両側が通る・
//! SPEC §4.2。変える文字が無ければ `Cow` で借用）、履歴クエリキー正規化
//! `normalize_history_query_key`、検索/インデクサ双方が使う `char_bitmask`
//! （bits 0-25 = a-z, 26-35 = 0-9）を一元定義する。正規化が冪等であることは全コードポイントの
//! 検知器で固定してある。

use std::borrow::Cow;

use nucleo_matcher::chars::normalize as nucleo_normalize;

/// ASCII ローマ字をひらがなに変換する（カタカナもひらがなに正規化）。
/// 非 ASCII 文字（漢字など）はそのまま通過する。
pub fn to_kana(s: &str) -> String {
    use wana_kana::ConvertJapanese;
    s.to_hiragana()
}

/// [`normalize_for_match`] + パス区切り文字（`/` `¥`）を `\` に統一する。
/// 履歴キーの正規化はこの関数に一元化する（DRY）。
/// `record_launch`・`query_count`・`migrate_normalize_keys`・`search.rs` の
/// `path_history_key` が全て同じ正規化を使うことを保証する。
pub fn normalize_history_query_key(query: &str) -> Cow<'_, str> {
    let nq = normalize_for_match(query);
    if nq.contains('/') || nq.contains('\u{00a5}') {
        Cow::Owned(nq.replace(['/', '\u{00a5}'], "\\"))
    } else {
        nq
    }
}

/// パス区切り文字（`\` `/` `¥`）を 1 つでも含むか。
///
/// **クエリ側とエントリ側が同じ述語を通ることが、#1057 の最適化の根拠である。** パスクエリで
/// name の Fuzzy スコアリングを飛ばしてよいのは「区切りを含む needle は、区切りを含まない
/// 名前に部分列として存在しえない」からで、**両辺を別々の判定で見た瞬間にその論証が切れる**。
/// ゆえに片方だけを書き換えられない形（この 1 関数）に閉じる。
///
/// `¥`(U+00A5) は日本語 Windows でバックスラッシュとして使われるため含める。
/// **`contains(['\\', '/', '¥'])` と 1 つに束ねないこと。** 複数 `char` のパターンは 1 文字ずつ
/// UTF-8 を復号して 3 通り比べる。単一 `char` の [`str::contains`] は memchr へ落ちるので、
/// **3 パスに分けるほうが速い**——束ねたくなる形が遅いほうである。額の正本は
/// `PERFORMANCE.md`「採用: パスクエリで name の Fuzzy スコアリングを行わない」の「対価」節
/// （**ここへ写さない**——写した数字が正本と食い違う事故を実際に踏んだ）。
#[inline]
pub fn contains_path_sep(s: &str) -> bool {
    s.contains('\\') || s.contains('/') || s.contains('\u{00a5}')
}

/// Compute a character-presence bitmask for a lowercase ASCII string.
/// Bits 0-25 = 'a'-'z', bits 26-35 = '0'-'9'. All other chars are ignored.
/// Used by both `SearchEngine` (query-time pre-filter) and `IndexCache` (build-time persistence).
pub fn char_bitmask(lower: &str) -> u64 {
    let mut mask: u64 = 0;
    for b in lower.bytes() {
        match b {
            b'a'..=b'z' => mask |= 1u64 << (b - b'a'),
            b'0'..=b'9' => mask |= 1u64 << (26 + (b - b'0')),
            _ => {}
        }
    }
    mask
}

/// Character-presence bitmask for a single lowercase entry name (or file name).
/// ASCII strings get the bit-exact mask from [`char_bitmask`]; non-ASCII strings
/// (typically CJK, Arabic, etc. — `normalize_for_match` already folds Latin accents to
/// ASCII) get `u64::MAX` so `(query_mask & u64::MAX) == query_mask` always holds and
/// the entry is never excluded by the bitmask pre-filter.
///
/// Single definition shared by `search.rs` (Wave 2) and `indexer.rs` (cache build /
/// incremental extend) — previously duplicated in 3 call sites (issue #437).
#[inline]
pub fn name_char_mask(lower: &str) -> u64 {
    if lower.is_ascii() {
        char_bitmask(lower)
    } else {
        u64::MAX
    }
}

/// Character-presence bitmask for an optional lowercase file name. `None` (entry has no
/// file_name component) yields `0` — such entries cannot match via the file_name path,
/// so failing the bitmask pre-filter is correct. `Some` defers to [`name_char_mask`].
#[inline]
pub fn file_char_mask(lower_file_name: Option<&str>) -> u64 {
    lower_file_name.map_or(0, name_char_mask)
}

/// 1 エントリぶんの派生文字列が、どこまで上流と重複しているか。
///
/// 鎖は `lower_file_name` → `lower_name` → `name` の順で、各段が上流と一致すれば持たずに済む。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DerivedSharing {
    /// `lower_name` が `name` とバイト一致する（`normalize_for_match` が名前をそのまま返す）。
    pub lower_name_is_name: bool,
    /// `lower_file_name` が `lower_name` とバイト一致する。
    ///
    /// **`None`（file name 成分が無い）はここに含まれない。** 「無い」と「同一」は別の状態で
    /// あり、混ぜると `entry_view` が file name として `lower_name` を返す。
    pub file_name_is_lower_name: bool,
}

/// 派生文字列の共有を**測る**（推論しない）。
///
/// **記録側（`indexer::save_cache_sorted_in`）と適用側（`SearchEngine::assemble`）が同じ関数を
/// 通ることだけが、ディスクとメモリで潰れ方が一致する根拠である**（`normalize_entry_key_into`
/// と同じ理屈）。片方だけを書き換えると、索引はディスクの潰し方を信じて読み替えるので、
/// **結果が静かにずれる**——クラッシュも検索の失敗も起こさず、スコアだけが変わる。
///
/// **両方を同時に測るのは、呼び出し側に順序の責任を持たせないためである。** file name 側の
/// 比較相手は「潰す前の `lower_name`」であり、name 側を先に潰すとその相手が消えて共有を
/// 取りこぼす（結果は正しいまま削減だけが減るので、挙動テストでは捕まらない）。
///
/// **`is_folder` から推論してはならない。** 実データでは folder の 100% が一致するが、それは
/// indexer の名前導出規則（folder は `file_name()`、file は `file_stem()`）の帰結であって、
/// `SearchEngine::new` が受け取る `AppEntry` の性質ではない。
#[inline]
pub fn measure_derived_sharing(
    name: &str,
    lower_name: &str,
    lower_file_name: Option<&str>,
) -> DerivedSharing {
    DerivedSharing {
        lower_name_is_name: lower_name == name,
        file_name_is_lower_name: lower_file_name == Some(lower_name),
    }
}

/// Derive the lowercase, accent-folded file name from an entry's `target_path`, or
/// `None` if the path has no file name component. Single definition shared by
/// `search.rs` (Wave 1) and `indexer.rs` (cache build / incremental extend) —
/// previously duplicated in 3 call sites (issue #437).
#[inline]
pub fn lower_file_name(target_path: &str) -> Option<String> {
    std::path::Path::new(target_path)
        .file_name()
        .and_then(|f| f.to_str())
        .map(|f| normalize_for_match(f).into_owned())
}

/// 1 文字ぶんの照合用の写像。小文字化 → 折りたたみ → 小文字化の順である。
///
/// **最後の小文字化が冪等性を閉じる。** nucleo の表は小文字を大文字へ写すことがあり
/// （`ʀ` → `R`・U+0262 `ɢ` → `G`）、そこで止めると 2 回目の適用で結果が変わる。
#[inline]
fn fold_char(c: char) -> impl Iterator<Item = char> {
    c.to_lowercase()
        .map(nucleo_normalize)
        .flat_map(char::to_lowercase)
}

/// [`fold_char`] が `c` をそれ自身 1 文字へ写すか。借用の判定は、組み立てと同じ写像に
/// この問いを当てるだけで決める——別の条件を書くと、片方だけが変わる文字で割れる（#1310）。
#[inline]
fn folds_to_itself(c: char) -> bool {
    if c.is_ascii() {
        // ASCII では nucleo の表は恒等であり、変わるのは大文字だけ（全コードポイントの
        // 検知器がこの近道も通す）。
        return !c.is_ascii_uppercase();
    }
    let mut it = fold_char(c);
    it.next() == Some(c) && it.next().is_none()
}

/// 照合用の正規化の唯一の入口（SPEC §4.2）。クエリとエントリ名の両側がこれを通る。
///
/// 前後の空白を除き、空白類（`char::is_whitespace`）を `' '` へ揃えて連続を 1 つに圧縮し、
/// 各文字に [`fold_char`] を当てる。変える文字が無ければ trim 済みの入力を借用で返す
/// （ASCII の小文字と単一の空白だけのクエリは確保しない）。
///
/// 借用と所有の一致・冪等性は、全コードポイントの検知器で固定してある
/// （`normalize_for_match_agrees_with_the_owned_reference_on_every_codepoint`・
/// `normalize_for_match_is_idempotent_on_every_codepoint`）。
pub fn normalize_for_match(s: &str) -> Cow<'_, str> {
    let trimmed = s.trim();

    let mut prev_space = false;
    let borrowable = trimmed.chars().all(|ch| {
        if ch.is_whitespace() {
            let single = ch == ' ' && !prev_space;
            prev_space = true;
            single
        } else {
            prev_space = false;
            folds_to_itself(ch)
        }
    });
    if borrowable {
        return Cow::Borrowed(trimmed);
    }

    // 写した結果が空白になる文字も空白の連続として扱い、圧縮と前後の除去を写したあとの
    // 列に当てる。今の nucleo の表にそういう文字は無い（全コードポイントで 0 件）が、
    // 表が変わって現れれば、ここで扱わない限り 2 回目の適用で結果が変わる——そのときは
    // 冪等性の検知器が落ちる。
    let mut out = String::with_capacity(trimmed.len());
    let mut pending_space = false;
    let mut push = |q: char| {
        if q.is_whitespace() {
            pending_space = !out.is_empty();
        } else {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            out.push(q);
        }
    };
    for ch in trimmed.chars() {
        if ch.is_whitespace() {
            push(' ');
        } else {
            fold_char(ch).for_each(&mut push);
        }
    }

    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::{
        DerivedSharing, measure_derived_sharing, normalize_for_match, normalize_history_query_key,
        to_kana,
    };
    use nucleo_matcher::chars::normalize as nucleo_normalize;
    use std::borrow::Cow;

    /// **`lower_file` が `None` のとき、共有フラグを立ててはならない。**
    ///
    /// `None` は「file name 成分が無い」であって「`lower_name` と同一」ではない。ここを
    /// 取り違えると `entry_view` が `Some(lower_name)` を返し、file name を持たないエントリが
    /// file name 経由でマッチする。**実データの影響は小さくない**——フォルダ 256,262 件は
    /// すべて `None` である（実運用点の内訳で folder 側が 0 件なのはそれが理由）。
    ///
    /// しかも `lower_name` 側も同時に潰れていると、素朴な `lower_file == lower_name` 比較は
    /// `None == None` で真になる。**その形はディスク由来の潰し済み入力でだけ現れる**ので、
    /// 潰す前の値どうしを比べるこの関数を通すことが唯一の防ぎ方である。
    #[test]
    fn absent_file_name_never_shares_even_when_lower_name_is_shared() {
        let s = measure_derived_sharing("Projects", "projects", None);
        assert!(!s.file_name_is_lower_name, "None は「同一」ではない");

        // `name` がそのまま小文字（= lower_name を潰せる）フォルダでも同じ。
        let s = measure_derived_sharing("projects", "projects", None);
        assert_eq!(
            s,
            DerivedSharing {
                lower_name_is_name: true,
                file_name_is_lower_name: false,
            }
        );
    }

    #[test]
    fn measures_both_sharings_independently() {
        // file と lower_name が一致し、name も小文字（両方潰せる）。
        assert_eq!(
            measure_derived_sharing("readme", "readme", Some("readme")),
            DerivedSharing {
                lower_name_is_name: true,
                file_name_is_lower_name: true,
            }
        );
        // name に大文字があるので lower_name は潰せないが、file は lower_name と同一。
        assert_eq!(
            measure_derived_sharing("README", "readme", Some("readme")),
            DerivedSharing {
                lower_name_is_name: false,
                file_name_is_lower_name: true,
            }
        );
        // file が別物（`file_stem` と `file_name` の差＝拡張子）。
        assert_eq!(
            measure_derived_sharing("firefox", "firefox", Some("firefox.lnk")),
            DerivedSharing {
                lower_name_is_name: true,
                file_name_is_lower_name: false,
            }
        );
    }

    /// **判定は「潰す前の値」に対して行う。** file name 側の比較相手は解決後の `lower_name`
    /// であり、name 側を先に潰すとその相手が消える。この関数が両方を同時に測るのは、
    /// 呼び出し側に順序の責任を持たせないためである（`assemble` はかつてコメントで
    /// 順序を約束していた）。
    #[test]
    fn file_name_sharing_is_measured_against_the_uncollapsed_lower_name() {
        // lower_name が name と同一（潰せる）でも、file 側の判定はその文字列に対して行う。
        let s = measure_derived_sharing("projects", "projects", Some("projects"));
        assert!(s.lower_name_is_name);
        assert!(
            s.file_name_is_lower_name,
            "name 側を先に潰した実装ではここが false になり、共有を取りこぼす"
        );
    }

    #[test]
    fn trim_and_lowercase() {
        assert_eq!(normalize_for_match("  HeLLo  "), "hello");
    }

    #[test]
    fn collapse_whitespace() {
        assert_eq!(normalize_for_match("a   b\t\tc"), "a b c");
    }

    #[test]
    fn requires_alloc_for_tabs() {
        let q = normalize_for_match("foo\tbar");
        assert!(matches!(q, Cow::Owned(_)));
        assert_eq!(q, "foo bar");
    }

    #[test]
    fn requires_alloc_for_fullwidth_space() {
        let q = normalize_for_match("foo\u{3000}bar");
        assert!(matches!(q, Cow::Owned(_)));
        assert_eq!(q, "foo bar");
    }

    #[test]
    fn requires_alloc_for_nbsp() {
        let q = normalize_for_match("foo bar");
        assert!(matches!(q, Cow::Owned(_)));
        assert_eq!(q, "foo bar");
    }

    /// 正規化後に空なら空を返す。空の扱いは呼び出し側の面が決める（SPEC §4.2）。
    #[test]
    fn whitespace_only_normalizes_to_empty() {
        assert_eq!(normalize_for_match(" \t\u{3000}\u{a0}"), "");
    }

    #[test]
    fn accent_folding_resume() {
        assert_eq!(normalize_for_match("résumé"), "resume");
        assert_eq!(normalize_for_match("RÉSUMÉ"), "resume");
    }

    #[test]
    fn accent_folding_cafe() {
        assert_eq!(normalize_for_match("Café"), "cafe");
    }

    #[test]
    fn accent_folding_naive() {
        assert_eq!(normalize_for_match("naïve"), "naive");
    }

    #[test]
    fn accent_folding_allocates() {
        let q = normalize_for_match("café");
        assert!(matches!(q, Cow::Owned(_)));
    }

    #[test]
    fn ascii_only_borrows() {
        let q = normalize_for_match("hello");
        assert!(matches!(q, Cow::Borrowed(_)));
    }

    #[test]
    fn lowercases_ascii_and_keeps_single_spaces() {
        assert_eq!(normalize_for_match("Hello World"), "hello world");
    }

    #[test]
    fn to_kana_converts_romaji_to_hiragana() {
        assert_eq!(to_kana("dokyu"), "どきゅ");
    }

    #[test]
    fn to_kana_converts_katakana_to_hiragana() {
        assert_eq!(to_kana("ドキュメント"), "どきゅめんと");
    }

    #[test]
    fn to_kana_normalizes_halfwidth_katakana() {
        // wana_kana 5.0 で半角カタカナの正規化が to_hiragana に入った（#365 / crate PR #21）。
        // to_kana はクエリと索引名の両方に対称適用されるため、半角カナ入力もひらがなとして
        // マッチするようになる。この有益な新挙動を回帰として固定する。
        assert_eq!(to_kana("ﾜﾅｶﾅ"), "わなかな");
        // 半角濁点（ﾞ U+FF9E）の合成も全角と同じく濁音ひらがなになる。
        assert_eq!(to_kana("ｶﾞ"), "が");
    }

    #[test]
    fn to_kana_fully_converts_when_possible() {
        // "dokyu" は完全にひらがなに変換され、ASCII アルファベットが残留しない。
        // kana_query ガード（ASCII 残留チェック）の前提を確認する。
        let result = to_kana("dokyu");
        assert!(
            !result.bytes().any(|b| b.is_ascii_alphabetic()),
            "完全に変換できるローマ字は ASCII アルファベットが残留しないはず: {}",
            result
        );
    }

    #[test]
    fn to_kana_leaves_ascii_residue_for_partial_romaji() {
        // "documents" の "ts" は wana_kana が単独のかなにマップできないため残留する。
        // → kana_query ガードが None にして migemo マッチをスキップする。
        let result = to_kana("documents");
        assert!(
            result.bytes().any(|b| b.is_ascii_alphabetic()),
            "部分変換で ASCII 残留が発生するはず: {}",
            result
        );
    }

    #[test]
    fn to_kana_passes_through_kanji() {
        let result = to_kana("書類");
        assert!(result.contains("書類"));
    }

    #[test]
    fn history_query_key_unifies_slash() {
        assert_eq!(normalize_history_query_key("tool/editor"), "tool\\editor");
    }

    #[test]
    fn history_query_key_unifies_yen() {
        assert_eq!(
            normalize_history_query_key("tool\u{00a5}editor"),
            "tool\\editor"
        );
    }

    #[test]
    fn history_query_key_folds_accents() {
        // normalize_for_match が é→e に折りたたむことを確認
        assert_eq!(normalize_history_query_key("café\\foo"), "cafe\\foo");
    }

    #[test]
    fn history_query_key_collapses_spaces() {
        // normalize_for_match が連続スペースを圧縮することを確認
        assert_eq!(
            normalize_history_query_key("my  tools\\foo"),
            "my tools\\foo"
        );
    }

    #[test]
    fn history_query_key_no_path_sep_passthrough() {
        // パス区切りなしの場合は normalize_for_match と同じ
        assert_eq!(normalize_history_query_key("hello"), "hello");
    }

    #[test]
    fn history_query_key_borrows_when_no_change() {
        // ASCII 小文字 + バックスラッシュのみの場合は Cow::Borrowed
        let result = normalize_history_query_key("tool\\editor");
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    /// 常に所有で組み立てる素朴な参照実装。借用の判定を持たないので、`normalize_for_match`
    /// の借用路と所有路のどちらがずれても、この出力との不一致として現れる（#1310）。
    fn reference_normalize(s: &str) -> String {
        let mapped: String = s
            .chars()
            .flat_map(|c| -> Vec<char> {
                if c.is_whitespace() {
                    vec![' ']
                } else {
                    c.to_lowercase()
                        .map(nucleo_normalize)
                        .flat_map(char::to_lowercase)
                        .collect()
                }
            })
            .collect();
        mapped.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// 全コードポイントを、#1310 の表の文脈と空白をはさむ文脈に置いた入力。
    fn all_codepoint_inputs() -> impl Iterator<Item = String> {
        (0..=0x10FFFF_u32).filter_map(char::from_u32).flat_map(|c| {
            [
                format!("{c}"),
                format!("a{c}"),
                format!("A{c}x"),
                format!("x  {c}"),
                format!("{c} {c}"),
            ]
        })
    }

    fn report(failures: &[String]) -> String {
        format!(
            "{} 件: {:?}",
            failures.len(),
            failures.iter().take(8).collect::<Vec<_>>()
        )
    }

    /// **借用で返す条件と、所有で組み立てる結果が、全コードポイントで一致する**（#1310）。
    /// 借用で返るのは、組み立てた結果が trim 済みの入力と等しいときに限る——不一致は、
    /// 借用路が「変わらない」と判定した文字を所有路が変える形でしか起きない。
    #[test]
    fn normalize_for_match_agrees_with_the_owned_reference_on_every_codepoint() {
        let failures: Vec<String> = all_codepoint_inputs()
            .filter(|s| normalize_for_match(s) != reference_normalize(s).as_str())
            .map(|s| format!("{:?}", s))
            .collect();
        assert!(
            failures.is_empty(),
            "参照実装との不一致 {}",
            report(&failures)
        );
    }

    /// **正規化は冪等で、出力を入れ直すと借用で返る。** 履歴のキーは読み込みのたびに
    /// `migrate_normalize_keys` で正規化し直されるので、冪等でない文字を含むキーは
    /// 記録時の形と食い違う。`normalize_history_query_key` も同じ性質を持つ。
    #[test]
    fn normalize_for_match_is_idempotent_on_every_codepoint() {
        let mut failures = Vec::new();
        for s in all_codepoint_inputs() {
            let once = normalize_for_match(&s).into_owned();
            let twice = normalize_for_match(&once);
            if twice != once.as_str() || !matches!(twice, Cow::Borrowed(_)) {
                failures.push(format!("{:?} -> {:?} -> {:?}", s, once, twice));
            }
            let key = normalize_history_query_key(&s).into_owned();
            let key2 = normalize_history_query_key(&key);
            if key2 != key.as_str() || !matches!(key2, Cow::Borrowed(_)) {
                failures.push(format!("history {:?} -> {:?} -> {:?}", s, key, key2));
            }
        }
        assert!(failures.is_empty(), "冪等でない {}", report(&failures));
    }
}
