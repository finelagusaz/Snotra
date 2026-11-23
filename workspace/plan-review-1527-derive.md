# #1527 独立導出（race:boundaries ④ と `read_config`）

計画・調査（`workspace/plan.md`・`research.md`・`adversarial-*.txt`）は読んでいない。目にもしていない。

## 1. 判断: 列挙する（パターン `read_config\(` を ④ に足す）

根拠:

- **`read_config(` 以外に config の読みの綴りが残っていない。** `AppState.config` は private（`src-tauri/src/state.rs:25`）で、外から `state.config.read()` と書くと E0616（同 doc `state.rs:108-109`）。read guard を取るのは `AppState::read_config` の本体 `state.rs:144` の 1 行だけである。したがって **src-tauri で config を新しく読む差分は、ほぼ必ず `read_config(` の行だけを足し、`\.read\(\)` には当たらない。** これが #1527 の穴そのもの。
- **現行の ④ はこれを 1 件も拾わない（実測）。** 現在のツリーの src-tauri/src・snotra-core/src・snotra-egui-runtime/src・snotra-settings/src・scripts の全行を「追加行」とみなして `selectPopulation` → `classify` に通した結果、`read_config\(` に当たる非コメント行は **25 行**。そのうち現行の ④ パターンに当たる行は **0 行**（スクラッチの測定スクリプト `m.mjs` で `scripts/race-boundaries.mjs` を import して実行）。
  - 例外的に拾える経路は 2 つ: `egui_shell::read_config` の本体 `app.try_state::<crate::AppState>()`（`egui_shell/mod.rs:469`、`try_state` で当たる）と `AppState::read_config` の本体（`.read()` で当たる）。どちらも定義の側であって、新しい呼び出し点では当たらない。
- **これは並行境界である。** 読みは `Arc<RwLock<Config>>` の read guard で、`update_config`（config_watcher スレッドで engine 錠の内側にある `write()`）と競合する。読みの中で錠や I/O を取ると永久待ちになる、という契約（`state.rs:116-125`）は、読みを足すたびに確かめる対象になる。値の一貫性（`commands/icon.rs:13`「間に `update_config` が挟まると…新旧で混ざる」）も境界の問い d・e の対象である。
- **誤検出の母集団は小さい（実測）。**
  - 現在のツリー: 25 行のうち 24 行が呼び出し点。残る 1 行は `egui_shell/launcher_controller/activation/tests.rs:733` の文字列リテラル `for forbidden in ["read_visible_rows(", "read_config("]`（ソーステキスト検査）で、`.rs` のテストなので母集団から外せない。
  - 履歴: `git log origin/main -p -G'read_config\('` の非コメント追加行は 33 行。そのうち偽陽性は上と同じ文字列リテラルの 3 版（3/33）。それ以外はすべて呼び出し点だった（`Some(s) => s.read_config(read)` の委譲も含む。委譲も読みなので真陽性と数えてよい）。
  - `src-tauri` 以外に `read_config` は無い（snotra-core / snotra-egui-runtime / snotra-settings / scripts とも 0。`.ts/.tsx/.mjs/.ps1` の `git grep` も 0）。
- 列挙しない案（理由を doc に書く）を採る根拠は見つからなかった。`read_config` は綴りが一意で、誤検出が少なく、しかもこの crate で config を読む唯一の口である。

## 2. 変更するファイルとシンボル

| ファイル | シンボル | 変更 |
|---|---|---|
| `scripts/race-boundaries.mjs` | `KINDS` の ④（`ord: "④"`）の `patterns` | `"read_config\\("` を足す（JS の文字列リテラルとしての綴り。正規表現は `read_config\(`）。直上のコメント（現行 `:31-34` の RwLock の段落）の隣に、足した理由を 1〜2 文書く: config の読みは private フィールドで `read_config` に閉じているので、`\.read\(\)` は呼び出し側に現れない（#1527）。必要なら偽陽性の母集団（ソーステキスト検査の文字列リテラル）も書く |
| `scripts/race-boundaries.test.mjs` | 新しい `describe`（既存の `describe("④ の RwLock 系 …")`（`:190-229`）と同じ形） | 差分の形の fixture を `parseDiff` → `selectPopulation` → `classify` に通し、`[text, pattern]` の組で assert する（`.claude/rules/safety-nets.md`「検出器のカバー範囲」の理由ごとの assert）。fixture の行は現行のソースから写す。候補: `state.read_config(\|cfg\| {`（`commands/launch.rs`）、`super::read_config(`（呼び出しの頭だけの行。`window_coordinator.rs`）、`crate::egui_shell::read_config(`。不混入の側の候補は §3 の境界ケースから選ぶ |

- **文書側に写しは無い（確認済み）。** `.claude/skills/race-check/SKILL.md:22` と `docs/build-commands.md:244` は、パターンの SSOT として `scripts/race-boundaries.mjs` を指すだけである。`docs/check-skill-skeleton-design.md` に ④ の言及は無い（grep 0 件）。`docs/superpowers/specs/2026-07-29-plan-review-instrument-split-design.md:168` は「同型の正規表現母集団」と言うだけで、パターンを写していない。
- 序数・`name: "managed state の読み書き"` は変えない（`KINDS` の doc「序数 ①〜⑧ は書き換えない」）。名前は read_config を含んでも真のまま。
- テスト「並行と無縁のコードは 1 種別も立てない」（`race-boundaries.test.mjs:184-187`）の fixture は `read_config` を含まないので影響しない。

## 3. 境界条件

- **複数行に割れた呼び出し:** 現行では呼び出しの 17 行（`read_config(` で行が終わるもの。§1 の測定の出力から数えた）が `super::read_config(` / `crate::egui_shell::read_config(` で行末に終わり、クロージャは次の行以降に続く。**パターンは頭の行で当たる**ので問題ない。`read_config` と `(` が別の行に割れる形・turbofish `read_config::<T>(` は現行 0 件（`git grep -E 'read_config::\|read_config *$'` が空）。rustfmt はこの割り方をしない。⚠️ 将来 turbofish で書かれると漏れるが、型推論で書かずに済むので実害の見込みは低い。下限としてコメントへ書くかは任意。
- **定義行:** `pub fn read_config<T>(`（`state.rs:143`）と `pub(crate) fn read_config<T>(`（`egui_shell/mod.rs:464`）は、`<T>` が挟まるため `read_config\(` に当たらない（実測。bare の `read_config` なら 27 行になり、差の 2 行はこの 2 つの定義）。定義の本体の読み（`.read()` / `try_state`）は既存のパターンで拾うので、定義を外して困らない。テストで「定義行は立たない」を不混入として assert するかは任意（⚠️ 定義を外すのが意図か偶然かを書き残さないと、後で `read_config\b` へ緩めたときに黙って変わる）。
- **コメント行:** `read_config(` を含むコメントは 3 行（`activation/tests.rs:433・676・678` の `///`）。どれも `isCommentLine` で外れる（実測: raw 28 行、うちコメント 3 行 → 25 行）。doc の intra-link `[`crate::AppState::read_config`]` は `(` を伴わないので、そもそも当たらない。
- **文字列リテラル:** `activation/tests.rs:733` は当たる（偽陽性 1）。既存の `\.read\(` 系と同じく、ソーステキスト検査の文字列は行の判定では外せない。受け入れて、コメントかテストの fixture に書き残すのが妥当。
- **ラベル競合:** `classify` は `findIndex` で **最初に当たったパターン**をラベルにする（`race-boundaries.mjs:176`）。`read_config(` の 25 行のうち、他の ④ パターンにも当たる行は 0（実測の「④現行」列がすべて `-`）。将来 `app.try_state::<AppState>().map(\|s\| s.read_config(..))` のような行が来ると、配列で先にある `try_state` のラベルになる（件数は 1 のまま）。末尾に置けば既存のラベルは 1 つも変わらない。ラベルまで assert するテストは、競合しない行で書く。
- **書き込み側 `update_config`:** 足さなくてよい。src-tauri の呼び出し 2 行（`config_watcher.rs:152`、`state.rs:239` のテスト）は `state.engine.lock().unwrap().update_config(..)` と 1 行で書かれ、すでに `\.lock\(` で当たる（実測）。`Engine::update_config` は `&mut self` を要求し、`AppState` からは engine 錠を経ないと呼べない。本体の `*self.config.write()` は `\.write\(\)` で当たる。snotra-core 内の 3 行（定義と `engine.rs` のテスト `engine.update_config(..)`）は当たらないが、そこは engine を値で持つテストである。⚠️ 残余: `let mut eng = state.engine.lock().unwrap();` を既存のスコープに持ったまま `eng.update_config(x)` の 1 行だけを足す差分は漏れる。現行にこの形は無い。足すなら `update_config\(` で、snotra-core のテストが 2 行増える。足さないなら、コメントへ 1 文書けば判断が残る。
- **迂回の読み:** `engine.lock().unwrap().config_handle().read()`（`state.rs:113` が残余として記録）は `\.lock\(` と `\.read\(\)` で当たる。ハンドルを `let h = …config_handle();` で取り出してから後で `h.read()` する形も、`.read()` の行で当たる。`config_handle` をパターンに足す必要は無い。

## 4. 変更で偽になる散文

- 見つからなかった。調べた範囲は次のとおり。
  - `race-boundaries.mjs` の `formatReport` 末尾（「別名 import・再エクスポート・マクロ生成・`Builder::new().spawn` は当たらない」）は真のまま。
  - `state.rs:106`「この crate で read guard を取る唯一の地点」・`egui_shell/mod.rs:457` は、スクリプトと無関係で真のまま。
  - `docs/architecture.md:248`・`PERFORMANCE.md:631`・`docs/development-principles.md:110` の `read_config` への言及は、④ について何も主張していない。
  - `.claude/` と規範文書に「④ は read_config を拾わない」という記述は無い（`git grep`）。
- 書き足す側の注意: 新しいコメントに「config の読みはすべて `read_config(` を通る」と全称で書くと、迂回（`config_handle().read()`）が反例になる。「外から届く綴りは `read_config(` に閉じている（`state.rs` の `read_config` の doc）」のように正本を指す形にする（`AGENTS.md`「検証の作法」の全称の項）。数（25 行・3/33）をコメントへ写すなら、測った sha を添える（既存の `716b816 で実測` と同じ作法）。

## ⚠️ 確信の持てない所見

- ⚠️ `read_config\(` と `\bread_config\(` は、現行では同じ 25 行になる（実測）。`try_read_config(` のような名前が将来できたときに違いが出る。どちらでもよいが、`\b` を付けると意図がはっきりする。
- ⚠️ 定義行を外すことを assert すると、テストが「意図」を固定する。固定しないなら、テストはそれを見ていないことを承知しておく。
- ⚠️ `update_config` の扱い（§3）は判断の分かれ目である。現行の差分の形では `\.lock\(` が覆っているので「足さない」を推すが、残余は 1 文書き残したほうがよい。
