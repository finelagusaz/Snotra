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
| S（ソーステキスト） | 23 | Rust 12（純 10 + 二重 2）・ガバナンス 10（純 7 + 二重 3）・スモーク 1 |
| F（ファイルシステム） | **0** | 該当なし（理由は次段） |
| X（外部設定） | 7 | ガバナンスのみ（純 4 + 二重 3） |

候補の行数は Rust 16 + ガバナンス 14 + スモーク 1 = 31、二重分類 5 件（Rust 2・ガバナンス 3）を
加えた延べ分類数は 36（6+23+0+7）。

**C はすべて Rust に閉じる**——ガバナンス・スモークの母集団はいずれも Rust の型システムの外側
（JS/PowerShell のリテラルか、TOML/外部ツールの仕様）にあるため、コンパイラが SSOT になり得ない。
**Task 6 の derive 導入判断に効く数字は C=6**（うち純 C は 4 件、残り 2 件は Rust #1・#2 の
二重分類の片側）。

**F が 0 件である理由**: 「母集団がファイルシステムそのもの」の検査は Phase 2 の時点で
「母集団の SSOT が走査対象自身にあり、足し忘れの経路が構造的に無い」として候補から除外済み
（§9.2 末尾「候補から外したもの」の最終段落・`G-architecture-table` 等 11 件）。手書きリテラルの
まま残った候補は、性質上「ファイルの実在」ではなく「ソーステキスト上の編集方針」（S）か
「外部ツール／TOML 設定の仕様」（X）のどちらかに落ちた——動的走査で守られる型（F）と
手書きで取り残される型（S/X）が非重複だったのは偶然ではなく、**F 型のリスクは Phase 2 以前に
既に機構で塞がれているため**である（詳細は Task 3 レポート）。

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
（S 7 件・X 4 件・S+X 二重 3 件。延べ S10/X7。分類は Task 3、判定基準は §9.0）。

| # | 候補 | 手書き一覧（識別子・行） | 見落としうる形 | 分類 | 分類理由（母集団の真の SSOT） |
|---|---|---|---|---|---|
| 1 | `G-module-index` | `MODULE_INDEX_CRATES`（governance-check.mjs:92、crate 4 件） | 新 crate を追加しても追記し忘れると、そのモジュール索引は双方向照合されない | X | crate 一覧の真の SSOT はルート `Cargo.toml` の `[workspace] members`（TOML 設定）。governance-check.mjs 自身の手書きではなく外部設定の写しである |
| 2 | `G-module-index` | 順方向照合の拡張子フィルタ（governance-check.mjs:116、無名の正規表現 `rs\|ts\|tsx\|html`） | 「モジュール構成」節に `` `foo.mjs` `` のようなこの 4 拡張子以外のバッククォート参照があっても実在照合されない（`MODULE_INDEX_CRATES` とは独立した 2 本目のハードコード拡張子一覧） | S | どの拡張子を実在照合の対象にするかは本プロジェクト独自の編集方針であり、参照すべき外部の権威的仕様は無い |
| 3 | `G-references` | `governanceDocs()`（governance-check.mjs:1339、ルート文書 4 件 + crate CLAUDE.md 正規表現 4 crate） | 新 crate の CLAUDE.md がこの正規表現に無いと、その文書内の参照実在は照合されない | S + X | ルート文書 4 件のリストは編集方針の手書き決定（S）。crate CLAUDE.md 正規表現は `MODULE_INDEX_CRATES`（#1）と同じ crate 名一覧を独立に持つ「2 本目」（Task 2 が既に指摘）——真の SSOT は同じく Cargo.toml（X）。二重分類 |
| 4 | `G-spec-sections` | 同上（`governanceDocs()` を共有） | 同上——新 crate CLAUDE.md 内の `SPEC §N` 参照が照合対象から漏れる | S + X | #3 と同一関数を共有するため同じ橋渡し構造（root docs=S、crate 名=X） |
| 5 | `G-adr-citations` | 同上（`adrCitationDocs` が `docs`＝`governanceDocs()` を含む） | 同上に加え ADR 短縮引用が該当文書内で照合されない（他の入力＝ADR/skills/`.rs`・`.mjs` は走査ベースのため影響は限定的） | S + X | #3/#4 と同じ `governanceDocs()` を内包（他の入力は動的走査ゆえ候補から除外済み・下記参照） |
| 6 | `G-references` | `REF_EXTENSIONS`（governance-check.mjs:30、拡張子 11 種） | バッククォート内パス様参照の実在照合は、拡張子がこの一覧に無いファイル種別（`/` を含んでいても）を静かにスキップする（修正ラウンド 1 の指摘） | S | 「実在検査の対象と見なすソース系拡張子」は編集方針であり、外部仕様の写しではない |
| 7 | `G-adr-citations` | `adrCitationDocs` の `.rs\|.mjs` 拡張子ホワイトリスト（governance-check.mjs:1757、`/\.(rs\|mjs)$/`） | `.ts` / `.tsx` / `.ps1` 等の非 docs ソースに ADR の短縮引用があっても実在照合を素通りする（修正ラウンド 2 の指摘。同じ行の `!f.endsWith(".test.mjs")` だけを分析し本体の向き判定を書き漏らしていた） | S | 同上（#6 と同型の編集方針） |
| 8 | `G-area-budget` | `ALWAYS_LOADED_FILES`（governance-check.mjs:1052、`["CLAUDE.md", "AGENTS.md"]`） | 常時ロード面に 3 つ目のファイルが増えても追記し忘れると火災報知器の面積に算入されない | S | 「常時ロードされる」は Claude Code ハーネスの挙動という外部事実だが、それを記述する機械可読な設定ファイルは本リポジトリに無い——この配列自身が唯一の記録であり、他に指せる SSOT が無い |
| 9 | `G-stale-identifiers` | `STALE_EXTRA_DOCS`（governance-check.mjs:1505、固定パス 4 件） | 新設した「意図の SSOT」級の文書がここに無いと、腐り識別子の検査対象から漏れる | S | コメント自身が「静的リテラルであること自体が fail-closed である」と明記——ディレクトリ規則で導出できない編集方針上の選定（#8 と同型） |
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
