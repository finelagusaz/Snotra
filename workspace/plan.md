# plan — issue #1240: 入れ子 CLAUDE.md の単一ファイル不変条件を `//!` へ移し、面積計器に入れ子欄を足す

調査: `workspace/research.md`（敵対的調査の採否を含む）。独立導出: `workspace/plan-review-1240-derivation.md`（採否は末尾「plan-review 結果」）。ブランチ: `chore/nested-claude-md-invariants-to-module-doc`。

## 目的と受け入れ条件

**目的**: 「モジュール構成」節に積まれた**単一ファイルにしか効かない不変条件**を、そのファイルの `//!` / `///` へ移す（co-location・#562 の後半）。独立導出で判明したとおり、**多くは `.rs` 側が既に同文を持つ写し**であり、その場合は CLAUDE.md 側を消して差分（検知器名・実測値・射程）だけを `.rs` へ足す。面積計器に入れ子 `CLAUDE.md` の欄を足し、盲点を消す。**削減量は目的ではない**（40k 閾値は元から超えていない・research.md「訂正後の数字」）。

受け入れ条件（issue 本文 2026-09-06 訂正版）:

1. 動かした bullet の全数について「元の行 → 再確立地点（既存 doc の L 番号 / 新規に書いたアイテム / 消去の理由）」の対応表が `workspace/migration-1240.md` にあり、PR 本文へ写す
2. 「モジュール構成」節に残る太字 bullet はすべて横断（名指す識別子の定義が 2 ファイル以上）か、名前の索引（`snotra-core/CLAUDE.md` L5 が宣言する例外）である
3. `npm run governance:check` が緑で、成功行に `入れ子 CLAUDE.md N 字` が出る
4. `cargo doc --workspace --no-deps --document-private-items` が緑
5. `.rs` 側から CLAUDE.md の索引節を正本として指していた散文（下「D-1 の付け替え」7 件）が、動かした先を指すか、残した bullet を指し続けている

## 判定基準（実装者はこれだけで分類する）

- **単一**: bullet が名指す識別子（関数・型・フィールド・テスト名）の**定義**がすべて 1 つのファイル、またはその子モジュール（`search.rs` と `search/*.rs`、`config.rs` と `config/*.rs`、`launcher_controller.rs` と `launcher_controller/*.rs`）にある → 動かす
- **横断**: 定義が 2 ファイル以上（別 crate を含む）に分かれる → 残す。**`snotra-core/CLAUDE.md` L5 が「名前の索引」として太字を許す 3 種（ロック最小化パターンの名前・件数パラメータの対応・エントリ名の導出）も残す**
- **写し**: 移し先の `//!` / `///` が既に同じ主張を持つ → CLAUDE.md 側を消し、`.rs` に無い差分（検知器名・実測値・射程の但し書き）だけを足す。対応表には「既存 doc が正本（file:line）」と書く
- **置き場**: 責務・依存方向・モジュール全体に効く規律は `//!`。契約・保証・禁止文・測定値はその関数/型/フィールドの `///`。経緯（#NNN・撤去済みの旧名）は動かさず `#NNN` の一語に畳む（`docs/comment-guidelines.md`「配置基準（3 層）」「短く保つ」）。構造の列挙（「このファイルには X・Y・Z を置く」）は写しなので `//!` に既在の責務文で足りる
- **索引にはファイル名行を必ず残す**（`- `x.rs` — 責務は `//!``）。**太字 bullet がそのファイルの索引行を兼ねているものがある**（`footprint.rs` / `path_store.rs` / `query_plan.rs` / `scoring.rs` / `build.rs`、および `str_arena.rs` / `index_tree.rs` / `autostart.rs` / `heap_trace.rs` / `startup.rs` / `monitor.rs` のように索引行と太字が同一行のもの）。**行は残し、太字の本文だけを消す**。src-tauri L47（`tests.rs` の唯一の言及・非太字）と snotra-core L58（`search/tests/` の 6 ファイルの唯一の言及）は**動かさない**

## 変更ファイル一覧と対象シンボル

### Phase 1 — 面積計器（対応 2）

| ファイル | 変更 |
|---|---|
| `scripts/governance/instrument.mjs` | `import { MODULE_INDEX_CRATES } from "./checks/G-module-index.mjs"`（`checks/` からの import は L6 の `G-skill-table` に前例）。`nestedClaudeMdFiles()` を `Object.keys(MODULE_INDEX_CRATES).map(c => `${c}/CLAUDE.md`)` で導く。`normativeArea` の返り値に **平坦な** `nested` を足す。`checkNormativeAreaInstrument` に「入れ子が読めない」の母集団欠落 finding を足す（`ALWAYS_LOADED_FILES` と同型・`sumChars` を再利用）。ヘッダ L23〜25 の「skills 本文・モジュール CLAUDE.md・docs・ADR は対象外」を「skills 本文・docs・ADR は対象外。**モジュール CLAUDE.md は合否を持たない別欄で報告だけする**——数字は課税ではなく、上限の無い面へ逃げた分が見えないままになる方を避ける（#1240）」へ改める |
| `scripts/governance-check.mjs` L178〜201 | `areaNested: area.nested` を evidence の袋へ平坦に入れる（`areaAlways` の隣・入れ子にしない理由は L187〜189） |
| `scripts/governance/evidence.mjs` L105 | template に `・入れ子 CLAUDE.md ${ev.areaNested} 字（報告のみ）` を足す。消費点が母集団なので供給を落とせば `evidenceView` が finding にする（既存機構） |
| `scripts/governance/evidence.test.mjs` L8〜31 | `complete()` fixture に `areaNested` を足す（足さないと「緑: すべて記録済み」が `?` で落ちる）。`delete src.areaNested` で finding が出るテストを 1 本足す |
| `scripts/governance/instrument.test.mjs` | `base` fixture に入れ子 4 枚を足す（無いと既存の緑テストが全部赤になる）。`normativeArea().nested` が 4 枚の合計になるテスト、1 枚欠けると `G-area-instrument 母集団の欠落` finding が出るテストを足す |
| （変更しない） | `governance-check.test.mjs` L58〜60（計器を検査配列へ入れない）/ L79（メッセージ形式を保つ）。`ADR-retire-area-budget` L10 の成功行の形（凍結）。`health-check` skill・`governance-docs.md`・`governance-manifest.mjs`（写しではない・独立導出 A-1 で確認） |

### Phase 2 — snotra-core: 索引節（L 番号は `snotra-core/CLAUDE.md` のファイル行）

| L | 主題 | 判定 | 処置 |
|---|---|---|---|
| 14 | `IndexInputs` に載せるもの | 写し（`engine.rs` L66〜77 の `IndexInputs` doc が同文） | 消す |
| 15〜16 | `index_stale` ledger・`complete_index_drain` の clear 条件 | 単一（engine.rs） | `engine.rs` `//!` に ledger の契約を書き、`complete_index_drain` の `///` に clear 条件。**`docs/design/2026-05-31-coherence-staleset.md` L15 の「正本は CLAUDE.md「モジュール構成」」を `engine.rs` の `IndexInputs` / ledger doc へ付け替える** |
| 17 | config.rs の re-export が外部名を決める | 単一（`//!` の面: 依存方向） | `config.rs` `//!` へ（L63 の indexer.rs と同型） |
| 19 | serde 既定（#824）・検知器名 | 写し（`schema.rs` `//!` L6〜7 が太字で持ち「射程と検知器は CLAUDE.md」と委ねる） | 検知器名（`empty_section_deserializes_to_default_*` / `config_parses_with_all_sections_omitted`）と例外（配列要素の必須フィールド）を `schema.rs` `//!` へ取り込み、委ねる一文を消す。CLAUDE.md 側は消す |
| 20 | 件数パラメータ | 名前の索引（L5 の例外） | 残す |
| 21 | `icon_cache_cap` 派生 | 写し（`schema.rs` L475〜479 `保守注意:`） | 消す |
| 22 | paths.rs の正規化 2 つの共有 | 単一（非太字）。`paths.rs:5` が CLAUDE.md の `opener.rs` 節を指す | 残す（L35〜36 と対で横断側） |
| 24 | 保存先は `Config::config_dir()` から | 写し（`location.rs` `//!` L3〜4）。「`dirs::config_dir()` を直接呼ぶ箇所は他に無い」は全称 | 全称の一文だけ `location.rs` `//!` へ（検知器 `config_dir_is_wired_…` を名指す）、残りは消す |
| 25 | env 上書き・`config_dir_from` は env を読まない | 写し（bullet 自身が「契約の全文は rustdoc」） | 消す。Windows の `dirs` 同一性の実測だけ `config_dir_from` の `///` に無ければ足す |
| 28〜30 | migration の書き方・順序・行末コメント正本 | 写し（`migrate.rs` `//!` L7〜8） | 消す。L30 の「実測: 責務分割のレビューまで…」は経緯ゆえ捨てる |
| 31 | `additional` → `scan` の順序は検知器が守る | 単一 | `migrate/tests.rs:529` のテストは `#[cfg(test)]` ゆえ `cargo doc` の外——`apply_migrations` の `///` に検知器名を 1 文で置く |
| 32 | validate は検出のみ | 写し（`validate.rs` `//!` L3） | 消す |
| 35〜36 | opener の依存方向 | 横断（config.rs ↔ opener.rs、`paths.rs:5` が指す） | 残す |
| 37 | hotkey の parser 非複製 | 横断（消費者は settings / platform） | 残す |
| 39 | 並列の列は添字 | 横断（`str_arena` / `index_tree::NameArena` を名指す・`rules/snotra-core-search.md` L17 が指す） | 残す |
| 40 | kana 逐次 push 禁止・`kana_column_survives_chunked_parallel_merge` | 単一（build.rs） | `search/build.rs` `new_with_cached_masks` の `///`（`不変条件:`）。測定値は `PERFORMANCE.md` を正準形で指す（1 物理行） |
| 41 | 正規化キーは索引に持たない | 横断（scoring ↔ indexer/keys・「`normalize_entry_key` の冪等性契約」節が正本） | 残す |
| 42 | kana 列は migemo 有効時のみ | 単一（build.rs `//!` L8 が一部） | `search/build.rs` `//!` へ統合（空ガードの記述を足す）。CLAUDE.md 側は消す |
| 43 | migemo トグルは再構築経由 | 横断（crate 越し） | 残す |
| 44 | パスマッチングのスコア式・`has_path_sep` で pre-filter スキップ | 単一 | 式は `scoring.rs` の `PATH_BASE`（L471 付近）の `///`、スキップは `query_plan.rs` の `///`。**`tests/path_query_cost.rs:3` の「「モジュール構成」の search.rs 節」を `query_plan.rs` へ付け替える** |
| 45 | scoring.rs に何を置くか | 構造の列挙（写し） | 本文を消し `- `search/scoring.rs` — スコアリング・順位計算（責務は `//!`）` に。`pub(super)` の可視性方針だけ `scoring.rs` `//!` に無ければ足す |
| 46 | query_plan.rs に何を置くか | 同上 | 同型 |
| 47 | `sorted_prefix_len` | 横断（index_tree / path_store / cache）。`path_store.rs` `//!` L38〜46 が既に持つ | 残す（変更なし） |
| 48 | PathStore の 2 系統・セグメント比較禁止・v7 の木 | 写し（`path_store.rs` `//!` L3〜9・`# 組み立ての 2 系統`・検知器名も既在） | 本文を消し `- `search/path_store.rs` — …（責務は `//!`）` に |
| 49 | build.rs が唯一の入口 | 写し（`build.rs` `//!` L1〜9） | 本文を消しファイル名行に |
| 50〜56 | 派生文字列の共有鎖 | 横断（build / columns / query / indexer） | 残す。**L56 後半の `IndexMaterial` の段落は `indexer.rs:101` の `///` が同文**——その段落だけ消す。`str_arena.rs:116` の「CLAUDE.md が『`is_folder` から推論してはならない』で戒めた」は L52 が残るので変更なし |
| 57 | footprint.rs | 写し（`footprint.rs` `//!` L6〜17 が同じ 2 主張） | 本文を消しファイル名行に。**`search/tests/performance.rs:103` と `indexer/cache/breakdown.rs:167` の「CLAUDE.md の footprint 節」を `footprint.rs` の `//!` へ付け替える** |
| 58 | search/tests の索引 | 索引そのもの | 動かさない |
| 60〜61 | history の `top_n` live-read・再導入禁止 | 写し（`history.rs` `//!` L3〜4） | 「再導入しないこと」の禁止文が `//!` に無ければ 1 文足し、CLAUDE.md 側は消す |
| 63 | indexer.rs の re-export | 単一（L17 と同型） | `indexer.rs` `//!` へ |
| 72 / 73 / 77 | str_arena 線上表現 / index_tree 辿る規則 / autostart 状態の正本 | 写し（各 `//!` が同文） | 太字の本文を消し、索引行だけ残す |
| 79 | instant.rs | 「正本は `//!` と各 `///`」と自ら言う | 索引行だけに畳む |
| 83〜84 | tests/path_query_cost の計器 | 単一 | `tests/path_query_cost.rs` `//!`（L1〜6 に一部あり）へ統合。CLAUDE.md 側はファイル名行だけ |

残す（変更なし）: L13・18・26・33・34・38・59・62・64〜71・74〜76・78・80〜82。

### Phase 3 — src-tauri: 索引節（L 番号は `src-tauri/CLAUDE.md` のファイル行）

| L | 主題 | 判定 | 処置 |
|---|---|---|---|
| 11 | 背景再スキャン撤去済み | 写し（`main.rs` `//!` L4〜5） | 太字の本文を消しファイル名行に |
| 13 | ビルドフラグはメソッド経由 | 写し（同ファイル「実装パターン」L121 が同文・`rules/src-tauri.md` L12 はそちらを指す） | 索引側を消す（L121 は索引節の外ゆえ触らない） |
| 14 / 20 | コヒーレンシは engine の ledger（同文 2 行） | 横断（crate 越し） | L20 を消し L14 に統合 |
| 15 | `invalidate_icon_cache` の TOCTOU | 写し（`icon.rs` `//!` L5〜6） | 実測（#522・17/2000 回）と「片方だけだと `save_if_dirty` で復活」が `invalidate_icon_cache` の `///`（L191）に無ければ足す。CLAUDE.md 側は消す |
| 17 | `start_index_build` の順序・`build_index_from_material` に閉じる・起動経路は別 | 横断（indexing.rs ↔ main.rs の 2 経路） | 残す（前半の順序は `indexing.rs` `//!` L3〜4 と重なるが、要点は 2 経路の存在） |
| 18 | `catch_unwind`（panic 戦略） | 写し（`indexing.rs` `//!` L5） | release=abort の但し書きを `indexing.rs` の `catch_unwind` 呼び出し点（L62 付近）の `//` に置き、CLAUDE.md 側は消す |
| 19 | finish 後の再チェック・panic 経路では再 kick しない | 単一 | `start_index_build` の `///`（または `indexing.rs` `//!`） |
| 22〜23 | `ReadFailed` は適用しない・バウンドリトライ | 単一 | `should_apply_config_change` と `load_with_read_failed_retry` の `///`。**`config_watcher.rs` `//!` L5「多サブシステムに跨る不変条件は CLAUDE.md を正とする」を「跨るものは CLAUDE.md、読込失敗の扱いは本ファイルの `///`」へ直す** |
| 24 / 25 | 破棄の順序 / 再構築判定 | 横断 | 残す |
| 30 / 31 / 32 | heap_trace / startup / monitor | 写し（各 `//!`・`point_monitor_work_area` の `///`） | 太字の本文を消し索引行だけに（`heap_trace.rs:22` は索引行を名指すので行は残す） |
| 41 | 起動の入口の置き場 | 写し（`activation.rs` `//!` L3〜13 の方が新しく「集める規範は要らなくなった」） | 消す（`//!` が正本） |
| 47 | ソーステキスト検査（`tests.rs` の唯一の言及） | 索引 | 動かさない |
| layout.rs 行 | 撤去済み一覧（#646 / #752 / #835） | 経緯 | 一覧を消し `- `layout.rs` — 高さ・可視性・幾何・debounce・中間省略の純粋核（責務は `//!`）` に |
| 58〜62 | window_coordinator の 4 規則 | 横断（window_coordinator / layout / notify / view / monitor・参照 5 本が「の項」を名指す） | **残す（変更なし）**。issue 本文の例示（`MonitorFromWindow`）は第 4 規則が `monitor.rs` の doc を正本に指しており、動かす対象ではなかった——PR 本文に記す |
| 65〜114 の `###` 7 項 | 横断と宣言済み | 対象外 |

残す: `commands/` / `platform/` の集約行、`egui_shell/` のファイル一覧。

### Phase 4 — 付け替え・対応表・逆向き監査

| ファイル | 変更 |
|---|---|
| **D-1 の付け替え（7 件）** | `config/schema.rs:7`（Phase 2 L19）/ `search/tests/performance.rs:103` と `indexer/cache/breakdown.rs:167`（L57）/ `tests/path_query_cost.rs:3`（L44）/ `config_watcher.rs:5`（Phase 3 L22〜23）/ `docs/design/2026-05-31-coherence-staleset.md:15`（L15〜16）。`str_arena.rs:116` と `paths.rs:5` は指し先が残るので変更なし |
| `.claude/rules/snotra-core-search.md` L17 | L39 は残すので**変更なし**（実測の上で確認だけ） |
| `workspace/migration-1240.md` | 対応表: 元 L / 文の先頭 20 字 / 判定（単一・横断・写し・索引）/ 再確立地点（file:item または「既存 doc file:line」）/ 差分として足した内容。**消去のみの行も載せる** |
| 逆向き監査（サブエージェント 1 体・sonnet） | `git diff main...HEAD -- snotra-core/CLAUDE.md src-tauri/CLAUDE.md` が消した太字文ごとに、対応表を見ずに `git grep` で再確立地点を独立に探し、`workspace/reverse-audit-1240.txt` へ「見つかった（file:line）/ 見つからない」を書く。見つからない行は Phase 2〜3 へ戻す |

## 実装順序

Phase 1（機構・独立）→ Phase 2 → Phase 3 → Phase 4。Phase ごとに検証 green 後にコミット。**各 Phase の中は「`.rs` へ書く（差分がある場合）→ CLAUDE.md から消す」の順**——逆にすると消えた行がどこにも無い瞬間ができる。

## 不変条件と異常系

- **消える不変条件を作らない**: CLAUDE.md から消す文は、同じコミットで `.rs` に着地しているか、既存 doc が正本であることを対応表が file:line で名指す
- **索引のファイル名を消さない**: `G-module-index` の逆方向照合（basename）が守る。**効いていることをフォールトインジェクションで一度実測する**（Phase 2 の最初に索引行を 1 つ消して `governance:check` が当該 basename を名指しで赤にすることを確認し、戻す・`.claude/rules/safety-nets.md`）
- **`.rs` へ書くときの折返し**: 正準形参照（`` `PERFORMANCE.md`「採用: …」 `` / `` `SPEC.md`「13.1 設定データ」 `` / `` `docs/development-principles.md`「…」 ``）とバッククォート内の識別子を物理改行で折らない（`G-folded-heading-refs` / `G-folded-code-spans` が `.rs` も見る）。`[`X`]` の閉じを全角にしない（`G-fullwidth-doc-link-bracket`）
- **rustdoc の lint**: `Vec<Option<String>>` 等の山括弧はバッククォートの中に置く（`invalid_html_tags` は deny）。`[`X`]` 形は解決するパスだけ（`broken_intra_doc_links` は deny・`snotra-core` は `private_intra_doc_links` を allow、`src-tauri` は warn）。`[[paths.scan]]` はバッククォートで包む
- **`cargo doc` は hook で走らない**: Phase 2〜3 の各末尾で手で走らせる。`#[cfg(test)]` 配下の doc は視界の外なので、検知器名は製品側の `///` に置く
- **計器の別欄は合否を持たない**: `checkNormativeAreaInstrument` が返す finding は「読めない」だけ。面積の大小で finding を作らない（`ADR-retire-area-budget` 決定 1）。欄名に「（報告のみ）」を添えて削減圧力の副作用を抑える（独立導出 D-6）
- **写しを増やさない**: Phase 3 L13 のように同一文書の別節が同文を持つものは、索引側を消して 1 か所にする。索引節の外の写し（L41 ↔ 冪等性契約節 等）は本 issue で触らない（スコープ過剰・独立導出 E-10）
- 異常系: 移し先の `//!` が既に長い（`path_store.rs` 46 行・`activation.rs` 14 行）→ 差分が無ければ足さない。統合で既存の文を書き換えるときは対応表に「既存 doc へ統合」と書く

## テスト方針と検証コマンド

| 局面 | コマンド | 期待 |
|---|---|---|
| Phase 1（先に赤） | `npm test -- scripts/governance` | `areaNested` 欠落・入れ子欠落の新テストが赤 → 実装後に緑 |
| Phase 1 | `npm run governance:check` | 成功行に `入れ子 CLAUDE.md 88594 字（報告のみ）`（38,664 + 31,782 + 13,051 + 5,097・本日実測。実装時点の値で読み替える） |
| Phase 1（消費点の実測） | `governance-check.mjs` 側の `areaNested` 供給を一時的に落として `npm run governance:check` | `evidence が読む areaNested が未記録` の finding が印字される（exit は格下げゆえ 0）→ 戻す |
| Phase 2 冒頭（検出器の実測） | 索引行 1 本（例: `- `search/footprint.rs` …`）を消して `npm run governance:check` | `実ファイル snotra-core/src/search/footprint.rs が索引（…）に見当たらない` で赤 → 戻す |
| 各 Phase | `npm run governance:check` | 24 件緑。成功行の `.rs 137 件`・見出し参照件数が**減っていない** |
| Phase 2〜3 | `cargo doc --workspace --no-deps --document-private-items` | 警告 0 |
| Phase 2〜3 | post-edit hook（fmt / clippy / crate test）の沈黙 | `.rs` は検査割り当て済みゆえ沈黙 = 合格 |
| 文字数の実測 | `node -e 'const fs=require("fs");for(const f of process.argv.slice(1)){console.log([...fs.readFileSync(f,"utf8").replace(/\r/g,"")].length,f)}' snotra-core/CLAUDE.md src-tauri/CLAUDE.md` | 対応表の末尾に前後の値を記す（数値目標ではない・2026-09-06 実測 38,664 / 31,782） |
| 残余の確認 | 索引節の太字 bullet を列挙し、各行が横断か名前の索引であることを対応表で突き合わせる | 受け入れ条件 2 |

## `SPEC.md`・関連文書の更新要否

- `SPEC.md`: 不要（挙動を変えない。`SPEC.md` から索引節の bullet への参照は 0 件・独立導出 D-5）
- `PERFORMANCE.md`: 不要（同上）
- `docs/design/2026-05-31-coherence-staleset.md` L15: 付け替え（Phase 4）
- `docs/comment-guidelines.md` L108 の模範例: 足さない（例の列挙は腐る側・独立導出 E-7）
- 索引節の前書き「`//!` に収まらない**横断**不変条件」: 現物がその約束に一致する状態になるので据え置く
- `RETROSPECTIVE.md`: `/retrospective` で扱う（本計画の外）

## 作業項目

### Phase 1 — 面積計器
- [x] `instrument.mjs` に入れ子欄（`MODULE_INDEX_CRATES` 由来）・母集団欠落 finding・ヘッダの理由文の改稿
- [x] `governance-check.mjs` / `evidence.mjs` に `areaNested` を通す
- [x] `instrument.test.mjs` / `evidence.test.mjs` の fixture と同型テスト（先に赤: 新 4 本が落ち既存 23 本は緑・2026-09-06 実測）
- [x] 消費点の実測と `npm test`（506 件緑）/ `governance:check`（入れ子 CLAUDE.md 88594 字）——供給を落とすと `evidence が読む areaNested が未記録` の finding が印字され行は `? 字` になる。**exit は 0 のまま**（計器の供給断は `metaFindings` へ格下げされており、ゲートへ戻るのは監査モードだけ・`ADR-governance-meta-demotion`）。「赤」ではなく「沈黙しない」が実測された保証

### Phase 2 — snotra-core 索引節
- [ ] 検出器の実測（索引行 1 本を消して赤を見て戻す）
- [ ] 上表の「写し」行: 差分だけ `.rs` へ足し、CLAUDE.md 側を消す
- [ ] 上表の「単一」行: `.rs` へ書いてから CLAUDE.md 側を消す
- [ ] `workspace/migration-1240.md` に対応表を書く
- [ ] `cargo doc` と `governance:check` が緑

### Phase 3 — src-tauri 索引節
- [ ] 上表の行を処置し、対応表へ追記
- [ ] `cargo doc` と `governance:check` が緑

### Phase 4 — 付け替え・逆向き監査
- [ ] D-1 の 7 件を付け替える（`rules/snotra-core-search.md` は変更なしを確認）
- [ ] 索引節に残る太字 bullet がすべて横断か名前の索引であることを対応表で確認（受け入れ条件 2）
- [ ] 逆向き監査を 1 体起動し、`workspace/reverse-audit-1240.txt` で全行「見つかった」になるまで戻す
- [ ] 文字数を実測して対応表に記す

## 未確定（実装前に潰す）

- [x] `path_store.rs` の `//!`（46 行）と索引 L47〜48 の重なり — 2026-09-06 に突き合わせた。L48 は全て既在（`raw_into` 5 / `normalized_into` 2 / `セグメント` 4 / `adopt` 4 / 検知器名 1）。L47 は独立導出により横断（index_tree / cache にも定義）と判定し直し、残す
- [x] `//!` へ移した長文が複数の設計論点を抱えないか — 判定は「判定基準」節に固定した。独立導出で「写し」が多数と判明し、新規に書く量は当初見込みより小さい
- [x] `window_coordinator.rs` の 4 規則を動かすか — 独立導出 C-2 と D-2（5 ファイルにまたがり、参照 5 本が「の項」を名指す）により横断として残す。issue 本文の例示は PR 本文で訂正する
- [x] 計器の「読めない」finding を入れ子に足すか — 足す（`ALWAYS_LOADED_FILES` と同型。`G-module-index` が先に赤にするとしても、計器の数字が沈黙で欠けない方を取る）。fixture へ 4 枚を足す費用は小さい

## 人間レビュー

- [x] 承認済み — 2026-09-06 / 問い: "`workspace/plan.md` を承認しますか。承認後は workspace をコミット・プッシュし、実装は `/implement` へ渡します。注釈があれば `plan.md` へ直接書き込むか、ここでお伝えくださいませ。" / 回答: "承認する"

## plan-review 結果

- リスク: 高（ガバナンス文書の移動・圧縮／hook・CI が見る機構の変更）
- レビュー方式: 独立導出 1 体（`--deep`・`workspace/plan-review-1240-derivation.md`）
- エージェント数: 1（ほかに /start-issue 3b の敵対的調査 1 体）

### 要対処（計画へ反映済み）
- 太字 bullet の多くが `.rs` 側に同文を持つ写し — 「移す」から「消して差分だけ足す」へ処置を改めた — 再照合: `engine.rs` L66〜77・`validate.rs` `//!` L3・`history.rs` `//!` L3〜4・`footprint.rs` `//!` L6〜17・`autostart.rs` `//!` L4〜7・`icon.rs` `//!` L5〜6・`indexing.rs` `//!` L3〜5・`main.rs` `//!` L4〜5・`heap_trace.rs` `//!` L3〜4 を主エージェントが読んで確認
- 移し先の `.rs` が CLAUDE.md を正本に指している 7 件（D-1）— Phase 4 に付け替えを追加 — 再照合: `schema.rs:7`・`performance.rs:103`・`breakdown.rs:167`・`path_query_cost.rs:3`・`config_watcher.rs:5`・`paths.rs:5`・`docs/design/2026-05-31-coherence-staleset.md:15` を読んで確認
- `window_coordinator.rs` の 4 規則・`sorted_prefix_len`・件数パラメータ・opener 依存方向・hotkey・並列列・正規化キーは横断（または L5 が宣言する名前の索引）— 「残す」へ判定を改めた — 再照合: `snotra-core/CLAUDE.md` L5 の例外宣言と、識別子の定義が複数ファイルにまたがることを導出の file:line で確認
- 索引節の外（`実装パターン` L121）に同文があるもの（src-tauri L13）— 索引側を消して 1 か所へ — 再照合: L115〜133 を読んで確認
- `evidence.test.mjs` の `complete()` fixture と `instrument.test.mjs` の `base` fixture — 足さないと既存の緑が赤になる — Phase 1 に追加
- `.rs` へ書くときの折返し・rustdoc lint（`G-folded-*`・`invalid_html_tags`）— 不変条件に追加
- 検出器と消費点のフォールトインジェクション（索引行の削除・`areaNested` 供給の停止）— 検証表に追加

### 軽微
- 独立導出 D-6（成功行の数字が削減圧力になる副作用）— 欄名に「（報告のみ）」を添える形で反映
- `str_arena.rs:116` の指し先（L52）は残るので変更なし

### 未検証
- 各「写し」行の**差分**（`.rs` に無い検知器名・実測値）の正確な量は実装時に bullet ごとに突き合わせる（対応表に残す）

### 判断
- 実装着手: 人間の裁定待ち

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー 1 体（独立導出・`--deep`）
- エージェント数: 2（敵対的調査 1 + 独立導出 1）
- 要対処: 7 件（上記・すべて計画へ反映）
- 未検証: 「写し」行ごとの差分量（実装時に対応表で確定）
