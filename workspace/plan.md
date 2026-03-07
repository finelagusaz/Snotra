# plan: issue #191 — 最大表示行数以上の結果をスクロールで閲覧可能にする

## 設計方針

`max_results` の責務を「ウィンドウの可視行数」に限定し、バックエンドの取得件数上限を
別定数 `SCROLL_FETCH_LIMIT` で管理する。フロントエンド（CSS・JS）はスクロール実装済みのため変更不要。

```
max_results (設定値) → ウィンドウ高さのみ制御
SCROLL_FETCH_LIMIT   → バックエンドが返す最大件数（100 固定）
```

---

## 変更ファイル一覧（2ファイル）

| ファイル | 変更内容 |
|---------|---------|
| `snotra-core/src/engine.rs` | `SCROLL_FETCH_LIMIT` 定数を追加し、`search()`・`capture_folder_list_context()`・`list_folder()` の件数上限を変更。テスト更新 |
| `SPEC.md` | §3.2 の記述は既にスクロール対応を示しているため変更不要（確認のみ） |

---

## 実装順序

### Phase 1: `snotra-core/src/engine.rs` の変更

**1a. 定数追加**（ファイル先頭付近 `use` の後）:
```rust
/// ウィンドウの可視行数（max_results）を超えてスクロールする際の検索取得上限。
/// max_results はウィンドウ高さのみを制御し、この上限まで結果を取得してスクロールで閲覧できる。
/// snotra-settings の max_results 最大値が 50 なので、常に 50 より大きい値を設定する。
const SCROLL_FETCH_LIMIT: usize = 100;
```

**1b. `Engine::search()` の変更**:
```rust
pub fn search(&mut self, query: &str) -> Vec<SearchResult> {
    let mode = SearchMode::from(self.config.search.normal_mode);
    let boost = HistoryBoostConfig::from(&self.config.search);
    let max = self.config.appearance.max_results;
    self.search_engine
        .search_with_history_boost(query, max.max(SCROLL_FETCH_LIMIT), &self.history, mode, boost)
}
```

**1c. `Engine::capture_folder_list_context()` の変更**:
```rust
pub fn capture_folder_list_context(&self) -> FolderListContext {
    FolderListContext {
        mode: SearchMode::from(self.config.search.folder_mode),
        show_hidden_system: self.config.search.show_hidden_system,
        max_results: self.config.appearance.max_results.max(SCROLL_FETCH_LIMIT),
    }
}
```

**1d. `Engine::list_folder()` の変更**（テスト用同期ラッパー）:
```rust
pub fn list_folder(&self, dir: &str, filter: &str) -> Vec<SearchResult> {
    let ctx = self.capture_folder_list_context();
    folder::list_folder(
        Path::new(dir),
        filter,
        ctx.mode,
        ctx.show_hidden_system,
        &self.history,
        ctx.max_results,  // capture_folder_list_context() 変更により SCROLL_FETCH_LIMIT 以上になる
    )
}
```
※ `list_folder` の引数 `ctx.max_results` は `capture_folder_list_context()` が既に `max(SCROLL_FETCH_LIMIT)` を適用するため、追加変更不要

**1e. テスト更新**:

既存テスト `search_respects_max_results_from_config` は `config.max_results = 2` で 4 エントリを検索し `results.len() <= 2` を検証。
変更後は `SCROLL_FETCH_LIMIT.max(2) = 100` で最大 100 件取得するため、4 件全部が返る。

変更方針: テスト名と検証内容を「max_results はウィンドウ高さを制御するが、結果件数は SCROLL_FETCH_LIMIT まで返る」に更新:
```rust
#[test]
fn search_returns_up_to_scroll_fetch_limit_regardless_of_max_results() {
    let mut config = default_config();
    config.appearance.max_results = 2;
    let mut engine = Engine::new(
        make_entries(&["app1", "app2", "app3", "app4"]),
        empty_history(),
        config,
    );
    let results = engine.search("app");
    // max_results はウィンドウ高さのみを制御する。
    // 取得件数は SCROLL_FETCH_LIMIT（100）まで許可されるため、全 4 件が返る。
    assert_eq!(results.len(), 4);
}
```

---

## 不変条件

1. `search_with_history_boost` の引数 `max_results` は常に 1 以上（`max_results == 0` の場合は config バリデーションで弾かれる。`0.max(100) = 100`）
2. フロントエンドのウィンドウ高さ計算 `cachedMaxResults * RESULT_ROW_HEIGHT + PADDING` は変更しない → ウィンドウ高は常に設定値に基づく固定高
3. `recent_history()` は `max_history_display` で制御しており `SCROLL_FETCH_LIMIT` の影響を受けない
4. `SCROLL_FETCH_LIMIT > snotra-settings の max_results 最大値 (50)` を維持する → 常にスクロール余地がある

---

## テスト方針

- `cargo test -p snotra-core`: 既存テスト + 更新テストが通ることを確認
- `cargo check -p snotra-core -p snotra -p snotra-settings`: 型チェック
- 手動確認: 検索でヒット数が `max_results` を超える場合にスクロールバーが表示されること

---

## SPEC.md 更新要否

不要。SPEC §3.2 L123 に「超過時はスクロールバーを表示する」と既に記述されており、今回の変更はその記述に合わせるバグ修正。

---

## セルフレビュー

### 1. 対称コードパス

- 通常検索 (`Engine::search`)・フォルダ閲覧 (`capture_folder_list_context`) の両方に適用 ✓
- `recent_history()` は `max_history_display` で独立管理 → 変更不要であることを確認 ✓
- `search.rs::search_with_history_boost` のシグネチャは変更しない → 呼び出し元のみ変更 ✓

### 2. 影響範囲の網羅性

- `SCROLL_FETCH_LIMIT` を使うのは `engine.rs` の3箇所のみ
- `search_respects_max_results_from_config` テストが壊れることを確認し更新計画に含めた ✓
- `folder.rs::list_folder` の直接呼び出しは `engine.rs::list_folder()` 経由のみ → 変更不要 ✓

### 3. 境界条件

- `max_results = 0`: config バリデーションで弾かれる（`config.rs:640`）。`0.max(100) = 100` のため安全
- `max_results = 50`（最大設定値）: `50.max(100) = 100` → 50 行見えて残り 50 行スクロール ✓
- エントリ数 < SCROLL_FETCH_LIMIT: 全件返る（既存の動作と同じ）

### 4. リソース管理

バックエンドの Vec サイズが増えるが、一時的なメモリ使用であり問題なし。

### 5. 既存パターンとの整合

`search_with_history_boost` は既にパラメータで上限を受け取る設計 → 新パターン不要 ✓

### 6. YAGNI 違反

新しい設定項目を追加しない。`SCROLL_FETCH_LIMIT` は固定定数 → シンプル ✓

### 7. シンプル化の挑戦

「この複雑さが必要か」: `max(max_results, SCROLL_FETCH_LIMIT)` の1行変更のみ。これ以上シンプルにできない。

### 8. 破壊不変条件

- 「ウィンドウ高さは max_results 行分に固定」の不変条件: `resultsWindowController.ts` は変更しないため維持 ✓
- `search_with_history_boost` のインターフェースは変更しない → 既存テスト（search.rs 内）は全通過 ✓
