# research.md — issue #388: 件数パラメータの命名整理

## 確定した改名（ユーザー選択: 全面整理案）

| 旧 config キー | 新 config キー | 現在の型 | 新しい型 |
|---|---|---|---|
| `appearance.max_results` | `appearance.visible_rows` | 必須 `usize` | `usize`（serde default 化） |
| `search.top_n_history` | `search.result_limit` | `Option<usize>` | `Option<usize>` |
| `search.max_history_display` | `search.recent_limit` | `Option<usize>` | `Option<usize>` |

挙動不変・後方互換マイグレーション付き。「検索 200 / recent 8」の非対称は意図された UX として維持。

## 実役割（命名の根拠）

- `max_results`(8) = **可視行数**（ウィンドウ高さ・先読み第1バッチ）。`appearance` の必須フィールド
- `top_n_history`(200) = **検索・フォルダの結果リスト最大長**（`Engine::search` / `capture_folder_list_context` の fetch_limit、`engine.rs:116/130`。「履歴」専用ではない＝誤名）。フロント `iconCacheSize`・`Config::icon_cache_cap()` がこれ由来
- `max_history_display`(8) = **空クエリ recent リスト件数**（`recent_history`、`engine.rs:122`）

## スコープ（IN / OUT の判断）

### IN — config キーの読み書きに直結する全層
1. **`snotra-core/src/config.rs`**: struct フィールド・`default_*()` 関数・`effective_*()` accessor・`Default`・`apply_migrations()`・`validate()`・テスト
2. **`src-tauri/src/commands/config.rs`**: bootstrap payload フィールド（`BootstrapAppearanceConfig.max_results`→`visible_rows`、`BootstrapPayload.top_n_history`→`result_limit`。recent_limit は bootstrap 非公開）
3. **`src-tauri/src/config_watcher.rs`**: フィールドアクセス（`new_config.appearance.max_results` 等の差分検知）と `effective_top_n_history()` 呼び出し
4. **`ui/src/lib/types.ts`**: bootstrap 型（`max_results`→`visible_rows`、`top_n_history`→`result_limit`）
5. **`ui/src/MainApp.tsx`**: bootstrap 読み取り（`bootstrap.appearance.max_results`→`.visible_rows`、`bootstrap.top_n_history`→`.result_limit`）
6. **`snotra-settings/src/tabs/visual.rs`**（max_results の DragValue）・**`tabs/search.rs`**（top_n_history / max_history_display の DragValue）
7. **識別子レベルの整合（compile-checked・低リスク）**:
   - `ConfigError::MaxResultsZero` → `VisibleRowsZero`（`error.rs` + `app.rs` match + `i18n.rs` `err_max_results_zero`→`err_visible_rows_zero` + テスト）
   - i18n メソッド名: `label_max_results`→`label_visible_rows`、`label_max_history_display`→`label_recent_limit`（**表示テキストは不変**＝設定の説明文。メソッド名のみ）
   - 注: `label_max_history`（= top_n_history 相当「最大列挙数」）は別メソッド。`result_limit` に対応するなら整合のため確認（i18n.rs:379 付近）
8. **docs（living）**: `SPEC.md`（§3.4 line 96 / §7.2 line 283 / §7.5 line 342）・`snotra-core/CLAUDE.md`（config.rs 行 line 8、先日追記分）・`PERFORMANCE.md`（line 156 ベンチ説明）

### OUT — 対象外（理由付き）
- **汎用関数パラメータ `max_results`**（`folder.rs` 24/90、`search.rs` 367/383、`engine.rs` の `FolderListContext.max_results`/`ctx.max_results`）: config フィールドではなく top-k の汎用パラメータ。`ctx.max_results` は `effective_top_n_history()` 値を保持するが命名は汎用。**pre-existing な内部命名で issue スコープ外**（YAGNI。触ると churn 爆発）
- **IPC イベント名 `max-results-changed` / `top-n-history-changed`**（config_watcher emit ↔ MainApp listen の lockstep）: 内部文字列・非永続・非ユーザー向け。改名は**最高リスク（型チェック外の文字列一致、ミスマッチで config 反映が silent break）**かつ e2e 反応性テスト（`tauri.slash.e2e.ts:705`）への波及で `e2e` ラベル必須化。config キー改名は**イベント名を据え置いても完結**する（watcher が `visible_rows` を読み既存イベント名で emit）。→ **据え置き**（emit 箇所に「IPC 安定のため旧名維持」コメント）。フロント local signal `maxResults`・ResultsSection prop も同様にコスメティック内部命名で据え置き
- **`docs/design/2026-05-31-coherence-staleset.md`**（top_n_history 6件）: 日付き歴史的設計文書＝point-in-time 記録。原文保持
- **e2e `buildE2EConfigToml`（旧キー使用）**: 旧キー `max_results`/`top_n_history`/`max_history_display` をそのまま残す。後方互換マイグレーションが効くため**動作不変＝そのまま backward-compat の生きたテストになる**。`:705` の `.replace("max_results = 8", ...)` も旧キーのまま機能（イベント名据え置きのため反応性パス不変）

→ この切り分けにより本 PR は **カテゴリ A（Rust）+ B（TS）**。IPC イベント名・反応性パスの文字列は不変なので `e2e` ラベル不要（e2e は手動 dispatch で backward-compat 確認可）。

## マイグレーション設計（proven な Option+apply_migrations を踏襲）

`toml` crate の `#[serde(alias)]` サポートの不確実性・新規パターン導入を避け、既存の `apply_migrations()` 拡張で対応する（CLAUDE.md「TOML フィールド移動」パターン）。

### `visible_rows`（旧 `max_results`、必須 → default 化）
- 新: `#[serde(default = "default_visible_rows")] pub visible_rows: usize`（`default_visible_rows()->8`）
- legacy: `#[serde(default, skip_serializing)] pub max_results: Option<usize>`
- apply_migrations: `if let Some(v) = self.appearance.max_results.take() { self.appearance.visible_rows = v; changed = true; }`（単一ソースのため無条件上書き。旧 config は visible_rows 未記載→default→legacy で上書き）
- consumers は `.visible_rows` 直アクセス（Option/accessor 不要＝settings DragValue もシンプル）
- validate: `self.appearance.visible_rows == 0` → `ConfigError::VisibleRowsZero`
- **注意**: 必須→default 化で「`[appearance]` あり・キー欠落」が parse エラーから default(8) に緩和される（より寛容。benign）

### `result_limit` / `recent_limit`（旧 `top_n_history` / `max_history_display`、Option 維持）
- 新: `#[serde(default)] pub result_limit: Option<usize>` / `pub recent_limit: Option<usize>` + `effective_result_limit()` / `effective_recent_limit()`
- legacy（intermediate）: `#[serde(default, skip_serializing)] pub top_n_history: Option<usize>` / `pub max_history_display: Option<usize>`（現フィールドが legacy 化）
- legacy（oldest）: 既存の `appearance.top_n_history` / `appearance.max_history_display`（据え置き）
- apply_migrations（2層レガシーを result_limit/recent_limit へ集約）:
  ```
  let legacy = self.search.top_n_history.take().or(self.appearance.top_n_history.take());
  if let Some(v) = legacy { self.search.result_limit.get_or_insert(v); changed = true; }
  // recent_limit も同様
  // 末尾の get_or_insert_with(default) を result_limit/recent_limit に retarget
  ```
- consumers: `effective_top_n_history()` → `effective_result_limit()`、`effective_max_history_display()` → `effective_recent_limit()`

## 既存マイグレーションの現状（config.rs:860-890）
- `appearance.top_n_history` → `search.top_n_history`（if none）/ `appearance.max_history_display` → `search.max_history_display`（if none）
- 末尾 `get_or_insert_with(default)` で None→Some(default) 解決（settings DragValue::get_or_insert を no-op 化し has_changes 誤発火防止）。retarget が必要

## 技術的制約 / リスク
- **キー/識別子形式の変更は3者同時**（新規記録=新キー書込・既存移行=apply_migrations・外部参照=effective accessor）。CLAUDE.md 不変条件
- **`Config::default()` の明示初期化・`reset_to_default()` 経路**にも新フィールド初期化を追加（CLAUDE.md 警告）
- **`Config::icon_cache_cap()`（#387 で追加）** が `effective_top_n_history()` / `effective_max_history_display()` / `appearance.max_results` を参照 → accessor/フィールド改名に同期必須
- **テスト fixture の TOML 文字列 `max_results = 8` 等は大量（config.rs 39件等）**だが、後方互換が効くため**大半は変更不要**（旧キーのまま migration テストとして機能）。新キー fixture + 旧キー migration テストを追加し、フィールドアクセス `config.appearance.max_results`（assert 等）のみ `.visible_rows` に更新

## 未解決の疑問
- なし。改名は確定、移行は proven パターン、スコープは IN/OUT を根拠付きで確定。
- （実装時の早期確認）`get_or_insert_with` retarget 後に settings `search.rs:57` の `get_or_insert` が新フィールド（result_limit）を指すこと
