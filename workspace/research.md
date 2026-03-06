# research.md — issue #138: perf: opt-level クレート単位最適化

## issue の要約

現在ワークスペース全体で `opt-level = 3` を使用しているが、クレートごとに `opt-level = "s"` を適用して
バイナリサイズの削減と速度のトレードオフを計測し、最適な構成を決定する。

## 関連コード

### 現状の設定（`Cargo.toml` ルート）

```toml
[profile.release]
opt-level = 3
lto = true
strip = true
codegen-units = 1
panic = "abort"
```

`Cargo.toml` L8-13 に集約されている。3クレートすべてに同じ設定が適用されている。

### 計測対象バイナリ

- `target/release/snotra.exe` — src-tauri クレート（Tauri + Win32 統合）
- `target/release/snotra-settings.exe` — snotra-settings クレート（egui GUI、低頻度起動）

### ベンチマーク

- `snotra-core/src/search.rs` L1215 — `bench_fuzzy_search_scaling`, `bench_new_scaling`
  - `#[ignore]` 属性付き、`cargo test --release -p snotra-core bench_ -- --ignored --nocapture` で実行
  - folder.rs にも `bench_` 関数が存在する

## 技術的制約（重要）

### Cargo のクレート単位プロファイル上書き方法

issue 本文では「各クレートの `Cargo.toml` に `[profile.release]` を追記する形式」と記載されているが、
**これは Cargo ワークスペースでは無効**。

Cargo の仕様上、ワークスペースメンバーの `Cargo.toml` に書いた `[profile.*]` セクションは **無視される**。
プロファイル設定はワークスペースルートの `Cargo.toml` にのみ有効。

正しいクレート単位上書き構文（ルート `Cargo.toml` に `[profile.release.package.<name>]` セクションを追記）:

```toml
[profile.release.package.snotra-settings]
opt-level = "s"

[profile.release.package.snotra]
opt-level = "s"

[profile.release.package.snotra-core]
opt-level = "s"
```

参考: https://doc.rust-lang.org/cargo/reference/profiles.html#overrides

### LTO との相互作用

現在 `lto = true`（full LTO）が設定されている。LTO はリンク時にクレート境界を超えて最適化するため、
クレート単位の opt-level 差異の効果が部分的に緩和される可能性がある。
`[profile.release.package.*]` の opt-level オーバーライドはコード生成フェーズ（LLVM IR 生成段階）に
適用され、LTO フェーズと独立している。依存クレート（eframe など）への LTO の影響は限定的なため、
snotra-settings のサイズ削減効果は依然として期待できる。

### ベンチマーク実行環境

`cargo test --release -p snotra-core bench_ -- --ignored --nocapture` は Windows 必須
（`windows` クレート依存のため macOS/Linux 不可）。

### snotra-core のホットパス特性

- `search.rs` が中心的なホットパス（nucleo-matcher + rayon 並列スコアリング）
- `opt-level = "s"` によってループのアンローリングや SIMD ベクトル化が削減される可能性がある
- issue での判断基準: ベースライン比 10% 未満のレグレッションなら採用候補

## 既存パターン

- プロファイル設定はルート `Cargo.toml` の `[profile.release]` にすべて集約（既存パターン通り）
- ベンチマークは `snotra-core` 内の `#[ignore]` テストとして実装済み（search.rs, folder.rs に複数件）

## 未解決の疑問

- `lto = true` 環境下での `opt-level = "s"` の実際の効果量（計測して初めて判断可能）
- `opt-level = "s"` が snotra-core の自動ベクトル化に影響するか（ベンチで確認）
- Windows 環境でしか計測できないため、CI での自動計測は困難
