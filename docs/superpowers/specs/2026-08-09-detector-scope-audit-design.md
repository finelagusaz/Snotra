# 検知器の射程監査 — 「検知する」と称して検知していない箇所を数え上げる

作成日: 2026-08-09 / ブランチ: `chore/detector-scope-audit` / issue: #1008

#1000 のサイクルで、doc の主張が実際の射程と食い違う事例が 3 度出た（3 件とも外部レンズが捕まえ、
自分では 1 件も気づいていない）。**同じ弱さを持つ検査が他にどれだけ在るかは、数えれば分かる。**

## 1. 目的と非目標

**目的**: 「網羅を守る」と読める検査のうち、実際には守っていないものを数え上げ、各件の射程を
実際の姿へ揃える。

**非目標**（issue が実測を根拠に否定している）:

- **条項の新設** — `ADR-retire-norm-review` が実測で示したとおり、規範のフォールトインジェクションは
  判別力ゼロであり、塞ぐほど悪化した（`/race-check` の 2 巡で詰まる箇所が 18 → 21）
- **`governance:check` への検査追加** — 「doc の主張と検査の射程が合っているか」は意味的整合であり
  機械判定できない。`AGENTS.md` が「受容する残余」と定めている領域である

## 2. 対照となる既知の 2 件

同じ弱さを持ちながら、**扱いが対照的で、どちらも正しい**。

| 姿 | 実物 | 何をしているか |
|---|---|---|
| **射程を doc に書く** | `src-tauri/src/events.rs` の `event_names_are_pairwise_distinct` | 「保証は狭い……将来の追加を守る機構ではなく、現時点のコピペ重複を弾くだけ」と明記 |
| **母集団をソーステキストへ移す** | `src-tauri/src/startup.rs` の `count_matches_the_enum_declaration` | `include_str!` で自分のソースを走査し、enum 宣言の variant 数と `COUNT` を照合 |

**多くの場合、狭い保証で十分である。** 機構へ倒すのは、その一覧の足し忘れが**製品の欠陥になる**ときだけ。

## 3. 母集団 — 3 層と実測件数

| 層 | 母集団の取り方 | 件数 |
|---|---|---:|
| Rust | **cargo に問う**: `cargo test --workspace -- --list \| grep -c ': test$'` | **905** |
| ガバナンス | `grep -c "id: 'G-" scripts/governance-check.mjs` | **19** |
| スモーク | `scripts/lib/SnotraTraceInvariants.psm1` の `$script:Invariants` | **3**（H1 / H4 / H5・H2/H3 は欠番） |

### 3.1 なぜ grep で列挙しないか

**Rust の列挙は grep を使わない。** `cargo test -- --list` は「テキストがどう書かれているか」ではなく
**何が登録されたか**を返すため、属性の挟まり・`#[cfg]` での消滅・マクロ生成のテストを正しく扱う
（`AGENTS.md`「列挙も SSOT のツール自身に問う」）。

grep で同じことをしようとした過程で 1 件実測した: 素朴な `grep -A1 '#\[test\]' | grep 'fn '` は
**880 件しか取れず 25 件を落とす**（`#[ignore]` / `#[cfg(windows)]` が属性の間に挟まる形）。
**全列挙を主軸にすると決めた当の抽出が、パターンの形で母集団を削っていた** — issue の ⚠ 節が警告した罠の
実例である。属性を飛ばす awk を書けば 905 に一致したが、**それは cargo の答えと突き合わせて初めて
「一致した」と言える**のであって、awk 単独では正しさを主張できない。

**残る 22 件（G-* と H）に `--list` 相当の口は無い。** ここは手作業だが全数を目で見られる規模である。
なお `Get-SnotraTraceInvariantNames` は手書き一覧を返す関数であり、**それ自体が監査対象**である。

## 4. 手順

### Phase 1 — 全列挙

§3 のコマンドで各層の一覧を作る。**この一覧が母集団である。**

### Phase 2 — 篩（母集団の SSOT を特定する）

905 + 22 件の一覧に目を通し、**一覧・配列・`match` の腕を走査している検査**を候補として抜く。
大半の検査は一覧を走査していないので、ここで候補は大きく絞れる。

抜いた候補それぞれについて、**「この検査が走査している一覧の、正しい母集団は誰が知っているか」を問う。**

| 母集団の SSOT | 例 | 定型の変異 | 倒し先の機構 |
|---|---|---|---|
| **コンパイラ** | enum variant / struct field / trait impl | variant を 1 つ足す | derive（`EnumCount` / `EnumIter`）・網羅 `match` |
| **ソーステキスト** | `const` の集合（`events.rs` の 9 定数）、モジュール内の関数 | `const` を 1 つ足す | `include_str!` 走査（`startup.rs` の姿） |
| **ファイルシステム** | 対象文書 35 件、rules の glob | ファイルを 1 つ足す | ディレクトリ走査 |
| **外部設定** | CI job、npm script | job を 1 つ足す | 設定ファイルの解析 |

**この問いは意味ではなく事実を問う。** 「網羅を守ると主張しているか」は文言の意味の問題で判定がぶれるが、
「その一覧の母集団を誰が知っているか」は構造の問題であり、答えが一意に決まる。**そして分類が決まれば
変異の形も倒し先も決まる** — 候補ごとに変異を設計する必要が無くなる（これが §7 の費用リスクを消す）。

**検算**: 構文パターン起点の grep（`let all = [` / `: [T; N] = [` / `.iter().map(` 等・粗く 103 件）と
全称文言起点の grep（「網羅」「すべての」「全 variant」・粗く 39 件）を走らせ、篩が拾えていたかを見る。
**grep は母集団の決定には使わず、篩の見落としの検算にだけ使う。** 差分が出たら篩の基準の側を直す。

### Phase 3 — 確定（定型変異）

候補それぞれに、§4 の分類が定める**定型の変異**を当て、実際に落ちるかを測る。落ちなければ射程不一致として確定。

**文言の読みは篩、変異の実測は確定** — この非対称が判定手段の設計を決めている。#1000 の 3 件のうち
「足し忘れると落ちる」と書いた 1 件は、文言もコードももっともらしく見え、**実際に variant を足して
測って初めて**「1 本も落ちない」と判明した。篩だけで閉じるとこの型を取りこぼす。

### Phase 4 — 倒す

- **既定は射程を doc へ明記**（§2 の `events.rs` の姿）。狭い保証で十分なら、そう書けば済む
- **機構へ倒す**のは、足し忘れが製品の欠陥になるときだけ。倒し先は §4 の分類が示す

**derive（`strum` 等）の導入は分類の結果を見てから判断する。** ワークスペースに enum 補助 crate は
現在 1 つも無く（実測）、依存追加はこの issue の射程を超えうる。**「コンパイラが SSOT」の件数が
分類で判明してから、この issue で入れるか別 issue へ切るかを決める。**

倒した後で**再び変異を当て**、doc どおりの挙動を実測する（射程を書いた側は落ちないまま、機構へ倒した側は
落ちる）。`.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」。

## 5. 成果物

- **本設計書** — 母集団の取り方（§3）・分類表（§4）・仕分け結果（実装で追記する）
- **各件の射程は当該ソースの doc コメントへ**（`events.rs` の姿。設計書へ写しを置かない）
- **列挙スクリプトは残さない** — Rust は cargo が答えるので書く必要が無く、残る 22 件は一度数えれば済む。
  走り続ける足場を `scripts/` へ置くと「いずれ CI に載せる」圧が生まれ、§1 が否定した
  `governance:check` への検査追加へ構造的に接近する

## 6. 受け入れ

- 母集団が数え上げられ、件数が記録されている（**0 件なら「既知の 2 件で全部だった」と本書に記録して終える**
  — それも成果である）
- 各件が「射程を書く」「機構へ倒す」のどちらかに倒れている
- 倒した後で変異を当て、実際に落ちる/落ちないことを確かめている

## 7. 想定するリスク

- **篩の見落とし** → grep 2 軸との差分で検算する（Phase 2）。ただし grep を母集団の決定には使わない
- **分類が一意に決まらない件が出る** → 「一覧の SSOT が複数にまたがる」形（例: ソーステキストの const を
  ファイルシステムの走査と突き合わせる検査）はありうる。その場合は**変異を分類ごとに 1 つずつ当てる**
  （どちらの足し忘れも検知しないなら 2 件として数える）
- **derive 導入の判断がこの issue に載りきらない** → §4 の分類結果を見て、別 issue へ切る道を残す

## 8. 関連

- #1000（4 件が出たサイクル・PR #1006）
- `docs/adr/ADR-retire-norm-review.md`（規範の検算手段を廃止した実測）
- `docs/adr/ADR-startup-instrument-contract-shape.md`（#1000 で却下した 3 案）

## 9. 候補一覧

3 層合計 **30 件**（Rust 15 件・ガバナンス 14 件・スモーク 1 件）。**この時点では分類も判定も行わない**
（Phase 2 の SSOT 分類は Task 3、Phase 3 の変異確定は Task 4）。

### 9.1 Rust（Task 1）

母集団 905 件（`cargo test --workspace -- --list | grep -c ': test$'`、§3 の記載と一致）を全数読み、
テスト本体が一覧・配列・`match` の腕を走査しているものを候補として抜いた。件数: **15 件**。

| # | 候補 |
|---|---|
| 1 | `snotra-settings/src/app.rs::section_table_covers_all_config_fields` |
| 2 | `snotra-settings/src/app.rs::section_table_no_false_positive_when_unchanged` |
| 3 | `snotra-core/src/error.rs::bin_error_source_all_variants_return_none` |
| 4 | `src-tauri/src/events.rs::event_names_are_pairwise_distinct` |
| 5 | `snotra-settings/src/hotkey_input.rs::every_ui_generated_key_is_in_the_core_accepted_set` |
| 6 | `src-tauri/src/startup.rs::count_matches_the_enum_declaration` |
| 7 | `src-tauri/src/startup.rs::every_phase_key_is_present_even_when_skipped` |
| 8 | `src-tauri/src/startup.rs::failure_reasons_are_stable_and_unique` |
| 9 | `src-tauri/src/startup.rs::index_and_from_index_are_inverse_over_the_whole_enum` |
| 10 | `src-tauri/src/startup.rs::keys_are_unique` |
| 11 | `src-tauri/src/startup.rs::out_of_range_index_is_dropped_instead_of_panicking` |
| 12 | `snotra-core/src/hotkey.rs::modifier_aliases_order_duplicates_and_empty_segments_form_one_set` |
| 13 | `snotra-core/src/hotkey.rs::key_aliases_share_one_semantic_key` |
| 14 | `snotra-core/src/hotkey.rs::supported_key_set_parses_case_insensitively` |
| 15 | `src-tauri/src/platform/hotkey.rs::prepared_named_key_aliases_use_the_same_typed_mapping` |

### 9.2 ガバナンス（Task 2）

母集団 19 件（`grep -n "id: 'G-" scripts/governance-check.mjs`、§3 の記載と一致）の実装を読み、
**各検査が検査対象（母集団）をどこから取っているか**を分類した。この層は「手書きの配列・オブジェクト
リテラルから母集団を取るもの」と「ファイルシステム走査・外部ファイル解析（`Cargo.toml` の
`workspaceMembers`・doc 内の表・`selectChecks` の import）から動的に取るもの」が混在しており、
**前者だけを候補として抜いた**——後者は母集団の SSOT が走査対象自身にあり、足し忘れの経路が構造的に無い。

**判定基準は「手書きか」だけでなく「足し忘れの向き」である。** 除外リスト（除外し忘れると過剰包含
＝チェック対象が増える方向）と包含フィルタ（追記し忘れると過小包含＝チェックが抜ける方向）は
形が似ていても向きが逆で、前者は本監査の対象外（後述）。修正ラウンド 1・2 で計 4 件の検討漏れを
指摘され（`REF_EXTENSIONS` と、それを機に再走査して見つけた同型の一覧のうち、1 行に同居する別の句
だけを分析して本体の向き判定を書き漏らした 2 件を含む）、同じ観点（包含フィルタか・向きは過小包含か）
で再走査を重ねた。**再走査で列挙した拡張子系ハードコードリテラル 7 件は 1 件残らず候補/除外＋理由の
どちらかへ処分済み**（内訳は表の直後の「候補から外したもの」を参照）。件数: **14 件**。

| # | 候補 | 手書き一覧（識別子・行） | 見落としうる形 |
|---|---|---|---|
| 1 | `G-module-index` | `MODULE_INDEX_CRATES`（governance-check.mjs:92、crate 4 件） | 新 crate を追加しても追記し忘れると、そのモジュール索引は双方向照合されない |
| 2 | `G-module-index` | 順方向照合の拡張子フィルタ（governance-check.mjs:116、無名の正規表現 `rs\|ts\|tsx\|html`） | 「モジュール構成」節に `` `foo.mjs` `` のようなこの 4 拡張子以外のバッククォート参照があっても実在照合されない（`MODULE_INDEX_CRATES` とは独立した 2 本目のハードコード拡張子一覧） |
| 3 | `G-references` | `governanceDocs()`（governance-check.mjs:1339、ルート文書 4 件 + crate CLAUDE.md 正規表現 4 crate） | 新 crate の CLAUDE.md がこの正規表現に無いと、その文書内の参照実在は照合されない |
| 4 | `G-spec-sections` | 同上（`governanceDocs()` を共有） | 同上——新 crate CLAUDE.md 内の `SPEC §N` 参照が照合対象から漏れる |
| 5 | `G-adr-citations` | 同上（`adrCitationDocs` が `docs`＝`governanceDocs()` を含む） | 同上に加え ADR 短縮引用が該当文書内で照合されない（他の入力＝ADR/skills/`.rs`・`.mjs` は走査ベースのため影響は限定的） |
| 6 | `G-references` | `REF_EXTENSIONS`（governance-check.mjs:30、拡張子 11 種） | バッククォート内パス様参照の実在照合は、拡張子がこの一覧に無いファイル種別（`/` を含んでいても）を静かにスキップする（修正ラウンド 1 の指摘） |
| 7 | `G-adr-citations` | `adrCitationDocs` の `.rs\|.mjs` 拡張子ホワイトリスト（governance-check.mjs:1757、`/\.(rs\|mjs)$/`） | `.ts` / `.tsx` / `.ps1` 等の非 docs ソースに ADR の短縮引用があっても実在照合を素通りする（修正ラウンド 2 の指摘。同じ行の `!f.endsWith(".test.mjs")` だけを分析し本体の向き判定を書き漏らしていた） |
| 8 | `G-area-budget` | `ALWAYS_LOADED_FILES`（governance-check.mjs:1052、`["CLAUDE.md", "AGENTS.md"]`） | 常時ロード面に 3 つ目のファイルが増えても追記し忘れると火災報知器の面積に算入されない |
| 9 | `G-stale-identifiers` | `STALE_EXTRA_DOCS`（governance-check.mjs:1505、固定パス 4 件） | 新設した「意図の SSOT」級の文書がここに無いと、腐り識別子の検査対象から漏れる |
| 10 | `G-stale-identifiers` | `VOCAB_TEST_FILE`（governance-check.mjs:1499、`.test.(mjs\|ts\|tsx)` の拡張子 3 種） | この形以外のテスト専用ファイル（Rust の `#[cfg(test)] mod` 等・コメントで残余と明記済み）の語彙が「現行語彙」へ紛れ込み、実在しない識別子が偶然そのテスト専用語彙と一致すると stale 判定から漏れる |
| 11 | `G-stale-identifiers` | `currentVocabulary` のコメント除去振り分け（governance-check.mjs:1556、`/\.(ps1\|toml\|yml)$/` の可否で `#` 除去 or `stripRustComments` を選ぶ） | `VOCAB_SOURCE_EXT`（:1495）へ `#` コメント言語の拡張子を追加してもここへ追記し忘れると、その言語のコメントが語彙へ生で混入し、由来注記等に含まれる腐り識別子が偶然一致して stale 判定から漏れる（`currentVocabulary` 自身のコメントが「含めると `resetForShow` のような由来注記が語彙に化け、腐りが原理的に検出できない（実測 11 件）」と明記する失敗形の再演。修正ラウンド 2 の Minor 指摘） |
| 12 | `G-workspace-lints` | `REQUIRED_RUSTDOC_LINTS`（governance-check.mjs:345、lint 名 2 件） | 3 つ目の必須 rustdoc lint を deny させたくても追記し忘れると非実効のまま緑になる |
| 13 | `G-clippy-disallowed` | `REQUIRED_DISALLOWED_METHODS`（governance-check.mjs:461、禁止メソッドパス 7 件） | 8 つ目の禁止対象メソッドを追加しても追記し忘れると禁止漏れが検知されない |
| 14 | `G-clippy-disallowed` | `DISALLOWED_METHODS_GROUPS`（governance-check.mjs:521、群名 2 件。コメントに「上流が 3 つ目の群へ入れたら、この配列が更新されるまで沈黙する」と残余が明記済み） | 上流 clippy が 3 つ目の打ち消し群を持ったとき、この検査は気づかない |

**候補から外したもの（理由。除外リスト＝過剰包含の向きと、包含フィルタでも向きが安全側のものを分けて書く）**:

- 除外リスト（足し忘れの向きが過剰包含）: `WALK_EXCLUDE_NAMES` / `WALK_EXCLUDE_PATHS`
  （governance-check.mjs:38-39、全検査共通の走査除外）は除外し忘れたディレクトリのファイルが
  誤って検査対象に入る方向であり、本監査が捉える「見落とし」（過小包含）とは逆。`OUTPUT_ONLY_FLAGS`
  （G-hook-commands・governance-check.mjs:860）も同様に、追記し忘れの向きは false negative ではなく
  false positive（無関係なフラグ差分で赤くなる）。`adrCitationDocs` のテストファイル除外
  （governance-check.mjs:1757、`!f.endsWith(".test.mjs")`）も、除外し忘れるとテストのフィクスチャ
  （意図的に実在しない ADR 名を持つ）が誤って検査対象に入り赤くなる方向で同じ——**ただし同じ 1757 行の
  `.rs|.mjs` 拡張子ホワイトリスト本体は向きが逆（過小包含）であり、上の表 7 行目として候補に入れてある**
- 包含フィルタだが向きが安全側（過小包含すると検査が緩むのではなく厳しくなる）: `VOCAB_SOURCE_EXT`
  （G-stale-identifiers・governance-check.mjs:1495） は「現行語彙」の元になるソース拡張子の一覧。
  この一覧が漏れる（新しいソース言語の拡張子が無い）と、その言語由来の正当な識別子が語彙に入らず、
  文書中のその識別子が**偽陽性で stale 扱いになる**方向であり、見落としではなく過検出に倒れる。
  `EXTERNAL_CMD_LINE`（governance-check.mjs:1519、外部コマンド名の一覧）も同様に、未知のコマンド名
  を持つ行は識別子照合の対象に残る（除外されない）ため過検出方向で安全
- 包含フィルタだが外部仕様の完全列挙であり自チーム管理の成長する一覧ではない: `clippyMethodsDenied`
  の TOML キー判定（governance-check.mjs:558「dotted 形」・:567「サブテーブル形」、どちらも
  `(level|priority)` の 2 択）。この 2 キーは Cargo の lint エントリ形式が定める**現時点で完全な**
  スキーマであり（`lintLevel` / `lintPriority` も同じ 2 キーを共有・:283, :291）、この一覧が漏れて
  いるのは「このチームが足し忘れた」のではなく Cargo 側が仕様を広げたときに限られる（外部設定の
  ドリフトであり design §4「外部設定」の SSOT に属する残余）。**表記の綴り（インライン/dotted/
  サブテーブルの 3 形）は取りこぼしなく全部読む設計であることをコード自身のコメントが明記して
  おり（:527-529）、狭いのはキー名の集合ではなく綴りの網羅性の話である**ため候補には入れなかった
- 母集団を走査・外部ファイル解析・import から得ており手書きリテラルではないもの: `G-architecture-table` /
  `G-build-commands` / `G-ci-table` / `G-rules-globs` / `G-skill-table` / `G-hook-commands` /
  `G-hook-fires` / `G-check-skill-enumeration` / `G-adr-file-names` / `G-heading-refs` /
  `G-near-heading-refs`

### 9.3 スモーク（Task 2）

母集団 3 件（`$script:Invariants`、`scripts/lib/SnotraTraceInvariants.psm1:30`、
`@('H1', 'H4', 'H5')`。§3 の記載と一致）。**この一覧自身が候補である**——`Get-SnotraTraceInvariantNames`
（同ファイル:41）はこの手書き配列を返すだけの関数で、新しい不変条件を判定ロジックへ追加してもこの配列へ
追記し忘れると、記録・集計・exit code のどこにも現れない（同関数の doc comment 自身がこの経路を
警告している：「呼び出し側はこの一覧を写さない……判定を 1 つ足したときモジュール側だけが直り……
黙って落ちる」）。件数: **1 件**。

H2 / H3 が欠番であることについて: `git log --all -S "'H2'" -- scripts/lib/SnotraTraceInvariants.psm1`
・同 `-S "'H3'"` はいずれも 0 件で、モジュール新設コミット（#879）の時点で既に `H1`/`H4`/`H5` の
3 件だった。削除された痕跡は無く、採番の飛ばし（H2/H3 が実装された形跡が無い）と判断する。
