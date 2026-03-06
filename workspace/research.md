# research.md — issue #144: snotra-settings を CI の cargo check / clippy 対象に追加

## issue の要約

CI の `rust-check` ジョブが `snotra-core` と `snotra` のみを対象としており、`snotra-settings` のコンパイルエラーや clippy 警告がリリースまで検出されない。`-p snotra-settings` を追加する。

## 関連コード

- `.github/workflows/ci.yml` L63: `cargo check -p snotra-core -p snotra`
- `.github/workflows/ci.yml` L69: `cargo clippy -p snotra-core -p snotra -- -D warnings`

## 既存パターン

`-p` フラグで複数パッケージを指定する形式が既に使われている。同じパターンに追加するだけ。

## 技術的制約

- `snotra-settings` は `egui` + `windows` クレート依存 → `windows-latest` ランナー必須（既に満たしている）
- `cargo test` は対象外（ユニットテストなし）

## 未解決の疑問

なし。
