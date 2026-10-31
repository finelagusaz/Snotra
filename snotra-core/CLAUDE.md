# snotra-core

UI 非依存の純ロジック lib crate（Win32 を使うモジュールもある）。新規ロジックは可能な限りここへ置き、ユニットテストで守る。

各項目は編集の瞬間に守る指示である。理由・経緯・測定値は書かず、正本（`//!` / `///` / `docs/adr/` / `PERFORMANCE.md`）を指す。

## モジュール構成

責務は各ファイルの `//!` が正本。ここはファイルの索引と、1 ファイルの `//!` に収まらない不変条件だけを置く。ユニットテストは対象モジュールの子（`tests.rs` / `tests/`、またはインラインの `mod tests`）に置き、親の private を直接読む（テストのために可視性を広げない）。

- `lib.rs`（宣言のみ）・`engine.rs`・`error.rs`・`binfmt.rs`・`window_data.rs`・`instant.rs`・`ui_types.rs`・`folder.rs`・`history.rs`・`query.rs`・`str_arena.rs`・`index_tree.rs`・`win_registry.rs`
- `autostart.rs`（テスト `autostart/tests.rs` は純粋部のみ——OS I/O は開発機の実登録を書き換える）
- `hotkey.rs` — 永続ホットキー文字列の parser はここだけに置き、設定検証・UI・Win32 platform は `ParsedHotkey` を消費する（下流へ複製しない）
- `opener.rs` — **依存方向は `config.rs` → `opener.rs` に保つ**（型は `opener.rs` に置き `config.rs` が re-export）。逆向きは `config/paths.rs` の `normalize_scan_path_key` / `normalize_extensions`（`pub(crate)`）を使う 1 本だけ
- `config.rs` と子 `config/schema.rs`・`config/paths.rs`・`config/location.rs`・`config/io.rs`・`config/migrate.rs`・`config/validate.rs`（テストは `config/tests.rs` と各子の `tests.rs`）
  - 件数パラメータの名前の索引: `appearance.visible_rows` = 可視行数 / `search.result_limit` = 検索・フォルダの結果リスト最大長（`Engine::search` と `capture_folder_list_context` の fetch_limit）/ `search.recent_limit` = 空クエリの recent 件数
- `search.rs` と子 `search/build.rs`・`search/query_plan.rs`・`search/scoring.rs`・`search/path_store.rs`・`search/footprint.rs`。テストは `search/tests/` に機能別（`mod.rs`・`common.rs`・`basic.rs`・`ranking.rs`・`incremental.rs`・`migemo.rs`・`path.rs`・`build.rs`・`performance.rs`）
  - **並列の列は添字で対応づけて持つ**: `entries` / `lower_names` / `lower_file_names` / `char_masks` / `file_name_char_masks` / `kana_lower_names` / `kana_char_masks`。すべてが `Vec` ではない（`lower_names` / `lower_file_names` は `str_arena`、表示名と `kana_lower_names` は `index_tree` の `NameArena`）。列を足すときは下の「実装前チェック（必須）」
- `indexer.rs` と子 `indexer/scan.rs`・`indexer/keys.rs`・`indexer/columns.rs`・`indexer/cache.rs`・`indexer/cache/breakdown.rs`・`indexer/path_env.rs`。共有治具は `indexer/test_support.rs` の 1 つだけ（ガードを写すと相互排除が消える）
- crate ルートの統合テスト（手元専用の計器か CI で走る検知器かは各 `//!`・計測値は `PERFORMANCE.md`）: `tests/search_frame_cost.rs`・`tests/memory_footprint.rs`・`tests/path_query_cost.rs`・`tests/match_cost.rs`・`tests/dir_stat_cost.rs`・`tests/search_allocations.rs`

## 開発ルール

- **UI 表示文字列を持たない。** エラーは `is_error: true` や専用の error 型で返し、文言は UI 層（`src-tauri/src/egui_shell/strings.rs`・`snotra-settings/src/i18n.rs`）が組み立てる。理由は UI 非依存であること（Win32 非依存ではない）
- **テストから実 `%APPDATA%\Snotra` に触れない。** 読む側: fixture に `HistoryStore::load()` を使わない（空は `HistoryStore::empty()`、内容は `HistoryStore::load_in` へ注入。食い違いは CI では緑のまま開発機でだけ出る・#963）。書く側: `Config::config_dir()` を内部で解決する製品の入口を通るテストへ記録を足すと、テストは緑のまま実ファイルを書く。dir を注入する入口（`load_cache_in` / `save_cache_sorted_in` の形）へ寄せ、`SNOTRA_CONFIG_DIR` で迂回しない（プロセス大域ゆえ並列の他テストを動かす）。検算はフルテスト前後で実ファイルが変わらないことを測る。`tests/` の `#[ignore]` 計器だけは `load()` でよい
- `#[cfg(windows)]` のテストは `#[cfg(windows)]` で囲むか `if let Some(..) =` でスキップする。`assert!(value.is_some())` のような環境前提のアサーションを書かない

## 実装前チェック（必須）

- `Ord` / `Reverse` / `BinaryHeap` を扱う変更では、`BinaryHeap` の先頭が最良/最悪のどちらかを実装前に明記する。top-k 更新を変えたら入力順を変えても結果が不変なテストを足す/直す
- `SearchEngine` にフィールドを足す前に、既存の列で代替できないか・導出できないかを検討する。導出の可否は「その読みが早期 return の前か後か」で決まる（`PERFORMANCE.md`「パスクエリ全走査のコスト — `normalized_keys` を保持するか導出するか」）
- 並列の列を足すときは同時に触る: `EntryView`・`entry_view()`・`assemble()` の `debug_assert!` と `shrink_to_fit()`。Wave 1 の文字列正規化は `compute_wave1`、Wave 2 のマスクは `compute_wave2` に足す。**`new_with_cached_masks` は `compute_wave2` をどの枝でも通らず（マスクはキャッシュから来る）、`compute_wave1` も派生文字列を持たない枝でしか通らない**——キャッシュに載る列は下の「IndexCache バージョン変更チェックリスト」も当て、載らない列は `Collapsed` / `Raw` 枝で別に建てる（`kana_for_cached` の形）。要素ごとの確保を持つ列（`Vec<String>` 等）にせず、`str_arena` / `NameArena` の表現で持つ
- kana 2 列（`kana_lower_names` / `kana_char_masks`）は、製品の経路では migemo 有効時だけ建つ。アクセスは `is_empty()` ガードを通し、構築の分岐は `compute_wave1(.., migemo_enabled)` と `new_with_cached_masks` の `CachedLower` 両 variant へ**同時に**入れる（片方だけだと migemo ON でも空になる）。migemo の切り替えは `IndexInputs` の差分による再構築で反映される
- 派生文字列の潰しは、ビットマスクを確定した**後**に行う（理由と検知器は `indexer/columns.rs` の `derive_entry_lowers` / `derive_entry_collapsed` の doc）
- `query.rs` の正規化を変えたら、タブ・全角スペース・NBSP を `' '` に揃えるテストと、全コードポイントの検知器（`normalize_for_match_*_on_every_codepoint`）を通す

## クロスモジュール不変条件

### `normalize_entry_key` の冪等性契約

- `indexer::normalize_entry_key` は**冪等**である（規則の定義は `indexer/keys.rs` の `//!` と `normalize_entry_key_into`、検知器は `migrate_normalize_keys_is_idempotent`）。依存するモジュールは `git grep normalize_entry_key` で数える
- **記録側と照合側は同じ関数を通す。別実装を足さない。** 1 バイトずれても結果は沈黙でずれる（ブーストやスコアだけが消える）。パスキーは `normalize_entry_key_into`（末尾セグメント版 `normalize_file_name_key_into` も含む）、照合用の名前とクエリは `query::normalize_for_match`、派生文字列の共有判定は `query::measure_derived_sharing` を通す。照合側の `search/path_store.rs` だけは、性能のためにパスキーの規則を再実装している（一致は `search/tests/path.rs` の `path_store_cursor_matches_*` の 2 本が守る）。パスキーの規則を変えたら、`search/path_store.rs` と `tests/path_query_cost.rs` の写しも直す。クエリ側の `path_query`（`search/query_plan.rs`）は照合用の名前とも別の正規化である
- 規則を変えるときは、依存するモジュールすべてと冪等性を確かめる

### `char_bitmask` は `query.rs` に一元化済み

`char_bitmask` 系のマスクとその派生（`name_char_mask` / `file_char_mask` / `lower_file_name`）は `query.rs` だけが定義する。`search.rs` と `indexer/columns.rs` は import して使い、手書きしない。

### incremental cache とパスクエリの非互換

`has_path_sep` のクエリでは incremental search を使わない。収集と再利用は `IncrementalCache::caches_candidates` の 1 本を通し、read 側の述語が正しさを担う（論証は `can_reuse` / `caches_candidates` の doc、却下した経路は `docs/adr/ADR-path-query-tail-top-k.md`）。

## データ永続化の注意

- シリアライザを替えるとき・**データの意味を変えるとき**は、どちらも版をバンプし旧形式のフォールバックを足す（バイト列が同じでも解釈が変われば破損と同じ）
- `deserialize_failed → save()`（デコード失敗で空データを即保存）をしない。フォールバック読み込みを先に試し、次の通常 save で新形式へ昇格させる
- **読み込み失敗は種類で扱いを分ける**（区分と各枝の保全は `config/io.rs` の `LoadOutcome` / `load_from_dir_reporting` の doc）。1 分岐を直すときは同じ `match` の兄弟分岐の保全方針も揃える
- TOML フィールドを別 struct へ移すときは削除せず `#[serde(default, skip_serializing)] pub field: Option<T>` で残し、`apply_migrations()` で `take()` して移す。`Config::default()` の明示初期化に `field: None` を足す。部分的なテストでも他の移行の副作用を踏まえてアサーションを組む
- serde 表現（variant・`untagged` / `flatten` / `tag`）を変えるときは、旧形式が deserialize できるテストを新形式の往復とは別に足す。旧形式が parse に失敗すると `.bak` 退避で全設定が消え、`apply_migrations()` では救えない。旧形式は `Legacy { .. }` variant 等で受理する
- オンディスク struct を触るときは（「バイト形式不変」を主張する場合も）、後方互換を**旧形式の凍結バイト列**からの load で証明する。新コードの出力を golden にしても forward-stability しか示さない。形式を変える計画は、着手前に最小 spike で往復バイト一致を確かめてから建てる

## history.rs のキー正規化に関するチェックリスト

パスキーの形式を変えたら 3 者を同時に揃える（片方だけ直すと互換性バグが残る）:

1. 新規記録（`record_launch` / `record_folder_expansion`）で正規化しているか
2. 既存データ移行（`load()` 内 `migrate_normalize_keys`）で全キーを正規化しているか
3. 外部向け参照 API（`get_global_stats` / `query_count_normalized` 等）が正規化を内部で完結させているか

クエリキーは `query::normalize_history_query_key` を使う。raw なマップを返して呼び出し側にキー形式を知らせる pub API を作らず、`query_count_normalized` の形で包む。

## IndexCache バージョン変更チェックリスト

`indexer/cache.rs` の `IndexCache` にフィールドを足すときは全部を揃える（1 つ欠けるとキャッシュヒット時とミス時で結果が違う）:

1. `IndexCache<'a>` に `Cow<'a, [T]>` で追加（save は `Cow::Borrowed`）
2. `INDEX_CACHE_VERSION` をバンプ
3. 旧スキーマを `IndexCacheVN` として残す
4. `load_cache_in` のフォールバック鎖へ枝を足し、各枝で `LoadCacheResult.version` に読めた版を入れる
5. `save_cache_sorted` に新フィールドの計算を足す
6. `CachedMasks` に `Option<T>` で追加（旧キャッシュでは `None`）
7. `SearchEngine::new_with_cached_masks()` で受け取り、`None` なら自前で計算する
8. **直前の版を受け取る側を数え直す**（版を上げた瞬間、直前の版が全ユーザーの手元の版になる）: (a) `cache_byte_breakdown_in` の鎖（`INDEX_WIRE_SINCE` の doc）、(b) `load_cache_in_reports_the_version_it_actually_read` の枝、(c) 直前の版の凍結バイト列を `load_cache_in` 経由で読むテスト（`try_deserialize_with_header` の直呼びでは枝選択を通らない）。枝の数を散文に書かない
9. golden bytes（`index_cache_on_disk_format_is_stable`）を更新する

データの意味だけを変えて版を上げるときも、8 と 9 は要る。

**版を上げずに型を替えるなら、線上表現を 1 バイトも変えない。** `names`（`NameArena`）と派生文字列 2 列（`str_arena`）がこの形で、守るのは golden ではなく `*_wire_format_is_identical_to_*` の検知器である（射程は `index_tree.rs` / `str_arena.rs` の doc）。崩すと全ユーザーが版はそのままに読めなくなる（症状は cache-miss の全走査）。

## engine.rs のロック最小化パターン

- `Config` だけは `Mutex<Engine>` の外から読める（`config_handle()`）が、書き（`update_config`）は `&mut self` のまま内側に置く。この非対称を崩さない（理由は `engine.rs` の `config` フィールドの doc）
- `FolderListContext`: ロック内でスナップショット → ロック外で I/O → ロック内で `finalize_folder_list()`
- `PrebuiltIndex`: ロック外で構築 → ロック内でスワップ。入口は `PrebuiltIndex::from_material` の 1 つ
- `PreparedHistorySave`: ロック内で snapshot → ロック外で `save()`
- 新しい `Engine` メソッドで I/O や索引構築をロック内に入れない

## index.bin 書き込みの排他（INDEX_WRITE_LOCK）

- **`index.bin` を書く新しい経路は必ず `with_index_write_lock` を経由させる**（固定 tmp 名の置換ゆえ単一書き手が前提）。`save_cache_sorted` 自身はロックを取らない
- 形式昇格の枝は読みがロックの外にあり、そのウィンドウは起動順序だけで閉じている（`INDEX_WRITE_LOCK` の doc）。ロード後に走りうる書き手を足すときは見直す

## indexer.rs の索引更新の契機

- 更新の契機と `index.bin` を書く契機は別である（`indexer/cache.rs` の `//!`）。旧版の書き戻しは `load_cache_in` の旧版枝（`upgrade_legacy_cache_in`）に置く
- **木を直に読む版（`entries` を持たない版）の枝で、木から作り直して昇格させない**——ロード直後に全エントリの複製が戻る（却下した案は `docs/adr/ADR-index-v7-cache-miss.md`）

## `scan_all` の重複排除

根ごとの役割（`root_roles`）で照合と記録を分けている（機序は `indexer/scan.rs` の `root_roles` の doc）。CI が守らない退行の射程は `Dedup::accept` の doc。

## エントリ名の導出ルール

`indexer/scan.rs` はファイルを `file_stem()`（拡張子なし）、フォルダを `file_name()` で名付ける。`folder.rs` のフォルダ内列挙は拡張子付きの `file_name()`（展開時は拡張子が手がかりになるため）。この導出は共通関数へ括り出さない（各処理に隣接しており束ねる消費者が無い・#997。別モジュールに消費者が生じたら再検討）。

## Config のデシリアライズ経路

- `Config` をデシリアライズする新しい経路（インポート・テスト用ファクトリ等）は `apply_migrations()` の要否を明示的に判断する（迂回すると旧版の移行が漏れる）
- `Option<T>` を「明示設定か」の sentinel に使うなら、`Default` は `None` を返す（`Some` だと `[search]` 不在でも `is_none()` が偽になり移行が起きない）。使う側は `effective_*()` で既定へ解決し、`get_or_insert_with` は `apply_migrations()` の最後にまとめる。既定へ戻すときは `Config::normalized_default()`（移行を通した既定値）を使う（検知器 `normalized_default_resolves_all_migration_sentinels`）
