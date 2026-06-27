# インスタントコマンド exec 種別追加 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** インスタントコマンドに「実行ファイル + 引数」種別（`exec`）を追加し、`everything.exe -s {query}` のような引数つきコマンドが正しく起動するようにする。

**Architecture:** `InstantCommand` を `InstantAction`（`Url`/`Exec`/`Legacy`）の untagged enum を持つ形に拡張する。`Url` は現状の `ShellExecuteW` 経路（ゼロ回帰）、`Exec` は `Command::new(exe).args(...)` 経路。引数分割（`split_args`）と変数展開（`expand_vars`/`expand_exec_args`）は `snotra-core` の純関数に集約し、Win32（env 展開・spawn）は `src-tauri` に残す。旧 `command` は `Legacy` variant が拾い `apply_migrations` で `Url` へ無改変移行する。

**Tech Stack:** Rust（snotra-core / src-tauri "snotra" / snotra-settings）、serde + toml 1.x、Tauri v2、egui（設定 GUI）、SolidJS + TypeScript（ui）、Vitest。

設計書: `docs/superpowers/specs/2026-06-27-instant-command-exec-action-design.md`

## Global Constraints

- ブランチは `feat/instant-command-exec-action`（既存）。`main` へ直接コミットしない。`git` コマンドはチェーンしない（`add` と `commit` を分ける）。
- `snotra-core` は Win32 非依存・ユニットテスト必須。env 展開は関数注入（`env_expand: impl Fn(&str) -> String`）でテスト可能に保つ。
- Rust ツールチェーンは **≥ 1.77.2**（CVE-2024-24576 / BatBadBut 対策。`.bat`/`.cmd` 引数エスケープ）。
- `.rs` 編集後は PostToolUse フックが clippy（`snotra-core` 編集では core テストも）を自動実行する。明示コマンドは `docs/build-commands.md` を SSOT とする。
- **Rust タスク間の下流クレート compile-fail は意図的な「改名検出器」**（AGENTS.md）。各タスクは自クレートを green に戻す。全クレート green に戻るのは Task 6 完了時。
- Rust 検証: `cargo check -p snotra-core -p snotra -p snotra-settings` / `cargo clippy -p snotra-core -p snotra -p snotra-settings --all-targets -- -D warnings` / `cargo test -p snotra-core`
- フロント検証: `npm run typecheck` / `npm test` / `npm run build`
- 各コミットの末尾に `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>` trailer を付ける（`git commit -m "subject" -m "Co-Authored-By: …"` 等）。
- `windows` クレート（v0.62）の API（`ExpandEnvironmentStringsW` 等）はシグネチャ・feature フラグを使用前に確認する（`src-tauri/CLAUDE.md`）。
- exec 種別は **`.exe` 限定**（`.bat`/`.cmd`/`.lnk` 不可）。exe ピッカーフィルタは `["exe"]`。
- 変数展開順序: `split_args` → トークンごとに **env 展開 → `{query}`/`{clip}` 置換**。外部入力（query/clip）は env 展開しない。

---

### Task 1: `split_args` を snotra-core へ移設

**Files:**
- Modify: `snotra-core/src/instant.rs`（`split_args` を追加）
- Modify: `src-tauri/src/commands/launch.rs:154-201`（`split_args` 定義を削除し import に置換）

**Interfaces:**
- Produces: `pub fn snotra_core::instant::split_args(args: &str) -> Vec<String>`（クォート対応分割。`"…"` 内の空白を1トークンに保持。空クォート `""` はトークンを生成しない）

- [ ] **Step 1: 移設先テストを書く（失敗）**

`snotra-core/src/instant.rs` の `#[cfg(test)] mod tests` に追加（既存 launch.rs のテストを移設）:

```rust
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
```

- [ ] **Step 2: テスト失敗を確認**

Run: `cargo test -p snotra-core split_args`
Expected: FAIL（`split_args` 未定義 / コンパイルエラー）

- [ ] **Step 3: `split_args` を instant.rs に移設**

`snotra-core/src/instant.rs` のトップレベル（`expand_instant_command` の近く）に追加:

```rust
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
```

- [ ] **Step 4: launch.rs の定義を削除し import に置換**

`src-tauri/src/commands/launch.rs`:
- `fn split_args(...) { ... }`（154-201 付近の `split_args` 本体）を削除
- `build_launch_args` 内の `split_args(args)` 呼び出しを `snotra_core::instant::split_args(args)` に変更
- launch.rs の `#[cfg(test)] mod tests` にある `use super::split_args;` と `split_args_*` テスト群を削除（snotra-core へ移設済み）。`build_launch_args` のテストは残す

- [ ] **Step 5: テスト通過とビルドを確認**

Run: `cargo test -p snotra-core split_args`
Expected: PASS（5件）
Run: `cargo check -p snotra-core -p snotra -p snotra-settings`
Expected: 全 crate green（移設は機械的でシグネチャ変更なし）

- [ ] **Step 6: コミット**

```
git add snotra-core/src/instant.rs src-tauri/src/commands/launch.rs
git commit -m "refactor(core): split_args を snotra-core へ移設しオープナーと共有"
```

---

### Task 2: `expand_vars` と `expand_exec_args` を snotra-core に新設

**Files:**
- Modify: `snotra-core/src/instant.rs`

**Interfaces:**
- Produces:
  - `pub fn expand_vars(template: &str, query: &str, clipboard: &str) -> String`（`{query}`/`{clip}` を生置換）
  - `pub fn expand_exec_args(args: &str, query: &str, clipboard: &str, env_expand: impl Fn(&str) -> String) -> Vec<String>`（`split_args` → 各トークンに `env_expand` → `expand_vars`）
- Consumes: `split_args`（Task 1）

- [ ] **Step 1: 失敗するテストを書く**

`snotra-core/src/instant.rs` の `mod tests` に追加:

```rust
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
```

- [ ] **Step 2: テスト失敗を確認**

Run: `cargo test -p snotra-core exec_args`
Expected: FAIL（`expand_exec_args` 未定義）

- [ ] **Step 3: 実装**

`snotra-core/src/instant.rs`、`expand_instant_command` の直後に追加。既存 `expand_instant_command` も `expand_vars` を使う形へリファクタ:

```rust
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
```

`expand_instant_command` 本体を `expand_vars` 経由に書き換え（挙動不変）:

```rust
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
```

- [ ] **Step 4: テスト通過を確認**

Run: `cargo test -p snotra-core instant`
Expected: PASS（新規 `exec_args_*` 9件 + 既存 `expand_instant_command` テスト群が引き続き通る）

- [ ] **Step 5: 全 crate ビルドを確認**

Run: `cargo check -p snotra-core -p snotra -p snotra-settings`
Expected: 全 crate green（`expand_instant_command` のシグネチャ不変・新関数は追加のみ）

- [ ] **Step 6: コミット**

```
git add snotra-core/src/instant.rs
git commit -m "feat(core): expand_vars / expand_exec_args を追加（exec 引数構築の純関数）"
```

---

### Task 3: データモデル（`InstantAction` enum）と移行・serde ゲート

**Files:**
- Modify: `snotra-core/src/config.rs:48-54`（`InstantCommand` 構造体）、`apply_migrations`（888）、`Config::default()` instant_commands（758-769）、テスト群（1700 付近・2788/2810・2836）
- Modify: `snotra-core/src/instant.rs`（`mod tests` の `sample_commands` フィクスチャ 123-128, 165）

**Interfaces:**
- Produces:
  - `pub enum InstantAction { Url { url: String }, Exec { exe: String, args: String }, Legacy { command: String } }`（`#[serde(untagged)]`、`Exec.args` は `#[serde(default)]`）
  - `InstantCommand { name: String, description: String, action: InstantAction }`（`action` は `#[serde(flatten)]`）
  - 移行後 `Legacy` は存在しない（`apply_migrations` が `Url` 化）

- [ ] **Step 1: serde ゲートと移行テストを書く（失敗）**

`snotra-core/src/config.rs` の `mod tests` に追加。**T2/T1/T3/T15/T17 が最優先ゲート**:

```rust
    // ---- InstantAction serde gate (release gate: 失敗は全設定リセットを意味する) ----
    fn cfg_with_instant(cmds: Vec<InstantCommand>) -> Config {
        Config { instant_commands: cmds, ..Default::default() }
    }

    #[test] // T2: legacy 行が deserialize できる（最重要・データ損失検出器）
    fn instant_legacy_command_deserializes() {
        let legacy = cfg_with_instant(vec![InstantCommand {
            name: "g".into(), description: String::new(),
            action: InstantAction::Legacy { command: "https://x/?q={query}".into() },
        }]);
        let s = toml::to_string(&legacy).expect("serialize legacy");
        // Legacy は `command = "..."` 形（=旧オンディスク形式）で出力される
        assert!(s.contains("command ="));
        let parsed: Config = toml::from_str(&s).expect("legacy deserialize must succeed");
        assert!(matches!(parsed.instant_commands[0].action, InstantAction::Legacy { .. }));
    }

    #[test] // T15 + T17: legacy → Url 移行（自動分割しない）・冪等
    fn instant_legacy_migrates_to_url_idempotently() {
        let mut cfg = cfg_with_instant(vec![InstantCommand {
            name: "ev".into(), description: String::new(),
            action: InstantAction::Legacy { command: "C:\\tools\\editor.exe".into() },
        }]);
        assert!(cfg.apply_migrations());
        assert_eq!(cfg.instant_commands[0].action,
            InstantAction::Url { url: "C:\\tools\\editor.exe".into() }); // Exec にしない
        let changed_again = cfg.apply_migrations();
        assert!(!changed_again || cfg.instant_commands[0].action
            == InstantAction::Url { url: "C:\\tools\\editor.exe".into() }); // 冪等
    }

    #[test] // T1: Config 全体の serialize 往復で変種が保たれる
    fn instant_exec_roundtrip_preserves_variant() {
        let cfg = cfg_with_instant(vec![InstantCommand {
            name: "ev".into(), description: "Everything".into(),
            action: InstantAction::Exec { exe: "everything.exe".into(), args: "-s {query}".into() },
        }]);
        let s = toml::to_string_pretty(&cfg).expect("serialize");
        let parsed: Config = toml::from_str(&s).expect("deserialize");
        assert_eq!(parsed.instant_commands[0].action,
            InstantAction::Exec { exe: "everything.exe".into(), args: "-s {query}".into() });
    }

    #[test] // T3: url と exe を両方書いた行は Url 先勝ち（untagged 宣言順）
    fn instant_both_url_and_exe_prefers_url() {
        let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"
            [appearance]
            window_width = 600
            [paths]
            additional = []
            [[instant_commands]]
            name = "x"
            url = "https://x"
            exe = "y.exe"
        "#;
        let cfg: Config = toml::from_str(toml_str).expect("parse");
        assert!(matches!(cfg.instant_commands[0].action, InstantAction::Url { .. }));
    }

    #[test] // T4: Exec で args 省略 → 空文字
    fn instant_exec_args_defaults_empty() {
        let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"
            [appearance]
            window_width = 600
            [paths]
            additional = []
            [[instant_commands]]
            name = "n"
            exe = "notepad.exe"
        "#;
        let cfg: Config = toml::from_str(toml_str).expect("parse");
        assert_eq!(cfg.instant_commands[0].action,
            InstantAction::Exec { exe: "notepad.exe".into(), args: String::new() });
    }
```

- [ ] **Step 2: テスト失敗を確認**

Run: `cargo test -p snotra-core instant_legacy_command_deserializes`
Expected: FAIL（`InstantAction` 未定義・`InstantCommand.action` 未定義でコンパイルエラー）

> **ゲート判断**: Step 4 で `instant_legacy_command_deserializes`（T2）が通らない場合、`flatten`+`untagged`+`toml` の非互換が判明したことを意味する。設計 §3.1 の退避B（フラット `Option<String>` 群 url/exe/args/command + `validate` 排他）へ切り替える。**先に進まない**。

- [ ] **Step 3: 構造体と enum を変更**

`snotra-core/src/config.rs:48-54` を置換:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstantCommand {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(flatten)]
    pub action: InstantAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InstantAction {
    Url { url: String },
    Exec {
        exe: String,
        #[serde(default)]
        args: String,
    },
    Legacy { command: String },
}
```

- [ ] **Step 4: 移行ロジックを追加**

`apply_migrations`（config.rs:888）の `changed` を返す前（952 の `changed` 直前）に追加:

```rust
        // 旧 `command` 単一文字列 → `Url` へ無改変移行（自動分割しない＝ゼロ回帰）
        for cmd in &mut self.instant_commands {
            if let InstantAction::Legacy { command } = &mut cmd.action {
                let url = std::mem::take(command);
                cmd.action = InstantAction::Url { url };
                changed = true;
            }
        }
```

`Config::default()` の instant_commands（758-769）を置換:

```rust
            instant_commands: vec![
                InstantCommand {
                    name: "g".to_string(),
                    description: "Google 検索".to_string(),
                    action: InstantAction::Url {
                        url: "https://www.google.com/search?q={query}".to_string(),
                    },
                },
                InstantCommand {
                    name: "gh".to_string(),
                    description: "GitHub 検索".to_string(),
                    action: InstantAction::Url {
                        url: "https://github.com/search?q={query}".to_string(),
                    },
                },
            ],
```

- [ ] **Step 5: 既存フィクスチャを `action` 形へ更新**

`snotra-core/src/config.rs`:
- `validate_instant_command_duplicate_name`（2790-2800）と `validate_instant_command_unique_names_ok`（2813-2823）の各 `InstantCommand { name, command: "...", description }` を `InstantCommand { name, description, action: InstantAction::Url { url: "...".into() } }` へ
- 既存 `instant_command_round_trip_toml`（2836）を、移行後の変種も検証する形へ書き換え（転用で不変条件を孤立させない）:

```rust
    #[test]
    fn instant_command_round_trip_toml() {
        let toml_str = r#"
            [hotkey]
            modifier = "Alt"
            key = "Q"
            [appearance]
            window_width = 600
            [paths]
            additional = []
            [[instant_commands]]
            name = "g"
            command = "https://google.com/search?q={query}"
            [[instant_commands]]
            name = "memo"
            command = "C:\\tools\\editor.exe"
        "#;
        let mut config: Config = toml::from_str(toml_str).expect("parse");
        config.apply_migrations();
        assert_eq!(config.instant_commands.len(), 2);
        assert_eq!(config.instant_commands[0].name, "g");
        assert!(matches!(config.instant_commands[0].action, InstantAction::Url { .. }));
        assert!(matches!(config.instant_commands[1].action, InstantAction::Url { .. }));
    }
```

`snotra-core/src/instant.rs` の `sample_commands`（123-128）と `filter_case_insensitive`（165）の `InstantCommand { name, command: "...", description }` リテラルを `action: InstantAction::Url { url: "...".into() }` 形へ（`use crate::config::InstantAction;` を test mod に追加）。

- [ ] **Step 6: テスト通過を確認**

Run: `cargo test -p snotra-core`
Expected: PASS（serde ゲート T1-T4/T15/T17 含む全 core テスト。**T2 が green であること＝データ損失なし**）

- [ ] **Step 7: snotra-core を確認（下流は意図的に red）**

Run: `cargo check -p snotra-core`
Expected: green
Run: `cargo check -p snotra`
Expected: **FAIL（意図的）**。`.command` 参照（src-tauri/commands/instant.rs）と `get_instant_commands` の戻り型で改名検出。Task 4 で修復する

- [ ] **Step 8: コミット**

```
git add snotra-core/src/config.rs snotra-core/src/instant.rs
git commit -m "feat(core): InstantCommand を Url/Exec/Legacy 種別へ拡張・legacy 移行"
```

---

### Task 4: src-tauri の実行ディスパッチ・exec 起動・IPC DTO

**Files:**
- Modify: `src-tauri/src/commands/instant.rs`（`execute_instant_command` ディスパッチ・`get_instant_commands` DTO 化）
- Modify: `src-tauri/src/commands/launch.rs`（`launch_exec_core` 追加・`InstantCommandDto` 追加・`expand_env` 追加）
- Modify: `src-tauri/Cargo.toml`（`windows` クレートに `Win32_System_Environment` feature 追加）

**Interfaces:**
- Consumes: `InstantAction`（Task 3）、`expand_vars`/`expand_exec_args`（Task 2）
- Produces:
  - `pub struct InstantCommandDto { name: String, description: String, display: String }`（camelCase serialize）
  - `get_instant_commands(...) -> Result<Vec<InstantCommandDto>, String>`
  - `fn launch_exec_core(exe: &str, args: &str, query: &str, clipboard: &str) -> LaunchResult`

- [ ] **Step 1: DTO 表示文字列の純テストを書く（失敗）**

`src-tauri/src/commands/launch.rs` の `#[cfg(test)] mod tests` に追加:

```rust
    use super::InstantCommandDto;
    use snotra_core::config::{InstantCommand, InstantAction};

    #[test]
    fn instant_dto_display_url() {
        let c = InstantCommand { name: "g".into(), description: "d".into(),
            action: InstantAction::Url { url: "https://x".into() } };
        assert_eq!(InstantCommandDto::from(&c).display, "https://x");
    }
    #[test]
    fn instant_dto_display_exec_with_args() {
        let c = InstantCommand { name: "ev".into(), description: String::new(),
            action: InstantAction::Exec { exe: "everything.exe".into(), args: "-s {query}".into() } };
        assert_eq!(InstantCommandDto::from(&c).display, "everything.exe -s {query}");
    }
    #[test]
    fn instant_dto_display_exec_no_args_has_no_trailing_space() {
        let c = InstantCommand { name: "n".into(), description: String::new(),
            action: InstantAction::Exec { exe: "notepad.exe".into(), args: String::new() } };
        assert_eq!(InstantCommandDto::from(&c).display, "notepad.exe");
    }
```

- [ ] **Step 2: テスト失敗を確認**

Run: `cargo test -p snotra instant_dto`
Expected: FAIL（`InstantCommandDto` 未定義）

- [ ] **Step 3: DTO と `launch_exec_core`・`expand_env` を実装**

`src-tauri/src/commands/launch.rs` に追加:

```rust
use snotra_core::config::{InstantCommand, InstantAction};
use snotra_core::instant::{expand_vars, expand_exec_args};
use std::process::Stdio;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// フロントへ返すインスタントコマンド情報（種別の内部構造を隠す）
#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InstantCommandDto {
    pub name: String,
    pub description: String,
    pub display: String,
}

impl From<&InstantCommand> for InstantCommandDto {
    fn from(c: &InstantCommand) -> Self {
        let display = match &c.action {
            InstantAction::Url { url } => url.clone(),
            InstantAction::Exec { exe, args } => {
                if args.is_empty() { exe.clone() } else { format!("{exe} {args}") }
            }
            InstantAction::Legacy { command } => command.clone(),
        };
        Self { name: c.name.clone(), description: c.description.clone(), display }
    }
}

/// 環境変数 `%VAR%` を展開する（Win32 ExpandEnvironmentStringsW）。非 Windows は素通し。
pub(crate) fn expand_env(input: &str) -> String {
    #[cfg(windows)]
    {
        use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
        use windows::core::HSTRING;
        let src = HSTRING::from(input);
        unsafe {
            let needed = ExpandEnvironmentStringsW(&src, None);
            if needed == 0 { return input.to_string(); }
            let mut buf = vec![0u16; needed as usize];
            let written = ExpandEnvironmentStringsW(&src, Some(&mut buf));
            if written == 0 { return input.to_string(); }
            // 末尾 NUL を除いて UTF-16 → String
            let len = (written as usize).saturating_sub(1);
            String::from_utf16_lossy(&buf[..len])
        }
    }
    #[cfg(not(windows))]
    { input.to_string() }
}

/// exec 種別の起動。COM 不要（CreateProcessW 直叩き）。
pub(crate) fn launch_exec_core(exe: &str, args: &str, query: &str, clipboard: &str) -> LaunchResult {
    let exe_expanded = expand_vars(&expand_env(exe), query, clipboard);
    let arg_tokens = expand_exec_args(args, query, clipboard, expand_env);

    let mut cmd = std::process::Command::new(&exe_expanded);
    cmd.args(&arg_tokens);
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    match cmd.spawn() {
        Ok(_) => LaunchResult::ok(0),
        Err(e) => LaunchResult::failed(-1, format!("spawn_failed: {e}")),
    }
}
```

`src-tauri/Cargo.toml` の `windows` 依存の `features` に `"Win32_System_Environment"` を追加。

> **API 確認**: `ExpandEnvironmentStringsW` の windows v0.62 でのシグネチャ（引数の `PCWSTR`/`Option<&mut [u16]>`・戻り値 `u32`＝NUL 含む TCHAR 数）を実装前に確認する。差異があれば呼び出しを調整する。

- [ ] **Step 4: DTO テスト通過を確認**

Run: `cargo test -p snotra instant_dto`
Expected: PASS（3件）

- [ ] **Step 5: `execute_instant_command` と `get_instant_commands` をディスパッチ化**

`src-tauri/src/commands/instant.rs`:

`get_instant_commands` の戻り型を DTO へ:

```rust
#[tauri::command]
pub fn get_instant_commands(
    prefix_input: String,
    app: tauri::AppHandle,
) -> Result<Vec<super::launch::InstantCommandDto>, String> {
    let state = app.state::<AppState>();
    let engine = state.engine.lock().unwrap();
    let commands = &engine.config().instant_commands;
    Ok(filter_instant_commands(commands, &prefix_input)
        .into_iter()
        .map(super::launch::InstantCommandDto::from)
        .collect())
}
```

`execute_instant_command` を `action` でディスパッチ（テンプレート取得を `action.clone()` に変更）:

```rust
    let action = {
        let state = app.state::<AppState>();
        let engine = state.engine.lock().unwrap();
        engine.config().instant_commands.iter()
            .find(|c| c.name == name)
            .ok_or_else(|| format!("instant command not found: {name}"))?
            .action.clone()
    };

    let clipboard = arboard::Clipboard::new()
        .and_then(|mut cb| cb.get_text())
        .unwrap_or_default();

    let join = tauri::async_runtime::spawn_blocking(move || {
        use snotra_core::config::InstantAction;
        match action {
            InstantAction::Url { url } => {
                let expanded = expand_instant_command(&url, &query, &clipboard);
                super::launch::launch_item_core(&expanded)
            }
            InstantAction::Exec { exe, args } => {
                super::launch::launch_exec_core(&exe, &args, &query, &clipboard)
            }
            // load 後は移行済みで到達しないが、防御的に Url 扱い
            InstantAction::Legacy { command } => {
                let expanded = expand_instant_command(&command, &query, &clipboard);
                super::launch::launch_item_core(&expanded)
            }
        }
    });
    let result = match timeout(Duration::from_millis(LAUNCH_TIMEOUT_MS), join).await {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => LaunchResult::failed(-1, format!("launch_worker_join_error: {e}")),
        Err(_) => LaunchResult::timeout(LAUNCH_TIMEOUT_MS),
    };
    Ok(result)
```

import の調整: `use snotra_core::instant::{expand_instant_command, filter_instant_commands};`（`expand_instant_command` は Url/Legacy で使用）。不要になった import を整理。

- [ ] **Step 6: src-tauri ビルドと lint を確認**

Run: `cargo check -p snotra-core -p snotra`
Expected: green（snotra-settings は Task 6 まで意図的に red）
Run: `cargo clippy -p snotra --all-targets -- -D warnings`
Expected: green

- [ ] **Step 7: コミット**

```
git add src-tauri/src/commands/instant.rs src-tauri/src/commands/launch.rs src-tauri/Cargo.toml
git commit -m "feat(tauri): instant exec 起動ディスパッチと InstantCommandDto を追加"
```

---

### Task 5: フロントエンドの表示（DTO 消費）

**Files:**
- Modify: `ui/src/lib/types.ts:47-51`（`InstantCommand` 型）
- Modify: `ui/src/stores/search.ts:301`（副表示）
- Modify: `ui/src/stores/search.test.ts:75-76`（フィクスチャ）

**Interfaces:**
- Consumes: `InstantCommandDto`（Task 4、`{ name, description, display }`）

- [ ] **Step 1: フィクスチャと型を更新（テスト先行）**

`ui/src/lib/types.ts`:

```ts
export interface InstantCommand {
  name: string;
  display: string;
  description: string;
}
```

`ui/src/stores/search.test.ts:75-76` の `command:` プロパティを `display:` に変更（例: `CMD_GOOGLE` / `CMD_CLIP` フィクスチャ）。値はそのまま（テンプレート文字列）。

- [ ] **Step 2: 失敗を確認**

Run: `npm run typecheck`
Expected: FAIL（`search.ts:301` が `cmd.command` を参照しており `command` が型に無い）

- [ ] **Step 3: 副表示を `display` に変更**

`ui/src/stores/search.ts:301`:

```ts
        description: cmd.description || cmd.display,
```

（**注意**: `search.ts:334/336` の `cmd.command` は別型 `SlashCommand`。触らない）

- [ ] **Step 4: typecheck とフロントテスト通過を確認**

Run: `npm run typecheck`
Expected: PASS
Run: `npm test`
Expected: PASS（instant command 関連テスト含む）

- [ ] **Step 5: コミット**

```
git add ui/src/lib/types.ts ui/src/stores/search.ts ui/src/stores/search.test.ts
git commit -m "feat(ui): instant コマンド副表示を DTO の display に切替"
```

---

### Task 6: 設定UI（種別ラジオ・exec フィールド・移行ヒント・i18n）

**Files:**
- Modify: `snotra-settings/src/tabs/instant.rs`（`ModalState`・モーダル・リスト行・移行ヒント）
- Modify: `snotra-settings/src/i18n.rs`（新規文字列 Ja/En）

**Interfaces:**
- Consumes: `InstantAction`（Task 3）、`expand_exec_args`/`expand_instant_command`（Task 2）

- [ ] **Step 1: i18n 文字列を追加**

`snotra-settings/src/i18n.rs` の `Tr` に新規メソッドを追加（Ja/En 両方）。既存 `label_instant_command`/`hint_instant_command` は URL 用に転用:

```rust
    pub fn label_instant_kind(&self) -> &'static str {
        match self.0 { Language::Ja => "種別", Language::En => "Kind" }
    }
    pub fn radio_instant_url(&self) -> &'static str {
        match self.0 { Language::Ja => "URL / 既定アプリで開く", Language::En => "URL / open with default app" }
    }
    pub fn radio_instant_program(&self) -> &'static str {
        match self.0 { Language::Ja => "プログラム（exe + 引数）", Language::En => "Program (exe + args)" }
    }
    pub fn label_instant_exe(&self) -> &'static str {
        match self.0 { Language::Ja => "実行ファイル (.exe)", Language::En => "Executable (.exe)" }
    }
    pub fn label_instant_args(&self) -> &'static str {
        match self.0 { Language::Ja => "引数", Language::En => "Arguments" }
    }
    pub fn hint_instant_program(&self) -> &'static str {
        match self.0 {
            Language::Ja => ".exe のみ。スクリプトはインタプリタを実行ファイルに指定。{query} / {clip} と %VAR% が使えます",
            Language::En => ".exe only. For scripts, set the interpreter as the executable. {query} / {clip} and %VAR% are supported",
        }
    }
    pub fn hint_instant_migrate(&self) -> &'static str {
        match self.0 {
            Language::Ja => "引数つきの可能性があります。プログラム種別へ作り直すと正しく起動します",
            Language::En => "This may contain arguments. Recreate it as a Program command to launch correctly",
        }
    }
```

- [ ] **Step 2: ModalState を種別対応へ拡張**

`snotra-settings/src/tabs/instant.rs` の `ModalState`（11-19）と open/save を改修:

```rust
#[derive(Default, PartialEq, Clone, Copy)]
enum EditKind { #[default] Url, Program }

#[derive(Default)]
struct ModalState {
    open: bool,
    mode: ModalMode,
    editing_index: Option<usize>,
    edit_name: String,
    edit_description: String,
    edit_kind: EditKind,
    edit_url: String,
    edit_exe: String,
    edit_args: String,
}
```

`open_create` はフィールドを全クリア + `edit_kind = EditKind::Url`。`open_edit` / `open_create_from` は `action` から復元:

```rust
    fn load_action(&mut self, cmd: &InstantCommand) {
        self.edit_name = cmd.name.clone();
        self.edit_description = cmd.description.clone();
        self.edit_url.clear();
        self.edit_exe.clear();
        self.edit_args.clear();
        match &cmd.action {
            InstantAction::Url { url } => { self.edit_kind = EditKind::Url; self.edit_url = url.clone(); }
            InstantAction::Exec { exe, args } => {
                self.edit_kind = EditKind::Program;
                self.edit_exe = exe.clone();
                self.edit_args = args.clone();
            }
            InstantAction::Legacy { command } => { self.edit_kind = EditKind::Url; self.edit_url = command.clone(); }
        }
    }
```

`save_instant_command`（302-316）を種別に応じた `action` 構築へ:

```rust
fn save_instant_command(config: &mut Config, modal: &ModalState) {
    let action = match modal.edit_kind {
        EditKind::Url => InstantAction::Url { url: modal.edit_url.clone() },
        EditKind::Program => InstantAction::Exec {
            exe: modal.edit_exe.clone(),
            args: modal.edit_args.clone(),
        },
    };
    let cmd = InstantCommand {
        name: modal.edit_name.clone(),
        description: modal.edit_description.clone(),
        action,
    };
    if let Some(idx) = modal.editing_index {
        if idx < config.instant_commands.len() {
            config.instant_commands[idx] = cmd;
        }
    } else {
        config.instant_commands.push(cmd);
    }
}
```

（`use snotra_core::config::InstantAction;` を追加）

- [ ] **Step 3: モーダルの種別ラジオ + 条件フィールド + プレビュー**

`show_modal`（241-263 の Command フィールド部分）を置換:

```rust
        // Kind
        ui.label(tr.label_instant_kind());
        ui.horizontal(|ui| {
            ui.radio_value(&mut state.modal.edit_kind, EditKind::Url, tr.radio_instant_url());
            ui.radio_value(&mut state.modal.edit_kind, EditKind::Program, tr.radio_instant_program());
        });
        ui.add_space(4.0);

        match state.modal.edit_kind {
            EditKind::Url => {
                ui.label(tr.label_instant_command()); // URL 用に転用
                ui.text_edit_singleline(&mut state.modal.edit_url);
                ui.label(egui::RichText::new(tr.hint_instant_command()).small().color(crate::app::TEXT_SECONDARY));
                if !state.modal.edit_url.is_empty() {
                    let preview = snotra_core::instant::expand_instant_command(&state.modal.edit_url, "example", "(clipboard)");
                    ui.add_space(4.0);
                    ui.label(tr.label_instant_preview());
                    ui.label(egui::RichText::new(&preview).small().color(crate::app::TEXT_SECONDARY));
                }
            }
            EditKind::Program => {
                ui.label(tr.label_instant_exe());
                ui.text_edit_singleline(&mut state.modal.edit_exe);
                // exe ファイルピッカーは ["exe"] 限定（opener.rs の ExePicker を流用する場合もフィルタを exe のみに）
                ui.label(tr.label_instant_args());
                ui.text_edit_singleline(&mut state.modal.edit_args);
                ui.label(egui::RichText::new(tr.hint_instant_program()).small().color(crate::app::TEXT_SECONDARY));
                if !state.modal.edit_exe.is_empty() {
                    // 実行時と同じ純関数でプレビュー（乖離防止）。env は展開せず素通し（プレビュー用）
                    let tokens = snotra_core::instant::expand_exec_args(&state.modal.edit_args, "example", "(clipboard)", |s| s.to_string());
                    let preview = format!("{} {}", state.modal.edit_exe, tokens.join(" "));
                    ui.add_space(4.0);
                    ui.label(tr.label_instant_preview());
                    ui.label(egui::RichText::new(preview.trim()).small().color(crate::app::TEXT_SECONDARY));
                }
            }
        }
```

- [ ] **Step 4: リスト行表示と移行ヒント**

リスト行（128-146）の `&cmd.command` 参照を `action` 由来の表示へ。`Url` 種別で http(s) 非開始かつ空白を含む行に移行ヒントを表示:

```rust
                ui.vertical(|ui| {
                    ui.label(if cmd.name.is_empty() { tr.label_no_name() } else { &cmd.name });
                    if !cmd.description.is_empty() {
                        ui.label(egui::RichText::new(&cmd.description).small().color(crate::app::TEXT_SECONDARY));
                    }
                    let (display, suspect_legacy) = match &cmd.action {
                        InstantAction::Url { url } =>
                            (url.clone(), !url.starts_with("http://") && !url.starts_with("https://") && url.contains(' ')),
                        InstantAction::Exec { exe, args } =>
                            (if args.is_empty() { exe.clone() } else { format!("{exe} {args}") }, false),
                        InstantAction::Legacy { command } => (command.clone(), command.contains(' ')),
                    };
                    ui.label(egui::RichText::new(&display).small().color(crate::app::TEXT_SECONDARY));
                    if suspect_legacy {
                        ui.label(egui::RichText::new(format!("⚠ {}", tr.hint_instant_migrate()))
                            .small().color(egui::Color32::from_rgb(196, 120, 28)));
                    }
                });
```

- [ ] **Step 5: 全 crate ビルドと lint（ここで workspace green に戻る）**

Run: `cargo check -p snotra-core -p snotra -p snotra-settings`
Expected: **全 crate green**
Run: `cargo clippy -p snotra-core -p snotra -p snotra-settings --all-targets -- -D warnings`
Expected: green
Run: `cargo run -p snotra-settings`（任意・目視）
Expected: Instant タブで URL/プログラム切替・プレビュー・移行ヒントが表示される（カテゴリ D 目視）

- [ ] **Step 6: コミット**

```
git add snotra-settings/src/tabs/instant.rs snotra-settings/src/i18n.rs
git commit -m "feat(settings): instant コマンドに種別ラジオ・exec フィールド・移行ヒントを追加"
```

---

### Task 7: SPEC.md §19 とモジュールドキュメント同期

**Files:**
- Modify: `SPEC.md`（§19.2 / §19.4 / §19.5 / §19.6 / §19.8）
- Modify: `snotra-core/CLAUDE.md`（instant.rs モジュール記述・split_args 移設）

**Interfaces:** なし（ドキュメントのみ）

- [ ] **Step 1: SPEC.md §19 を更新**

- §19.2 設定構造: TOML 例を2形態（`url` / `exe`+`args`）に更新。`command =` の旧例は「旧形式（自動で `url` へ移行）」と注記
- §19.4 変数展開: exec の env 展開（`%VAR%`）と「split → env展開 → 置換」順序、外部入力は env 展開しないことを追記
- §19.5 マッチングと結果表示: 副表示は `display`（url または `exe args`）
- §19.6 実行フロー: 種別ディスパッチ（`Url`→`ShellExecuteW` / `Exec`→`Command::new` + `spawn_blocking`/`timeout`/`CREATE_NO_WINDOW`、spawn 失敗は `LaunchResult::failed`）
- §19.8 設定画面: フィールドを kind ラジオ + url / exe / args へ
- §19 内の子セクション番号と後続セクション番号のずれを確認

- [ ] **Step 2: snotra-core/CLAUDE.md を更新**

`instant.rs` のモジュール記述に `expand_vars` / `expand_exec_args` / `split_args`（launch.rs から移設）を追記。

- [ ] **Step 3: ドキュメント整合を目視確認**

Run: `git diff --stat`
Expected: `SPEC.md` と `snotra-core/CLAUDE.md` のみ変更

- [ ] **Step 4: コミット**

```
git add SPEC.md snotra-core/CLAUDE.md
git commit -m "docs(spec): instant exec 種別の仕様と core モジュール記述を同期"
```

---

## 最終検証（全タスク完了後）

- [ ] `cargo check -p snotra-core -p snotra -p snotra-settings` green
- [ ] `cargo clippy -p snotra-core -p snotra -p snotra-settings --all-targets -- -D warnings` green
- [ ] `cargo test -p snotra-core` green（serde ゲート T1-T4 / 移行 T15/T17 / exec 引数 T7-T12,T16 / DTO は src-tauri）
- [ ] `cargo test -p snotra instant` green（DTO display）
- [ ] `npm run typecheck` / `npm test` / `npm run build` green
- [ ] 手動 smoke（Windows）: `config.toml` に `[[instant_commands]] name="ev" exe="C:\\Users\\Eoh\\scoop\\shims\\everything.exe" args="-s {query}"` を書き、`@ev foo` で Everything が `-s foo` で起動することを確認（カテゴリ D）
- [ ] 旧 `command =` 形式の既存 config が起動時に消えず `url` へ移行されることを確認（T2 の実機裏取り）
- [ ] E2E は instant 未参照のため `e2e` ラベル不要（カテゴリ C 非該当。ウィンドウ生成・ホットキー・スラッシュコマンドに触れていない）
