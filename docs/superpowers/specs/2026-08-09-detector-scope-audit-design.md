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

3 層合計 **25 件**（Rust 15 件・ガバナンス 9 件・スモーク 1 件）。**この時点では分類も判定も行わない**
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
件数: **9 件**。

| # | 候補 | 手書き一覧（識別子・行） | 見落としうる形 |
|---|---|---|---|
| 1 | `G-module-index` | `MODULE_INDEX_CRATES`（governance-check.mjs:92、crate 4 件） | 新 crate を追加しても追記し忘れると、そのモジュール索引は双方向照合されない |
| 2 | `G-references` | `governanceDocs()`（governance-check.mjs:1339、ルート文書 4 件 + crate CLAUDE.md 正規表現 4 crate） | 新 crate の CLAUDE.md がこの正規表現に無いと、その文書内の参照実在は照合されない |
| 3 | `G-spec-sections` | 同上（`governanceDocs()` を共有） | 同上——新 crate CLAUDE.md 内の `SPEC §N` 参照が照合対象から漏れる |
| 4 | `G-adr-citations` | 同上（`adrCitationDocs` が `docs`＝`governanceDocs()` を含む） | 同上に加え ADR 短縮引用が該当文書内で照合されない（他の入力＝ADR/skills/`.rs`・`.mjs` は走査ベースのため影響は限定的） |
| 5 | `G-area-budget` | `ALWAYS_LOADED_FILES`（governance-check.mjs:1052、`["CLAUDE.md", "AGENTS.md"]`） | 常時ロード面に 3 つ目のファイルが増えても追記し忘れると火災報知器の面積に算入されない |
| 6 | `G-stale-identifiers` | `STALE_EXTRA_DOCS`（governance-check.mjs:1505、固定パス 4 件） | 新設した「意図の SSOT」級の文書がここに無いと、腐り識別子の検査対象から漏れる |
| 7 | `G-workspace-lints` | `REQUIRED_RUSTDOC_LINTS`（governance-check.mjs:345、lint 名 2 件） | 3 つ目の必須 rustdoc lint を deny させたくても追記し忘れると非実効のまま緑になる |
| 8 | `G-clippy-disallowed` | `REQUIRED_DISALLOWED_METHODS`（governance-check.mjs:461、禁止メソッドパス 7 件） | 8 つ目の禁止対象メソッドを追加しても追記し忘れると禁止漏れが検知されない |
| 9 | `G-clippy-disallowed` | `DISALLOWED_METHODS_GROUPS`（governance-check.mjs:521、群名 2 件。コメントに「上流が 3 つ目の群へ入れたら、この配列が更新されるまで沈黙する」と残余が明記済み） | 上流 clippy が 3 つ目の打ち消し群を持ったとき、この検査は気づかない |

**候補から外したもの（理由）**: `WALK_EXCLUDE_NAMES` / `WALK_EXCLUDE_PATHS`（governance-check.mjs:38-39）
は全検査共通の走査除外リストだが、足し忘れの向きは**過剰包含**（除外し忘れたディレクトリのファイルが
誤って検査対象に入る）であり、本監査が捉える「見落とし」（過小包含）とは逆方向のため対象外とした。
`OUTPUT_ONLY_FLAGS`（G-hook-commands・governance-check.mjs:860）も同様に、追記し忘れの向きは
false negative ではなく false positive（無関係なフラグ差分で赤くなる）である。`G-architecture-table` /
`G-build-commands` / `G-ci-table` / `G-rules-globs` / `G-skill-table` / `G-hook-commands` /
`G-hook-fires` / `G-check-skill-enumeration` / `G-adr-file-names` / `G-heading-refs` /
`G-near-heading-refs` は母集団をファイルシステム走査・外部ファイル解析・import した実装から得ており、
手書きリテラルではないため対象外とした。

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
