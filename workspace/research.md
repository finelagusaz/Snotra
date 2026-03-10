# Research: デフォルトインスタントコマンド追加 (Issue #222)

## issue の要約

`Config::default()` の `instant_commands` が空のため、初回ユーザーが機能の存在に気づけない。デフォルトで 1-2 個のコマンドを含めて発見性を上げる。

## 関連コード

| ファイル | 関連箇所 |
|---------|---------|
| `snotra-core/src/config.rs:640` | `Config::default()` — `instant_commands: Vec::new()` |
| `snotra-core/src/config.rs:170-171` | `#[serde(default)]` — 既存 config.toml にフィールドなければ空 Vec |
| `snotra-core/src/config.rs:727-774` | `Config::load()` — マイグレーション処理の流れ |
| `snotra-core/src/config.rs:27-30` | `InstantCommand` struct（name, command） |
| `snotra-core/src/instant.rs` | `expand_instant_command` — URL 判定 + `{query}/{clip}` 展開 |
| `SPEC.md:627-715` | §19 インスタントコマンド機能仕様 |

## 既存パターン

- `Config::default_scan_paths()` — デフォルトスキャンパスを環境依存で生成するパターンがある
- `migrate_additional_to_scan()` — 既存設定のマイグレーションパターンがある

## マイグレーション分析

- `#[serde(default)]` は `Vec::default()`（空 Vec）を使う。`Config::default()` を変えても既存ユーザーの `load()` には影響しない
- 既存ユーザーへの追加は `load()` にマイグレーション処理が必要。しかしユーザーが意図的に削除した場合の区別が不可能
- **結論**: 新規インストール時のみ（`Config::default()` 変更のみ）が最もシンプルかつ安全

## デフォルトコマンド候補

issue 提案通り:
- `g` → `https://www.google.com/search?q={query}` （Google 検索）
- `gh` → `https://github.com/search?q={query}` （GitHub 検索）

SPEC.md §19 の設定例にも `g` が既に記載されている。

## 技術的制約

- なし。純粋な設定値の追加のみ
