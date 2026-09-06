# 対象 issue: #1240 — 独立導出（plan.md / research.md / adversarial-1240.txt は未読）

作成: 2026-09-06。コードと文書だけから導いた。根拠は `file:line` か grep 結果。⚠️ は確信の持てない項目。
行番号は本日の `main`（d2e9804）時点。

## 0. 先に一つ、構造上の発見

**snotra-core の「モジュール構成」節では、太字の不変条件 bullet の多くが、そのファイルの索引行を兼ねている。** 実測（`workspace/` 外のスクリプトで節内のバッククォート付き basename を数えた）:

| basename | 節内での言及行（1 行のみ） | その行の性格 |
|---|---|---|
| `footprint.rs` | L57 | 太字 bullet「**常駐ヒープの内訳は `search/footprint.rs` が数える**」＝索引行を兼ねる |
| `path_store.rs` | L48 | 太字 bullet「**`target_path` は索引に持たず…**（`search/path_store.rs`…）」＝同 |
| `query_plan.rs` | L46 | 太字 bullet ＝ 同 |
| `str_arena.rs` / `index_tree.rs`（L73 側） / `autostart.rs`（L77） / `heap_trace.rs`（src-tauri L30） / `startup.rs`（src-tauri L31） | 索引行 ＋ 同一行に太字の不変条件 | 行を丸ごと消すと索引が消える |
| `basic.rs` `common.rs` `incremental.rs` `migemo.rs` `mod.rs` `path.rs` | L58 のみ | 太字 bullet「**ユニットテストは `search/tests/` へ機能別に置く**」の中 |
| `tests.rs`（src-tauri） | L47 のみ | 非太字 bullet「ソーステキスト検査は対象モジュールの子として置く」の中 |

**帰結**: 「bullet を `//!` へ移す」を「行を消す」で実装すると `G-module-index` の逆方向照合が赤になる（`checks/G-module-index.mjs` L~95-105: production ファイルの basename が節に無ければ finding）。**正しい形は「ファイル名行は残し、太字の本文だけを `//!` へ移して、行は `（責務・不変条件は `//!`）` の索引だけにする」**。要求 1 の「CLAUDE.md 側は索引（ファイル名行＋横断の bullet）だけにする」と一致する。

---

## A. 変更が必要なファイルと触るシンボル

### A-1. 機構側（要求 2: 入れ子 CLAUDE.md の文字数の欄）

| ファイル | 触るシンボル | 根拠 |
|---|---|---|
| `scripts/governance/instrument.mjs` | `normativeArea(snapshot)`（L107-116）——`{ always, rules }` に 3 面目（仮に `nested`）を足す。母集団は `checks/G-module-index.mjs` の `MODULE_INDEX_CRATES` の鍵から `<crate>/CLAUDE.md` を導く（手で 4 枚を並べると `ALWAYS_LOADED_FILES` L29-34 の doc が宣言する「足し忘れは誰も報せない」型を 1 つ増やす。`instrument.mjs` は既に L6 で `checks/G-skill-table.mjs` から import しており、`checks/` からの import は前例がある）。`checkNormativeAreaInstrument`（L88-103）へ「読めない」finding を足すかは判断事項——読めなければ `G-module-index` が先に赤にするので、計器側では**面積の総和が 0 になっても沈黙しない**ことだけ守れば足りる ⚠️ | L34, L42-51, L88-116 |
| `scripts/governance/instrument.mjs` | ヘッダコメント L23-25「skills 本文・**モジュール CLAUDE.md**・docs・ADR は対象外——課税すれば〜」を書き換える。報告欄に載せることは課税ではないが、「対象外」の一文はこの変更で偽になる。`ADR-area-metric-characters` L25（ratchet に含めない却下）とは「合否ではなく報告」で両立する旨を 1 文で | L21-26 |
| `scripts/governance-check.mjs` | `runAll` L178（`const area = normativeArea(snapshot)`）・L200-201（`areaAlways` / `areaRules`）に並べて**平坦な** `areaNested`（仮称）を袋へ入れる。入れ子（`area.nested` をそのまま渡す）にしない——L187-189 に実測つきの理由 | L178-205 |
| `scripts/governance/evidence.mjs` | `assembleEvidence` の template L105 `恒久規範 常時ロード ${ev.areaAlways} 字・rules ${ev.areaRules} 字` に `・入れ子 CLAUDE.md ${ev.areaNested} 字` 相当を足す。消費点が母集団なので供給を忘れれば `evidenceView` が finding にする（L14-35 の doc） | L105 |
| `scripts/governance/evidence.test.mjs` | `complete()`（L8-31）に新キーを足す。足さないと L34-41「緑: すべて記録済み」が `?` を含んで**落ちる** | L8-41 |
| `scripts/governance/instrument.test.mjs` | `normativeArea` の返り値に新面が在るテストを追加。新面を「読めなければ finding」にするなら、既存 fixture `base`（L12）は入れ子 CLAUDE.md を 1 枚も持たないので L13-16 以下の**緑テスト全部**が赤になる→`base` に 4 枚（か母集団導出に合わせた枚数）を足す | L5-80 |
| `docs/adr/ADR-retire-area-budget.md` L10 | 成功行の形 `恒久規範 常時ロード N 字・rules N 字` を引用している。**凍結ゆえ編集しない**（`ADR-adr-frozen-history`） | L10 |

**現在値の参考**（`wc -m`・CR 込みの概算）: snotra-core 71,419 / src-tauri 57,892 / snotra-settings 23,587 / snotra-egui-runtime 9,511（計 ≈162k）。常時ロード面は 19,708 字・rules 15,865 字（本日の `governance:check` 成功行）。

**変更不要と判定したもの（要求 2）**: `.claude/skills/health-check/SKILL.md` L76/L117（「何を母集団に取るかはこの関数が持つ」＝写しではない）、`docs/build-commands.md` L172、`.claude/rules/governance-docs.md` L11、`scripts/governance-manifest.mjs`（面積の面は持たない）、`governance-check.test.mjs` L58-60/L79（id とメッセージ形式 `G-area-instrument 母集団の欠落` を保てば緑）。

### A-2. 文書側（要求 1）

| ファイル | 触る節・シンボル | 備考 |
|---|---|---|
| `snotra-core/CLAUDE.md` | 「## モジュール構成」L8-84 の太字 bullet（候補は C-1）。**L5 の冒頭注記**「例外は名前の索引である——ロック最小化パターンの名前・件数パラメータの対応・エントリ名の導出は…太字にしている」は、件数パラメータ（L20）を動かすなら同期する | 索引行は残す（§0） |
| `src-tauri/CLAUDE.md` | 「## モジュール構成」L7-63 の太字 bullet（候補は C-2）。L36 の宣言「`//!` に収まらない横断不変条件は本節の `###` 各項」により **`###` 小節（L65-113）は横断と宣言済み＝対象外** | 同上 |
| 移し先 `.rs`（C-1/C-2 の右列） | `//!` か該当アイテムの `///`。**既に `//!` が同じ主張を持つファイルが多い**（D-1）——その場合は「移す」ではなく「CLAUDE.md 側の写しを消し、残る差分（検知器名・射程）だけを `//!` へ足す」 | `docs/comment-guidelines.md` L53「`//!` = モジュール全体に効く不変条件」・L54「`///` = 契約・保証・局所的な理由」 |
| `snotra-core/src/config/schema.rs` L6-7 | `//!` が「射程と検知器は `snotra-core/CLAUDE.md`」と**CLAUDE.md を正本に指している**。L19 bullet を移すなら、この一文を自分の `//!` へ取り込む形に書き換える（自己参照になる） | D-1 |
| `snotra-core/src/search/tests/performance.rs` L103 / `snotra-core/src/indexer/cache/breakdown.rs` L167 | 「`snotra-core/CLAUDE.md` の footprint 節 / search.rs 節」を正本として引く散文形の参照。L57 を `footprint.rs` の `//!` へ寄せるなら、指し先を `footprint.rs` の `//!` へ直す | D-1 |
| `snotra-core/src/str_arena.rs` L116 | 「`snotra-core/CLAUDE.md` が『`is_folder` から推論してはならない』で戒めた」——L52 サブ bullet を指す。動かすなら指し先を直す | D-1 |
| `snotra-core/tests/path_query_cost.rs` L3 | 「`snotra-core/CLAUDE.md`「モジュール構成」の search.rs 節」——L44（`has_path_sep` は Fuzzy pre-filter をスキップ）を指す。ラベルは見出しに着地し続けるので機構は緑のまま、**意味だけ腐る** | D-1 |
| `src-tauri/src/config_watcher.rs` L3-5 | `//!`「多サブシステムに跨る不変条件は `src-tauri/CLAUDE.md` を正とする」。L22-23（単一ファイル）を取り込むなら文言を分ける | D-1 |
| `.claude/rules/snotra-core-search.md` L17 | 「横断不変条件（並列 Vec レイアウト…）: `snotra-core/CLAUDE.md` の search.rs 節」——L39 を動かすなら散文形ゆえ機構は見ない。**ただし L39 は横断（C-1）なので動かさない判定なら触らない** | D-2 |
| `docs/design/2026-05-31-coherence-staleset.md` L15 | 「as-built は `IndexInputs` と `index_stale` ledger で、正本は `snotra-core/CLAUDE.md`「モジュール構成」」——L14-16 を `engine.rs` へ寄せるなら意味が腐る（`docs/design/` は `governanceDocs` と `allRefDocs` の母集団内・`lib.mjs` L559-566, L708-712） | D-2 |

**編集時に届くもの（赤ではない）**: 入れ子 CLAUDE.md の編集で `checkModuleIndex` 双方向・`checkHeadingRefs` 等・`reportFor`（`dependents.mjs`）の reminder（`docs/hooks.md` L107-118）。「モジュール構成」を指す参照は約 20 本（B-2 の grep）あるので、`dependents` の一覧は長くなるが合否は動かない。

---

## B. この変更で赤になりうる検査

### B-1. 機構側（要求 2）
1. **`scripts/governance/evidence.test.mjs` L34-41** — template にキーを足して fixture に足さないと `?` を含んで落ちる（消費点＝母集団の設計そのもの）。
2. **`scripts/governance/instrument.test.mjs`** — 新面の「読めない→finding」を `checkNormativeAreaInstrument` へ足すなら、fixture に入れ子 CLAUDE.md が無い既存の緑テスト（L13-16, L22-30 ほか）が全部赤になる。
3. **`governance-check.test.mjs` L58-60** — `G-area-instrument` を検査配列へ入れないこと（計器のまま）。L79 のメッセージ grep `G-area-instrument 母集団の欠落` を保つ。
4. ⚠️ `governance-manifest.test.mjs` L80-95 のコメントは import グラフの**説明**であり数を assert していない（L100-125 を読んだ限り `instrument.mjs` の import 本数を測るテストは無い）。

### B-2. 文書側（要求 1）

**(a) 索引節からファイル名の言及が消えると赤になる検査 = `G-module-index`（逆方向）**。§0 の表が実測。とくに:
- snotra-core L58 を丸ごと移すと `basic.rs` / `common.rs` / `incremental.rs` / `migemo.rs` / `mod.rs` / `path.rs` の 6 件が赤（`ranking.rs` は L48 に、`performance.rs` は L81 に、`build.rs` は他行にもある）。
- snotra-core L57 / L48 / L46 を丸ごと移すと `footprint.rs` / `path_store.rs` / `query_plan.rs` が赤。
- src-tauri L47 を移すと `tests.rs` が赤（src-tauri 内でその basename はこの 1 行にしか無い）。
- 編集直後は `docs/hooks.md` L108 の reminder（`<crate>/CLAUDE.md` を編集した→双方向の不整合）が同じ判定で鳴る。**PR では `governance-check` job が赤にする**。

**(b) 見出し参照の実在を見る検査 = `G-heading-refs`**（着地先は ATX 見出し・番号付き項目・**太字リード**・テスト名。`lib.mjs` L485-490 `ANCHOR_SPECS`）。太字 bullet は**アンカーになりうる**ので、消すと参照が「着地しない」で赤になる。生きた層から入れ子 2 枚へ向く正準形参照の全数を grep した（`rg -o` ・`workspace/` と `docs/adr` `docs/superpowers` は照合母集団外だが列挙には含めた）。**「モジュール構成」節内の太字 bullet をラベルにしている参照は 0 件**——ラベルは次のどれかだった:
- 見出しそのもの「モジュール構成」: `snotra-core/tests/path_query_cost.rs:3`・`src-tauri/src/state.rs:101`・`egui_shell/{window_coordinator.rs:24,190, view.rs:972,1214,1248, mod.rs:21, layout.rs:394, launcher_controller/activation.rs:348, launcher_controller/search_flow.rs:44}`・`docs/architecture.md:247`・`.claude/agents/code-reviewer.md:68`・`.claude/rules/safety-nets.md:50`・`docs/design/2026-05-31-coherence-staleset.md:15`・ADR 4 本（凍結）
- `###` 小節（src-tauri・横断と宣言済み）: 「処置を返す純粋核の強制」(`lifecycle.rs:35`, `search_state.rs:128`)・「イベント駆動 wake の不変条件」(`notify.rs:84`, `window_coordinator.rs:844`, `snotra-egui-runtime/CLAUDE.md:24`, `/race-check` L16, `G-heading-refs.mjs:16`, `dependents.test.mjs:47`)・「テーマ色・font・行高の読みは 1 フレーム 1 回」(`code-reviewer.md:124`)・「trace の presence 検査は状態の検査ではない」(`docs/build-commands.md:90`, `scripts/visual-check-colors.ps1:32`, `scripts/smoke-egui.ps1:550`)
- 「モジュール構成」の外の節・太字リード: 「可視性を変える操作はイベントループスレッドに閉じてある」(Win32 節 L191・`SPEC.md:625`, `state.rs:37`, `layout.rs:453`, `results_window.rs:28,111`)・「Win32 / Tauri 注意事項」・「ウィンドウ生成の制約」・「working set の能動回収」・「共有 core 関数の返り値契約」・snotra-core の「データ永続化の注意」「IndexCache バージョン変更チェックリスト」「`normalize_entry_key` の冪等性契約」「index.bin 書き込みの排他」「indexer.rs の索引更新の契機」「history.rs のキー正規化に関するチェックリスト」「incremental cache とパスクエリの非互換」「engine.rs のロック最小化パターン」「Config のデシリアライズ経路」「読み込み失敗は種類で扱いを分ける」(ADR)
- **ゆえに (b) は、対象 bullet を消しても機構では赤にならない。腐るのは「「モジュール構成」の `window_coordinator.rs` の項」「の search.rs 節」「の #1032 条項」のような**ラベルの後ろの散文**だけである（D-2）。

**(c) 移した先の `.rs` で新しく赤になりうる検査**（`.rs` は `allHeadingRefDocs` の腕に入る・本日の成功行「.rs 137 件」）:
- `G-folded-heading-refs`: bullet 内の正準形（例: L24 `docs/development-principles.md`「config の値は到達性の検出器を持たない」、L39/L40/L47/L56 の `PERFORMANCE.md`「採用: …」、L19 の `SPEC.md`「13.1 設定データ」）を `//!` で**物理改行で折ると赤**。1 物理行に収める。
- `G-folded-code-spans`: バッククォート内の識別子を行またぎさせると赤（`docs/comment-guidelines.md` L126）。長い bullet を `//!` の 100 桁前後へ折り返すときに最も踏みやすい。
- `G-fullwidth-doc-link-bracket`: `[`X`]` の閉じを全角にすると赤（`///` `//!` が対象）。
- `G-adr-citations`: `.rs` コメントの `ADR-<slug>` は実在照合される（既に実在するものしか書かないので通常は緑）。
- `cargo doc --workspace --no-deps --document-private-items`（CI `ci.yml` L219-220 のみ・**PostToolUse hook は走らない**・`docs/build-commands.md` L32）: `broken_intra_doc_links` / `invalid_html_tags` が deny（ルート `Cargo.toml` L21-23）。散文の `Vec<Option<String>>` / `Arc<RwLock<Config>>` / `{0, entries.len()}` はバッククォートの外へ出すと `invalid_html_tags` で赤。`` [`X`] `` 形に書き換えるなら解決するパスであること（`snotra-core` は `#![allow(rustdoc::private_intra_doc_links)]` L12。`src-tauri` に同 allow は無いが warn 止まりで deny ではない ⚠️）。
- PostToolUse（`.rs`）: `cargo fmt --check` / clippy `-D warnings` / `cargo test -p <crate>` が走る（`post-edit.mjs` L377-394）。rustfmt はコメントを整形しないので `//!` の追記では原則沈黙＝合格。clippy の `doc_markdown` は有効化されていない（ルート `[workspace.lints.clippy]` は `disallowed_methods` のみ・L34-35）。

**(d) 変わらないもの**: `G-stale-identifiers` は入れ子 CLAUDE.md も `.rs` も母集団に持たない（`lib.mjs` L694 `STALE_EXTRA_DOCS` はルート `CLAUDE.md` のみ・L701-712）。`G-references` は入れ子 CLAUDE.md を見る（`governanceDocs` L562）が、削る変更で新しい非実在参照は生まれない。

---

## C. 移すべき bullet の候補と「単一 / 横断」判定

判定基準: bullet が名指す識別子の**定義**が 1 ファイル（またはその子モジュール）に閉じるか。定義位置は `rg -n 'fn …|struct …'` の実測。

### C-1. `snotra-core/CLAUDE.md`（L8-84）

| 行 | bullet（太字部の要約） | 名指す識別子の定義 | 判定 | 移し先の候補 |
|---|---|---|---|---|
| L14 | `IndexInputs` には「変わったら索引を建て直す入力」だけ | `engine.rs:79` `pub struct IndexInputs`（doc L66-77 が**同じ主張を既に持つ**） | 単一（engine.rs）。ただし破棄は `config_watcher::icons_turned_off`（src-tauri）と結線 | **既に `///` に在る＝写し**。CLAUDE.md 側を消す |
| L15 | コヒーレンシ判断は `index_stale` ledger に閉じる | `engine.rs:281,286,291,307`（`mark_index_stale` ほか） | 単一（engine.rs）。ただし src-tauri L14/L20/L25 が「engine の ledger に一元化」と**写しを 3 行**持つ | `engine.rs` の `//!` か `index_stale` フィールドの `///` ⚠️（src-tauri 側の写しをどうするかは横断） |
| L16 | `complete_index_drain` は snapshot == 現在のときだけ clear | `engine.rs:307` ＋テスト L575,586 | 単一 | `complete_index_drain` の `///` |
| L17 | crate 外から見える名前は `config.rs` の re-export が決める | `config.rs`（re-export）だが消費者は crate 外 | **横断**（呼び出し形の維持は下流の契約） | 残す |
| L19 | 新しいセクション・設定キーには serde の既定 | `schema.rs`。検知器 `config/tests.rs:232-281`（`empty_section_deserializes_to_default_*` / `config_parses_with_all_sections_omitted`） | 単一（config 子モジュール群） | `schema.rs` の `//!` L6-7 が**既に太字で持ち、射程と検知器だけを CLAUDE.md に委ねている**→検知器名を `//!` へ取り込み CLAUDE.md 側を消す |
| L20 | 件数パラメータ（`visible_rows` / `result_limit` / `recent_limit`） | `schema.rs:259` `effective_result_limit`。消費者は `Engine::search` / `capture_folder_list_context` / `recent_history`（engine.rs）・history の `top_n` | **横断**（L5 が「名前の索引」と宣言） | 残す |
| L21 | `icon_cache_cap()` は派生 | `schema.rs:479` ＋ `schema/tests.rs:138-175` | 単一 | `icon_cache_cap` の `///`（非太字だが同じ塊） |
| L24 | 新しい永続ファイルの保存先も `Config::config_dir()` から | `location.rs:33`。**「`dirs::config_dir()` を直接呼ぶ箇所は他に無い」は crate 全体の全称** | ⚠️ 単一寄り（`location.rs` の `//!` L3-4 が既に「保存先を導く経路はここ」を持つ）。全称の部分は横断 | `//!` に既に在る。CLAUDE.md 側は全称の 1 文だけ残すか消す |
| L25 | env 上書きはそのまま使い `Snotra` を付け足さない／`config_dir_from` は env を読まない | `location.rs:46` ＋ `location/tests.rs:12-79`（`config_dir_is_wired_to_dirs_config_dir_with_snotra_suffix` L79） | 単一 | `config_dir_from` の `///`（bullet 自身が「契約の全文は rustdoc」と言っている＝写し） |
| L28 | 旧キーの後方互換移行 | `migrate.rs:58,105,116` | 単一 | `migrate.rs` の `//!`（L7-8 が既に「呼び出し順は `apply_migrations` が持つ」） |
| L29 | migration は系統ごとの private fn | `migrate.rs:105-` | 単一 | 同上（既に `//!` L7） |
| L30 | 呼び出し順は固定・依存は行末コメントが正本 | `migrate.rs:58-` ＋ L61 の行末コメント | 単一 | 同上（既に `//!` L8「依存の一覧をここへ写さない」） |
| L31 | `additional` → `scan` が正規化より先であることは検知器が守る | `migrate/tests.rs:529` | 単一（migrate 子） | そのテストの `///`（bullet 自身が「射程と死角は同テストの doc が正本」） |
| L32 | `validate.rs` は検出だけ・補正しない | `validate.rs` `//!` L3 が**同文を持つ** | 単一（validate/migrate の対） | 写し。CLAUDE.md 側を消す |
| L35 | 依存方向は `config.rs` → `opener.rs` | `config.rs`（re-export）と `opener.rs` | **横断**（2 ファイルの向き） | 残す（`config/paths.rs:5` `//!` も「取り決めは CLAUDE.md の `opener.rs` 節」と指す） |
| L37 | `HotkeyConfig` は re-export・parser を下流へ複製しない | `hotkey.rs:14,137`。消費者は settings / platform | **横断** | 残す |
| L39 | 並列の列は添字で対応づけて持つ | `search.rs` ＋ `str_arena.rs` ＋ `index_tree.rs`（NameArena） | **横断**（3 ファイル） | 残す（`.claude/rules/snotra-core-search.md` L17 が指す） |
| L40 | `kana_lower_names` をアリーナへ逐次 push で組み直さない | `search/build.rs:177,180,444`（`KANA_CHUNK` / `new_with_cached_masks`）・検知器 `search/tests/build.rs:956` | 単一（search 子） | `new_with_cached_masks` の `///` か `KANA_CHUNK` の `///` |
| L41 | 正規化キーは索引に持たない・畳み込み比較を別実装で書かない | `search/scoring.rs:79` `with_normalized_key`・`indexer/keys.rs:30` | **横断**（search ↔ indexer。「クロスモジュール不変条件」節 L114-126 が既に正本） | 残す（か L122 への参照 1 行へ縮める） |
| L42 | kana 2 列は `migemo_enabled` のときのみ構築 | `search/build.rs:180`・`search.rs:350`（`kana_available`）・`scoring.rs:328,449` | 単一（search 子） | `search/build.rs` の `//!`（L8 が既に「kana 系 2 本の {0, len} 不変条件」を持つ） |
| L43 | migemo トグルの反映は index 再構築経由 | `engine.rs` `IndexInputs` ＋ src-tauri `config_watcher` / `indexing` | **横断**（crate 越し） | 残す |
| L44 | パスマッチング（スコア `3000 - min(byte_pos, 500)`・`has_path_sep` で pre-filter スキップ） | `search/query_plan.rs` / `scoring.rs` | 単一（search 子）。**ただし `tests/path_query_cost.rs:3` が CLAUDE.md を正本に指す** | `query_plan.rs` か `scoring.rs` の `///`（式は「精密事実」ゆえコードの doc が正本であるべき・`docs/development-principles.md` L58-64） |
| L45 | スコアリングは `search/scoring.rs` へ | 索引行を兼ねる | 単一 | 本文の列挙（`mod score_tier`・`TopK`…）は `scoring.rs` の `//!` へ。**列挙は「構造の写し」**（`comment-guidelines.md` L17）ゆえ削る側 |
| L46 | クエリ計画は `search/query_plan.rs` へ | 索引行を兼ねる（`query_plan.rs` の唯一の言及） | 単一 | 同上。**行は残す** |
| L47 | 整列は「先頭から何件までか」で持つ（`sorted_prefix_len`） | `index_tree.rs:292,304,442,748`・`path_store.rs:124,272-309`・`indexer/cache.rs:624` | **横断**（3 ファイル） | 残す（`path_store.rs:277-284` に既に要点あり） |
| L48 | `target_path` はフォルダ木から組み立てる（`PathStore`・セグメント比較禁止） | `path_store.rs:71,99,232,247,272,509`・`index_tree.rs`・検知器 `search/tests/ranking.rs` | 主に単一（path_store）だが `index_tree.rs` と `indexer` に跨る。索引行を兼ねる | 本文は `path_store.rs` の `//!`（L3-9 が既に一部）。**行は残す** |
| L49 | 索引の構築処理は `search/build.rs` へ（`from_material` が唯一の入口） | `search/build.rs:398,444,200` | 単一 | 既に `//!` L1-9 にほぼ在る。列挙を削る |
| L50-56 | 派生文字列の共有鎖（`None` へ潰す・`file_name_is_lower_name`・`measure_derived_sharing`・`IndexMaterial`） | `search/build.rs:200-295`・`indexer/columns.rs:136,234,50,80`・`query.rs:129`・`indexer.rs:55,104`・`path_store.rs:95,141`・検知器 `search/tests/build.rs:187,411,491,773` | **横断**（search / indexer / query の 3 系） | 残す（ただし L56 後半の `IndexMaterial` は `indexer.rs:101` の `///` が同文を持つ＝写し部分は削れる） |
| L57 | 常駐ヒープの内訳は `search/footprint.rs` が数える（`..` を書かない・構築前 Vec を走査しない） | `footprint.rs:107`・`path_store.rs:183` | 単一。索引行を兼ねる。`footprint.rs` の `//!` L6-17 が**同じ 2 主張を既に持つ** | 写し。**行は残し本文を消す**。`performance.rs:103` / `breakdown.rs:167` の指し先を直す |
| L58 | ユニットテストは `search/tests/` へ機能別に置く | 索引（6 basename の唯一の言及） | 単一だが**索引そのもの** | **動かさない**（動かすと B-2(a) で 6 件赤） |
| L60-61 | `top_n` は焼き込まず live-read／`HistoryStore` に `top_n` を再導入しない | `history.rs:167,204,344`。呼び手は `Engine`（`effective_result_limit`） | 単一寄り（history.rs）。`//!` L3-4 が同文 | 写し。CLAUDE.md 側を消す |
| L63 | crate 外から見える名前は `indexer.rs` の re-export が決める | L17 と同型 | **横断** | 残す |
| L72 | `str_arena.rs` 線上表現は `Vec<Option<String>>` のまま | `str_arena.rs:730,797` ＋ `//!` L13 | 単一。索引行を兼ねる | 写し（`//!`「線上表現は変わっていない」節）。行は残す |
| L73 | `index_tree.rs` 辿る規則はここが唯一（`walk_to_root` / `raw_path_into` / `TreeNodes`） | `index_tree.rs:313,336,388` ＋ `//!` L14「辿る規則は 1 つ」 | 単一。索引行を兼ねる | 写し。行は残す |
| L77 | `autostart.rs` 状態の正本は OS | `autostart.rs` `//!` L4-7 が同文 | 単一。索引行を兼ねる | 写し。行は残す |
| L83-84 | `tests/path_query_cost.rs` は唯一の計器／`…_at_operating_point` は実起動経路を再現・旗を出力に添える | `tests/path_query_cost.rs:185,292` | 単一（統合テスト 1 ファイル） | 同ファイルの `//!`（L1-6 に一部あり） |

### C-2. `src-tauri/CLAUDE.md`（L7-63）

| 行 | bullet | 名指す識別子の定義 | 判定 | 移し先 |
|---|---|---|---|---|
| L11 | 背景再スキャンの spawn と適用は撤去済み | `main.rs` `//!` L4-5 が同文 | 単一（歴史） | 写し。消す（`git log` / ADR が持つ） |
| L13 | ビルド開始/終了は `try_begin_index_build` / `finish_index_build` 経由 | `state.rs:109,123` | 単一（state.rs）。**ただし「実装パターン」L121 に同じ規範の写しがあり `.claude/rules/src-tauri.md` L12 はそちらを指す** | `state.rs` の `///`。L121 との二重化を先に解く（D-3） |
| L14 | コヒーレンシ判断を 2 AtomicBool から導かない | `state.rs`（フラグ）＋ `engine.rs`（ledger） | ⚠️ 横断（crate 越し・snotra-core L15 と対） | 残す（か snotra-core 側と 1 か所へ） |
| L15 | `invalidate_icon_cache` は単一 lock 内で両方無効化 | `icon.rs:191,207` ＋ `//!` L5-6 が同文 | 単一 | 写し。`invalidate_icon_cache` の `///` に TOCTOU の実測（17/2000）を残して CLAUDE.md 側を消す |
| L17 | `start_index_build` は stale → CAS → spawn／`build_index_from_material` に閉じる／起動経路は別に持つ | `indexing.rs:22,114`・`main.rs:214`（`PathMerge`）・`indexer.rs:104` | **横断**（indexing.rs と main.rs の 2 経路が要点） | 残す（前半の順序だけは `indexing.rs` `//!` L3-4 に既在） |
| L18 | ビルド本体は `catch_unwind` で包む | `indexing.rs:62` ＋ `//!` L5 | 単一 | 写し。本文の panic 戦略の詳細を `indexing.rs:54` 付近のコメントへ |
| L19 | finish 後に `is_index_stale` 再チェック・panic 経路では再 kick しない | `indexing.rs`（drain ループ） | 単一 | `indexing.rs` の `//!` か `start_index_build` の `///` |
| L20 | コヒーレンシは engine の ledger に一元化 | — | 横断（L14 と同文の写し） | L14 と統合 |
| L22-23 | `LoadOutcome::ReadFailed` では適用せず早期 return（`should_apply_config_change`）／バウンドリトライ | `config_watcher.rs:187,220` ＋テスト L281,289 | 単一 | `should_apply_config_change` の `///`。`config_watcher.rs` `//!` L5 の文言を直す（D-1） |
| L24 | `icons_turned_off` → `drop_icon_cache` は `update_config` より後 | `config_watcher.rs:203`・`icon.rs:176`・`commands/icon.rs:7`（`ensure_icon_cache_loaded_if_enabled`） | **横断**（3 ファイルの順序と窓） | 残す |
| L25 | 再構築要否は `IndexInputs::from_config` の差分・`!indexing` ゲートなしで kick | `engine.rs:87`・`indexing.rs:22` | 横断 | 残す |
| L30 | `heap_trace.rs` 既定ビルドには入らない | `heap_trace.rs:53,111` ＋ `//!` L3-4 が同文 | 単一。索引行を兼ねる | 写し。行は残す |
| L31 | `startup.rs` 終端を `RegisterInitialHotkey` の arm だけに閉じない | `startup.rs:543,545`・`platform/mod.rs:59,131` | ⚠️ 横断寄り（platform/mod.rs の arm を名指す）。ただし bullet 自身が「正本は `//!`」と宣言 | 索引行を残し本文を消す（`startup.rs` `//!` L64 付近が既に持つ） |
| L32 | `monitor.rs` 基準モニターは必ず点から決める | `monitor.rs:78-92` `point_monitor_work_area` の `///` が同旨 | 単一。索引行 | 写し。行は残す |
| L41 | 起動の入口は `launcher_controller/` の直下に置く | `activation.rs` `//!` L3-13 と `activation/tests.rs` | 単一（launcher_controller 子）。**`//!` の方が新しい主張**（「集める規範は要らなくなった」）で CLAUDE.md と温度差 | `//!` を正本に、CLAUDE.md 側を消す |
| L47 | ソーステキスト検査は対象モジュールの子（`activation/tests.rs`） | 非太字。`tests.rs` の唯一の言及 | 索引 | **動かさない**（B-2(a)） |
| L58-62 | `window_coordinator.rs` の 4 規則（2 か所の高さ・3 箇所の基準モニター・`read_bar_anchor`・`MonitorFromWindow` 禁止） | `window_coordinator.rs:219,564,687,774,810,1021`・`layout.rs:105,346,366`・`notify.rs:54`・`view.rs`・`monitor.rs:90` | **横断**（5 ファイル） | 残す（参照 5 本が「`window_coordinator.rs` の項」を名指す） |

---

## D. 見落としやすい依存

### D-1. 移し先の `.rs` が既に CLAUDE.md を正本として指しているもの（自己参照になる）
- `snotra-core/src/config/schema.rs:7` `//!`「射程と検知器は `snotra-core/CLAUDE.md`」→ L19 と対。
- `snotra-core/src/search/tests/performance.rs:103`・`snotra-core/src/indexer/cache/breakdown.rs:167` → L57（footprint 節）と対。
- `snotra-core/src/str_arena.rs:116` → L52。
- `snotra-core/tests/path_query_cost.rs:3` → L44（「モジュール構成」の search.rs 節）。
- `src-tauri/src/config_watcher.rs:5` `//!` → L22-25 全体。
- `src-tauri/src/heap_trace.rs:22` は**索引行**を撤去対象として名指す（本文ではなく行）。行を残す限り真。
- `snotra-core/src/config/paths.rs:5` → L22（非太字）と L35-36（横断）。

### D-2. 「モジュール構成」の後ろに散文で節を特定している参照（機構は緑のまま、意味だけ腐る）
- 「の `window_coordinator.rs` の項」: `view.rs:972,1248`・`mod.rs:21`・`layout.rs:394`・`window_coordinator.rs:24`（L58-62 は横断＝残すので影響なし）。
- 「の search.rs 節」: `tests/path_query_cost.rs:3`（L44）・`.claude/rules/snotra-core-search.md:17`（L39・散文形）・`breakdown.rs:167`（L57）。
- 「の #1032 条項」「の当該条項」: `state.rs:101`・`activation.rs:348`・`search_flow.rs:44`・`window_coordinator.rs:190`・`docs/architecture.md:247` → `###` 小節（対象外）。
- 「の trace 規範」: `view.rs:1214` → `###`（対象外）。
- `docs/design/2026-05-31-coherence-staleset.md:15` → L14-16。

### D-3. 同じ規範の写しが CLAUDE.md の別節にもあるもの
- src-tauri L13（モジュール構成）と L121（実装パターン）: `try_begin_index_build` / `finish_index_build` 経由の規範が 2 か所。`.claude/rules/src-tauri.md` L12 は L121 側を指す。**L13 を `state.rs` へ移すなら L121 も同時に正本 1 か所へ**（`AGENTS.md`「条件別チェック」の「文書に事実の写しを増やす変更」行）。
- src-tauri L14 / L20 / L25 と snotra-core L15: 「コヒーレンシは engine の `index_stale` ledger」が 4 行。
- snotra-core L41 と「`normalize_entry_key` の冪等性契約」L122・`PERFORMANCE.md:704`・`SPEC.md:813`。

### D-4. `.claude/rules/` から索引節の bullet を指しているもの
- `.claude/rules/snotra-core.md` L10-17 は「実装前チェック」「冪等性契約」「char_bitmask」「index.bin 排他」「索引更新の契機」「scan_all」「開発ルール」——**すべて「モジュール構成」の外**。影響なし。
- `.claude/rules/snotra-core-search.md` L17 だけが「search.rs 節」を散文で指す（L39/L42/L44 が対象）。
- `.claude/rules/src-tauri.md` L12-16 は「実装パターン」「ウィンドウ生成の制約」「Win32 / Tauri 注意事項」「engine.rs のロック最小化パターン」——外。

### D-5. `PERFORMANCE.md` / `SPEC.md` から
- 両者が指す入れ子 CLAUDE.md の見出しは B-2(b) の一覧のとおり**すべて「モジュール構成」の外**（`PERFORMANCE.md:85,106,704,814,991,1250,1543`・`SPEC.md:625,765,813,814`）。影響なし。
- 逆向き: 対象 bullet が `PERFORMANCE.md`「採用: …」を正準形で指す（L39, L40, L47, L56, L84）。`.rs` へ移すときは 1 物理行に収める（B-2(c)）。

### D-6. 面積計器が入れ子 CLAUDE.md を数え始めることの副作用
- `instrument.mjs` L23-25 の「対象外」宣言と `ADR-area-metric-characters` L25 の却下理由が「課税」を根拠にしている。報告欄は課税ではないが、**成功行に出た数字は次のサイクルで「削れ」の圧力になりうる**——`ADR-retire-area-budget` L24 が記録した「数字に押されて上限の無い面へ逃がす」副作用の逆向き。合否を持たないことを欄の名前か doc に明記する ⚠️（判断は plan 側）。

---

## E. 変更しなくてよいのに変更したくなるもの（スコープ過剰の候補）

1. **`###` 小節（src-tauri L65-113）** — L36 が「横断不変条件は本節の `###` 各項」と宣言済み。参照 15 本超が着地している。対象外。
2. **snotra-core の後半節**（「クロスモジュール不変条件」L112-146・「データ永続化の注意」・「IndexCache バージョン変更チェックリスト」・「engine.rs のロック最小化パターン」・「index.bin 書き込みの排他」以降）— 「モジュール構成」の外であり `PERFORMANCE.md` / `SPEC.md` / rules / skills の参照先。対象外。
3. **`snotra-settings/CLAUDE.md` / `snotra-egui-runtime/CLAUDE.md` の bullet 整理** — 要求 1 は 2 枚に限る。要求 2 の母集団（4 枚）とは別。
4. **`G-module-index` の判定変更**（索引行の所有関係を見る等）— `ADR-module-index-reverse-scope` で却下済み。§0 の帰結は「行を残す」で足りる。
5. **ADR の編集**（`ADR-retire-area-budget` L10 の成功行の形・`ADR-area-metric-characters` L25）— 凍結。
6. **`.claude/skills/health-check/SKILL.md` の Check 1 / 面積計器の記述** — 母集団は関数が持つと書いてあり写しではない。
7. **`docs/comment-guidelines.md` L108 の模範例**（`search.rs` / `opener.rs` / `working_set.rs` / `state.rs`）— 今回の移設で模範例が増えても足さない（例の列挙は腐る側）。
8. **`AGENTS.md` L73「ファイル（`.rs`）を追加/削除」行** — 索引の規範は変わらない。
9. **`docs/architecture.md` L55/L61「→ モジュール構成は …」** — ファイルを指すだけ。変わらない。
10. **写しの解消（D-3）を全部やる誘惑** — L13/L121 のように対象 bullet と直結するものだけ。snotra-core L41 と冪等性契約節の重複は別 issue の規模。
11. **`instrument.mjs` の `ALWAYS_LOADED_FILES` を `governanceDocs` 由来へ置き換える等の一般化** — `ALWAYS_LOADED_FILES` L29-34 の doc が「保証は狭い」と宣言して受容している。今回は面を 1 つ足すだけ。

---

## F. 着手前に測るべきもの（plan の検証項目の候補）

- 移設後に `npm run governance:check` を走らせ、成功行の `.rs` 件数・見出し参照件数が**減っていない**こと（`.rs` の走査元は増えても減らない）と、`G-module-index` の finding 0 件。
- `cargo doc --workspace --no-deps --document-private-items` を**手で**走らせる（hook は走らない・`docs/build-commands.md` L32）。
- 変異注入 1 本: 索引行を 1 つ消して `governance:check` が当該 basename を名指しで赤にすること（§0 の帰結の実測。`.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」）。
- 要求 2: `evidence.mjs` の template に新キーを足した状態で `governance-check.mjs` 側の供給を落とし、`evidence が読む \`areaNested\` が未記録` の finding が出ること（消費点＝母集団の実測）。
