# 調査 — issue #1139: `governance:check` の CI 残余 2 検査を編集時へ前倒しする

## issue の要約

`governance-check` job の CI ログ 45 日分（run 1000 本）で赤くなったのは 9 回・findings 15 件、うち
**12/15 が `G-module-index` と `G-references` の 2 本**だった。この 2 本を CI の事後検査から
**PostToolUse hook（編集時）へ前倒しできないか**を検討する。

issue が「決めること」として挙げたのは 3 点。

1. **前倒しの費用** — hook は編集ごとに走る。2 検査だけを切り出して呼べるか
2. **母集団の与え方** — hook は編集された 1 ファイルしか知らない。`G-module-index` は集合の照合なので
   スナップショット全体が要るのではないか（差分だけで判定できる形かを先に測る）
3. **前倒ししても CI は外さない**（`AGENTS.md`「検証の層と、層と層の隙間」）

なお issue は「CI で 0 件だから不要」は使えない基準だと先に確定させている（`G-heading-refs` が反例）。
本調査もその前提を継承し、**前倒しは「CI から降ろす」ことではなく「層を 1 枚足す」ことである**。

## 関連ファイル・モジュール・関数

すべて読了し、下記のシンボルは実在を確認済み。

| ファイル | 見たもの |
|---|---|
| `.claude/hooks/post-edit.mjs` | `selectChecks` / `isSourceFileWrite` / `dependentsReminder` / `BUDGETS` / `main()` の warnings 経路 |
| `scripts/governance/checks/G-module-index.mjs` | `checkModuleIndex(snapshot, crates)` / `MODULE_INDEX_CRATES` |
| `scripts/governance/checks/G-references.mjs` | `checkReferences(snapshot, docs, filterIgnored)` / `REF_EXTENSIONS` |
| `scripts/governance/lib.mjs` | `makeSnapshot` / `governanceDocs` / `gitIgnoredPaths` |
| `scripts/governance/dependents.mjs` | CLI 契約（hook から subprocess で呼ばれる先例・#1140） |
| `scripts/governance/registry.mjs` | `checkModulesFrom`（`checks/` 直下の走査） |
| `scripts/governance/checks/G-hook-fires.mjs` | `checkHookFires`（`selectChecks` ↔ `docs/hooks.md` 表の照合） |
| `docs/hooks.md` | 「PostToolUse（post-edit.mjs）の発火一覧」表・「検査ではない reminder」表 |
| `.claude/rules/safety-nets.md` | フォールトインジェクションの作法（複製に変異を当てる） |

## 再利用できる既存パターン

- **`dependentsReminder`（#1140）が、hook から governance スクリプトを subprocess で呼ぶ先例である。**
  `.md` 編集時に `node scripts/governance/dependents.mjs <rel>` を `spawnSync` し、stdout の 1 行を
  `warnings` へ積む。**exit code を動かさない**（gate ではない reminder）。
- **2 検査はどちらも母集団を引数で受け取る形に既になっている。** `checkModuleIndex(snapshot, crates)` の
  第 2 引数は既定 `Object.keys(MODULE_INDEX_CRATES)`、`checkReferences(snapshot, docs, filterIgnored)` の
  第 2 引数は `buildChecks` が `governanceDocs(snapshot)` を渡す。**絞った呼び出しは新しい述語を
  必要としない**——切り出しのために判定を書き直す必要がない。
- **crate の写像は `MODULE_INDEX_CRATES` を import して使える**（`{ src, exts }` を持つ）。hook 側で
  prefix 写像を新設すると members の写しが 1 枚増える（#500 の型）。
- **`.md` が `G-references` の対象かは `governanceDocs(snapshot).includes(rel)` で問える**——
  対象集合の写しを hook 側へ持たずに済む。

## 実測（2026-08-19・この機体・warm cache）

計測スクリプトは `<scratchpad>/measure.mjs` と `<scratchpad>/measure-red.mjs`。
変異は**実ファイルへ当てず、`snapshot` をラップして注入した**（`.claude/rules/safety-nets.md`
「稼働中のガードを弱めない——複製に変異を当てる」）。

### 費用（緑の枝）

| 対象 | 実測 |
|---|---|
| `node -e ""`（プロセス起動） | 47.8 / 57.9 / 71.4 ms |
| `makeSnapshot(root)` | 19.9 ms（files=469） |
| `governanceDocs(snapshot)` | 0.8 ms（docs=35） |
| `checkModuleIndex`（4 crate 全部） | 3.6 ms |
| `checkModuleIndex`（1 crate） | 1.9 ms |
| `checkReferences`（35 docs 全部） | 11.1〜15.2 ms |
| `checkReferences`（1 doc） | 0.3〜0.6 ms |
| **`node scripts/governance-check.mjs`（＝CI が叩く形）** | **588.7 / 646.6 / 669.9 ms**（再測 652.4 / 656.1 / 663.6 ms） |
| **`npm run governance:check`（＝人が叩く形）** | **1091.4 / 1122.7 / 1290.7 ms**（npm 自身の起動が乗る） |
| **`node scripts/governance/dependents.mjs AGENTS.md`**（既存の hook subprocess） | **233.0 / 234.1 / 237.8 ms** |

### 費用（赤の枝）と発火の確認

**緑だけを測ると `gitIgnoredPaths` の `git check-ignore` spawn を一度も通らない**（`paths.length === 0`
で早期 return するため）。変異を注入して測り直した。

| 変異 | 実測 | findings |
|---|---|---|
| `docs/hooks.md` に実在しない `` `docs/no-such-file-1139.md` `` を 1 行 | **30.0 ms**（同 doc の無変異は 0.6 ms） | 1 件・正しく名指し |
| `snotra-core/src/__orphan_1139.rs` を files へ追加（逆方向） | 2.4 ms | 1 件・正しく名指し |
| `snotra-core/CLAUDE.md` の索引へ実在しない `` `no_such_module_1139.rs` ``（順方向） | 1.7 ms | 1 件・正しく名指し |

**3 種の変異すべてが発火した**——検知器は前倒し先でも発火しうる（`measure-whether-detector-can-fire`）。
git spawn は赤のときだけ 29 ms 上乗せされる。

### この数字が答えること

- **issue の前提「`governance-check` 全体（数秒）」は、実測ではどちらの起動形でも 1.3 秒以下である。**
  「数秒」の出所は `.github/workflows/ci.yml:50` のコメント（「依存ゼロ・数秒で完了する」）で、
  issue はそれを引いている。CI が実際に叩くのは `node scripts/governance-check.mjs`（`ci.yml:74`・
  `runs-on: ubuntu-latest`）であって `npm run` ではない。**「桁が 1 つ違う」とまでは言わない**——
  node 直叩き 0.65 秒に対しては言えるが、npm 経由 1.1〜1.3 秒に対しては言えない
  （`AGENTS.md`「全称表現は前提条件とセットで書く」）。issue 本文は書き換えず、計画側に持つ。
- **絞った呼び出しの総額は node 起動 50〜70 ms + `makeSnapshot` 20 ms + 検査 2〜4 ms ＝ 約 75〜95 ms**
  （参照が赤なら +29 ms）。**既に受け入れている `dependentsReminder` の 235 ms より安い。**
- ゆえに**費用は絞り込みの根拠にならない**——全体 0.6 秒でも毎編集で払えなくはない。
  **絞る根拠は帰属である**: 全体を回すと「今の編集と無関係な既存の赤」が毎編集鳴り、慣れを作る
  （`partial-automation-habituates`）。絞れば鳴るのは「今の編集が壊しえた findings」だけになる。
- **「決めること」第 2 の答え**: `checkModuleIndex` は `allBasenames`（`snapshot.files` 全体）と
  `production`（`cfg.src` 配下の全 `.rs`）の両方を要るので、**差分だけでは判定できない**。
  `checkReferences` も `exists()` が `snapshot.files` 全体を引く。**しかし `makeSnapshot` は 20 ms なので、
  「不能だが安い」で問いは費用として消滅する。**

## 技術的制約

1. **新しい CLI を `scripts/governance/checks/` へ置いてはならない。** `registry.mjs` の
   `checkModulesFrom` が `checks/` 直下の全 `.mjs`（`.test.mjs` 以外）に `id` / `run` の export を
   要求して throw するため、CLI を置いた瞬間 `CHECK_MODULES` の構築が落ち、`governance:check` 自体が
   起動しなくなる。置き場は `dependents.mjs` / `instrument.mjs` と同じ `scripts/governance/` 直下。
2. **判定を hook へ静的 import してはならない**（`post-edit.mjs` と `dependents.mjs` の双方が注記）。
   import 文は `try { main() } catch` の**外**で走るため、解決に失敗すると JSON エンベロープを出さずに
   プロセスごと落ち、`.rs` の fmt / clippy / test まで**全編集が沈黙する**。subprocess で呼ぶ。
3. **スクリプトが無いツリーでは静かに no-op**（`dependentsReminder` の `existsSync` ガードと同じ）。
   この機構より前に凍結された worktree が該当する。
4. **`G-hook-fires` の母集団は `checks.push("<id>")` のリテラル全件である。** reminder 経路
   （`warnings.push`・id を持たない）で実装するなら、`docs/hooks.md` の**発火一覧表**・`BUDGETS`・
   `post-edit.test.mjs` の id カナリアは**無傷**である。gate（`selectChecks` へ id を足す）に転べば
   この 4 点セットが同一変更で動く。**この前提自体を計画の検証項目に置く。**
5. **`BUDGETS` のエントリ漏れは全検査が緑の間は沈黙し、最初の失敗で hook を TypeError で落とす。**
   gate 経路を選ぶ場合の落とし穴。
6. **`gitIgnoredPaths` は外部の `git` を spawn する**（`governance:check` の「依存ゼロ」は npm 依存の話）。
   絞った呼び出しでも `filterIgnored` を渡さないと**免除が効かず偽の赤**になる（既定引数は何も免除しない）。

## 設計上の分岐（計画で決める）

### (A) gate か reminder か

**reminder（exit code を動かさない）に倒す。** 新規 `.rs` を書いた直後に索引が未更新なのは正常な
作業順であり、gate にすると正当な途中状態が常時赤になって無視される
（`detector-scope-only-as-tight-as-needed`「広く縛ると正当な変更まで赤にして無視される」）。
CI 側が gate のまま残ることは issue の「決めること」第 3 が要求している。

### (B) トリガーの広げ方

現行 `isSourceFileWrite` は **Write 限定**で、その理由として「既存ファイルの Edit まで拾うと
沈黙=合格を壊す頻度になる」と「索引整合の判定そのものは governance:check が SSOT。ここで再実装すると
drift する」の 2 つを挙げている。判定つきの subprocess は後者には答える（判定は同じ関数を呼ぶので
drift しない）。**前者には答えない**——下記の反証を参照。

**#629/#630 の形（作成後に索引を書かず、以後の編集が全部沈黙する）を捕まえるには Edit まで要る。**

**反証（3b で壊された・採用）**: 「判定つきなら実際に欠けているときだけ鳴るので頻度が下がる」は片面的である。
`checkModuleIndex` は **crate 内の全ファイル**を照合するので、未解消の索引債務が crate 内に 1 件でも
残っている間は、**その crate への無関係な `.rs` Edit のたびに同じ reminder が繰り返し出る**。
Write 単発通知との比較で「頻度が下がる」とは言えず、**債務窓の間はむしろ増える**。

**この反証への手当て（計画で決める）**: reminder の内容を**編集した当のファイルに帰属する findings** へ
絞る。逆方向（実ファイルが索引に無い）は `rel` そのものが主語なのでフィルタできる。順方向
（索引に実在しないファイル名）は編集ファイルと無関係なので落ちるが、それは CI が引き取る層である。

**債務窓が現実にどれだけ開くかの実測（3b）**: 新規 `.rs` を追加したコミット 12 件のサンプルで
**12/12 が同じコミットで `CLAUDE.md` の索引も更新していた**。コミット粒度では債務は 0 に保たれており、
債務窓は稀である。**ただしこれは編集粒度の主張を裁定しない**——hook が走るのは編集ごとで、
1 コミットは多くの編集を畳んでいる（`dependents.mjs` の doc が同じ区別を持つ）。

**`isSourceFileWrite` の docstring は設計判断ごと書き換えが要る**——コードと注釈の矛盾として残さない。

### (C) 届け先のチャネル

現行の reminder は `warnings` → `systemMessage`（**人間向け**）。`post-edit.mjs` の `buildEnvelope` は
`additionalContext` が**エージェント向け**だと注記している（#471 実測）。#629/#630 はエージェントの
実行漏れなので、**当の失敗主体に見えるチャネルへ載せるかを決める**。曖昧にすると
「機構を足したのに当の失敗主体に見えない」で終わる。

**⚠️ 実証では決まらない（3b）**: #629/#630 の再発が「エージェントが `systemMessage` を見落とした」ためか
「当時 reminder 機構がまだ無かった」ためかは切り分けられない——**#629/#630 は reminder 機構の実装前の
出来事である**。ゆえにこの分岐は過去ログではなく設計原則で決める。**先例は `TS_LIKE` の情報行**で、
検査でないものを `sections`（→ `additionalContext`）へ載せる形が既に在る。

### (D) `.md` 編集時の二重 spawn

`.md` は既に `dependentsReminder`（235 ms）が走る。`G-references` を別 CLI にすると 2 spawn で
合計 325 ms 前後、同じ CLI へ相乗りさせると `node` 起動と `makeSnapshot` を共有して 250 ms 前後。
**責務は別（`dependents` は「合否を持たない計器」、`G-references` は検査由来）なので別 CLI が素直**だが、
費用差は計画で明示する。

## 非目標（issue の射程外として明記する）

- **`G-module-linkage`（`mod` 宣言の照合）は前倒ししない。** issue は 2 検査を名指ししており、
  `G-module-linkage` は CI 45 日で 0 件＝issue 自身の選定基準に掛からない。索引照合は `mod` 宣言を
  見ないので、「`mod` 忘れを見るのは `governance:check` の `G-module-linkage` である」系の規範文
  （`docs/hooks.md`「Claude Code の RA インスタンスと hook の分担」・ルート `CLAUDE.md`「フック」）は
  **真のまま残る**。5c でユーザーが広げたければ広げられる形で計画に置く。
- **CI の `governance-check` job は外さない**（issue の「決めること」第 3）。

## 波及するガバナンス文書

- `docs/hooks.md`「検査ではない reminder（発火一覧に現れない）」表 — 行を足す（または既存行を書き換える）
- `.claude/hooks/post-edit.mjs:15` の reminder の**数え上げ**が動く。**この行は今日すでに紛らわしい**——
  「gate ではない reminder が **2 つ**在る」と書きながら括弧内に 3 項目（`config-warn` / 新規 `.rs` の索引 /
  `.md` の依存参照）を挙げている（`config-warn` は `checks.push` を通るので数えない対象、という整合は
  取れなくはない）。ルート `CLAUDE.md`:29 の「`.md` には検査ではない reminder が **1 つ**在るが」も同型。
  **数を直すのではなく「〜だけではない」の下限主張へ倒す**（`universal-claim-fix-regenerates-itself`:
  偽の全称を直した文がまた別の形で偽になる連鎖を止める）
- **`AGENTS.md`:67**（条件別チェック表「ファイル（`.rs`）を追加/削除」行）の「**編集時の hook の沈黙を
  『`mod` も足りている』と読まない**」——索引側だけが編集時に見られるようになるので、**この文の射程が
  「索引と `mod`」から「`mod`」へ狭まる**。**issue 本文はこの文をルート `CLAUDE.md`「フック」の記述として
  引用しているが、逐語の所在は `AGENTS.md` である**（実測。同期対象の文書が issue の記述と違う）
- **削除の残余は不変**（`rm` は `Edit|Write` matcher に届かず、CI の `governance-check` が orphan を
  捕捉する）——新機構が削除も見ると誤読されない一文を残す

## 未解決の疑問（計画の「未確定」欄へ送る）

1. (C) のチャネル — `systemMessage` だけか、`additionalContext` にも載せるか
2. (D) の相乗り — 別 CLI か、`dependents.mjs` と 1 spawn に束ねるか
3. `.rs` の Edit まで広げたときの**実際の発火率** — 「索引が欠けている状態」は本来まれなので、
   ほとんどの編集で沈黙するはず。ただし**測っていない**（`dependents.mjs` は実装後に 80 コミットで
   測って計画段階の見積もり 24〜25% を 55% へ訂正している。同じ轍を踏まないため、実装前に測るか、
   測れないなら「測っていない」と書く）

## 敵対的調査（3b）の所見と採否

サブエージェント 1 体（general-purpose / sonnet）。詳細は `workspace/adversarial-1139.txt`、
再現スクリプトは `workspace/adv-cli-1139.mjs` / `adv-spawn-1139.mjs` / `adv-crosscrate-1139.mjs`
（いずれも `/implement` が `workspace/` ごと撤去する）。

### 壊せた項目（2 件・いずれも採用）

| 所見 | 採否 | 自分で裁定した一次証拠 |
|---|---|---|
| 費用表の**ラベルが誤り**。`npm run governance:check` は 1091〜1291 ms で、588〜670 ms は `node scripts/governance-check.mjs` の値 | **採用**（表を訂正済み） | 自分で再測: npm 経由 1091.4 / 1122.7 / 1290.7 ms、node 直叩き 652.4 / 656.1 / 663.6 ms。**機序も一次証拠で確認**——`ci.yml:74` は `run: node scripts/governance-check.mjs`、`runs-on: ubuntu-latest`（3b が渡された「Windows runner」という前提の方が誤りだった） |
| 設計分岐 (B) の「判定つきなら頻度が下がる」が**片面的**。索引債務が crate 内に残る間、無関係な Edit のたびに同じ reminder が繰り返し出る | **採用**（(B) 節に反証と手当てを追記） | `checkModuleIndex` のソース——逆方向は `production`（`cfg.src` 配下の全 `.rs`）を回すので、編集ファイルに関係なく同じ finding を出す |

### 壊せなかった項目（7 件）

- 費用見積もり「約 75〜95 ms」は実 `spawnSync` 計測（70〜96 ms）と一致した——**単純和の見積もりが
  `import` 解決コストを落としているのではないかという攻めは通らなかった**
- 「issue の前提『数秒』より小さい」は CI の実 run（`gh api .../jobs`）で独立に裏付けられた
  （governance check ステップは 1 秒未満で完走）
- `G-references` を 1 doc へ絞ったときの取りこぼし（**他の文書からその編集ファイルを指す壊れた参照**）は、
  `Edit|Write` matcher がリネーム・削除を捉えない構造上、**編集起点では発生しえない**
- `allBasenames` がスナップショット全体である（別 crate の同名 basename による偽の緑）ことは、
  **絞り込みの有無で変わらない**——narrow / full の双方で同一の偽の緑を実測
- 技術的制約 1（`checks/` へ置くと `registry.mjs` が throw）と 4（`G-hook-fires` は `checks.push` だけを拾う）は
  ソース確認で正確
- 設計判断 (A)（gate ではなく reminder）は反証されなかった。新規 `.rs` 追加コミット 12 件で 12/12 が
  `CLAUDE.md` を同時更新していたが、**コミット粒度の観測は編集粒度の主張を裁定しない**（(B) 節に記録）
- 相対 `import`・worktree の懸念はいずれも構造的に問題なし

### ⚠️（確信の持てない所見・すべて計画へ送る）

1. **(C) のチャネル選択は過去ログから切り分け不能**——#629/#630 は reminder 機構の実装前の出来事
   （(C) 節へ反映済み。設計原則で決める）
2. **Edit まで広げたときの実発火率は未測定**（`research.md` 自身も認めている。未解決の疑問 3）
3. `dependentsReminder`（#1140）が実運用で無視されていないかは、運用実績がまだ無く検証不能——
   **reminder という形式そのものの有効性に、この時点で一次証拠は無い**
4. 「『数秒』より桁が 1 つ小さい」は npm 経由 1.3 秒との対比では強すぎる（実測節で表現を弱めた）
