# research — issue #1240: 入れ子 CLAUDE.md の単一ファイル不変条件を `//!` へ移し、面積計器の母集団に入れ子を足す

調査日: 2026-09-06 / ブランチ: `chore/nested-claude-md-invariants-to-module-doc`

## issue の要約

- `snotra-core/CLAUDE.md`（71,419 字）と `src-tauri/CLAUDE.md`（57,892 字）が約 40k 字の閾値を超える。膨らみの主因は「モジュール構成」節（31.4k / 35.8k）に**単一ファイルにしか効かない不変条件**が bullet として積まれていること
- 対応 1: それらをそのファイルの `//!` / `///` へ移す（#562 の後半）。対応 2: `G-area-instrument` の報告行に入れ子 `CLAUDE.md` の字数を足す（合否なし）
- 受け入れ条件: 両ファイル 40k 字未満 / 消した bullet 全数の再確立地点対応表 / `governance:check` 緑 + 入れ子字数の報告 / `cargo doc` 緑 / `rules/snotra-core-search.md` の指し先一致

## 関連ファイル・モジュール・関数（実在を grep で確認済み）

### 文書側

| ファイル | 役割 |
|---|---|
| `snotra-core/CLAUDE.md` | 対象 1。「モジュール構成」は L8〜85。他の `##` は L96 以降（実装前チェック・クロスモジュール不変条件・永続化・IndexCache チェックリスト・排他・索引更新の契機・…） |
| `src-tauri/CLAUDE.md` | 対象 2。「モジュール構成」は L7〜64。他に「Win32 / Tauri 注意事項」(11.4k)・「実装パターン」等 |
| `docs/comment-guidelines.md` | 移し先の様式の正本。「配置基準（3 層）」の表: `//!` = 責務・依存方向・モジュール全体の不変条件、`///` = 契約・保証・局所理由・測定値、`#NNN`/ADR = 経緯・却下案。「短く保つ」: 削らず層を分ける。「日本語の折返し」: コードスパンと正準形参照を行またぎさせない（2 形のみ） |
| `.claude/rules/comments.md` | `.rs` 編集で自動配送。`cargo doc` を手で走らせる指示 |
| `.claude/rules/snotra-core.md` / `snotra-core-search.md` / `src-tauri.md` | ルーター。`snotra-core-search.md` は「横断不変条件（並列 Vec レイアウト・ビットマスク一元化・has_path_sep 非互換）は `snotra-core/CLAUDE.md` の search.rs 節」を指す（付け替え対象）。`src-tauri.md` は「実装パターン」「ウィンドウ生成の制約」「Win32 / Tauri 注意事項」「engine.rs のロック最小化パターン」を指す（索引節の外・本 issue で動かさない） |
| `docs/build-commands.md` L23/L32 | `cargo doc --workspace --no-deps --document-private-items` は CI のみ発火・hook 非発火 |
| `docs/adr/ADR-module-index-reverse-scope.md` | 索引行の所有関係を機構で見ないと決めた。basename 包含方式の残余 |
| `docs/adr/ADR-retire-area-budget.md` / `ADR-doc-promise-over-area-ratchet.md` | 面積は合否を持たない・一次規範は「書く約束」 |

### 機構側

| ファイル・シンボル | 本 issue との関係 |
|---|---|
| `scripts/governance/checks/G-module-index.mjs` `checkModuleIndex` / `MODULE_INDEX_CRATES` / `moduleIndexSources` | 「モジュール構成」節（`sectionOf(/^## モジュール構成$/)`）を照合。**逆方向**: `src/` 配下の全 `.rs`（テストを含む・`excludeTest` 未設定）の basename が節内にバッククォート付きで現れること。**順方向**: 節内の `` `x.rs` `` が実在すること。索引行かどうかは見ない |
| `scripts/governance/instrument.mjs` `ALWAYS_LOADED_FILES` / `sumChars` / `normativeArea` / `checkNormativeAreaInstrument` | 面積計器。母集団は `CLAUDE.md` + `AGENTS.md` + skill description（常時ロード）と `.claude/rules/*.md`。**モジュール CLAUDE.md は明示的に対象外**で、その理由がヘッダに書いてある（下「技術的制約」） |
| `scripts/governance/evidence.mjs` L105 / `scripts/governance-check.mjs` L178〜201 | 成功行「恒久規範 常時ロード N 字・rules N 字」の組み立て。`evidence.test.mjs` が `areaAlways` 欠落を赤にする（新しい欄も同型で守る） |
| `scripts/governance/instrument.test.mjs` | 計器のテスト（母集団欠落だけを判定する形を固定） |
| `scripts/governance/lib.mjs` `HEADING_REF` / `checks/G-heading-refs.mjs` | 正準形 `` `<対象>`「<見出し>」 `` の実在照合。アンカーは ATX 見出し・番号付きリスト・**太字リード**。前方一致 |
| `scripts/governance/edit-findings.mjs` | 入れ子 `CLAUDE.md` 編集時の reminder が `checkModuleIndex` をそのまま呼ぶ |

### コード側（移し先候補と現状の `//!` 行数）

| ファイル | `//!` 行数 | 備考 |
|---|---|---|
| `snotra-core/src/search.rs` | 6 | 並列 Vec の理由は「`SearchEngine` の struct doc を参照」と書く。索引 L32〜37 の内容は無い（`並列の列は添字` / `migemo トグル` は `.rs` 側 0 件） |
| `snotra-core/src/search/scoring.rs` | 7 | `with_normalized_key` あり |
| `snotra-core/src/search/build.rs` | 9 | `assemble` / `from_material` の所在 |
| `snotra-core/src/search/path_store.rs` | 46 | 既に長い。索引 L41 との重なりを移す前に確認する |
| `snotra-core/src/search/query_plan.rs` | 7 | |
| `snotra-core/src/indexer.rs` | 11 | `IndexMaterial` |
| `snotra-core/src/indexer/cache.rs` | 12 | **`INDEX_WRITE_LOCK` の直列化・走査の契機を既に `//!` に持つ**（`INDEX_WRITE_LOCK` 8 箇所）。索引節外の「index.bin 書き込みの排他」「indexer.rs の索引更新の契機」と写しの関係 |
| `snotra-core/src/config.rs` / `config/schema.rs` / `config/migrate.rs` / `config/location.rs` | 15 / 7 / 11 / 6 | L18 は「契約の全文は `config_dir` / `config_dir_from` の rustdoc」と自ら正本を指す |
| `snotra-core/src/history.rs` / `engine.rs` | 4 / 7 | |
| `src-tauri/src/egui_shell/window_coordinator.rs` | 29 | **`//!` が「共有の実体の正本は `src-tauri/CLAUDE.md`「モジュール構成」の `window_coordinator.rs` の項」と CLAUDE.md を正本に指名している**（向きを反転する対象）。`MonitorFromWindow` は `.rs` 側 0 件 |
| `src-tauri/src/indexing.rs` / `config_watcher.rs` / `state.rs` | 5 / 5 / 6 | 索引の bullet が最も厚い 3 ファイル |

## 索引節の bullet の分類（機械的な基準）

**基準**: bullet が名指す識別子（関数・型・フィールド・テスト名）の**定義**がすべて 1 つのファイル（またはその子モジュール `x/`）にあるなら「単一」→ 移す。2 つ以上のファイル（別 crate を含む）に分かれるなら「横断」→ 残す。禁止文（「〜してはならない」）を含む bullet は、その禁止が掛かる関数の `///` へ置く（残りは文脈として同じ場所へ付ける）。

### snotra-core（**注意: 本表の L 番号は節内の相対行で、ファイル行は +7**。`workspace/plan.md` はファイル行で書く）

| L | 主題 | 判定 | 移し先 |
|---|---|---|---|
| 7〜9 | `IndexInputs` / `index_stale` ledger / `complete_index_drain` | 単一（定義は engine.rs） | `engine.rs` `//!`（消費者が src-tauri に在る旨は 1 文で残す） |
| 10 | config.rs の re-export が外部名を決める | 単一 | `config.rs` `//!` |
| 12 | serde 既定（#824）と検知器 | 単一 | `config/schema.rs` `//!` |
| 13 | 件数パラメータの名前と役割 | 単一（フィールド定義は schema.rs） | 各フィールドの `///` |
| 14 | `icon_cache_cap` 派生 | 単一 | `config.rs`（既存 `保守注意:` の隣） |
| 15 | paths.rs の正規化 2 つの共有 | 単一 | `config/paths.rs` `//!` |
| 17〜18 | 保存先導出・env 上書き | 単一（L18 は既に rustdoc が正本） | `config/location.rs` `//!`（重複分は削る） |
| 19 | io.rs → 「データ永続化の注意」 | ポインタ | 残す |
| 21〜24 | migration の書き方・順序・検知器 | 単一 | `config/migrate.rs` `//!` と `apply_migrations` `///` |
| 25 | validate は検出のみ | 単一 | `config/validate.rs` `//!` |
| 26 | テストは子モジュールに置く | 横断（crate 規約） | 残す |
| 28〜29 | opener の依存方向 | 依存方向 = `//!` の面 | `opener.rs` `//!` |
| 30 | hotkey の parser 非複製 | 単一 | `hotkey.rs` `//!` |
| 32 | 並列の列は添字 | 単一 | `search.rs` `//!` / `SearchEngine` struct doc |
| 33 | kana の逐次 push 禁止・検知器 | 単一（build.rs） | `search/build.rs` `new_with_cached_masks` `///` |
| 34 | 正規化キーは索引に持たない・別実装禁止 | 単一（scoring.rs） | `search/scoring.rs` `with_normalized_key` `///` |
| 35 | kana 列は migemo 有効時のみ | 単一 | `search.rs` `//!` |
| 36 | migemo トグルは再構築経由 | **横断**（`update_config` は engine.rs・kick は src-tauri `config_watcher`） | 残す |
| 37 | パスマッチングのスコア | 単一 | `search/scoring.rs`（式は `///`） |
| 38 / 39 / 42 / 50 | scoring / query_plan / build / footprint の「何をここへ置くか」 | 単一 | 各 `//!`。**索引にはファイル名行を必ず残す**（下「技術的制約」の逆方向照合） |
| 40 | `sorted_prefix_len`・測り直し禁止 | 禁止は path_store の producer | `search/path_store.rs` `///` |
| 41 | PathStore の 2 系統・セグメント比較禁止 | 単一 | `search/path_store.rs` `//!`（46 行と突き合わせ、重複は削る） |
| 43〜47 | 派生文字列の潰し（assemble 側） | 単一（build.rs） | `search/build.rs` `assemble` `///` |
| 48 | 共有判定は `measure_derived_sharing` を通す | 3 経路が同じ関数を通る規律 | `query.rs` `measure_derived_sharing` `///`（呼び出し 3 点を名指し） |
| 49 | from_material / IndexMaterial / PATH マージの検知器 | 単一寄り（自ら「正本は `IndexMaterial` の doc」と言う部分あり） | `search/build.rs` `from_material` `///` + `indexer.rs` `IndexMaterial` `///`（重複は削る） |
| 51 | search/tests の索引 | 横断（ファイル一覧・G-module-index が basename を要求） | 残す |
| 53〜54 | history の `top_n` live-read | 単一 | `history.rs` `//!` |
| 56 | indexer.rs の re-export | 単一 | `indexer.rs` `//!` |
| 57〜63 | indexer 子のポインタ・テスト規約 | ポインタ | 残す |
| 65 / 66 / 70 / 72 / 76〜77 | str_arena / index_tree / autostart / instant / tests の太字文 | 単一 | 各 `//!` |

見込み: 太字 bullet 22.5k のうち残すのは L26・36・51・63 と各ファイル名行 ≒ 4k → **索引節で −18k**。残り 71.4k − 18k ≒ 53k で、**索引節だけでは 40k を切らない**（下「未解決の疑問」1）。

### src-tauri（L 番号は `src-tauri/CLAUDE.md`）

| L | 主題 | 判定 | 移し先 |
|---|---|---|---|
| main.rs | 背景再スキャン撤去済み・明示操作のみ | 経緯 + 規律 | `main.rs` `//!` 1 文（ADR-rescan-explicit-only を指す） |
| state.rs ×2 | ビルドフラグの規律 | 単一 | `state.rs` `//!` |
| icon.rs | `invalidate_icon_cache` の TOCTOU | 単一 | `icon.rs` `invalidate_icon_cache` `///` |
| indexing.rs ×4 | drain / catch_unwind / 再チェック | 単一（`build_index_from_material` は自ら「詳細は同関数の doc」） | `indexing.rs` `//!` と `start_index_build` `///` |
| config_watcher.rs ×5 | ReadFailed 早期 return・破棄の順序・再構築判定・イベント一覧 | 単一 | `config_watcher.rs` `//!`（イベント一覧は `events.rs` `//!` でも可） |
| heap_trace / startup / monitor | 太字文 | 単一 | 各 `//!` |
| commands/ / platform/ | 集約行 | 例外（責務をここに書く設計） | 残す |
| launcher_controller/activation | 入口の置き場 | 単一（自ら「死角の正本は同ファイルの `//!`」） | `activation.rs` `//!` |
| layout.rs | 撤去済み一覧（#646/#752/#835） | 経緯 | `layout.rs` `//!` は現在形だけ、経緯は `#NNN` に任せて索引から削る |
| window_coordinator.rs ×4 | 窓の幾何の 4 規則 | 単一（第 4 規則は `monitor::point_monitor_work_area` の doc が正本） | `window_coordinator.rs` `//!`。**同ファイルの `//!` が CLAUDE.md を正本に指名している文を反転する** |

見込み: 索引節 35.8k のうち移せるのは約 20k → 57.9k − 20k ≒ **38k**（40k をわずかに切る）。

## 再利用できる既存パターン

- **#562 の手順そのもの**（責務散文 → `//!`、`CLAUDE.md` は索引 + 横断へ）。写しの畳み方は #937 の commit `0dde304`
- `docs/comment-guidelines.md`「定型ラベル」の `不変条件:` / `保守注意:` / `回帰テスト:`。`# Why ...` 見出しで長文を構造化する様式（模範は `snotra-core/src/search.rs`）
- 参照の付け替えを機構で捕まえる形: `governance:check` の G-heading-refs（正準形）・G-module-index（basename）・散文の識別子照合・`cargo doc` の intra-doc link（`[`X`]`）
- 「消した行の不変条件を名指しし、再確立地点を探す」逆向きの監査（ルート `CLAUDE.md`「サブエージェント委譲と worktree」）

## 技術的制約

1. **索引節からファイル名の言及を消してはならない。** `search/scoring.rs` / `search/footprint.rs` / `search/tests/*.rs` 等は太字 bullet の中でしか名指されていない（L38・50・51）。bullet を移すときは `` - `search/scoring.rs` — 責務は `//!` `` の 1 行を残さないと G-module-index の逆方向が赤になる（これが移行漏れ検出器として働く）
2. **正準形の見出し参照のアンカーは残す。** 索引節の外の `##`/`###` は `SPEC.md` / `PERFORMANCE.md` / skills / rules / `.rs` から正準形で参照されている（例: 「IndexCache バージョン変更チェックリスト」は `cache.rs` `//!` と `PERFORMANCE.md`、「Win32 / Tauri 注意事項」は 7 箇所）。太字リードもアンカーになる（「テーマ色・font・行高の読みは 1 フレーム 1 回」は `code-reviewer.md` から）。**索引節の太字 bullet を正準形で指す参照は 0 件**（Grep で全ツリー確認）——bullet を移しても G-heading-refs は赤にならない。ゆえに bullet の消失は G-module-index の basename か逆向きの監査でしか捕まらない
3. **`window_coordinator.rs` の `//!` は CLAUDE.md「モジュール構成」を正本に指名している。** 移すと自己参照になるので、その文を「本 `//!` が正本」へ反転し、CLAUDE.md 側の項をポインタにする（両方を同じ変更で）
4. **面積計器は入れ子 CLAUDE.md を意図して外している。** `instrument.mjs` ヘッダ: 「skills 本文・モジュール CLAUDE.md・docs・ADR は対象外——『その作業に入った者だけが読む面』への退去は #593 が推奨する経路であり、課税すれば登ってほしい階梯を登る側が罰せられる」。合否を持たない今の計器で「報告に載せる」ことがこの理由に触れるかは判断が要る（下「未解決の疑問」2）。足すなら**別欄**（`入れ子 CLAUDE.md N 字`）にして常時ロード面に混ぜない。母集団は `MODULE_INDEX_CRATES` のキー（4 crate）から導き、一覧を二重に書かない。`evidence.test.mjs` の欠落検知と `instrument.test.mjs` の母集団欠落テストを同型で足す
5. **`cargo doc` は hook で走らない。** `.rs` の doc を触ったら `cargo doc --workspace --no-deps --document-private-items` を手で走らせる（`broken_intra_doc_links` は deny）。`#[cfg(test)]` 配下の doc は視界の外
6. **折返しの禁則 2 形**: `//!` へ移す文の中のコードスパンと正準形参照を行またぎさせない。`G-folded-heading-refs` / `G-near-heading-refs` が `.rs` のコメントも見る
7. **`.md` の非リンク角括弧**: `//!` 内の `[...]` は rustdoc が intra-doc link として解釈する。移す文に `[[paths.scan]]` 等があれば backtick で包む（既に包まれているか確認）
8. **入れ子 CLAUDE.md の編集は reminder が鳴る**（`edit-findings.mjs` が `checkModuleIndex` を呼ぶ）が、鳴らないことを緑と読まない（射程は basename の包含だけ）
9. **`.rs` の編集は post-edit hook が cargo 系の検査を走らせる**（fmt が先頭）。doc コメントだけの変更でも走るので、Phase を crate ごとに切ってコミット単位を保つ

## 未解決の疑問

（2026-09-06 人間の判断: 「続行（根拠を差し替え）」——数値目標を外し、対応 1 は索引節の太字 bullet に限定、対応 2 はそのまま。1 と 2 はこれで決着。3・4 は計画の未確定欄へ）

1. **[決着: 範囲は広げない]** snotra-core は索引節だけでは 40k を切らない（見込み 53k）。同じ基準を索引節の外の `##` にも当てるか。候補と根拠: 「index.bin 書き込みの排他」(2.6k) と「indexer.rs の索引更新の契機」(3.8k) は `cache.rs` の `//!` が既に同じ規律を持つ（写し）。「IndexCache バージョン変更チェックリスト」(5k) は `cache.rs` を編集する者だけが要る。「`scan_all` の重複排除」(2k) は `indexer/scan.rs` 単一。「Config のデシリアライズ経路」(1.5k) は `migrate.rs` から参照される単一。合計 ≒ 15k で 40k を切る。**見出しは正準形の参照先なので、本文を 1〜2 行のポインタにして見出し自体は残す**。issue の対象は「モジュール構成」節と書いてあるので、範囲の拡張は要求判断（→ 人間へ確認）
2. **計器に入れ子を足すことと `instrument.mjs` ヘッダの理由の整合**。合否を持たない別欄なら「課税」には当たらないと読むが、ヘッダの文言を書き換える必要がある（#593 の経路を罰しないことを明記して、報告だけ足す）。要求判断（→ 人間へ確認）
3. `path_store.rs`（`//!` 46 行）と索引 L41 の重なりの量——移す前に突き合わせて、重複分は移さず削るだけにする。実装時に測る
4. `//!` に移した長文が「module doc が複数の設計論点を抱える」形にならないか。`docs/comment-guidelines.md`「短く保つ」に従い、契約は `///` へ降ろす。判定は bullet ごとに実装時に行う（基準は上表の「移し先」列）

## 敵対的調査（3b）の所見と採否

枠: general-purpose / sonnet ×1。出力 `workspace/adversarial-1240.txt`。

| 所見 | 判定 | 採否と反映 |
|---|---|---|
| **単位不一致**: 71,419 / 57,892 は `wc -c`（バイト）。計器 `countChars`（コードポイント・CR 除く）では **38,664 / 31,782** で両ファイルとも 40k 未満。CRLF ではない | 確定（主エージェントも独立に同じ実測） | **採る**。issue の前提「40k 閾値超え」は偽。Claude Code の閾値は JS の文字数（`getMaxMemoryCharacterCount`）でコードポイントに近く、バイトではない。対応 1 の動機は「数値目標」から「置き場の是正（co-location）」へ改める。issue 本文の訂正が要る |
| **src-tauri の索引節は L7〜114**（`sectionOf` は `###` で閉じない）。L65〜114 の 7 つの `###` は分類表に無い | 確定（`grep -n "^## "` → 7 / 115） | **採る**。7 つの `###` はいずれも複数ファイルにまたがる横断規則で「残す」。分類表の行は正しいが、見込みは母集団の半分を見ずに出ていた |
| ⚠️ L40 `sorted_prefix_len` は `index_tree.rs` にも同名フィールドがある | 中確信 | **一部採る**。index_tree 側は列の受け渡し型のフィールドで規律の別定義ではない。移し先は path_store のまま、`///` に「値は `TreeColumns` 経由で運ぶ」の 1 文を添える |
| 壊せなかった: 正準形参照 0 件（サンプル 4）・excludeTest 不在・instrument ヘッダ逐語・window_coordinator の自己参照・post-edit は .rs 全般に無条件発火・4 識別子の定義は単一 | — | 維持 |
| 未検査: 太字 bullet 全数の参照悉皆・governance:check 実行・内訳検算 | — | 主エージェント側で実施済み: governance:check は 24 件緑（本日実測）、内訳はコードポイントで再計測（下） |

### 訂正後の数字（コードポイント・CR 除く。計器と同じ数え方）

| ファイル | 全体 | 索引節 | 太字 bullet | `###` 配下 |
|---|---|---|---|---|
| snotra-core/CLAUDE.md | 38,664 | 17,861（L8〜85） | 12,952 | 0 |
| src-tauri/CLAUDE.md | 31,782 | 20,033（L7〜114） | 4,033 | 9,827（横断・残す） |

**帰結**: 「40k 未満」という受け入れ条件は現状で満たされており、削減の数値目標は動機にならない。残る根拠は (a) 単一ファイルの不変条件が `//!` ではなく CLAUDE.md にだけ在る置き場の誤り（co-location・`docs/comment-guidelines.md` の配置基準）、(b) 面積計器が入れ子を数えていない盲点、(c) 当該 crate で作業する全セッションが毎回読む量（38k 字 ≒ 2 万 token 前後）。**「未解決の疑問」1（索引節の外へ範囲を広げるか）は数値目標が消えたため前提を失う**——範囲は索引節の太字 bullet に留める。
