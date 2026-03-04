# Issue #130 実装計画: メモリ消費量削減

## 概要

Tauri を維持したまま、使用頻度の低いウィンドウ（`about`, `settings`）を起動時の事前生成から除外し、都度生成・都度破棄にする。加えて、スラッシュコマンドの候補案内表示を削除する。

Issue の考慮事項に基づく判断:

| 対象 | 方針 | 理由 |
|------|------|------|
| `about` ウィンドウ | **都度生成・都度破棄** | 静的コンテンツ、状態なし、極稀にしか使わない |
| `settings` ウィンドウ | **都度生成・都度破棄** | 稀にしか使わない。初回表示レイテンシは許容範囲（ユーザーが意図的に開く操作） |
| `results` ウィンドウ | **現状維持（常駐）** | 検索のたびに表示、レイテンシが重要 |
| スラッシュコマンド候補 | **削除** | Issue で「削ってよさそう」と明記 |

### メモリ削減効果の見込み

- WebView2 インスタンス 2 つ分の常駐メモリを削減（about + settings）
- 各 WebView2 インスタンスは通常 40-80 MB のメモリを使用するため、合計 80-160 MB の削減が見込める

## 変更ファイル一覧

| ファイル | 変更内容 |
|---------|---------|
| `src-tauri/src/main.rs` | about/settings の事前生成を削除、first-run パス調整 |
| `src-tauri/src/commands/window.rs` | about/settings の CloseRequested を破棄許可に変更、hide_settings→destroy、settings-shown emit 削除 |
| `ui/src/components/SettingsWindow.tsx` | settings-shown リスナー削除、onCloseRequested の preventDefault 削除 |
| `ui/src/stores/search.ts` | コマンド候補表示ロジック削除 |
| `ui/src/lib/commands.ts` | `filterCommands` 関数削除 |
| `ui/src/lib/commands.test.ts` | コマンド候補フィルタのテスト削除 |
| `scripts/smoke-startup.ps1` | requiredLabels から about/settings 除外 |
| `SPEC.md` | §6.1, §7.5, §8, §14.3 更新 |

計: 8 ファイル、4 フェーズ

---

## 実装順序

### Phase 1: about ウィンドウの都度生成・都度破棄

**`src-tauri/src/main.rs`**:
- `ensure_window_with_timing(&app_handle, "about", commands::ensure_about_window)?;` を削除

**`src-tauri/src/commands/window.rs`** の `ensure_about_window`:
- `CloseRequested` ハンドラで `api.prevent_close()` を呼ばない（= 閉じると破棄される）
- 破棄前に `main.set_always_on_top(true)` を復元（settings が表示中でない場合のみ）
- 既存の alwaysOnTop 復元ロジックはそのまま活用し、`prevent_close` のみ削除

### Phase 2: settings ウィンドウの都度生成・都度破棄

**`src-tauri/src/main.rs`**:
- `ensure_window_with_timing(&app_handle, "settings", commands::ensure_settings_window)?;` を削除
- `is_first_run` 時: `open_settings` コマンドを直接呼び出す形に変更

**`src-tauri/src/commands/window.rs`**:
- `ensure_settings_window`: `prevent_close` を削除、close 前に alwaysOnTop 復元 + first-run index build 開始
- `hide_settings`: hide → `close()`（破棄）に変更。alwaysOnTop 復元 + first-run index build は close で発火する CloseRequested ハンドラに委譲
- `open_settings`: `settings-shown` emit を削除

**`ui/src/components/SettingsWindow.tsx`**:
- `listen("settings-shown", ...)` リスナーを削除（都度生成で `onMount` が毎回発火するため不要）
- `onCloseRequested` の `event.preventDefault()` を削除
- コメント "The window is pre-created and hidden on close, so onMount only fires once." を更新

### Phase 3: スラッシュコマンド候補表示の削除

**`ui/src/stores/search.ts`**:
- `commandToResult` ヘルパー関数を削除
- `showCommandResults` 関数を削除
- `commandMatches` シグナルの用途を確認し、候補表示のみなら削除
- コマンドモード時: `findCommand` で完全一致 → 即実行、部分一致 → 何も表示しない（0件）
- `/` 入力時に results ウィンドウを表示しない

**`ui/src/lib/commands.ts`**:
- `filterCommands` 関数を削除

**`ui/src/lib/commands.test.ts`**（存在する場合）:
- `filterCommands` 関連テストを削除

### Phase 4: スモークテスト・SPEC.md 更新

**`scripts/smoke-startup.ps1`**:
- `$requiredLabels = @("results", "about", "settings")` → `@("results")` に変更
- サマリーから `about_ms`, `settings_ms` カラムを除外

**`SPEC.md`**:
- §6.1: 「すべての固定ウィンドウは起動時に一括で事前生成する」→ `results` のみ事前生成、`about`/`settings` は都度生成に変更
- §7.5: about/settings の「hide して再利用」→「破棄して都度生成」に変更
- §8: トレイアイコン表示条件から about/settings の事前生成完了を除外
- §14.3: ヘルプ表示セクションを削除（候補案内なし）

---

## 不変条件

1. `open_about` / `open_settings` は内部で `ensure` を呼ぶので、事前生成なしでも動作する
2. about/settings を閉じた後、`main` の `alwaysOnTop` が必ず復元される（他方が開いていない場合のみ）
3. first-run 時の settings 表示と index build 開始が正常に動作する
4. `/o`, `/a`, `/s`, `/q`, `/r` の即実行は引き続き動作する
5. hotkey-pressed リスナー内の settings 可視チェック: ウィンドウ不在時は `None` → `unwrap_or(false)` で正常動作

---

## テスト方針

### 自動テスト
- `npm test` — フロントエンドユニットテスト
- `npm run build` — フロントエンドビルド
- `cargo check -p snotra-core -p snotra` — Rust 型チェック

### 手動検証（Windows 必須）
- `npm run smoke:startup` — 起動スモーク
- `/a` → about 表示 → Escape → about 破棄（タスクマネージャでプロセス数確認）
- `/o` → settings 表示 → 閉じる → settings 破棄
- first-run 時に settings が正常に表示される
- ホットキー表示/非表示が正常動作

---

## セルフレビュー

### 1. 対称コードパス ✅
- `open_about` / close、`open_settings` / `hide_settings` の両方向を確認済み
- `alwaysOnTop` 復元の about/settings 相互チェックも確認済み（close handler 内で他方の可視状態を確認）

### 2. 影響範囲の網羅性 ✅
- `ensure_about_window` 呼び出し元: `main.rs`（削除）、`open_about`（維持）、`ensure_window` IPC（維持）
- `ensure_settings_window` 呼び出し元: `main.rs`（削除）、`open_settings`（維持）、`ensure_window` IPC（維持）
- `settings-shown` イベント: emit（`window.rs:150` 削除）、listen（`SettingsWindow.tsx:57` 削除）
- `filterCommands` 呼び出し元: `search.ts:80,144` のみ → 削除可能
- hotkey-pressed の settings 可視チェック: `None` → `false` → 変更不要
- `hide_settings` 呼び出し元: `SettingsWindow.tsx`（Escape + footer）→ 呼び出し自体は維持、内部動作が hide→close に変更

### 3. 境界条件 ✅
- settings 開中に about 閉じ → alwaysOnTop 復元しない（settings がまだ表示中）
- about 開中に settings 閉じ → alwaysOnTop 復元しない（about がまだ表示中）
- 両方閉じ → alwaysOnTop 復元する
- settings/about 不在時に hotkey-pressed → `None` → `false` → 正常

### 4. リソース管理 ✅
- `on_window_event` はウィンドウ破棄で自然に解放
- JS 側 `listen("settings-shown")` は削除
- `onCloseRequested` のリスナーも不要に

### 5. 既存パターンとの整合 ✅
- `ensure_*_window` の冪等パターンをそのまま活用（新規パターン導入なし）

### 6. YAGNI 違反 ✅
- about/settings の都度生成のみ。コード分割や追加の抽象化は導入しない
