# Issue #245 実装計画

作成日: 2026-03-12
ブランチ: `fix/indexing-signal-mismatch`
対象 Issue: #245 - インデックス再構築中に通常検索が可能になる問題

---

## 判定: バグ

`SPEC.md §3` 「構築中は検索ウィンドウに「インデックス構築中...」メッセージを表示」という仕様意図に対し、再構築時に frontend の `indexing()` signal が正しく `true` にならないため、ガードが機能していない。コード修正で対応する（仕様変更ではない）。

---

## 受け入れ条件（テスト可能な形）

1. `/s` 実行後にウィンドウを再表示しても、インデックス構築完了まで通常検索がブロックされる
2. `handleInput` で `/` や `@` プレフィックスはバイパスされる（既存動作を維持）
3. `config_watcher` 経由の設定変更による再構築中も同様にブロックされる
4. `indexing-complete` 後は正常に通常検索ができる

---

## 影響範囲

### 触る

- `src-tauri/src/indexing.rs` → `start_index_build()` に `emit("indexing-started", ())` を追加
- `ui/src/stores/search.ts` → `initIndexingState()` に `indexing-started` リスナーを追加

### 触らない（理由）

- `src-tauri/src/commands/system.rs` (`rebuild_index`): `start_index_build()` を呼ぶだけで、emit は `start_index_build()` 内で行う。変更不要
- `src-tauri/src/config_watcher.rs` (`apply_config_change`): 同上、`start_index_build()` 経由でカバーされる
- `ui/src/components/SearchWindow.tsx` (`handleInput`): ガード自体は正しい。signal が正しく更新されれば動作する
- `src-tauri/src/commands/config.rs` (`get_bootstrap_payload`): 起動時の初期化に使用、再構築通知とは別ルート。変更不要

### 対称コードパス確認

`indexing-complete` ↔ `indexing-started` がペアになる。
- `indexing-complete` は `indexing.rs` の末尾（build スレッド完了時）で emit
- `indexing-started` は `indexing.rs` の先頭（`state.indexing = true` セット直後）で emit

両 emit とも同じ `start_index_build()` 内に配置するため、全呼び出しパスで対称性が保たれる:
- `/s` コマンド → `rebuild_index` → `start_index_build()` ✓
- `config_watcher` → `apply_config_change()` → `start_index_build()` ✓
- 初回ビルド → `main.rs setup` → `start_index_build()` ✓（初回は `initIndexingState()` が `getIndexingState()` で true を取得するため、イベントが届いてもべき等）

### `docs/` 更新要否

なし（アーキテクチャの変更ではなく既存パターンの修正）。

---

## 事前調査（レビュー未然防止）

### `indexing-started` イベントのべき等性

`setIndexing(true)` を複数回呼んでも SolidJS のシグナルは同一値なら再実行しない（べき等）。
初回起動時に `getIndexingState()` で `true` を得た後、さらに `indexing-started` が届いても問題なし。

### リソース管理（listen のライフサイクル）

`initIndexingState()` は `unlisten` 関数を返すパターンを既に持つ（`unlistenIndexingComplete`）。
`indexing-started` のリスナーも同様に `unlistenIndexingStarted` として保持し、同じ cleanup 関数に含める。

### フラグの真偽ペア

`indexing` フラグ:
- `true` にする: `start_index_build()` → emit `indexing-started` → `setIndexing(true)` ✓
- `false` にする: build スレッド完了 → emit `indexing-complete` → `setIndexing(false)` ✓

失敗時（build スレッドの `spawn` 失敗）: `start_index_build()` は `thread::Builder::spawn().ok()` で spawn エラーを無視している。この場合 `indexing-started` を emit した後 `indexing-complete` が来ない。
→ 既存の `indexing-complete` も同じ問題を持つ。spawn 失敗は極めてまれ（OS リソース枯渇時のみ）で本 issue の範囲外。

### `updateResults([])` を `indexing-started` リスナーで呼ぶか

呼ばなくてよい。`shouldShowResults = results().length > 0 && (!indexing() || instantCommandMode())` のため、`indexing=true` になれば `shouldShowResults` が `false` になり結果は自動的に非表示になる。`runRefresh()` が呼ばれた際も `indexing_guard` ブランチで `updateResults([])` される。

---

## 実装手順

### Step 1: `src-tauri/src/indexing.rs` に `indexing-started` emit を追加

`start_index_build()` 内、`state.indexing.store(true, Ordering::SeqCst)` の直後（`PlatformCommand::SetIndexing(true)` 送信の後）に追加:

```rust
// Notify frontend that indexing has started
let _ = app.emit("indexing-started", ());
```

### Step 2: `ui/src/stores/search.ts` の `initIndexingState()` に `indexing-started` リスナーを追加

`unlistenIndexingComplete` と対称に `unlistenIndexingStarted` を追加:

```ts
let unlistenIndexingStarted: (() => void) | undefined;

// initIndexingState() 内:
unlistenIndexingStarted = await listen("indexing-started", () => {
  trace("search:indexing_state:started");
  setIndexing(true);
});

// cleanup 関数に含める:
return () => {
  unlistenIndexingComplete?.();
  unlistenIndexingComplete = undefined;
  unlistenIndexingStarted?.();
  unlistenIndexingStarted = undefined;
};
```

### Step 3: 検証

- `cargo check -p snotra-core -p snotra -p snotra-settings`
- `npm run typecheck`
- `npm run build`
- `npm test`（既存テストがパスすること）

---

## 変更ファイル一覧

| ファイル | 変更種別 | 内容 |
|---|---|---|
| `src-tauri/src/indexing.rs` | 修正 | `start_index_build()` に `emit("indexing-started", ())` を追加 |
| `ui/src/stores/search.ts` | 修正 | `initIndexingState()` に `indexing-started` リスナーを追加、cleanup に含める |

---

## セルフレビュー

### チェック項目と結果

1. **対称ペアの確認**: `indexing-started` / `indexing-complete` のペア。`indexing.rs` の `start_index_build()` 内に両方配置。全呼び出しパス（`/s`・`config_watcher`・初回ビルド）で一貫して発火する ✓

2. **リソース管理**: `listen("indexing-started")` の `unlisten` を `initIndexingState()` の返り値 cleanup 関数に含める。既存の `unlistenIndexingComplete` と同パターン ✓

3. **フラグの真偽ペア**: `setIndexing(true)` は `indexing-started` リスナーで、`setIndexing(false)` は `indexing-complete` リスナーで行われる。spawn 失敗は既存の問題と同様で本 issue のスコープ外 ✓

4. **初回ビルドのべき等性**: 初回起動時は `getIndexingState()` が `true` を返すため `setIndexing(true)` 済み。その後 `indexing-started` が届いても `setIndexing(true)` を再度呼ぶだけで副作用なし ✓

5. **`updateResults([])` の不要性を確認**: `shouldShowResults` のメモが `indexing()` に依存しているため、`setIndexing(true)` だけで表示が切り替わる。余計な結果クリアは不要 ✓

6. **`src-tauri/CLAUDE.md` の発火イベント一覧の更新要否**: `config_watcher.rs` のコメントに発火イベントが列挙されているが、`indexing-started` は `indexing.rs` から発火するため、そのコメントへの追記は必要ない。ただし `src-tauri/CLAUDE.md` の `config_watcher.rs` の説明行「発火するイベント: ... `indexing-complete`（indexing.rs から）」という記述があるが、`indexing-started` も追加すべきか検討 → 追加する。

7. **`CLAUDE.md` モジュール構成の同期**: 新規ファイル追加・削除なし。既存ファイルの修正のみ ✓

### セルフレビューによる修正点

- `initIndexingState()` のリスナー本体から `updateResults([])` と `setSelected(0)` を削除（不要と判明、Step 2 を更新済み）
- `src-tauri/CLAUDE.md` の `config_watcher.rs` 説明行に `indexing-started`（indexing.rs から）を追記することを「触る範囲」に追加

### 最終的な変更の最小性評価

2ファイル（+ CLAUDE.md 1行）、各数行の追加のみ。既存パターンの対称的な適用であり、新しい概念を導入しない。最小変更と判断する。
