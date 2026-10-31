# plan-review 独立導出 — issue #1327（snotra-core/CLAUDE.md を「編集の瞬間に守る指示」だけに縮める）

- 対象: `snotra-core/CLAUDE.md`（ブランチ chore/core-claude-slim の作業ツリー・274 行）
- 制約: `workspace/` の他ファイル（plan.md 等）は読んでいない。計画とは独立の導出である
- 記号: K = 残す（編集の瞬間に要り、他に正本が無い）/ D = 正本が他にある（参照へ置換か削除）/ M = テスト・governance が守る / H = 経緯・測定値（削る）/ X = 偽・不要 / ⚠ = 確信が持てない

---

## 5. 変更が必要なファイルとシンボル（結論を先に）

### 本体
- `snotra-core/CLAUDE.md` — 全面書き直し

### `.rs`（すべてコメント・doc だけ。挙動は変えない）
| ファイル:行 | 何を直すか | 理由 |
|---|---|---|
| `snotra-core/src/indexer/cache.rs:881`（`load_cache_in` 内のコメント） | 「`Raw` にすると `None` どうしの一致でフラグが立つ」を、「測り直しは無駄」の表現へ | 偽（§4-2）。`search/build.rs:29-36` の `DerivedStrings` の doc が否定 |
| `snotra-core/src/indexer/cache/tests.rs:331-332`（`frozen_v6_bytes_load_as_collapsed_through_load_cache_in` の doc） | 同上 | 同上 |
| `snotra-core/src/search/build.rs:250`（`assemble` 内のコメント） | 「ここで測り直すと `None` どうしの一致で誤ったフラグが立つ」 | 同上（同じファイルの 29-36 行と自己矛盾） |
| `snotra-core/src/indexer/cache.rs:63`（`LoadOrScanStats` の doc） | 「反復 6 で実際に踏んだ経緯は CLAUDE.md『indexer.rs の索引更新の契機』が持つ」 | 経緯を消すと偽になる。経緯の参照ごと消すか PERFORMANCE.md を指す |
| `snotra-core/src/indexer/cache.rs:900`（`load_cache_in` の v7 コメント） | 「『indexer.rs の索引更新の契機』が戒める複製が戻る」 | 裸形参照。新版で「v7 の枝では昇格しない／木から作り直すと複製が戻る」を残さないなら着地先が消える（`docs/adr/ADR-index-v7-cache-miss.md` を既に隣で指しているのでそちらへ寄せられる） |
| `snotra-core/src/indexer/cache.rs:418`（`load_or_scan_with_stats_in` の doc） | 「#1013 の Gotcha」 | 散文で Gotcha 節を指す。Gotcha 節を移すなら移転先を指す |
| `snotra-core/src/indexer/cache.rs:522, 605` / `indexer/columns.rs:102` | 「（→『index.bin 書き込みの排他』）」裸形 | 見出しを残すなら不要。改名・統合するなら要修正（governance は見ない） |
| `snotra-core/src/str_arena.rs:116` | 「`snotra-core/CLAUDE.md` が『`is_folder` から推論してはならない』で戒めた」 | その文言を消すなら、正本 `search/tests/build.rs:792` の `shared_file_name_flag_is_measured_not_inferred_from_is_folder` か `query.rs` / `search/path_store.rs`（同じ語句が在る）を指す |
| `snotra-core/src/config/io.rs:59` | 「（snotra-core/CLAUDE.md: deserialize_failed → save() はデータ喪失を招く）」 | 文言依存。規則を残すなら字面を合わせ、消すなら `io.rs` 冒頭 `//!`（正本宣言あり）を指す |
| `snotra-core/src/config/paths.rs:5` | 「依存の向きの取り決めは CLAUDE.md の `opener.rs` 節」 | モジュール索引の opener.rs 行の依存方向記述に依存。残すか、`opener.rs` の `//!` へ移して指す |
| `snotra-core/src/indexer/keys.rs:6`（`//!`） | 「冪等性の契約と依存する 3 モジュールの一覧は CLAUDE.md『…』」 | 「3 モジュール」が偽（§4-5）。数を写さず、一覧を持たない形へ。あわせて `//!` 1 行目の「小文字化 + `/` → `\`」に `trim` が欠ける（§4-6） |
| `snotra-core/tests/memory_footprint.rs:292` | 「昇格の機序は『index.bin 書き込みの排他』」 | 指し先違い（昇格は「indexer.rs の索引更新の契機」）。縮小と独立に直す |
| `src-tauri/src/config_watcher.rs:187` | 「`Config::load` の『一時的失敗は退避も上書きもしない』保全方針（snotra-core/CLAUDE.md）」 | 正本は `config/io.rs` の `LoadOutcome::ReadFailed` と `Config::load_from_dir_reporting` の doc（io.rs:1-5 が自ら正本宣言）。そちらを指す |

### `.md` / スキル / rule
| ファイル:行 | 何を直すか |
|---|---|
| `.claude/rules/snotra-core.md:12-18, 24` | 裸形の見出し参照 8 種（「実装前チェック（必須）」「`normalize_entry_key` の冪等性契約」「history.rs のキー正規化に関するチェックリスト」「`char_bitmask` は `query.rs` に一元化済み」「index.bin 書き込みの排他」「indexer.rs の索引更新の契機」「`scan_all` の重複排除」「開発ルール」）。**governance は照合しない**（対象綴りが無い）。見出しを 1 字でも変えたら手で直す。L8「事実の正本は `snotra-core/CLAUDE.md` とコード」も、正本をコードへ寄せるなら書き換え |
| `.claude/rules/snotra-core-search.md:17` | 「`モジュール構成` の `search.rs` 行」に並列 Vec レイアウトを期待している。search.rs 行を索引だけにするなら、`SearchEngine` の struct doc を指す |
| `.claude/rules/snotra-core-search.md:24` | 「実装前チェック」— 見出しを残せば着地は続く（前方一致） |
| `.claude/skills/persistence-check/SKILL.md:18` | 「SSOT は CLAUDE.md『データ永続化の注意』」— 実は SKILL 自身が同内容をほぼ全部持つ（§2 参照）。どちらを正本にするか決める |
| `.claude/skills/persistence-check/SKILL.md:92-95`（コードフェンス内） | `CLAUDE.md「IndexCache バージョン変更チェックリスト」`（対象が裸の `CLAUDE.md` ＝ルートに解決される綴り）と、**括弧内の 7 項目の写し**。`CLAUDE.md「history.rs のキー正規化チェックリスト」` は**既にラベルが実在の見出しと違う**（実見出しは「…に関するチェックリスト」）。フェンス内ゆえ governance は見ない |
| `PERFORMANCE.md:1250` | 「IndexCache バージョン変更チェックリスト」の **8 項目**（数の写し）。数を消す |
| `PERFORMANCE.md:704` | 「冪等性契約」が「ASCII 限定の別実装を名指しで禁じている」に依存。「迂回する正規化比較を書かない」を残すなら着地は保たれる |
| `PERFORMANCE.md:814, 991` | 「データ永続化の注意」の「最小 spike で往復バイト一致を実証してから」に依存 |
| `SPEC.md:815-816` | 「正規化ステップの内訳は CLAUDE.md『冪等性契約』／『history.rs のキー正規化…』を正本とする」。内訳をコード（`indexer/keys.rs` の `normalize_entry_key_into`・`query.rs` の `normalize_history_query_key`）へ寄せるなら SPEC の指し先も変える（SPEC は語彙層ゆえ関数を指すかは要判断 ⚠） |
| `AGENTS.md:61` | 「history キーは `snotra-core/CLAUDE.md`」— 見出し無しの指し。チェックリストを残すなら変更不要 |
| `.claude/agents/code-reviewer.md:14` | 「モジュール固有の不変条件・**過去のバグパターン**を把握する」— 経緯を消すと後半が空振り ⚠（直すかは任意） |
| `docs/architecture.md:55` / `docs/development-principles.md:36` | 「モジュール構成は CLAUDE.md」「責務の集約先は各サブディレクトリのドキュメント」— 責務は既に `//!` が正本（CLAUDE.md:10 自身がそう言う）。development-principles.md:36 は既存の不一致 ⚠ |

ADR（`docs/adr/ADR-config-dir-env-seam-rejected-alternatives.md:26`・`ADR-index-v7-cache-miss.md:17,21`・`ADR-measurement-canon-in-code-doc.md:44-45`・`ADR-path-query-forward-single-pass.md:15`・`ADR-stale-identifier-detector-scope.md:144,186`）は凍結された歴史ゆえ直さない（`ADR-adr-frozen-history`）。`docs/superpowers/` も同様。

---

## 3. この文書を指している参照と governance の照合範囲

### 3a. 正準形 `` `snotra-core/CLAUDE.md`「…」 ``（node で `HEADING_REF` と同じ正規表現を当てて抽出・docs/superpowers・workspace 除く）

| 参照元 | ラベル | 中身への依存 |
|---|---|---|
| .claude/rules/snotra-core-search.md:17 | モジュール構成 / `char_bitmask` は `query.rs` に一元化済み / incremental cache とパスクエリの非互換 | search.rs 行の並列 Vec 記述 |
| .claude/rules/snotra-core-search.md:24 | 実装前チェック | BinaryHeap 先頭明記・入力順不変テスト |
| .claude/rules/src-tauri.md:16 | engine.rs のロック最小化パターン | 抽出 → 即解放 |
| .claude/skills/persistence-check/SKILL.md:18 | データ永続化の注意 | SSOT 宣言 |
| .claude/skills/race-check/SKILL.md:16 | engine.rs のロック最小化パターン | 不変条件の正本宣言 |
| src-tauri/CLAUDE.md:100 | engine.rs のロック最小化パターン | 「書き込みは engine lock の内側に残す理由」 |
| PERFORMANCE.md:704 | `normalize_entry_key` の冪等性契約 | 別実装禁止 |
| PERFORMANCE.md:814, 991 | データ永続化の注意 | 最小 spike |
| PERFORMANCE.md:1250 | IndexCache バージョン変更チェックリスト | **「8 項目」＝数の写し** |
| PERFORMANCE.md:1544 | incremental cache とパスクエリの非互換 | 不変条件 |
| SPEC.md:815 | `normalize_entry_key` の冪等性契約 | 正規化ステップの内訳の正本宣言 |
| SPEC.md:816 | history.rs のキー正規化に関するチェックリスト | 同上（クエリキー） |
| snotra-core/src/config/migrate.rs:5 | Config のデシリアライズ経路 | apply_migrations 判断の指針 |
| snotra-core/src/config/tests.rs:311 | データ永続化の注意 | 後方互換の向き |
| snotra-core/src/indexer/cache.rs:11 | IndexCache バージョン変更チェックリスト | 項目 |
| snotra-core/src/indexer/cache.rs:63 | indexer.rs の索引更新の契機 | **反復 6 の経緯**（H を消すと偽） |
| snotra-core/src/indexer/cache.rs:347 | index.bin 書き込みの排他 | 固定 tmp 名の食い合い |
| snotra-core/src/indexer/cache/tests.rs:267, 396, 457 | データ永続化の注意 | 凍結バイト列の向き |
| snotra-core/src/indexer/keys.rs:6 | `normalize_entry_key` の冪等性契約 | **「依存する 3 モジュールの一覧」** |
| snotra-core/tests/memory_footprint.rs:292 | index.bin 書き込みの排他 | **指し先違い**（昇格の機序） |
| docs/adr/…（3 件） | 読み込み失敗は種類で扱いを分ける / indexer.rs の索引更新の契機 / データ永続化の注意 | 走査元外（凍結） |

### 3b. 物理改行で折れた正準形
- `docs/adr/ADR-measurement-canon-in-code-doc.md:44-45`（ADR・走査元外）のみ確認。生きた層には無かった（grep `snotra-core/CLAUDE.md\`$` 相当で目視）。

### 3c. 裸形・散文形（governance は照合しない）
- `.claude/rules/snotra-core.md:10-18, 24`（裸形 8 種 + 「`snotra-core/CLAUDE.md` の該当節」）
- `snotra-core/src/indexer/cache.rs:522, 605`、`indexer/columns.rs:102`: 「→『index.bin 書き込みの排他』」
- `snotra-core/src/indexer/cache.rs:900`: 「『indexer.rs の索引更新の契機』が戒める複製」
- `snotra-core/src/indexer/cache.rs:418`: 「#1013 の Gotcha」
- `snotra-core/src/str_arena.rs:116`: 「`snotra-core/CLAUDE.md` が『`is_folder` から推論してはならない』で戒めた」
- `snotra-core/src/config/io.rs:59`: 「snotra-core/CLAUDE.md: deserialize_failed → save() はデータ喪失を招く」
- `snotra-core/src/config/paths.rs:5`: 「`snotra-core/CLAUDE.md` の `opener.rs` 節」
- `src-tauri/src/config_watcher.rs:187`: 「（snotra-core/CLAUDE.md）」
- `.claude/skills/persistence-check/SKILL.md:92-95`（フェンス内・裸の `CLAUDE.md` が対象）
- `AGENTS.md:61`・`docs/architecture.md:55`・`docs/development-principles.md:36`・`.claude/agents/code-reviewer.md:14`（文書全体を指す）

### 3d. 数の写し
- `PERFORMANCE.md:1250`「8 項目」
- `persistence-check/SKILL.md:93` 括弧内の 7 項目列挙
- `snotra-core/src/indexer/keys.rs:5-6`「依存する 3 モジュール」（しかも既に偽）
- CLAUDE.md 内の自己写し: L89「他 5 Vec」（偽・§4）、L101/111「3モジュール」（偽）、L109「派生が 1 つある」（⚠）、L195「他に 3 つ」、L210「書き手 3 つ」

### 3e. governance の照合範囲（`scripts/governance/lib.mjs`・`checks/`）
- **G-heading-refs**（`checks/G-heading-refs.mjs`）: 走査元は `headingRefDocs`（`.md` 全部から `docs/superpowers/`・`workspace/`・`docs/adr/` を除く）＋ `.rs` コメント（`headingRefSourceDocs`）＋ `.mjs/.ps1` 等のコメント。照合するのは**正準形**（`HEADING_REF`: バッククォートの対象の直後に `「…」`、1 物理行内、入れ子 1 段まで）で、**対象綴りが `isRefTargetSpelling`（`.md` / `.mjs` / `/skill`）に当たるものだけ**。着地は `collectAnchors`（ATX 見出し・番号付き項目・**太字リードの箇条**・describe/it）を `normAnchor`（`` ` `` `*` 「」 空白を除去）したものへの**前方一致**。→ 「実装前チェック」は「実装前チェック（必須）」へ前方一致で着地する。**見出しを消しても同名の太字リード箇条が残れば着地してしまう**点に注意（逆に、見出しを太字リードへ格下げしても赤にならない）。コードフェンス内は `refScanLines` が除く。
- **見ない**: 裸形（対象綴り無し）・散文形（「〜の Gotcha」「CLAUDE.md の `opener.rs` 節」）・**参照元が依存している中身**（見出しが残れば中身が消えても緑）・数の写し・ADR と docs/superpowers からの参照。
- **G-near-heading-refs**: 対象と「」の間に助詞が挟まる形を、**着地する場合だけ**正準形へ促す。着地しなければ黙る。
- **G-folded-heading-refs**: 折れた正準形を赤にする（射程は同ヘッダ）。
- **G-module-index**（`checks/G-module-index.mjs`）: `snotra-core/CLAUDE.md` の `^## モジュール構成$` 節（**次の見出しで閉じる必要がある**＝最終節にしてはならない）について、(順) 節内のバッククォート `*.rs/ts/tsx/html` の basename が実在、(逆) `snotra-core/src/**/*.rs` の**全 basename（テスト `tests.rs`・`mod.rs`・`test_support.rs` 含む・59 本）**が節内に `` `base` `` か `` /base` `` で現れる。**索引行の所有関係は見ない**。→ 縮めても**ファイル名は 1 つも落とせない**。`tests/*.rs`（crate ルート統合テスト）は `src/` 外ゆえ逆方向の母集団外（書いてあれば順方向で実在照合のみ）。
- **G-module-linkage**: `mod` 到達性（CLAUDE.md とは無関係）。
- **G-references**: `governanceDocs` に `snotra-core/CLAUDE.md` が入る——**この文書内に書いた** `/` 入りバッククォートパス（拡張子が `REF_EXTENSIONS`）の実在を見る。新版でパスを書くならここで照合される。
- **G-stale-identifiers**: 母集団（`staleIdentifierTargets`）に**モジュール CLAUDE.md は入らない**（`ADR-stale-identifier-detector-scope.md:186` が明記）。→ 新版に書いた識別子が腐っても鳴らない。
- **G-adr-citations**: 新版で `ADR-<slug>` を引くなら照合される（母集団は `adrCitationDocs`）⚠ 母集団に module CLAUDE.md が入るかは未確認。

---

## 1. 現行の規則の列挙と判定

（行番号は現行 `snotra-core/CLAUDE.md`。テスト・関数の実在は `git grep "fn <name>"` で確認済み）

### 冒頭・構成（L1-6）
| # | 規則 | 判定 | 正本 / 守り手 |
|---|---|---|---|
| 1 | L3「純ロジック lib crate。**Win32 非依存**」 | **X（偽）** | L78 が自ら否定。`snotra-core/Cargo.toml:24` が `[target.'cfg(windows)'.dependencies] windows`。`docs/adr/ADR-autostart-state-ownership.md:109` も否定済み |
| 2 | L5「太字 = 守る指示、後続 = 理由」の書式宣言 | X（新版の書式に置換） | — |
| 3 | L6 構成の目次 | X | — |

### モジュール構成（L8-69）
| # | 規則 | 判定 | 正本 / 守り手 |
|---|---|---|---|
| 4 | ファイル名の一覧そのもの | **M（必須で残す）** | G-module-index が全 basename を要求 |
| 5 | L10「責務は各 `//!` が正本」 | K（1 行） | — |
| 6 | L13 engine.rs `index_stale` ledger の契約は `//!` | D | `engine.rs` `//!` |
| 7 | L16 件数パラメータの対応（`visible_rows` / `result_limit` / `recent_limit`） | D ⚠ | `config/schema.rs:263-268` `effective_result_limit` 等・`engine.rs:170-176`（`capture_folder_list_context` が `effective_result_limit`）。名前の索引ゆえ残す価値は低い |
| 8 | L17/L23-25 **依存方向は `config.rs` → `opener.rs`**、re-export で既存パス維持、`normalize_opener_target` が `paths.rs` の `pub(crate)` を使う | K（短縮）⚠ | `config.rs:36` の `pub use crate::opener::{…}`。`config/paths.rs:1-5` の `//!` が半分を持つが「取り決めは CLAUDE.md」と指している。`opener.rs` の `//!` へ移せば D 化できる |
| 9 | L19 読み込み失敗の扱い分けの正本は「データ永続化の注意」 | **X（正本宣言の衝突）** | `config/io.rs:1-5` の `//!` が「区分は `LoadOutcome`、各枝は `Config::load_from_dir_reporting` の rustdoc が正本」と宣言している |
| 10 | L22/L54 ユニットテストは対象モジュールの子に置く（private を読むため可視性を広げない） | K（1 行に統合） | — |
| 11 | L26 hotkey 文字列 parser を下流へ複製しない（`ParsedHotkey` を消費） | K ⚠ | `hotkey.rs:3` `//!` に変換の記述はあるが「複製しない」は無い |
| 12 | L28 並列の列は添字で対応づけ（+ アリーナの内訳・「比例確保は 1 つも残っていない」） | D + H | `SearchEngine` の struct doc（`.claude/rules/snotra-core-search.md` も struct doc を正本に挙げる）。額は PERFORMANCE.md |
| 13 | L29 **正規化キーは索引に持たず `with_normalized_key` で導出。正規化比較を別実装で書き起こさない** | K（§冪等性契約へ統合） | `search/scoring.rs:121-130`・`indexer/keys.rs:19-23` が同文を持つ |
| 14 | L30 migemo トグルは index 再構築経由（`IndexInputs` 差分） | D | `engine.rs:87` `IndexInputs`、`src-tauri/src/config_watcher.rs:104` |
| 15 | L33 整列は `sorted_prefix_len`（範囲）で持つ・`index.bin` は bool のまま・「マージ後に測り直す」形にしない | D + H | `search.rs:521-532`（`sorted_prefix_len` doc）・`PathStore::cmp_paths` doc（`search/path_store.rs:277`）。額は PERFORMANCE.md「採用: 整列している**範囲**を捨てない」 ⚠「マージ後に測り直してはならない」は doc に明示されているか未確認 |
| 16 | L36 重複する派生文字列を鎖で共有（数値含む） | D + H | `search/build.rs:270-285`（`assemble`）、`indexer/columns.rs:45` |
| 17 | L37 **判定を全部済ませてから落とす／潰す位置はビットマスク確定より後** | D | 前段: `search/build.rs:275-279`。後段: `indexer/columns.rs:175-180`（`derive_entry_lowers`: 「マスクは潰す前の完全な文字列から導出」・compute_wave2 側の順序も名指し）。検知器 `indexer/columns/tests.rs:131` `derived_masks_come_from_the_uncollapsed_strings`。**メモリ側（Wave2→assemble）は構造で順序が決まる**ため孤立した知識ではない |
| 18 | L38 **`is_folder` から推論しない** | M | `search/tests/build.rs:792` `shared_file_name_flag_is_measured_not_inferred_from_is_folder`。同趣旨が `query.rs`・`search/path_store.rs`・`str_arena.rs` にある |
| 19 | L39 `index.bin`（v6）でも同じ潰し方・3 状態 `LowerFileName` | D + **X（版リテラル）** | `indexer/columns.rs:45-50` `LowerFileName` doc。`cache.rs:43-46` が版リテラル禁止 |
| 20 | L40 潰し済みは型で区別・**同じ型で渡すとフラグが立つ** | **X（理由が偽）** | `search/build.rs:29-36` `DerivedStrings` doc・`PERFORMANCE.md`「潰し済みかどうかを型で区別する（理由は「壊れるから」ではなく「無駄だから」）」 |
| 21 | L41 **派生の共有判定は `query::measure_derived_sharing` を通す** | K（短縮）/D | `search/build.rs:281-284` が同文。`query.rs:120` |
| 22 | L42 cache-miss 枝が記録側の導出を使う・分岐は `SearchEngine::from_material` 1 か所・テスト群・PATH マージ | D + M + H | `search/build.rs:401-404` `from_material` doc、`engine.rs:66-69`。テスト `search/tests/build.rs:187` `save_side_collapse_and_assemble_measurement_agree_at_entry_view`・`:430` `path_merge_after_cache_miss_agrees_with_deriving_over_the_extended_tree`・`:510` `path_merge_extends_the_tree_even_without_derived_data`。`IndexMaterial::extend_with_path_entries`（`indexer.rs:104`）doc |
| 23 | L44 search テストは `search/tests/` へ機能別 | M（ファイル名）/ X（説明） | G-module-index |
| 24 | L61-62 autostart テストは純粋部のみ（OS I/O は実スタートアップを書き換える） | D | `autostart/tests.rs:1-3` の `//!` が同内容を持つ |
| 25 | L65-69 crate ルート統合テスト（`#[ignore]` ハーネス） | D | 各 `//!`。`tests/dir_stat_cost.rs` は**索引に無い**（`src/` 外ゆえ G-module-index は鳴らない）⚠ |

### 開発ルール（L71-79）
| # | 規則 | 判定 | 正本 / 守り手 |
|---|---|---|---|
| 26 | L73 新規ロジックはこの crate に | K（1 行） | — |
| 27 | L74 `#[cfg(test)]` でユニットテストを書く | X（一般則） | — |
| 28 | L75 **テスト fixture に `HistoryStore::load()` を使わない**（`empty()` / `load_in`）。計測ハーネスだけは可 | **K（実害あり #963・issue 指定）** | `history.rs:94` `load`・`:112` `empty`（`pub(crate)`）・`:117` `load_in`。機械的な守り手無し |
| 29 | L76 検索スコア計算は search.rs、フォルダ列挙は folder.rs | X / D | `//!` |
| 30 | L77 **UI 表示文字列を持たない** | K | `config/io.rs:12` `LoadOutcome` doc 等に断片。`.claude/rules/snotra-core.md:18` が「開発ルール」を指す |
| 31 | L78 Win32 非依存ではない（表示文字列規範とは独立） | K（冒頭 #1 の訂正として 1 行） | `Cargo.toml:24` |
| 32 | L79 `#[cfg(windows)]` コードのテストは環境前提アサーションを避ける | K | — |

### 実装前チェック（L81-95）
| # | 規則 | 判定 | 正本 / 守り手 |
|---|---|---|---|
| 33 | L83 共通原則は AGENTS.md | X | — |
| 34 | L84 **`BinaryHeap` の先頭が最良/最悪かを明記** | K | `.claude/rules/snotra-core.md:24`・`snotra-core-search.md:24` がこの節を指す |
| 35 | L85 **top-k 変更時は入力順不変テスト** | K + M | `search/tests/ranking.rs:22` `search_top_k_is_independent_from_input_order`・`:254` `topk_merge_is_order_independent`・`:293` |
| 36 | L86 `SearchEngine` にフィールドを足す前に再利用・**導出**を検討（早期 return の前後で判断） | K（短縮）+ D | PERFORMANCE.md「パスクエリ全走査のコスト — `normalized_keys` を保持するか導出するか」 |
| 37 | L87 **並列 Vec 追加時に `EntryView`・`entry_view()`・`assemble` の `debug_assert!` と `shrink_to_fit()`・`compute_wave1/2` を同時更新** | **K（実害あり・issue 指定）** | `search/scoring.rs:143` `EntryView`・`:342` `entry_view`・`search/build.rs:192-336` `assemble`。検知は `search/tests/build.rs`（余剰容量） |
| 38 | L88-91 **kana 2 列は長さ `{0, len}` の例外・`is_empty()` ガード・両パスに同時に入れる** | D（大半）/ K（「両パスに同時に」） | `search/build.rs:11-15` の `//!` と `:328-334` の `debug_assert`。⚠ L89「他 **5** Vec」は偽（§4-4） |
| 39 | L92 migemo は `IndexInputs` に含める（単一定義） | D | `engine.rs:87-117` |
| 40 | L93 **incremental cache の read（`can_reuse`）と write（`update`）を対で変更** | D | `search.rs:179-276`（`IncrementalCache`・`caches_candidates`・`can_reuse`）・`.claude/rules/snotra-core-search.md` |
| 41 | L94 `query.rs` 正規化変更時は空白類統一テストと冪等性テスト | M | `query.rs:311/318/325` `requires_alloc_for_tabs/fullwidth_space/nbsp`・`:521` `normalize_for_match_is_idempotent_on_every_codepoint` |
| 42 | L95 `folder.rs` ソート順変更時は `//!` の構造を崩さず入力順不変テストを通す | D + M | `folder.rs` `//!`・`folder.rs:657` `score_entries_top_k_order_independent_of_input_order` |

### クロスモジュール不変条件（L97-131）
| # | 規則 | 判定 | 正本 / 守り手 |
|---|---|---|---|
| 43 | L99-111 **`normalize_entry_key` の冪等性契約・定義は `normalize_entry_key_into` 1 つ・迂回する正規化比較を書かない・変更時は依存先と冪等性テストを確認** | **K（実害あり・issue 指定・外から 4 件参照）** | `indexer/keys.rs:14-30`（同文あり）。テスト: `history.rs:838` `migrate_normalize_keys_is_idempotent`（⚠ 入力が既に正規化済みのキーで、`normalize_entry_key` の 2 回適用を直接は測っていない）・`tests/path_query_cost.rs:89` `derives_same_bytes_as_normalize_entry_key`（**実 index が無ければ自己スキップ＝CI では守らない**）。依存先の一覧は偽（§4-5） |
| 44 | L109 末尾セグメント版 `normalize_file_name_key_into` も別実装しない | D | `indexer/keys.rs:56-84` |
| 45 | L113-117 `char_bitmask`・`name_char_mask`・`file_char_mask`・`lower_file_name` は `query.rs` に一元化 | D ⚠ | `query.rs:54-136`（bits 0-25/26-35 は `query.rs` doc に一致・`#[inline]` も一致）。「一元化済み」は事実記述で指示性は弱い。ただし外から正準形 1 件・裸形 1 件の参照あり＝見出しは残す必要 |
| 46 | L119-121 hidden/system 判定は `is_hidden_or_system` 1 つ | D / X | `indexer/scan.rs:219`、`folder.rs:11` が import。経緯（#437 逆極性）は H |
| 47 | L123-125 **照合用の正規化の入口は `normalize_for_match` 1 つ・借用判定は写像に「恒等か」を問う形** | D | `query.rs:167-175`。検知器 `normalize_for_match_agrees_with_the_owned_reference_on_every_codepoint`・`normalize_for_match_is_idempotent_on_every_codepoint` |
| 48 | L127-131 **`has_path_sep` 時は incremental 無効・収集と再利用は `caches_candidates` 1 本・read 側が正しさを担う** | D + M | `search.rs:192-246`（ほぼ全文が doc にある）。検知器 `search/tests/incremental.rs:208` `path_query_results_are_identical_to_a_fresh_engine`（`can_reuse` doc が変異注入で実測と記載）。却下経路は `docs/adr/ADR-path-query-tail-top-k.md`。外から正準形 2 件 → 見出しは残す |

### データ永続化の注意（L133-152）
| # | 規則 | 判定 | 正本 / 守り手 |
|---|---|---|---|
| 49 | L135 シリアライザ切替は版バンプ + 旧形式フォールバック | K | `persistence-check/SKILL.md` Step 2 が同内容 |
| 50 | L136 **意味を変えるときも版バンプ** | K | 同 SKILL Step 2・ADR-index-v7-cache-miss が引用 |
| 51 | L137 **`deserialize_failed → save()` で学習データを潰さない** | K（`config/io.rs:59` が文言参照） | SKILL Step 4 |
| 52 | L138-142 **読み込み失敗は種類で扱い分け（不在/破損/一時的）・兄弟分岐を揃える** | **K（実害 #338/#343・issue 指定）だが正本は D** | `config/io.rs:1-5`・`:14-23` `LoadOutcome`・`Config::load_from_dir_reporting`。テスト `config/io/tests.rs:150` `load_from_dir_parse_failure_backs_up_and_does_not_save`・`:177` `…missing_file_is_first_run_and_saves_default`・`:234` `…invalid_utf8_is_backed_up`・`:284` `…transient_read_error_leaves_file_intact`。→ 新版は「兄弟分岐を揃える」指示 + 正本ポインタで足りる |
| 53 | L143 TOML フィールド移動は `skip_serializing` で残し `apply_migrations` で移す・`Config::default()` に `field: None` | K | SKILL Step 5 が一部。`config/migrate.rs:24` `migrate_additional_to_scan` が実例 |
| 54 | L144-147 **serde 表現変更は旧形式 deserialize テストを別に追加**（untagged `Legacy`） | K | SKILL Step 3 |
| 55 | L148-152 **オンディスク struct のリファクタは旧形式の凍結バイト列 load テストで証明・最小 spike を先に** | K（外から 5 件が文言依存） | SKILL Step 3。`cache/tests.rs` の `GOLDEN_V*` |

### history.rs のキー正規化チェックリスト（L154-162）
| 56 | **新規記録・既存移行・参照 API の 3 者を揃える／クエリキーは `normalize_history_query_key`** | **K（SPEC.md:816・AGENTS.md:61・rule が参照）** | `query.rs:24-31`、`history.rs:385` `migrate_normalize_keys`・`:260` `get_global_stats`・`:282` `query_count_normalized`。SKILL Step 5 も 3 者を写す |

### 内部キー形式の知識を漏洩させない（L164-169）
| 57 | raw なキー形式を強制する pub API を使わず/作らず encapsulated API を使う | K ⚠ | 悪例の `get_query_stats` は**実在しない**（`git grep "fn get_query_stats"` 0 件）。良例 `query_count_normalized`（`history.rs:282`）は実在。悪例は経緯ゆえ削れる |

### IndexCache バージョン変更チェックリスト（L171-191）
| 58 | L173-183 **7 項目**（struct・版・`IndexCacheVN`・`load_cache()` の鎖と `LoadCacheResult.version`・`save_cache_sorted()`・`CachedMasks`・`new_with_cached_masks()`） | **K（実害・issue 指定・外から 2 件＋SKILL）** | 機構無し。`cache.rs:47` `INDEX_CACHE_VERSION`。⚠ 7 は `SearchEngine::new_with_cached_masks` が `pub(crate)` で `from_material` 経由になった今も有効か要確認 |
| 59 | L185 **版を上げたら直前の版を受け取る側を数え直す**（`cache_byte_breakdown_in` の鎖・`load_cache_in_reports_the_version_it_actually_read`・直前版の凍結バイト列を `load_cache_in` 経由で読むテスト）・枝の数を散文に書かない | **K（issue 指定）** | `cache/breakdown.rs:259`・`cache/tests.rs:1413`・`:326-336` `frozen_v6_bytes_load_as_collapsed_through_load_cache_in`。⚠ (b) の理由「この値だけが昇格判定の入力」は**偽**（§4-3） |
| 60 | L187 派生 2 列の線上表現・`IndexCacheV6` も列の型で読む | D + M + X（版リテラル） | `cache.rs:186-197`・`:215-220`。テスト `str_arena.rs:730` `lower_name_column_wire_format_is_identical_to_vec_of_option_string`・`:797` `lower_file_column_wire_format_is_identical_to_vec_of_lower_file_name` |
| 61 | L189 **`NameArena` の `Serialize/Deserialize` を触ると版そのままで全ユーザーが cache-miss** | K（短縮）/ D | `cache.rs:163-170`（`names` フィールド doc が同内容）・`index_tree.rs:961` `arena_wire_format_is_identical_to_vec_of_string` |
| 62 | L191 golden bytes `index_cache_on_disk_format_is_stable` を版バンプ時に更新 | M + K（1 行） | `cache/tests.rs:150`・`cache.rs:152-158` |

### engine.rs のロック最小化パターン（L193-201）
| 63 | **`Config` の読みは錠の外、書き（`update_config`）は `&mut self`。非対称を崩さない** | K（外から 4 件参照）/ D | `engine.rs:116` `config` フィールド doc・`:258-262` `update_config` doc・`:263-271` `config_handle` doc |
| 64 | `FolderListContext` / `PrebuiltIndex`（入口は `from_material` 1 つ）/ `PreparedHistorySave` | D（部分）/ K（名前の索引として短く） | `engine.rs:51-53` `PrebuiltIndex` doc、`history.rs:50-52`。`FolderListContext`（`engine.rs:29`）には doc が無い ⚠ |
| 65 | L201 **新しい Engine メソッドで I/O や索引構築をロック内で行わない** | **K** | 機構無し |

### index.bin 書き込みの排他（L203-214）
| 66 | **`index.bin` を書く経路はすべて `with_index_write_lock` を経由**・`save_cache_sorted` 自身はロックを取らない（取ると自己デッドロック） | **K（issue 指定・外から 2 件＋裸形 3 件）** | `cache.rs:1-5` `//!`・`:1043-1053` `INDEX_WRITE_LOCK` doc・`:593-605`。機構無し（型に無い契約と `cache.rs:603-605` 自身が明記） |
| 67 | L210 世代機構を撤去した理由 | H | `docs/adr/ADR-rescan-explicit-only.md`（⚠ 世代機構の撤去理由が ADR に在るかは「世代機構」の語で 1 件ヒットのみ） |
| 68 | L212 **形式昇格だけはロード外で読みロード内で書く。今日ウィンドウが開かないのは順序（`main` 起動段の 1 呼び出し元）による。ロード後に走る書き手を足すとウィンドウが戻る** | **K（正本がここだけ）** | `cache.rs:692`（`version` 近傍の「昇格は読み終えてからロックを取りに行く」）は事実のみで、「足すと戻る」の警告は無い |
| 69 | L212 **プロセスをまたぐ同時起動は守れていない**（`static` の射程） | **K or 移転（ここだけ）** | `cache.rs:344-347` がテストバイナリ多プロセスについて同じ射程を言うのみ |

### indexer.rs の索引更新の契機（L216-225）
| 70 | 更新の契機は明示操作の 3 つ | D | `SPEC.md:67-80` §3.3・`cache.rs:6-9` `//!`・`docs/adr/ADR-rescan-explicit-only.md` |
| 71 | 「更新の契機」と「書く契機」は別（形式昇格） | D | `cache.rs:8-9` `//!`・SPEC §3.3 |
| 72 | L222 **`cache_load` と `total` の間に処理を足すときは `LoadOrScanStats` に項目を作る** | D（正本は struct doc と明記） | `cache.rs:58-63`（**この doc が CLAUDE.md の経緯を指している**） |
| 73 | L223 **旧版の書き戻しは `load_cache_in` の旧版枝（`upgrade_legacy_cache_in`）**／昇格をロード側に置くなの射程は v7 の枝だけ | D + **X（v7 の記述が腐っている）** | `cache.rs:729-746`・`:895-901`。テスト `cache/tests.rs:691` `load_cache_upgrades_a_legacy_format_in_place`・`:1156` `load_cache_does_not_rewrite_when_the_format_is_current` |
| 74 | L224 **`save_cache_sorted(_in)` の直前に `sort_entries_canonical`** | D + M | `indexer/scan.rs:262` doc・`cache.rs:755-760`。テスト `cache/tests.rs:758` `legacy_upgrade_sorts_before_saving_so_the_tree_stays_shared` |
| 75 | L225 **形式昇格は `built_at` を打ち直さない** | D + M | `cache.rs:565` `BuiltAt` doc・`:742-745`。テスト `cache/tests.rs:833` `upgrade_carries_the_built_at_it_read` |

### `scan_all` の重複排除（L227-229）
| 76 | 根ごとに `check`/`record`（`root_roles`）・入れ子の根は表現可能 | D | `indexer/scan.rs:40-80`（`RootRole`・`root_roles` doc） |
| 77 | 検知器は両順序（`scan_all_dedups_when_roots_are_nested` / `…_when_the_child_root_comes_first`） | M | `indexer/scan/tests.rs:81, 113` |
| 78 | **退行の射程は 2 段: `root_roles` は CI（`root_roles_over_the_real_shape_leave_the_largest_root_inert`）、`Dedup::accept` の確保復活・結線の退行は `tests/memory_footprint.rs`（`#[ignore]`）だけ** | **K or 移転（ここだけ）** | `indexer/scan/tests.rs:37`。`Dedup::accept` doc（`scan.rs:93-99`）は「本設計の全部」と言うが CI が守らないことは書いていない |
| 79 | 「片方では捕まらない変異が在る」と書かない | H（文書作法） | — |

### エントリ名の導出ルール（L231-239）
| 80 | ファイルは `file_stem`・フォルダは `file_name`・folder.rs は `file_name`（意図的な差） | K ⚠ | `indexer/scan.rs` 内のコード。`//!`/`///` に明記があるか未確認 |
| 81 | 共通関数へ括り出さない（#997） | K（1 行）+ H | `0bb7b11` の経緯は H |

### Config のデシリアライズ経路（L241-251）
| 82 | **Config をデシリアライズする新経路は `apply_migrations()` の要否を判断** | K（`migrate.rs:5` が参照） | `config/migrate.rs:1-5` `//!` が同文 |
| 83 | **`Option<T>` sentinel の `Default` は `None`**・`effective_*` で解決・`get_or_insert_with` は `apply_migrations` の最後 | K | `config/schema.rs:263-268`。テスト `config/migrate/tests.rs:71` `apply_migrations_always_resolves_none_to_some`・`:96` `normalized_default_resolves_all_migration_sentinels` |
| 84 | L251 `reset_to_default()` で `apply_migrations()` を呼ぶ | **X（腐り）** | `snotra-settings/src/app.rs:252-255` は `Config::normalized_default()`（`config/migrate.rs:222`）を使う |

### Gotcha（計測の罠）（L253-273）
| 85 | **PS7 の `Get-Process` に `ReadOperationCount` は無い・走査の進行は CPU 時間とスレッド数で見る・計器の沈黙と対象の停止は別** | **移転（ここだけ）** | リポジトリ内に `ReadOperationCount`/`TotalProcessorTime` の出現は他に 0 件 |
| 86 | **製品の入口を通るテストは実 `%APPDATA%\Snotra` を書く → dir 注入の入口へ寄せる・`SNOTRA_CONFIG_DIR` で迂回しない・前後の行数で検算** | K（テストを書く瞬間に要る）/ 部分 D | `cache.rs:414-418`（「製品の入口でテストを書かない」）・`cache.rs:712-717` `LegacyUpgrade` doc。`SNOTRA_CONFIG_DIR` の禁止理由は `docs/adr/ADR-config-dir-env-seam-rejected-alternatives.md` ⚠ |

---

## 2. 正本がこの文書にしか無い内容（消すと失われる）と移転先候補

| 内容（現行行） | 移転先の候補 |
|---|---|
| 形式昇格のロック・ウィンドウは「順序」で閉じている／ロード後に走る書き手を足すと戻る（L212） | `snotra-core/src/indexer/cache.rs` の `upgrade_legacy_cache_in` doc（:729-746）か `INDEX_WRITE_LOCK` doc（:1043-1046） |
| プロセスをまたぐ同時起動は `INDEX_WRITE_LOCK` の射程外（L212） | 同 `INDEX_WRITE_LOCK` doc（`cache.rs:344-347` の既存記述と合流） |
| `scan_all` の退行の射程（`root_roles` は CI、`Dedup::accept`／結線は `memory_footprint` の `#[ignore]` だけ）（L229） | `indexer/scan.rs` の `Dedup::accept` doc（:93-99）か `scan_all` doc |
| PS7 `Get-Process` の計器の罠（L257-264） | `docs/development-principles.md`（計測・デバッグの節）⚠ snotra-core 固有ではない。あるいは計測ハーネスの `//!` |
| 製品入口のテストが実データを書く罠のうち「`SNOTRA_CONFIG_DIR` で迂回しない（プロセス大域 env）」「フルテスト前後の行数で検算」（L266-273） | `config/location.rs` の `ENV_CONFIG_DIR`（:23）doc、または新版 CLAUDE.md に K として残す |
| 並列 Vec 追加時の同時更新リスト（`EntryView`・`entry_view`・`debug_assert`・`shrink_to_fit`・`compute_wave1/2`）（L87） | K として残す（機構無し）。あるいは `assemble` doc |
| kana 系の分岐を両パスに同時に入れる（L91） | K として残す（`search/build.rs` `//!` が一部） |
| IndexCache 版変更の 7 項目 + 直前の版の数え直し（L173-185） | K として残す（機構無し）。代替は `INDEX_CACHE_VERSION` の doc（`cache.rs:40-47`） |
| `HistoryStore::load()` をテスト fixture に使わない（L75） | K。あるいは `history.rs:94` の `load` doc |
| 新しい Engine メソッドで I/O・索引構築をロック内に置かない（L201） | K |
| `#[cfg(windows)]` テストの書き方（L79） | K |
| `BinaryHeap` 先頭の明記・top-k の入力順不変テスト（L84-85） | K（rule 2 本が指している） |
| `Option<T>` sentinel の `Default` は `None`（L245-250） | K。あるいは `config/schema.rs` の `SearchConfig` doc |
| TOML フィールド移動の手順（L143）、serde 表現変更・凍結バイト列（L144-152） | K。**ただし `persistence-check/SKILL.md` Step 2-5 がほぼ全文を持つ**——正本をどちらにするか決める（今は SKILL が「SSOT は CLAUDE.md」と言い、内容は SKILL 側が詳しい） |
| 依存方向 `config.rs` → `opener.rs`（L24-25） | `opener.rs` の `//!`（`config/paths.rs:5` の参照も同時に付け替え） |
| hotkey parser を下流へ複製しない（L26） | `hotkey.rs` の `//!` |
| エントリ名の導出（file_stem / file_name）と folder.rs との意図的な差（L233-237） | `indexer/scan.rs` の該当箇所の doc ⚠（現状 doc に在るか未確認） |
| 反復 6 の経緯（`cache.rs:63` が「CLAUDE.md が持つ」と言っている） | PERFORMANCE.md（「起動段の live ブロック」の採用節）⚠ 既に在るか未確認。無ければ `cache.rs:63` の参照ごと削る |

---

## 4. 今の文書にある偽の記述（コードと食い違う）

1. **L3「Win32 非依存」** — L78 と `snotra-core/Cargo.toml:24`（`windows` crate）が否定。（issue 既出）
2. **L40「潰し済みの列を同じ型で渡すと `None` どうしの一致でフラグが立つ」** — `search/build.rs:29-36` の `DerivedStrings` doc と `PERFORMANCE.md:2059-2068` が「測り直しても結果は変わらない（2026-08-08 実験）」と否定。（issue 既出）**同じ偽がコードにも 3 か所ある**: `indexer/cache.rs:881`、`indexer/cache/tests.rs:331-332`、`search/build.rs:250`。
3. **L185 (b)「（`LoadCacheResult.version`）この値だけが昇格判定の入力」** — `cache.rs:700-709` の `version` フィールド doc は「**読み手はテストだけ**。かつては昇格判定の入力だったが、昇格がロードの旧版枝へ移り判定は枝そのものになった」。L178「各枝で読めた版を入れる」の理由（枝の退行を外から見る唯一の手段）は真。
4. **L89「`assemble` の `debug_assert!` は他 5 Vec を `== entries.len()` で検証」** — `search/build.rs:320-326` が検証するのは 4 本（`lower_names`・`lower_file_names`・`char_masks`・`file_name_char_masks`）。`entries` は `PathStore` そのもの。
5. **L101/L111「以下の 3 モジュールが依存する」（scan / history / search）** — `normalize_entry_key(_into)` の呼び出しは他に `indexer/path_env.rs:12,149,216` と `index_tree.rs:813`（`file_key_into`）がある。`indexer/keys.rs:5-6` もこの「3 モジュールの一覧」を指している。
6. ⚠ **L101「小文字化 + `/` → `\\`」** — 実装（`indexer/keys.rs:31-33`）は**先に `trim`** する。`SPEC.md:815` がこの節を「正規化ステップの内訳の正本」としているので、内訳として不完全。`keys.rs:1` の `//!` も同じく `trim` を欠く。
7. **L223「『昇格をロード側に置いてはならない』の射程は v7 の枝だけ——v7 は木を直読み」** — 現行は `INDEX_CACHE_VERSION = 8`（`cache.rs:47`）で、**v7 は読まない（cache-miss）**（`cache.rs:895-901`・`docs/adr/ADR-index-v7-cache-miss.md`）。木を直読みするのは現行版で、現行版は昇格しない。
8. **L39「`index.bin`（v6）でも」・L187「線上表現が v7 と同一」** — 版リテラルの焼き込み。`cache.rs:40-46` の `INDEX_CACHE_VERSION` doc が名指しで禁じている形で、現行（v8）とずれている（L187 は v6 の struct doc `cache.rs:216` も同じ「v7 と同一」で、内容としては v8 とも同一ゆえ偽とまでは言えない ⚠）。
9. **L251「`reset_to_default()` でも `Config::default()` 後に `apply_migrations()` を呼ぶ」** — `reset_to_default` は snotra-core に無く、`snotra-settings/src/app.rs:252-255` は `Config::normalized_default()`（`config/migrate.rs:222`）を使う。手で `apply_migrations` を呼ぶ規則は構造で不要化済み（`config/migrate/tests.rs:97` のコメントが #439 で回避策をモデル層へ閉じ込めたと記録）。
10. **L19「読み込み失敗の扱い分けは『データ永続化の注意』が正本」** — `config/io.rs:1-5` の `//!` が「`LoadOutcome` と `Config::load_from_dir_reporting` の rustdoc が正本」と宣言。正本宣言が 2 か所で衝突。
11. ⚠ **L107「実インデックスの全パスでの一致を `derives_same_bytes_as_normalize_entry_key` が固定する」** — テストは実 index が無ければ `return` で自己スキップ（`tests/path_query_cost.rs:89-92`）。CI では固定していない。「固定する」は開発機限定。
12. ⚠ **L101「冪等性は `migrate_normalize_keys_is_idempotent` で保証」** — テスト（`history.rs:838-857`）の入力は既に正規化済みのキー（`c:\fake\app.lnk`）で、`normalize_entry_key` を大文字・`/` 入りの入力に 2 回当てる形になっていない。保証は代理。
13. ⚠ **L167-169 悪例 `get_query_stats`** — 実在しない API（経緯の例示）。
14. ⚠ **L109「同じ関数に『末尾セグメントだけ』を通す派生が 1 つある」** — `index_tree.rs:805-814` の `file_key_into` も末尾セグメントのキーを（同じ関数を通して）組み立てる。数え上げとしては 2 つ目がある。
15. **L65-69 の統合テスト索引に `tests/dir_stat_cost.rs` が無い**（`git ls-files snotra-core/tests` に実在）。G-module-index の逆方向は `src/` だけを見るので鳴らない。偽というより欠落 ⚠。
16. `snotra-core/tests/memory_footprint.rs:292`（CLAUDE.md の外）: 昇格の機序を「index.bin 書き込みの排他」と指す——実際は「indexer.rs の索引更新の契機」。

---

## 新版を書くときの governance 上の制約（まとめ）

- `## モジュール構成` を残し、**次に別の `##` 見出しを置く**。節内に `snotra-core/src` の全 59 basename をバッククォートで残す（散文は削れるがファイル名は削れない）。
- 外から正準形で指されている見出し（3a の 13 ラベル）は**同じ字面か、その前方一致になる形**で残す。「実装前チェック（必須）」は「実装前チェック」で始まれば着地する。
- 「読み込み失敗は種類で扱いを分ける」は ADR からだけ指されている（走査元外）——残さなくても CI は鳴らないが、ADR の指しは死ぬ（凍結ゆえ許容）。
- 見出しが残って**中身が消える**参照は CI が見ない——3a の「中身への依存」列と 3c を手で直す。
- 新版に書いた識別子は G-stale-identifiers の母集団外。パス（`/` 入り）だけは G-references が見る。
