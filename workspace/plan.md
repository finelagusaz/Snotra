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
| `src-tauri/src/main.rs` | `ensure_results_window()` 呼び出し削除。ホットキー hide で results 参照削除 + `window-hidden` emit 追加。`show_main_and_emit` 冒頭に `set_size(52)` 追加。single_instance ハンドラを `show_main_and_emit` に統合。invoke_handler から削除コマンドを除外 |
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
- **search.ts に `mainVisible` は追加しない**（責務分離: search.ts は検索ロジック専用）

**1b. ResultsWindow.tsx → ResultsSection.tsx にリファクタ**
- IPC リスナー（`results-data-changed` / `results-selection-changed` / `results-visibility-changed`）を削除
- 代わりに search.ts のシグナル（`results`, `selected`）を直接 import
- `onClick` / `onDoubleClick` / `onMouseEnter` をコールバック props に変更（親が search store の関数を渡す）
- **hover debounce (50ms) を維持する**: 現在の `ResultsWindow.tsx:226-230` の `setTimeout(50ms)` パターンを ResultsSection 内に移植。削除すると高頻度 `setSelected` でパフォーマンス劣化
- bootstrap payload からの `show_icons` / `max_results` 取得は維持
- **`visible` prop を受け取り、false 時に Blob URL を解放する**（責務分離: 可視状態の判断は親が行い、ResultsSection はリソース管理のみ担当）:
  ```tsx
  // ResultsSection.tsx
  interface ResultsSectionProps {
    visible: boolean;
    onClickResult: (index: number) => void;
    onDoubleClickResult: (index: number) => void;
    onHoverResult: (index: number) => void;
  }

  createEffect(() => {
    if (!props.visible) {
      iconCache.revokeAll();
      setIconCacheVersion((v) => v + 1); // ← revokeAll とペアで必ずインクリメント
    }
  });
  ```
- **revokeAll + setIconCacheVersion は常にペアで呼ぶ**: revoke 済み URL が `<img src>` に残ることを防ぐ（既存 ResultsWindow.tsx の3箇所すべてがこのペアを守っている）

**1c. MainApp.tsx の簡素化**
- `createResultsWindowController` 関連コード全削除
- results 系リスナー 5 個削除（`results-data-changed`, `results-visibility-changed`, `result-clicked`, `results-render-done`, `result-double-clicked`, `result-hovered`）
- **`window-hidden` リスナーを追加**: Rust ホットキー hide 時に emit される `window-hidden` を受け取り `setMainVisible(false)` を呼ぶ。`window-shown` リスナーの対称ペア
- **`mainVisible` はローカルシグナルとして MainApp 内に保持**（search.ts には入れない）:
  ```ts
  const [mainVisible, setMainVisible] = createSignal(false);
  ```
- ウィンドウ高さ変更ロジック追加:
  ```ts
  createEffect(() => {
    const show = shouldShowResults();
    const height = show ? 52 + maxResults() * 30 + 16 : 52;
    void win.setSize(new LogicalSize(cachedWidth, height));
  });
  ```
- **ResultsSection に `visible` prop を渡す**: `shouldShowResults() && mainVisible()` を算出し、false 時に ResultsSection 内で Blob URL が解放される
- `hideMainAndResults` → `hideMain`（results hide 不要）。hide 時に `setMainVisible(false)` を呼ぶ
- `auto_hide_on_focus_lost` の `is_main_foreground` チェック削除（同一ウィンドウなので不要）。**削除理由と `blurCancelled` debounce を残す理由をコードコメントに明記する**（debounce はドラッグ移動時の一時的フォーカス喪失対策として引き続き必要）

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
- ホットキー hide ブロック（L464-474）: `get_webview_window("results")` 参照削除
- **ホットキー hide ブロックに `emit("window-hidden", ())` を追加**: JS 側で `setMainVisible(false)` を呼び、Blob URL 即時解放をトリガーする（`window-shown` との対称ペア）
- **`show_main_and_emit` の冒頭で `set_size(width, 52)` を呼んでウィンドウ高さをリセット**（show 側1箇所に集約する戦略。hide 経路が増えても漏れない。`set_size(52)` → `show()` の順なら非表示中にリサイズが完了しチラつきなし。プロセス異常終了後の再起動でもカバーされる）
- **single_instance ハンドラを `show_main_and_emit` に統合**: 現在の直接 `w.show()` + `w.set_focus()` を `show_main_and_emit(&app, ime_control)` に置き換え、高さリセット・IME 制御・`window-shown` emit を漏れなく適用する。`ime_off` をクロージャでキャプチャする
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
- **削除前に最終ベンチマーク値を `PERFORMANCE.md` に記録する**（results ウィンドウ生成が起動時間のどの割合を占めていたかの参考値）
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
2. **show 時にウィンドウ高さを 52px にリセットする**: `show_main_and_emit` の冒頭で `set_size(width, 52)` → `show()` の順で呼ぶ。hide 経路に依存しないため漏れがない
3. **resizable: false でも set_size は動作する**: 既存の config_watcher がこのパターンを使用
4. **alwaysOnTop は維持**: 統合前と同じ
5. **decorations: false は維持**: 統合前と同じ
6. **検索状態（query / results / selected / folderState / toolSelectionState）は search.ts に集約**: 統合後もこの原則は変わらない。`mainVisible` は search.ts に入れない（ウィンドウ可視状態は MainApp のローカル責務）
7. **アイコン取得は Tauri IPC（get_icons_batch）のまま**: バイナリ転送方式は変更なし、ResultsSection 内で直接呼び出し
8. **LruIconCache は1インスタンスのみ**: 統合により重複排除
9. **revokeAll と setIconCacheVersion は常にペアで呼ぶ**: revoke 済み Blob URL が `<img src>` に渡るのを防ぐ
10. **ウィンドウ高さ変更は非同期**: `set_size` の完了を await してから次の操作に進む。ただし高頻度変更（キー入力ごと）は避け、`shouldShowResults` の変更時のみ実行
11. **Blob URL は ResultsSection の `visible` prop false 時に解放する**: `shouldShowResults() && mainVisible()` が false になったとき revokeAll + iconCacheVersion インクリメント。Escape/ホットキー hide で results 配列がクリアされないケースでもリーク防止
12. **hover debounce (50ms) を維持する**: ResultsSection 内の onMouseEnter で 50ms setTimeout を使い、高頻度 setSelected を抑制

## テスト方針

### 自動テスト
- `npm test`: フロント ユニットテスト（Vitest）
- `npm run build`: フロントビルド（single-page）
- `cargo check -p snotra-core -p snotra -p snotra-settings`: Rust 型チェック
- `cargo clippy -p snotra-core -p snotra -p snotra-settings`: lint
- `npm run smoke:startup`: スモークテスト（results ウィンドウ検証が削除されていること）

### 手動確認
- 検索入力→結果表示→ウィンドウ高さ拡大（**拡大時に空白領域が一瞬見えないこと**）
- Escape→ウィンドウ高さ縮小（**縮小時にチラつかないこと**）
- ホットキー hide→show（**show 時に一瞬大きいウィンドウが見えないこと**）
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

### 1. 対称コードパス（/symmetric-check 反映済み）
- results hide は `shouldShowResults` false 時にリアクティブに実行。show/hide の対称性は SolidJS のリアクティブシステムが保証 ✓
- ウィンドウ高さ拡大/縮小は同一の `createEffect` で制御 ✓
- **[追加] ウィンドウ高さリセットは show 側に集約**: `show_main_and_emit` の冒頭で `set_size(52)` を呼ぶ。hide 経路に依存しない ✓
- **[追加] single_instance ハンドラも `show_main_and_emit` を経由**: 高さリセット漏れ防止 ✓
- **[追加] `window-hidden` イベントで mainVisible を即時更新**: Rust ホットキー hide → `emit("window-hidden")` → JS `setMainVisible(false)` → Blob URL 即時解放 ✓
- **[追加] Blob URL 解放は `mainVisible` false をトリガーに含める**: Escape/ホットキー hide で results 配列がクリアされないケースでも revokeAll が発火する ✓
- **[追加] revokeAll + setIconCacheVersion は常にペア**: 既存3箇所のパターンを維持 ✓
- **[追加] hover debounce 50ms 維持**: ResultsSection 移行時に落とさない ✓

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

### 4. リソース管理（/symmetric-check 反映済み）
- LruIconCache: 1インスタンス。`shouldShowResults` false **または** `mainVisible` false 時に `revokeAll()` + `setIconCacheVersion++` → effect で実行
- ResizeObserver: ResultsSection 内。onCleanup で disconnect
- listen() リスナー: 削減される（results 関連 IPC 不要）。残存リスナーは onCleanup で解除
- hover debounce タイマー: ResultsSection 内。onCleanup で clearTimeout

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
| ウィンドウ高さが結果非表示時に 52px に戻る | set_size 失敗 or hide 経路漏れでウィンドウが大きいまま残る | smoke テスト + 手動確認（Escape / ホットキー / auto_hide の3経路） |
| show 時に高さ 52px リセット | `show_main_and_emit` を経由しない show パスがあると高さリセット漏れ（例: single_instance） | `show_main_and_emit` の呼び出し箇所を grep で全列挙 + 手動確認 |
| `window-hidden` イベントで mainVisible 即時更新 | Rust 側 hide パスで emit 漏れがあると Blob URL リーク | 開発ツールで Blob URL 数を確認。hide→数分待ち→show して増加していないこと |
| Blob URL が hide 後にリークしない | mainVisible false 時の revokeAll が全 hide 経路でトリガーされないとメモリリーク | 上記 window-hidden + JS hideMain の2経路でカバー |
| revokeAll + iconCacheVersion ペア | revokeAll のみで iconCacheVersion を更新しないと revoke 済み URL が img に渡る | 目視: hide→show 後にアイコンが壊れた画像にならないこと |
| alwaysOnTop 維持 | 変更なし | 既存テスト |
| ホットキーで表示/非表示 | hide 時に results 参照を削除するため regression 可能性 | E2E テスト |
| auto_hide_on_focus_lost | is_main_foreground 削除により挙動変化の可能性 | 手動確認（ドラッグ操作、外部ウィンドウクリック） |
| ドラッグ移動 + 位置保存 | ウィンドウ高さ変更によりドラッグ判定への影響 | 手動確認 |
| hover debounce 50ms | 移行時に落とすと高頻度 setSelected でパフォーマンス劣化 | 手動確認: マウスを素早く動かしてちらつかないこと |
