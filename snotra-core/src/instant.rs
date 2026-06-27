use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};

use crate::config::InstantCommand;

/// シェル風クォート対応の引数分割。
/// `"..."` で囲まれた部分はスペースを含んでも1トークンとして扱う。
/// 閉じクォートがない場合は行末まで1トークン。
/// 空クォート `""` はトークンを生成しない。
pub fn split_args(args: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in args.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            c if c.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// `{query}` / `{clip}` を生のまま置換する。URL エンコードはしない。
pub fn expand_vars(template: &str, query: &str, clipboard: &str) -> String {
    template.replace("{query}", query).replace("{clip}", clipboard)
}

/// exec 種別の引数トークン列を構築する。
/// 手順: `split_args` で分割 → 各トークンに `env_expand`（環境変数展開）→ `{query}`/`{clip}` 置換。
/// この順序により (1) 外部入力 query/clip は env 展開されない、(2) env 値の空白は
/// トークン内に留まり引数を割らない、(3) 空白入り query は1引数を保つ。
/// `build_launch_args` の `{path}` 末尾補完は行わない（exec は path を持たない）。
pub fn expand_exec_args(
    args: &str,
    query: &str,
    clipboard: &str,
    env_expand: impl Fn(&str) -> String,
) -> Vec<String> {
    split_args(args)
        .into_iter()
        .map(|tok| expand_vars(&env_expand(&tok), query, clipboard))
        .collect()
}

/// Expand `{query}` and `{clip}` placeholders in an instant command template.
///
/// If the command starts with `http://` or `https://`, variable values are
/// URL-encoded before substitution. Otherwise they are inserted as-is.
pub fn expand_instant_command(command: &str, query: &str, clipboard: &str) -> String {
    let is_url = command.starts_with("http://") || command.starts_with("https://");

    if is_url {
        let q = utf8_percent_encode(query, NON_ALPHANUMERIC).to_string();
        let c = utf8_percent_encode(clipboard, NON_ALPHANUMERIC).to_string();
        expand_vars(command, &q, &c)
    } else {
        expand_vars(command, query, clipboard)
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

    // ---- expand_exec_args tests ----
    fn no_env(s: &str) -> String { s.to_string() }

    #[test]
    fn exec_args_empty_is_no_tokens() {
        let r = expand_exec_args("", "q", "c", no_env);
        assert!(r.is_empty()); // build_launch_args と異なり末尾 append しない
    }
    #[test]
    fn exec_args_query_with_spaces_stays_one_arg() {
        let r = expand_exec_args("-s {query}", "hello world", "", no_env);
        assert_eq!(r, vec!["-s", "hello world"]);
    }
    #[test]
    fn exec_args_query_cannot_inject_extra_args() {
        let r = expand_exec_args("-s {query}", "--flag a b", "", no_env);
        assert_eq!(r, vec!["-s", "--flag a b"]); // 展開は split 後なので1引数のまま
    }
    #[test]
    fn exec_args_query_quote_is_literal() {
        let r = expand_exec_args("{query}", "a\"b", "", no_env);
        assert_eq!(r, vec!["a\"b"]); // split は展開前に走るので再分割しない
    }
    #[test]
    fn exec_args_clip_newline_is_literal() {
        let r = expand_exec_args("{clip}", "", "a\nb", no_env);
        assert_eq!(r, vec!["a\nb"]);
    }
    #[test]
    fn exec_args_empty_query_yields_empty_arg() {
        let r = expand_exec_args("-s {query}", "", "", no_env);
        assert_eq!(r, vec!["-s", ""]);
    }
    #[test]
    fn exec_args_inline_placeholder_preserves_space() {
        let r = expand_exec_args("-s={query}", "hello world", "", no_env);
        assert_eq!(r, vec!["-s=hello world"]);
    }
    #[test]
    fn exec_args_env_value_with_space_stays_in_token() {
        // env 展開は split 後なので env 値の空白が引数を割らない
        let env = |s: &str| s.replace("%FOO%", "C:\\a b");
        let r = expand_exec_args("--dir %FOO%", "", "", env);
        assert_eq!(r, vec!["--dir", "C:\\a b"]);
    }
    #[test]
    fn exec_args_external_input_is_not_env_expanded() {
        // query が運んだ %FOO% は展開されない（env 展開はトークン→置換の順で置換が後）
        let env = |s: &str| s.replace("%FOO%", "EXPANDED");
        let r = expand_exec_args("{query}", "%FOO%", "", env);
        assert_eq!(r, vec!["%FOO%"]);
    }

    // ---- split_args (quote-aware splitting) tests ----
    #[test]
    fn split_args_quoted_token_preserves_spaces() {
        assert_eq!(split_args(r#"--dir "My Documents""#), vec!["--dir", "My Documents"]);
    }
    #[test]
    fn split_args_unclosed_quote_consumes_to_end() {
        assert_eq!(split_args(r#"--dir "My Documents"#), vec!["--dir", "My Documents"]);
    }
    #[test]
    fn split_args_adjacent_quotes_join() {
        assert_eq!(split_args(r#"--open="My File""#), vec!["--open=My File"]);
    }
    #[test]
    fn split_args_empty_quotes_produce_no_token() {
        assert_eq!(split_args(r#"a "" b"#), vec!["a", "b"]);
    }
    #[test]
    fn split_args_plain_whitespace_only() {
        assert_eq!(split_args("  -a   -b  "), vec!["-a", "-b"]);
    }
}
