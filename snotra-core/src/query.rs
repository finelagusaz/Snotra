//! クエリと履歴キーの正規化、および文字存在ビットマスク計算。
//!
//! `normalize_query`（小文字化・アクセント折畳み・空白圧縮。ASCII 小文字のみなら `Cow` で
//! ゼロアロケーション）、履歴クエリキー正規化 `normalize_history_query_key`、検索/インデクサ
//! 双方が使う `char_bitmask`（bits 0-25 = a-z, 26-35 = 0-9）を一元定義する。正規化は冪等。

use std::borrow::Cow;

use nucleo_matcher::chars::normalize as nucleo_normalize;

/// ASCII ローマ字をひらがなに変換する（カタカナもひらがなに正規化）。
/// 非 ASCII 文字（漢字など）はそのまま通過する。
pub fn to_kana(s: &str) -> String {
    use wana_kana::ConvertJapanese;
    s.to_hiragana()
}

/// Lowercase + accent-fold a string using nucleo's normalization table.
/// This aligns with nucleo's fuzzy matcher behavior (é→e, ü→u, etc.).
/// Order: lowercase first, then normalize — nucleo's table covers both cases
/// (É→e, é→e) so the result is equivalent to nucleo's internal normalize→casefold.
pub fn to_lower_folded(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .map(nucleo_normalize)
        .collect()
}

/// `normalize_query` + パス区切り文字（`/` `¥`）を `\` に統一する。
/// 履歴キーの正規化はこの関数に一元化する（DRY）。
/// `record_launch`・`query_count`・`migrate_normalize_keys`・`search.rs` の
/// `path_history_key` が全て同じ正規化を使うことを保証する。
pub fn normalize_history_query_key(query: &str) -> Cow<'_, str> {
    let nq = normalize_query(query);
    if nq.contains('/') || nq.contains('\u{00a5}') {
        Cow::Owned(nq.replace(['/', '\u{00a5}'], "\\"))
    } else {
        nq
    }
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
/// (typically CJK, Arabic, etc. — `to_lower_folded` already folds Latin accents to
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

/// Derive the lowercase, accent-folded file name from an entry's `target_path`, or
/// `None` if the path has no file name component. Single definition shared by
/// `search.rs` (Wave 1) and `indexer.rs` (cache build / incremental extend) —
/// previously duplicated in 3 call sites (issue #437).
#[inline]
pub fn lower_file_name(target_path: &str) -> Option<String> {
    std::path::Path::new(target_path)
        .file_name()
        .and_then(|f| f.to_str())
        .map(to_lower_folded)
}

pub fn normalize_query(query: &str) -> Cow<'_, str> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Cow::Borrowed("");
    }

    // Check if the string needs allocation: contains uppercase, accented chars,
    // or multiple adjacent whitespaces
    let mut needs_alloc = false;
    let mut prev_space = false;
    for ch in trimmed.chars() {
        if ch.is_whitespace() {
            if prev_space || ch != ' ' {
                needs_alloc = true;
                break;
            }
            prev_space = true;
        } else {
            if ch.is_uppercase() || nucleo_normalize(ch) != ch {
                needs_alloc = true;
                break;
            }
            prev_space = false;
        }
    }

    if !needs_alloc {
        return Cow::Borrowed(trimmed);
    }

    // Allocate and build ONLY if necessary
    let mut out = String::with_capacity(trimmed.len());
    let mut prev_space_build = false;

    for ch in trimmed.chars() {
        if ch.is_whitespace() {
            if !prev_space_build {
                out.push(' ');
                prev_space_build = true;
            }
        } else {
            out.extend(ch.to_lowercase().map(nucleo_normalize));
            prev_space_build = false;
        }
    }

    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::{normalize_history_query_key, normalize_query, to_kana, to_lower_folded};
    use std::borrow::Cow;

    #[test]
    fn trim_and_lowercase() {
        assert_eq!(normalize_query("  HeLLo  "), "hello");
    }

    #[test]
    fn collapse_whitespace() {
        assert_eq!(normalize_query("a   b\t\tc"), "a b c");
    }

    #[test]
    fn requires_alloc_for_tabs() {
        let q = normalize_query("foo\tbar");
        assert!(matches!(q, Cow::Owned(_)));
        assert_eq!(q, "foo bar");
    }

    #[test]
    fn requires_alloc_for_fullwidth_space() {
        let q = normalize_query("foo　bar");
        assert!(matches!(q, Cow::Owned(_)));
        assert_eq!(q, "foo bar");
    }

    #[test]
    fn accent_folding_resume() {
        assert_eq!(normalize_query("résumé"), "resume");
    }

    #[test]
    fn accent_folding_cafe() {
        assert_eq!(normalize_query("Café"), "cafe");
    }

    #[test]
    fn accent_folding_naive() {
        assert_eq!(normalize_query("naïve"), "naive");
    }

    #[test]
    fn accent_folding_allocates() {
        let q = normalize_query("café");
        assert!(matches!(q, Cow::Owned(_)));
    }

    #[test]
    fn ascii_only_borrows() {
        let q = normalize_query("hello");
        assert!(matches!(q, Cow::Borrowed(_)));
    }

    #[test]
    fn to_lower_folded_strips_accents() {
        assert_eq!(to_lower_folded("Café"), "cafe");
        assert_eq!(to_lower_folded("RÉSUMÉ"), "resume");
        assert_eq!(to_lower_folded("naïve"), "naive");
    }

    #[test]
    fn to_lower_folded_ascii_unchanged() {
        assert_eq!(to_lower_folded("Hello World"), "hello world");
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
        // normalize_query が é→e に折りたたむことを確認
        assert_eq!(normalize_history_query_key("café\\foo"), "cafe\\foo");
    }

    #[test]
    fn history_query_key_collapses_spaces() {
        // normalize_query が連続スペースを圧縮することを確認
        assert_eq!(normalize_history_query_key("my  tools\\foo"), "my tools\\foo");
    }

    #[test]
    fn history_query_key_no_path_sep_passthrough() {
        // パス区切りなしの場合は normalize_query と同じ
        assert_eq!(normalize_history_query_key("hello"), "hello");
    }

    #[test]
    fn history_query_key_borrows_when_no_change() {
        // ASCII 小文字 + バックスラッシュのみの場合は Cow::Borrowed
        let result = normalize_history_query_key("tool\\editor");
        assert!(matches!(result, Cow::Borrowed(_)));
    }
}
