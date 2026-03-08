# research.md — issue #193: 検索ウィンドウ・検索結果ウィンドウを統一する

## issue の要約

検索入力ウィンドウ（`main`）と検索結果ウィンドウ（`results`）を1つの WebView ウィンドウに統合する。
- **メモリ消費量削減**: WebView2 インスタンスの固定コスト（~5-10MB）を排除
- **ウィンドウ制御のシンプル化**: 2ウィンドウ間の IPC・位置同期・表示/非表示制御を排除

## 現在のアーキテクチャ

### 2ウィンドウ構成

| ウィンドウ | label | HTML | サイズ | 役割 |
|-----------|-------|------|--------|------|
| main | `main` | `main.html` | 600×52 | 検索入力バー |
| results | `results` | `results.html` | 600×(max_results×30+16) | 検索結果リスト |

- **ビルド**: Vite multi-page build → 2つの JS バンドル（main: 33KB, results: 7KB, theme: 16KB×2）
- **通信**: 3イベントパターン（`results-data-changed` / `results-selection-changed` / `results-visibility-changed`）
- **results の特殊処理**: `WS_EX_NOACTIVATE`（フォーカス非奪取）、main 直下 4px gap 配置、setup フェーズ事前生成

### ウィンドウ制御に関わるファイルと行数

| ファイル | 行数 | 役割 |
|---------|------|------|
| `ui/src/lib/resultsWindowController.ts` | 262 | results の位置・サイズ・表示制御 |
| `ui/src/components/ResultsWindow.tsx` | 256 | results の描画 + IPC リスナー |
| `ui/src/ResultsApp.tsx` | 16 | results エントリポイント |
| `ui/src/results.tsx` | 8 | results レンダラー |
| `ui/results.html` | — | results HTML |
| `ui/src/lib/searchEvents.ts` | ~30 | 3イベントのペイロード型 |

### MainApp.tsx のリスナー構成（現在）

1. `window-shown` → controller.updateMainVisible + resetForShow
2. `results-data-changed` → controller.handleDataChanged（位置/サイズ/表示制御）
3. `results-visibility-changed` → controller.handleVisibilityChanged
4. `result-clicked` → activateSelectedByIndex
5. `results-render-done` → perfMarkRenderDone
6. `result-double-clicked` → setSelected
7. `result-hovered` → setSelected
8. `platform-event` → ホットキー失敗通知
9. `onResized` → controller.updateMainSize
10. `onMoved` → controller.updateMainPosition + handleMainMoved
11. `onFocusChanged` → auto-hide-on-focus-lost
12. `visual-config-changed` → applyTheme
13. `max-results-changed` → controller.updateMaxResults
14. `hotkey-registration-failed` → setHotkeyFailureNotice

### search.ts の emit 関数

- `emitDataChanged(items, selected, requestId, reason)` — results 配列変更時
- `emitSelectionChanged(selected)` — 選択のみ変更時
- `emitVisibilityChanged(reason)` — 非表示時

### results → main IPC（クリック/ホバー）

- `notifyResultClicked(index)` → Rust → `result-clicked` イベント → main JS
- `notifyResultDoubleClicked(index)` → Rust → `result-double-clicked` → main JS
- `notifyResultHovered(index)` → Rust → `result-hovered` → main JS

### Rust 側の results 関連コード

- `main.rs:421` — `ensure_results_window()` 呼び出し
- `main.rs:466-474` — ホットキートグルで results も hide
- `commands/window.rs:18-36` — `ensure_results_window()`: WebviewWindowBuilder で生成
- `commands/window.rs:175-215` — `ensure_window` コマンド
- `commands/window.rs:247-265` — `set_window_no_activate`: WS_EX_NOACTIVATE 設定
- `commands/window.rs:228-245` — `is_main_foreground`: プロセス ID 比較（auto-hide 用）
- `config_watcher.rs:139` — ウィンドウ幅変更時に `["main", "results"]` 両方リサイズ

## 統合後のアーキテクチャ

### 単一ウィンドウ構成

- `main` ウィンドウ1つに入力バー + 結果リストを縦配置
- 高さを動的変更: 入力のみ(52px) ↔ 入力+結果(52 + max_results*30 + 16px)
- Vite single-page build に移行
- IPC イベント不要 → SolidJS シグナルの直接参照

### 排除されるもの

- results WebView2 インスタンス（~5-10MB メモリ節約）
- 3イベントパターン（emitDataChanged / emitSelectionChanged / emitVisibilityChanged）
- resultsWindowController 全体
- WS_EX_NOACTIVATE / set_window_no_activate
- ensure_results_window / ensure_window コマンド
- notifyResultClicked / notifyResultDoubleClicked / notifyResultHovered（Rust IPC 経由→直接 JS）
- is_main_foreground（プロセス ID 比較不要に）
- results-render-done イベント（同一ウィンドウ内で直接計測可能に）

## 関連コード一覧

### 削除対象

| ファイル | 理由 |
|---------|------|
| `ui/results.html` | results エントリポイント |
| `ui/src/results.tsx` | results レンダラー |
| `ui/src/ResultsApp.tsx` | results ルートコンポーネント |
| `ui/src/lib/resultsWindowController.ts` | results 位置/表示制御 |
| `ui/src/lib/searchEvents.ts` | 3イベントペイロード型（IPC 不要） |

### 大幅変更対象

| ファイル | 変更内容 |
|---------|---------|
| `ui/src/MainApp.tsx` | controller 削除、ResultsWindow を直接マウント、リスナー大幅削減 |
| `ui/src/components/ResultsWindow.tsx` | IPC リスナー → シグナル直接参照、onClick/onHover を直接ハンドル |
| `ui/src/components/SearchWindow.tsx` | 結果リストを同じ DOM に含める or 分離は維持 |
| `ui/src/stores/search.ts` | 3つの emit 関数を削除、代わりにウィンドウ高さ変更を通知 |
| `ui/src/styles/global.css` | 入力バー + 結果リストの縦配置レイアウト |
| `vite.config.ts` | multi-page → single-page |
| `src-tauri/tauri.conf.json` | main ウィンドウの初期高さ（52px 維持、動的変更は JS 側） |
| `src-tauri/src/main.rs` | ensure_results_window 削除、hotkey hide の results 参照削除 |
| `src-tauri/src/commands/window.rs` | ensure_results_window/ensure_window/set_window_no_activate/is_main_foreground 削除 |
| `src-tauri/src/commands/mod.rs` | 削除コマンドの登録解除 |
| `src-tauri/src/config_watcher.rs` | ウィンドウ幅変更で results を参照しない |
| `ui/src/lib/invoke.ts` | 削除コマンドのラッパー削除 |
| `ui/src/lib/commands.ts` | hideResultsWindow 削除、hideAllWindows 簡素化 |

### E2E / スモークテスト

| ファイル | 変更内容 |
|---------|---------|
| `scripts/smoke-startup.ps1` | `requiredLabels` から `results` を削除 |
| `scripts/bench-startup.ps1` | results_ms 計測を削除 |
| `e2e/tauri.slash.e2e.ts` | `waitForVisibleLabel(driver, "results")` → main 内の DOM 要素で判定に変更 |

### 変更不要

- `snotra-core/` — 純ロジック層
- `snotra-settings/` — 別プロセス
- `ui/src/stores/folder.ts`, `ui/src/stores/tool-selection.ts` — ウィンドウ非依存
- `ui/src/lib/lruIconCache.ts` — キャッシュロジック自体は変更不要
- `ui/src/lib/truncatePath.ts`, `ui/src/lib/folderNav.ts`, `ui/src/lib/pathQuery.ts`
- `ui/src/components/ResultRow.tsx` — 個別行の描画、変更不要

## 技術的制約

### 1. ウィンドウ高さの動的変更

統合後の最重要課題。main ウィンドウの高さを結果の表示/非表示に応じて変更する。

- `Tauri set_size()` は IPC 呼び出し（~2ms）。resizable: false でも API 経由は動作する
- **入力のみ**: 52px（現在の main 高さ）
- **入力+結果**: 52 + max_results×30 + 16 = 308px（デフォルト max_results=8 の場合）
- SPEC §3.5「ヒット数が最大表示件数未満でも高さは維持」→ 結果表示中は固定高
- Windows のリサイズは左上基準 → 結果は入力バーの下に展開される（意図通り）

### 2. ウィンドウリサイズのタイミング

- 結果が表示されるとき: set_size で高さ拡大
- Escape / launch / 非表示時: set_size で高さ縮小（52px に戻す）
- 高さ変更は非同期（await）→ 描画との同期が課題
- **方針**: 高さ変更は Rust IPC `set_size` で行い、JS 側は結果の有無に応じてリクエスト

### 3. decorations: false + resizable: false

- `decorations: false` の場合、OS のリサイズバーが表示されない → ユーザーによる手動リサイズは不可
- `resizable: false` でも `set_size()` API は動作する（Tauri が内部で Win32 `SetWindowPos` を呼ぶ）
- **確認済み**: 既存の `config_watcher.rs` が `resizable: false` の main/results に `set_size` を呼んでいる

### 4. auto_hide_on_focus_lost の簡素化

- 現在: results クリック時に `is_main_foreground()` でプロセス ID 比較し誤非表示を防止
- 統合後: 同一ウィンドウ内クリックはフォーカス喪失しない → プロセス ID 比較不要
- ドラッグ移動時の 100ms 猶予は維持（既存動作）

### 5. perf 計測の変更

- 現在: `results-render-done` イベントを results → main に送信し `perfMarkRenderDone` で計測
- 統合後: 同一コンテキスト内で `requestAnimationFrame` 後に直接 `perfMarkRenderDone` 呼び出し

### 6. AcceleratorKeyPressed ハンドラ

- main ウィンドウに登録済み。変更不要

### 7. E2E テスト

- `waitForVisibleLabel(driver, "results")` / `switchToLabel(driver, "results")` → main ウィンドウ内の DOM 要素（`.result-list-standalone` の子要素有無）で判定に変更
- `switchToLabel` 呼び出しが不要になり E2E テストも簡素化

## 未解決の疑問

1. **ウィンドウ高さ切り替えの UX**: 結果0件→非表示のとき、高さを52pxに即座に戻すとチラつく可能性。既存の `shouldShow` ロジックと同期する必要がある
   → 方針: 現在の `shouldShow` ロジック（`items.length > 0` のとき true）をそのまま踏襲し、shouldShow が変わったときのみ高さ変更

2. **`config_watcher.rs` のウィンドウ幅変更**: 現在 `["main", "results"]` 両方をリサイズしている。統合後は `"main"` のみ → 高さは現在のロジック（inner_size の logical height を維持）でそのまま動く

3. **初回表示時のウィンドウ高さ**: 起動時は visible: false + height: 52px。最初の検索結果表示時に高さを拡大。show_on_startup = true の場合は表示後に検索を行うため問題なし
