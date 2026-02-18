const KNOWN_EXTENSIONS: &[&str] = &[
    ".exe", ".lnk", ".bat", ".cmd", ".msi", ".com", ".scr", ".ps1",
];

/// クエリ末尾が既知の拡張子なら (stem, Some(ext)) を返す。そうでなければ (query, None)。
pub fn split_query_extension(query: &str) -> (&str, Option<&str>) {
    for ext in KNOWN_EXTENSIONS {
        if let Some(stem) = query.strip_suffix(ext) {
            return (stem, Some(ext));
        }
    }
    (query, None)
}

pub fn normalize_query(query: &str) -> String {
    let mut out = String::new();
    let mut prev_space = false;

    for ch in query.trim().chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.extend(ch.to_lowercase());
            prev_space = false;
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::{normalize_query, split_query_extension};

    #[test]
    fn trim_and_lowercase() {
        assert_eq!(normalize_query("  HeLLo  "), "hello");
    }

    #[test]
    fn collapse_whitespace() {
        assert_eq!(normalize_query("a   b\t\tc"), "a b c");
    }

    #[test]
    fn split_known_ext() {
        let (stem, ext) = split_query_extension("ssp.exe");
        assert_eq!(stem, "ssp");
        assert_eq!(ext, Some(".exe"));
    }

    #[test]
    fn split_unknown_ext() {
        let (stem, ext) = split_query_extension("config.toml");
        assert_eq!(stem, "config.toml");
        assert_eq!(ext, None);
    }
}
