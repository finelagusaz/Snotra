use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};

use crate::config::InstantCommand;

/// Expand `{query}` and `{clip}` placeholders in an instant command template.
///
/// If the command starts with `http://` or `https://`, variable values are
/// URL-encoded before substitution. Otherwise they are inserted as-is.
pub fn expand_instant_command(command: &str, query: &str, clipboard: &str) -> String {
    let is_url = command.starts_with("http://") || command.starts_with("https://");

    if is_url {
        let q = utf8_percent_encode(query, NON_ALPHANUMERIC).to_string();
        let c = utf8_percent_encode(clipboard, NON_ALPHANUMERIC).to_string();
        command.replace("{query}", &q).replace("{clip}", &c)
    } else {
        command.replace("{query}", query).replace("{clip}", clipboard)
    }
}

/// Filter instant commands by prefix-matching `input` against command names.
/// An empty `input` returns all commands.
pub fn filter_instant_commands<'a>(
    commands: &'a [InstantCommand],
    input: &str,
) -> Vec<&'a InstantCommand> {
    if input.is_empty() {
        return commands.iter().collect();
    }
    let lower = input.to_lowercase();
    commands
        .iter()
        .filter(|c| c.name.to_lowercase().starts_with(&lower))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- expand_instant_command tests ----

    #[test]
    fn url_query_is_encoded() {
        let result = expand_instant_command(
            "https://google.com/search?q={query}",
            "機械学習",
            "",
        );
        assert!(result.contains("%E6%A9%9F%E6%A2%B0%E5%AD%A6%E7%BF%92"));
        assert!(!result.contains("機械学習"));
    }

    #[test]
    fn url_clip_is_encoded() {
        let result = expand_instant_command(
            "https://translate.com/?text={clip}",
            "",
            "hello world",
        );
        assert!(result.contains("hello%20world"));
        assert!(!result.contains("hello world"));
    }

    #[test]
    fn url_symbols_are_encoded() {
        let result = expand_instant_command(
            "https://example.com/?q={query}",
            "a&b=c",
            "",
        );
        assert!(result.contains("a%26b%3Dc"));
    }

    #[test]
    fn non_url_query_is_raw() {
        let result = expand_instant_command(
            "C:\\editor.exe {query}",
            "hello world",
            "",
        );
        assert_eq!(result, "C:\\editor.exe hello world");
    }

    #[test]
    fn non_url_clip_is_raw() {
        let result = expand_instant_command(
            "C:\\editor.exe {clip}",
            "",
            "clipboard text",
        );
        assert_eq!(result, "C:\\editor.exe clipboard text");
    }

    #[test]
    fn empty_query_expands_to_empty() {
        let result = expand_instant_command(
            "https://google.com/search?q={query}",
            "",
            "",
        );
        assert_eq!(result, "https://google.com/search?q=");
    }

    #[test]
    fn no_placeholders_returns_as_is() {
        let result = expand_instant_command("C:\\tools\\calc.exe", "ignored", "ignored");
        assert_eq!(result, "C:\\tools\\calc.exe");
    }

    #[test]
    fn both_placeholders_in_url() {
        let result = expand_instant_command(
            "https://example.com/?q={query}&c={clip}",
            "test",
            "data",
        );
        assert_eq!(result, "https://example.com/?q=test&c=data");
    }

    // ---- filter_instant_commands tests ----

    fn sample_commands() -> Vec<InstantCommand> {
        vec![
            InstantCommand { name: "g".to_string(), command: "https://google.com?q={query}".to_string(), description: String::new() },
            InstantCommand { name: "gm".to_string(), command: "https://mail.google.com".to_string(), description: String::new() },
            InstantCommand { name: "n".to_string(), command: "notepad.exe".to_string(), description: String::new() },
        ]
    }

    #[test]
    fn filter_empty_returns_all() {
        let cmds = sample_commands();
        let result = filter_instant_commands(&cmds, "");
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn filter_prefix_match() {
        let cmds = sample_commands();
        let result = filter_instant_commands(&cmds, "g");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "g");
        assert_eq!(result[1].name, "gm");
    }

    #[test]
    fn filter_exact_match() {
        let cmds = sample_commands();
        let result = filter_instant_commands(&cmds, "gm");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "gm");
    }

    #[test]
    fn filter_no_match() {
        let cmds = sample_commands();
        let result = filter_instant_commands(&cmds, "xyz");
        assert!(result.is_empty());
    }

    #[test]
    fn filter_case_insensitive() {
        let cmds = vec![
            InstantCommand { name: "Google".to_string(), command: "https://google.com".to_string(), description: String::new() },
        ];
        let result = filter_instant_commands(&cmds, "google");
        assert_eq!(result.len(), 1);
    }
}
