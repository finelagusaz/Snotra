# research.md — issue #191: 最大表示行数以上の結果がスクロールで見られない

## issue の要約

`max_results`（設定値）が「ウィンドウの高さ（可視行数）」と「バックエンドの検索結果件数上限」の両方に使われている。
そのため、結果が `max_results` 件を超えると余分なものが返ってこず、スクロールが機能しない。

あるべき姿（SPEC §3.2 L123）: 「ヒット数が最大表示件数未満でも高さは維持され、超過時はスクロールバーを表示する」

---

## 関連コード

### バックエンド（結果件数を打ち切っている箇所）

| ファイル | 箇所 | 内容 |
|---|---|---|
| `snotra-core/src/engine.rs:77` | `Engine::search()` | `max_results` を `search_with_history_boost` に渡す → ヒープ容量の上限になる |
| `snotra-core/src/engine.rs:88-91` | `capture_folder_list_context()` | `FolderListContext.max_results` に同値を入れる |
| `snotra-core/src/engine.rs:104,118` | `finalize_folder_list()`, `list_folder()` | `ctx.max_results` を `folder::score_entries` / `folder::list_folder` に渡す |
| `snotra-core/src/search.rs:313-525` | `search_with_history_boost()` | BinaryHeap の capacity を `max_results` に固定（= 返る結果数の上限） |
| `snotra-core/src/folder.rs:87-139` | `score_entries()` | `max_results` を `truncate` の k として使う |

### フロントエンド（すでにスクロール対応済み）

| ファイル | 内容 |
|---|---|
| `ui/src/styles/global.css:53` | `.result-list-standalone { overflow-y: auto; }` — スクロール CSS は実装済み |
| `ui/src/components/ResultsWindow.tsx:27-36` | `ensureRowVisible()` — スクロール位置調整ロジック実装済み |
| `ui/src/components/ResultsWindow.tsx` | `scrollToSelected()` — キー移動時の自動スクロール実装済み |
| `ui/src/lib/resultsWindowController.ts:110-115` | ウィンドウ高さを `cachedMaxResults * RESULT_ROW_HEIGHT + PADDING` で固定 — これは正しい挙動 |

### ウィンドウ高さ計算（変更不要）

`resultsWindowController.ts` はウィンドウ高を `max_results` 行分に固定しており、これは SPEC の意図通り。
変更するのはバックエンドの取得件数上限のみ。

---

## 既存パターン

- `recent_history()` は別途 `max_history_display` で件数を管理しており、`max_results` とは独立 → 変更不要
- `FolderListContext.max_results` フィールドはプライベートで、`engine.rs` 内のみで使用 → 影響範囲は engine.rs だけ
- `snotra-settings` の設定 UI で `max_results` のレンジは `1..=50` → `SCROLL_FETCH_LIMIT` はこれより大きい値にする必要あり

---

## 技術的制約

- `search_with_history_boost` の BinaryHeap はキャパシティを `max_results` に使う。件数を増やしても O(N log k) で計算量変化は軽微
- `snotra-settings` の UI で `max_results` の range は `1..=50`。`SCROLL_FETCH_LIMIT = 100` とすれば常に設定値より大きく、スクロール余地が生まれる
- スクロール CSS・JS は実装済みのため、フロントエンドの変更は不要

---

## 未解決の疑問

なし。SPEC L123 と既存実装で要件・実装方針ともに明確。
