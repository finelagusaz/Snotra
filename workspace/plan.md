# Plan: デフォルトインスタントコマンド追加 (Issue #222)

## 変更ファイル一覧

| ファイル | 変更内容 |
|---------|---------|
| `snotra-core/src/config.rs` | `Config::default()` の `instant_commands` を 2 件に変更 |
| `snotra-core/src/config.rs` | 既存テストの `assert!(config.instant_commands.is_empty())` を更新 |

## 実装詳細

### Phase 1: Config::default() にデフォルトコマンド追加

`config.rs:640` の `instant_commands: Vec::new()` を以下に変更:

```rust
instant_commands: vec![
    InstantCommand {
        name: "g".to_string(),
        command: "https://www.google.com/search?q={query}".to_string(),
    },
    InstantCommand {
        name: "gh".to_string(),
        command: "https://github.com/search?q={query}".to_string(),
    },
],
```

### Phase 2: テスト更新

- `default_config_has_expected_values` (config.rs:2116): `assert!(config.instant_commands.is_empty())` → デフォルトコマンドの存在を検証するアサーションに変更
- `validate_default_config_returns_no_errors` が引き続き通ることを確認（name 重複チェックに引っかからないこと）

## 不変条件

- 既存ユーザーの config.toml には `[[instant_commands]]` がないため `serde(default)` で空 Vec になる → 既存ユーザーへの影響なし
- `validate()` の `InstantCommandDuplicateName` チェック: `g` と `gh` は一意 → OK
- SPEC.md の設定例と一致（`g` は §19 の例に既出）

## テスト方針

```bash
cargo check -p snotra-core -p snotra -p snotra-settings
cargo test -p snotra-core
cargo clippy -p snotra-core -- -D warnings
```

## SPEC.md 更新要否

不要。§19 の設定例に `g` が既に記載されており、デフォルト値の追加は SPEC の挙動変更ではない。

## セルフレビュー

1. **対称コードパス**: なし（設定値の追加のみ）
2. **影響範囲の網羅性**: `instant_commands` を参照する箇所は `validate()` と `instant.rs` の `filter_instant_commands` / `expand_instant_command`。いずれもデータ駆動なので値の追加に影響なし
3. **境界条件**: デフォルトコマンドの name が空文字でない → validate 通過。name が一意 → 重複チェック通過
4. **リソース管理**: なし
5. **既存パターンとの整合**: `default_scan_paths()` と同様、`default()` で初期値を設定するパターン
6. **YAGNI 違反**: なし。issue の要求範囲どおり
7. **シンプル化**: これ以上シンプルにできない変更
8. **破壊不変条件**: なし
