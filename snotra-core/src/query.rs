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
    use super::{normalize_query, to_kana, to_lower_folded};
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
}
