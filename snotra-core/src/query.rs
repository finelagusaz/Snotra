use std::borrow::Cow;

pub fn normalize_query(query: &str) -> Cow<'_, str> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Cow::Borrowed("");
    }

    // Check if the string needs allocation: contains uppercase or multiple adjacent whitespaces
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
            if ch.is_uppercase() {
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
            out.extend(ch.to_lowercase());
            prev_space_build = false;
        }
    }

    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::normalize_query;
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
}
