# plan: issue #193 — 検索ウィンドウ・検索結果ウィンドウを統一する

## 設計方針

main ウィンドウ1つに検索入力バーと結果リストを縦配置し、results WebView を廃止する。
ウィンドウ高さは結果表示時に動的拡大、非表示時に 52px に縮小する。
3イベントパターンを廃止し、SolidJS シグナルの直接参照に切り替える。

## 変更ファイル一覧

### Phase 1: フロントエンド統合（UI 層の変更）

| ファイル | 変更内容 |
|---------|---------|
| `vite.config.ts` | multi-page → single-page（results エントリポイント削除） |
| `ui/src/MainApp.tsx` | controller 削除。ResultsWindow を直接レンダリング。リスナー大幅削減（results 系 5 個削除）。ウィンドウ高さ変更ロジック追加 |
| `ui/src/components/SearchWindow.tsx` | 結果リストを同じ DOM 構造に含める（SearchWindow 直下に ResultsSection を配置） |
| `ui/src/components/ResultsWindow.tsx` | → `ResultsSection.tsx` にリネーム。IPC リスナー → props/シグナル直接参照。onClick/onDoubleClick/onHover をコールバック props 化 |
| `ui/src/stores/search.ts` | 3つの emit 関数（emitDataChanged/emitSelectionChanged/emitVisibilityChanged）を削除。代替: `shouldShowResults` シグナルを新設し、MainApp がリアクティブに高さ変更 |
| `ui/src/styles/global.css` | 入力バー + 結果リストの縦配置。`.results-window` を `.results-section` に変更。全体の `height: 100%` を維持 |
| `ui/src/lib/commands.ts` | `hideResultsWindow` 削除。`hideAllWindows` を簡素化（main hide のみ） |
| `ui/src/lib/invoke.ts` | 不要になったコマンドラッパー削除: `ensureWindow`, `setWindowNoActivate`, `notifyResultClicked`, `notifyResultDoubleClicked`, `notifyResultHovered`, `isMainForeground` |
| `ui/src/lib/perf.ts` | `results-render-done` イベント待ち → 同一コンテキスト内の rAF コールバックに変更 |

### Phase 1 で削除するファイル

| ファイル | 理由 |
|---------|------|
| `ui/results.html` | results エントリポイント不要 |
| `ui/src/results.tsx` | results レンダラー不要 |
| `ui/src/ResultsApp.tsx` | results ルートコンポーネント不要 |
| `ui/src/lib/resultsWindowController.ts` | results 位置/表示制御不要 |
| `ui/src/lib/searchEvents.ts` | 3イベントペイロード型不要 |

### Phase 2: バックエンド整理（Rust 層の変更）

| ファイル | 変更内容 |
|---------|---------|
| `src-tauri/src/main.rs` | `ensure_results_window()` 呼び出し削除。ホットキー hide で results 参照削除。invoke_handler から削除コマンドを除外 |
| `src-tauri/src/commands/window.rs` | `ensure_results_window` 関数削除。`ensure_window` コマンド削除。`set_window_no_activate` コマンド削除。`is_main_foreground` コマンド削除 |
| `src-tauri/src/commands/mod.rs` | 削除コマンドの re-export を除外 |
| `src-tauri/src/config_watcher.rs` | ウィンドウ幅変更: `["main", "results"]` → `["main"]` のみ |
| `src-tauri/tauri.conf.json` | main ウィンドウの初期高さは 52px 維持（変更なし） |

### Phase 2 で削除するコマンド（Rust）

- `ensure_window` (`#[tauri::command]`)
- `set_window_no_activate` (`#[tauri::command]`)
- `is_main_foreground` (`#[tauri::command]`)
- `notify_result_clicked` (`#[tauri::command]`)
- `notify_result_double_clicked` (`#[tauri::command]`)
- `notify_result_hovered` (`#[tauri::command]`)

### Phase 3: テスト・スクリプト更新

| ファイル | 変更内容 |
|---------|---------|
| `scripts/smoke-startup.ps1` | `requiredLabels` から `"results"` 削除。`results_ms` 参照削除 |
| `scripts/bench-startup.ps1` | `results_ms` 計測削除 |
| `e2e/tauri.slash.e2e.ts` | `waitForVisibleLabel(driver, "results")` / `switchToLabel(driver, "results")` → main 内 DOM 要素で判定 |
| `ui/src/lib/commands.test.ts` | hideResultsWindow 関連のテスト更新 |
| `src-tauri/src/capabilities/*.json` | results ウィンドウの権限エントリがあれば削除 |

### Phase 4: ドキュメント更新

| ファイル | 変更内容 |
|---------|---------|
| `SPEC.md` | §7.5 サブウィンドウ生成 → results 事前生成の記述削除。§3.7 結果表示同期契約 → IPC 3イベント → シグナル直接参照に変更 |
| `CLAUDE.md` | 横断的な実装パターンの「3イベント分割」記述を更新 |
| `ui/CLAUDE.md` | マルチウィンドウ通信の不変条件 → 削除/簡素化。モジュール構成更新 |
| `src-tauri/CLAUDE.md` | モジュール構成更新（削除コマンド反映） |

## 実装順序

### Phase 1: フロントエンド統合

**1a. search.ts のシグナル公開**
- `shouldShowResults` を computed シグナル（`results().length > 0 && !indexing()`）として公開
- 3つの emit 関数を削除（emitDataChanged / emitSelectionChanged / emitVisibilityChanged）
- `eventGeneration` カウンタを削除
- `searchEvents.ts` のインポートを削除

**1b. ResultsWindow.tsx → ResultsSection.tsx にリファクタ**
- IPC リスナー（`results-data-changed` / `results-selection-changed` / `results-visibility-changed`）を削除
- 代わりに search.ts のシグナル（`results`, `selected`, `shouldShowResults`）を直接 import
- `onClick` / `onDoubleClick` / `onMouseEnter` をコールバック props に変更（親が search store の関数を渡す）
- bootstrap payload からの `show_icons` / `max_results` 取得は維持
- `iconCache.revokeAll()` は `shouldShowResults` が false になったときの createEffect で実行

**1c. MainApp.tsx の簡素化**
- `createResultsWindowController` 関連コード全削除
- results 系リスナー 5 個削除（`results-data-changed`, `results-visibility-changed`, `result-clicked`, `results-render-done`, `result-double-clicked`, `result-hovered`）
- ウィンドウ高さ変更ロジック追加:
  ```ts
  createEffect(() => {
    const show = shouldShowResults();
    const height = show ? 52 + maxResults() * 30 + 16 : 52;
    void win.setSize(new LogicalSize(cachedWidth, height));
  });
  ```
- `hideMainAndResults` → `hideMain`（results hide 不要）
- `auto_hide_on_focus_lost` の `is_main_foreground` チェック削除（同一ウィンドウなので不要）

**1d. SearchWindow.tsx + CSS 統合**
- SearchWindow に `<ResultsSection />` を含める
- CSS: `.search-bar` と `.results-section` の縦配置

**1e. vite.config.ts**
- multi-page input から `results.html` を削除

**1f. commands.ts / invoke.ts の整理**
- `hideResultsWindow` 削除
- `hideAllWindows` → `win.hide()` + `notifyMainHidden()` のみ
- 不要コマンドラッパー削除

### Phase 2: バックエンド整理

**2a. main.rs**
- `ensure_window_with_timing(&app_handle, "results", ...)` 行を削除
- ホットキー hide ブロックの `get_webview_window("results")` 参照削除
- `invoke_handler` から削除対象コマンド除外

**2b. commands/window.rs**
- `ensure_results_window()` 関数削除
- `ensure_window` コマンド削除
- `set_window_no_activate` コマンド削除
- `is_main_foreground` コマンド削除

**2c. commands/mod.rs**
- notify_result_clicked / notify_result_double_clicked / notify_result_hovered の re-export と `#[tauri::command]` 定義を削除（これらは results ウィンドウからの IPC コールバック用だった）

**2d. config_watcher.rs**
- L139: `for label in &["main", "results"]` → `"main"` のみに

### Phase 3: テスト更新

**3a. smoke-startup.ps1**
- `$requiredLabels` から `"results"` 削除
- `results_ms` 参照削除

**3b. bench-startup.ps1**
- `results_ms` 計測削除

**3c. E2E テスト**
- results ウィンドウ切り替え → main 内 DOM 要素で判定
- `waitForVisibleLabel(driver, "results")` → main 内 `.result-row` の出現待ち

**3d. ユニットテスト**
- `npm test` で全テスト通過確認
- commands.test.ts の `hideResultsWindow` テスト更新

### Phase 4: ドキュメント

- SPEC.md / CLAUDE.md / ui/CLAUDE.md / src-tauri/CLAUDE.md 更新

## 不変条件

1. **ウィンドウ高さ = 入力バー高さ(52px) + 結果表示時のみ結果エリア高さ**: `shouldShowResults` が true のとき `52 + max_results * 30 + 16`、false のとき `52`
2. **resizable: false でも set_size は動作する**: 既存の config_watcher がこのパターンを使用
3. **alwaysOnTop は維持**: 統合前と同じ
4. **decorations: false は維持**: 統合前と同じ
5. **検索状態（query / results / selected / folderState / toolSelectionState）は search.ts に集約**: 統合後もこの原則は変わらない
6. **アイコン取得は Tauri IPC（get_icons_batch）のまま**: バイナリ転送方式は変更なし、ResultsSection 内で直接呼び出し
7. **LruIconCache は1インスタンスのみ**: 統合により重複排除
8. **ウィンドウ高さ変更は非同期**: `set_size` の完了を await してから次の操作に進む。ただし高頻度変更（キー入力ごと）は避け、`shouldShowResults` の変更時のみ実行
9. **Escape / launch / 非表示時に高さを 52px に戻す**: `shouldShowResults` が false になったら自動的にリサイズ

## テスト方針

### 自動テスト
- `npm test`: フロント ユニットテスト（Vitest）
- `npm run build`: フロントビルド（single-page）
- `cargo check -p snotra-core -p snotra -p snotra-settings`: Rust 型チェック
- `cargo clippy -p snotra-core -p snotra -p snotra-settings`: lint
- `npm run smoke:startup`: スモークテスト（results ウィンドウ検証が削除されていること）

### 手動確認
- 検索入力→結果表示→ウィンドウ高さ拡大
- Escape→ウィンドウ高さ縮小
- ↑↓キーで選択移動→スクロール追従
- フォルダ展開→通常モード復帰
- ツール選択→Escape
- ドラッグ移動→位置保存
- auto_hide_on_focus_lost
- ホットキートグル
- max_results 設定変更→即時反映
- ウィンドウ幅設定変更→即時反映

## SPEC.md 更新要否

**必要**。以下を更新:
- §3.7: 3イベント分割の記述 → 「同一ウィンドウ内のシグナルで状態同期」に変更
- §7.5: results ウィンドウ事前生成の記述を削除。ウィンドウ高さ動的変更の記述を追加
- §7.6: 状態遷移図の注釈を更新（results ウィンドウ関連を削除）

## セルフレビュー

### 1. 対称コードパス
- results hide は `shouldShowResults` false 時にリアクティブに実行。show/hide の対称性は SolidJS のリアクティブシステムが保証 ✓
- ウィンドウ高さ拡大/縮小は同一の `createEffect` で制御 ✓
- Blob URL の revokeAll は `shouldShowResults` false 時の effect で実行 ✓

### 2. 影響範囲の網羅性
- `grep -r "results" src-tauri/` で results ウィンドウ参照を全て列挙済み ✓
- `grep -r "results-data-changed\|results-selection-changed\|results-visibility-changed" ui/` で IPC イベント参照を全て列挙済み ✓
- `grep -r "result-clicked\|result-double-clicked\|result-hovered" ui/ src-tauri/` でクリック IPC を全て列挙済み ✓
- E2E テスト・スモークスクリプトの results 参照を確認済み ✓

### 3. 境界条件
- max_results = 1: ウィンドウ高さ = 52 + 30 + 16 = 98px（最小結果表示）
- max_results = 50: ウィンドウ高さ = 52 + 1500 + 16 = 1568px（画面に収まるか確認要→設定の上限値なので画面外にはみ出る可能性あり。ただし既存動作も同様）
- 結果0件: ウィンドウ高さ 52px 維持
- indexing 中: ウィンドウ高さ 52px 維持（shouldShowResults = false）

### 4. リソース管理
- LruIconCache: 1インスタンス。`shouldShowResults` false 時に `revokeAll()` → effect で実行
- ResizeObserver: ResultsSection 内。onCleanup で disconnect
- listen() リスナー: 削減される（results 関連 IPC 不要）。残存リスナーは onCleanup で解除

### 5. 既存パターンとの整合
- ウィンドウ高さ動的変更は config_watcher の既存パターンを踏襲
- SolidJS シグナル → createEffect → API 呼び出しは既存パターン（folderFilter effect 等）
- 新規パターンの導入なし ✓

### 6. YAGNI 違反
- ウィンドウ高さのアニメーションは実装しない（シンプルな即時切替）
- 結果リストのマウント/アンマウント切替は行わない（CSS display で制御、DOM は常に存在）
- なし ✓

### 7. シンプル化の挑戦
- **最大の簡素化**: 2ウィンドウ → 1ウィンドウにより、IPC 通信層全体が不要に
- **新しい複雑さ**: ウィンドウ高さの動的変更（`set_size` 呼び出し）。ただし既存の config_watcher パターンと同一
- **状態管理は複雑化しない**: search.ts のシグナルをそのまま使う。新しい AtomicBool や Mutex は不要
- `shouldShowResults` シグナルが唯一の新規状態。これは `results().length > 0 && !indexing()` の派生値であり、独立した状態フラグではない

### 8. 破壊不変条件

| 不変条件 | リスク | 検知手段 |
|---------|--------|---------|
| ウィンドウ高さが結果非表示時に 52px に戻る | set_size 失敗時にウィンドウが大きいまま残る | smoke テスト + 手動確認 |
| alwaysOnTop 維持 | 変更なし | 既存テスト |
| ホットキーで表示/非表示 | hide 時に results 参照を削除するため regression 可能性 | E2E テスト |
| auto_hide_on_focus_lost | is_main_foreground 削除により挙動変化の可能性 | 手動確認（ドラッグ操作、外部ウィンドウクリック） |
| ドラッグ移動 + 位置保存 | ウィンドウ高さ変更によりドラッグ判定への影響 | 手動確認 |
