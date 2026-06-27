# plan.md — issue #388: 件数パラメータの命名整理

## 判定: 仕様変更（config キー = 文書化された設定スキーマ）

config キー名は `SPEC.md` §7 等に記載される設定スキーマ。挙動は不変だがキー名（文書化された契約）が変わる → **SPEC 同期が必要**。`SPEC` → コード → docs の順で整合。

## 受け入れ条件（テスト可能）

1. 旧キー `max_results` / `top_n_history` / `max_history_display` を含む config.toml が読め、新フィールド `visible_rows` / `result_limit` / `recent_limit` に正しく移行される（後方互換）。
2. 新キーの config.toml が読め、保存時は新キーで書かれる（旧キーは `skip_serializing`）。
3. 旧キーと新キー両方が無い `[search]` は default に解決される（`effective_*()`）。
4. `[appearance]` の最古レガシー `top_n_history` / `max_history_display` も `result_limit` / `recent_limit` へ移行（2層レガシーの集約）。
5. `validate()` は `visible_rows == 0` で `VisibleRowsZero` を返す。
6. 挙動不変: 検索 = result_limit(200)、空クエリ recent = recent_limit(8)、可視 = visible_rows(8)。`Config::icon_cache_cap()` の派生値も不変（既定 1000）。
7. frontend bootstrap が新フィールド名で型整合し typecheck が通る。

## 実装フェーズ

### Phase 1 — snotra-core config.rs（中核）

1. **フィールド改名 + legacy 追加**（`AppearanceConfig` / `SearchConfig`）:
   - **3キーとも `Option<usize>` + `effective_*()` で統一**（Codex 指摘: 元 max_results が必須だったのは歴史的経緯。Option 統一で migration を対称化＝新キー明示時は新優先）
   - `AppearanceConfig`: `#[serde(default)] pub visible_rows: Option<usize>` + legacy `#[serde(default, skip_serializing)] pub max_results: Option<usize>`
   - `SearchConfig`: `result_limit: Option<usize>` / `recent_limit: Option<usize>`（`#[serde(default)]`）+ legacy `#[serde(default, skip_serializing)] top_n_history: Option<usize>` / `max_history_display: Option<usize>`
2. **default 関数**: `default_top_n_history`→`default_result_limit`、`default_max_history_display`→`default_recent_limit`、新規 `default_visible_rows()->8`
3. **accessor**: `effective_top_n_history`→`effective_result_limit`、`effective_max_history_display`→`effective_recent_limit`、新規 `effective_visible_rows()`（`visible_rows.unwrap_or_else(default_visible_rows)`）
4. **`Default for AppearanceConfig`（あれば）/ `SearchConfig` / `Config` / `Config::default()`**: 新フィールド初期化（`visible_rows: None`、`result_limit: None`、`recent_limit: None`、legacy: 全 None）。**CLAUDE.md 不変条件「Option sentinel の Default は None」を全 3 キーで満たす**
5. **`apply_migrations()`**（plan-review + Codex 反映。**3キー対称**）:
   - **`visible_rows`**: `let legacy = self.appearance.max_results.take(); if let Some(v) = legacy { self.appearance.visible_rows.get_or_insert(v); changed = true; }`（単層 take・visible_rows None 時のみ補完＝新優先。result_limit と対称）
   - **`result_limit`**: `let legacy = self.search.top_n_history.take().or(self.appearance.top_n_history.take()); if let Some(v) = legacy { self.search.result_limit.get_or_insert(v); changed = true; }`（2層 take・search 優先・None 時のみ補完）
   - **`recent_limit`**: 同パターン（`search.max_history_display` + `appearance.max_history_display`）
   - **末尾 `get_or_insert_with`**: 現行 `search.top_n_history.get_or_insert_with(default_top_n_history)`/`max_history_display` を**削除し**、`visible_rows.get_or_insert_with(default_visible_rows)`/`result_limit.get_or_insert_with(default_result_limit)`/`recent_limit` の**3つに置き換える**（OLD を残すと take 後 legacy が再 Some 化し skip_serializing が無効になる）
6. **`validate()`**: `appearance.max_results == 0` → `appearance.effective_visible_rows() == 0`。`Config::icon_cache_cap()`（line 762/764-772 付近）の `effective_top_n_history()`/`effective_max_history_display()`/`appearance.max_results` を新 accessor（`effective_result_limit`/`effective_recent_limit`/`effective_visible_rows`）へ（**コメント line 764-765 も新名に同期**）
7. **`error.rs` + 全参照（4箇所チェックリスト）**: `ConfigError::MaxResultsZero` → `VisibleRowsZero`。同期先: ① error.rs:67（定義）② config.rs:1048（push）③ config.rs テスト assert（`MaxResultsZero` grep、2箇所）④ snotra-settings app.rs:237（match arm）⑤ i18n.rs:152（`err_max_results_zero`→`err_visible_rows_zero`）
8. **テスト**:
   - 既存 assert の `config.appearance.max_results` → `.visible_rows`、`effective_top_n_history()`→`effective_result_limit()` 等を改名
   - 既存 `validate_max_results_zero` を **`validate_visible_rows_zero` に改名**（新規でなく rename。内部の `max_results = 0` → `visible_rows = 0`）
   - **新規 migration テスト**: `migrate_legacy_max_results_to_visible_rows` / `migrate_search_top_n_history_to_result_limit`（intermediate）/ `migrate_appearance_top_n_history_to_result_limit`（oldest）/ **`migrate_prefers_search_over_appearance_top_n_history`（両在 fixture: `[appearance] top_n_history=100` + `[search] top_n_history=200` → result_limit=200 で search 優先）** / `migrate_max_history_display_chain`
   - **round-trip**: `new_keys_save_and_reload`（新キー読込→保存）/ `legacy_keys_not_serialized`（保存後の TOML に旧キーが出ない）
9. **`engine.rs`（Codex Critical: accessor 呼び出し元）**: 本番5箇所の `effective_top_n_history()`→`effective_result_limit()`（line 116/130/172/177）、`effective_max_history_display()`→`effective_recent_limit()`（line 122）。テスト内 `config.appearance.max_results`（311/375/377/530）→ `visible_rows` 直書き（Option のため `Some(n)` か `effective_*` で読む箇所を確認）。`history.rs` の 1 参照（コメント/param）も確認

### Phase 2 — src-tauri（bootstrap / config_watcher）

10. **`commands/config.rs`**: `BootstrapAppearanceConfig.max_results`→`visible_rows`、`BootstrapPayload.top_n_history`→`result_limit`。代入元 `engine.config().appearance.max_results`→`effective_visible_rows()`、`effective_top_n_history()`→`effective_result_limit()`
11. **`config_watcher.rs`**: 差分検知のフィールドアクセス（`appearance.max_results`→`effective_visible_rows()`、`effective_top_n_history()`→`effective_result_limit()`）。**イベント名文字列 `"max-results-changed"` / `"top-n-history-changed"` は据え置き**（IPC 安定）。emit 箇所に「イベント名は IPC 安定のため旧名維持（config キーは visible_rows/result_limit）」コメントを追加
12. **`icon.rs:29`（src-tauri、Codex WARN）**: `cap` フィールド doc コメント「実効 cap は呼び出し側で `max(max_results)` に floor 済み」は **#387 で撤去済みの旧 floor を指す stale**。派生方式（`Config::icon_cache_cap()` がワーキングセットから導出）に合わせて修正（`max_results` 言及も解消）。`commands/icon.rs` の cap 取得経路自体は `cfg.icon_cache_cap()` で変更なし

### Phase 3 — snotra-settings（UI）

13. **`tabs/visual.rs:148-149`**: `tr.label_max_results()`→`label_visible_rows()`、`config.appearance.max_results`（DragValue）→ `config.appearance.visible_rows.get_or_insert(config.appearance.effective_visible_rows())`（Option 化に伴い search.rs と同じ get_or_insert パターンに）
13. **`tabs/search.rs:55-61`**: `label_max_history()`→`label_result_limit()`（:55 caller、top_n_history 相当の「最大列挙数」ラベル）、`effective_top_n_history()`→`effective_result_limit()`、`top_n_history.get_or_insert`→`result_limit.get_or_insert`、`label_max_history_display()`→`label_recent_limit()`、`max_history_display`→`recent_limit`
14. **`i18n.rs`**: メソッド名改名 `label_max_results`→`label_visible_rows`、`label_max_history`→`label_result_limit`（i18n.rs:379、result_limit 対応）、`label_max_history_display`→`label_recent_limit`、`err_max_results_zero`→`err_visible_rows_zero`（**表示テキストは全て不変**＝設定の説明文）
15. **`app.rs:237`**: match arm `ConfigError::MaxResultsZero`→`VisibleRowsZero`、`err_max_results_zero()`→`err_visible_rows_zero()`

### Phase 4 — frontend（ui）

16. **`lib/types.ts`**: `BootstrapAppearanceConfig.max_results`→`visible_rows`、`BootstrapPayload.top_n_history`→`result_limit`
17. **`MainApp.tsx:209/212`**: `bootstrap.appearance.max_results`→`.visible_rows`、`bootstrap.top_n_history`→`.result_limit`。**local signal `maxResults`・event 名 `"max-results-changed"`/`"top-n-history-changed"` は据え置き**（内部命名・IPC 安定）

### Phase 5 — docs

14. **`SPEC.md`**: §3.4(line96 `max(max_results,...)`→新名)・§7.2(line283 `max_history_display`→`recent_limit`)・§7.5(line342 `top_n_history`→`result_limit`)
15. **`snotra-core/CLAUDE.md`**: config.rs 行（line 8）の役割マップを新名へ。「#388 で改名提案中」→「#388 で改名済み」等に更新
16. **`src-tauri/CLAUDE.md`**: config_watcher の発火イベント一覧（`max-results-changed`/`top-n-history-changed`）は**据え置き**（イベント名不変）。フィールド名に言及があれば確認
    - **`PERFORMANCE.md:156` は OUT**（Codex WARN）: `### フォルダ列挙（bench_folder_*、max_results=50）` は**汎用 top-k bench パラメータ**で config キー `appearance.max_results` ではない（汎用パラメータ OUT 方針と整合）。据え置き

## 実装順序の依存
Phase 1（core）→ Phase 2/3（core に依存）→ Phase 4（bootstrap 型に依存）→ Phase 5（docs）。
- **Phase 1 後の mid-verify（必須）**: `cargo test -p snotra-core`（migration Green）→ `cargo build -p snotra -p snotra-settings`（accessor/フィールド改名の下流 compile 確認）。ここで compile-fail を全て潰してから Phase 2/3 の中身に進む（accessor 改名は下流で必ず compile-fail するため、ビルドが改名漏れ検出器になる）

## 不変条件
- **3者同時**: 新規記録（保存=新キー、skip_serializing で旧は書かない）・既存移行（apply_migrations）・外部参照（effective accessor）が揃う
- **migration は3キー対称（新優先）**: 各新フィールドは `get_or_insert(legacy)` で「新キー明示時は新優先・None 時のみ legacy 補完」。`result_limit`/`recent_limit` は2層（`search.*` ← `appearance.*`、search 優先）、`visible_rows` は1層（`appearance.max_results`）。take() で全 legacy 層をクリア
- **`get_or_insert_with(default)` の retarget**: apply_migrations 末尾で visible_rows/result_limit/recent_limit を None→Some(default) 解決（settings DragValue::get_or_insert を no-op 化、has_changes 誤発火防止）。3キー分に置き換える
- **挙動不変**: 全 effective 値・icon_cache_cap 派生・検索/recent 件数が改名前と一致
- **IPC イベント名・反応性パス不変**: emit/listen 文字列を変えない＝config 反応性は byte 一致（e2e 反応性テスト不変）
- **異常系**: legacy と new 両方が None → effective が default。3キーとも Option-with-default で統一（旧 max_results 必須→Option 化、`[appearance]` キー欠落は default(8) に解決＝benign。シナリオ「両在」でも新優先で対称）

## テスト方針

### snotra-core（必須・TDD）
- migration: `migrate_legacy_max_results_to_visible_rows` / `migrate_search_top_n_history_to_result_limit` / `migrate_appearance_top_n_history_to_result_limit`(最古) / `migrate_max_history_display_chain`
- round-trip: `new_keys_save_and_reload`（新キー書込→読込）、`legacy_keys_not_serialized`（保存後に旧キーが出ない）
- 既存 fixture（旧キー）の大半は**変更せず backward-compat テストとして残す**
- behavior: `effective_result_limit`/`effective_recent_limit`/`visible_rows` の値が改名前と一致、`icon_cache_cap()` 不変
- validate: `validate_visible_rows_zero`

### 検証コマンド（`docs/build-commands.md` カテゴリ A + B）
- `cargo clippy -p snotra-core -p snotra -p snotra-settings --all-targets -- -D warnings`
- `cargo test -p snotra-core -p snotra`
- `npm run typecheck` / `npm run build`（bootstrap 型変更）
- **カテゴリ C 非該当**（イベント名・反応性パス不変、ウィンドウ/ホットキー/スラッシュ不変）→ `e2e` ラベル不要。ただし e2e config は旧キーのまま動く（backward-compat 確認は手動 dispatch で可）

## SPEC.md 更新要否
**要**。§3.4 / §7.2 / §7.5 の3キー言及を新名へ。セクション番号の増減なし。

---

## セルフレビュー（Step 5）

### 5a. check スキル適用結果

| スキル | 適用 | 結果 |
|---|---|---|
| `/plan-review` | 実行（3 Explore 並列: migration / bootstrap・UI / scope・docs） | 設計矛盾なし。明示度向上の指摘を反映済み（get_or_insert 置換・visible_rows migration 明示・ConfigError 4箇所・2層両在テスト・validate test 改名・label_max_history→label_result_limit・Phase1 mid-verify） |
| Codex レビュー（`codex:rescue`） | 実行 | **Critical 1 + WARN 3 を反映**: ① engine.rs の effective_* 本番5箇所を Phase 1 に追加（漏れると compile-fail）② visible_rows を Option 化し migration を3キー対称に（両在で新優先）③ PERFORMANCE.md:156 は汎用 bench param＝OUT に修正 ④ icon.rs:29 の stale floor コメント（#387 で撤去済み）を修正対象に追加 |
| `/symmetric-check` | plan-review 内で実施 | 対称ペア = save(skip_serializing 旧キー)↔load(migration 旧キー)、new↔legacy フィールド、2層レガシー(search↔appearance)。plan-review agent1 が take/get_or_insert 順序と skip_serializing の整合を検証。emit↔listen は文字列据え置きで対称保持 |
| `/state-check` | N/A | UI モード・状態遷移図・ガード条件を追加しない。純粋な識別子改名 |
| `/race-check` | N/A | 新規 async 関数なし |
| `/cache-check` | N/A | incremental search/メモ化の述語単調性に非該当（config 改名） |

### 5b. セルフレビューチェックリスト

1. **対称コードパス**: save↔load（skip_serializing↔migration）、new↔legacy、2層レガシー集約を確認（plan-review 検証済み）
2. **影響範囲の網羅性**: 3キーを rust/ts/md/e2e で grep 済み。config フィールドアクセス（改名対象）と汎用 top-k パラメータ（OUT）を分離。bootstrap/config_watcher/settings/frontend/docs を列挙
3. **境界条件**: legacy+new 両 None→default、2層両在→search 優先、visible_rows 必須→default(8) 緩和、icon_cache_cap 派生不変
4. **リソース管理**: 新規フラグ・プロセス・listen なし。legacy フィールドは `skip_serializing` で保存されず次回 load で消える（migration 済みなら take で空）
5. **既存パターンとの整合**: Option+apply_migrations（既存の appearance→search 移行と同型）を踏襲。serde alias（新規パターン・toml サポート不確実）を避ける。`visible_rows` のみ非 Option（max_results が元々必須のため、各キーの現状の型を尊重）
6. **YAGNI 違反**: IPC イベント名・汎用パラメータ・歴史的設計文書・frontend local signal を OUT。最小スコープで config キー改名を完遂
7. **シンプル化の挑戦**: 各キーの現状の型（max_results=必須、他=Option）を尊重し過剰な統一を避ける。イベント名据え置きで反応性パスを byte 不変に保ち e2e ラベルを回避
8. **破壊不変条件**: ① config.toml 後方互換（旧キー読込→新フィールド。破れると既存ユーザー設定が default に潰れる→ migration テスト群で担保）② 挙動不変（effective 値・検索/recent 件数・icon_cache_cap 派生が改名前と一致→ behavior テスト）③ bootstrap lockstep（型チェックで担保）④ IPC 反応性（emit↔listen 文字列据え置きで不変）

**計画の completeness: 高 / 実装着手可否: 可**
