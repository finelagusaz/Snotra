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
| ガバナンス | `grep -c 'id: "G-' scripts/governance-check.mjs` | **19** |
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

**上表のガバナンスのコマンドは、Task 8 の検算で引用符を実測へ合わせた**（当初の
`grep -c "id: 'G-"` は単一引用符を探しており、`main` の時点から **0 件**を返す——件数 19 の側は
`npm run governance:check` の「検査 19 件」と一致しており正しい）。**数え上げの根拠に置いたコマンドが
対象を含まないまま自明に答えを返す形**であり、`AGENTS.md`「検証コマンドは『観測形が対象を含むか』まで
測る」に当たる。本監査が他所で数えて回った型が、本書自身の §3 に在った。

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
変異の形も倒し先も決まる** — 候補ごとに変異を設計する必要が無くなる（**変異の設計コストが件数に
比例して膨らむ**という費用リスクは、これで消える）。

**検算**: 構文パターン起点の grep（`let all = [` / `: [T; N] = [` / `.iter().map(` 等・粗く 103 件）と
全称文言起点の grep（「網羅」「すべての」「全 variant」・粗く 39 件）を走らせ、篩が拾えていたかを見る。
**grep は母集団の決定には使わず、篩の見落としの検算にだけ使う。** 差分が出たら篩の基準の側を直す。

**実測（Task 3）**: 件数は 103 / 39 で上の見積もりと同数。差分は 1 件で、篩の側を直した
（§9.1 #16）。**この 2 軸は `--include=*.rs` ゆえ Rust 層にしか掛かっていない**——ガバナンス・
スモークの篩の検算は Task 1 / Task 2 の手読みだけが担う。

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

**derive の結果**: C は延べ 6 分類ですべて Rust に閉じた（§9.0）。うち 4 分類は `Phase` で①（コンパイラと
#6 が現に守っている・§10.2）、③ に落ちたのは `SECTION_TABLE` の #1 / #2 の C 側 2 分類だけである。
**derive は導入していない**（`Cargo.toml` / `Cargo.lock` は本ブランチで無変更・実測）——機構へ倒した
唯一の 1 件は PowerShell 側（§11.2）で derive と無関係であり、残る #1 / #2 は doc へ倒した（§11.3）。
**別 issue も切っていない**: §11.3 が残したフォローアップの到達点は `i18n.rs` の形（網羅 `match` ＋
`wildcard_enum_match_arm` の deny・§10.6.3）であって derive ではないため、切る対象が無い。

## 5. 成果物

- **本設計書** — 母集団の取り方（§3）・分類表（§4）・仕分け結果（**候補一覧は §9、定型変異の実測は
  §10、処置は §11、受け入れの検算は §12**）
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

**3 件の結末**（予定形と実結果を突き合わせる・Task 8）:

| リスク | 実際に起きたか | 結末 |
|---|---|---|
| 篩の見落とし | **起きた**（1 件） | grep 2 軸の差分が Rust #16 を出し、候補が 30 → 31 になった（§9.1・§4 Phase 2 の「実測」）。**ただし検算は `*.rs` にしか掛かっていない**——ガバナンス・スモークの見落としは受容する残余 |
| 分類が一意に決まらない件 | **起きた**（5 件） | 二重分類として数えた（Rust の C+S 2・ガバナンスの F+X 3）。上の定めどおり**分類ごとに変異を 1 つずつ当て**、候補 31 件・延べ 36 分類の 2 通りで数えている（§9.0・§10） |
| derive 導入が載りきらない | **起きなかった** | C は 6 分類すべて Rust で、③ に落ちたのは 2 分類のみ。derive を要する倒し先が現れず、導入も別 issue 化もしていない（根拠は §4 Phase 4 の「結果」） |

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

母集団 19 件（`grep -n 'id: "G-' scripts/governance-check.mjs`、§3 の記載と一致。引用符は Task 8 の
検算で実測へ合わせた——§3.1 末尾）の実装を読み、
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
| 12 | `G-workspace-lints` | `REQUIRED_RUSTDOC_LINTS`（governance-check.mjs:345、lint 名 2 件） | 3 つ目の rustdoc lint を deny で足しても追記し忘れると、**その行が後日まるごと消えても誰も気づかない**（**足した lint が非実効になるのではない**——cargo は適用するし、在るあいだは `rustdocLintsAreDenied` の「全エントリが deny/forbid」の側で見られている。固定されないのは「在り続けること」・修正ラウンド 2 で訂正） | X | ルート `Cargo.toml` の `[workspace.lints.rust]` に対する要求項目のカナリア——真の母集団は TOML 設定の側にある |
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
**`<フィルタ>` は候補列のテスト名そのもの**（`::` の左は crate。例: #3 なら
`cargo test -p snotra-core bin_error_source_all_variants_return_none`）——再現時にコマンドが
一意に決まるようにこう決めてある。

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
| 1 | `G-module-index` / `MODULE_INDEX_CRATES` | X | `snotra-probe` crate を新設し `Cargo.toml` の `members` へ追加（`MODULE_INDEX_CRATES` へは足さない）。その `CLAUDE.md` の「モジュール構成」に実在しない `` `no_such_probe_file.rs` `` を書き、`src/lib.rs` は索引に載せない | #2 の対照が同じ役目を果たす——**同じ payload（実在しない `.rs` のバッククォート参照）が、`MODULE_INDEX_CRATES` に載っている crate の「モジュール構成」節では赤くなる**。ゆえにこの緑は「検査が見ない crate だった」ことを意味する | **③**（`workspace member 5 件` と数えられながら索引は照合されない） |
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

**脚注（修正ラウンド 2 で追記）— #1・#3〜#5 の X 側・#13 について。** 上の ③ は
「`npm run governance:check` 単体では照合されない」という測定どおりの判定であり、変えない。
**ただしこの層の外に相方が居る件が 2 つある**（`governance-check.test.mjs` の実リポジトリ
カナリアを全数走査して確かめた——鍵は `new URL("..", import.meta.url)`。ヒット 5 件のうち
一覧の同期を固定するのは次の 2 件だけで、残り 3 件は「実リポで緑」「サマリ件数」の確認である）:

- **#1 と #3〜#5 の X 側** — 母集団カナリア（#701）が実 `Cargo.toml` を読み、`CLAUDE.md` を持つ
  member が `MODULE_INDEX_CRATES` と `governanceDocs()` の**両方**に載ることを `npm test` で強制する。
  上の変異（`snotra-probe` を members へ足し `CLAUDE.md` を置く）は **`npm test` なら赤くなったはず**である。
  残る穴は `CLAUDE.md` を持たない crate と、`governanceDocs()` の**ルート文書配列**の側だけ
- **#13** — G-clippy-disallowed のカナリアが実 `src-tauri/clippy.toml` を読み、
  `disallowed-methods` が `REQUIRED_DISALLOWED_METHODS` と**同じ長さで全要素を含む**ことを assert する。
  ゆえに「8 件目を clippy.toml へ足して定数へ足し忘れる」形は `npm test` が赤くする。
  **既存 doc の記述は真のまま**である——`src-tauri/clippy.toml` 冒頭と `docs/build-commands.md` が
  書くのは「**足したパスが解決すること**は見ない」「8 件目の**書き損じ**は射程外」であり、
  カナリアが固定するのは 2 つの一覧の同期であって、パスが clippy に解決されるかではない

**#12・#14 は該当しない**（`REQUIRED_RUSTDOC_LINTS` を実 `Cargo.toml` と突き合わせるカナリアは無く、
G-workspace-lints のカナリアが見るのは members の導出と「実リポで緑」だけである）。
**vitest 層が §3 の母集団に入っていないため、本表の測定はそこを一度も観測していない**（§12.5）。

**この層で②が 1 件も出ないのは、候補の抜き方が正しかったことの裏返しである。** Task 2 は
「母集団を走査・外部解析から動的に取るもの」を候補から外しており、残った 14 件は定義上すべて
手書きリテラルを母集団にしている——手書きの一覧は、その一覧の外で起きた追加を原理的に見られない。

### 10.4 スモーク 1 件（S）

`scripts/lib/SnotraTraceInvariants.psm1:30` の `$script:Invariants = @('H1', 'H4', 'H5')`。

**変異**: 同ファイルの判定本体へ H6（`rows > 1000` で violation を積む）を 1 つ足し、
`$script:Invariants` へは足さない。

**測定 1（Pester）**: `Invoke-Pester -Path scripts/lib/SnotraTraceInvariants.Tests.ps1`
（Pester 6.0.1・`target/pester/` のキャッシュを直接読み、統合テストの exe を要らなくした）。
素で **41 passed / 0 failed**、変異後も **41 passed / 0 failed** → **③**。

**測定 2（結果の帰結）**: `rows = 2000` の `egui_results:show` を 1 件だけ食わせると、

```
Violations       : H6          ← 違反は現に作られている
Overall keys     : H1,H4,H5    ← そこに H6 は無い
FailedInvariants : []
FailureCount     : 0           ← exit code は 0
```

**違反が確定していながら exit code が 0 になる**——`Get-SnotraTraceInvariantNames` の doc comment が
警告する「黙って落ちる」経路が、実際に起きることを測った。この 1 件は本監査で唯一、③ の帰結
（何が壊れるか）まで実測できた候補である。

**スモーク層の集計: ① 0 件・② 0 件・③ 1 件。**

### 10.5 集計と ③ の一覧

| 層 | 分類数 | ① | ② | ③ | 測定不能 |
|---|---:|---:|---:|---:|---:|
| Rust | 18 | 4 | 1 | 13 | 0 |
| ガバナンス | 17 | 0 | 0 | 17 | 0 |
| スモーク | 1 | 0 | 0 | 1 | 0 |
| **計** | **36** | **4** | **1** | **31** | **0** |

**③（射程不一致）は 31 分類・候補 26 件。** 内訳:

- **Rust 13 分類 / 11 件**: #1（C・S の両方）・#2（C・S の両方）・#3・#4・#5・#8・#12・#13・#14・#15・#16
- **ガバナンス 17 分類 / 14 件**: 候補すべて
- **スモーク 1 分類 / 1 件**

**②は 1 件だけである**（`startup.rs::count_matches_the_enum_declaration`）——§2 が
「母集団をソーステキストへ移す」姿として挙げた既知の 1 件、まさにそれである。①の 4 件も
`Phase` に閉じており、その 4 本を守っているのは自分自身ではなく**コンパイラと #6** である。
**つまり本リポジトリで「一覧の足し忘れ」を自力で捕まえている検査は、現時点で #6 の 1 本しかない。**

**測定不能は 0 件**。ただし 1 件だけ、変異の当て方を替えて測った候補がある——Rust #5 は
`egui::Key::ALL`（外部 crate の const）へ variant を足せないため、**`Key::ALL` に現に居て
マッピングされていない variant（`Key::Backspace`）が在るまま素の検査が緑である**ことを
もって同じ命題を測った（その `Backspace` へマッピングを足すと検査は赤くなる＝スロットは生きている）。

### 10.6 「①」を額面どおり受け取ってはならない件（Task 5/6 への申し送り）

**①と数えた 4 件以外にも、素の変異ではコンパイルが落ちた分類が 6 つある**（#1 の C・S、
#2 の C・S、#3、#8＝候補 4 件）。**止まった先は 2 種類ある**:

- **監査対象のガード自身**（#1 C / #2 C の**調整後**）——`field_mutations()` の `..` なし
  destructure が E0027 を出す。ただし `audit_probe: _,` の 1 行で通り、`SECTION_TABLE` の
  足し忘れは残る（**素の停止は無関係な `Default for Config` の E0063 である**）
- **監査対象とは別の一覧**（#1 S / #2 S / #3 / #8）——`label()`・UI dispatch・`Display` の
  網羅 match・`reason()`。腕を 1 本足せば緑に戻り、当該一覧の足し忘れは残る

**どちらも「緑のビルドに足し忘れが残る」ので③に数えた**（§10.0 の定義）。

#### 10.6.1 同型の偽の主張の現存箇所（Task 5 の作業一覧は 10.6.1〜10.6.4 である）

**`field_mutations()` の doc は事実と食い違っている。** 「mutation と `SECTION_TABLE` の
両方に対応を追加するまで検出が続く」と書いてあるが、実測では destructure へ `audit_probe: _,` の
1 行を足すだけで 2 本とも緑になる。**#1000 で外部レンズが捕まえた 3 件と同じ型**（「足し忘れると
落ちる」と書いてあるが落ちない）である。

**そして同じ主張は 1 か所ではない。** `grep -rn "section_table_covers_all_config_fields"` で
数え上げた現存箇所は 4 つで、**1 か所だけ直すと 3 か所が残る**（`AGENTS.md`「バグ発見時は
同一パターン全コードパス検索を行う」）。

| # | 場所 | 主張 | なぜ偽か |
|---|---|---|---|
| A1 | `snotra-settings/src/app.rs:691-693`（`field_mutations()` の doc） | 「mutation と `SECTION_TABLE` の**両方に対応を追加するまで検出が続く**」 | destructure へ `_` を 1 つ書けば通る（実測 `2 passed`） |
| A2 | `snotra-settings/src/app.rs:102-104`（`TabId::has_changes` の doc） | 「`SECTION_TABLE` の 1 箇所だけを更新すればよい（`section_table_covers_all_config_fields` テストが**更新漏れを検出する**）」 | 同上。更新漏れは検出されない |
| A3 | `snotra-settings/src/app.rs:113-118`（`SECTION_TABLE` の doc） | 「この表の合成が `draft != saved` と一致することを **Config の全フィールドについて**検証する」 | 検証されるのは `field_mutations()` の `vec!` に載っているフィールドだけ。**全称が実装より強い** |
| A4 | `snotra-settings/CLAUDE.md:73` | 「更新漏れは `section_table_covers_all_config_fields` テストの**網羅 destructure がコンパイルエラー/テスト失敗で検出する**」 | 同上。**規範文書側の写しであり、放置すると規範を守る読者を誤らせる** |

**A4 は他の 3 か所より配送の射程が広い。** `snotra-settings/**/*.rs` を編集すると
`.claude/rules/snotra-settings.md` が自動配送され、その rule は「事実の正本は
`snotra-settings/CLAUDE.md`」と宣言している（ただし rule 自身はこの節を名指してはいない——
自動配送面から **1 ホップ**である）。`///` の doc コメントは当該ファイルを開いた者しか読まないので、
**A4 → A1〜A3 の順で直すのが配送コストに見合う。**

#### 10.6.2 候補外の偽の主張と、そこへ読者を送る 1 行（測定していない）

| # | 場所 | 主張 | 備考 |
|---|---|---|---|
| B1 | `snotra-settings/src/tabs/visual.rs:381-383` | 「**`app.rs` の `field_mutations` と同型**」と自称したうえで「`PresetDef` にフィールドが増えるとここがコンパイルエラーになり、**下の変異を足すまで検出が続く**」 | Task 1 の篩は `default_config_matches_obsidian_preset` を候補に採っていないため §10 の 36 分類には現れない。**構造が #1 C と同一なので測定は不要**（destructure へ `_` を 1 つ書けば通る）。Task 5/6 の射程には入れる |
| B2 | `snotra-settings/CLAUDE.md:135` | 「UI が持つ定数と `snotra-core` の既定値の一致は……**テストが唯一の検知手段になる**」と書き、例として B1 のテスト（`tabs/visual.rs` の `default_config_matches_obsidian_preset`）を名指す | **B1 に対する A4 と同じ関係**——規範文書側から B1 へ読者を送る 1 行。この行自身は「唯一の検知手段」と言うだけで網羅を主張していないが、指し先（B1）が偽なので**対で直す**。A4 と同じく `.claude/rules/snotra-settings.md` から 1 ホップ |

#### 10.6.3 同じ観点で ③ 26 件を再走査した結果（識別子を鍵にした走査で 4 件・述語を鍵にした走査で 1 件）

**探し方**: (1) ③ と判定した Rust 11 件のテスト名を 1 つずつ全リポジトリ grep
（`--include=*.rs --include=*.md --include=*.mjs --include=*.ps1 --include=*.psm1 --include=*.yml`）、
(2) ガバナンス 14 件が属する 8 つの `G-*` id を `.md` / `.rs` / `.toml` / `.yml` へ grep、
(3) 監査対象の一覧の識別子（`TabId::ALL` / `MODULE_INDEX_CRATES` / `governanceDocs` /
`REF_EXTENSIONS` / `ALWAYS_LOADED_FILES` / `STALE_EXTRA_DOCS` / `REQUIRED_RUSTDOC_LINTS` /
`REQUIRED_DISALLOWED_METHODS` / `DISALLOWED_METHODS_GROUPS` / `Get-SnotraTraceInvariantNames`）を
`.md` へ grep、(4) `.claude/rules/` `.claude/skills/` `.claude/agents/` の `governance:check`
言及を全数読む。**(1)〜(4) はすべて識別子を鍵にしている。**

**(5) 鍵を識別子から日本語の述語へ替えてもう一度走らせた**——`テストが.*検出|が捕捉する|赤で強制|が守る`
（外部レビューの鍵）と、`沈黙しない|素通りしない|漏れない|取りこぼさない|見落とさない|必ず(赤|落ちる)|保証する`・
`(が|を)検出する|検出が続く|(が|を)検知する|(が|を)捕まえる|(が|を)防ぐ|(が|を)見張る|(が|を)強制する|コンパイルエラーになる`・
`検出が続く|まで検出|網羅性ガード|網羅を強制`（自前の鍵。`.md` に加え `.rs` / `.mjs` / `.ps1` / `.psm1` も）。
**この経路でしか出ないものが 1 件あった（C5）**——保護の主張は識別子を 1 つも含まずに書けるので、
(1)〜(4) では原理的に届かない。

**ADR 層を対象に含めるかの基準**: **ADR は原則として対象外とする**（`ADR-adr-frozen-history` の
「凍結された歴史」——決定日時点の世界の記述は、今の実装と食い違っても直さない）。**ただし
その ADR 自身が「受容する残余」「読者への契約」として現在形で読者へ宣言している行は含める。**
凍結が免除するのは当時の記述であって、いま読者に手順を指示している行ではない。
この基準により C5（`ADR-adr-frozen-history.md:39`＝同 ADR が「読者への契約」と名指した行）は**含み**、
`ADR-stale-identifier-detector-scope.md:138, 240`（却下理由と、その論証の後日の引用）は**含めない**
——後者は判断の記録であって読者への手順ではなく、かつ「`G-references` が守るポインタの指し先」は
その母集団の中では現に真である。

| # | 場所 | 主張 | なぜ偽か（対応する実測） |
|---|---|---|---|
| C1 | `.claude/skills/health-check/references/mechanized-checks.md:9` | 「番号連続性に加え、**リポジトリ内の** `SPEC §N.x` 参照の実在も検査対象」 | 走査元は `governanceDocs()`（35 文書）だけ。ルート直下の新設文書・新 crate の `CLAUDE.md`・`.rs` / `.mjs` / `.ps1`・`docs/adr/`・`docs/superpowers/` はいずれも射程外（§10.3 #4 の F / X 側で `SPEC §99.9` が素通りすることを実測） |
| C2 | `.claude/rules/governance-docs.md:21` | 「ADR を消すときは生きた層の引用を散文化してから（**G-adr-citations が赤で強制する**）」 | 強制されるのは `governanceDocs()` + `docs/adr/` + `.claude/skills/**` + 非 docs の `.rs` / `.mjs` だけ。`.ts` / `.tsx` / `.ps1` の引用（§10.3 #7）・新設ルート文書・新 crate `CLAUDE.md` の引用（同 #5）は素通りする |
| C3 | `.claude/skills/health-check/SKILL.md:23` と `.claude/skills/implement/SKILL.md:77` | 「モジュール構成の乖離は G-module-index が機械検査する。**ここでは実行しない**」／「索引漏れは `governance:check` が捕捉する」 | 照合されるのは `MODULE_INDEX_CRATES` の 4 crate だけ。**5 つ目の crate は索引が丸ごと未照合のまま緑になる**（§10.3 #1 で実測）。health-check は全面委譲を宣言しているので、この穴を拾う人がいなくなる |
| C4 | `.claude/skills/health-check/references/mechanized-checks.md:8, 10` | 「対象は**ガバナンス文書群全体**に一般化された」 | **境界事例・低優先**。「ガバナンス文書群」を `governanceDocs()` の定義そのものと読めば真だが、読者は「規範文書ならどれでも」と取りうる（C1 と同じ母集団の話であり、C1 を直すときに合わせて見ればよい） |
| C5 | `docs/adr/ADR-adr-frozen-history.md:39`（「受容する残余」節） | 「ADR を削除するときは、生きた層に残る引用を先に散文化してから消す（**G-adr-citations が赤で強制するため、手順を忘れても沈黙はしない**）」 | **C2 と同一の主張で、「沈黙はしない」という全称否定を伴うぶん強い。** §10.3 #5 / #7 の実測どおり `.ts` / `.tsx` / `.ps1` の引用・新設ルート文書・新 crate `CLAUDE.md` は素通りするので偽。**C2 とは対である**——同 ADR の 1 行上（:38）が「読者への契約は**本 ADR と `.claude/rules/governance-docs.md` の 1 行**が担う」と、この 2 か所を契約の担い手として名指している。**片方だけ直すと相方が残る**（A 群とまったく同じ失敗形） |

**偽ではないと確認したもの（同じ grep に掛かったが射程が正しく書けている）**:
`Cargo.toml:32`（「上流 clippy が 3 つ目の群へ入れたら、その群は見張られない」と残余を名指し・
§10.3 #14 の実測と一致）、`src-tauri/clippy.toml:46` と `docs/build-commands.md:29`
（「見るのは既知 7 件の在否であって、足したパスが解決することは見ない」「8 件目として
足したパスの書き損じは射程外」・同 #13 と一致）、`docs/build-commands.md:28` と
`docs/development-principles.md:103`（G-workspace-lints は 2 lint を名指しで検査する設計だと
書いてある。§10.3 #12 の③は「3 つ目を**固定しない**」ことであって、doc が主張する降格・欠落の
検知は現に働く）、`docs/comment-guidelines.md:23`（G-stale-identifiers が「型で修飾した形には
当たらない」と残余を明記）、`src-tauri/src/startup.rs:78, 524, 529`（#6 が捕まえるという主張は
**②の実測どおり真**）、`scripts/lib/SnotraTraceInvariants.psm1:38-39`（`Get-SnotraTraceInvariantNames`
の doc は「写しを持つと黙って落ちる」と**警告している側**であり、保護を主張していない）、
`docs/development-principles.md:38, 42`（`TabId::ALL` を参照先・例として挙げるだけで、
その一覧が足し忘れから守られているとは主張していない）、
`docs/adr/ADR-startup-instrument-contract-shape.md:28`（「取り落としを捕まえるのは**キーの網羅**」＝
`Phase` の①の実測どおり真）、`docs/adr/ADR-clippy-disallowed-enforcement.md:43`
（「`disallowed_methods` **以外**の clippy lint のレベルは、どの検査も見ていない」と射程外を明記）。

**同じ形を正しく書けている実例が 1 つある（Task 5/6 の直し方の型）**:
`snotra-settings/src/i18n.rs:5-6` は「新キーは `TrKey` に variant を足すだけで、`ja()` / `en()` が
非網羅コンパイルエラーになり網羅を強制する」と A1 とよく似た主張をするが、**こちらは真である**——
守っているのが `..` なし destructure（`_` 1 つで黙る）ではなく **match の網羅性**であり、しかも
**その唯一の逃げ道を `#[deny(clippy::wildcard_enum_match_arm)]` で塞いだうえで、doc がその二段目まで
書いている**（:205 と :421 に属性が現に在ることを確認）。A1 群を「射程を書く」で倒すか「機構へ倒す」かを
決めるとき、機構側の到達点はこの形である。

**残り 9 件の Rust ③ 候補（#3・#4・#5・#8・#12〜#16）は、自分の定義位置以外のどこからも
名指されていなかった**（各テスト名の全リポジトリ grep がヒット 1 件。唯一の例外は
`docs/superpowers/plans/2026-07-25-pr-c-platform-event-dissolution.md:231` に写された
`event_names_are_pairwise_distinct` のコード断片だが、`docs/superpowers/` は #589 で
非規範化された歴史資料であり規範面ではない）。**ゆえにこれら 9 件について偽の主張が在りうるのは
テスト自身の doc コメントの中だけであり、§10.2 の「最終」列がその全数である**——**ただしこの全数は
「検出器を名指した主張」に限る。** 識別子を鍵にした走査 (1)〜(4) は、識別子を 1 つも含まない散文
（「この一覧の足し忘れは必ず落ちる」とだけ書いた行）には原理的に届かない。その層は (5) の述語走査で
覆ったが、**述語の鍵は語彙の当て推量であり網羅を主張できない**——この残余は受容する
（`AGENTS.md`「全称表現は前提条件とセットで書く」）。

#### 10.6.4 別種の誤り（射程ではなく検出器名の取り違え・1 件）

| # | 場所 | 誤り |
|---|---|---|
| D1 | `docs/development-principles.md:152` | 「hidden で working set trim が効かず可視時同値 43MiB のままだった gap は **G-module-index メモリ実測**だけが暴いた」——`G-module-index` はモジュール索引の双方向照合であってメモリを測らない。**検出器名の取り違え**であり、本監査が数えた「射程の食い違い」とは別の型だが、`G-*` を鍵にした走査 (2) に掛かったのでここへ置く（処分は Task 5 の判断に委ねる） |

#### 10.6.5 倒す順序（③ 31 分類 + A/B/C 群のあいだの優先関係）

1. **スモークの `$script:Invariants`（§10.4）** — ③ のうち**帰結まで実測できた唯一の件**。
   違反が確定していながら `FailureCount = 0` になる＝**足し忘れが製品の検証を素通りさせる**。
   §4 の「機構へ倒すのは足し忘れが製品の欠陥になるときだけ」に当たるのはここだけである
2. **A 群 4 か所（§10.6.1）** — doc が事実と食い違っており、`docs/development-principles.md:150`
   （「『このテストは X を検出する』と doc に書いたら、X を実際に起こして落ちることを確かめる」）に
   **リポジトリ自身の規範として反している**。順序は配送射程の広い **A4 → A1〜A3**
3. **C 群（§10.6.3）** — 規範面の偽。**C2 と C5 は対で直す**（ADR が契約の担い手として名指した 2 か所）。
   次に C3（`.claude/skills/` は毎セッション description が載る面）、C1（C4 は同時に見る）
4. **B 群（§10.6.2）** — 候補外だが構造は既知。**B1 と B2 も対**（A1↔A4 と同じ関係）
5. **残りの ③**（Rust #3・#4・#5・#8・#12〜#16 とガバナンス 14 件） — **既定どおり「射程を doc へ
   明記」で足りる**。名指す doc がそもそも無い（9 件）か、doc が既に残余を書いている
   （ガバナンス側の大半）ため、機構へ倒す理由が無い

## 11. 仕分けの結果（Task 5・Task 6 への申し送り）

**判定基準は §4 のとおり「足し忘れが製品の欠陥になるか」で、根拠を 1 行で書けることを条件にした。**
書けないものは既定どおり doc へ倒した（狭い保証で十分）。

### 11.1 集計

| 処置 | Rust | ガバナンス | スモーク | 計 |
|---|---:|---:|---:|---:|
| **doc へ射程を書いた** | 9 | 11 | 0 | **20** |
| **確認したが変更不要**（既に射程が書けている） | 2 | 3 | 0 | **5** |
| **機構へ倒す**（Task 6） | 0 | 0 | 1 | **1** |
| 計（③ の候補件数） | 11 | 14 | 1 | **26** |

このほか **A/B/C/D 群の偽の主張 12 件**（A 4・B 2・C 5・D 1）を是正した。**「件」は §10.6 の表の行数であって
ファイル数でも行位置の数でもない**——C3 は 2 ファイル（`health-check/SKILL.md` と `implement/SKILL.md`）に
またがり、A1〜A3 は `app.rs` の 3 箇所、A4 と B2 は同じ `snotra-settings/CLAUDE.md`、C1 と C4 は同じ
`mechanized-checks.md` なので、**触ったファイルは 9 枚**である。
A1〜A3 は Rust ③ #1/#2 と同じ箇所であり、上表の「doc へ射程を書いた」に含まれる。

### 11.2 機構へ倒す 1 件（Task 6 の入力）

**`scripts/lib/SnotraTraceInvariants.psm1` の `$script:Invariants`。**
判定根拠 1 行: **判定本体が violation を積んでも一覧に無ければ `FailureCount = 0` で exit 0 になり、
製品の回帰がスモークを素通りする**（§10.4 で帰結まで実測）。③ のうち製品の欠陥に達するのはここだけである。

**この件の doc は直していない**——`Get-SnotraTraceInvariantNames` の doc は保護を主張しておらず、
むしろ「呼び出し側はこの一覧を写さない……黙って落ちる」と**警告している側**である（§10.6.3）。
偽の主張が無いので消すものが無く、残るのは機構の実装だけである。

**実装結果（Task 6）**: `SnotraTraceInvariants.Tests.ps1` へ**モジュール自身のソーステキストを
走査するテスト**を 1 本足した。`Invariant = '…'` / `-Invariant '…'` のリテラルを拾い、
`Get-SnotraTraceInvariantNames` と**両方向**で突き合わせる。§2 が挙げた「母集団を一覧ではなく
ソーステキストへ移す」形（`startup.rs::count_matches_the_enum_declaration`）の PowerShell 版であり、
一覧は残して照合を足した——順序が表示・集計の列順を決めるため、導出に置き換えると編集順が
列順になる。**一覧を母集団に取る形では原理的に届かない**ことは、既存の
「返す名前が Overall のキーと過不足なく一致する」が変異下でも緑のまま通ることで確認した。

実測（Pester 6.0.1・`target/pester/` のキャッシュ・`SnotraTraceInvariants.Tests.ps1` 単体）:

| 状態 | 結果 |
|---|---|
| 素（検査を足す前） | 41 passed / 0 failed |
| §10.4 の変異（H6 を判定本体へ・一覧へ足さない）／検査を足す前 | 41 passed / 0 failed（③ の再現） |
| 同変異／検査を足した後 | 41 passed / **1 failed**（`but got 'H6'`） |
| 逆向きの変異（`'H9'` を一覧へ・判定本体には無い） | 41 passed / **1 failed**（`but got 'H9'`） |
| 素（検査を足した後） | **42 passed / 0 failed** |

**新しい検査自身の射程**（正本は同テストのコメント）: 単一引用符のリテラルしか見えない・
`Invariant = '…'` と `-Invariant '…'` の 2 形しか見ない・走査するのは当該 `.psm1` 1 枚だけ・
順序は守らない・**名前と判定の対応も守らない**（H4 の判定が H5 の名前で違反を積んでも、
両方が一覧に在れば通る）。`npm run test:powershell` が `scripts/lib` を丸ごと拾うため、
この検査は PR CI（`ci.yml` の "Run PowerShell tests (Pester)"）で走る——**ただし常時ではない**。
当該ステップは `rust-check` job に属し、その job は `skip-ci` ラベルの付いた PR では
**丸ごと省かれる**（`ci.yml` の 3 job のうち skip-ci ガードを持たないのは `governance-check`
だけである・#587）。

**skip-ci を張った PR で消えるのは照合だけで、判定器そのものは動く。** `e2e.yml` は
`scripts/lib/**` を `paths` に持ち（`ci.yml` とは別に `pull_request` で起動し、skip-ci
ガードを持たない）、その `smoke-egui` job が走らせる `scripts/smoke-egui.ps1` は
`SnotraTraceInvariants.psm1` を import して `Test-SnotraTraceInvariants` を呼ぶ。
**つまりこのモジュールを触った PR では、一覧の照合が省かれたまま判定器だけが本番同様に
走る組み合わせが作れる**——§10.4 が測った「違反を積みながら exit 0」がまさにその状況で
起きる。skip-ci はそのために張るラベルではないが、射程としてはここが最も薄い。

### 11.3 機構化の余地（Task 6 の裁量・必須ではない）

**Rust ③ #1/#2（`SECTION_TABLE` / A 群）。** 足し忘れの帰結は「新セクションを編集してもタブ別
ダーティ点（`•`）が出ない」であり、保存・破棄は構造体全体の `PartialEq` で判定するのでデータは
壊れない——**表示の退行に留まるため §4 の基準には届かない**と判定し、doc で倒した。ただし
§10.6.3 が挙げた到達点（`snotra-settings/src/i18n.rs` の「網羅 `match` ＋ `wildcard_enum_match_arm`
の deny」）はこの箇所にもそのまま当たるので、Task 6 が費用を見て採るなら止める理由は無い。

**この issue では実装しない（裁定・2026-08-09）。** §4 の基準（足し忘れが製品の欠陥になるか）に
届かない件を裁量で格上げすると、必須 1 件（§11.2）の実測と混ざる。フォローアップとして残す
——**手を付けなかったのは見落としではない**。

**却下した案（否定の知識）: `/health-check` へ「crate が増えた直後は目で見る」という手順を足す。**
本サイクルの途中で実際にこれを書き、修正ラウンド 1 で取り下げた。理由は 3 つある——
(a) 同スキルが冒頭で**実行するのは 2 つ**と宣言する構造と食い違い、本文中に 3 つ目の条件付き手順が
生える。(b)「crate が増えた直後」を**誰がいつ判定するか**が決められない（`/health-check` は crate
追加の直後に走るとは限らない）。(c) ルート `CLAUDE.md`「最重要ルール」2 が定める
**エージェント設定（スキル・フック・rules）の変更は合意してから**に当たり、Claude が単独で
判断してよい範囲を越える。**そもそも前提が偽だった**——手順で埋めようとした穴は
`governance-check.test.mjs` の母集団カナリアが既に塞いでいた（§11.4）。**規範を手順で足す前に、
既存の機構が見ていないかを測るべきだった**というのが、この 2 度の誤りから残る教訓である。

### 11.4 crate の足し忘れは誰が見るか（修正ラウンド 2 で訂正）

**本節はかつて「機構も人も見ない区間ができる」と書いていた。これは偽であった**——最終レビューが
反例を挙げ、`scripts/governance-check.test.mjs:93-124` を読んで確かめた。

**現に固定されている**: 同ファイルの母集団カナリア（#701）が実 `Cargo.toml` の
`[workspace] members` を読み、**`CLAUDE.md` を持つ member が `MODULE_INDEX_CRATES` と
`governanceDocs()` の両方に載ること**を assert する（母集団が空になる形へのガードも持つ）。
`vitest.config.ts` の include が `scripts/**/*.test.mjs` を拾い、`npm test` として CI の
`node-check`（ubuntu）と `rust-check`（windows）の両方で走る。**残る穴は `CLAUDE.md` を持たない
crate だけであり、そのとき照合すべき索引もまだ無い**（ほかに `skip-ci` ラベルの付いた PR では
両 job とも走らない——実測: `ci.yml` の当該 2 job だけがそのラベルで gate されている）。

**誤りの機序**: §10.3 の測定コマンドは**すべて `npm run governance:check`** であり、
**vitest 層は §3 が定めた 3 層の母集団に入っていない**（§12.5 に明記した）。`governance:check` が
緑だったという観測は正しく、そこから**「機構は無い」へ一般化した瞬間に偽になった**。
`AGENTS.md`「全称否定（『X は存在しない』）も同じ強さの主張である——不在の観測 1 つで確定させず、
探し方を変えて所在を確かめてから書く」に、**その規範を引用しながら書いた本書自身が反していた**。
皮肉なことに、そのカナリアのコメント自身が「crate を足しても何も鳴らない（沈黙する経路）」と
この穴を説明しており、**カナリアはまさにそれを塞ぐために #701 で書かれていた**。

**波及の是正**（同一パターン全コードパス検索の結果・5 か所）: `.claude/skills/health-check/SKILL.md`・
`.claude/skills/implement/SKILL.md`・`governance-check.mjs` の `MODULE_INDEX_CRATES` と
`governanceDocs()` の doc・本節。**§10.3 #1 / #3〜#5 の X 側の測定表には脚注を付けた**——
③ の判定（`governance:check` 単体では照合されない）は変えていない。測定が測ったとおりだからである。

### 11.5 変更不要と確認した 5 件

- Rust: `events.rs::event_names_are_pairwise_distinct`（§2 の範そのもの）・
  `startup.rs::failure_reasons_are_stable_and_unique`（「`Phase` と同じ弱さを持つ」と明記済み）
- ガバナンス: `VOCAB_TEST_FILE`（Rust 側の穴を「受容する残余」節が持つ）・
  `REQUIRED_DISALLOWED_METHODS`（射程の正本は `src-tauri/clippy.toml` 冒頭と
  `docs/build-commands.md`）・`DISALLOWED_METHODS_GROUPS`（残余をコメントが明記済み）

### 11.6 検証

`cargo doc --workspace --no-deps --document-private-items` は `snotra-core` の既存 9 件
（`private_intra_doc_links`・`--document-private-items` を渡したときだけ出る形）のみで、
**本サイクルの前後で同数**（stash して測った）。新規の警告は無い。
`npm run governance:check` は全検査 passed、`vitest run scripts/` は 316 passed。

#### 11.6.1 倒した後の再変異（Task 7・§6 の受け入れ「倒した後で変異を当て、実際に落ちる/落ちないことを確かめている」）

**測ったのは Task 5・Task 6 をすべて含んだ最終状態である**（Task 6 の Red はその時点の木での測定）。

**(1) 機構へ倒した 1 件は、今度は落ちる。** §10.4 と同じ変異（判定本体へ H6 を足し
`$script:Invariants` へは足さない）を当て、`Invoke-Pester -Path scripts/lib/SnotraTraceInvariants.Tests.ps1`
（Pester 6.0.1）:

```
[-] 判定本体が名指しする不変条件と一覧が過不足なく一致する（モジュールのソースを走査する）
 Expected $null or empty, because 判定本体だけが知る不変条件は FailureCount に数えられず exit 0 になる, but got 'H6'.
PASSED=41 FAILED=1
```

素は **42 passed / 0 failed**。**同じ変異が §10.4 では 41 passed / 0 failed だったので、
差は新しい検査 1 本によるものである。**

**runtime の帰結は変わっていない**（`rows = 2000` を食わせると依然 `Violations: H6` /
`Overall keys: H1,H4,H5` / `FailureCount: 0`）。**倒したのは検知であって挙動ではない**——
足し忘れは今もスモークを素通りさせるが、**その足し忘れを持つ木は Pester で赤になるので出荷されない**。

**(2) 全体の挙動不変**（doc へ射程を書いた 20 件 + 変更不要 5 件を面で覆う）:

| コマンド | 結果 |
|---|---|
| `cargo test --workspace` | **884 passed / 0 failed / 21 ignored**。合計 **905** は §3 の母集団（`cargo test --workspace -- --list` の実測）と一致——**テストは 1 本も増減していない** |
| `npm run governance:check` | 全検査 passed。構造の件数（検査 19 / 対象文書 35 / rules 8 / skills 12 / 見出し参照 180 / member 4 / clippy 禁止 7 / ADR 41 / 短縮引用 210）は Task 4 時点と**すべて同一**。動いたのは面積（rules 11469 → 11554 字）と散文の識別子（286 → 290 件）だけで、どちらも Task 5 が rules / doc へ射程を書き足した分である |
| `npm run test:powershell` | **98 passed / 0 failed**（Task 6 の新設 1 本を含む）。**1 回目は 97 passed / 1 failed** で、落ちたのは `起動後の最初のフレームで入力欄が打鍵を受け取れる状態になっている`——`SnotraSmoke` 側の実機起動テストで、**再走で緑**（#897 が記録する既知のフレークと同じ形）。本サイクルが触ったのは `SnotraTraceInvariants` だけであり、当該テストは import すらしない |

**(3) 代表への再変異 — 変異 6 通り・③ 候補としては 4 件**（Task 5 が最も手を入れた箇所を選んだ）。
**数え方に注意する**（本監査は「候補件数」と「延べ分類数」を併用しているので混ざりやすい）:
**Rust #1 の C と S は同一候補の 2 分類であって 2 候補ではない**（§9.0 の二重分類）。
**B1 は ③ 26 候補の外である**——§10.6.2 のとおり Task 1 の篩が
`default_config_matches_obsidian_preset` を候補に採っていないので、分母にも分子にも入らない。
ゆえに**代表で直接検証した distinct な ③ 候補は 4 件**（Rust #1・ガバナンス #6・ガバナンス #8・Rust #16）である。

| 代表 | 選んだ理由 | 変異 | 結果 |
|---|---|---|---|
| Rust #1 C（`section_table_covers_all_config_fields`） | A 群の中心。`app.rs` は本サイクル最大の Rust 差分（26 行） | `Config` へ `audit_probe` ＋ `Default` と destructure の最小修正 | **2 passed**（③ のまま） |
| Rust #1 S（同上・`TabId::ALL`） | 同上 | `TabId` へ `AuditProbe`（`ALL` へは足さない）＋ 網羅 match 2 本へ腕 | **2 passed**（③ のまま） |
| **B1**（`default_config_matches_obsidian_preset`） | Task 5 が主張文を書き換えた箇所。**§10.6.2 で「測定不要」としたまま射程文だけを書いたので、ここで初めて測る** | `PresetDef` へ `audit_probe` ＋ destructure へ `audit_probe: _,` ＋ 全 `PRESETS` を埋める（変異は足さない） | **1 passed** ——**書き換えた射程文（「`新フィールド: _,` の 1 行で通り、下の変異を足さないまま緑になる」）が実測どおりであることを確認した** |
| ガバナンス #8（`ALWAYS_LOADED_FILES`） | `governance-check.mjs` へ射程コメントを足した箇所 | 5000 字の `PROBE-ALWAYS.md` ＋ `CLAUDE.md` へ `@` | **緑**。常時ロード面は 14421 → **14438 字**（§10.3 と同値） |
| ガバナンス #6（`REF_EXTENSIONS`） | 同上 | `docs/architecture.md` へ実在しない `` `scripts/lib/NoSuchProbe.psm1` `` | **緑**（③ のまま） |
| Rust #16（`system_shortcuts_are_checked_after_semantic_normalization`） | `snotra-core/src/hotkey.rs` は 2 番目に大きい Rust 差分（21 行） | `is_system_shortcut()` へ `alt_only && Home` を追加（テストの `blocked` へは足さない） | **10 passed**（③ のまま） |

**(4) 直接測っていない 21 件が (2) でカバーされる理由**（③ 26 候補 − 代表 4 件 − 機構 1 件〔(1) で別枠に検証済み〕
＝ **21**。§11.1 の処置別に割ると、**代表 4 件はすべて「doc へ射程を書いた」20 件の中から選んでいる**ので、
残りは同 20 件のうちの **16 件**と、「変更不要」**5 件のすべて**である）:
**本サイクルで実行される行を足したのは
`SnotraTraceInvariants.Tests.ps1` の新しい検査 1 本だけである。** 目視ではなく機械で確かめた——
`git diff 1f02be1..HEAD -- '*.rs'` と同 `-- scripts/governance-check.mjs` から
`+++` / `---` とコメント行（`///` `//!` `//` `*` `/*`）を差し引くと**残る行が 0 件**であり、
`SnotraTraceInvariants.psm1` の +10 行も同じくコメントである。残りはすべて `*.md` の散文。
**コメントは実行されない**——ゆえに「doc を書くついでに挙動を変えた」が起きうるのは
(i) コメントが構文を壊してコンパイル/parse に失敗する、(ii) `.mjs` のコメントが検査の入力
（語彙・ADR 引用）に紛れ込む、の 2 経路だけである。(i) は `cargo test --workspace` の 905 件と
`npm run test:powershell` の 98 件が、(ii) は `npm run governance:check` の構造件数が
Task 4 と一致することが覆う。**各候補の③性は「その一覧の外で起きた追加を検査が見ない」という
構造の性質であり、コメントの追加では変わらない**（変わるなら (i)(ii) のどちらかとして現れる）。

## 12. 受け入れの検算（Task 8）

§6 の 3 条件を 1 つずつ、**根拠の節を指したうえで対象そのものを測って**確かめた
（`AGENTS.md`「主張は代理ではなく対象そのもので測ってから書く」）。

### 12.1 条件 1 — 母集団が数え上げられ、件数が記録されている

**満たしている。** 記録は §3（3 層の母集団）・§9（候補 31 件）・§9.0（延べ 36 分類）・
§10.5（③ 31 分類 / 26 件）。Task 8 で 3 層とも取り直した:

| 層 | コマンド | 実測（2026-08-09・Task 8） | §3 の記録 |
|---|---|---:|---:|
| Rust | `cargo test --workspace -- --list \| grep -c ': test$'` | 905 | 905 ✅ |
| ガバナンス | `grep -c 'id: "G-' scripts/governance-check.mjs` | 19 | 19 ✅ |
| スモーク | `SnotraTraceInvariants.psm1:30` の `$script:Invariants` | 3 | 3 ✅ |

ガバナンスの 19 は `npm run governance:check` が出す「検査 19 件」とも一致する（別経路の照合）。
**ただし §3 に書いてあったコマンドは引用符が実装と違い 0 件を返していた**——Task 8 で直し、
経緯は §3.1 末尾に残した。**件数そのものは正しく、直したのは再現手段である。**

### 12.2 条件 2 — 各件が「射程を書く」「機構へ倒す」のどちらかに倒れている

**満たしている。** 倒す対象は③の 26 候補であり（①4 分類・②1 件は現に守られているので倒す対象ではない・
§10.5）、処置の集計は §11.1 の 20（doc へ書いた）+ 5（既に書けていた・§11.5）+ 1（機構・§11.2）= 26。
**設計書の表を信じずに現物を数えた**:

| 処置 | 現物 | 実測 |
|---|---|---|
| doc へ射程（Rust 9 件） | `app.rs`（#1/#2）・`error.rs`（#3）・`hotkey_input.rs`（#5）・`snotra-core/hotkey.rs`（#12/#13/#14/#16）・`platform/hotkey.rs`（#15） | 実測へ差し戻した射程 doc が在る（`#1008` を伴う記述は 5 ファイルに計 9 か所。`TabId::has_changes` の doc だけは番号を持たず、`SECTION_TABLE` の doc を正本として指す形） |
| doc へ射程（ガバナンス 11 件） | `governance-check.mjs` の射程コメント **9 か所**（`grep -c 1008` ＝ 9） | 9 か所で 11 候補を覆う——`governanceDocs()` の 1 か所が #3/#4/#5 の 3 件を担うため（§9.2） |
| 機構（1 件） | `SnotraTraceInvariants.Tests.ps1` のソース走査テスト | §12.3 で赤にできることまで測った（Task 8 が独立に再現） |
| 変更不要（5 件） | §11.5 の 5 件 | 射程が既に書かれていることを Task 5 が確認済み |

このほか A/B/C/D 群 12 件（9 ファイル）の偽の主張を是正した（§11.1）。

### 12.3 条件 3 — 倒した後で変異を当て、落ちる/落ちないことを確かめている

**満たしている。** 一次の記録は §11.6.1（Task 7 が最終状態で測った (1)(2)(3)）。
**Task 8 は、そのうち「機構へ倒した 1 件が今度は落ちる」を独立に再現した**——受け入れの中核であり、
報告の転記で済ませない（`AGENTS.md`「『解消した』の判定は再実行の結論を受け取るのではなく、
指摘を見つけた道具で自分で測る」）。

| 状態 | `Invoke-Pester -Path scripts/lib/SnotraTraceInvariants.Tests.ps1`（Pester 6.0.1） |
|---|---|
| 素 | **42 passed / 0 failed** |
| 変異（判定本体へ `Invariant = 'H6'` を足し `$script:Invariants` へは足さない） | **41 passed / 1 failed** ——`Expected $null or empty, …, but got 'H6'` |

変異は `git checkout -- scripts/lib/SnotraTraceInvariants.psm1` で撤去し、`git status --short` が
空であることを確認した。**doc へ倒した側が落ちないことは §11.6.1 (3) の代表 4 件が測っており、
Task 8 では取り直していない**（変異が破壊的で、最終検証の木を汚さないため）。

### 12.4 最終検証（Task 8・全コマンドの実測）

| コマンド | 結果 |
|---|---|
| `npm run governance:check` | **exit 0・全検査 passed**（検査 19 / 対象文書 35 / rules 8 / skills 12 / 常時ロード 14421 字 / rules 11554 字 / 見出し参照 180 / member 4 / clippy 禁止 7 / 散文の識別子 290 / ADR 41 / 短縮引用 210）——**§11.6.1 (2) の記録と全項目一致** |
| `cargo test --workspace` | **884 passed / 0 failed / 21 ignored**（計 905・母集団と一致） |
| `npm run test:powershell` | **98 passed / 0 failed**（1 回目で緑。§11.6.1 (2) が記録したフレークは再現せず） |
| `cargo doc --workspace --no-deps --document-private-items` | **exit 0**。警告は `snotra-core` の既存 9 件のみで**新規は無い**（§11.6 と同数） |
| `git status --short` | **空**（変異・一時ファイルの残留なし） |

### 12.5 受け入れの外に残るもの（本書が主張しないこと）

- **vitest 層（`.claude/hooks` / `.githooks` / `scripts` の `*.test.mjs`・`npm test` で実測 617 本。
  内訳は `scripts/` 316・`.claude/hooks` 285・`.githooks` 16）は §3 の 3 層母集団に入っていない。** §10 の測定コマンドは Rust が `cargo test`、ガバナンスが
  `npm run governance:check`、スモークが `Invoke-Pester` であり、**`npm test` は一度も走っていない**
  ——ゆえに本書の ③ は「その 3 コマンドが照合しない」を意味するのであって、「どの機構も見ない」
  ではない。**この読み違えを実際に犯した**（§11.4。`governance-check.test.mjs` の母集団カナリアが
  現に crate の足し忘れを固定していた）。同じ層に他の相方が居るかは §11.4 の走査で確かめたが、
  **その走査は「実リポジトリを読む test」を鍵にした 1 軸であり網羅を主張できない**
- **篩の見落としの検算は Rust 層にしか掛かっていない**（§4 Phase 2 の「実測」）。ガバナンス 19 件・
  スモーク 3 件は全数を手で読んだが、grep 2 軸のような独立の検算は当てていない
- **偽の主張の走査は、識別子と日本語述語を鍵にした grep であり網羅を主張できない**（§10.6.3 の末尾）
- **③ 26 件のうち 21 件は変異を直接当て直していない**——根拠は §11.6.1 (4)（本サイクルが足した
  実行される行は 1 本だけ、という機械的な確認）であって、各件の再測定ではない
