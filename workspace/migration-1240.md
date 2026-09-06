# migration-1240 — 動かした bullet の対応表（元の行 → 再確立地点）

判定の凡例: **単一** = `.rs` へ新規に書いた / **写し** = `.rs` に既在（CLAUDE.md 側だけ消し、差分があれば足した）/ **索引** = 行を残し本文だけ消した / **経緯** = 履歴ゆえ捨てた（`#NNN` が持つ）。L 番号は変更前（main `d2e9804`）のファイル行。

## snotra-core/CLAUDE.md「モジュール構成」

| 元 L | 文の先頭 | 判定 | 再確立地点 | 足した差分 |
|---|---|---|---|---|
| 13 | `engine.rs` — …以下は engine ロックに閉じる横断コヒーレンシ | 索引 | 行を残し、ledger の正本を `//!` と明記 | — |
| 14 | `IndexInputs` には「変わったら索引を建て直さねばならない入力」だけ | 写し | `engine.rs` `IndexInputs` の `///`（L66〜77・`show_icons` を外した経緯まで同文） | なし |
| 15 | config 変更と index 再構築のコヒーレンシ判断は `index_stale` ledger に閉じる | 単一 | `engine.rs` `//!`（新設段落: 4 メソッドの列挙・Mutex 内側が lost-update を塞ぐ根拠・#347/#348-A・消費者と `AtomicBool` の役割） | 全文 |
| 16 | `complete_index_drain` は snapshot == 現在のときだけ stale をクリア | 写し | `engine.rs` `complete_index_drain` の `///`（L299〜301） | なし |
| 17 | crate の外から見える名前は `config.rs` の re-export が決める | 写し | `config.rs` `//!` L4〜6 | なし |
| 19 | 新しいセクション・設定キーには serde の既定を付ける（#824） | 写し | `config/schema.rs` `//!` L6〜7（太字で既在） | 検知器名 2 種（`empty_section_deserializes_to_default_*` / `config_parses_with_all_sections_omitted`）・配列要素の例外・`SPEC.md`「13.1 設定データ」の指し。「射程と検知器は CLAUDE.md」の委ね文を消した |
| 21 | `Config::icon_cache_cap()` はこれらから派生 | 写し | `config/schema.rs` `icon_cache_cap` の `///`（L475〜479 `保守注意:`） | なし |
| 24 | 新しい永続ファイルの保存先も `Config::config_dir()` から導く（#803） | 写し | `config/location.rs` `//!` L3〜4・`config_dir` の `///` L31〜32 | 全称「`dirs::config_dir()` を直接呼ぶ箇所は他に無い」・2 経路の欠陥への指し・判定核が env を読まない代償・結線を pin する検知器名 |
| 25 | env 上書きはそのまま使い `Snotra` を付け足さない | 写し | `config/location.rs` `config_dir_from` の `///`（bullet 自身が「契約の全文は rustdoc」） | Windows の `dirs` 同一性の実測（`config_dir()` = `data_dir()`・危険なのは `*_local_dir()`）を `//!` に 1 文追加（レビュー Medium 2 で欠落が判明し追記） |
| 28 | 旧キーの後方互換移行 | 写し | `config/migrate.rs` `//!` L7〜8・`apply_migrations` 本体の行末コメント | なし |
| 29 | migration を足すときは系統ごとの private fn として書く | 写し | `config/migrate.rs` `//!` L7 | なし |
| 30 | migration の呼び出し順は元と同一に固定し、依存は行末コメントが正本 | 写し | `config/migrate.rs` `//!` L8 | なし（「実測: 責務分割のレビューまで…」は経緯ゆえ捨てた） |
| 31 | `additional` → `scan` の追加が正規化より先であることは検知器が守る | 単一 | `config/migrate.rs` `apply_migrations` の `///`（新設段落） | 全文 |
| 32 | `validate.rs` は検出だけを行い、補正はしない | 写し | `config/validate.rs` `//!` L3 | なし |
| 40 | `kana_lower_names` をアリーナへの逐次 push で組み直してはならない（#1056） | 単一 | `search/build.rs` `new_with_cached_masks` の `///`（`不変条件:` 段落） | 全文（検知器 `kana_column_survives_chunked_parallel_merge`・`KANA_CHUNK` の注意を含む） |
| 42 | `kana_lower_names` / `kana_char_masks` は `migemo_enabled` のときのみ構築 | 単一（`//!` L8 に一部） | `search/build.rs` `//!`（新設段落） | 空ガード（`kana_available` / `kana_char_masks.is_empty()`）と窓の panic 防止 |
| 44 | パスマッチング（スコア式・`has_path_sep` で pre-filter スキップ） | 単一 | 式: `search/scoring.rs` `PATH_BASE` の `///`（式は既在・フォールバック条件を追加）/ スキップ: `search/query_plan.rs` `//!`（新設段落） | フォールバック条件・スキップの理由と計器の指し |
| 45 | スコアリング・順位計算は `search/scoring.rs` へ置く（列挙） | 索引 | `search/scoring.rs` `//!` L1〜7（責務・階層・親に残すもの） | なし（構造の列挙は写し） |
| 46 | クエリ計画の純粋導出は `search/query_plan.rs` へ置く（列挙） | 索引 | `search/query_plan.rs` `//!` L1〜7 | なし |
| 48 | `target_path` は索引に持たず、フォルダ木から組み立てる（2 系統・セグメント比較禁止・v7） | 索引（写し） | `search/path_store.rs` `//!` L3〜9・`# 組み立ての 2 系統`・検知器名 `search_result_order_is_stable_…` も既在 | なし |
| 49 | 索引の構築処理は `search/build.rs` へ置く（唯一の入口） | 索引（写し） | `search/build.rs` `//!` L1〜9・`from_material` の `///` | なし |
| 56（一部） | 木と派生データは `indexer::IndexMaterial` が組のまま運ぶ…同じそこを通る | 写し | `indexer.rs` `//!` L4〜7・`extend_with_path_entries` の `///`（L99〜101） | なし（bullet の残り——検知器 3 本と crate 内の余地——は横断ゆえ残した） |
| 57 | 常駐ヒープの内訳は `search/footprint.rs` が数える（構築前 Vec を走査しない・`..` を書かない） | 索引（写し） | `search/footprint.rs` `//!` L6〜17 | なし。`search/tests/performance.rs:103` と `indexer/cache/breakdown.rs:167` の指し先を CLAUDE.md から `footprint.rs` の `//!` へ付け替えた |
| 60 | 剪定容量 `top_n` は焼き込まず引数で受け取る（live-read） | 写し | `history.rs` `//!` L3〜4・`load` の `///` L82〜84 | なし |
| 61 | `HistoryStore` に `top_n` フィールドを再導入しないこと | 単一 | `history.rs` `//!`（1 文追加） | 全文 |
| 63 | crate の外から見える名前は `indexer.rs` の re-export が決める | 写し | `indexer.rs` `//!` L9〜11 | なし |
| 72 | `str_arena.rs` 線上表現は `Vec<Option<String>>` のまま・守るのは `*_wire_format_is_identical_to_*` | 索引（写し） | `str_arena.rs` `//!`「線上表現は変わっていない」節 | なし |
| 73 | `index_tree.rs` 辿る規則はここが唯一持つ | 索引（写し） | `index_tree.rs` `//!` L14 | なし |
| 77 | `autostart.rs` 状態の正本は OS であり `Config` にフィールドを持たない | 索引（写し） | `autostart.rs` `//!` L4〜7 | なし |
| 79 | `instant.rs` 公開関数の署名・契約と変数展開の中核は `//!` と各 `///` を正とする | 索引 | `instant.rs` `//!` / `///`（bullet 自身が正本を指していた） | なし |
| 83（後半） | `normalized_keys` を保持するか導出するかの差を測る唯一の計器 | 単一 | `tests/path_query_cost.rs` `//!` | 1 文 |
| 84 | `measure_path_query_frame_cost_at_operating_point` は実起動の経路を再現する（#1067） | 単一 | `tests/path_query_cost.rs` `//!`（新設段落） | 全文（数値は幅で書き、正本は `PERFORMANCE.md`） |

### 残した太字 bullet（横断・名前の索引）と根拠

| 元 L | 文の先頭 | 残す理由 |
|---|---|---|
| 20 | 件数パラメータ | L5 が宣言する「名前の索引」 |
| 35〜36 | 依存方向は `config.rs` → `opener.rs` | 2 ファイルの向き。`config/paths.rs:5` の `//!` がこの節を指す |
| 37 | `HotkeyConfig` は re-export・parser を下流へ複製しない | 消費者が settings / platform（別 crate） |
| 39 | 並列の列は添字で対応づけて持つ | `str_arena` / `index_tree::NameArena` を名指す。`rules/snotra-core-search.md` L17 が指す |
| 41 | 正規化キーは索引に持たない | `scoring.rs` ↔ `indexer/keys.rs`。「`normalize_entry_key` の冪等性契約」節が正本 |
| 43 | migemo トグルの反映は index 再構築経由 | `engine.rs` ↔ `snotra` crate の `config_watcher` |
| 47 | 整列は「先頭から何件までか」で持つ | `index_tree` / `path_store` / `indexer/cache` |
| 50〜56 | 派生文字列の共有鎖 | `build` / `indexer/columns` / `query` / `indexer` |
| 58 | ユニットテストは `search/tests/` へ | ファイル一覧（6 basename の唯一の言及） |

### 付け替えた指し先

| ファイル:行 | 前 | 後 |
|---|---|---|
| `snotra-core/tests/path_query_cost.rs:3` | `snotra-core/CLAUDE.md`「モジュール構成」の search.rs 節 | `snotra-core/src/search/query_plan.rs` の `//!` |
| `snotra-core/src/search/tests/performance.rs:103` | `snotra-core/CLAUDE.md` の footprint 節 | `search/footprint.rs` の `//!` |
| `snotra-core/src/indexer/cache/breakdown.rs:167` | `snotra-core/CLAUDE.md` の search.rs 節 | `search/footprint.rs` の `//!` |
| `snotra-core/src/config/schema.rs:7` | 射程と検知器は `snotra-core/CLAUDE.md` | 自身の `//!`（検知器名を取り込んだ） |
| `docs/design/2026-05-31-coherence-staleset.md:15` | 正本は `snotra-core/CLAUDE.md`「モジュール構成」 | `snotra-core/src/engine.rs` の `//!` と `IndexInputs` の doc |

## src-tauri/CLAUDE.md「モジュール構成」

| 元 L | 文の先頭 | 判定 | 再確立地点 | 足した差分 |
|---|---|---|---|---|
| 11 | 背景再スキャンの spawn と結果適用は #1001 で撤去済み | 写し | `main.rs` `//!` L4〜5・`ADR-rescan-explicit-only` | なし |
| 13 | インデックスビルドの開始/終了は `try_begin_index_build()` / `finish_index_build()` 経由 | 写し（同文書内） | `src-tauri/CLAUDE.md`「実装パターン」の同文（`rules/src-tauri.md` L12 が指す）・`state.rs` の両メソッドの `///` L106〜123 | なし（索引側を消して 1 か所に） |
| 15 | `invalidate_icon_cache` はメモリ内と `icons.bin` を単一 lock 内で無効化 | 写し | `icon.rs` `//!` L5〜6・`invalidate_icon_cache` の `///`・`invalidate_icon_cache_with` の doc（実測 17/2000 回まで既在・L200/L493） | なし |
| 18 | ビルド本体は `catch_unwind` で包む（panic 戦略依存） | 写し | `indexing.rs` `//!` L5・`start_index_build` 本体の catch_unwind 直前のコメント（unwind / abort の両戦略まで同文） | なし |
| 19 | finish 後に `is_index_stale` を再チェック・unwind の panic 経路では再 kick しない | 単一 | `indexing.rs` `start_index_build` の `///`（新設段落） | 全文 |
| 20 | コヒーレンシは engine の `index_stale` ledger に一元化 | 写し（同文書内） | 同節 L14（残した横断 bullet） | なし |
| 22 | `LoadOutcome::ReadFailed` では適用せず早期 return | 写し | `config_watcher.rs` `should_apply_config_change` の `///` L179〜186 | `//!` の「跨る不変条件は CLAUDE.md」を「読込失敗の保全は本ファイルが正本」へ反転 |
| 23 | 早期 return の前にバウンドリトライ（既定 3 回 × 150ms） | 写し | `config_watcher.rs` `load_with_read_failed_retry` の `///` L212〜219・定数 `CONFIG_READ_RETRY_MAX` / `CONFIG_READ_RETRY_BACKOFF` | `//!` に定数への intra-doc link を足した（数を散文に写さない） |
| 30 | `heap_trace.rs` 既定ビルドには入らない・欄の不在は「測っていない」 | 索引（写し） | `heap_trace.rs` `//!` L3〜4 | 「欄の不在が『測っていない』であり 0 ではない」の 1 句を `//!` へ追加（レビュー Low 4） |
| 31 | `startup.rs` 終端を `RegisterInitialHotkey` の arm だけに閉じない | 索引（写し・bullet 自身が「正本は `//!`」） | `startup.rs` `//!`（失敗経路と一度きり性の節） | なし |
| 32 | `monitor.rs` 基準モニターは必ず点から決める | 索引（写し） | `monitor.rs` `point_monitor_work_area` の `///` L77〜86 | なし |
| 41 | 起動の入口は `launcher_controller/` の直下に置くこと | 写し（`//!` の方が新しい） | `activation.rs` `//!` L3〜13（「集める規範は要らなくなった」・受容する死角まで） | なし |
| 50 | `layout.rs` の撤去済み一覧（#646 PR2 / #752 / #835） | 経緯 | `#646` / `#752` / `#835`・`ADR-results-fixed-height`（現在形は `layout.rs` `//!`） | なし |

### 残した太字 bullet（横断）と根拠

| 元 L | 文の先頭 | 残す理由 |
|---|---|---|
| 14 | コヒーレンシ判断を 2 AtomicBool から導かない | `state.rs` ↔ `snotra-core` `engine.rs`（crate 越し） |
| 17 | `start_index_build` の順序・`build_index_from_material` に閉じる・起動経路は別 | `indexing.rs` ↔ `main.rs` の 2 経路 |
| 24 | アイコンキャッシュの破棄は `update_config` より後 | `config_watcher.rs` / `icon.rs` / `commands/icon.rs` |
| 25 | index 再構築の要否は `IndexInputs::from_config` の差分 | `config_watcher.rs` ↔ `engine.rs` |
| 58〜62 | `window_coordinator.rs` の 4 規則 | `window_coordinator` / `layout` / `notify` / `view` / `monitor`。参照 5 本が「の項」を名指す |
| 65〜114 | `###` 7 項 | L36 が「横断不変条件は本節の `###` 各項」と宣言済み |

### 付け替えた指し先

| ファイル:行 | 前 | 後 |
|---|---|---|
| `src-tauri/src/config_watcher.rs:5` | 多サブシステムに跨る不変条件は `src-tauri/CLAUDE.md` を正とする | 読込失敗の保全は本ファイル、跨るもの（破棄の順序・再構築判定）は CLAUDE.md |

## 文字数（コードポイント・CR 除く）

| ファイル | 前 | 後 |
|---|---|---|
| snotra-core/CLAUDE.md | 38,664 | 30,573 |
| src-tauri/CLAUDE.md | 31,782 | 29,496 |
