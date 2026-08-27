# 独立導出レビュー: #1201（母集団を `activation.rs` 1 枚 → `launcher_controller/` 直下の子 `*.rs` 群へ）

走査範囲: `src-tauri/`・`docs/`・`AGENTS.md`・`CLAUDE.md`・`SPEC.md`・`.claude/rules/` のみ（指定どおり `workspace/` / `.claude/worktrees/` / `target/` / `scripts/` は見ていない）。

## 0. 実測した地形

`C:/workspace/Snotra/src-tauri/src/egui_shell/launcher_controller/` 直下の子 `*.rs`（6 枚）:

```
activation.rs  folder_nav.rs  frame_stages.rs  hide_request.rs  search_flow.rs  updater_toast.rs
```

`mod` 宣言は `src-tauri/src/egui_shell/launcher_controller.rs:56-61`（6 本・`mod activation; mod folder_nav; mod frame_stages; mod hide_request; mod search_flow; mod updater_toast;`）。
`activation/tests.rs` は**直下ではなくサブディレクトリ**（`activation/`）にあるので、#1201 の母集団定義（「直下」）から構造的に外れる。

実測（すべて上記 6 枚に対して）:

- 3 本のアンカー（`fn on_enter(` / `fn activate_or_execute(` / `fn shift_activate(`）の**逐語出現はそれぞれ 1 件**、すべて `activation.rs`（397 / 472 / 589 行）。兄弟ファイルの散文には `` `on_enter` `` 等がバッククォート付きで現れるだけで `fn ` を伴わない（`search_flow.rs:5,41,105`、`launcher_controller.rs:14,172`）
- 6 枚に **字下げ 4 以外の `fn ` 行は 1 本も無い**（indent 0 / 8 とも 0 件）。→ `tests.rs:255` の「コーパスからは測れない」の根拠は**母集団を広げても成立する**（再測済み）
- 禁止語（`self.indexing()` / `read_visible_rows(` / `read_config(`）の 6 枚での出現: `activation.rs:357,572` / `hide_request.rs:49` / `search_flow.rs:46,87,128`。**どの出現も、そのファイルの最初の字下げ 4 ヘッダより後**にある（各ファイルの最初のヘッダ行: 90 / 38 / 41 / 28 / 30 / 31）
- `#[cfg(test)]` / `mod tests` は 6 枚のうち `activation.rs:634-635` のみ（`mod tests;` の 1 行）。→ テスト側のリテラルは母集団へ入らない
- 各ファイルの**最後の**字下げ 4 ヘッダ: activation.rs=`fn on_enter(`(589) / folder_nav.rs=`fn on_nav_keys(`(143) / frame_stages.rs=`fn poll_async(`(125) / hide_request.rs=`fn on_focus_changed(`(91) / search_flow.rs=`fn poll_search_debounce(`(296) / updater_toast.rs=`fn spawn_install(`(64)
- 起動の入口 2 本は `pub(in crate::egui_shell) fn` の形（`activation.rs:397,589`）。`method_header` はこの形を通す（`pub` → `(`…`)` の読み飛ばし）が、`method_header_accepts_visibility_and_async_before_fn`（tests.rs:300-311）の fixture 6 本に `pub(in …)` は無い

追加の実測:

- `activation.rs` の可視性修飾: `pub(super) fn drain_launch(`(247) / `pub(super) fn execute_slash(`(294) / `pub(in crate::egui_shell) fn activate_or_execute(`(397) / `pub(in crate::egui_shell) fn on_enter(`(589)。**起動の入口 3 本の実際の形は `pub(in crate::egui_shell) fn` × 2 と素の `fn` × 1 であり、`pub(super) fn` は 1 本も無い**
- `docs/superpowers/` に `activation.rs` / `launcher_controller` を名指すソーステキスト検査の記述は 0 件（`母集団` の hit は `indexing.rs` / `startup.rs` / `main.rs` / governance 系の凍結済み計画のみ）
- `SPEC.md` に `activation` / `起動の入口` / `母集団` / `ソーステキスト` の hit 0 件
- 「外へ出」の hit のうち本 issue に該当するのは `src-tauri/CLAUDE.md:41` の 1 件のみ

---

## 1. 変更が必要なファイルと、触るシンボル・節

### A. `C:/workspace/Snotra/src-tauri/src/egui_shell/launcher_controller/activation/tests.rs`（本体）

| 位置 | 触るもの | 内容 |
|---|---|---|
| `//!` :1-8 | モジュール doc | 「母集団は `activation.rs` 1 枚である」「`include_str!("../activation.rs")`」「起動の入口が別の子モジュールへ移れば母集団は割れる」を、直下の子 `*.rs` 群という新しい母集団の記述へ差し替える。**「1 枚」「割れる」の語がここの核心**。切り出し helper の局所性（ADR 参照）はそのまま |
| :415-416, :494-495, :564-565 | `let src = include_str!("../activation.rs");` × 3 | 母集団の取得。**3 検査すべてが同じ 1 行を持つ**——写しである。1 つの `fn population() -> [(&'static str, &'static str); N]`（ファイル名, ソース）へ寄せ、3 検査がそれを回す形が素直 |
| :414-463 | `activation_uses_frame_values_not_live_reads` | `owners_of(src, needle)` を**ファイル単位**で回す（下記 4-(2)）。ヘッダ assert（:427-434）は全ファイル横断のヘッダ集合で判定してよい |
| :493-515 | `activation_entry_points_consult_the_display_gate` | `method_body(src, anchor, canary)` を、**アンカーを含むファイルを特定してから**当てる（下記 4-(3)）。「ちょうど 1 枚が含む」assert を足す |
| :563-576 | `on_enter_delegates_the_flush_decision_to_the_predicate` | 同上 |
| :255-260 | `method_header_requires_exactly_four_spaces_of_indent` の doc | 「`include_str!` が読む `activation.rs` には字下げ 0 / 8 の `fn ` 行が 1 本も無く（分割の前後で不変・2026-08-27 に再測）」→ **母集団 6 枚で再測した結果も 0 件**（本レビューの §0）。文面を新しい母集団へ更新し、再測日を書き換える |
| :296-311 | `method_header_accepts_visibility_and_async_before_fn` とその doc | doc :297-298「現に `pub(super) ` が挟まる定義が起動の入口に在り」は**今日すでに偽**（入口は `pub(in crate::egui_shell)`）。fixture 6 本に `pub(in crate::egui_shell)` 形が無い。母集団が広がると 4 枚がこの形を使うので**さらに load-bearing になる**——fixture を 1 本足し、doc を実測へ合わせる |
| :382-389 | `activation_uses_frame_values_not_live_reads` の doc「対象外へ落ちる経路は 2 通り」 | 「`run_search_with` の `indexing` の live-read と `lang()` の `read_config` は `search_flow.rs` と `launcher_controller.rs` に在り、**この検査はもう見ていない**」→ **半分が偽になる**。`search_flow.rs:87` の `self.indexing()` は母集団へ入り、`fn run_search_with(` へ帰属して落ちる（＝「母集団の外」ではなく「帰属で落ちる」側へ移る）。`launcher_controller.rs`（親・直下ではない）側は依然「母集団の外」 |
| :398-405 | 同 doc「母集団は production 1 枚（`activation.rs`）で、この `mod tests` は別ファイルだから」 | 「1 枚」が偽になる。**構造的保証が何に載っているかが変わる**——新しい根拠は「母集団が**直下のみ**であり `activation/tests.rs` はサブディレクトリに在る」こと。この一句を明示しないと、後で母集団を再帰走査へ広げる変更が保証を黙って壊す |
| :411-413 | 同 doc 末尾「**母集団は `activation.rs` 1 枚である。** …そのとき直すのは母集団であって assert ではない（`activation.rs` の `//!` が正本）」 | 段落ごと差し替え。**参照先の `//!` から当該規範が消える**ので、宙に浮いた正本指しになる |
| :423-426 | `entry_points` のヘッダ assert のコメント | 「3 本のアンカーは可視性修飾の有無で 2 形（`pub(super) fn` / 素の `fn`）に分かれる」→ **今日すでに偽**（実際は `pub(in crate::egui_shell) fn` / 素の `fn`）。ついでに直す |
| :486-492 | `activation_entry_points_consult_the_display_gate` の doc「残る死角」 | 「母集団は当該メソッドのソーステキストだけ」は存在形なので不変。**ただしアンカー探索が複数ファイルにまたがる**旨（一意性 assert）を足す |

### B. `C:/workspace/Snotra/src-tauri/src/egui_shell/launcher_controller/activation.rs`

- `//!` :3-15。**規範「起動の入口をこのファイルの外へ出してはならない（この制約の正本はここである）」を撤去する**（本 issue の狙い）。
- 「**守られ方は方向で違う。移動は赤になり、追加は沈黙する。**」の段落は**書き換えであって削除ではない**。新しい three-way は:
  1. **直下の別の子モジュールへの移動** → 母集団に含まれるので**検査は生き続ける**（＝本 issue の成果）
  2. **母集団配列に未登録のファイル（新規追加の子モジュール）への移動** → ヘッダ assert が落ちて**赤**
  3. **4 本目の入口を別モジュールへ新設** → **依然として沈黙**（`entry_points` が明示の配列であり母集団から導かれないため）。**受容する死角のまま**
- 「入口が別の子モジュールへ移ると母集団が割れる」「赤を消すために `include_str!` のパスだけ書き換えると射程が黙って狭まる」も偽になる。
- 「**どの入口が対象かの正本は散文ではなく `tests.rs` の `entry_points` 配列である**」は真のまま。

### C. `C:/workspace/Snotra/src-tauri/CLAUDE.md`

- **:41** — `**起動の入口をこのファイルの外へ出さないこと**——ソーステキスト検査の母集団がここに縛られている（制約と死角の正本は同ファイルの `//!`）` を撤去（規範 → 機構）。責務散文（「起動の入口（Enter / クリック / Shift+Enter）と dispatch・in-flight 回収・slash の即実行」）は残す。
- **:47** — `` `include_str!("../activation.rs")` が母集団であり `` を、直下の子 `*.rs` 群という記述へ更新。**ここへファイル一覧を書き写さないこと**（AGENTS.md「文書に事実の写しを増やす変更」——正本は `launcher_controller.rs` の `mod` 宣言）。
- :34 の `egui_shell/launcher_controller/activation.rs` は `launch_*_core` の再利用元としての名指しで、**変更不要**。

### D. `C:/workspace/Snotra/docs/development-principles.md`

- **:213**「**守りたい本体が `include_str!` の読む当のファイルに在る自己検査なら、ファイル全体は B の上位集合である**……（本体が別ファイルへ移りうる検査ではこの前提が外れる）」——原理としては真のままだが、**括弧内の但し書きに「解けた実例」ができる**（母集団をディレクトリへ広げれば、本体が兄弟ファイルへ移っても下界は保たれる）。1 句だけ足す（**軽微**。原理の書き換えではない）。
- :215 / :221 は `activation/tests.rs` を正本として指すだけなので**変更不要**。

### E. 変更不要と判定したもの（根拠つき）

- `C:/workspace/Snotra/docs/architecture.md:81` — 「`launcher_controller/activation.rs` の起動の入口が表示側と同じ述語を呼ぶ」。**入口は移動しない**ので位置の記述は真のまま。#1201 は母集団を広げるだけである。
- `C:/workspace/Snotra/docs/adr/ADR-source-text-probe-helper-locality.md` / `ADR-source-text-probes-not-lifted-to-types.md` / `ADR-activation-gate-placement.md` — 凍結された歴史（`ADR-adr-frozen-history`）。**編集しない**。なお helper-locality の反転条件は「次にソーステキスト検査を**新設する** issue が立ったら（＝3 つ目の**局所実装**が生まれるなら）」であり、#1201 は既存サイトの母集団を広げるだけなので**発火しない**。
- `C:/workspace/Snotra/SPEC.md` — hit 0 件（§5 参照）。
- `C:/workspace/Snotra/.claude/rules/` — `activation` / ソーステキスト検査を名指す条項は 0 件。`safety-nets.md` は**参照して従う**側であって変更対象ではない。
- `C:/workspace/Snotra/docs/superpowers/` — 名指し 0 件（すべて `indexing.rs` / `startup.rs` / `main.rs` / governance 系の凍結済み計画）。

---

## 2. 「偽になる散文」の一覧（識別子ではなく概念ラベルで検出したもの）

grep した概念ラベル: `include_str!` / `母集団` / `1 枚` / `外へ出` / `起動の入口` / `activation.rs` / `ソーステキスト` / `pub(super)` / `entry_points`。

| # | 場所 | いま書いてあること | なぜ偽になるか |
|---|---|---|---|
| P1 | `tests.rs://!`:3 | 「母集団は `activation.rs` **1 枚**である」 | 6 枚になる |
| P2 | `tests.rs://!`:4-6 | 「起動の入口が別の子モジュールへ移れば母集団は割れる」 | 直下なら割れない（本 issue の狙いそのもの） |
| P3 | `tests.rs`:255-256 | 「`include_str!` が読む `activation.rs` には字下げ 0 / 8 の `fn ` 行が 1 本も無く」 | 母集団の名前が変わる（**主張自体は 6 枚でも真**——本レビューが再測） |
| P4 | `tests.rs`:297-298 | 「現に `pub(super) ` が挟まる定義が**起動の入口に在り**」 | **今日すでに偽**（入口は `pub(in crate::egui_shell)`。`pub(super)` は `drain_launch`/`execute_slash`） |
| P5 | `tests.rs`:386-387 | 「`search_flow.rs` と `launcher_controller.rs` に在り、この検査は**もう見ていない**」 | `search_flow.rs` は母集団へ入る（帰属で落ちるへ変わる） |
| P6 | `tests.rs`:399-400 | 「母集団は production **1 枚**（`activation.rs`）で、この `mod tests` は別ファイルだからである」 | 「1 枚」が偽。保証の根拠が「別ファイル」から「**直下のみ**」へ移る |
| P7 | `tests.rs`:411-413 | 「母集団は `activation.rs` 1 枚である……そのとき直すのは母集団であって assert ではない（`activation.rs` の `//!` が正本）」 | 正本先の規範が消える＝宙に浮く |
| P8 | `tests.rs`:424-425 | 「3 本のアンカーは可視性修飾の有無で 2 形（**`pub(super) fn`** / 素の `fn`）に分かれる」 | **今日すでに偽**（`pub(in crate::egui_shell) fn` / 素の `fn`） |
| P9 | `activation.rs://!`:3-5 | 「**起動の入口をこのファイルの外へ出してはならない**（この制約の正本はここである）」 | 撤去対象そのもの |
| P10 | `activation.rs://!`:9-15 | 「移動は赤になり、追加は沈黙する」「`include_str!` のパスだけ書き換えると射程が黙って狭まる」 | 3 分岐（緑／赤／沈黙）へ変わる |
| P11 | `src-tauri/CLAUDE.md`:41 | 「起動の入口をこのファイルの外へ出さないこと——母集団がここに縛られている」 | 撤去対象 |
| P12 | `src-tauri/CLAUDE.md`:47 | 「`include_str!("../activation.rs")` が母集団であり」 | 母集団が変わる |
| P13 | `docs/development-principles.md`:213 | 「（本体が別ファイルへ移りうる検査ではこの前提が外れる）」 | 外れない構成が可能になった＝**但し書きが強すぎる**（軽微） |

---

## 3. `AGENTS.md`「条件別チェック」のトリガーと、それぞれが要求する作業

| トリガー行 | 当たる理由 | 要求される作業 |
|---|---|---|
| **ガバナンス機構自身の配置を変える（判定ファイルの移動・**母集団の切り出しの変更**・走査元の追加）** | 母集団の切り出しをまさに変える／走査元（読むファイル）を 1 → 6 へ増やす | `.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」で**測り直す**。かつ**足す前に「壊れたとき緑が緑のまま推移するか」を問う**（→ §4 の新しい沈黙がまさにこれ） |
| **セーフティネット（…rules・skills・**規範**）を新設/変更** | `activation.rs` の `//!` の規範と `src-tauri/CLAUDE.md:41` の規範を**撤去**する | `.claude/rules/safety-nets.md`（規範文書は自動配送されないので手動参照）。**ルート `CLAUDE.md` 最重要ルール 2「セーフティネットの変更は合意してから」に当たる**——issue #1201 の本文がその合意にあたる旨を PR 本文へ明記する |
| **機構・層・ファイル群を撤去する** | 規範 1 本の撤去（`git grep` で語彙の残存を数え上げ、「撤去を描写している / 在る前提で書いている」へ振り分ける） | 語彙: 「外へ出」「1 枚」「母集団がここに縛られている」「移動は赤・追加は沈黙」。**識別子の残存 0 件を根拠にしない**（本レビューの §2 が概念ラベル側の数え上げ） |
| **ガバナンス文書（`*.md`…）を変更** | `src-tauri/CLAUDE.md` を編集 | `npm run governance:check`（`docs/build-commands.md` カテゴリ F）。PR では `governance-check` job が常時実行 |
| **網羅性が要件** | 「直下の子 `*.rs` 群」の列挙が要件 | **母集団を誰が知っているか**＝`launcher_controller.rs:56-61` の `mod` 宣言（rustc が正本を持つ——`mod` の無い `.rs` はコンパイルされない）。加えて `/plan-review`「Step 2b」＝本レビュー |
| **検査・検証手段を新設する／どの手段で保証するか決める** | 規範 → 機構の転換そのもの | `docs/development-principles.md`「検証の層と、層と層の隙間」——**穴は層の内側ではなく境界に空く**（母集団の列挙と `mod` 宣言の境界が今回の穴） |
| **`Option`/フラグ/enum variant など**どの分岐が選ばれるかを決める値**の出所を変更** | 母集団という「入力」の出所が変わり、**1 行も変えていない下流が初めて走る** | 「この値で初めて走る行」を列挙する。**具体例: `search_flow.rs:87` の `self.indexing().get()` が初めて `owners_of` の走査対象になる**（帰属先は `fn run_search_with(` で入口ではないので緑）。`hide_request.rs:49` / `search_flow.rs:46,128` の `read_config(` も同様に初めて走る |
| **文書に事実の写しを増やす変更** | `include_str!` の配列は `mod` 宣言の写しである | 正本を 1 か所に。写しを消せない（`include_str!` はパスを計算できない）以上、**写しであることを機械照合で可視にする**（→ §4-(1)） |
| **関数・型を新規定義／改名／導入** | `population()` 等の helper を足すなら | 呼び出し元は LSP `findReferences`＋`/dry-check`。ただし `#[cfg(test)]` 内に閉じるので射程は小さい |
| **件数 N・上限パラメータ・導出の入力を変更** | 母集団の枚数 N が 1 → 6 | 下流全段に影響を追う（上の「値の出所」行と重なる） |

表の外だが必須:

- **`.claude/rules/comments.md`**: `///` / `//!` を大量に書き換えるので、`docs/build-commands.md` カテゴリ A の `cargo doc` 行を**手で走らせる**（intra-doc link 切れは **CI でのみ発火し PostToolUse hook は沈黙する**）。とくに `tests.rs` の doc は intra-doc link（`[`method_body`]` 等）が密である。
- **`docs/build-commands.md` カテゴリ A**（fmt / clippy / test）は PostToolUse hook が自動で撃つ。`.rs` の doc 編集は沈黙 = 合格。

---

## 4. この変更が新しく作りうる「沈黙の経路」

**先に線を引く**: #1201 が**受容する**死角は「`entry_points` が母集団から導かれない＝4 本目の入口の新設が沈黙する」。以下はそれとは**別物**で、いずれも**この変更が新しく作る**沈黙である。

### (1) 【要対処】母集団がディレクトリから導かれない — 新しい子モジュールが黙って母集団の外に落ちる

`include_str!` はパスを計算できないので、母集団は**リテラルの配列**にならざるをえない。すると:

- `launcher_controller/` に新しい子モジュール（例 `dispatch.rs`）を足し、`launcher_controller.rs` へ `mod dispatch;` を書き、**配列への追加を忘れる** → 母集団はその 1 枚を欠いたまま**全検査が緑**である。
- ここへ後から起動の入口を移すと**赤**にはなる（ヘッダ assert）。しかし**禁止語（`self.indexing()` / `read_config(`）が入口の中で使われる形はもう見えない**ので、#1201 が狙った「どの子モジュールへ移っても生き続ける」は**部分的にしか達成されない**。

AGENTS.md の問い「壊れたとき緑が緑のまま推移するか」への答えは**推移する**（気づく契機が無い）。ゆえに**機構を置くべき側**である。

**推奨する機構**: `include_str!("../../launcher_controller.rs")`（tests.rs から見て `../..` は `src/egui_shell/`）で親モジュールのソーステキストを取り、`^mod (\w+);` に相当する行を列挙して、母集団配列のファイル名集合と**完全一致**を assert する。不一致で赤になる。
- 相対パスは**実測が要る**: `activation/tests.rs` → `..` = `launcher_controller/`、`../..` = `egui_shell/`。したがって `../../launcher_controller.rs`。**書いたら必ず `cargo test` で通ることを確かめる**（AGENTS.md「判定ロジックは代表入力で実行して測る」）。
- 現在値は 6 本（`activation` / `folder_nav` / `frame_stages` / `hide_request` / `search_flow` / `updater_toast`）。**この 6 という数を doc へ書かない**（数え上げは足すたびに腐る・AGENTS.md「検証の作法」）。
- **これは新しいソーステキスト検査ではあるが、`ADR-source-text-probe-helper-locality` の反転条件（3 つ目の**局所実装**＝別サイト）には当たらない**——同じサイト（`activation/tests.rs`）に閉じ、切り出しの形も本体の切り出しではない。ただし**この判断はユーザー裁定にかける価値がある**（→ §未検証）。

### (2) 【要対処】ソースを連結して 1 本の `owners_of` に渡すと、帰属がファイル境界を越える

`owners_of` は `current: Option<&str>` を持ち越すので、連結した母集団では**前のファイルの最後のヘッダが、次のファイルの最初のヘッダまで帰属先として生き残る**。

**実測した危険度**: `activation.rs` の**最後の**字下げ 4 ヘッダは `pub(in crate::egui_shell) fn on_enter(`（:589）＝**起動の入口そのもの**。アルファベット順に連結すると次は `folder_nav.rs` で、その最初のヘッダは :38。つまり **`folder_nav.rs` の 1〜37 行目（`//!` と `use` と `impl` 宣言）に禁止語が 1 語でも現れると、`fn on_enter(` へ帰属して恒久的な赤になる**。

いま 6 枚のどのファイルにも「最初のヘッダより前の禁止語」は 0 件（実測済み・§0）。**潜在であって現在の欠陥ではない**が、`folder_nav.rs` の module doc に `` `read_config(` `` と 1 語書くだけで発火する。

**設計制約**: 母集団を `[(name, src); N]` として持ち、**ファイルごとに `owners_of` を回す**（連結しない）。帰属はファイル境界でリセットされる。

### (3) 【要対処】`method_body` の `split_once(anchor)` は連結すると「最初のファイルが勝つ」

`method_body` は `src.split_once(anchor)` で母集団の**先頭からの最初の出現**を採る。連結した母集団では:

- **ファイルの並び順が load-bearing になる**（並べ替えただけで対象が変わる）。
- 前のファイルの**doc コメント**にアンカー文字列（`fn shift_activate(` 等）が現れると、そこで split して**別ファイルの本体を母集団として切り出す**。字下げ 4 の doc 行なら冒頭の字下げ assert も通る（`method_body` の doc :36-40 が「同じ字下げの doc コメント行にアンカー文字列が先行出現した場合は通る」と自ら認めている形が、**ファイルをまたいで**効くようになる）。
- これは #1077 の「隣のメソッドを飲み込む」沈黙の**ファイル間版**である。

**実測**: 3 アンカーの逐語出現はいまそれぞれ 1 件のみ（すべて `activation.rs`）。兄弟ファイルの言及は `` `on_enter` `` の形で `fn ` を伴わない。**潜在であって現在の欠陥ではない**。

**設計制約**: アンカーを含むファイルを母集団配列から**絞り込み**、**「ちょうど 1 枚が含む」を assert**してからそのファイルへ `method_body` を当てる。0 枚なら「改名した／移した」で赤、2 枚以上なら曖昧で赤。

### (4) 【要対処】母集団を「直下」に限ることが構造的保証を担っている — doc へ明記する

`activation/tests.rs` が母集団の外に在ることは、いま**サブディレクトリに在る**ことだけが理由である。母集団の列挙を「ディレクトリを再帰的に歩く」形へ後で広げると:

- `method_body` 側: `tests.rs`:419 の `"fn on_enter(",`（字下げ 8）でアンカーが先に一致し、字下げ assert が落ちて**赤**（安全側）。
- `owners_of` 側: `tests.rs` の `fn` はすべて字下げ 0 なのでヘッダが 1 つも認識されず、**テスト側のリテラル出現は帰属先を持たず黙って捨てられる**（緑）。

つまり片側は赤・片側は緑で、**気づける保証がない**。「直下のみである」ことを、なぜそうなのか（テスト自身を母集団へ入れない）とセットで doc へ書く。

### (5) 【軽微】`method_header` の `pub(in crate::egui_shell)` 形が事実上の主要形になるのに fixture が無い

母集団 6 枚のうち 4 枚（`folder_nav` / `frame_stages` / `hide_request` / `search_flow`）が `pub(in crate::egui_shell) fn` を使い、起動の入口 2 本もこの形である。`method_header` の実装（:178-184）は `pub` → `(`…`)` の読み飛ばしでこれを通す（**production で現に通っている**）が、`method_header_accepts_visibility_and_async_before_fn` の fixture 6 本にこの形が無い。読み飛ばしを壊す変異は `activation_uses_frame_values_not_live_reads` のヘッダ assert が落として赤にするので**沈黙ではない**が、fixture 1 本の追加は安い。**先行する欠落であって #1201 が作るものではない**（入口はいまもこの形）。

### (6) 【軽微】3 検査が同じ `include_str!` 行を持つ写しが 3 → 3 のまま残る

母集団の定義が複数行になるので、写しの費用が上がる。1 つの `fn population()` へ寄せる。**ただし `owners_of` / `method_body` の 2 helper を統合しないこと**——極性が違えば要る不変条件が違う（`ADR-source-text-probe-helper-locality`、ルート `CLAUDE.md`「意図的なリファクタリングの結果を元に戻さない」）。

### (7) 【要対処・実測項目】フォールトインジェクションで測るべき 3 変異

`.claude/rules/safety-nets.md` の要求に対して、**この変更に固有の**変異は次の 3 つ:

- **(a) 既存の発火が生き残ることの確認**: `on_enter` へ `self.indexing()` を 1 行挿す → 赤。既存 doc :409 がこの変異を記録している。
- **(b) 本 issue の狙いそのものの測定**: 起動の入口 1 本（例 `shift_activate`）を**一時的に `search_flow.rs` へ移し**、そのうえで (a) の変異を入れる → **赤になること**。移しただけでは緑のままであること。**これを測らないと「規範を機構へ移した」は主張できない**——(a) だけでは母集団を広げた効果は 1 ミリも測れていない。
- **(c) §4-(1) の穴の実測**: `launcher_controller/` へ空の子モジュール（`mod` 宣言つき）を足して母集団配列を更新しない → **機構を置けば赤、置かなければ緑**。この対照の**差**が機構の証拠であって、緑になったこと自体は証拠ではない。

---

## 5. `SPEC.md` の更新要否

**不要。**

根拠:

1. `grep -n "activation\|起動の入口\|母集団\|ソーステキスト" SPEC.md` → **0 件**。SPEC.md はソーステキスト検査の存在も母集団も記述していない。
2. `AGENTS.md`「開発ワークフロー」1 の判定基準（「`SPEC.md` に当該挙動の記述があるか、その記述に**合わせる**のか**変える**のか」）に照らすと、記述が無いうえに**製品の挙動が 1 ビットも変わらない**——変わるのは `#[cfg(test)]` 配下の母集団の取り方と doc / CLAUDE.md の散文だけである。
3. 表示ゲート（§4.5 / §4.7）と起動の可否の関係は SPEC.md が持つが、**その不変条件自体は変わらない**（守り方の射程が広がるだけ）。

---

## 6. 所見の 3 分類（対象 issue #1201 との関係を各 1 行で）

### 要対処

| # | 所見 | #1201 との関係 |
|---|---|---|
| R1 | 母集団を `mod` 宣言と機械照合しないと、新しい子モジュールが黙って母集団の外に落ちる（§4-(1)） | 本 issue が新しく作る沈黙。「壊れたとき緑が緑のまま推移する」に該当し、AGENTS.md の判断規準では機構を置く側 |
| R2 | ソースを連結して `owners_of` を 1 回回す実装は、帰属がファイル境界を越えて恒久的な偽陽性を作りうる。`activation.rs` の最後のヘッダが `fn on_enter(` である以上、隣接ファイルの module doc 1 行で発火する（§4-(2)） | 本 issue の実装方式に対する設計制約。ファイル単位で回せば消える |
| R3 | `method_body` の `split_once` は連結すると並び順依存になり、doc コメント経由のアンカー横取りが**ファイルをまたいで**効く。「ちょうど 1 枚が含む」assert が要る（§4-(3)） | 同上。#1077 の沈黙のファイル間版を新しく開く |
| R4 | 母集団が「**直下のみ**」であることが `activation/tests.rs` を母集団外に保つ唯一の根拠になる。理由とセットで doc へ書く（§4-(4)） | 本 issue が構造的保証の載り先を「別ファイル」から「直下のみ」へ移す |
| R5 | フォールトインジェクションは (a) 既存発火の生存 だけでなく **(b) 入口を兄弟ファイルへ移した状態での発火** を必ず測る（§4-(7)） | (b) を測らないと本 issue の主目的（規範 → 機構）が未検証のまま緑になる |
| R6 | 偽になる散文 P1〜P12 を漏れなく直す。とくに P7（撤去される `//!` を正本として指す行）と P5（`search_flow.rs` が母集団へ入る） | 撤去トリガー（AGENTS.md「機構・層・ファイル群を撤去する」）の数え上げ対象 |

### 軽微

| # | 所見 | #1201 との関係 |
|---|---|---|
| M1 | `tests.rs`:297-298 と :424-425 の `pub(super)` の名指しは**今日すでに偽**（起動の入口は `pub(in crate::egui_shell)`） | 先行する腐り。同じ doc を触る差分なのでついでに直すのが安い |
| M2 | `method_header_accepts_visibility_and_async_before_fn` に `pub(in crate::egui_shell)` の fixture が無い（§4-(5)） | 母集団拡大で 4 枚がこの形になり load-bearing さが増す。沈黙ではない（ヘッダ assert が赤にする） |
| M3 | 3 検査が持つ `include_str!` 行の写しを `population()` へ寄せる（§4-(6)）。ただし `owners_of` / `method_body` は統合しない | 母集団定義が複数行になるので写しの費用が上がる |
| M4 | `docs/development-principles.md`:213 の括弧内但し書き「本体が別ファイルへ移りうる検査ではこの前提が外れる」に、解けた実例がある旨を 1 句足す | 原理は真のまま。書き換えではなく追記 |
| M5 | `tests.rs`:255-260 の「字下げ 0 / 8 の `fn ` 行が 1 本も無い」は**母集団 6 枚でも成立する**（本レビューが再測） | 文面の母集団名と再測日だけ更新すればよい。fixture の追加は不要 |

### 未検証

| # | 所見 | #1201 との関係 |
|---|---|---|
| U1 | `scripts/` 配下（`scripts/governance/` の判定を含む）にこの母集団・`activation.rs` を名指す記述が無いかを**見ていない**——走査範囲の指定で除外されているため | AGENTS.md「機構・層・ファイル群を撤去する」は `scripts/` を含む生きた層すべてを母集団と定める。実装側で `git grep` が要る |
| U2 | PR 本文（squash で main の commit message になるがファイル grep には入らない）は数え上げの母集団に含めていない | AGENTS.md「文書に事実の写しを増やす変更」が明示的に要求する母集団 |
| U3 | §4-(1) の `mod` 宣言スキャナが `ADR-source-text-probe-helper-locality` の反転条件（3 つ目の局所実装）に当たるかは**裁定が要る**。私の読みでは同一サイト内であり当たらない | 機構を足す判断そのものなので、ルート `CLAUDE.md`「セーフティネットの変更は合意してから」の対象 |
| U4 | `include_str!("../../launcher_controller.rs")` の相対解決を**実際にコンパイルして確かめていない**（パスから導出しただけ） | AGENTS.md「判定ロジックは代表入力で実行して測る」——実装時に `cargo test` で接地すること |
| U5 | 「直下の子 `*.rs` 群」がいま 6 枚であることは実測したが、**実装時点でも 6 枚である保証はない**（別の作業が子モジュールを足しうる） | R1 の機構を置けばこの不確かさは構造的に消える |

