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

3 層合計 **31 件**（Rust 16 件・ガバナンス 14 件・スモーク 1 件。Rust の 1 件増は Task 3 の
grep 2 軸検算で見つけた篩の見落とし——§9.1 の #16 と Step 1 の記録を参照）。

### 9.0 分類の内訳（Task 3）

母集団の SSOT で 4 分類（C/S/F/X、定義は §4）した結果。**一意に決まらない件は 2 分類を付す**
（§7 の留保どおり）ため、件数は「候補の行数」と「延べ分類数」の 2 通りで数える。

| 分類 | 延べ件数 | 内訳 |
|---|---:|---|
| **C**（コンパイラ） | **6** | Rust のみ（純 C 4 件 + Rust #1・#2 の C+S 二重 2 件） |
| S（ソーステキスト） | 18 | Rust 12（純 10 + 二重 2）・ガバナンス 5・スモーク 1 |
| F（ファイルシステム） | **5** | ガバナンスのみ（純 2 + ガバナンス #3/#4/#5 の F+X 二重 3） |
| X（外部設定） | 7 | ガバナンスのみ（純 4 + 二重 3） |

候補の行数は Rust 16 + ガバナンス 14 + スモーク 1 = 31、二重分類 5 件（Rust の C+S 2・ガバナンスの
F+X 3）を加えた延べ分類数は 36（6+18+5+7）。

**C はすべて Rust に閉じる**——ガバナンス・スモークの母集団はいずれも Rust の型システムの外側
（JS/PowerShell のリテラルか、TOML/外部ツールの仕様・ファイル一覧）にあるため、コンパイラが
SSOT になり得ない。**Task 6 の derive 導入判断に効く数字は C=6**（うち純 C は 4 件、残り 2 件は
Rust #1・#2 の二重分類の片側）。

**F は当初 0 件と誤って結論していた（修正ラウンド 1 で訂正）。** §4 が F の実例として挙げる
「対象文書 35 件」は `governanceDocs().length`（governance-check.mjs:1793,1854）そのものであり、
これはガバナンス候補 #3/#4/#5（`G-references`/`G-spec-sections`/`G-adr-citations` が共有する
`governanceDocs()`）の母集団と完全に一致する。`governanceDocs()` の中身を分解すると:

- **動的に glob する部分**（`docs/**/*.md`・`.claude/rules/*.md`・`.claude/skills/*/SKILL.md`。
  governance-check.mjs:1341-1347 の後半 3 条件）は母集団の SSOT が走査対象自身にあり、
  Phase 2（Task 2）の時点で「足し忘れの経路が構造的に無い」候補として正しく除外されている
- **手書きのルート文書配列**（`["CLAUDE.md", "AGENTS.md", "CONTRIBUTING.md", "SPEC.md"]`、
  governance-check.mjs:1341）は**要素そのものがリポジトリのファイルパスである**——
  「足し忘れたとき、足したことを知っている者は誰か」を問うと、その新設ファイルの実在を
  知っているのはファイルシステムであり、F が正しい。修正前は「ディレクトリ規則で導出できない
  編集方針上の選定だから S」と判定したが、これは**選定理由（なぜこの 4 件を選んだか）と
  母集団の型（要素がファイルか否か）を混同していた**——選定が編集方針でも、要素がファイル
  である以上 F 側の足し忘れリスク（新しいファイルを追加し忘れる）は現に存在する
- **crate CLAUDE.md 正規表現**（`/^(snotra-core|...)\/CLAUDE\.md$/`）は crate 名という
  Rust ファイル階層の外側にある識別子でパラメタ化されており、真の SSOT は Cargo workspace
  の member 一覧（X、§9.2 #1 と同型）

ゆえに #3/#4/#5 は **F + X**（旧: S + X）に訂正する。**同じ「配列の要素が裸のファイルパス」という
形は候補 #8（`ALWAYS_LOADED_FILES`）・#9（`STALE_EXTRA_DOCS`）にも当たる**——いずれも
`["CLAUDE.md", "AGENTS.md"]` / `["SPEC.md", "CLAUDE.md", "AGENTS.md", "snotra-settings/SETTINGS-DESIGN.md"]`
という、要素が全て実在ファイルパスの手書き配列であり、こちらも **S → F** に訂正する。

**F ではない（S のまま）候補との境界**: 拡張子フィルタ（§9.2 #2/#6/#7/#10/#11）は要素が
`"rs"` `"ts"` のような**拡張子・カテゴリ**であってファイルパスそのものではない——ファイルシステムは
「どの拡張子を実在検査の対象にすべきか」を教えてくれない（無数の拡張子を持つファイルが実在し得る）
ため、これらは編集方針（S）のまま残る。同様に `MODULE_INDEX_CRATES`（#1）はキーが crate 名という
識別子であり、値の `src:` ディレクトリパスはその識別子から機械的に導出される付随情報にすぎない
ため、母集団の本体は「crate 名の一覧」（X、Cargo.toml）のままとした。X 側 3 件
（`REQUIRED_RUSTDOC_LINTS`・`REQUIRED_DISALLOWED_METHODS`・`DISALLOWED_METHODS_GROUPS`）も
要素が lint 名・メソッドパス・group 名であってファイルパスではないため F 化の対象外。
この境界（要素がファイルパスか、ファイルを指す識別子か、ファイルとは無関係なカテゴリ／識別子か）
で全 14 件・全 16 件（Rust）・1 件（スモーク）を見直したが、上記 3 件（#3/#4/#5 の F 側・#8・#9）
以外に F 性の見落としは無かった（Rust・スモークの母集団は enum variant／文字列 alias／PowerShell
識別子であり、ファイルパスを要素に持つものは無い）。

### 9.1 Rust（Task 1 が 15 件・Task 3 の grep 検算で #16 を追加）

母集団 905 件（`cargo test --workspace -- --list | grep -c ': test$'`、§3 の記載と一致）を全数読み、
テスト本体が一覧・配列・`match` の腕を走査しているものを候補として抜いた。件数: **16 件**
（C 4 件・S 10 件・C+S 二重 2 件。延べ C6/S12）。

| # | 候補 | 分類 | 分類理由（母集団の真の SSOT） |
|---|---|---|---|
| 1 | `snotra-settings/src/app.rs::section_table_covers_all_config_fields` | C + S | `SECTION_TABLE` は異なる SSOT を持つ 2 母集団の対応表。Config フィールド側は `field_mutations()` の `..` なし destructure で網羅がコンパイル時に強制される（C）。TabId 側は `TabId::ALL`（`const ALL` の手書き配列）——`startup.rs` の doc が同型パターンを「一覧自身が母集団になる限り足し忘れを検出できない」と明記する非保護の一覧（S）。§7 の留保どおり二重分類 |
| 2 | `snotra-settings/src/app.rs::section_table_no_false_positive_when_unchanged` | C + S | 同上（`TabId::ALL` を走査する点は #1 と同じ橋渡し構造） |
| 3 | `snotra-core/src/error.rs::bin_error_source_all_variants_return_none` | S | `BinError` 5 variant を手書き `Vec` で列挙。`impl Error for BinError {}` は `source()` を上書きせず default 実装が常に `None` を返すため判定は自明——母集団はコンパイラ保護の無い手書き列挙 |
| 4 | `src-tauri/src/events.rs::event_names_are_pairwise_distinct` | S | 9 定数の手書き配列。テスト自身の doc comment が「保証は狭い……この配列へ足さなければ検査対象にならない」と明記——§2「射程を doc に書く」の既知の好例そのもの |
| 5 | `snotra-settings/src/hotkey_input.rs::every_ui_generated_key_is_in_the_core_accepted_set` | S | `egui::Key::ALL`（外部 crate の配列）を `egui_key_to_config_name` でフィルタするが同関数の match は `_ => None` を持つ非網羅——新しい `egui::Key` variant の追加をコンパイラは検出しない |
| 6 | `src-tauri/src/startup.rs::count_matches_the_enum_declaration` | S | `include_str!` で自ファイルのソーステキストを走査し `enum Phase` 宣言の variant 数を数える——§2「母集団をソーステキストへ移す」の既知の好例そのもの |
| 7 | `src-tauri/src/startup.rs::every_phase_key_is_present_even_when_skipped` | C | `Phase::all()` を走査。`key()` は網羅 match（ワイルドカード無し）ゆえ variant を足すとコンパイルが通らない |
| 8 | `src-tauri/src/startup.rs::failure_reasons_are_stable_and_unique` | S | `StartupFailure` 7 variant を手書き `let all = [...]` で列挙。テスト自身の doc comment が「Phase と同じ弱さを持つ——variant を足してここへ書き足さなくても落ちない」「網羅の証明ではない」と明記 |
| 9 | `src-tauri/src/startup.rs::index_and_from_index_are_inverse_over_the_whole_enum` | C | テスト自身の doc comment が「足し忘れを捕まえるのはコンパイラである。`index()` が網羅 match ゆえ variant を足すとコンパイルが通らない」と明記 |
| 10 | `src-tauri/src/startup.rs::keys_are_unique` | C | `Phase::all()` を走査し、網羅 match の `key()` が返す値の一意性を検査（#7 と同型） |
| 11 | `src-tauri/src/startup.rs::out_of_range_index_is_dropped_instead_of_panicking` | C | `Phase::all()` を走査（#7/#9/#10 と同型）。COUNT 上げ忘れの再現は配列側を縮めて行うが、走査母集団自体は変わらない |
| 12 | `snotra-core/src/hotkey.rs::modifier_aliases_order_duplicates_and_empty_segments_form_one_set` | S | modifier alias（`"win" \| "super" \| "meta"` 等）は文字列パターンの手書き match。ワイルドカード `_ =>` を持ち非網羅——新 alias の追加漏れをコンパイラは検出しない |
| 13 | `snotra-core/src/hotkey.rs::key_aliases_share_one_semantic_key` | S | key alias も同様に文字列パターンの手書き match（#12 と同型） |
| 14 | `snotra-core/src/hotkey.rs::supported_key_set_parses_case_insensitively` | S | `HotkeyKey` 全 62 variant 中 12 件の代表サンプルによる spot check。母集団は `parse()` 内の非網羅文字列 match |
| 15 | `src-tauri/src/platform/hotkey.rs::prepared_named_key_aliases_use_the_same_typed_mapping` | S | `key_vk()` 自体は `HotkeyKey` の全 variant を網羅 match で変換する（C）が、このテストが直接走査するのは alias 文字列のペア（`["Delete", "del"]`）——母集団は snotra-core 側と同じ非網羅 match |
| 16 | `snotra-core/src/hotkey.rs::system_shortcuts_are_checked_after_semantic_normalization` | S | Task 3 Step 1 の grep 2 軸検算で発見（篩の見落とし）。`is_system_shortcut()` が判定する Windows 予約ショートカットの手書き `blocked` 配列（7 組）を走査——母集団は文字列ベースのハードコード一覧で、コンパイラ保護は無い |

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
どちらかへ処分済み**（内訳は表の直後の「候補から外したもの」を参照）。件数: **14 件**
（S 5 件・F 2 件・X 4 件・F+X 二重 3 件。延べ S5/F5/X7。分類は Task 3、修正ラウンド 1 で
#3/#4/#5・#8・#9 を S→F〔一部 F+X〕に訂正済み・判定基準は §9.0）。

| # | 候補 | 手書き一覧（識別子・行） | 見落としうる形 | 分類 | 分類理由（母集団の真の SSOT） |
|---|---|---|---|---|---|
| 1 | `G-module-index` | `MODULE_INDEX_CRATES`（governance-check.mjs:92、crate 4 件） | 新 crate を追加しても追記し忘れると、そのモジュール索引は双方向照合されない | X | crate 一覧の真の SSOT はルート `Cargo.toml` の `[workspace] members`（TOML 設定）。governance-check.mjs 自身の手書きではなく外部設定の写しである |
| 2 | `G-module-index` | 順方向照合の拡張子フィルタ（governance-check.mjs:116、無名の正規表現 `rs\|ts\|tsx\|html`） | 「モジュール構成」節に `` `foo.mjs` `` のようなこの 4 拡張子以外のバッククォート参照があっても実在照合されない（`MODULE_INDEX_CRATES` とは独立した 2 本目のハードコード拡張子一覧） | S | どの拡張子を実在照合の対象にするかは本プロジェクト独自の編集方針であり、参照すべき外部の権威的仕様は無い |
| 3 | `G-references` | `governanceDocs()`（governance-check.mjs:1339、ルート文書 4 件 + crate CLAUDE.md 正規表現 4 crate） | 新 crate の CLAUDE.md がこの正規表現に無いと、その文書内の参照実在は照合されない | F + X | **修正ラウンド 1 で S→F に訂正**（§9.0 参照）。ルート文書 4 件のリストは要素そのものがリポジトリのファイルパス——足し忘れを知るのはファイルシステム（F）。crate CLAUDE.md 正規表現は `MODULE_INDEX_CRATES`（#1）と同じ crate 名一覧を独立に持つ「2 本目」（Task 2 が既に指摘）——真の SSOT は Cargo.toml（X）。二重分類 |
| 4 | `G-spec-sections` | 同上（`governanceDocs()` を共有） | 同上——新 crate CLAUDE.md 内の `SPEC §N` 参照が照合対象から漏れる | F + X | #3 と同一関数を共有するため同じ橋渡し構造（root docs=F、crate 名=X。修正ラウンド 1 で訂正） |
| 5 | `G-adr-citations` | 同上（`adrCitationDocs` が `docs`＝`governanceDocs()` を含む） | 同上に加え ADR 短縮引用が該当文書内で照合されない（他の入力＝ADR/skills/`.rs`・`.mjs` は走査ベースのため影響は限定的） | F + X | #3/#4 と同じ `governanceDocs()` を内包（他の入力は動的走査ゆえ候補から除外済み・下記参照。修正ラウンド 1 で訂正） |
| 6 | `G-references` | `REF_EXTENSIONS`（governance-check.mjs:30、拡張子 11 種） | バッククォート内パス様参照の実在照合は、拡張子がこの一覧に無いファイル種別（`/` を含んでいても）を静かにスキップする（修正ラウンド 1 の指摘） | S | 「実在検査の対象と見なすソース系拡張子」は編集方針であり、外部仕様の写しではない |
| 7 | `G-adr-citations` | `adrCitationDocs` の `.rs\|.mjs` 拡張子ホワイトリスト（governance-check.mjs:1757、`/\.(rs\|mjs)$/`） | `.ts` / `.tsx` / `.ps1` 等の非 docs ソースに ADR の短縮引用があっても実在照合を素通りする（修正ラウンド 2 の指摘。同じ行の `!f.endsWith(".test.mjs")` だけを分析し本体の向き判定を書き漏らしていた） | S | 同上（#6 と同型の編集方針） |
| 8 | `G-area-budget` | `ALWAYS_LOADED_FILES`（governance-check.mjs:1052、`["CLAUDE.md", "AGENTS.md"]`） | 常時ロード面に 3 つ目のファイルが増えても追記し忘れると火災報知器の面積に算入されない | F | **修正ラウンド 1 で S→F に訂正**（§9.0 参照）。要素は `CLAUDE.md`/`AGENTS.md` という実在ファイルパスそのもの——「なぜこの 2 件を常時ロード扱いにするか」は編集方針（ハーネスの挙動という外部事実）だが、「足し忘れた 3 つ目のファイルの実在」を知るのはファイルシステムであり、母集団の型は F |
| 9 | `G-stale-identifiers` | `STALE_EXTRA_DOCS`（governance-check.mjs:1505、固定パス 4 件） | 新設した「意図の SSOT」級の文書がここに無いと、腐り識別子の検査対象から漏れる | F | **修正ラウンド 1 で S→F に訂正**（§9.0 参照）。#8 と同型——要素は全て実在ファイルパス。コメントの「静的リテラルであること自体が fail-closed」は選定理由の説明であり、母集団の型（F）を否定しない |
| 10 | `G-stale-identifiers` | `VOCAB_TEST_FILE`（governance-check.mjs:1499、`.test.(mjs\|ts\|tsx)` の拡張子 3 種） | この形以外のテスト専用ファイル（Rust の `#[cfg(test)] mod` 等・コメントで残余と明記済み）の語彙が「現行語彙」へ紛れ込み、実在しない識別子が偶然そのテスト専用語彙と一致すると stale 判定から漏れる | S | 対象拡張子の選定は編集方針（#2/#6/#7 と同型） |
| 11 | `G-stale-identifiers` | `currentVocabulary` のコメント除去振り分け（governance-check.mjs:1556、`/\.(ps1\|toml\|yml)$/` の可否で `#` 除去 or `stripRustComments` を選ぶ） | `VOCAB_SOURCE_EXT`（:1495）へ `#` コメント言語の拡張子を追加してもここへ追記し忘れると、その言語のコメントが語彙へ生で混入し、由来注記等に含まれる腐り識別子が偶然一致して stale 判定から漏れる（`currentVocabulary` 自身のコメントが「含めると `resetForShow` のような由来注記が語彙に化け、腐りが原理的に検出できない（実測 11 件）」と明記する失敗形の再演。修正ラウンド 2 の Minor 指摘） | S | 同上（拡張子選定は編集方針） |
| 12 | `G-workspace-lints` | `REQUIRED_RUSTDOC_LINTS`（governance-check.mjs:345、lint 名 2 件） | 3 つ目の必須 rustdoc lint を deny させたくても追記し忘れると非実効のまま緑になる | X | ルート `Cargo.toml` の `[workspace.lints.rust]` に対する要求項目のカナリア——真の母集団は TOML 設定の側にある |
| 13 | `G-clippy-disallowed` | `REQUIRED_DISALLOWED_METHODS`（governance-check.mjs:461、禁止メソッドパス 7 件） | 8 つ目の禁止対象メソッドを追加しても追記し忘れると禁止漏れが検知されない | X | doc comment 自身が「含めなかったメソッドと、その除外理由の正本は `src-tauri/clippy.toml` 冒頭のコメントである」と明記——真の母集団は外部 TOML 設定 |
| 14 | `G-clippy-disallowed` | `DISALLOWED_METHODS_GROUPS`（governance-check.mjs:521、群名 2 件。コメントに「上流が 3 つ目の群へ入れたら、この配列が更新されるまで沈黙する」と残余が明記済み） | 上流 clippy が 3 つ目の打ち消し群を持ったとき、この検査は気づかない | X | 群一覧の真の SSOT は upstream clippy 自身の lint-group taxonomy（`clippy-driver -W help`）——本リポジトリの外にある仕様 |

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
黙って落ちる」）。件数: **1 件**。**分類: S**（PowerShell の手書き文字列配列。コンパイラ・ファイル
システム・外部設定のいずれも SSOT になり得ず、この関数自身が唯一の記録）。

H2 / H3 が欠番であることについて: `git log --all -S "'H2'" -- scripts/lib/SnotraTraceInvariants.psm1`
・同 `-S "'H3'"` はいずれも 0 件で、モジュール新設コミット（#879）の時点で既に `H1`/`H4`/`H5` の
3 件だった。削除された痕跡は無く、採番の飛ばし（H2/H3 が実装された形跡が無い）と判断する。

## 10. 仕分け表（Task 4・定型変異の実測）

§9 の候補 31 件・延べ 36 分類それぞれに §4 の定型変異を当て、実際に落ちるかを測った。
**読みでは判定していない**——#1000 の 3 件がいずれも読みで見抜けなかったため、全件を変異で測った。

### 10.0 判定の定義（3 分岐と「素の結果／最終判定」を分ける理由）

| 結果 | 意味 |
|---|---|
| **①** | コンパイラが守っている（緑のビルドに足し忘れが残らない） |
| **②** | その検査が落ちる |
| **③** | どちらも通る＝**射程不一致** |

**素の変異の結果と最終判定を分けて記録する。** 定型変異を当てるとコンパイルが落ちることが多いが、
落ちた場所が**監査対象の一覧とは別の一覧**（`Display` の網羅 match・UI dispatch の match 等）である
場合、開発者はそこへ腕を 1 本足すだけで緑に戻り、**監査対象の一覧の足し忘れはそのまま残る**。
そこで各件について、

1. **素の変異の結果** — 変異をそのまま当てたときに何が起きたか（コンパイルエラーの箇所を明記）
2. **調整** — 監査対象の一覧には足さずに、コンパイルだけを通すための最小修正
3. **調整後** — その状態で監査対象の検査が落ちるか
4. **最終判定** — **緑のビルドに当該一覧の足し忘れが残るなら ③**、残り得ないなら ①／②

の 4 つを記録する。**①と数えるのは、コンパイラが止めた対象が監査対象の一覧そのものだったとき**
（または他の検査が同じ足し忘れを必ず捕まえるとき）に限る。`dead_code` / `unused` による停止は
①ではない——本ワークスペースは `[workspace.lints]` に `unused` の deny を持たず（実測: `cargo test`
で `warning: constant AUDIT_PROBE is never used` と出て**コンパイルは通った**）、`-D warnings` が
効くのは clippy 経路だけなので、`cargo test` を測定コマンドにする限りこの罠は踏まない。

### 10.1 対照実験（既知 2 件・レシピの検算）

| 対照 | 変異 | コマンド | 期待 | 実測 |
|---|---|---|---|---|
| `events.rs::event_names_are_pairwise_distinct` | 末尾へ `pub(crate) const AUDIT_PROBE: &str = "audit-probe";` | `cargo test -p snotra event_names` | PASS（③） | **PASS**（`dead_code` は warning 止まりでコンパイルは通る）✅ |
| `startup.rs::count_matches_the_enum_declaration` | `enum Phase` へ `AuditProbe,` | `cargo test -p snotra count_matches_the_enum_declaration` | FAIL（②）・ただしコンパイルが先に落ちうる | **素: E0004（`key()`/`index()` の 2 箇所）→ 腕を 2 本足して調整 → FAILED（left: 10, right: 9）**✅ |

**両方とも期待どおり。** レシピは妥当であると確かめてから 10.2 以降へ入った。

### 10.2 Rust 16 件（延べ 18 分類：C 6・S 12）

測定コマンドはいずれも `cargo test -p <crate> <フィルタ>`（workspace 全体は走らせない）。

| # | 候補 | 分類 | 変異 | 素の結果 | 調整 | 調整後 | 最終 |
|---|---|---|---|---|---|---|---|
| 1 | `snotra-settings::section_table_covers_all_config_fields` | C | `Config` へ `pub audit_probe: bool` を追加 | ①相当 E0063（`Default for Config`）→調整後 **E0027**（`field_mutations()` の `..` なし destructure＝当該ガード） | `Default` へ `audit_probe: false`／destructure へ `audit_probe: _` | **PASS** | **③** |
| 1 | 同上 | S | `TabId` へ `AuditProbe,`（`TabId::ALL` へは足さない） | ①相当 E0004（`label()` :79 と UI dispatch :589＝**別一覧**） | 両 match へ腕 1 本ずつ | **PASS** | **③** |
| 2 | `snotra-settings::section_table_no_false_positive_when_unchanged` | C | 同 #1 C | 同上（同一 crate がコンパイル不能） | 同上 | **PASS** | **③** |
| 2 | 同上 | S | 同 #1 S | 同上 | 同上 | **PASS** | **③** |
| 3 | `snotra-core::bin_error_source_all_variants_return_none` | S | `BinError` へ `AuditProbe,` | ①相当 E0004（`impl Display` ＝**別一覧**） | `Display` へ腕 1 本 | **PASS** | **③** |
| 4 | `src-tauri::event_names_are_pairwise_distinct` | S | `AUDIT_PROBE` 定数を追加（対照 1） | **PASS** | 不要 | — | **③**（doc が明記済み） |
| 5 | `snotra-settings::every_ui_generated_key_is_in_the_core_accepted_set` | S | ①`egui::Key::ALL` の未マップ variant（`Key::Backspace`）が現に存在したまま素で緑／②逆向きに `Key::Backspace => Some("Backspace")` を追加 | ①向き **PASS**（射程外）／②向き **FAILED**（`UI key mapping set changed`） | 不要 | — | **③**（監査対象の向き＝上流 variant 追加は素通り。手書き `expected` 側の足し忘れは②で守られる） |
| 6 | `src-tauri::count_matches_the_enum_declaration` | S | `Phase` へ `AuditProbe,`（対照 2） | ①相当 E0004（`key()`/`index()`） | 両 match へ腕 | **FAILED** | **②** |
| 7 | `src-tauri::every_phase_key_is_present_even_when_skipped` | C | 同上 | **E0004**（`index()` は doc が名指しする当該ガード） | 腕 2 本 | PASS（ただし #6 が FAILED） | **①** |
| 8 | `src-tauri::failure_reasons_are_stable_and_unique` | S | `StartupFailure` へ `AuditProbe,` | ①相当 E0004（`reason()` ＝**別一覧**。`todo!()` でも通る旨をテストの doc 自身が明記） | `reason()` へ腕 1 本 | **PASS** | **③**（doc が明記済み） |
| 9 | `src-tauri::index_and_from_index_are_inverse_over_the_whole_enum` | C | 同 #7 | **E0004**（`index()`＝doc が名指しする当該ガード） | 腕 2 本 | PASS（#6 が FAILED） | **①** |
| 10 | `src-tauri::keys_are_unique` | C | 同 #7 | **E0004** | 腕 2 本 | PASS（#6 が FAILED） | **①** |
| 11 | `src-tauri::out_of_range_index_is_dropped_instead_of_panicking` | C | 同 #7 | **E0004** | 腕 2 本 | PASS（#6 が FAILED） | **①** |
| 12 | `snotra-core::modifier_aliases_order_duplicates_and_empty_segments_form_one_set` | S | modifier match へ `\| "cmd"` を追加（テストの `["Win","Super","Meta"]` には足さない） | **PASS**（`hotkey::tests` 10 本すべて緑） | 不要 | — | **③** |
| 13 | `snotra-core::key_aliases_share_one_semantic_key` | S | key match へ `\| "ret"`（Enter の 3 つ目の alias） | **PASS**（同 10 本すべて緑） | 不要 | — | **③** |
| 14 | `snotra-core::supported_key_set_parses_case_insensitively` | S | `HotkeyKey` へ `AuditProbe,` ＋ parse 腕 `"auditprobe"` | **PASS**（snotra-core は素で通る・10 本すべて緑） | 不要 | — | **③**（下流 `src-tauri::key_vk()` は E0004 で止まるので**新 variant** は緑で出荷できないが、この検査自身は 62 中 12 の抜き取りであり、既存 variant への **alias 追加**は #13 の実測どおり誰も止めない） |
| 15 | `src-tauri::prepared_named_key_aliases_use_the_same_typed_mapping` | S | snotra-core の Delete alias を `"delete" \| "del" \| "rm"` へ | **PASS** | 不要 | — | **③** |
| 16 | `snotra-core::system_shortcuts_are_checked_after_semantic_normalization` | S | `is_system_shortcut()` へ `\|\| (alt_only && key == Home)` を追加（テストの `blocked` 7 組には足さない） | **PASS**（同 10 本すべて緑） | 不要 | — | **③** |

**Rust 層の集計: ① 4 件（#7/#9/#10/#11＝いずれも `Phase` の C 分類）・② 1 件（#6）・③ 13 件。**

**#1/#2 の C 分類について特記する。** `field_mutations()` の doc は「mutation と `SECTION_TABLE` の
両方に対応を追加するまで検出が続く」と書いているが、**実測ではそうならない**——destructure へ
`audit_probe: _,` の 1 行を足すだけでコンパイルが通り、`SECTION_TABLE` が新セクションを持たないまま
2 本のテストが緑になる（実測: `2 passed`）。`..` なし destructure が強制するのは「フィールドの存在を
1 度は目にすること」であって、対応表への追記ではない。

**`Phase`（#7/#9/#10/#11）だけが①なのは #6 が在るからである。** 腕を足して調整した状態で
`cargo test -p snotra startup::tests` を走らせると **19 passed / 1 failed** で、落ちるのは #6 だけ。
C 分類の 4 本は 1 本も落ちない——**コンパイラと #6 の二重の網が `Phase` を守っており、C 分類の
テスト自身は足し忘れを見ていない**。

### 10.3 ガバナンス 14 件（延べ 17 分類：S 5・F 5・X 7）

測定コマンドはすべて `npm run governance:check`（exit code と「N 件の不整合」で判定）。
この層に①は原理的に現れない——母集団は JS のリテラル・ファイル一覧・TOML であり、
Rust のコンパイラは関与しない（§9.0 の「C はすべて Rust に閉じる」と整合）。

**各変異には対照（同じ payload を射程内へ置いたら赤くなること）を測ってある。** 変異が
「検査が見ない場所に落ちた」のか「検査が見たのに素通りした」のかを、緑だけでは区別できないためである。

| # | 検査 / 一覧 | 分類 | 変異 | 対照（射程内へ同じ payload） | 結果 |
|---|---|---|---|---|---|
| 1 | `G-module-index` / `MODULE_INDEX_CRATES` | X | `snotra-probe` crate を新設し `Cargo.toml` の `members` へ追加（`MODULE_INDEX_CRATES` へは足さない）。その `CLAUDE.md` の「モジュール構成」に実在しない `` `no_such_probe_file.rs` `` を書き、`src/lib.rs` は索引に載せない | — | **③**（`workspace member 5 件` と数えられながら索引は照合されない） |
| 2 | `G-module-index` / 順方向の拡張子正規表現 | S | `snotra-core/CLAUDE.md` の「モジュール構成」節へ `` `no_such_probe.mjs` `` を追加 | 同じ位置を `` `no_such_probe.rs` `` にすると**赤**（`索引に記載の … に対応する実ファイルが無い`） | **③** |
| 3 | `G-references` / `governanceDocs()` | F | ルート直下に `PROBE-AUDIT.md` を新設し、実在しない `` `docs/no-such-probe-doc.md` `` を書く | `docs/architecture.md` へ同じ 1 行を置くと**赤** | **③** |
| 3 | 同上 | X | 新 crate `snotra-probe/CLAUDE.md` に同じ参照を書く（crate 名の正規表現に無い） | 同上 | **③** |
| 4 | `G-spec-sections` / `governanceDocs()` | F | `PROBE-AUDIT.md` に `SPEC §99.9` | `docs/architecture.md` では**赤** | **③** |
| 4 | 同上 | X | `snotra-probe/CLAUDE.md` に `SPEC §99.9` | 同上 | **③** |
| 5 | `G-adr-citations` / `governanceDocs()` | F | `PROBE-AUDIT.md` に `ADR-no-such-probe-adr` | `docs/architecture.md` では**赤** | **③** |
| 5 | 同上 | X | `snotra-probe/CLAUDE.md` に同じ引用 | 同上 | **③** |
| 6 | `G-references` / `REF_EXTENSIONS` | S | `docs/architecture.md` へ実在しない `` `scripts/lib/NoSuchProbe.psm1` ``（`.psm1` は `REF_EXTENSIONS` の `ps1` に当たらない） | 同じ行の `.md` 版は**赤** | **③** |
| 7 | `G-adr-citations` / `.rs\|.mjs` ホワイトリスト | S | `vitest.config.ts` へ `// probe: ADR-no-such-probe-adr` | 同じ 1 行を `scripts/race-boundaries.mjs` へ置くと**赤** | **③** |
| 8 | `G-area-budget` / `ALWAYS_LOADED_FILES` | F | 5000 字の `PROBE-ALWAYS.md` を新設し `CLAUDE.md` へ `@PROBE-ALWAYS.md` を足す | — | **③**（常時ロード面は 14421 → **14438 字**しか動かない＝`CLAUDE.md` 側の 1 行分だけ。5000 字は算入されない） |
| 9 | `G-stale-identifiers` / `STALE_EXTRA_DOCS` | F | ルート `PROBE-AUDIT.md` に `` `noSuchProbeIdentifier()` `` 他 2 形 | `docs/architecture.md` では**赤** | **③**（照合件数 286 / 33 文書が動かない） |
| 10 | `G-stale-identifiers` / `VOCAB_TEST_FILE` | S | `docs/architecture.md` へ `` `probe_test_only_ident` `` を書き、その識別子を Rust の `#[cfg(test)] mod` の中にだけ定義する | **A/B で測った**: 定義前は**赤**、`#[cfg(test)]` へ足すと**緑** | **③**（`VOCAB_TEST_FILE` は `.test.(mjs\|ts\|tsx)` しか見ないので Rust のテスト語彙が現行語彙へ入る） |
| 11 | `G-stale-identifiers` / `currentVocabulary` のコメント除去振り分け | S | `.psm1` に `# probeCommentIdent` を書き、`VOCAB_SOURCE_EXT` へ `psm1` を足す（:1556 の `/\.(ps1\|toml\|yml)$/` へは足さない） | **A/B で測った**: `VOCAB_SOURCE_EXT` へ足す前は**赤**、足すと**緑** | **③**（`#` コメントが `stripRustComments` を素通りして語彙に化ける） |
| 12 | `G-workspace-lints` / `REQUIRED_RUSTDOC_LINTS` | X | `Cargo.toml` の `[workspace.lints.rustdoc]` へ 3 つ目の `private_intra_doc_links = "deny"` | `invalid_html_tags` を別名へ書き換えると**赤** | **③** |
| 13 | `G-clippy-disallowed` / `REQUIRED_DISALLOWED_METHODS` | X | `src-tauri/clippy.toml` へ 8 つ目の禁止パスを追加 | `all_styles_mut` を別名へ書き換えると**赤** | **③**（`clippy 禁止 8 件` と数えられるが、8 件目は固定されない） |
| 14 | `G-clippy-disallowed` / `DISALLOWED_METHODS_GROUPS` | X | `[workspace.lints.clippy]` へ 3 つ目の群 `suspicious = "allow"` | 同じ位置を `style = "allow"` にすると**赤** | **③** |

**ガバナンス層の集計: ① 0 件・② 0 件・③ 17 件。**

**この層で②が 1 件も出ないのは、候補の抜き方が正しかったことの裏返しである。** Task 2 は
「母集団を走査・外部解析から動的に取るもの」を候補から外しており、残った 14 件は定義上すべて
手書きリテラルを母集団にしている——手書きの一覧は、その一覧の外で起きた追加を原理的に見られない。
