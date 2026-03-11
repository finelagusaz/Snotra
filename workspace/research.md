# Issue #247 調査: CI Node.js 20 → 24

## Issue 要約

GitHub Actions が Node.js 20 ランナーを段階的に廃止する。2026年6月2日以降、
`actions/checkout@v4` と `actions/setup-node@v4` は Node.js 24 で強制実行される。
現時点では警告のみだが、期限前に Node.js 24 ネイティブ対応のアクションバージョンへ
アップグレードする必要がある。

## 関連ファイル

すべての変更対象は `.github/workflows/` 以下の YAML ファイル 5 本。

| ファイル | 使用アクション | 変更要否 |
|---|---|---|
| `ci.yml` | `actions/checkout@v4`, `actions/setup-node@v4` | 要変更 |
| `e2e.yml` | `actions/checkout@v4`, `actions/setup-node@v4` | 要変更 |
| `create-release.yml` | `actions/checkout@v4`, `softprops/action-gh-release@v2` | 要変更 |
| `release.yml` | `actions/checkout@v4`, `actions/setup-node@v4`, `Swatinem/rust-cache@v2`, `swatinem/rust-cache@v2` (大文字小文字揺れ)、`softprops/action-gh-release@v2` | 要変更 |
| `label-sync.yml` | `actions/checkout@v4`, `EndBug/label-sync@v2` | 要変更 |

## 現行バージョンと最新バージョン

| アクション | 現行 | 最新 | Node ランタイム |
|---|---|---|---|
| `actions/checkout` | `v4` | `v6.0.2` | v4=Node20, v5=Node20, v6=Node24 |
| `actions/setup-node` | `v4` | `v6.3.0` | v4=Node20, v5=Node20, v6=Node24 |
| `softprops/action-gh-release` | `v2` | `v2.5.0` | 変更不要（警告なし） |
| `EndBug/label-sync` | `v2` | `v2.3.3` | 変更不要（警告なし） |
| `dtolnay/rust-toolchain` | `@stable` | - | Rust ツールチェーン固有。Node ランタイム依存なし |
| `Swatinem/rust-cache` | `v2` | `v2.8.2` | 変更不要（警告なし）|

GitHub 公式アナウンス: https://github.blog/changelog/2025-09-19-deprecation-of-node-20-on-github-actions-runners/

## 既存パターン

- `ci.yml` の `frontend-check` ジョブは Node.js 22 で動作している（`node-version: 22`）。
  アクション自体の Node ランタイムとユーザーコードの Node.js バージョンは別の話。
- `e2e.yml` と `release.yml` も `node-version: 22` を指定している。
- `release.yml` に `Swatinem/rust-cache@v2` と `swatinem/rust-cache@v2`（先頭大文字小文字揺れ）が存在。
  同じアクションなので統一する（大文字小文字は GitHub Actions では区別されないが、統一が望ましい）。

## 技術的制約

- アクションバージョンのメジャーアップグレード（v4 → v6）は破壊的変更を含む可能性があるため
  リリースノートを確認する必要がある。
- `actions/checkout@v6` の主な変更点:
  - Node 24 ランタイムに移行
  - 機能面での主要な変更は後方互換性を維持（パラメータ API は変わらない）
- `actions/setup-node@v6` の主な変更点:
  - Node 24 ランタイムに移行
  - `node-version`, `cache` パラメータの API は変わらない

## 未解決の疑問

特になし。Issue の要求は一意に解釈できる。変更は機械的なバージョン番号の置換。
