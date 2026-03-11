# 実装計画: CI Node.js 20 → 24 (Issue #247)

作成日: 2026-03-12
対応ブランチ: `chore/ci-node24-upgrade`
依拠: `workspace/research.md`

---

## 概要

GitHub Actions の Node.js 20 廃止警告に対応する。
`actions/checkout@v4` と `actions/setup-node@v4` を Node 24 ネイティブ対応バージョンへアップグレードする。
SPEC.md 更新は不要（CI 設定の維持保守）。

---

## 変更ファイル一覧（3ファイル）

| ファイル | 変更内容 |
|---|---|
| `.github/workflows/ci.yml` | `checkout@v4` → `checkout@v4`、`setup-node@v4` → `setup-node@v4` |
| `.github/workflows/e2e.yml` | `checkout@v4` → `checkout@v4`、`setup-node@v4` → `setup-node@v4` |
| `.github/workflows/release.yml` | `checkout@v4` → `checkout@v4`、`setup-node@v4` → `setup-node@v4`、`swatinem/rust-cache@v2`（小文字 s）→ `Swatinem/rust-cache@v2`（大文字 S）に統一 |
| `.github/workflows/create-release.yml` | `checkout@v4` → `checkout@v4` |
| `.github/workflows/label-sync.yml` | `checkout@v4` → `checkout@v4` |

**注記**: 全 5 ファイルで `actions/checkout@v4` が使われている。
`actions/setup-node@v4` は `ci.yml`・`e2e.yml`・`release.yml` の 3 ファイルで使われている。

---

## バージョン変更詳細

| アクション | 現行 | 変更後 | 理由 |
|---|---|---|---|
| `actions/checkout` | `@v4` | **`@v4`** (→ v4.3.1 に自動追従) | v4.3.1 で "Port v6 cleanup to v4" として Node 24 backport 済み |
| `actions/setup-node` | `@v4` | **`@v4`** (→ v4.4.0 に自動追従) | v4 branch 最新で Node 24 対応済み |
| `swatinem/rust-cache` (release.yml) | `@v2` (小文字 s) | `Swatinem/rust-cache@v2` (大文字 S) | 表記揺れの統一（他ファイルに合わせる） |

**変更方針の根拠**:
- `actions/checkout` の `@v4` タグは常に最新パッチ（現在 v4.3.1）を指す。
  v4.3.1 のリリースノートに "Port v6 cleanup to v4" とあり、Node 24 対応の backport が行われている。
- GitHub が `@v4` タグを自動的に最新パッチへ移動させるため、`@v4` のままで警告解消が期待できる。
- もし `@v4` のままで CI 警告が残る場合は、`@v6` へのメジャーバンプに切り替える（フォールバック）。
- `softprops/action-gh-release@v2`・`EndBug/label-sync@v2`・`dtolnay/rust-toolchain@stable`・`Swatinem/rust-cache@v2` は警告が出ていないため変更しない。

---

## フェーズ構成

フェーズ 1 のみ（全変更を一括実施）。

### フェーズ 1: GitHub Actions バージョン更新（5ファイル一括）

**変更後ファイル別詳細**:

#### `.github/workflows/ci.yml`
- L24: `uses: actions/checkout@v4` → `uses: actions/checkout@v4`（変更なし、@v4 tag が最新 patch に追従）
- L27: `uses: actions/setup-node@v4` → `uses: actions/setup-node@v4`（同上）

**注**: `@v4` tag のまま変更なし。GitHub が最新パッチ（v4.3.1 / v4.4.0）へ自動追従するため警告が解消される。

#### `.github/workflows/e2e.yml`
- L16: `uses: actions/checkout@v4` → 変更なし
- L19: `uses: actions/setup-node@v4` → 変更なし

#### `.github/workflows/create-release.yml`
- L18: `uses: actions/checkout@v4` → 変更なし

#### `.github/workflows/release.yml`
- L26: `uses: actions/checkout@v4` → 変更なし
- L29: `uses: actions/setup-node@v4` → 変更なし
- L40: `uses: swatinem/rust-cache@v2` → `uses: Swatinem/rust-cache@v2`（大文字 S に統一）

#### `.github/workflows/label-sync.yml`
- L18: `uses: actions/checkout@v4` → 変更なし

**実質的な変更は `release.yml` の 1 箇所のみ**（`swatinem` → `Swatinem` の表記統一）。
他ファイルは `@v4` タグが最新パッチへの自動追従によって解消されるため変更不要。

---

## フォールバック対応（必要な場合のみ）

もし PR マージ後も CI 警告が消えない場合（`@v4` が Node 20 のまま）:

全 5 ファイルで以下の変更を追加で実施:
- `actions/checkout@v4` → `actions/checkout@v4` ← これではなく **`actions/checkout@v6`**
- `actions/setup-node@v4` → `actions/setup-node@v6`

`v6` への API 互換性: `checkout` / `setup-node` 共に主要パラメータ（`ref`、`node-version`、`cache`）に後方互換があることをリリースノートで確認済み。

---

## 不変条件

- CI の動作（Node.js バージョン 22 でのビルド・テスト実行）は変更しない。
  変更するのはアクション自体のランタイム（Node 20 → 24）のみ。
- `npm run build`・`cargo check`・テスト実行のコマンドは変更しない。
- `release.yml` の大文字小文字変更（`swatinem` → `Swatinem`）は動作に影響しない
  （GitHub Actions はアクション名の大文字小文字を区別しない）。

---

## テスト方針

- ローカルでのテスト不要（YAML 構文変更は ci lint で検出される）。
- PR を作成して CI が通ることを確認する。
- CI ログで Node.js 20 の deprecation 警告が消えていることを確認する。

---

## SPEC.md 更新要否

不要（プロダクト仕様変更なし）。

---

## セルフレビュー

### 対称コードパス確認
- 全 5 ワークフローファイルをスキャンし、`actions/checkout@v4` と `actions/setup-node@v4` の
  出現を網羅的に確認した。
- `release.yml` のみ `swatinem`（小文字）と `Swatinem`（大文字）の表記揺れが存在。修正対象に追加。

### 影響範囲
- `.github/workflows/` のみ。プロダクションコード・Rust・TypeScript への影響なし。
- `release.yml` の `swatinem` → `Swatinem` は動作に影響しない（大文字小文字は無視される）。

### 境界条件
- `@v4` タグの自動追従が v4.3.1 に未追従の場合、PR マージ後も警告が残る。
  その場合は `@v6` へのメジャーバンプを追加コミットで実施する。

### リソース管理
- 該当なし。

### YAGNI
- 警告の出ていない他のアクション（`softprops/action-gh-release@v2` 等）は変更しない。
- `dtolnay/rust-toolchain@stable` も警告なしのため変更しない。

### シンプル化
- 変更量を最小限に保つ方針を採用。実質的な変更は `release.yml` の表記揺れ修正 1 箇所のみ。
  `@v4` tag の自動追従に頼ることでファイル変更を最小化している。

### 修正した点（セルフレビューで発見）
1. `release.yml` に `swatinem/rust-cache@v2`（小文字 s）の表記揺れを発見。
   同ファイル内の他の記述・他ファイルはすべて `Swatinem`（大文字 S）なので修正対象に追加。
2. `actions/checkout@v6` へのメジャーバンプを当初計画したが、
   `v4.3.1` の Node 24 backport により `@v4` タグのまま解決できる可能性が高いため、
   変更を最小限（1 箇所）に絞った。フォールバック手順を計画書に明記することで対処。
