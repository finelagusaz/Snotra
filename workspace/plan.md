# plan.md — issue #138: perf: opt-level クレート単位最適化

## 概要

ルート `Cargo.toml` の `[profile.release.package.*]` を使い、クレートごとの opt-level を計測・決定する。
実装は「計測スクリプト整備 → 計測実施 → 判断 → Cargo.toml 反映」の順で進める。

## 変更ファイル一覧

| ファイル | 変更内容 |
|---------|---------|
| `Cargo.toml` | `[profile.release.package.*]` セクション追加（採用クレートのみ） |

変更ファイルは最大 1 件。計測フェーズでは Cargo.toml を一時的に変更しながら計測し、
最終的に採用構成のみを残す。

## 実装順序

### Phase 1: ベースライン取得（全クレート opt-level = 3）

現状の Cargo.toml を変更せずに計測する。

**バイナリサイズ**:
```bash
ls -lh target/release/snotra.exe
ls -lh target/release/snotra-settings.exe
```

**速度ベンチマーク**:
```bash
cargo test --release -p snotra-core bench_ -- --ignored --nocapture
```

結果を Issue にコメントとして記録する。

---

### Phase 2: snotra-settings に opt-level = "s" を適用

ルート `Cargo.toml` に以下を追記:

```toml
[profile.release.package.snotra-settings]
opt-level = "s"
```

**ビルド**:
```bash
cargo build --release -p snotra-settings
```

**計測**:
```bash
ls -lh target/release/snotra-settings.exe
```

**動作確認**: About タブ・設定画面の表示（手動）

**判断**: サイズ削減が確認できれば採用（速度は要件外・低頻度起動のため）

---

### Phase 3: src-tauri に opt-level = "s" を適用

ルート `Cargo.toml` に以下を追加:

```toml
[profile.release.package.snotra]
opt-level = "s"
```

**ビルド**:
```bash
cargo build --release -p snotra
```

**計測**:
```bash
ls -lh target/release/snotra.exe
```

**動作確認**: IPC・Win32 呼び出し・ホットキー・トレイ（手動）

**判断**: サイズ削減がある＆動作に問題なければ採用

---

### Phase 4: snotra-core に opt-level = "s" を適用

ルート `Cargo.toml` に以下を追加:

```toml
[profile.release.package.snotra-core]
opt-level = "s"
```

**ベンチマーク実行**:
```bash
cargo test --release -p snotra-core bench_ -- --ignored --nocapture
```

ベースライン（Phase 1）との差分をスループット比で計算する。
**判断基準**: レグレッション < 10% → 採用候補、>= 10% → 不採用

---

### Phase 5: 最終構成の決定・Cargo.toml 反映

Phase 2〜4 の計測結果を踏まえ、採用するクレートのみ `[profile.release.package.*]` を残し、
不採用のクレートのセクションは削除する。

計測結果と採用/不採用の根拠を Issue にコメントとして記載する。

## 不変条件

1. `snotra-core` の opt-level 変更は速度レグレッション < 10% の場合のみ採用する
2. 動作の正確性は変わらない（最適化レベルはコード生成のみに影響）
3. ルート `Cargo.toml` の他の設定（`lto`, `strip`, `codegen-units`, `panic`）は変更しない
4. issue 本文に記載のある誤った方法（各クレートの Cargo.toml に `[profile.release]` を書く方法）は使わない
   → ルート Cargo.toml の `[profile.release.package.*]` を使う

## テスト方針

自動テストは不要（プロファイル設定はコード変更なし）。

検証コマンド（変更後に実施）:
```bash
cargo check -p snotra-core -p snotra   # 型チェック
cargo test -p snotra-core               # 既存ユニットテスト
```

## SPEC.md 更新要否

**不要**。opt-level はビルド設定であり、ユーザー向け挙動に変化なし。

---

## セルフレビュー

### 1. 対称コードパス

対称ペアは存在しない。Cargo.toml のプロファイル設定追加のみ。

### 2. 影響範囲の網羅性

- 変更箇所: ルート `Cargo.toml` のみ
- 依存クレートの opt-level: `[profile.release.package.*]` は直接指定したクレートのみに適用。
  依存クレート（eframe, tauri 等）は `profile.release` のデフォルト（`opt-level = 3`）を引き続き使用

### 3. 境界条件

- ベンチマーク計測の再現性: Windows 環境の負荷状況により変動する可能性がある。複数回実行して平均を取る
- `lto = true` 環境: LTO によりクレート間最適化が発生するため、クレート単位の効果が緩和される可能性がある

### 4. リソース管理

プロファイル設定の変更のためリソース管理の考慮は不要。

### 5. 既存パターンとの整合

Cargo の `[profile.release.package.*]` は標準的なパターン。新規パターンなし。

### 6. YAGNI 違反

なし。最小限の変更（Cargo.toml に数行追加）で目的を達成する。

### 修正点

- issue 本文の誤った実装方法（各クレートの Cargo.toml に `[profile.release]`）を修正。
  正しくはルート Cargo.toml の `[profile.release.package.*]` を使用。
