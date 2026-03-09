---
paths:
  - "ui/src/**/*.{ts,tsx}"
---

# ui 実装パターン

- 検索ウィンドウのドラッグ移動は `.search-bar` の `data-tauri-drag-region` 属性で実現。`<input>` には付与しないため入力操作は維持される
- ドラッグ開始時の一時的なフォーカス喪失で `auto_hide_on_focus_lost` が誤発火するため、`onFocusChanged` の非表示処理に 100ms の猶予を設けフォーカス復帰時にキャンセルする設計
- **`async` 関数内で `await` をまたぐ可変変数はローカルキャプチャする**: `let` 変数やモジュールスコープの可変変数を `await` をまたいで参照する場合、関数冒頭で `const` にコピーしてから使う
- **`await` 後に保存状態を復元する場合は staleness チェックを入れる**: `searchGeneration` 等の世代カウンタで「`await` 中に状態が変わっていないか」を検証してから復元する

## Blob URL 管理の不変条件

- アイコンの Blob URL は `LruIconCache`（`lruIconCache.ts`）が一元管理する。`URL.createObjectURL` で生成した URL は必ず `cache.set()` または早期リターン時の明示的 `revokeObjectURL` で回収する
- `parseBinaryBatch` で Blob URL を生成した後、`cache.set()` に到達する前に早期リターンするパス（stale guard 等）では、`parsed` 内の全 URL を明示的に `revokeObjectURL` すること
- `ResultsSection` の `visible` prop が `false` になったとき `cache.revokeAll()` + `iconCacheVersion` 更新で Blob URL を一括解放する
