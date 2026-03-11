# Issue #245 実装計画

作成日: 2026-03-12
調査更新日: 2026-03-12（多角的サブエージェント調査で精緻化）
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
- `SPEC.md` → §8.6 状態遷移図に `indexing-started` イベント発火の明示（オプション）

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
- `/s` コマンド → `commands/system.rs:15` → `start_index_build()` ✓
- `config_watcher` → `config_watcher.rs:160` → `start_index_build()` ✓
- 初回ビルド → `main.rs:465` → `start_index_build()` ✓（初回は `initIndexingState()` が `getIndexingState()` で true を取得するため、イベントが届いてもべき等）
- snotra-settings 終了後（first-run モード） → `commands/window.rs:141` → `start_index_build()` ✓

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

**ファイル**: `src-tauri/src/indexing.rs`

#### 現在の状態（調査確認済み）

```rust
// 行 23: フラグを true にセット
state.indexing.store(true, Ordering::SeqCst);

// 行 25-30: プラットフォームブリッジ通知
if let Some(bridge) = app.try_state::<Mutex<PlatformBridge>>()
    && let Ok(b) = bridge.lock()
{
    b.send_command(PlatformCommand::SetIndexing(true));
}

// 行 32: バックグラウンドスレッド spawn
let app_handle = app.clone();
```

#### 追加箇所

`PlatformCommand::SetIndexing(true)` 送信ブロックの **直後**、`app.clone()` の **直前** に以下を追加:

```rust
// Notify frontend that indexing has started
let _ = app.emit("indexing-started", ());
```

#### 完成形のイメージ

```rust
state.indexing.store(true, Ordering::SeqCst);

if let Some(bridge) = app.try_state::<Mutex<PlatformBridge>>()
    && let Ok(b) = bridge.lock()
{
    b.send_command(PlatformCommand::SetIndexing(true));
}

// ← ここに追加
// Notify frontend that indexing has started
let _ = app.emit("indexing-started", ());

let app_handle = app.clone();
// ... spawn ...
```

#### `indexing-complete` の既存 emit との比較（対称確認）

```rust
// 完了側（既存、行 84）
let _ = app_handle.emit("indexing-complete", ());

// 開始側（追加）
let _ = app.emit("indexing-started", ());
```

ペイロードはどちらも `()` で統一。emit の戻り値は `let _ =` で無視（既存パターンと同じ）。

---

### Step 2: `ui/src/stores/search.ts` の `initIndexingState()` に `indexing-started` リスナーを追加

**ファイル**: `ui/src/stores/search.ts`

#### 現在の `initIndexingState()` 実装（調査確認済み、行 696-715）

```typescript
async function initIndexingState(): Promise<() => void> {
  try {
    const state = await api.getIndexingState();
    setIndexing(state);
    trace("search:indexing_state:init", { indexing: state });
  } catch (e) {
    trace("search:indexing_state:error", { error: String(e) });
    console.error("Failed to get indexing state:", e);
  }

  unlistenIndexingComplete = await listen("indexing-complete", () => {
    trace("search:indexing_state:complete");
    setIndexing(false);
    void runRefresh();
  });
  return () => {
    unlistenIndexingComplete?.();
    unlistenIndexingComplete = undefined;
  };
}
```

#### モジュールスコープの変数宣言（現在の `unlistenIndexingComplete` の定義場所に追記）

`unlistenIndexingComplete` の宣言箇所（関数の近く）を探して、直下に以下を追加:

```typescript
let unlistenIndexingStarted: (() => void) | undefined;
```

#### `initIndexingState()` の変更後

```typescript
async function initIndexingState(): Promise<() => void> {
  try {
    const state = await api.getIndexingState();
    setIndexing(state);
    trace("search:indexing_state:init", { indexing: state });
  } catch (e) {
    trace("search:indexing_state:error", { error: String(e) });
    console.error("Failed to get indexing state:", e);
  }

  unlistenIndexingComplete = await listen("indexing-complete", () => {
    trace("search:indexing_state:complete");
    setIndexing(false);
    void runRefresh();
  });

  // ↓ 追加ブロック
  unlistenIndexingStarted = await listen("indexing-started", () => {
    trace("search:indexing_state:started");
    setIndexing(true);
  });

  return () => {
    unlistenIndexingComplete?.();
    unlistenIndexingComplete = undefined;
    unlistenIndexingStarted?.();          // ← 追加
    unlistenIndexingStarted = undefined;  // ← 追加
  };
}
```

#### `listen` の import 確認

`listen` はすでに行 2 でインポート済み（変更不要）:
```typescript
import { listen } from "@tauri-apps/api/event";
```

---

### Step 3: テスト追加

**ファイル**: `ui/src/stores/search.test.ts`

既存の `search.test.ts` に `indexing-started` イベントのテストセクションを追加する。
Tauri の `listen` はモック済み（`vi.mock("@tauri-apps/api/event")`）のため、イベントを手動で発火させる形でテストを書く。

追加するテストケース:
1. `indexing-started` イベントを受信すると `indexing()` signal が `true` になる
2. `indexing-started` + `indexing-complete` のシーケンスで `false` に戻る

---

### Step 4: 検証

```bash
cargo check -p snotra-core -p snotra -p snotra-settings
npm run typecheck
npm run build
npm test
```

---

## 変更ファイル一覧

| ファイル | 変更種別 | 変更行数（概算） |
|---|---|---|
| `src-tauri/src/indexing.rs` | 修正 | +2行（コメント + emit） |
| `ui/src/stores/search.ts` | 修正 | +7行（変数宣言・リスナー・cleanup 2行） |
| `ui/src/stores/search.test.ts` | テスト追加 | +20〜30行 |

---

## セルフレビュー

### チェック項目と結果

1. **対称ペアの確認**: `indexing-started` / `indexing-complete` のペア。`indexing.rs` の `start_index_build()` 内に両方配置。全呼び出しパス（4経路）で一貫して発火する ✓

2. **リソース管理**: `listen("indexing-started")` の `unlisten` を `initIndexingState()` の返り値 cleanup 関数に含める。既存の `unlistenIndexingComplete` と同パターン ✓

3. **フラグの真偽ペア**: `setIndexing(true)` は `indexing-started` リスナーで、`setIndexing(false)` は `indexing-complete` リスナーで行われる。spawn 失敗は既存の問題と同様で本 issue のスコープ外 ✓

4. **初回ビルドのべき等性**: 初回起動時は `getIndexingState()` が `true` を返すため `setIndexing(true)` 済み。その後 `indexing-started` が届いても `setIndexing(true)` を再度呼ぶだけで副作用なし ✓

5. **`updateResults([])` の不要性を確認**: `shouldShowResults` のメモが `indexing()` に依存しているため、`setIndexing(true)` だけで表示が切り替わる。余計な結果クリアは不要 ✓

6. **`CLAUDE.md` モジュール構成の同期**: 新規ファイル追加・削除なし。既存ファイルの修正のみ ✓

7. **emit の位置**: `PlatformCommand::SetIndexing(true)` 送信後・`app.clone()` 前に配置。バックグラウンドスレッドが走り始める前に frontend へ通知されるため、race condition なし ✓

8. **`Emitter` trait の use 文**: 既存コードで `app_handle.emit("indexing-complete", ())` が行 84 で使われているため、`Emitter` trait はすでに use 宣言済み（追加不要）✓

### セルフレビューによる修正点（初回計画からの変更）

- `initIndexingState()` のリスナー本体から `updateResults([])` と `setSelected(0)` を削除（不要と判明）
- emit 挿入位置を `app.clone()` 直前と明確化（`PlatformCommand::SetIndexing(true)` の後）
- 4つの `start_index_build()` 呼び出し経路をすべて列挙（初回は `main.rs:465`、first-run は `commands/window.rs:141` も確認）
- `unlistenIndexingStarted` 変数の宣言箇所の指定を追加

### 最終的な変更の最小性評価

実装コードは合計約 9行の追加のみ。既存の `indexing-complete` パターンを対称的に適用するだけで、新しい概念・状態・抽象化を一切導入しない。最小変更と判断する。
