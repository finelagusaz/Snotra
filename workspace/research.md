# Research — 結果ウィンドウの高さを最大表示件数の高さに固定する (#165)

## issue の要約

結果ウィンドウの高さを `max_results` の高さに固定し、ヒット件数が少なくてもウィンドウ高さを維持する。ヒット数 > max_results 時はスクロールバー表示。ヒット数 0 時は従来通り hide。

## 関連コード

### 高さ計算の現状

`ui/src/lib/resultsWindowController.ts:116`:
```ts
const resultsHeight = Math.min(count * RESULT_ROW_HEIGHT + RESULTS_PADDING * 2, 400);
```
- `RESULT_ROW_HEIGHT = 30`、`RESULTS_PADDING = 8`
- 現状は **実際の結果件数 (`count`)** ベースで高さを算出し、上限 400px でキャップ
- `count = results.length` (payload から取得)

### max_results の流れ

- **定義**: `snotra-core/src/config.rs:208` — `AppearanceConfig.max_results: usize`（デフォルト 8）
- **検索側での使用**: `snotra-core/src/engine.rs:77` — `Engine::search()` が `self.config.appearance.max_results` で結果を切り詰め
- **フロントへの伝達**: 現在 **BootstrapPayload に max_results は含まれていない**
- **設定変更時**: `config_watcher.rs` で `config.toml` 変更を検知するが、max_results 変更時のフロント通知イベントは **存在しない**

### 結果ウィンドウの表示フロー

1. `search.ts:emitDataChanged()` → `results-data-changed` イベント emit
2. `MainApp.tsx` のリスナーが `controller.handleDataChanged(payload)` を呼ぶ
3. `resultsWindowController.ts:handleDataChanged()` で `results.length` から高さ算出 → `setSize` → `setPosition` → `show`

### CSS 側

- `global.css:51-53` — `.result-list-standalone { flex: 1; overflow-y: auto; }`
- ウィンドウ自体を固定高にすれば、内容が少ない場合は余白が残り、多い場合はスクロールバーが自然に出る

## 既存パターン

- `show_icons` は `BootstrapPayload` + `show-icons-changed` イベントで伝達する既存パターン
- `window_width` は `config_watcher.rs` で変更検知 → Rust 側で直接 `set_size` — ただし width のみ変更し height は維持する設計
- **`window_width` 変更パターンが最も近い**: `config_watcher` で変更検知 → results ウィンドウの `set_size` を Rust 側で直接呼ぶ

## 技術的制約

- `resultsWindowController` は `main` ウィンドウ側で動作し、結果ウィンドウの `setSize` を呼ぶ
- `max_results` の値をフロントに伝える経路が必要（Bootstrap + config 変更時）
- `results` ウィンドウは常に Tauri のウィンドウサイズで高さが決まる（CSS は `height: 100%`）

## 設計の選択肢

### 案A: resultsWindowController で固定高を算出（フロント側）
- `max_results` を BootstrapPayload + イベントでフロントに伝達
- `resultsWindowController` が `max_results` をキャッシュし、高さ計算を `maxResults * ROW_HEIGHT + PADDING * 2` に変更
- **メリット**: 件数ベースのリサイズロジックが一箇所に集約
- **デメリット**: max_results を伝達する新しい IPC 経路が必要

### 案B: config_watcher で Rust 側から直接 results の高さを設定 ★推奨
- window_width と同じパターン: `config_watcher` が `max_results` 変更を検知 → results ウィンドウの高さを `set_size` で設定
- `resultsWindowController` の高さ計算を `max_results` ベースの固定高に変更
- 起動時は BootstrapPayload 経由で `max_results` をフロントに渡す
- **メリット**: width 変更と同一パターン。config_watcher が results の高さも直接管理
- **デメリット**: `resultsWindowController` も max_results を知る必要がある（size 変更のスキップ判定に使う）

### 案C: handleDataChanged で max_results ベースの固定高を使う（最小変更）★最推奨
- BootstrapPayload に `max_results` を追加
- `max_results` 変更時のイベント `max-results-changed` を追加（config_watcher から emit）
- `resultsWindowController` が `cachedMaxResults` を保持
- `handleDataChanged` の高さ計算を `cachedMaxResults * ROW_HEIGHT + PADDING * 2` に変更（count ベースではなく）
- **メリット**: 高さ計算のロジックが resultsWindowController に集約、width 変更はそのまま
- **デメリット**: max_results のイベント経路が1つ増える

→ **案C を採用**: 最もシンプル。config_watcher の Rust 直接リサイズは width だけを担当し、height は resultsWindowController が max_results ベースで固定管理。責務分離が明確。

## 未解決の疑問

なし。設計方針は明確。
