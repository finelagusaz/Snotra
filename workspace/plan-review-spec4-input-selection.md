# 独立導出: SPEC §4 へ「文字入力は選択を 1 行目へ戻す」を as-built で明文化する（#921）

対象 issue: **#921**（`SPEC §4 に「打鍵は選択を 1 行目へ戻す」の記述が無い（#838 で §6.3 だけに書いた非対称）`）
ブランチ: `docs/spec-4-typing-selection-resets`（比較対象 `main` = `d707ce7`）
実施日: 2026-08-04
本書は**導出のみ**である（`AGENTS.md`「分析・調査・助言を求められたら、調査結果のみを報告する」に従い、`SPEC.md` も `.rs` も編集していない）。

与件（人間の裁定）:

1. `SPEC.md` §4 の末尾へ **§4.9「入力と選択」を新設**し、§4.8 の ↑↓ の 1 行（`SPEC.md:208`）もそこへ移す
2. §6.3 の「絞り込みの打鍵は選択を 1 行目へ戻す（as-built）」（`SPEC.md:253`）は**削除**し §4.9 へ一本化する

---

## 1. 変更が必要なファイルとシンボルの一覧

| # | ファイル:行 | シンボル / 見出し | 変更 | 分類 |
|---|---|---|---|---|
| 1 | `SPEC.md:198`〜`208` | `### 4.8 マウス操作` | `:208`（↑↓ の 1 行）を §4.9 へ移す。`:207` は残す（§5 参照） | 要対処 |
| 2 | `SPEC.md:208` の直後（`## 5. 履歴・優先度システム`（`SPEC.md:210`）の前） | `### 4.9 入力と選択`（新設） | 新設。移設した ↑↓ 行 + 新規の「文字入力は選択を 1 行目へ戻す」 | 要対処 |
| 3 | `SPEC.md:253` | §6.3 の `- 絞り込みの打鍵は選択を 1 行目へ戻す（as-built）` | 削除（§4.9 へ一本化。ただし §2.3 の作文制約を満たすこと） | 要対処 |
| 4 | `src-tauri/src/egui_shell/view.rs:217` | `read_pre_widget_input`（コメント） | `SPEC §4.8` の参照先が §4.9 へ移るため、参照を **§4.9 へ張り替える** | 要対処 |
| 5 | `src-tauri/src/egui_shell/search_state.rs:884` | テストブロック見出しコメント `---- フォルダ内の絞り込みと選択（#838・SPEC §6.3 の as-built）----` | `§6.3` の当該行が消えるため参照を張り替える（→ §4.9） | 要対処 |
| 6 | `src-tauri/src/egui_shell/search_state.rs:893` | `folder_filter_typing_resets_selection_to_first_row` の doc `/// …（SPEC §6.3・絞り込みの打鍵と選択）` | 同上（→ §4.9） | 要対処 |
| 7 | `src-tauri/src/egui_shell/search_state.rs:898` | 同テストの doc `/// **§6.3 のもう 1 つの as-built（列挙失敗のエラー行が…）` | **「もう 1 つ」が偽になる**（§6.3 の as-built 行は 2 本から 1 本へ減る）。文言の見直しが要る | 要対処 |
| 8 | `src-tauri/src/egui_shell/launcher_controller.rs:1230` | `on_input_changed` のコメント `// SolidJS parity: 毎打鍵 selected=0（M1 gap 是正）` | **任意**。§4.9 への参照を足すと腐り検知が効く（現状 SPEC 参照が無い） | 軽微 |

### 1.1 触ってはいけない参照（同じ節番号を引くが別の事実を指す）

| ファイル:行 | 引いている節 | 指している事実 | 判定 |
|---|---|---|---|
| `src-tauri/src/egui_shell/launcher_controller.rs:56` | §4.8 | 「通常起動（tools 先頭 → tool 起動 / 無ければ item 起動）」 | §4.8 に残る（クリック起動） |
| `src-tauri/src/egui_shell/launcher_controller.rs:211` | §4.8 | 「シングルクリック / Enter で index 行を起動」 | 同上 |
| `src-tauri/src/egui_shell/launcher_controller.rs:519` | §19.6/§4.8 | Enter/クリックの単一 dispatch | 同上 |
| `src-tauri/src/egui_shell/results_view.rs:279` | §4.8 | 「ダブルクリックは扱わない（double-click=選択は as-built で到達不能）」 | §4.8 の `SPEC.md:205-206` に残る |
| `docs/adr/ADR-stale-identifier-detector-scope.md:53` | §4.8 | 「ダブルクリックは独立した挙動を持たない（as-built）」 | 同上。加えて ADR は凍結された歴史（`ADR-adr-frozen-history`）ゆえ触らない |
| `snotra-core/src/folder.rs:191` / `:742` | §6.3 | 「フォルダ内の絞り込みは表示名のみ（フルパス非対象）」 | §6.3 の `SPEC.md:250` に残る |
| `SPEC.md:239`（§6.1） | §6.3 | 「フォルダ内の絞り込み文字列（§6.3）はクリアされる」 | §6.3 は節として残るので参照は生きる。触らない |
| `SPEC.md:207` | — | 「キーボードナビゲーション（Arrow ↑↓）とマウス操作は互いに干渉しない」 | §5 参照。**移さない** |
| `docs/superpowers/**` / `.superpowers/**` の §4.8・§6.3 参照（計 10 件超） | §4.8 / §6.3 | 歴史資料（#589 で非規範化・`governanceDocs` の走査対象外） | 触らない |

### 1.2 検出器がこの張り替えを守らないこと（実測）

`scripts/governance-check.mjs:1111-1119` の `governanceDocs()` が `G-spec-sections` の走査母集団であり、その述語は
`CLAUDE.md` / `AGENTS.md` / `CONTRIBUTING.md` / `SPEC.md` / `docs/*.md`（`superpowers`・`adr` を除く）/
モジュール `CLAUDE.md` / `.claude/rules/*.md` / `.claude/skills/*/SKILL.md` のみで、**`.rs` を 1 件も含まない**。
ゆえに上表 #4〜#7（すべて `.rs` のコメント）は `npm run governance:check` でも PR CI の `governance-check` job でも
**沈黙する**。この一覧が唯一の捕捉手段である。

なお `G-spec-sections`（`scripts/governance-check.mjs:215-259`）は子セクション番号の連続性も見るため、
`### 4.8` の次を `### 4.9` にする限り新設自体は緑である（`4.9` は現在どこにも存在しない — 全 `*.md` / `*.rs` grep で 0 件）。

### 1.3 概念ラベルによる `*.rs` 全走査（節番号を持たない散文の取りこぼし対策）

§1.2 のとおり `.rs` は検出器の視界の外にあり、この一覧の完全性が唯一の担保になる。上表は
`§4.8` / `§6.3` / `1 行目|先頭へ戻` という**節番号・定型句起点**の grep で得たため、節番号を持たない散文を
取りこぼしうる。概念ラベルで別途走査した:

```
grep -rn "絞り込み\|打鍵" --include=*.rs . | grep -v "^./target/"   → 31 件
```

**新規の要対処は 0 件。** 内訳:

- 既に §1 表へ列挙済み: `view.rs:215`（=`:217` と同一コメントブロック）、`search_state.rs:884 / 889 / 893 / 898`、`launcher_controller.rs:1230`
- **新規の軽微 2 件**（節番号を持たず §4.9 の事実を散文で述べている・§5「軽微」6 へ）:
  `search_state.rs:206`（`reset_selection` の doc「driver が打鍵（changed エッジ）ごとに呼ぶ」）と
  `search_state.rs:1006-1007`（`reset_selection_returns_to_top` の冒頭コメント「毎打鍵 setSelected(0)」）。
  どちらも**偽にはならない**（挙動を述べており §6.3 も §4.8 も指していない）ため必須ではないが、
  §4.9 への参照を足せば腐り検知が増える
- 残り（`engine.rs:159`・`folder.rs:143 / 190`・`launcher_controller.rs:113 / 741 / 743 / 802 / 1036 / 1232 / 1283`・
  `layout.rs:234`・`results_view.rs:28 / 36 / 527`・`search_state.rs:177 / 301 / 857 / 908 / 910`・
  `view.rs:90 / 621 / 958`）は主題が別（debounce・フィルタキャッシュ・`rows_generation`・§6.1 の左右ナビ・
  launching 中の抑止）であり、本変更で偽にならない。とくに `search_state.rs:910` の assert 文言
  「絞り込みの打鍵で選択は 1 行目へ戻る」は §6.3 から削除する文と同じ言い回しだが、**folder 経路の事実を
  述べる assert メッセージであって SPEC への参照ではない**——真のまま残る（触らない）

---

## 2. 受け入れ条件 1 の裏取り（実装上どこで起きるか）

### 2.1 成立する経路（すべて `changed()` エッジ 1 本に集約される）

打鍵が選択へ届く経路は `src-tauri/src/egui_shell/view.rs:632-634` の 1 本だけである。

```
if response.changed() {
    self.controller.on_input_changed(buf, in_folder, &ctx);
}
```

`on_input_changed`（`src-tauri/src/egui_shell/launcher_controller.rs:1203`）は `in_folder` で 2 分岐する。

| モード | 経路 | 選択リセットの実体 | 根拠 |
|---|---|---|---|
| 通常検索（`ViewKind::Results`・`QueryIntent::Plain`） | else 分岐 | `self.state.reset_selection();` | `launcher_controller.rs:1230` |
| インスタントコマンド（`QueryIntent::Instant`） | else 分岐（同上） | 同上（`reset_selection()` は `interp()` の**前**にあり、3 intent すべてに掛かる） | `launcher_controller.rs:1228-1230` と 1232 の `match` 順序 |
| スラッシュコマンド（`QueryIntent::Command`・`/r` 等） | else 分岐（同上） | 同上 | 同上 |
| フォルダ展開中の絞り込み（`ViewKind::Folder`） | if 分岐 | `set_folder_filter()` 内の `self.selected = 0;` | `search_state.rs:246-249` |

**`reset_selection()` の production 呼び出し点は `launcher_controller.rs:1230` の 1 か所だけである。**
リポジトリ全体（`--include=*.rs`、`target/` 除外、`snotra-settings/` を含む）で 6 件ヒットし、内訳は
定義 `search_state.rs:208` / doc 言及 `search_state.rs:156` / テスト内 `search_state.rs:670, 1005, 1012`
（`mod tests` は `search_state.rs:482` から）/ production 1 件（`launcher_controller.rs:1230`）。
`snotra-settings/src/` は `reset_selection` も `selected = 0` も 0 件。

### 2.2 成立しない場面（2 つ・どちらも「打鍵が届かない」形）

1. **ツール選択中**（§18.5）: `view.rs:472` の `let input_editable = !in_tool && !self.controller.is_launching();` が
   `TextEdit::interactive(input_editable)` へ渡り（`view.rs:623`）、非対話の TextEdit は `changed()` を返さない
   （`view.rs:619-622` のコメント「interactive(false)（通常描画のまま読み取り専用・changed 不発火）」）。
   SPEC 側の既存記述は `SPEC.md:800`（§18.5「ツール選択中の入力は無効化（検索結果が上書きされない）」）。
2. **起動 in-flight（launching）中**: 同じ `input_editable` の第 2 項（`is_launching()` は `launcher_controller.rs:180-182`）。
   SPEC 側の既存記述は `SPEC.md:1051-1052`（§19.6「打鍵は入力欄の無効化で抑止する」）と `SPEC.md:541-547`（§8.6）。

**どちらも §4.9 へ書いてはならない**（受け入れ条件 4 の射程は §18.5 だけだが、launching 側も同型の二重化になる）。

### 2.3 作文上の制約（実装者へ渡す4点・すべて in-repo 由来）

1. **§6.3 の行を削除する以上、§4.9 の文言は folder 内の絞り込みも覆っていなければならない。**
   §4.9 は「§4 検索システム」の下にあるため、「通常検索では…」と限定して書くと `SPEC.md:253` の削除が
   「移設」でなく**事実の消失**になり、受け入れ条件 1（§4 が実装と一致）と 2（二重に書かない）が衝突する。
   採り得るのは (a) §4.9 を両モードに掛かる文言で書く、(b) §6.3 に本文でなく §4.9 への参照 1 行を残す、のいずれか。
   これは §1 表の #5/#6（`search_state.rs` の folder テストが §6.3 を指している）と**同じ欠陥を反対側から見たもの**である。
   なお **(b) を採る場合、その参照行は `SPEC.md` 内にあるため 2 つの検出器の母集団に入る**——
   `G-spec-sections`（`§4.9` が実在するか）と `G-heading-refs`（`headingRefDocs` は `*.md` 全般が母集団）である。
   `SPEC.md` は裸の `（§6.3）`（`:239`）と `` `SPEC.md`「6.7 フォルダ展開中の現在地表示」``（`:193` / `:279`）の
   両形式を混在させているが、**「…」形式で書くなら見出し文字列は `### 4.9 入力と選択` と逐語一致させること**
   （新設ゆえ既存の照合実績が無く、誤字はレビューではなく CI で落ちる）。
2. **「打鍵」と無限定に書かない。** この SPEC で「打鍵」は左右カーソルキーを含む語であり（§6.1・`SPEC.md:235-236`）、
   #920 のコミットメッセージ（`d707ce7`）が「『打鍵しても』ではなく『文字を入力しても』と書く」と明示的に選択している。
3. **全称を前提条件なしで書かない**（`AGENTS.md`「検証の作法」）。0 件のときは選択の指す行が無い
   （`SPEC.md:237` が §6.1 で同じ限定を既に置いている）。
4. **移設する ↑↓ 行との並びを意図して書く。** `SPEC.md:208` は「↑↓ で選択を変えた直後の打鍵は**直前の編集位置**へ入る」
   と述べ、新規行は「その打鍵で選択は 1 行目へ戻る」と述べる。両立する（キャレット位置と選択行は別物）が、
   隣接させると読者は矛盾を疑う。§4.9 の中で順序と接続を決めること。

---

## 3. 受け入れ条件 3 の裏取り（挙動が変わりうる箇所）

**無い。**

根拠:

- `SPEC.md` は実行コードではない（`AGENTS.md` 3層分担の第1層）。
- `.rs` 側の変更（§1 表 #4〜#8）は**すべてコメント・doc コメントの文字列**であり、`view.rs:217` は
  `read_pre_widget_input` 内の行コメント、`search_state.rs:884` はテストモジュール内のブロックコメント、
  `search_state.rs:893/898` は `#[test] fn folder_filter_typing_resets_selection_to_first_row` の doc、
  `launcher_controller.rs:1230` は行末コメントである。式・分岐・定数のいずれも触らない。
- 挙動を担う 2 つの代入（`launcher_controller.rs:1230` の `reset_selection()` 呼び出しと
  `search_state.rs:248` の `self.selected = 0;`）はどちらも本作業の対象外である。

実装者への実務注意: `.rs` のコメント編集でも PostToolUse hook は発火する（`docs/hooks.md`）。
コメントを折り返し直す場合は `cargo fmt --check` が緑のままであること。

---

## 4. テストの要否

### 4.1 既存テストが固定している範囲（実測）

| テスト | 場所 | 固定している事実 | §4.9 の事実を固定するか |
|---|---|---|---|
| `reset_selection_returns_to_top` | `search_state.rs:1005` | `reset_selection()` という**プリミティブ単体**が `selected` を 0 にすること | ✗（配線を見ない） |
| `folder_filter_typing_resets_selection_to_first_row` | `search_state.rs:903` | `set_folder_filter()` が folder 中に `selected` を 0 にすること（#920 で追加） | 部分的（folder 側のみ・かつ状態核だけ） |
| `rows_generation_is_stable_on_selection_change` | `search_state.rs:665` | 選択移動で `rows_generation` を進めないこと | ✗（別主題） |

**§4.9 が述べる事実の本体は配線側（`on_input_changed` の else 分岐 → `reset_selection()`）にあり、これを固定するテストは存在しない。**
`src-tauri/src/egui_shell/launcher_controller.rs` に `mod tests` / `#[cfg(test)]` / `#[test]` は 0 件（grep 実測）。
`search_state.rs:888-891` のコメント自身が「射程は状態核だけである——打鍵から `set_folder_filter` への配線
（`view.rs` の `changed()` エッジと `launcher_controller.rs` の `on_input_changed`）は射程外」と明言している。

### 4.2 判定: **新規テストは不要（#920 の先例に従う）**

理由:

- 配線（`on_input_changed`）のテストには `egui::Context` と `AppHandle` が要り、#920 が §6.3 のもう 1 点について
  同じ理由で固定を断念し「腐り検知は `launcher_controller.rs` の既存コメントに委ねる」と受容している（`d707ce7` のコミットメッセージ）。
- 状態核側は `reset_selection_returns_to_top` が既に固定しており、追加しても同じ主張の二重化になる。
- **#920 式の変異実験（呼び出しを消して赤くなるテストを数える）はこの箇所では成立しない可能性がある**:
  `launcher_controller.rs:1230` は `reset_selection()` の唯一の production 呼び出し点であり、
  消すと `-D warnings` 下で `dead_code` によりビルドが落ちうる（テストの赤ではなくコンパイルエラーになる）。
  実験するなら「テストは緑のままビルドが落ちる」を期待値とすること。

**推奨する代替**: §1 表 #8（`launcher_controller.rs:1230` のコメントへ `SPEC §4.9` を書き足す）。
テストを持てない事実の腐り検知をコメントに委ねるのは #920 が採った同じ手であり、参照を張っておけば
次に §4.9 を動かす者の grep に掛かる。

---

## 5. 3 分類のまとめ

### 要対処

1. `SPEC.md:208`（§4.8 の ↑↓ 行）を新設 §4.9 へ移す
2. `SPEC.md:210` の直前へ `### 4.9 入力と選択` を新設し、「文字入力のたびに選択は 1 行目へ戻る」を as-built で書く
3. `SPEC.md:253`（§6.3 の該当行）を削除する。**ただし §2.3-1 の制約を満たすこと**——§4.9 の文言が folder 内の絞り込みを
   覆っていないなら、削除は「移設」でなく事実の消失になり受け入れ条件 1 と 2 が衝突する
4. `src-tauri/src/egui_shell/view.rs:217` の `SPEC §4.8` → `§4.9`（この行が引いている「単一行入力欄で ↑↓ に
   キャレット移動の用途は無い」は、まさに移設される `SPEC.md:208` である）
5. `src-tauri/src/egui_shell/search_state.rs:884` の `SPEC §6.3 の as-built` → §4.9 参照へ張り替え
6. `src-tauri/src/egui_shell/search_state.rs:893` の `（SPEC §6.3・絞り込みの打鍵と選択）` → §4.9 参照へ張り替え
7. `src-tauri/src/egui_shell/search_state.rs:898` の `**§6.3 のもう 1 つの as-built…**` は「もう 1 つ」が偽になる
   （§6.3 の as-built 行が 2 本 → 1 本）。文言を見直す
8. **上記 4〜7 はどの検出器も見ない**（`governanceDocs` は `.rs` を含まない・§1.2 実測）。この一覧が唯一の捕捉手段である

### 軽微

1. `src-tauri/src/egui_shell/launcher_controller.rs:1230` のコメントへ `SPEC §4.9` の参照を足す（任意・腐り検知の追加）
2. `SPEC.md:207`「キーボードナビゲーション（Arrow ↑↓）とマウス操作は互いに干渉しない」は §4.8 に**残す**——
   人間の裁定は `:208` の 1 行のみを名指しており、`:207` はマウスとキーボードの境界の話ゆえ「マウス操作」節に馴染む
   （検討したうえで移さない、と記録する）
3. launching 中も打鍵が届かない（`view.rs:472` の `!is_launching()`）が、これは `SPEC.md:1051-1052`（§19.6）と
   `SPEC.md:541-547`（§8.6）に既記述である。受け入れ条件 4 の §18.5 と同じ二重化の危険があるため §4.9 へ書かない
4. `docs/architecture.md` に §4.8 / §6.3 / 選択リセットへの参照は無い（`SPEC §4` の言及は `:82` の §4.7/§4.5 のみで
   本件と無関係）。`docs/` 配下の他の生きた文書にも該当参照は無い
5. `docs/superpowers/` と `.superpowers/` の §4.8 / §6.3 参照（10 件超）は歴史資料で `governanceDocs` の対象外。触らない
6. 概念ラベル走査（§1.3）で見つかった節番号なしの散文 2 件——`src-tauri/src/egui_shell/search_state.rs:206`
   （`reset_selection` の doc）と同 `:1006-1007`（`reset_selection_returns_to_top` の冒頭コメント）——は
   §4.9 の事実を述べているが偽にはならない。§4.9 への参照を足すのは任意（腐り検知の追加）
7. §6.3 に §4.9 への参照を残す案（§2.3-1 の (b)）を採る場合、「…」形式なら見出し文字列を
   `### 4.9 入力と選択` と逐語一致させること（`G-heading-refs` が照合する）

### 未検証

1. **`npm run governance:check` を本作業の差分に対して実行していない**（本タスクは導出のみで編集を行っていないため）。
   §1.2 は `scripts/governance-check.mjs` の**読み取り**による導出であり、実行結果ではない。実装後に走らせること
   （`docs/build-commands.md` カテゴリ F）
2. **実機での挙動確認を行っていない**。「通常検索で打鍵すると選択が 1 行目へ戻る」はコード読解（`view.rs:632` →
   `launcher_controller.rs:1230`）と #921 本文の一次証拠に基づく導出で、本タスクではアプリを起動していない
3. **`-D warnings` 下で `reset_selection()` の呼び出しを消したときに `dead_code` で落ちるか**は未実測（§4.2 の
   「落ちうる」は `pub fn` に対する `dead_code` の挙動からの推論）。変異実験を行うなら実測すること
4. **§4.9 の具体的な文面を書いていない**（§2.3 に制約を列挙するに留めた）。文面の妥当性——とくに folder 内の
   絞り込みまで覆えているか——は起草後に §2.3-1 で再検算が要る
