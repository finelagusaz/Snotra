# 調査 — #1108 `indexing.rs` の検知器は、終端の錨が消えると沈黙して緑になる

## issue の要約

`src-tauri/src/indexing.rs` の `start_index_build_invalidates_the_icon_cache` はソーステキスト検査（`include_str!` + 母集団の切り出し + `contains`）である。母集団の終端を `after.find("\npub(crate) fn ")` で探し、**見つからなければ `after`（EOF まで）を母集団にする**。EOF まで伸びると、探している 2 つの文字列は同じファイルの `mod tests` に assert のリテラルとして実在するため、**この検査は必ず緑になる**。

issue の提案は 3 点。(1) 行の走査を `str::lines` にする (2) **終端が実際に見つかったことを assert する** (3) 切り出しの helper を LF / CRLF 両方の fixture で測るテストを添える。先例は `docs/development-principles.md`「検証の層と、層と層の隙間」、実装例は `launcher_controller.rs` の `method_body`。

issue 自身が「セーフティネットの変更にあたるため、着手前に合意を取ること」と明記している（ルート `CLAUDE.md`「最重要ルール」）。本計画では Step 5c の人間承認がその合意に当たる。

## 一次証拠（このサイクルで実測したもの）

### 終端の錨は 1 つしかない

`src-tauri/src/indexing.rs` のトップレベル定義（列 0 の `fn` 宣言）:

```
22:pub fn start_index_build(app: &AppHandle) -> bool {
114:pub(crate) fn build_index_from_material(
133:fn drain_index(app_handle: &AppHandle) {
165:fn notify_indexing_started(app: &AppHandle) {
176:fn notify_indexing_complete(app: &AppHandle) {
187:#[cfg(test)]
```

`\npub(crate) fn ` に一致するのは **114 行の `build_index_from_material` ただ 1 つ**である。改名・前方への移動・可視性の変更・撤去のいずれでも母集団は EOF まで伸びる。issue の主張は真。

### `start_index_build` の本体に、列 0 の `}` は終端の 1 つしか現れない

22〜103 行を読み、中間の `}` はすべて字下げされていることを確認した（103 行が関数の閉じ）。ゆえに **`lines()` で `line == "}"` を探す形が終端として成立する**——他の関数の位置・名前・可視性に依存しない。

### 改行コードの非対称は現に存在する

- 手元: `git config core.autocrlf` = `input`（LF で checkout）
- `git check-attr text eol -- src-tauri/src/indexing.rs` = ともに `unspecified`（`.gitattributes` の射程外）→ CI（git-for-windows の system 既定 `core.autocrlf=true`）では **CRLF で checkout される**
- 現行の needle `"\npub(crate) fn "` は末尾に改行を持たないため CRLF でも `\n` 側が一致する（issue の「たまたま CRLF 安全」は真）。**新しい終端を `"\n}\n"` のような部分文字列で書くと、この安全がその場で失われる**

### #1077 の事故は同一ファイル内の写しだった（crate 横断ではない）

`git log -S "fn method_body"` = `3cf75a15`（PR #1107、#1077 サイクル）。同 PR の変更ファイルに `indexing.rs` は無い。壊れた検知器 2 本はいずれも `launcher_controller.rs` の中で、切り出しを検査ごとに書き写した結果である。**`docs/development-principles.md` 183 行「ゆえに切り出しの helper は 1 つに閉じる」の実証は、ファイル内の写しについてのものである**（crate 横断の写しが事故を起こした実証は無い）。

ただし `launcher_controller.rs:1589` の doc は自らの着想を「`indexing.rs` の `start_index_build_invalidates_the_icon_cache` と同じ形」と引用しており、**形が crate 横断で伝播した痕跡はある**（切り出しコードの写しではなく、「テスト席が要らない検査」という着想の引用）。

## 現況 — ソーステキスト検査の全 4 サイト（母集団は `include_str!` の grep）

| サイト | 切り出し | 終端の assert | canary | 広すぎる方向で沈黙するか |
|---|---|---|---|---|
| `indexing.rs:202` `start_index_build_invalidates_the_icon_cache` | `find("\npub(crate) fn ")`・**None なら EOF** | **無い** | あり | **する（本 issue）** |
| `launcher_controller.rs:1489` `method_body`（2 検査が共有） | `lines()` で `line == "    }"` | **あり**（`terminated`） | あり | しない |
| `startup.rs:592` `count_matches_the_enum_declaration` | `split_once("enum Phase {")` → `split_once('}')` | 両方 `expect` | 実質あり（0 件なら `COUNT` と不一致で赤） | しない |
| `view.rs:1374` `assert_read_once_in_production`（2 検査が共有） | `split_once("#[cfg(test)]")` の**前**を取る | `expect` | あり（`fn update(`） | しない（下記の残余を除く） |

`include_str!` の grep は 5 件で、`launcher_controller.rs` の 2 件が `method_body` を共有するため実装は 4 つである。**この母集団は `include_str!` / `fs::read_to_string` / `include_bytes!` の直接呼び出しと `*.psm1` の自己ソース読みを綴りで探した結果である**（3b が追加で走査し、自己ソース検査は 0 件を追認した）。**変数や定数を介して `env!("CARGO_MANIFEST_DIR")` からパスを組み立てる形は探していない**——「これで全部」とは書けない。

**全称では書かない。** 上の表は「現在のソースに対して」の判定であり、次の残余がある。

- `view.rs`: production 側に `#[cfg(test)]` という**文字列**が現れ、かつそれが `fn update(` より後・2 個目の読みより前に位置した場合、母集団が早期に切れて `assert_eq!(reads, 1)` が沈黙で緑になりうる。**現時点で production 側（1〜1317 行）の出現は 0 件**（`cfg(test)` の 3 件はすべて 1318 行以降）ゆえ現況の欠陥ではない
- `startup.rs`: `enum Phase` の variant が構造体形式（`Variant { .. },`）を持てば `split_once('}')` が宣言の途中で切れるが、その場合 `declared` は過小になり `COUNT` と不一致＝**赤**（安全側）

## 同族だが同型ではないもの（`scripts/` 側）

- `scripts/governance/checks/G-check-skill-enumeration.mjs:41` `sectionOf` — `end < 0 ? rest.length : end` で**終端が見つからなければ EOF まで**。形は同型だが、**Markdown の「最終節」ではそれが正しい意味論**である。壊れ方の向きも違う: 母集団が下流の節まで伸びると `/…-check` 参照が増え、「表に在るが 4a に無い」を**誤報する**（騒がしい向き）。ただし逆向き（4a に在るが表に無い）の欠落を伸びた母集団がマスクする筋は残る
- `scripts/governance/checks/G-module-linkage.mjs:127` — 閉じない raw string を EOF まで空白化する（`end < 0 ? n`）。孤児 `mod` を見落とす沈黙方向だが、閉じない raw string はそもそもコンパイルできない
- 判断: **この計画の射程に入れない。** 同型ではなく同族であり、裁定には別の一次証拠（Markdown 節構造の母集団）が要る。follow-up issue を切るかは人間の判断へ回す

## 同型の open issue はあるか

open 33 件の**タイトル**を走査した（`gh issue list --state open --limit 60`）。**その母集団の中に、#1108 と同型（母集団を持つ検査が広すぎる方向で沈黙する）のものは無い。**本文までは読んでいないため「タイトルに現れない同型」は掃えていない。 近いのは次で、いずれも別の型である。

- **#1089** `WALK_EXCLUDE_NAMES` の名前一致・全階層が生成物名のディレクトリ配下を沈黙で母集団から落とす — **狭すぎる**方向。除外規則が母集団を削る形で、終端の取り逃しではない
- **#1040** `race:boundaries` の検出パターンに RwLock 系が無い — 検出パターン（needle）側の欠落であり、母集団の切り出しではない
- **#1098** `evidence` が undefined を印字して exit 0 になる沈黙経路 — 沈黙経路だが検査の母集団ではない
- **#1028** 起動の終端の一度きり性に検知器を置く / **#926** SPEC §4.9 通常検索側に検知手段が無い / **#1014** 「start が走査より前」を CI で固定できるか — いずれも**検知器の不在**であり、既存検知器の壊れ方ではない

## 関連ファイル・シンボル（すべて grep で実在を確認済み）

- `src-tauri/src/indexing.rs` — `start_index_build`（22 行）・`build_index_from_material`（114 行）・`mod tests`（187 行）・`start_index_build_invalidates_the_icon_cache`（202 行）
- `src-tauri/src/egui_shell/launcher_controller.rs` — `method_body`（1489 行）・`method_body_is_line_ending_agnostic`（1522 行）
- `src-tauri/src/egui_shell/view.rs` — `assert_read_once_in_production`（1374 行）
- `src-tauri/src/startup.rs` — `count_matches_the_enum_declaration`（592 行）
- `docs/development-principles.md`「検証の層と、層と層の隙間」174〜183 行（母集団の 2 方向の壊れ方・CRLF・「helper は 1 つに閉じる」）
- `.claude/rules/safety-nets.md`（`paths` に `*.rs` は無く、**この issue の作業では自動配送されない**ため手動で読んだ）

## 技術的制約

- **`src-tauri` は `[lib]` を持たない**（`src-tauri/CLAUDE.md`）。`cargo test -p snotra --lib` は常に失敗する。テストは `cargo test -p snotra`、絞り込みはテスト名フィルタ
- **`-D warnings` 下で未使用の新 API は `dead_code` で落ちる**（`AGENTS.md` 条件別チェック表）。helper を新設するなら呼び出し点の移行と 1 コミットに束ねる
- `mod` 宣言と `CLAUDE.md` のモジュール索引は**別々に機構が見る**（`G-module-linkage`）。新ファイルを足すなら両方が要る
- 変更後の検証は `docs/build-commands.md` カテゴリ A（fmt / check / clippy / test / doc）と F（`governance:check`。`.md` を触る場合）

## 敵対的調査（3b）の所見と採否

出力は `workspace/adversarial-1108.txt`。命題 5 つのうち **1 つが壊れ、4 つは壊れなかった**。⚠️ が 4 件。

### 壊せた項目 — 命題 3「本体に列 0 の `}` は終端 1 つしか現れない」

**採る。** 3b は raw string で列 0 の `}` を書いた関数を構成し、`rustc` でコンパイルが通ることを実測した。「終端が見つかったこと」は「**正しい**終端が見つかったこと」を意味しない。

**ただし機序は自分で測り直した**（所見は採るが、添えられた説明は独立に誤りうる・#1056）。3b は severity を「両方の `contains` が落ちて大声で失敗する」と限定したが、**それは canary と対象の位置関係に依る**。scratchpad の `narrow_probe.rs` を `rustc -O` でコンパイルして実測した結果:

| ケース | 構成 | 判定 |
|---|---|---|
| A 否定形 assert（`method_body` を写した形） | canary の後・`self.indexing()` の前で切断 | **緑（沈黙）** |
| B 存在形 assert（`indexing.rs` の修正案） | canary の後・`invalidate_icon_cache(` の前で切断 | **赤** |
| C | canary より前で切断 | **赤**（canary assert が発火） |

**帰結は 2 つ。**

1. **本件（`indexing.rs`）への影響は無い。** 存在形 assert ゆえ、狭まる方向はどこで切れても赤になる（B と C）。plan.md の「不変条件と異常系」を、この 2 分岐で書き直した
2. **`launcher_controller.rs` の `activation_uses_frame_values_not_live_reads` には残余が実在する**（A）。あれは `!body.contains(forbidden)` の**否定形**であり、母集団が canary と forbidden の間で切れると**沈黙して緑になる**。現在の本体に該当する複数行文字列は無いので現況の欠陥ではないが、**`docs/development-principles.md` 176 行「狭すぎる（空）— 目印（canary）が母集団に在ることで捕まえられる」は否定形 assert には当てはまらない**——canary が捕まえるのは「空」であって「途中で切れる」ではない。**#1108 の射程外**につき follow-up issue の候補として人間へ回す

### 壊せなかった項目

- **命題 1**（他 3 サイトで広すぎる方向の沈黙は構成できない）— 3 サイトの「見つからない」分岐がすべて `expect` / `assert!(terminated)` による無条件 panic であることを読んで追認。`indexing.rs` の `None => after`（黙って EOF を採る）という**分岐の形そのものが特殊**である
- **命題 2**（`include_str!` の grep で完全）— `fs::read_to_string` の全ヒットは実 config / バックアップを読むテスト、`include_bytes!` は PNG、`*.psm1` の自己ソース読みは 0 件。**ただし間接構築は未探索**（上の表の注記へ反映済み）
- **命題 4**（`sectionOf` は騒がしい向き）— `AGENTS.md` の「条件別チェック」の後に同レベル見出しが実在し `end` は現に見つかる。かつ**下流の 2 節に `/…-check` の参照が 1 件も無い**ため、母集団が伸びても新たに拾われる参照はゼロ。射程外の判断を追認
- **命題 5**（#1077 は同一ファイル内の写し）— `git show 3cf75a15 --stat` を自分で実行し、`indexing.rs` が変更ファイルに無いことを追認

### ⚠️ の扱い

- **否定形 assert の残余（上記 A）** — 採る。実測で裁定済み。射程外の follow-up 候補
- **`G-module-linkage` の EOF フォールバックと `governance:check` の実行順序** — 保留。射程外であり、裁定には別の一次証拠（CI の job 順序）が要る
- **grep の間接構築が未探索 / `scripts/` の探索が非網羅** — 採る。上の 2 節へ限定を書き足した（全称で書かない）

## 未解決の疑問（plan.md の「未確定」へ引き継ぐ）

1. **切り出しを `indexing.rs` 内に局所で書くか、crate 共通の `#[cfg(test)]` helper へ寄せるか。** `docs/development-principles.md` 183 行は「helper は 1 つに閉じる」と書くが、実証はファイル内の写しについてのものである（上の一次証拠）。局所で書けば写しが 1 つ増え、レビューで DRY 違反として必ず挙がる。共通化すれば新ファイル + `mod` 宣言 + 索引の統治コストが乗り、終端の形（列 0 の `}` / 4 スペースの `}`）を引数化する必要がある
2. **`docs/development-principles.md` の当該節を更新するか。** 183 行が「1 つに閉じる」と書いている以上、1 の判断がどちらであれ**射程の書き方**が現況と一致するかを確かめる必要がある
3. `scripts/` 側の同族（`sectionOf`）へ follow-up issue を切るか
