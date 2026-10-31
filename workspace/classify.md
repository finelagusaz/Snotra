# snotra-core/CLAUDE.md 条項分類（2026-10-31・main ca30b1fd 時点）

行番号は現行 `snotra-core/CLAUDE.md` の行。分類が複数に跨る条項は「主分類 / 副分類」で書く。
正本の file:line はすべて grep で実在を確かめた（パスは `snotra-core/` からの相対）。⚠ は確信の持てない分類。

凡例: K=残す / D=写し（正本あり）/ H=経緯 / M=機構が守る / X=消してよい

## 冒頭

| 行 | 条項 | 分類 | 根拠・正本 | 草案での扱い |
|---|---|---|---|---|
| 3 | 「純ロジック lib crate。Win32 非依存」 | K（**偽**） | 78 行目が自ら否定（`indexer/path_env.rs` / `win_registry.rs` / `autostart.rs` がレジストリ API を呼ぶ） | 「UI 非依存の純ロジック lib crate（Win32 呼び出しは 3 ファイルに限る）」へ直す |
| 5 | 「太字 = 守る指示、後続 = 理由・経緯」の自己規定 | X | 太字 158 個でもう成り立っていない。経緯を退去すれば規定自体が不要 | 削除。草案は「各項目は守る指示。理由は正本（`//!` / `///` / ADR）を指す」の 1 行 |
| 6 | 本ファイルの構成（目次） | X | 見出しを見れば分かる | 削除 |

## モジュール構成

**前提**: `G-module-index`（`scripts/governance/checks/G-module-index.mjs`）は「モジュール構成」節にバッククォート付きの basename が在ることを**双方向**で照合する。ファイル名は M として残し（削ると赤）、説明文だけを削る。

| 行 | 条項 | 分類 | 根拠・正本 | 草案での扱い |
|---|---|---|---|---|
| 10 | `//!` が正本・本節は横断不変条件のみ（#562） | K / H | #562 は経緯 | 1 文に短縮 |
| 12–69 の各ファイル行の「（責務は `//!`）」 | M / X | G-module-index が名前を、各 `//!` が責務を持つ。「責務は `//!`」の反復 30 回は 10 行目の宣言の写し | 名前の列挙だけにする |
| 13 | `index_stale` ledger の契約は `//!` が正本 | D | `engine.rs` `//!`（文が自ら正本を指す） | 削除 |
| 16 | 件数パラメータの名前の索引（`visible_rows` / `result_limit` / `recent_limit`） | K ⚠ | `config/schema.rs` の各フィールド doc に役割があるかは未確認。#388 は H | 1 行で残す（`result_limit` が folder の fetch_limit も兼ねることは名前から推論できない） |
| 17, 24–25 | `opener.rs` との依存方向・`pub(crate)` で出す理由 | D / K | `opener.rs:4`（`//!` に依存方向あり）。**ただし `config/paths.rs:5` の `//!` が「`snotra-core/CLAUDE.md` の `opener.rs` 節」を正本として指している** | 1 行で残す（`paths.rs:5` の指しを切らないため。消すなら `paths.rs:5` を `opener.rs` の `//!` へ向け直す） |
| 19 | 読み込み失敗の扱い分けは「データ永続化の注意」が正本 | D | `config/io.rs:3-5`（`//!`）が既に `LoadOutcome` / `load_from_dir_reporting` を正本に指名 | 削除（下の 138 行を参照へ） |
| 22, 54, 61 | テストを対象モジュールの子に置く・`test_support.rs` は 1 つ・autostart は純粋部のみ | M / D | 名前は G-module-index。`indexer/test_support.rs:1-5` / `autostart/tests.rs:1-4` の `//!` が理由を持つ | 名前だけ残す。「親の private を読むので可視性を広げない」は 1 回だけ書く ⚠（推論可能ゆえ X でもよい） |
| 26 | `hotkey.rs`: 文字列 parser を下流へ複製しない | K ⚠ | `hotkey.rs` の `//!` に該当文なし（grep `複製` 0 件） | 1 行で残す |
| 28 | **並列の列は添字で対応づけて持つ**（`Vec` とアリーナの混在） | K | 注意事項の名指し項目。`search.rs:94-115` の struct doc は「parallel Vecs」とだけ言い、アリーナ混在を書いていない。額は `PERFORMANCE.md` へ D | 2 行に短縮（列名・アリーナの所在） |
| 29 | 正規化キーを索引に持たない・`with_normalized_key` が唯一の経路 | D | `search/scoring.rs:115-122`、`indexer/keys.rs`（`normalize_entry_key_into` の doc） | 「冪等性契約」節へ 1 語で統合 |
| 30 | migemo トグルは `IndexInputs` 差分の再構築経由 | D ⚠ | `engine.rs:75-77`（`IndexInputs` が単一定義・`migemo_enabled` を含む）。「`update_config` は engine を再構築しない」は doc 未確認 | 92 行と統合して 1 行 |
| 31–32, 34–35, 43, 45–53, 55–60, 62–64 | ファイル行 | M | G-module-index | 名前のみ |
| 33 | `sorted_prefix_len`・PATH マージは範囲を壊さない・測り直さない・`index.bin` は bool | D / H | `search/path_store.rs:126-128`、`index_tree.rs:737-747`（`extend_with_roots`・「測り直してはならない」と ADR-sorted-prefix-over-reflagging）、`indexer/cache.rs:179-182`（`sorted_by_path: bool`）。312,108 件・7,700〜8,900 µs は H（`PERFORMANCE.md` に既在） | 削除（`extend_with_roots` の doc が禁止を持つ） |
| 36 | 派生文字列の鎖（`lower_file_names` → `lower_names` → 表示名）・`entry_view` 1 点で読み替え | D | `search/build.rs:271-273`、`query.rs:96-101`。9.71 + 9.80 MiB は H | 削除 |
| 37 前半 | 判定を全部済ませてから落とす | D / M | `search/build.rs:275-279`、`query.rs:112-114`。検知器 `search/tests/build.rs` | 削除 |
| 37 後半 | **潰す位置はビットマスク確定より後**（`file_char_mask(None) == 0` の false negative） | K ⚠ / 要退去 | コード側に該当文なし（`build.rs` で grep 0 件） | `search/build.rs` の該当コメントへ退去を推奨。退去まで 1 行残す |
| 38 | `is_folder` から推論しない | D / M | `query.rs:116-118`、`search/build.rs:285-287`。検知器 `shared_file_name_flag_is_measured_not_inferred_from_is_folder`（`search/tests/build.rs`） | 削除 |
| 39 | v6+ でもディスク側で同じ潰し方・`LowerFileName` 3 状態 | D / H | `indexer/cache.rs` の `LowerFileName` doc（「ディスクに空きパディングは無い」）。21.63 → 1.90 MiB は H | 削除 |
| 40 | 潰し済みかを型で区別（`CachedLower` / `DerivedStrings`） | D | `search/build.rs` の `DerivedStrings` doc、`indexer/columns.rs` の `CachedLower` doc。**注意: 本文の「同じ型で渡すと `None` どうしの一致でフラグが立つ」は `DerivedStrings` の doc が「判定一元化前に固有の危険だった（2026-08-08 実験）」と否定している——CLAUDE.md 側が古い** | 削除（古い記述ごと） |
| 41 | 共有判定は `measure_derived_sharing` を通す | D | `query.rs:105-110` | 「冪等性契約」節の「同じ関数を通す」一般則に 1 語で含める |
| 42 | cache-miss 枝が記録側の潰しをそのまま使う・コンストラクタ選択・実データ検知器の射程・PATH マージの検知器・crate 内では書ける | D / M / H ⚠ | `SearchEngine::from_material` doc（「派生データの有無で分岐するのはここだけ」）、`IndexMaterial::extend_with_path_entries` doc、`search/tests/build.rs` の 2 検知器、`entry_view` 実データテストの doc（「機構としての保証は合成 fixture」）。「3 種の変異注入」「crate 内では書ける・現状は余地」は H / X ⚠ | 削除 |
| 44 | `search/tests/` の機能別配置 | M / D | 名前は G-module-index、責務は各 `//!` | 名前のみ |
| 65–69 | `tests/*.rs` 計器の説明 | M / D | 各ファイルの `//!`・計測値は `PERFORMANCE.md` | 名前と「`#[ignore]`・手元 release 専用」の 1 語 |

## 開発ルール

| 行 | 条項 | 分類 | 根拠・正本 | 草案での扱い |
|---|---|---|---|---|
| 73 | 新規ロジックはこの crate に | X ⚠ | crate の存在理由から推論可 | 削除（冒頭の 1 行に吸収） |
| 74 | `#[cfg(test)]` でテストを必ず書く | X | `AGENTS.md` ワークフロー手順 5 | 削除 |
| 75 | **`HistoryStore::load()` を fixture に使わない**（#963） | K | 検知器なし（grep で `snotra-core/tests/*` の計器以外に使用 0 件だが、守る機構は無い）。`tests/path_query_cost.rs:277` がこの規律を名指し | 短縮して残す。266–273 の「書く側」と 1 項にまとめる |
| 76 | スコア計算は `search.rs`・フォルダ列挙は `folder.rs` | X | ファイル構成から推論可 | 削除 |
| 77–78 | UI 表示文字列を持たない・理由は UI 非依存 | K | 機構なし。`.claude/rules/snotra-core.md` が「開発ルール」を名指し | 2 行に短縮 |
| 79 | `#[cfg(windows)]` テストは環境前提アサーションを書かない | K | 機構なし | 1 行 |

## 実装前チェック（必須）

| 行 | 条項 | 分類 | 根拠・正本 | 草案での扱い |
|---|---|---|---|---|
| 83 | `AGENTS.md`「事前調査」に従う | X | `AGENTS.md` は常時ロード | 削除 |
| 84 | `BinaryHeap` の先頭が最良/最悪かを明記 | K | `.claude/rules/snotra-core-search.md:24` が「実装前チェック」を名指し | 残す |
| 85 | top-k 変更は入力順不変テスト | K | 同上 | 残す |
| 86 | フィールド追加前に代替・導出を検討（早期 return の前後で判断） | K / D | 判断の額は `PERFORMANCE.md`「パスクエリ全走査のコスト — …」 | 1 行に短縮 |
| 87 | **並列の列を足すとき同時に触る場所**（`EntryView`・`entry_view()`・`assemble` の `debug_assert!` と `shrink_to_fit()`・`compute_wave1/2`） | K | 注意事項の名指し項目。`assemble` の doc は `shrink_to_fit` の合流点を持つが、触る場所の一覧はここだけ | 残す（並べ方を短く） |
| 88–91 | kana 2 列は長さ `{0, entries.len()}` の例外・`is_empty()` ガード・両パスへ同時に入れる | K / D | `search/build.rs:8`・`:327-331` が `debug_assert` 側を持つ（89 は D）。90–91 は機構なし | 2 行に短縮 |
| 92 | migemo は `IndexInputs` に含める（単一定義） | D | `engine.rs:75-77` | 1 語（30 行と統合） |
| 93 | incremental cache の述語は read/write を対で・単調性を検証 | D | `.claude/rules/snotra-core-search.md`「トリガー → 検査」と `IncrementalCache::can_reuse` の doc（`search.rs:228-245`）。#601・#894 は H | 削除（search.rs を触ると rule が配送される） |
| 94 | `query.rs` 正規化変更時は空白統一テストと冪等性テスト | M / K | 冪等性は `normalize_for_match_is_idempotent_on_every_codepoint`（`query.rs:173-175` が名指し）。空白統一の検知器名は未確認 ⚠ | 1 行 |
| 95 | `folder.rs` のソート順は `//!` が正本 | D / M | `folder.rs:3-5`、検知器 `score_entries_top_k_order_independent_of_input_order` | 削除 ⚠（`folder.rs` を触ると `//!` は目に入る） |

## クロスモジュール不変条件

| 行 | 条項 | 分類 | 根拠・正本 | 草案での扱い |
|---|---|---|---|---|
| 99–111 | **`normalize_entry_key` の冪等性契約** | K（核）/ D | 見出しは `indexer/keys.rs:6`・`SPEC.md:815`・`PERFORMANCE.md:704`・rules が指す。107 は `keys.rs` の `normalize_entry_key_into` doc と D、109 は `keys.rs:65-84` と D。冪等性は M（`migrate_normalize_keys_is_idempotent`・`history.rs`）、ASCII 高速路は M（`derives_same_bytes_as_normalize_entry_key`・`tests/path_query_cost.rs`） | 見出しを保ち 4 行: 冪等・依存 3 モジュール・「同じ関数を通す」（`normalize_entry_key_into` / `normalize_for_match` / `measure_derived_sharing` をまとめて）・変更時の確認 |
| 113–117 | `char_bitmask` 一元化・`name_char_mask` 等 3 関数 | D | `query.rs:1-7`（`//!`）、`query.rs:75-87` の doc（「Single definition shared by …」）。#437 は H | 見出しを保ち 1 行（`snotra-core-search.md:17` が指す） |
| 119–121 | hidden/system 判定の一元化 | D | `indexer/scan.rs:217-219` | 削除 |
| 123–125 | `normalize_for_match` が照合用正規化の唯一の入口（#1309 で書き換えたばかり） | D | `query.rs:3-7`（`//!`）・`query.rs:166-175`（入口の doc と検知器名）・借用判定は `fold_char` 周辺の doc（`query.rs` の「別の条件を書くと…割れる（#1310）」）。titlecase 31 字は H | 見出しを消し、99–111 の節へ 1 語で統合 ⚠（外部参照 0 件を確認済み。#1309 直後なので書き手の合意を取る） |
| 127–131 | **incremental cache とパスクエリの非互換** | D | `search.rs:228-245`（`can_reuse` の doc: read 側が正しさを担う・非パス→パス遷移）、`search.rs` の `caches_candidates` doc（`has_path_sep` 取り違え・安全性の論証）、`ADR-path-query-tail-top-k`。`PERFORMANCE.md` の額は H | 見出しを保ち 2 行（`snotra-core-search.md:17`・`PERFORMANCE.md:1544` が指す） |

## データ永続化の注意

| 行 | 条項 | 分類 | 根拠・正本 | 草案での扱い |
|---|---|---|---|---|
| 135 | シリアライザ切り替えは版バンプ＋旧形式フォールバック | K | 機構なし。外部参照多数 | 1 行 |
| 136 | 意味の変更も版バンプ | K | `ADR-index-v7-cache-miss` が名指しで引く（凍結ゆえ照合外） | 1 行 |
| 137 | `deserialize_failed → save()` 禁止 | K | `config/io.rs` のコメント（59 行付近）が名指しで引く | 1 行 |
| 138–142 | **読み込み失敗は種類で扱いを分ける** | D / K | `config/io.rs:3-5`（`//!`）・`LoadOutcome` の variant doc・`load_from_dir_reporting` の doc が 3 区分を全部持つ。残すべきは「1 分岐だけ直すと兄弟分岐に残る」の 1 文。#338/#343 は H（`io.rs:5` に既在） | 太字リードを保ち（`ADR-config-dir-env-seam-rejected-alternatives:26` が太字リードへ着地する。ADR は照合外だが壊さない）2 行 |
| 143 | TOML フィールドを別 struct へ移す手順 | K | 機構なし | 2 行 |
| 144–147 | serde 表現変更時は旧形式の deserialize テストを別に | K / H | #394 は H | 2 行 |
| 148–152 | 後方互換は旧形式の凍結バイト列で証明する | K / H | #461 は H | 2 行 |

## history.rs のキー正規化に関するチェックリスト（154–162）

| 条項 | 分類 | 根拠 | 扱い |
|---|---|---|---|
| 新規記録・既存移行・外部参照 API の 3 者を揃える | K | `SPEC.md:816` が見出しを指す。`AGENTS.md:61` の 4 点セットの源 | 見出しを保ち 4 行 |
| クエリキーは `normalize_history_query_key` に一元化 | D | `query.rs:21` | 1 語 |

## 内部キー形式の知識を漏洩させない（164–169）

| 条項 | 分類 | 根拠 | 扱い |
|---|---|---|---|
| raw map を返す pub API より encapsulated API | K ⚠ | 機構なし。history の 3 者チェックと同根 | history 節へ 1 行で統合（見出しは外部参照 0 件） |

## IndexCache バージョン変更チェックリスト（171–191）

| 行 | 条項 | 分類 | 根拠・正本 | 扱い |
|---|---|---|---|---|
| 173–183 | 7 項目 | K（核） | `indexer/cache.rs:11`（`//!`）がここを正本に指名 | 見出しを保ち、各項 1 行へ短縮（Cow の細部は D: `IndexCache` の doc） |
| 185 | **直前の版を受け取る側を数え直す**（(a)(b)(c)）・枝の数を書かない | K（核）/ H | 反復 10 は H。`load_cache_in_reports_the_version_it_actually_read`（`indexer/cache/tests.rs`）は M だが「数え直す」動作は機構外。`INDEX_WIRE_SINCE` の doc（`cache.rs:49-55`）が (a) の機序を持つ | 残す（(a)(b)(c) の 3 行） |
| 187 | 派生文字列 2 列の線上表現 | D / M | `str_arena.rs:13-25`（`//!`）、`indexer/cache.rs:216`、検知器 `*_wire_format_is_identical_to_*` 2 本 | 「線上表現を保つ型替え」1 項に統合 |
| 189 | `NameArena` の線上表現 | D / M | `index_tree.rs:63-72`・`:949-953`、検知器 `arena_wire_format_is_identical_to_vec_of_string` | 同上 |
| 191 | golden bytes を更新する | K / M / H | 検知器 `index_cache_on_disk_format_is_stable`。#461 の経緯は H | 1 行 |

## engine.rs のロック最小化パターン（193–201）

| 行 | 条項 | 分類 | 根拠・正本 | 扱い |
|---|---|---|---|---|
| 195 | `Config` は錠の外から読める・書きは内側・非対称を崩さない | D / K | `engine.rs:109-116`（`config` フィールドの doc: 「`update_config` を `&self` へ変えてはならない」）、`config_handle` / `update_config` の doc。#1128・保持時間の正本の所在は H | 見出しを保ち 1 行（`src-tauri/CLAUDE.md:100`・`race-check`・`rules/src-tauri.md` が指す） |
| 197 | `FolderListContext` | K / D | `engine.rs:5`（`//!` が 3 構築物を列挙）・`engine.rs:29`。手順（capture → 外で I/O → finalize）は doc 未確認 | 1 行 |
| 198 | `PrebuiltIndex`・入口は `from_material` 1 つ | D | `engine.rs:51-52`、`engine.rs:69` の `from_material` doc | 1 行 |
| 199 | `PreparedHistorySave` | D ⚠ | `history.rs:50-51`（短い英文 doc のみ。順序保証・`prepare_history_flush` の挙動は doc 未確認） | 1 行 |
| 201 | 新メソッドで I/O・構築をロック内で行わない | K | — | 残す |

## index.bin 書き込みの排他（203–214）

| 行 | 条項 | 分類 | 根拠・正本 | 扱い |
|---|---|---|---|---|
| 205 | 全経路が `INDEX_WRITE_LOCK` を通る・固定 tmp 名 | K（核）/ D | `indexer/cache.rs:4-5`（`//!`）・`:1043-1045` | 見出しを保ち残す（`cache.rs:347`・`tests/memory_footprint.rs:292` が指す） |
| 207–208 | 書き手の一覧 | D | `cache.rs:1044` | 削除 |
| 210 | 世代機構を #1023 で撤去した経緯 | H | `docs/adr/ADR-rescan-explicit-only.md:27` が撤去を記録 | 削除 |
| 212 | 形式昇格のウィンドウは順序で閉じている・ロード後の書き手を足すと戻る | D / K | `cache.rs:692-697`（`upgrade_save` の doc） | 1 行（「ロード後に走る書き手を足すなら昇格枝のウィンドウを見直す」） |
| 212 末尾 | プロセスをまたぐ同時起動は世代機構でも守れていなかった | H | — | 削除 |
| 213 | `save_cache_sorted` はロックを取らない（自己デッドロック） | D | `cache.rs:1045` | 1 語 |
| 214 | 新経路は `with_index_write_lock` を経由させる | K | — | 残す |

## indexer.rs の索引更新の契機（216–225）

| 行 | 条項 | 分類 | 根拠・正本 | 扱い |
|---|---|---|---|---|
| 218 | 更新の契機は明示操作 3 つ | D | `indexer/cache.rs:6-9`（`//!`）、`SPEC.md` §3.3、`ADR-rescan-explicit-only` | 1 行（見出しは `cache.rs:63`・`cache.rs:899` のコメント・rules が指す） |
| 220 | 「更新」と「書く」は別・形式昇格 | D | `cache.rs:8-9` | 上の 1 行に含める |
| 222 | `cache_load` と `total` の間に処理を足すなら `LoadOrScanStats` に項目 | D / H | `cache.rs:57-63`（「規則の正本はここ」と自称）。反復 6 の経緯は H——**`cache.rs:62-63` が経緯の在り処としてここを指している** | 削除。`cache.rs:62-63` の「経緯は CLAUDE.md が持つ」を直す必要あり（下「直す参照元」） |
| 223 前半 | 旧版の書き戻しは `upgrade_legacy_cache_in` に置く | D / M | `cache.rs` の `upgrade_legacy_cache_in` doc、検知器 `load_cache_upgrades_a_legacy_format_in_place` / `load_cache_does_not_rewrite_when_the_format_is_current`。2026-08-07・35.98 MiB は H | 削除 |
| 223 後半 | **「昇格をロード側に置いてはならない」の射程は v7（木を直読みする版）だけ**——木から作り直すと複製が戻る | K（核） | `indexer/cache.rs:896-901` のコメントと `ADR-index-v7-cache-miss` がこの規範を名指しで引く。コード側に禁止の本文は無い。62.5 MiB は H | 1 行で残す。**「v7」を「木を直に読む版（`entries` を持たない版）」へ一般化**（ADR 帰結節「次も同じ問いに当たる」と整合） |
| 224 | `save_cache_sorted` 前に `sort_entries_canonical` | D / M | `indexer/scan.rs:255-261`、検知器 `legacy_upgrade_sorts_before_saving_so_the_tree_stays_shared` | 削除 |
| 225 | 形式昇格は `built_at` を打ち直さない | D / M | `cache.rs:744-746`、検知器 `upgrade_carries_the_built_at_it_read` | 削除 |

## `scan_all` の重複排除（227–229）

| 条項 | 分類 | 根拠・正本 | 扱い |
|---|---|---|---|
| 根ごとに `check`/`record` を割り当てる・積むのは後続と重なる根だけ | D | `indexer/scan.rs:38-64`（`RootRole` / `root_roles` doc）・`:92-98`（`Dedup::accept`） | 見出しを保ち 1 行（rules が裸形で指す） |
| 検知器 2 本・両方向 | M | `scan_all_dedups_when_roots_are_nested` / `scan_all_dedups_when_the_child_root_comes_first`（`indexer/scan/tests.rs`） | 削除 |
| 「片方では捕まらない変異が在る」と書かない | X | 書き手への作文指示であり編集者を助けない | 削除 |
| **退行の射程 2 段**: `root_roles` の退行は CI（`root_roles_over_the_real_shape_leave_the_largest_root_inert`）、`Dedup::accept` の確保復活は `tests/memory_footprint.rs`（`#[ignore]`）だけが捕まえる | K / 要退去 | コード側に無い（grep `memory_footprint` 0 件 in `scan.rs`） | `Dedup::accept` の doc へ退去を推奨。退去まで 1 行残す |

## エントリ名の導出ルール（231–239）

| 条項 | 分類 | 根拠 | 扱い |
|---|---|---|---|
| ファイルは `file_stem()`、フォルダは `file_name()`、`folder.rs` は拡張子付き | K ⚠ | `indexer/scan.rs` の該当 doc を確認できず。`query.rs:116-118` が規則を前提として引用 | 2 行 |
| 共通関数へ括り出さない（#997） | K ⚠ / H | 再検討条件は K。fixture 撤去の経緯は H | 1 行 |

## Config のデシリアライズ経路（241–251）

| 条項 | 分類 | 根拠 | 扱い |
|---|---|---|---|
| 新しいデシリアライズ経路は `apply_migrations()` の要否を判断 | K | `config/migrate.rs:3-5`（`//!`）がここを指針の正本として指す | 見出しを保ち 1 行 |
| `Option<T>` sentinel: `Default` は `None`・`effective_*()`・`get_or_insert_with` は最後・`reset_to_default` も migrate | K / M | 検知器 `normalized_default_resolves_all_migration_sentinels`（`config/migrate/tests.rs:96`）、`migrate.rs:217` | 2 行 |

## Gotcha（計測の罠）（253–273）

| 条項 | 分類 | 根拠 | 扱い |
|---|---|---|---|
| 255 節の運用規則 | X | — | 削除 |
| 257–264 PS7 `Get-Process` に `ReadOperationCount` が無い・CPU 時間とスレッド数で測る | H / 要退去 | snotra-core を編集する瞬間の規則ではない（プロセス外計測の手順）。**他所に正本なし**（`git grep ReadOperationCount` は本ファイルのみ） | 退去先候補: `PERFORMANCE.md`「この文書へ記録するときの規約」近傍、または `docs/development-principles.md` の検証の節 |
| 266–272 製品の入口を通るテストに計器を足すと実 `%APPDATA%` を書く・dir 注入へ寄せる・`SNOTRA_CONFIG_DIR` で迂回しない・前後の行数で検算 | K / H | `rescan-log.jsonl` は撤去済み（`ADR-rescan-explicit-only`）ゆえ事例は H。規則は機構なし。`indexer/cache.rs` の `LegacyUpgrade` doc が「#1013 と同型」と引く | 開発ルールの `HistoryStore::load()` と 1 項に統合（読む側／書く側） |
| 273 先例の対照 | X | 統合で不要 | 削除 |

---

# 集計（主分類で数えた。1 条項の中に H が混ざる分は主分類に含む）

測り方: 行ごとの Unicode コードポイント数（`[...line].length`）を条項の行範囲で合算。見出し・空行は含めない。

| 分類 | 条項数 | 文字数 |
|---|---:|---:|
| K | 28 | 10,477 |
| D | 30 | 13,391 |
| M | 8 | 4,530 |
| H（独立した条項） | 3 | 1,016 |
| X | 6 | 571 |

**K・D の条項の中に埋まった H（issue 番号の経緯・反復 N・実測 MiB/µs・日付）はさらに数千字ある**——草案では K の中の H もすべて落とした。

- 現状 31,100 字 → 草案 9,138 字（**29.4%**）。太字 157 → 15
- 草案と旧版のバッククォート語の集合差は確認済み。落ちた識別子は「正本がコード側にある」もの（各行の D 列）と、撤去済みの語（`rescan-log.jsonl` / `/cache-check` / `IndexCacheRef` / `needs_reindex`）
- G-module-index: 旧「モジュール構成」節の `.rs` basename のうち草案に無いのは `indexing.rs` / `config_watcher.rs` の 2 つだけ（`snotra` crate のファイルで snotra-core の照合対象外）。snotra-core の全 basename は草案に在る

# 外から参照されている見出し

`git grep -n -P 'snotra-core/CLAUDE\.md`「'`。`docs/superpowers/` と `docs/adr/` は G-heading-refs の走査外（`scripts/governance/lib.mjs:539-558`）だが、ADR は壊さない方針で併記。

| 見出し | 参照元 | 草案 |
|---|---|---|
| モジュール構成 | `.claude/rules/snotra-core-search.md:17`（「の `search.rs` 行」まで指す） | 保持（並列の列は `search.rs` 行の下に残した） |
| 実装前チェック | `.claude/rules/snotra-core-search.md:24`、`.claude/rules/snotra-core.md`（裸形） | 保持（「実装前チェック（必須）」・前方一致で着地） |
| `normalize_entry_key` の冪等性契約 | `snotra-core/src/indexer/keys.rs:6`・`SPEC.md:815`・`PERFORMANCE.md:704`・rules 裸形 | 保持 |
| `char_bitmask` は `query.rs` に一元化済み | `.claude/rules/snotra-core-search.md:17`・rules 裸形 | 保持 |
| incremental cache とパスクエリの非互換 | `.claude/rules/snotra-core-search.md:17`・`PERFORMANCE.md:1544` | 保持 |
| データ永続化の注意 | `.claude/skills/persistence-check/SKILL.md:18`・`PERFORMANCE.md:814,991`・`snotra-core/src/config/tests.rs:311`・`snotra-core/src/indexer/cache/tests.rs:267,396,457`・`ADR-index-v7-cache-miss:21` | 保持 |
| 読み込み失敗は種類で扱いを分ける（太字リード。G-heading-refs は太字リードも着地先にする） | `ADR-config-dir-env-seam-rejected-alternatives:26`（照合外） | 太字リードとして保持 |
| history.rs のキー正規化に関するチェックリスト | `SPEC.md:816`・rules 裸形 | 保持 |
| IndexCache バージョン変更チェックリスト | `snotra-core/src/indexer/cache.rs:11`・`PERFORMANCE.md:1250` | 保持（下記の数の問題） |
| engine.rs のロック最小化パターン | `.claude/rules/src-tauri.md:16`・`.claude/skills/race-check/SKILL.md:16`・`src-tauri/CLAUDE.md:100` | 保持 |
| index.bin 書き込みの排他 | `snotra-core/src/indexer/cache.rs:347`・`snotra-core/tests/memory_footprint.rs:292` | 保持（括弧注記付き・前方一致） |
| indexer.rs の索引更新の契機 | `snotra-core/src/indexer/cache.rs:63`・`ADR-index-v7-cache-miss:17`・rules 裸形。散文形で `indexer/cache.rs:899` | 保持 |
| `scan_all` の重複排除 | rules 裸形のみ | 保持 |
| Config のデシリアライズ経路 | `snotra-core/src/config/migrate.rs:5` | 保持 |
| 開発ルール | rules 裸形のみ | 保持 |

**草案で消す見出し**（hidden/system・`query.rs` の正規化と `Cow<str>`・内部キー形式の知識を漏洩させない・`Option<T>` sentinel の小見出し・Gotcha とその 2 小見出し）: 正準形の外部参照は 0 件。

# 直す必要のある参照元（見出しは残るが、指している中身が消えるもの）

1. `snotra-core/src/indexer/cache.rs:62-63`（`LoadOrScanStats` の doc）——「反復 6 で実際に踏んだ経緯は `snotra-core/CLAUDE.md`「indexer.rs の索引更新の契機」が持つ」。草案は経緯を落とすので、句ごと削るか退去先を指す
2. `snotra-core/src/str_arena.rs:116`——「`snotra-core/CLAUDE.md` が「`is_folder` から推論してはならない」で戒めた」。草案から消えるので `query::measure_derived_sharing` の doc（`query.rs:116-118`）へ向け直す
3. `PERFORMANCE.md:1250`——「IndexCache バージョン変更チェックリスト」の **8 項目**。現行は 7 項目 + 番号なし段落で既にずれており、草案は 9 項目。数を消して見出しだけを指す
4. `src-tauri/src/config_watcher.rs:187`——「`Config::load` の『一時的失敗は退避も上書きもしない』保全方針（snotra-core/CLAUDE.md）」。草案はこの文を `config/io.rs` の `LoadOutcome` / `load_from_dir_reporting` の doc へ委ねるので、指し先をそちらへ
5. `snotra-core/src/config/paths.rs:5`——「依存の向きの取り決めは `snotra-core/CLAUDE.md` の `opener.rs` 節」。草案は `opener.rs` 行に残すので着地するが、`opener.rs:4` の `//!` を指すほうが写しが減る（任意）
6. `.claude/rules/snotra-core.md` の裸形「見出し」群——草案はすべて保持するので修正不要

# 退去が必要な内容（正本がまだどこにも無い）

| 内容 | 現在地 | 退去先の候補 |
|---|---|---|
| 潰す位置はビットマスク確定より後（`file_char_mask(None) == 0` で false negative） | 37 行 | `search/build.rs:271` 付近の「重複する派生文字列を落としながら列へ積む」コメント。退去すれば草案の「実装前チェック」から 1 行消せる |
| `scan_all` の退行の射程 2 段（`Dedup::accept` の確保復活は `tests/memory_footprint.rs` だけが見る・CI 外） | 229 行 | `indexer/scan.rs` の `Dedup::accept` の doc。退去すれば草案の節は 1 行になる |
| PS7 の `Get-Process` に `ReadOperationCount` が無い・走査の進行は `TotalProcessorTime` と `Threads.Count` で測る・「計器が黙っている ≠ 対象が止まっている」 | 257–264 行 | `PERFORMANCE.md`「この文書へ記録するときの規約」の近傍か `docs/development-principles.md` の検証の節。snotra-core の編集者を助ける規則ではない |
| 「木を直に読む版の枝で昇格させない」の本文 | 223 行 | コード側に禁止の本文が無く、`indexer/cache.rs:896-901` と ADR がこの CLAUDE.md を規範として引く。草案では K で残した。コードへ移すなら `load_cache_in` の v7 分岐コメント |
| 反復 6 の経緯（ロード直後の全エントリ複製が計測に現れなかった） | 222 行 | `PERFORMANCE.md` に既在か未確認 ⚠。無ければ `PERFORMANCE.md` へ |
| 「プロセスをまたぐ同時起動は世代機構でも守れていなかった」 | 212 行 | `ADR-rescan-explicit-only` に無い ⚠。今も真の残余（`INDEX_WRITE_LOCK` はプロセス大域）なので `INDEX_WRITE_LOCK` の doc へ 1 文 |

# ⚠ 一覧

1. 3 行「Win32 非依存」は 78 行と矛盾する偽の記述——草案で直した
2. 40 行「同じ型で渡すと `None` どうしの一致でフラグが立つ」は `search/build.rs` の `DerivedStrings` doc が「判定一元化前に固有の危険だった（2026-08-08 の実験で検知器は落ちなかった）」と否定している。CLAUDE.md 側が古い（草案では削除）
3. 123–125 行（`normalize_for_match`）は #1309 で書き換えたばかり。D と判定して見出しを消し「冪等性契約」節の 1 語へ統合した。書き換えた当人の意図（独立の節）を確かめたい
4. 16 行の件数パラメータの索引: `config/schema.rs` のフィールド doc に同じ対応があるか未確認
5. 26 行 `hotkey.rs` の「parser を複製しない」: `//!` に無いと判断（grep `複製` 0 件）したが、別の語で書かれている可能性
6. 30 行「`update_config` は engine を再構築しない」: doc での裏取りをしていない（草案は「`IndexInputs` の差分による再構築で反映」とだけ書いた）
7. 94 行の空白統一テストの検知器名を確かめていない
8. （解消）199 行 `PreparedHistorySave`: 順序保証は `history.rs:47-78`（`NEXT_SAVE_SEQUENCE` / `HISTORY_WRITE_STATE` で古い sequence の書き込みを捨てる）がコードで持ち、`dirty_count` を見ない理由は `history.rs:201-205`（`prepare_flush` の doc）が持つ。D で確定。ただし sequence 比較の側に「なぜ」の doc は無い（英文 1 行のみ）
9. 233–239 行のエントリ名導出: `indexer/scan.rs` の doc に規則の本文が無い（コードのみ）ので K で残した
10. 22 行「親の private を読むので可視性を広げない」は推論可能とみて X 寄り。草案では 1 文で残した
11. 42 行「実データ全件の相方は `#[ignore]` かつ自己スキップ・規模と実際のパスの形は手元でしか検証されない」: テスト doc は「機構としての保証は合成 fixture」とだけ言い、何が CI で守られないかの列挙はコード側に無い。削除したが、残余の明示として価値があるならテストの doc へ退去
