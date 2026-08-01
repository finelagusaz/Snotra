//! インスタントコマンド（プレフィックス起動する URL/コマンドテンプレート）の展開処理。
//!
//! 変数プレースホルダ `{name | mod | ...}`（name = query/clip/date/uuid、修飾子パイプ）の
//! 解析・展開と、シェル風の引数分割を提供する。各公開関数の契約は `///` を正とする
//! （`split_args` / `expand_instant_command` / `expand_exec_args` / `collect_unknown_modifiers`
//! / `filter_instant_commands`）。変数展開の中核（エンコードはシンクの責務・`{{X}}` エスケープ・
//! date/uuid の実行時解決）は private の `expand_template` / `walk_template` / `parse_placeholder`
//! / `format_date` の `///` を参照。

use chrono::{DateTime, Local};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use uuid::Uuid;

use crate::config::InstantCommand;

/// `{date}`（書式省略時）の既定 strftime 書式。
const DEFAULT_DATE_FMT: &str = "%Y-%m-%d";

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
#[derive(Debug, Clone, PartialEq, Eq)]
enum VarKind {
    Query,
    Clip,
    /// 実行時のローカル日時を strftime 書式で展開（引数は書式文字列）。
    Date(String),
    /// ランダムな UUID v4（出現ごとに新規生成）。
    Uuid,
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
/// 認識しない変数名なら `None`（呼び出し側がリテラル扱い）。
/// **runtime 適用と保存時バリデーションで共有する単一パーサ**。
///
/// 変数名はオプション引数を `:` で取れる（`date:%Y-%m-%d`）。name と arg は最初の
/// `:` で分割し、arg 内の `:` はリテラル（`date:%H:%M:%S` → 書式 `%H:%M:%S`）。
/// `query` / `clip` / `uuid` は引数を取らない——引数付き（例 `{query:x}`）は
/// 認識せずリテラルに戻し、従来挙動を保つ。
fn parse_placeholder(inner: &str) -> Option<Placeholder> {
    let mut segments = inner.split('|');
    let name_seg = segments.next()?.trim();
    let (name, arg) = match name_seg.split_once(':') {
        Some((n, a)) => (n.trim(), Some(a.trim())),
        None => (name_seg, None),
    };
    let var = match (name, arg) {
        ("query", None) => VarKind::Query,
        ("clip", None) => VarKind::Clip,
        ("uuid", None) => VarKind::Uuid,
        ("date", a) => VarKind::Date(a.unwrap_or(DEFAULT_DATE_FMT).to_string()),
        _ => return None,
    };
    let mods = segments.map(parse_modifier).collect();
    Some(Placeholder { var, mods })
}

/// ローカル日時を strftime 書式で整形する。**panic 安全**: 整形不能な書式は
/// すべて空文字列にフォールバックする——`Item::Error` を生む不正指定子（`%!` 等）
/// に加え、パースは成功するが整形時に `fmt::Error` を返す**パース専用指定子**
/// （`%#z` = `Fixed::TimezoneOffsetPermissive` 等）の両方を含む。
/// `to_string()` は `fmt::Error` を `.expect()` で unwrap して panic し、release の
/// `panic = "abort"` ではプロセス abort に化ける（#394）。代わりに `write!` で
/// Display の Err を panic させず値として伝播させ、Err なら空文字列を返す。
/// （`Item::Error` の事前走査はパース専用指定子を取りこぼすため不採用——code-reviewer 検出）
fn format_date(now: &DateTime<Local>, fmt: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    if write!(out, "{}", now.format(fmt)).is_err() {
        return String::new();
    }
    out
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
/// `{{...}}` はエスケープで literal な `{...}` を出力する（変数名と衝突する literal を
/// 書く唯一の手段。予約語が増えても opt-out が常に存在する）。
/// `expand_template`（適用）と `collect_unknown_modifiers`（検証）が共有する。
fn walk_template(template: &str, mut on_event: impl FnMut(TplEvent)) {
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        on_event(TplEvent::Literal(&rest[..open]));
        let after = &rest[open + 1..];
        // `{{X}}` エスケープ → literal `{X}`。中身は変数/修飾子として解釈しない。
        if let Some(inner_start) = after.strip_prefix('{') {
            if let Some(close) = inner_start.find("}}") {
                on_event(TplEvent::Literal("{"));
                on_event(TplEvent::Literal(&inner_start[..close]));
                on_event(TplEvent::Literal("}"));
                rest = &inner_start[close + 2..];
            } else {
                // 閉じ `}}` 不在 → 先頭 `{` だけ literal 化し2個目の `{` から再走査（best-effort）
                on_event(TplEvent::Literal("{"));
                rest = after;
            }
            continue;
        }
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
/// `{query|mods}` / `{clip|mods}` / `{date:書式|mods}` / `{uuid|mods}` を
/// 変数解決 → 修飾子チェーン →（`encode && !raw` のとき URL エンコード）で置換する。
/// **エンコードはシンク（種別）の責務**であり、修飾子は内容変換のみ。
/// `now` は呼び出し側が1回だけ捕捉して渡す——同一テンプレート内の複数 `{date}` が
/// 同じ実行時刻を反映するため（`{uuid}` は出現ごとに新規生成し意図的に異なる）。
fn expand_template(
    template: &str,
    query: &str,
    clipboard: &str,
    now: &DateTime<Local>,
    encode: bool,
) -> String {
    let mut out = String::with_capacity(template.len());
    walk_template(template, |ev| match ev {
        TplEvent::Literal(s) => out.push_str(s),
        TplEvent::Placeholder(ph) => {
            // date/uuid は実行時に解決する不純な源。query/clip は呼び出し側が渡す純値。
            let raw_value = match &ph.var {
                VarKind::Query => query.to_string(),
                VarKind::Clip => clipboard.to_string(),
                VarKind::Date(fmt) => format_date(now, fmt),
                VarKind::Uuid => Uuid::new_v4().to_string(),
            };
            let (value, raw) = apply_modifiers(&raw_value, &ph.mods);
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

/// Expand `{query}` / `{clip}` / `{date:書式}` / `{uuid}`（修飾子パイプ対応）in a URL/command template.
///
/// `http://` / `https://` で始まる場合は各プレースホルダ単位で URL エンコードする
/// （`raw` 修飾子が付く値は抑止）。それ以外は生のまま展開する。
pub fn expand_instant_command(command: &str, query: &str, clipboard: &str) -> String {
    let is_url = command.starts_with("http://") || command.starts_with("https://");
    expand_template(command, query, clipboard, &Local::now(), is_url)
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
    // 全トークン共通の実行時刻を1回だけ捕捉（`{date}` の一貫性）。
    let now = Local::now();
    split_args(args)
        .into_iter()
        .map(|tok| expand_template(&env_expand(&tok), query, clipboard, &now, false))
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
        let result = expand_instant_command("https://google.com/search?q={query}", "機械学習", "");
        assert!(result.contains("%E6%A9%9F%E6%A2%B0%E5%AD%A6%E7%BF%92"));
        assert!(!result.contains("機械学習"));
    }

    #[test]
    fn url_clip_is_encoded() {
        let result =
            expand_instant_command("https://translate.com/?text={clip}", "", "hello world");
        assert!(result.contains("hello%20world"));
        assert!(!result.contains("hello world"));
    }

    #[test]
    fn url_symbols_are_encoded() {
        let result = expand_instant_command("https://example.com/?q={query}", "a&b=c", "");
        assert!(result.contains("a%26b%3Dc"));
    }

    #[test]
    fn non_url_query_is_raw() {
        let result = expand_instant_command("C:\\editor.exe {query}", "hello world", "");
        assert_eq!(result, "C:\\editor.exe hello world");
    }

    #[test]
    fn non_url_clip_is_raw() {
        let result = expand_instant_command("C:\\editor.exe {clip}", "", "clipboard text");
        assert_eq!(result, "C:\\editor.exe clipboard text");
    }

    #[test]
    fn empty_query_expands_to_empty() {
        let result = expand_instant_command("https://google.com/search?q={query}", "", "");
        assert_eq!(result, "https://google.com/search?q=");
    }

    #[test]
    fn no_placeholders_returns_as_is() {
        let result = expand_instant_command("C:\\tools\\calc.exe", "ignored", "ignored");
        assert_eq!(result, "C:\\tools\\calc.exe");
    }

    #[test]
    fn both_placeholders_in_url() {
        let result =
            expand_instant_command("https://example.com/?q={query}&c={clip}", "test", "data");
        assert_eq!(result, "https://example.com/?q=test&c=data");
    }

    // ---- filter_instant_commands tests ----

    fn sample_commands() -> Vec<InstantCommand> {
        vec![
            InstantCommand {
                name: "g".to_string(),
                description: String::new(),
                action: InstantAction::Url {
                    url: "https://google.com?q={query}".into(),
                },
            },
            InstantCommand {
                name: "gm".to_string(),
                description: String::new(),
                action: InstantAction::Url {
                    url: "https://mail.google.com".into(),
                },
            },
            InstantCommand {
                name: "n".to_string(),
                description: String::new(),
                action: InstantAction::Url {
                    url: "notepad.exe".into(),
                },
            },
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
        let cmds = vec![InstantCommand {
            name: "Google".to_string(),
            description: String::new(),
            action: InstantAction::Url {
                url: "https://google.com".into(),
            },
        }];
        let result = filter_instant_commands(&cmds, "google");
        assert_eq!(result.len(), 1);
    }

    // ---- expand_exec_args tests ----
    fn no_env(s: &str) -> String {
        s.to_string()
    }

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
        assert_eq!(
            split_args(r#"--dir "My Documents""#),
            vec!["--dir", "My Documents"]
        );
    }
    #[test]
    fn split_args_unclosed_quote_consumes_to_end() {
        assert_eq!(
            split_args(r#"--dir "My Documents"#),
            vec!["--dir", "My Documents"]
        );
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
        assert_eq!(
            split_args("-s {query | trim}"),
            vec!["-s", "{query | trim}"]
        );
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
        // name が認識外（リテラル）の `{...}` は修飾子検査の対象外
        assert!(collect_unknown_modifiers("{foo | bogus}").is_empty());
    }
    #[test]
    fn unknown_modifiers_works_with_date_uuid() {
        // date/uuid 変数でも修飾子検査が機能する
        assert_eq!(
            collect_unknown_modifiers("{date:%Y | bogus}"),
            vec!["bogus"]
        );
        assert_eq!(collect_unknown_modifiers("{uuid | nope}"), vec!["nope"]);
        assert!(collect_unknown_modifiers("{date:%Y-%m-%d | upper}").is_empty());
        assert!(collect_unknown_modifiers("{uuid | upper}").is_empty());
    }

    // ---- date / uuid 変数 ----

    #[test]
    fn date_formats_with_strftime() {
        use chrono::TimeZone;
        // 固定ローカル時刻で決定論的に検証（now を注入できる純ヘルパー）
        let dt = Local.with_ymd_and_hms(2026, 6, 28, 13, 5, 9).unwrap();
        assert_eq!(format_date(&dt, "%Y-%m-%d"), "2026-06-28");
        assert_eq!(format_date(&dt, "%Y/%m/%d %H:%M:%S"), "2026/06/28 13:05:09");
    }

    #[test]
    fn date_invalid_format_is_empty_no_panic() {
        use chrono::TimeZone;
        let dt = Local.with_ymd_and_hms(2026, 6, 28, 13, 5, 9).unwrap();
        // 不正な書式指定子は空文字列にフォールバック（panic/abort しない、#394）
        assert_eq!(format_date(&dt, "%!"), "");
        assert_eq!(format_date(&dt, "%"), "");
        // パース専用指定子 %#z は Item::Error にならず整形時に fmt::Error を返す。
        // `write!` フォールバックがこれも空にすることを保証する（code-reviewer 検出の回帰防止）。
        assert_eq!(format_date(&dt, "%#z"), "");
    }

    #[test]
    fn date_default_format_when_no_arg() {
        // `{date}`（書式省略）は DEFAULT_DATE_FMT を採用する
        let ph = parse_placeholder("date").unwrap();
        assert_eq!(ph.var, VarKind::Date(DEFAULT_DATE_FMT.to_string()));
    }

    #[test]
    fn date_arg_keeps_second_colon() {
        // name と書式は最初の `:` で分割。書式中の `:` はリテラル
        let ph = parse_placeholder("date:%H:%M:%S").unwrap();
        assert_eq!(ph.var, VarKind::Date("%H:%M:%S".to_string()));
    }

    #[test]
    fn date_arg_is_trimmed() {
        let ph = parse_placeholder("date: %Y-%m-%d ").unwrap();
        assert_eq!(ph.var, VarKind::Date("%Y-%m-%d".to_string()));
    }

    #[test]
    fn url_date_is_expanded_and_encoded() {
        // URL 種別: `{date:%Y-%m-%d}` は展開され、ハイフンが URL エンコードされる（%2D）
        let r = expand_instant_command("https://x/?d={date:%Y-%m-%d}", "", "");
        assert!(r.contains("%2D"), "hyphen should be encoded: {r}");
        assert!(!r.contains("{date"), "placeholder should be gone: {r}");
    }

    #[test]
    fn exec_date_is_raw_not_encoded() {
        // exec 種別: 生展開（ハイフンそのまま）。空白入り書式も1引数を保つ
        let r = expand_exec_args("{date:%Y-%m-%d %H:%M}", "", "", no_env);
        assert_eq!(r.len(), 1, "braced placeholder stays one token: {r:?}");
        assert!(r[0].contains('-'), "hyphen kept raw: {r:?}");
        assert!(!r[0].contains("{date"), "placeholder should be gone: {r:?}");
    }

    #[test]
    fn date_chains_with_upper_modifier() {
        // `{date:%b | upper}` は月名略称（英語ロケール、ASCII 3文字）を大文字化。
        // expand 経路を実際に通して修飾子パイプとの合成を検証する。
        let r = expand_exec_args("{date:%b | upper}", "", "", no_env);
        assert_eq!(r.len(), 1);
        assert!(!r[0].contains("{date"), "expanded: {r:?}");
        assert!(
            !r[0].is_empty() && r[0].chars().all(|c| c.is_ascii_uppercase()),
            "upper-cased month abbrev: {r:?}"
        );
    }

    #[test]
    fn uuid_is_v4_lowercase_hyphenated() {
        let r = expand_exec_args("{uuid}", "", "", no_env);
        assert_eq!(r.len(), 1);
        let u = &r[0];
        let parts: Vec<&str> = u.split('-').collect();
        assert_eq!(
            parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
            vec![8, 4, 4, 4, 12],
            "8-4-4-4-12 hex layout: {u}"
        );
        assert_eq!(parts[2].chars().next(), Some('4'), "version nibble: {u}");
        assert!(
            u.chars().all(|c| c == '-' || c.is_ascii_hexdigit()),
            "hex only: {u}"
        );
        assert!(u.chars().all(|c| !c.is_ascii_uppercase()), "lowercase: {u}");
    }

    #[test]
    fn uuid_is_fresh_each_occurrence() {
        // 同一テンプレート内の2つの `{uuid}` は新規生成され異なる
        let r = expand_exec_args("{uuid} {uuid}", "", "", no_env);
        assert_eq!(r.len(), 2);
        assert_ne!(r[0], r[1], "each occurrence should be unique");
    }

    #[test]
    fn uuid_with_upper_modifier() {
        let r = expand_exec_args("{uuid | upper}", "", "", no_env);
        assert_eq!(r.len(), 1);
        assert!(
            r[0].chars().all(|c| c == '-' || !c.is_ascii_lowercase()),
            "upper applied: {:?}",
            r
        );
    }

    // ---- 後方互換: 引数付き query/clip/uuid はリテラル ----

    #[test]
    fn query_with_colon_arg_is_literal() {
        // `{query:x}` は変数として認識せずリテラルに戻す（従来挙動の保全）
        assert!(parse_placeholder("query:x").is_none());
        let r = expand_instant_command("C:/e.exe {query:x}", "", "");
        assert!(r.contains("{query:x}"), "should stay literal: {r}");
    }

    #[test]
    fn uuid_with_colon_arg_is_literal() {
        assert!(parse_placeholder("uuid:x").is_none());
    }

    // ---- {{...}} エスケープ（変数名と衝突する literal の opt-out） ----

    #[test]
    fn escape_yields_literal_for_reserved_word() {
        // {{date}} → literal {date}（展開しない）。URL でも braces は literal 経路で未エンコード
        let r = expand_instant_command("https://x/?d={{date}}", "", "");
        assert_eq!(r, "https://x/?d={date}");
    }

    #[test]
    fn escape_literal_query_and_clip() {
        let r = expand_instant_command("C:/t.exe {{query}} {{clip}}", "Q", "C");
        assert_eq!(r, "C:/t.exe {query} {clip}");
    }

    #[test]
    fn escape_mixed_with_real_placeholder() {
        // {{query}} は literal、{query} は展開（同一テンプレートで共存）
        let r = expand_instant_command("C:/t.exe {{query}}={query}", "hello", "");
        assert_eq!(r, "C:/t.exe {query}=hello");
    }

    #[test]
    fn escape_works_for_non_reserved_too() {
        // 一貫性: {{foo}} も literal {foo}（予約語に限らずエスケープは普遍）
        let r = expand_instant_command("C:/t.exe {{foo}}", "", "");
        assert_eq!(r, "C:/t.exe {foo}");
    }

    #[test]
    fn escape_in_exec_stays_one_token() {
        let r = expand_exec_args("{{date}}", "", "", no_env);
        assert_eq!(r, vec!["{date}"]);
    }

    #[test]
    fn escape_in_exec_with_inner_space_stays_one_token() {
        // 中に空白があっても brace 深度で1トークンを保つ（argv 不可分）
        let r = expand_exec_args("{{my note}}", "", "", no_env);
        assert_eq!(r, vec!["{my note}"]);
    }

    #[test]
    fn escape_content_is_not_modifier_validated() {
        // {{date | bogus}} は完全に literal。修飾子検査の対象外（保存時に弾かない）
        assert!(collect_unknown_modifiers("{{date | bogus}}").is_empty());
    }

    #[test]
    fn escape_asymmetric_is_modifier_validated() {
        // 非対称 {{date | bogus}（閉じ }} 不在）は best-effort で「{ + placeholder」と解釈され、
        // runtime/検証とも一貫して bogus を未知修飾子として扱う（malformed を保守的に弾く）。
        // 対称 {{…}}（literal・非検査）との非対称性を意図として固定する。
        assert_eq!(collect_unknown_modifiers("{{date | bogus}"), vec!["bogus"]);
    }

    #[test]
    fn escape_asymmetric_is_best_effort_no_panic() {
        // {{date}（閉じ }} 不在）は先頭 { を literal 化し残りを placeholder 扱い（best-effort）。
        // → "{" + 展開された日付。panic せず total。
        let r = expand_exec_args("{{date}", "", "", no_env);
        assert_eq!(r.len(), 1);
        assert!(r[0].starts_with('{'), "leading brace kept literal: {r:?}");
        assert!(
            !r[0].contains("{date}"),
            "second brace opened a placeholder: {r:?}"
        );
    }
}
