# plan: issue #191 — 最大表示行数以上の結果をスクロールで閲覧可能にする

## 設計方針

`max_results` の責務を「ウィンドウの可視行数」に限定し、バックエンドの取得件数上限は
既存の `top_n_history`（履歴保持件数）で制御する。`top_n_history` は本来
検索結果に表示されるアイテムの上限として設計された設定値。
フロントエンド（CSS・JS）はスクロール実装済みのため変更不要。

```
max_results   (設定値: 1..=50, デフォルト 8)    → ウィンドウ可視行数のみ制御
top_n_history (設定値: 10..=1000, デフォルト 200) → バックエンドが返す最大件数
```

---

## 変更ファイル一覧（2ファイル）

| ファイル | 変更内容 |
|---------|---------|
| `snotra-core/src/engine.rs` | `search()`・`capture_folder_list_context()` の件数上限を `top_n_history` に変更。テスト更新 |
| `SPEC.md` | §3.2 の記述は既にスクロール対応を示しているため変更不要（確認のみ） |

---

## 実装順序

### Phase 1: `snotra-core/src/engine.rs` の変更

**1a. `Engine::search()` の変更**:
```rust
pub fn search(&mut self, query: &str) -> Vec<SearchResult> {
    let mode = SearchMode::from(self.config.search.normal_mode);
    let boost = HistoryBoostConfig::from(&self.config.search);
    let fetch_limit = self.config.appearance.top_n_history;
    self.search_engine
        .search_with_history_boost(query, fetch_limit, &self.history, mode, boost)
}
```

**1b. `Engine::capture_folder_list_context()` の変更**:
```rust
pub fn capture_folder_list_context(&self) -> FolderListContext {
    FolderListContext {
        mode: SearchMode::from(self.config.search.folder_mode),
        show_hidden_system: self.config.search.show_hidden_system,
        max_results: self.config.appearance.top_n_history,
    }
}
```

**1c. `Engine::list_folder()`**: `capture_folder_list_context()` 経由で `ctx.max_results` を使うため追加変更不要。

**1d. テスト更新**:

既存テスト `search_respects_max_results_from_config` は `config.max_results = 2` で 4 エントリを検索し `results.len() <= 2` を検証。
変更後は `top_n_history`（デフォルト 200）で取得するため 4 件全部が返る。

```rust
#[test]
fn search_returns_up_to_top_n_history_regardless_of_max_results() {
    let mut config = default_config();
    config.appearance.max_results = 2;
    // top_n_history はデフォルト 200 → 4 件全部取得できる
    let mut engine = Engine::new(
        make_entries(&["app1", "app2", "app3", "app4"]),
        empty_history(),
        config,
    );
    let results = engine.search("app");
    // max_results はウィンドウ高さのみ制御する。
    // 取得件数は top_n_history（デフォルト 200）まで許可されるため、全 4 件が返る。
    assert_eq!(results.len(), 4);
}
```

---

## 不変条件

1. `top_n_history >= 1`（range 10..=1000 かつ config バリデーション済み）→ `max_results == 0` バリデーションエラー経路と同様に安全
2. フロントエンドのウィンドウ高さ計算 `cachedMaxResults * RESULT_ROW_HEIGHT + PADDING` は変更しない → ウィンドウ高は常に `max_results` に基づく固定高
3. `recent_history()` は `max_history_display` で別途制御 → 影響なし
4. `top_n_history >= max_results` が常に保証される必要はない（例: 最小設定 10 と max_results=8 の差は 2 行しかない）が、それ自体はユーザーの設定選択であり、システムの不変条件には影響しない

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

- `top_n_history` を検索上限に使う箇所は `engine.rs` の 2 箇所のみ
- `search_respects_max_results_from_config` テストが壊れることを確認し更新計画に含めた ✓
- `folder.rs::list_folder` の直接呼び出しは `engine.rs::list_folder()` 経由のみ → 変更不要 ✓

### 3. 境界条件

- `top_n_history = 10`（最小設定値）かつ `max_results = 8`: スクロールで最大 10 件 → 差分は 2 行のみだが動作は正しい
- `top_n_history = 1000`（最大設定値）: 最大 1000 件取得 → パフォーマンスへの影響は検索アルゴリズムの O(N log k) 特性から許容範囲
- エントリ数 < `top_n_history`: 全件返る（既存の動作と同じ）

### 4. リソース管理

取得件数増加に伴う一時的な Vec サイズ増大は問題なし。

### 5. 既存パターンとの整合

`search_with_history_boost` は既にパラメータで上限を受け取る設計 → 新パターン不要。`top_n_history` は既存の設定値を活用するため新規フィールド追加なし ✓

### 6. YAGNI 違反

新しい設定項目・定数を追加しない。既存の `top_n_history` を本来の目的に使う ✓

### 7. シンプル化の挑戦

変更量は `engine.rs` 2 行 + テスト更新のみ。これ以上シンプルにできない。

### 8. 破壊不変条件

- 「ウィンドウ高さは `max_results` 行分に固定」の不変条件: `resultsWindowController.ts` は変更しないため維持 ✓
- `search_with_history_boost` のインターフェースは変更しない → 既存テスト（search.rs 内）は全通過 ✓
