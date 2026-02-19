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
    use super::normalize_query;

    #[test]
    fn trim_and_lowercase() {
        assert_eq!(normalize_query("  HeLLo  "), "hello");
    }

    #[test]
    fn collapse_whitespace() {
        assert_eq!(normalize_query("a   b\t\tc"), "a b c");
    }
}
