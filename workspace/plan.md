# plan — issue #343: config 退避のトレイ通知 + finding C ガード

## ゴール（受け入れ条件）

1. **退避通知**: 起動時または実行中リロードで config が壊れて `.bak` 退避された（`RecoveredFromCorrupt`）とき、Win32 トレイバルーンで「設定の読み込みに失敗し既定値で動作中／`config.toml.bak` に退避」を Ja/En で通知する。
2. **finding C ガード**: 一時的 read 失敗（`ReadFailed`）で既定値表示中の `snotra-settings` が、ユーザー保存時に実 `config.toml` を既定値で上書きするのを防ぐ。
3. 正常時（`Loaded`）・first-run（`FirstRun`）はバルーンを出さず、保存も従来どおり。
4. `snotra-core` は UI 文字列を持たない（`LoadOutcome` はプレーン enum）。`load()` の公開シグネチャ不変（既存 7 呼び出し元に影響なし）。

## 設計

### 共通 seam: `LoadOutcome` + `load_reporting()`（snotra-core）

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadOutcome {
    Loaded,              // 正常 parse
    FirstRun,            // ファイル不在 → default 生成・保存
    RecoveredFromCorrupt,// parse 失敗 or 非 UTF-8 → .bak 退避 + default
    ReadFailed,          // permission/lock 等 → 据え置き + default
}
```

- `load_from_dir_reporting(dir) -> (Self, LoadOutcome)`: #338 の `load_from_dir` の各分岐に outcome を付す。
  - parse OK → `Loaded` / parse 失敗 → backup + `RecoveredFromCorrupt` / NotFound → save + `FirstRun` / InvalidData → backup + `RecoveredFromCorrupt` / その他 read 失敗 → `ReadFailed`。
- `load_from_dir(dir) -> Self = load_from_dir_reporting(dir).0`（内部呼び出し元は据え置き）。
- `pub fn load_reporting() -> (Self, LoadOutcome)`（config_dir None → `(default, FirstRun)`）。
- `pub fn load() -> Self = Self::load_reporting().0`（**既存シグネチャ維持**）。

### バルーン通知（src-tauri）

- `PlatformCommand::ShowConfigRecoveryBalloon`（unit variant）を追加。
- `process_commands`: arm 追加 → `if let Some(t) = tray.as_mut() { t.show_config_recovery_balloon(); }`（トレイ未生成時は no-op）。
- `tray.rs`: `show_config_recovery_balloon(&mut self)` を追加。`self.language` で Ja/En 文言を組み（メニュー文言と同じ inline）、`nid.szInfoTitle`/`szInfo`/`dwInfoFlags=NIIF_WARNING`、`nid.uFlags |= NIF_INFO`、`Shell_NotifyIconW(NIM_MODIFY, &nid)`。
  - **再発火防止（plan-review）**: `NIM_MODIFY` 直後に `nid.uFlags &= !NIF_INFO` を落とす（以後の `NIM_MODIFY` でバルーンが再発火しないように）。
  - `szInfo`(256 wchar)/`szInfoTitle`(64 wchar) は `encode_utf16().chain(once(0))` で NUL 終端し、長さを配列長で clamp（既存 `create` の `szTip` と同パターン）。
  - **実装時確認（plan-review）**: `windows` v0.62 `Win32_UI_Shell` で `NIF_INFO` / `NIIF_WARNING`(or `NIIF_INFO`) / `NOTIFYICONDATAW.{szInfo,szInfoTitle,dwInfoFlags}` の名称・可用性を確認（feature は導入済みでほぼ確実）。`PlatformCommand` に variant 追加時は `process_commands` の match arm を必ず追加（網羅性チェッカが検出）。
  - 文言例 Ja: タイトル「Snotra 設定を読み込めませんでした」本文「config.toml を解析できず既定値で起動しました。元の内容は config.toml.bak に退避しています。」/ En 対応。
- `main.rs`: 365 を `let (config, load_outcome) = Config::load_reporting();` に。**`SetTrayVisible(true)`（677）の後**に `if load_outcome == RecoveredFromCorrupt { bridge.send_command(ShowConfigRecoveryBalloon) }`。
- `config_watcher.rs`: 79 を `load_reporting()` に。`RecoveredFromCorrupt` なら（`language-changed` emit の後に）バルーンコマンド送信。

### finding C ガード（snotra-settings）

- `main.rs:20` を `load_reporting()` にし `(config, outcome)` を `app::run` に渡す。
- `app.rs`: `loaded_read_failed: bool`（`outcome == ReadFailed`）を保持。保存フローでガード。
- `i18n.rs`: 警告文言を追加。

**保存ガード UX（要求判断 — 下記「未確定」参照）**。推奨は**警告バナー＋確認チェックボックス**（plan-review が egui 即時モードで 2-click より安定と指摘）:
- `ReadFailed` 起動時は画面上部に警告バナーを常時表示（「設定を読み込めませんでした。既定値で表示中。保存すると既存設定が失われる可能性があります」）。
- Save は確認チェックボックス「既存設定が失われる可能性を承知で保存」をオンにするまで無効化。
- 状態: `loaded_read_failed: bool`（`outcome==ReadFailed`）+ `confirmed_despite_read_failed: bool`（チェックボックス）。**保存成功時に両方を `false` に戻す**（戻す経路を明示）。`save()`（app.rs:187 の `saved = config.clone()` 直後）でリセット。

## 未確定（要求判断・ユーザー確認）

**settings の `ReadFailed` 時の保存ガード方式**:
- (A) 警告＋明示確認で保存許可（**推奨**: 非破壊・ユーザーを詰まらせない・実装も中庸）
- (B) 保存を硬く拒否し「再読み込み」成功まで Save 無効（最も安全だが、読めない状態が続くとユーザーが設定変更を永続化できず詰む）

→ plan-review 後・実装着手前にユーザー確認する。

## 変更ファイル一覧

| ファイル | 変更 |
|---|---|
| `snotra-core/src/config.rs` | `LoadOutcome` enum、`load_from_dir_reporting`、`load_reporting`、`load`/`load_from_dir` をラッパー化。テスト拡張 |
| `snotra-core/src/lib.rs` | `LoadOutcome` の re-export（`pub use config::LoadOutcome;`、QoL・推奨） |
| `src-tauri/src/platform/mod.rs` | `PlatformCommand::ShowConfigRecoveryBalloon` 追加 + `process_commands` arm |
| `src-tauri/src/platform/tray.rs` | `show_config_recovery_balloon`（NIF_INFO）+ Ja/En 文言 |
| `src-tauri/src/main.rs` | `load_reporting()` 化 + 677 後にバルーン送信 |
| `src-tauri/src/config_watcher.rs` | `load_reporting()` 化 + リロード復旧でバルーン送信 |
| `snotra-settings/src/main.rs` | `load_reporting()` 化 + outcome を `app::run` へ |
| `snotra-settings/src/app.rs` | `ReadFailed` 保存ガード |
| `snotra-settings/src/i18n.rs` | 警告文言 |
| `SPEC.md` | 13.1/13.3 に通知・保存ガードを追記 |
| 各 `CLAUDE.md` | 新規ファイルは無い見込み（追記のみ）。発生時に同期 |

## 実装順序（フェーズ）

1. **Phase 1 — seam（snotra-core）**: `LoadOutcome` + `load_reporting` + `load_from_dir_reporting`。#338 の `load_from_dir_*` 統合テストを「outcome も assert」する形に拡張（TDD: Red→Green）。`load()` 不変を確認。
2. **Phase 2 — バルーン（src-tauri）**: `PlatformCommand` 追加 → `tray.show_config_recovery_balloon` → main.rs/watcher 配線。Win32 のためユニット不可、手動 smoke。
3. **Phase 3 — finding C ガード（snotra-settings）**: outcome 伝播 + 保存ガード（確定 UX で実装）。egui のためユニット不可、手動確認。
4. SPEC.md 同期。

## 不変条件

- `LoadOutcome` は UI 文字列を持たないプレーン enum。文言は src-tauri / snotra-settings 側。
- `Config::load()` 公開シグネチャ不変（既存 7 呼び出し元は無改修）。
- バルーンは**トレイ生成後**にのみ送る。トレイ未生成（`show_tray_icon=false`）では no-op（復旧時は default=tray ON のため起動時は出る）。`PlatformCommand` 受信側はトレイ `None` を安全に無視する。
- イベント順序: `language-changed` → バルーン（watcher 経路）。フロントが正しい言語、バルーンは確定言語で表示。
- settings: `saved` は Save 成功時のみ更新（既存不変）。ガードは「read 失敗起因の default」での上書きを止めるが、ユーザーが明示確認すれば保存可能（A 案）。
- **新規リソース・状態フラグの異常時挙動**: バルーンはファイア＆フォーゲット（保持リソースなし）。`loaded_read_failed` フラグは保存成功で解除（戻す経路を明示）。

## テスト方針

- **snotra-core（自動・主検証）**: `load_from_dir_reporting` の outcome を全分岐で assert（Loaded/FirstRun/RecoveredFromCorrupt×2（parse/非UTF-8）/ReadFailed）。#338 の既存統合テストを拡張。`cargo test -p snotra-core`。
- **src-tauri バルーン**: Win32 のためユニット不可（AGENTS.md）。手動 smoke:
  1. `%APPDATA%\Snotra\config.toml` を不正 TOML（または非 UTF-8 バイト）に書き換える
  2. `npm run tauri dev` で起動 → トレイバルーンが表示され、文言が言語設定どおり（Ja/En）であることを目視
  3. 正常 config では出ないこと、バルーン表示後に言語変更等で**再発火しない**ことを確認
- **snotra-settings ガード（finding C）**: egui のためユニット不可（snotra-settings CLAUDE.md「UI はテストしない」）。手動:
  1. 正常な `config.toml` を用意（既知の設定値）
  2. `config.toml` を読み取り拒否に変更（`icacls config.toml /deny %USERNAME%:R`）
  3. `cargo run -p snotra-settings` 起動 → 既定値表示＋警告バナーを確認、Save がチェックボックス未チェックで無効なことを確認
  4. 拒否を解除（`icacls config.toml /remove:d %USERNAME%`）→ 設定 → チェック → Save → **元の `config.toml` が既定値で上書きされていない**（または明示確認後のみ上書き）ことを確認
- 検証コマンドは `docs/build-commands.md` のカテゴリ A（snotra-core/src-tauri/snotra-settings は Rust なので A）と上記手動 smoke。手動 smoke の実施結果を実装報告に必須記載。

## SPEC.md 更新

- 13.1（設定データ）: 「読み込み失敗・復旧（parse/非UTF-8）はトレイ通知でユーザーに可視化する」。
- 13.3（バックアップ）or 13.1: 「読込失敗（権限/ロック）起因で既定値表示中の設定画面は、保存前に警告し既存設定の喪失を防ぐ」。

## セルフレビュー

`/plan-review`（Explore × 3: snotra-core / src-tauri / snotra-settings）を実行。**致命的な要対処ゼロ**、completeness 高・着手可。検出事項は計画に反映済み:

- **snotra-core**: 問題なし。`load()` 呼び出し元は実コード 3 箇所（main.rs:365 / config_watcher.rs:79 / snotra-settings/main.rs:20）のみで計画と一致、`backup.rs` は `from_toml_str` の別経路で対象外。outcome マッピング 5 分岐妥当、`config_dir None → FirstRun` 妥当、UI 文字列なし原則 OK、#338 統合テストに outcome assert を足せる構造。→ `lib.rs` re-export を推奨に格上げ。
- **src-tauri**: トレイ生成（`SetTrayVisible(true)`=main.rs:677 後）・トレイ無効時 None 無視・イベント順序（language-changed 先行）は問題なし。**反映**: バルーン再発火防止（`NIM_MODIFY` 直後に `NIF_INFO` を落とす）、UTF-16 NUL 終端 clamp、`NIF_INFO`/`NIIF_*` 可用性の実装時確認、match arm 追加（網羅性）。
- **snotra-settings**: draft/saved 整合・影響範囲（backup.rs 別経路）・シグネチャ変更は機械的で問題なし。**反映**: 保存ガード UX を「警告バナー＋確認チェックボックス」に確定（2-click は egui で不安定）、`confirmed_despite_read_failed` 状態追加＋保存成功で両フラグ解除、finding C の手動確認手順（icacls による read 拒否）を明記。

### セルフレビューチェックリスト（start-issue Step 5b）
1. **対称コードパス**: バルーンは一回限りの通知で `hide` 対の必要なし。load 結果は5分岐すべてに outcome 割当済み。
2. **影響範囲網羅**: `load()` 呼び出し元 grep 済み（3 箇所切替・他は不変）。`PlatformCommand` の match 全 arm 確認。
3. **境界条件**: トレイ未生成（show_tray_icon=false）→ None 無視。config_dir None → FirstRun。read 拒否解除前後の save。
4. **リソース管理**: バルーンはファイア＆フォーゲット（保持なし）。`loaded_read_failed`/`confirmed_despite_read_failed` は保存成功で戻す経路あり。
5. **既存パターン整合**: `PlatformCommand` 追加・トレイ Ja/En inline・`load_from_dir` seam・settings の status 機構を再利用。新規パターンなし。
6. **YAGNI**: トレイ無効時のフロントイベントフォールバックは持たない（復旧時 default=tray ON）。`LoadOutcome` は unit enum（bak パス等は持たない）。
7. **シンプル化**: 新スレッド/Mutex を増やさず既存 `PlatformCommand`・channel に乗せる。
8. **破壊不変条件**: 「壊れたら即アウト」= settings が read 失敗の既定値で実 config を上書き（finding C）。検知手段: チェックボックスガード + 手動 smoke（icacls）。`load()` 公開シグネチャ不変で既存経路を壊さない。

### 未確定（実装着手前にユーザー確認）
settings の `ReadFailed` 保存ガード方式: **(A) 警告バナー＋確認チェックボックスで保存許可（推奨）** / (B) 成功 load・明示リロードまで Save 硬く拒否。
