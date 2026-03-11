# Issue #245 調査レポート

調査日: 2026-03-12
調査ブランチ: `fix/indexing-signal-mismatch`
調査対象: Issue #245 - インデックス再構築中に通常検索が可能になる問題

---

## Issue の要約

既存インデックスがある状態でインデックスを再構築（`/s` コマンドまたは `config_watcher` による自動再構築）すると、フロントエンドの `indexing()` signal が `false` のままになる。

- `handleInput` のガード `if (indexing() && !trimmed.startsWith("/") ...)` が機能せず、通常検索が実行できてしまう
- `/o` は backend の `state.indexing` を直接参照するため正しくブロックされる
- 問題は「backend の `state.indexing` と frontend の `indexing()` signal が非同期で同期されていること」と「再構築開始時に frontend へ通知するイベントが存在しないこと」の組み合わせ

---

## 根本原因の特定

### フロー分析

#### 初回ビルド（正常動作）
1. `main()`: `initial_indexing = true` → `AppState.indexing = true`
2. `initIndexingState()` → `api.getIndexingState()` → `true` が返る → `setIndexing(true)`
3. UI: indexing ガードが機能する

#### 再構築（問題あり）
1. `/s` コマンド実行 → `hideMainWindow()` + `api.rebuildIndex()`
2. `rebuild_index` コマンド (`system.rs`): `state.index_build_started = false` → `start_index_build()` 呼び出し
3. `start_index_build()` (`indexing.rs`): `state.indexing = true` を**バックエンドでセット**
4. **この時点でフロントエンドへの通知なし** → `indexing()` signal は `false` のまま
5. ウィンドウを再表示（ホットキー）すると `indexing()` は依然 `false`
6. `handleInput` のガードが機能せず通常検索が通る
7. 構築完了時 → `indexing-complete` イベント発火 → `setIndexing(false)` + `runRefresh()`

#### `config_watcher` による再構築も同様
- `apply_config_change()`: `index_changed && !indexing_in_progress` → `index_build_started = false` → `start_index_build()`
- やはりフロントエンドへの通知なし

### コード証拠

`initIndexingState()` (`ui/src/stores/search.ts:696-715`):
```ts
async function initIndexingState(): Promise<() => void> {
  const state = await api.getIndexingState();
  setIndexing(state);                          // 起動時に一回だけポーリング
  unlistenIndexingComplete = await listen("indexing-complete", () => {
    setIndexing(false);                        // 完了時のみ受信
    void runRefresh();
  });
  ...
}
```

`start_index_build()` (`src-tauri/src/indexing.rs:13-89`):
- `state.indexing.store(true, ...)` → backend フラグを true に
- `app_handle.emit("indexing-complete", ())` → 完了時のみ emit
- **開始時の emit なし**

`rebuild_index` (`src-tauri/src/commands/system.rs:9-16`):
```rust
pub fn rebuild_index(state: State<AppState>, app: AppHandle) -> bool {
    if state.indexing.load(Ordering::SeqCst) { return false; }
    state.index_build_started.store(false, Ordering::SeqCst);
    indexing::start_index_build(&app)
    // start_index_build の戻りが true でも frontend への通知なし
}
```

### なぜ `getIndexingState()` ポーリングでは解決できないか

`initIndexingState()` は `onMount` 内で一度だけ呼ばれる（`MainApp.tsx:109`）。
再構築のたびにポーリングしていないため、再構築開始時に `indexing()` が更新されない。

---

## 関連コード一覧

| ファイル | 箇所 | 役割 |
|---|---|---|
| `src-tauri/src/indexing.rs:13-89` | `start_index_build()` | インデックス構築スレッド起動、`indexing-complete` 発火 |
| `src-tauri/src/commands/system.rs:9-16` | `rebuild_index` コマンド | `/s` からの再構築トリガー |
| `src-tauri/src/config_watcher.rs:156-161` | `apply_config_change()` | config 変更時の再構築トリガー |
| `ui/src/stores/search.ts:696-715` | `initIndexingState()` | frontend の indexing signal 初期化 + complete リスナー |
| `ui/src/stores/search.ts:17` | `const [indexing, setIndexing]` | indexing シグナル |
| `ui/src/components/SearchWindow.tsx:246` | `handleInput` | indexing ガード |

---

## 既存パターン

### `indexing-complete` イベントパターン（既存）
- backend: `app_handle.emit("indexing-complete", ())`
- frontend: `listen("indexing-complete", () => { setIndexing(false); runRefresh(); })`

### BootstrapPayload の `indexing` フィールド（既存）
- `get_bootstrap_payload` が `state.indexing` を返す（起動時の初期状態同期用）
- `initIndexingState()` では `getIndexingState()` IPC で同期

---

## 技術的制約

1. **`indexing-complete` の対称イベントとして `indexing-started` を追加するのが最小変更**
2. `start_index_build()` は `indexing.rs` に存在し、再構築開始時に `state.indexing = true` をセットする箇所が既にある。ここに `emit("indexing-started", ())` を追加するのが自然
3. `initIndexingState()` に `listen("indexing-started", ...)` を追加して `setIndexing(true)` を呼ぶ
4. `config_watcher::apply_config_change()` 経由の再構築も `start_index_build()` を呼ぶため、emit 追加だけで両方のパスをカバーできる

---

## 未解決の疑問

なし（根本原因は明確）
