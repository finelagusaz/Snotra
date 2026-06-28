use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};

use crate::config::InstantCommand;

/// シェル風クォート対応の引数分割。
/// `"..."` で囲まれた部分はスペースを含んでも1トークンとして扱う。
/// `{...}`（変数プレースホルダ）の内側のスペースも分割しない——`{query | trim}` の
/// ように修飾子パイプを含むプレースホルダを1トークンに保つため（exec の argv 不可分）。
/// 閉じクォートがない場合は行末まで1トークン。空クォート `""` はトークンを生成しない。
pub fn split_args(args: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut brace_depth = 0u32;
    for ch in args.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            '{' if !in_quotes => {
                brace_depth += 1;
                current.push(ch);
            }
            '}' if !in_quotes => {
                brace_depth = brace_depth.saturating_sub(1);
                current.push(ch);
            }
            c if c.is_whitespace() && !in_quotes && brace_depth == 0 => {
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

/// 変数に付与できる変換修飾子（v1）。
#[derive(Debug, Clone, PartialEq, Eq)]
enum Modifier {
    Lower,
    Upper,
    Trim,
    /// 値が空のとき代替する既定文字列。
    Default(String),
    /// URL 種別の自動エンコードを抑止するフラグ（値そのものは変えない）。
    Raw,
}

/// 認識する変数名。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VarKind {
    Query,
    Clip,
}

/// 修飾子セグメント。`Unknown` は設定保存時バリデーションが拒否する。
#[derive(Debug, Clone, PartialEq, Eq)]
enum ModSeg {
    Known(Modifier),
    Unknown(String),
}

/// `{...}` の解析結果（変数 + 修飾子チェーン）。
struct Placeholder {
    var: VarKind,
    mods: Vec<ModSeg>,
}

/// テンプレート走査イベント。
enum TplEvent<'a> {
    Literal(&'a str),
    Placeholder(&'a Placeholder),
}

/// `{...}` の内側文字列（波括弧を除いた部分）を解析する。
/// name が `query` / `clip` 以外なら `None`（呼び出し側がリテラル扱い）。
/// **runtime 適用と保存時バリデーションで共有する単一パーサ**。
fn parse_placeholder(inner: &str) -> Option<Placeholder> {
    let mut segments = inner.split('|');
    let var = match segments.next()?.trim() {
        "query" => VarKind::Query,
        "clip" => VarKind::Clip,
        _ => return None,
    };
    let mods = segments.map(parse_modifier).collect();
    Some(Placeholder { var, mods })
}

/// 1個の修飾子セグメントを解析する。`name:arg` は最初の `:` で分割し、
/// arg 内の `:` はリテラル（例: `default:about:blank` → arg = `about:blank`）。
fn parse_modifier(seg: &str) -> ModSeg {
    let (name, arg) = match seg.trim().split_once(':') {
        // arg の前後空白は trim（SPEC §19.4「`:` 周りの空白は任意」）。内部空白は保持される。
        Some((n, a)) => (n.trim(), Some(a.trim())),
        None => (seg.trim(), None),
    };
    match name {
        "lower" => ModSeg::Known(Modifier::Lower),
        "upper" => ModSeg::Known(Modifier::Upper),
        "trim" => ModSeg::Known(Modifier::Trim),
        "raw" => ModSeg::Known(Modifier::Raw),
        "default" => ModSeg::Known(Modifier::Default(arg.unwrap_or("").to_string())),
        other => ModSeg::Unknown(other.to_string()),
    }
}

/// 修飾子チェーンを左→右に適用する。戻り値 `.1` は `raw` 検出フラグ。
fn apply_modifiers(value: &str, mods: &[ModSeg]) -> (String, bool) {
    let mut v = value.to_string();
    let mut raw = false;
    for m in mods {
        match m {
            ModSeg::Known(Modifier::Lower) => v = v.to_lowercase(),
            ModSeg::Known(Modifier::Upper) => v = v.to_uppercase(),
            ModSeg::Known(Modifier::Trim) => v = v.trim().to_string(),
            ModSeg::Known(Modifier::Default(d)) => {
                if v.is_empty() {
                    v = d.clone();
                }
            }
            ModSeg::Known(Modifier::Raw) => raw = true,
            // 保存時バリデーションが拒否済み。runtime に到達したら無視する。
            ModSeg::Unknown(_) => {}
        }
    }
    (v, raw)
}

/// テンプレートを走査し、リテラル片と認識プレースホルダを順に `on_event` へ渡す。
/// 認識しない `{...}`・閉じ `}` 不在はリテラルとして温存する（total・panic しない）。
/// `expand_template`（適用）と `collect_unknown_modifiers`（検証）が共有する。
fn walk_template(template: &str, mut on_event: impl FnMut(TplEvent)) {
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        on_event(TplEvent::Literal(&rest[..open]));
        let after = &rest[open + 1..];
        if let Some(close) = after.find('}') {
            let inner = &after[..close];
            match parse_placeholder(inner) {
                Some(ph) => on_event(TplEvent::Placeholder(&ph)),
                None => {
                    // 認識しない `{...}` はリテラル温存
                    on_event(TplEvent::Literal("{"));
                    on_event(TplEvent::Literal(inner));
                    on_event(TplEvent::Literal("}"));
                }
            }
            rest = &after[close + 1..];
        } else {
            // 閉じ `}` が無い → 残りをリテラルに
            on_event(TplEvent::Literal("{"));
            on_event(TplEvent::Literal(after));
            return;
        }
    }
    on_event(TplEvent::Literal(rest));
}

/// インスタントコマンドのテンプレートを展開する。
/// `{query|mods}` / `{clip|mods}` を 変数解決 → 修飾子チェーン →（`encode && !raw` のとき
/// URL エンコード）で置換する。**エンコードはシンク（種別）の責務**であり、修飾子は内容変換のみ。
fn expand_template(template: &str, query: &str, clipboard: &str, encode: bool) -> String {
    let mut out = String::with_capacity(template.len());
    walk_template(template, |ev| match ev {
        TplEvent::Literal(s) => out.push_str(s),
        TplEvent::Placeholder(ph) => {
            let raw_value = match ph.var {
                VarKind::Query => query,
                VarKind::Clip => clipboard,
            };
            let (value, raw) = apply_modifiers(raw_value, &ph.mods);
            if encode && !raw {
                out.extend(utf8_percent_encode(&value, NON_ALPHANUMERIC));
            } else {
                out.push_str(&value);
            }
        }
    });
    out
}

/// テンプレート中の `{query|…}` / `{clip|…}` から未知の修飾子名を収集する。
/// 設定保存時バリデーション（`Config::validate`）が使用。`walk_template` を共有。
pub fn collect_unknown_modifiers(template: &str) -> Vec<String> {
    let mut unknown = Vec::new();
    walk_template(template, |ev| {
        if let TplEvent::Placeholder(ph) = ev {
            for m in &ph.mods {
                // 空セグメント（`{query | }` 等の末尾/連続パイプ）は Unknown("") になるが
                // 報告しない（runtime も no-op。空名のエラーは原因が伝わらないため）。
                match m {
                    ModSeg::Unknown(name) if !name.is_empty() => unknown.push(name.clone()),
                    _ => {}
                }
            }
        }
    });
    unknown
}

/// Expand `{query}` / `{clip}`（修飾子パイプ対応）in a URL/command template.
///
/// `http://` / `https://` で始まる場合は各プレースホルダ単位で URL エンコードする
/// （`raw` 修飾子が付く値は抑止）。それ以外は生のまま展開する。
pub fn expand_instant_command(command: &str, query: &str, clipboard: &str) -> String {
    let is_url = command.starts_with("http://") || command.starts_with("https://");
    expand_template(command, query, clipboard, is_url)
}

/// exec 種別の引数トークン列を構築する。
/// 手順: `split_args` で分割 → 各トークンに `env_expand`（環境変数展開）→ 修飾子パイプ付き変数置換。
/// この順序により (1) 外部入力 query/clip は env 展開されない、(2) env 値の空白は
/// トークン内に留まり引数を割らない、(3) 空白入り query は1引数を保つ。
/// 修飾子適用後の値も同じトークン内に in-place 置換され、引数を増やさない。
/// `build_launch_args` の `{path}` 末尾補完は行わない（exec は path を持たない）。
pub fn expand_exec_args(
    args: &str,
    query: &str,
    clipboard: &str,
    env_expand: impl Fn(&str) -> String,
) -> Vec<String> {
    split_args(args)
        .into_iter()
        .map(|tok| expand_template(&env_expand(&tok), query, clipboard, false))
        .collect()
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
    use crate::config::InstantAction;

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
            InstantCommand { name: "g".to_string(), description: String::new(), action: InstantAction::Url { url: "https://google.com?q={query}".into() } },
            InstantCommand { name: "gm".to_string(), description: String::new(), action: InstantAction::Url { url: "https://mail.google.com".into() } },
            InstantCommand { name: "n".to_string(), description: String::new(), action: InstantAction::Url { url: "notepad.exe".into() } },
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
            InstantCommand { name: "Google".to_string(), description: String::new(), action: InstantAction::Url { url: "https://google.com".into() } },
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
    #[test]
    fn split_args_keeps_braced_placeholder_with_spaces() {
        // `{...}` 内の空白は分割しない（修飾子パイプを1トークンに保つ）
        assert_eq!(split_args("-s {query | trim}"), vec!["-s", "{query | trim}"]);
    }
    #[test]
    fn split_args_keeps_braced_default_with_spaces() {
        assert_eq!(
            split_args("{query | default:a b}"),
            vec!["{query | default:a b}"]
        );
    }
    #[test]
    fn split_args_unbalanced_brace_in_quotes_does_not_leak() {
        // クォート内の不均衡 `{` は brace_depth に影響しない（クォート外へ持ち越さない）
        assert_eq!(split_args(r#""a{b" c"#), vec!["a{b", "c"]);
    }

    // ---- modifier pipe: URL sink ----
    #[test]
    fn url_modifier_lower() {
        let r = expand_instant_command("https://x.com/?q={query | lower}", "FooBar", "");
        assert_eq!(r, "https://x.com/?q=foobar");
    }
    #[test]
    fn url_modifier_upper() {
        let r = expand_instant_command("https://x.com/?q={query | upper}", "abc", "");
        assert_eq!(r, "https://x.com/?q=ABC");
    }
    #[test]
    fn url_modifier_trim_then_encode() {
        // trim 後に（シンクが）URL エンコード
        let r = expand_instant_command("https://x.com/?q={query | trim}", "  Foo Bar  ", "");
        assert_eq!(r, "https://x.com/?q=Foo%20Bar");
    }
    #[test]
    fn url_modifier_lower_then_raw_suppresses_encode() {
        // lower 適用後、raw でエンコード抑止 → スラッシュ温存
        let r = expand_instant_command("https://x.com/{query | lower | raw}/end", "Docs/API", "");
        assert_eq!(r, "https://x.com/docs/api/end");
    }
    #[test]
    fn url_without_raw_encodes_slash() {
        let r = expand_instant_command("https://x.com/{query}/end", "a/b", "");
        assert_eq!(r, "https://x.com/a%2Fb/end");
    }
    #[test]
    fn url_modifier_whitespace_is_optional() {
        let a = expand_instant_command("https://x.com/?q={query|lower}", "AB", "");
        let b = expand_instant_command("https://x.com/?q={query | lower}", "AB", "");
        assert_eq!(a, b);
        assert_eq!(a, "https://x.com/?q=ab");
    }
    #[test]
    fn url_clip_modifier() {
        let r = expand_instant_command("https://x.com/?q={clip | upper}", "", "hi");
        assert_eq!(r, "https://x.com/?q=HI");
    }

    // ---- modifier pipe: default ----
    #[test]
    fn default_fills_empty_query() {
        let r = expand_instant_command("https://x.com/{query | default:home}", "", "");
        assert_eq!(r, "https://x.com/home");
    }
    #[test]
    fn default_ignored_when_nonempty() {
        let r = expand_instant_command("https://x.com/{query | default:home}", "page", "");
        assert_eq!(r, "https://x.com/page");
    }
    #[test]
    fn default_arg_keeps_second_colon() {
        // default:about:blank → 既定値は "about:blank"（2個目以降の : はリテラル）
        let r = expand_instant_command("C:\\b.exe {query | default:about:blank}", "", "");
        assert_eq!(r, "C:\\b.exe about:blank");
    }
    #[test]
    fn default_arg_is_trimmed() {
        // `:` 直後の空白は trim（SPEC §19.4「`:` 周りの空白は任意」）。内部空白は保持
        let trimmed = expand_instant_command("C:\\b.exe {query | default: home}", "", "");
        assert_eq!(trimmed, "C:\\b.exe home");
        let inner = expand_instant_command("C:\\b.exe {query | default:a b}", "", "");
        assert_eq!(inner, "C:\\b.exe a b");
    }

    // ---- modifier pipe: literal / robustness ----
    #[test]
    fn unrecognized_placeholder_is_literal() {
        let r = expand_instant_command("C:\\app.exe {foo} {query}", "Q", "");
        assert_eq!(r, "C:\\app.exe {foo} Q");
    }
    #[test]
    fn unclosed_brace_is_literal() {
        let r = expand_instant_command("C:\\app.exe {query", "Q", "");
        assert_eq!(r, "C:\\app.exe {query");
    }
    #[test]
    fn unknown_modifier_at_runtime_is_ignored() {
        // 保存時に拒否される想定だが、runtime に到達しても panic せず既知のみ適用
        let r = expand_instant_command("C:\\app.exe {query | bogus | upper}", "ab", "");
        assert_eq!(r, "C:\\app.exe AB");
    }

    // ---- modifier pipe: exec sink + invariants ----
    #[test]
    fn exec_modifier_trim_stays_one_arg() {
        let r = expand_exec_args("-s {query | trim}", "  report  ", "", no_env);
        assert_eq!(r, vec!["-s", "report"]);
    }
    #[test]
    fn exec_default_fills_empty_query() {
        let r = expand_exec_args("{query | default:.}", "", "", no_env);
        assert_eq!(r, vec!["."]);
    }
    #[test]
    fn exec_modifier_value_stays_one_arg() {
        // 修飾子適用後の空白入り値も1引数（argv 不可分）
        let r = expand_exec_args("-s {query | lower}", "Hello World", "", no_env);
        assert_eq!(r, vec!["-s", "hello world"]);
    }
    #[test]
    fn exec_modifier_value_is_not_env_expanded() {
        // {clip | upper} の値が %FOO% でも env 展開されない（大文字化のみ・置換は env の後）
        let env = |s: &str| s.replace("%FOO%", "EXPANDED");
        let r = expand_exec_args("{clip | upper}", "", "%foo%", env);
        assert_eq!(r, vec!["%FOO%"]);
    }
    #[test]
    fn exec_raw_is_noop() {
        // exec では raw は no-op（値不変・エラー無し）
        let r = expand_exec_args("{query | raw}", "A/B", "", no_env);
        assert_eq!(r, vec!["A/B"]);
    }

    // ---- collect_unknown_modifiers (save-time validation) ----
    #[test]
    fn unknown_modifiers_known_only_is_empty() {
        assert!(collect_unknown_modifiers("{query | lower | trim | raw}").is_empty());
        assert!(collect_unknown_modifiers("{clip | default:x | upper}").is_empty());
        assert!(collect_unknown_modifiers("no placeholders here").is_empty());
    }
    #[test]
    fn unknown_modifiers_collects_bogus() {
        let u = collect_unknown_modifiers("{query | lower | bogus}");
        assert_eq!(u, vec!["bogus"]);
    }
    #[test]
    fn unknown_modifiers_ignores_unrecognized_name() {
        // name が query/clip 以外（リテラル）の `{...}` は修飾子検査の対象外
        assert!(collect_unknown_modifiers("{foo | bogus}").is_empty());
    }
}
