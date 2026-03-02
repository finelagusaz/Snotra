use std::borrow::Cow;

use nucleo_matcher::chars::normalize as nucleo_normalize;

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
    use super::{normalize_query, to_lower_folded};
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
}
