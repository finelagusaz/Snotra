# issue #1139 — G-module-index / G-references の編集時前倒し（独立導出）

**枠の性質**: `workspace/` 配下を一切読まず、issue 本文とコード・規範文書だけから導出した。他者の計画は見ていない。
**実施**: 2026-08-19。根拠は `file:line` か実測コマンドの出力。

---

## 0. 先に測った 2 つの前提（issue「決めること」への直接の答え）

### 0-1. 費用 — 2 検査の切り出しは成立する

実測（この作業ツリー・`node` 一発起動）:

| 対象 | 実測 |
|---|---|
| `makeSnapshot(cwd)`（469 ファイル） | **12 ms** |
| `checkModuleIndex(snapshot)` | **3 ms** |
| `governanceDocs` + `checkReferences` | **13 ms** |
| 上記 3 つを含む **node プロセス全体** | **116 ms** |
| `node scripts/governance-check.mjs` 全体 | **634 ms** |

判断材料:

- 純粋な判定コストは **28 ms**。支配項は node の起動（約 88 ms）である。
- 先例 `ADR-dependents-reminder-at-edit-time.md:18` が **「代償は node 起動 109〜130 ms、これを払う」** と既に裁定している。同 ADR の帰結（`:29`）は `.md` 1 回編集の hook 全体を 330〜386 ms と実測しており、**同じ subprocess へ相乗りすれば増分は判定分の 28 ms のみ**になりうる。
- 但し書き: `tRef = 13 ms` は **findings 0 件の木での値**。`gitIgnoredPaths`（`scripts/governance/lib.mjs:368-377`）は `paths.length === 0` なら `git` を spawn しないため、**赤が出る局面でだけ `git check-ignore` の spawn 1 回が乗る**。

### 0-2. 母集団 — 「差分では判定できない」が「安いので全部渡せる」

- `G-module-index` は**両方向とも全ファイル集合を要求する**。順方向は `allBasenames = new Set(snapshot.files.map(...))`（`scripts/governance/checks/G-module-index.mjs:37`）、逆方向は `snapshot.files.filter(f => f.startsWith(cfg.src))`（`:71-73`）。**編集された 1 ファイルの差分からは原理的に判定できない。**
- `G-references` は `governanceDocs(snapshot)`（`scripts/governance/lib.mjs:461-470`・実測 35 件）を走査元に取り、参照先の実在は `snapshot.files` 全体で照合する（`checks/G-references.mjs:27,42`）。これも全集合が要る。
- **答え**: hook は `rel` だけを渡し、**subprocess 側が `cwd = root` で全スナップショットを組む**。既存の分担そのままである（`.claude/hooks/post-edit.mjs:474` 「判定は subprocess 側が持つ」）。スナップショット構築 12 ms なので、母集団の問題は費用の問題に還元されて消える。
- **残るのは母集団ではなく「帰属」の問題である**（→ §4-1）。

---

## 1. 変更・新設が要るファイルとシンボル

以下は **主系（reminder 枝）** を軸に列挙し、**代替（gate 枝）** の差分を分けて示す。分岐の判別子は §5。

### 1-A. 主系（reminder 枝）— `dependents.mjs` と同型・exit code を動かさない

| # | ファイル | 変更 | 根拠 |
|---|---|---|---|
| A1 | **新設** `scripts/governance/edit-time.mjs`（名前は例。**`checks/` 配下に置いてはならない**） | CLI + 純関数。`makeSnapshot(process.cwd())` を組み、`checkModuleIndex` / `checkReferences` を呼び、**編集ファイルへ帰属する findings だけ**を 1 行の WARN に畳んで stdout へ出す。`process.exitCode = 0` 固定 | 置き場所: `scripts/governance/registry.mjs:17-32` が `checks/` 直下の `.mjs` を走査し、`id`/`run` を export しないファイルで **throw** する（`:27-29`）。CLI をそこへ置くと `governance:check` 全体が落ちる。`dependents.mjs` と同じ階層が正しい |
| A2 | 同上（import 元） | `import { makeSnapshot, governanceDocs, gitIgnoredPaths } from "./lib.mjs"` / `import { checkModuleIndex } from "./checks/G-module-index.mjs"` / `import { checkReferences } from "./checks/G-references.mjs"` | **facade（`scripts/governance-check.mjs`）経由で読んではならない**。`governance-check.mjs:71-82` が「再輸出の一覧が短いことに機構上の役目がある（#1094）」と明記しており、`checks/` の関数を facade へ戻すと消失検知が manifest 差分から import エラーへ戻る |
| A3 | 同上（CLI 契約） | `node scripts/governance/edit-time.mjs <rel>`・**exit code は常に 0**・依存ゼロ・決定的 | `dependents.mjs:172-198` の CLI ブロックが逐語の手本。`governance-check.mjs:14-15` の「依存ゼロ・決定的」契約は `checks/` 配下の各検査にも及ぶので、その呼び出し側も継承する |
| A4 | **新設** `scripts/governance/edit-time.test.mjs` | フォールトインジェクション red / 正常 green / 判定対象外の不混入。`vitest.config.ts` の include に `scripts/**/*.test.mjs` があるので自動で `npm test` と CI に載る | `vitest.config.ts:9-11`（実測） |
| A5 | `.claude/hooks/post-edit.mjs` — **新関数** `governanceReminder(rel, root, run = spawnSync)`（名前は例） | `dependentsReminder`（`:193-206`）の逐語同型。**`existsSync(script)` ガードを必ず踏襲する**（凍結 worktree で静かに何もしない）。`res.error || res.status !== 0` なら空文字 | `.claude/hooks/post-edit.mjs:193-206` |
| A6 | `.claude/hooks/post-edit.mjs` — `main()` の配線 2 行 | `const g = governanceReminder(rel, root); if (g) warnings.push(g);` を `:475-476` の隣へ | 同 |
| A7 | `.claude/hooks/post-edit.mjs` — **`isSourceFileWrite` の去就**（`:220-223` と `:466-472` の WARN） | この関数は「索引整合を判定せず低頻度シグナルで promt する」代替物である（JSDoc `:211-218`）。**A1 が本物の判定を持つので、盲目 reminder は撤去するのが筋**。撤去なら `post-edit.test.mjs:191-219` の 5 テストも同時に落とす | `.claude/hooks/post-edit.mjs:212-214`「ゆえに hook は**索引を検査せず**」——この理由付けが A1 で消える |
| A8 | `.claude/hooks/post-edit.mjs` — ヘッダーコメント `:15-18` | 「reminder が 2 つ在る」の数え上げ。**数を書き直さず非列挙化する**（→ §2-1） | 同 |
| A9 | `.claude/hooks/post-edit.test.mjs` | `governanceReminder` のユニット（spy 注入 6 本＝`dependentsReminder` の `:222-278` と同型）＋ **静的 import を足していないカナリアの対象へ新スクリプトも含める**（`:272`） | `.claude/hooks/post-edit.test.mjs:222-278` |
| A10 | `.claude/hooks/post-edit.test.mjs` — **統合テスト** | 一時 git repo に最小の木を作り hook を**プロセス起動**して、A6 の 2 行が消えたら赤になることを固定する | `ADR-dependents-reminder-at-edit-time.md:30`「配線は統合テストでしか守れない——その 2 行は消してもユニットテスト 96/96 緑だった（変異注入で実測）」 |
| A11 | **新設** `docs/adr/ADR-<slug>.md`（例 `ADR-governance-checks-at-edit-time.md`） | 却下した案（gate 化・CI から外す・差分だけで判定・checks/ へ CLI を置く・facade 経由 import・全 findings を出す）を否定の知識として残す | `AGENTS.md`「ドキュメント参照」の ADR 行。連番を振らない（`.claude/rules/governance-docs.md:19`） |
| A12 | `docs/hooks.md`「検査ではない reminder（発火一覧に現れない）」（`:101-110`） | 表へ行を足す／`.rs` Write の行を「索引の実判定」へ書き換える。`:110` の「純追記では出ず」は `.md` 依存参照に固有の条件なので**射程を明示する** | 実測 `docs/hooks.md:101-110` |
| A13 | `CLAUDE.md:29` / `.claude/rules/governance-docs.md:23` / `docs/build-commands.md:32,167` | §2 の一覧に従って訂正 | 同 |
| A14 | `AGENTS.md:65,67` | §2-9 / §2-10 | 同 |
| A15 | `.claude/skills/implement/SKILL.md:77,86` | §2-12 / §2-13 | 同 |
| A16 | `docs/development-principles.md:166`（層の表） | §2-11 | 同 |

**変更しないもの**: `.claude/settings.json`（matcher は `Edit|Write` のまま・`:17`）、`.github/workflows/ci.yml`（`governance check` step を外さない・`:73-74`）、`scripts/governance-check.mjs` の検査配列、`scripts/governance-manifest.mjs`（`manifest()` は `checks`/`docs`/`rules`/`skills` の 4 列しか見ないので、`scripts/governance/` 直下への新設は delta に現れない・`:23-33`）。

### 1-B. 代替（gate 枝）— `selectChecks` へ検査 id を足す場合の**追加**差分

| # | 対象 | 内容 | 根拠 |
|---|---|---|---|
| B1 | `.claude/hooks/post-edit.mjs` `selectChecks`（`:130-176`） | `checks.push("governance-lite")` を**このリテラルの形で**書く | `checks/G-hook-fires.mjs:128` の母集団抽出が `/checks\.push\("([^"]+)"\)/g` しか拾わない。別の形で発行すると照合の外に沈黙で出る |
| B2 | `.claude/hooks/post-edit.mjs` `BUDGETS`（`:40-53`） | 新 id のエントリ。欠けると**最初の失敗で hook 自体が TypeError で落ちる**（`:37-38`） | `post-edit.test.mjs:769-798` の完全性カナリア |
| B3 | `.claude/hooks/post-edit.test.mjs:775-784` `REPRESENTATIVE_EDITS` | 新 id を発火する代表パスを足さないと**カナリアが新 id を一度も見ない** | 同 |
| B4 | `.claude/hooks/post-edit.mjs` `buildCommand`（`:335-382`） | `nodeSpec([...])` の case。**`repro` の区切りは `/` へ正規化**（`toPosixPath`・`:339`）。怠ると PreToolUse が自分の hook の指示を拒む | `.claude/hooks/post-edit.mjs:326-333`＋`post-edit.test.mjs:584-586` の相互契約カナリア |
| B5 | `docs/hooks.md:46-59` の発火一覧 | 新 id を持つ行を追加。**さらに `（なし）` の行が要る**——`.md` が id を発火するようになると現行の空集合代表パス `docs/hooks.md`（`:59`）が空でなくなり、`checkHookFires` の `sawEmptyRow`（`checks/G-hook-fires.mjs:103,125-127`）が赤になる。**新しい空集合代表パスを選び直す**（候補: `Cargo.lock`・`scripts/governance/lib.mjs`・`.github/workflows/ci.yml`。いずれも `selectChecks` が空を返し、かつ実在する＝`:95` の実在検査を通る） | 同 |
| B6 | 凍結 worktree | `runCheck` は `existsSync` ガードを持たない。スクリプトが無いツリーで `node <missing>` を spawn すると **MODULE_NOT_FOUND が「検査失敗」として赤くなる**。ガード相当を `buildCommand` が `null` を返す形で入れると今度は `HOOK ERROR`（`:400-403`）になる | `.claude/hooks/post-edit.mjs:399-421` |

---

## 2. この変更で「偽になる」既存の散文

**枝によって偽になる範囲が違う**ので分けて示す。〔両〕= どちらの枝でも偽、〔gate〕= gate 枝でのみ偽、〔誤導〕= 偽ではないが読み手を誤らせる。

| # | 所在 | 現在の記述（要点） | どう偽になるか |
|---|---|---|---|
| 2-1 | `.claude/hooks/post-edit.mjs:15-16` | 「**検査とは別に、gate ではない reminder が 2 つ在る**（config-warn / 新規 .rs の索引 / .md の依存参照・#1140）」 | 〔両〕**数え上げ**。3 つ目が入る。なお現状すでに「2 つ」と言いながら 3 項目を並べており、数え上げ散文が腐りやすいことの実例になっている。**「2→3」と直さず、`.claude/rules/governance-docs.md:19`「機構の実装の詳細（述語の種類・件数・分岐の列挙）を散文へ写さない」に従って非列挙化する**（#1091 の「偽の全称を直した文がまた偽になる」も同じ向き） |
| 2-2 | `.claude/hooks/post-edit.mjs:212-214` | 「索引整合の判定そのものは governance:check が SSOT。ここで再実装すると drift する（DRY）。**ゆえに hook は索引を検査せず**、低頻度シグナルで reminder を出すだけに留める」 | 〔両〕**理由付けごと偽になる**。subprocess で SSOT を呼ぶなら「再実装＝drift」の前提が消え、「索引を検査せず」も偽。A7 で関数ごと撤去するなら記述も消える |
| 2-3 | `docs/hooks.md:101-110`「検査ではない reminder（発火一覧に現れない）」節 | 表 2 行（`.rs` Write → 索引 reminder / `.md` → 依存参照）。`:110`「純追記では出ず、判定スクリプトが無いツリーでも出ない」 | 〔両〕表が不完全になる。`:107` の「モジュール索引の更新 reminder（#629/#630）」は**盲目 promt から実判定へ**性質が変わる。`:110` の「純追記では出ず」は依存参照に固有の条件で、3 つ目には当たらない＝**全 reminder に掛かる読みになると偽** |
| 2-4 | `docs/hooks.md:59` | `` | `docs/hooks.md` | （なし） | 上記以外（`*.md`・`.claude/rules/**`・`.claude/skills/**`・`scripts/**` 等）は**検査が 1 つも走らない** …`.md` には検査でない reminder が在るが、id を持たないのでこの列は空のままである `` | 〔gate〕id が付けば「この列は空のまま」が偽。**機械が捕まえる**（`checks/G-hook-fires.mjs:107-116` の順序込み一致）。〔主系〕では真のまま |
| 2-5 | `CLAUDE.md:29` | 「**`.md` には検査ではない reminder が 1 つ在るが**（節の中身を変えたときの依存参照・#1140）」／「決定的な項目（参照実在・索引・…）は PR CI の `governance-check` job が**事後に**捕捉し」 | 〔両〕前者は数え上げが偽。後者は「事後に」が半分偽になる——参照実在と索引は編集時にも出る（合否は依然 CI が持つ）。ここは**列挙をやめ「合否は CI が持つ」だけを残す**のが再発しない形 |
| 2-6 | `.claude/rules/governance-docs.md:23` | 「**これらの編集に PostToolUse hook 検査は走らない**（`selectChecks` が空集合を返す）」「**検査ではない reminder は 1 つ出る**」 | 〔gate〕前段が偽。〔両〕後段の数え上げが偽。この行は `.md` を触るたび自動配送される面なので、腐ると被害が大きい |
| 2-7 | `docs/build-commands.md:32` | 「**検査とは別に gate ではない reminder が在り**（`.md` の依存参照・#1140）、その不在も『問題が無い』を意味しない」 | 〔両〕括弧内の名指しが不完全になる（数ではなく**列挙**が腐る形） |
| 2-8 | `docs/build-commands.md:167` | 「PostToolUse フックは `.md` に検査を割り当てない（#497 の受容を維持）ため、**編集時の沈黙は「何も走らなかった」である**」「**`.md` に出る依存参照の reminder は検査ではない**——鳴っても `governance:check` の代わりにはならず」 | 〔gate〕第 1 文が偽。〔両〕第 2 文の名指しが不完全。**「鳴っても代わりにならない」は主系でも真のまま**（帰属フィルタで一部しか見ないため。むしろ強調が要る） |
| 2-9 | `AGENTS.md:67` | 「**索引と `mod` 宣言は別々に機構が見る**」「**編集時の hook の沈黙を『`mod` も足りている』と読まない**」 | 〔誤導〕**この 2 文は偽にならない**（`G-module-linkage` は前倒ししないので `mod` は依然 CI 専管）。だが**非対称が生まれる**——索引は編集時に見えるようになり `mod` は見えないまま。「編集時の沈黙」の指す範囲が変わるので、**どちらが前倒しされたかを名指ししないと読み手が逆に取る**。issue 本文が引いているのがまさにこの一文である |
| 2-10 | `AGENTS.md:65` | 「ガバナンス文書…を変更 → `npm run governance:check`（PR では CI の governance-check job が常時実行）」 | 〔誤導〕真のまま（CI は外さない）が、**編集時に一部が出ることを知らないと「reminder が出なかった＝緑」と読む**経路ができる |
| 2-11 | `docs/development-principles.md:166`（「検証の層と、層と層の隙間」の表） | 「文書の整合 \| `governance:check` \| 参照・索引・命名の着地 \| 意味の側」 | 〔両〕**手段の列が不完全になる**（同じ保証の一部が編集時 hook にも載る）。同節の主張「**穴は層の内側ではなく境界に空く**」（`:168`）が直に当たる——**新しい編集時出力が消費されているか**を誰も見ない、が新しい隙間である |
| 2-12 | `.claude/skills/implement/SKILL.md:86` | 「ここで委ねるのは権威ある一式である（**hook が走らせない `cargo doc` と `npm run governance:check` を含む**。飛ばしやすいのはまさにその 2 つで、hook の沈黙に慣れることが原因だった）」 | 〔両〕「hook が走らせない `npm run governance:check`」が**部分的に偽**。しかも「hook の沈黙に慣れることが原因」という機序の記述は、前倒しによって**逆に強まりうる**（→ §4-4） |
| 2-13 | `.claude/skills/implement/SKILL.md:77` | 「索引漏れは `governance:check` が捕捉する…が PR まで漏らさない」 | 〔両〕不完全になる（編集時にも出る） |
| 2-14 | `scripts/governance-check.mjs:6-9` | 「PostToolUse hook は `.md`・rules・skills に**検査を割り当てない**（#497 で受容した残余）。本スクリプトはその残余のうち決定的に照合できる項目を **PR CI と `npm run governance:check` で引き取る**」 | 〔gate〕前段が偽。〔両〕後段が不完全（第 3 の呼び出し口ができる） |
| 2-15 | `docs/adr/ADR-dependents-reminder-at-edit-time.md:26` | 「**`.md` の沈黙の意味が変わった。**…検査ではない reminder が**1 つ**入る。この訂正は ルート `CLAUDE.md`・`docs/build-commands.md`（2 か所）・`docs/hooks.md`（2 か所）・`.claude/rules/governance-docs.md`・`post-edit.mjs` へ同時に入れた」 | **触ってはならない**（`ADR-adr-frozen-history`・`.claude/rules/governance-docs.md:21`「ADR 本文内の参照は照合されない——凍結された歴史であり腐るに任せる」）。**ただしこの一覧は「今回どこを直すべきか」の逐語の地図として使える**——同じ 6 か所＋α が今回の訂正対象である（§2 の一覧と照合すると 6 か所すべてが再登場している。これは偶然ではなく、**同じ命題を運ぶ写しの集合**である） |

### 2-x. 概念ラベル grep の実施記録（母集団の接地）

```
grep -rn "沈黙"                       --include=*.md .   → 30.2 KB（ヒット多数・上表へ反映）
grep -rn "reminder\|編集時\|編集のたび\|編集直後" --include=*.md .   → 12 行（全件を上表で処理）
grep -rn "索引"                       --include=*.md .   → 24 行（全件を検分）
grep -rn "PostToolUse\|post-edit\|selectChecks" -l （全拡張子） → 生きた層 20 ファイル
```
いずれも `workspace/`・`docs/superpowers/`・`.superpowers/`・`docs/adr/`（凍結）・`node_modules` を除外。
**「reminder」の grep が最も収量が高かった**（`docs/build-commands.md:37` の TS 型検査の行を除き、ほぼ全件が要訂正）——識別子 grep（`selectChecks` 等）では `CLAUDE.md:29`・`docs/build-commands.md:167`・`.claude/rules/governance-docs.md:23` のいずれも**取り逃がした**行があり、タスク指示の警告が実測で正しかった。

---

## 3. 壊してはならない既存の機構

| # | 機構 | 所在 | 壊れ方 |
|---|---|---|---|
| 3-1 | **「検査が走ったなら沈黙は合格」契約**（#471） | `.claude/hooks/post-edit.mjs:7-13` | reminder は `warnings` へ積むだけで exit code を動かしてはならない。gate 枝ならタイムアウト・起動失敗・出力溢れの 3 沈黙経路を新 id についても塞ぐ（`:414-425`） |
| 3-2 | **`registry.mjs` の throw** | `scripts/governance/registry.mjs:27-29` | 新 CLI を `checks/` へ置くと `id`/`run` 不在で throw し、**`governance:check` が丸ごと落ちる** |
| 3-3 | **facade の再輸出を増やさない**（#1094） | `scripts/governance-check.mjs:71-82` | `checks/` の関数を facade へ戻すと、そのファイルの消失検知が manifest 差分から import エラーへ戻る（#1092 の穴の再生） |
| 3-4 | **静的 import 禁止** | `.claude/hooks/post-edit.mjs:181-186`＋`post-edit.test.mjs:272` のカナリア＋`ADR-dependents-reminder-at-edit-time.md:18` | import 文は `try { main() } catch` の外で走る。解決に失敗すると **全 `Edit|Write` で hook がエンベロープごと沈黙する**（`.rs` の fmt/clippy/test を含む） |
| 3-5 | **`G-hook-fires` の 2 方向照合と `sawEmptyRow`** | `checks/G-hook-fires.mjs:103,107-116,125-127,128` | gate 枝で表を直さなければ CI が赤。**空集合の行を失うと「沈黙は合格ではない」の主張が表から消える** |
| 3-6 | **BUDGETS 完全性カナリア** | `.claude/hooks/post-edit.test.mjs:769-798` | gate 枝で `BUDGETS` を足し忘れると、**全検査が緑の間は沈黙し、最初の失敗で hook が TypeError で落ちる** |
| 3-7 | **repro の `/` 正規化（PreToolUse との相互契約・#768）** | `.claude/hooks/post-edit.mjs:326-333`／`post-edit.test.mjs:584-586` | gate 枝で `\` 区切りの repro を出すと、片方の hook が指示するコマンドをもう片方が拒む |
| 3-8 | **`governance:check` の「依存ゼロ・決定的」契約** | `scripts/governance-check.mjs:14-15` | 新 CLI も同じ制約下。`gitIgnoredPaths` は既に注入形になっている（`checks/G-references.mjs:9`）ので**注入の口を潰さない** |
| 3-9 | **CI の `governance check` step** | `.github/workflows/ci.yml:73-74`（`skip-ci` 非対象） | issue が明示している通り**外さない**。`AGENTS.md`「検証の層と、層と層の隙間」 |
| 3-10 | **`MODULE_INDEX_CRATES` の母集団カナリア**（#701） | `checks/G-module-index.mjs:19-33`＋`scripts/governance-check.test.mjs` | 新 CLI は `checkModuleIndex` をそのまま呼ぶことでこの保証を継承する。**crate 一覧を新 CLI 側へ写すと写しが 2 つになる** |
| 3-11 | **`isSourceFileWrite` の低頻度性の理由** | `.claude/hooks/post-edit.mjs:215-217`「Write に絞るのは、既存ファイルの Edit まで拾うと沈黙=合格を壊す頻度になるため」 | この判断は**新機構にも当たる**（→ §4-3） |
| 3-12 | **`governance-manifest` の 4 列** | `scripts/governance-manifest.mjs:23-33` | `scripts/governance/` 直下への新設は delta に現れない（`checks`/`docs`/`rules`/`skills` のみ）。**これは無害な事実だが、「manifest が承認を要求しなかった＝構造は変わっていない」と読んではならない** |

---

## 4. 実装上の落とし穴（構造的に踏みやすいもの）

### 4-1. 帰属フィルタの形が **trigger ごとに違う**（最大の罠）

`finding` は `{ file, line, message }`（`scripts/governance/lib.mjs:159`）で、**帰属先は「欠陥を報告する文書」であって「編集したファイル」ではない**。

- `G-references` の findings は `file = doc`（`checks/G-references.mjs:83`）→ **`.md` 編集時は `f.file === rel` で正しく絞れる**。
- `G-module-index` の findings は **両方向とも `file = <crate>/CLAUDE.md`**（`:67`, `:77`）。`.rs` を Write したときの当該 finding は `` 実ファイル src-tauri/src/foo.rs が索引（本文のバッククォート）に見当たらない `` で、**編集したパスは `message` の中にしかない**。

→ フィルタは **2 形**必要:
- `.md` 編集 → `f.file === rel`
- `.rs` Write → `f.message.includes(rel)`（basename ではなく**フルパス**で。basename 一致にすると `mod.rs` のような同名多発ファイルで誤爆する）

**フィルタを掛けないと**、無関係な編集のたびに既存の stale な findings が全部出る＝ゴム印化する（→ 4-4）。

### 4-2. 編集時と CI で**同じ検査が違う答えを出す**（沈黙の向き）

- 走査母集団 `makeSnapshot` は **`workspace/` を除外する**（`scripts/governance/lib.mjs:41`）。hook が触るファイルが `workspace/` 配下なら判定対象外——`/implement` の作業中は該当が多い。
- `G-references` の走査元 `governanceDocs`（`lib.mjs:461-470`）に**入らない `.md` がある**: `PERFORMANCE.md`・`RETROSPECTIVE.md`・`README.md`・`.claude/agents/*.md`・`docs/adr/**`・`docs/superpowers/**`。**これらを編集しても reminder は永遠に鳴らない**。「`.md` を編集すれば参照実在が見える」と書くと偽の全称になる（`AGENTS.md`「全称表現は前提条件とセットで書く」）。
- `G-module-index` は `MODULE_INDEX_CRATES` の 4 crate 限定（`:28-33`）。**crate 新設時は編集時も CI も同じく沈黙する**（既知の残余・#1008）。

### 4-3. 新規 `.rs` の Write では **reminder が必ず鳴る**

Write した瞬間、その `.rs` はまだ索引に無い。逆方向は**必ず** finding を出す。これは狙い通りの信号だが、

- **索引を直すまで、以降の `.rs` 編集のたびに鳴り続ける**（`.rs` の Edit まで trigger にした場合）。`isSourceFileWrite` が **Write に絞った理由**（`post-edit.mjs:215-217`）がそのまま当たる。
- **gate 枝ならこれが「検査失敗（exit N）」として赤くなる**。新規ファイルを書いた直後は**構造的に必ず赤**であり、**永久に赤いゲートはゲートが無いのと機能的に同じ**（`docs/development-principles.md`「判定を持たない道具を層に数えてよい」の注記が引く `ADR-declared-colors-over-modal-color`）。**gate 枝を採らない最強の理由がこれである。**

### 4-4. 慣れ（habituation）— 前倒しが `implement` の記述と衝突する

`.claude/skills/implement/SKILL.md:86` は「飛ばしやすいのは `cargo doc` と `npm run governance:check` の 2 つで、**hook の沈黙に慣れることが原因だった**」と書いている。**部分的な前倒しはこの機序を悪化させうる**——「hook が governance を見てくれる」と読めば、残る 17 検査（`cargo doc` を含む）を飛ばす動機が増える。訂正文には「**この reminder は `governance:check` の代わりにならない**」を必ず残す（`docs/build-commands.md:167` に既にその形の一文がある）。

### 4-5. 削除は届かない

`Edit|Write` matcher に**ファイル削除は届かない**（`.claude/hooks/post-edit.mjs:216-218` が既に明記）。ゆえに:
- `.rs` を消したときの索引の orphan（順方向の赤）は**編集時には見えない**。
- `.md` や `.rs` を消して**他の文書の参照が壊れた**場合も見えない（`G-references` は編集された文書**の中の**参照しか見ないので、帰属フィルタが構造的に落とす）。

→ **CI を外さない理由の実体がここにある。** 訂正文で「編集時が見るのは編集した文書に帰属する欠陥だけ」と射程を明示する。

### 4-6. 費用の測り直しは「実物」で

`ADR-dependents-reminder-at-edit-time.md:27-28` が実測している通り、**発火率の見積もりは合成フィクスチャで外れた**（24〜25% → 実測 55%）。「実物で走らせて初めて出た」欠陥がある。**`.md` を触った直近 N コミット / `.rs` を Write した直近 N コミットに対して、実際に何件出るかを実装後に測り直す**こと。

### 4-7. 相乗りするか、2 つ目の subprocess を立てるか

`.md` 編集ではすでに `dependents.mjs` が 1 プロセス起動している（node 起動 88〜130 ms）。**独立に 2 本目を立てると `.md` 編集の hook が 2 回の node 起動を払う**。判定コスト 28 ms に対し起動コストが 3〜4 倍なので、**1 本の CLI に畳む（`dependents.mjs` を含めて統合する／新 CLI から呼ぶ）検討をしないと、費用が構造的に無駄になる**。ただし畳むと `dependents.mjs` の「合否を持たない計器」という位置づけと新 CLI の責務が混ざるので、**責務分離とプロセス数のトレードオフを明示的に裁定する**必要がある。

### 4-8. `.rs` 編集経路へ費用を載せない

`dependentsReminder` は `rel.endsWith(".md")` を**関数の先頭で見て subprocess を起動しない**（`post-edit.mjs:194`、テスト `post-edit.test.mjs:235`「`.rs` 編集の経路に費用を載せない」）。新機構は `.rs` と `.md` の**両方**が trigger なので、この保護が薄れる。**`.rs` は Write のみ**に絞れば `.rs` Edit（最頻の操作）には費用が乗らない。

### 4-9. 変異注入の作法

`.claude/rules/safety-nets.md`「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」。加えて:
- **変異対象は他所から逐語で名指されていないものを選ぶ**（同ファイル）。新 id を逐語で持つファイル（`post-edit.test.mjs` の `REPRESENTATIVE_EDITS` 等）があると、消したときに別の理由で赤くなり「この層は黙っている」が観測できない。
- **複製で `npm test` を回すなら `node_modules` を junction で張る**（同ファイル）。張らないと vitest は起動前に落ち、**変異検知と終了コードが同じ**になる。

### 4-10. reminder は **人間向けチャネルにしか流れない**

`.claude/hooks/post-edit.mjs:308-310` が自ら書いている——「additionalContext が**エージェント向け**、systemMessage が**人間向け**」。そして `main()` は `const systemMessage = [...warnings, ...errors].join("\n")`（`:497`）で、**`warnings` を systemMessage 側にしか流していない**（`context` は `[...errors, ...sections]`・`:496`）。

つまり `dependentsReminder` 型に忠実に従うと、**新しい reminder はエージェントには届かない**。issue の動機は #629/#630＝**エージェントの索引更新漏れ**であり、行動すべき当人に届かない配線になりうる。既存の `isSourceFileWrite` WARN も同じ経路なので先例としては筋が通るが、これは §2-11 で引いた「**穴は層の内側ではなく境界に空く**」（`docs/development-principles.md:168`）がそのまま当たる形である——**出力が意図した消費者へ届いているかを、どの層も見ていない**。

裁定は本枠の仕事ではない。**問いとして名指しする**（→ §7-6）。

### 4-11. 新スクリプトの編集には hook が**何も走らない**

`scripts/governance/*.mjs` は `selectChecks` の割り当てが空（`docs/hooks.md:59`）。**新 CLI を書いている間、hook は一切鳴らない**。テスト（A4）を `npm test` で自分で回すこと。この非対称は「セーフティネットを作る作業自体はセーフティネットの外」という既知の形である。

---

## 5. 枝の判別子（裁定は本枠の仕事ではないが、材料を並べる）

| 判別子 | reminder 枝 | gate 枝 |
|---|---|---|
| 新規 `.rs` Write 直後の構造的な赤（4-3） | WARN で済む | **必ず赤**——ゲートとして機能しない |
| 帰属フィルタの失敗時の被害 | ノイズ | 誤った赤 |
| 凍結 worktree（B6） | `existsSync` で静かに何もしない | MODULE_NOT_FOUND が検査失敗に化ける |
| `sawEmptyRow` の玉突き（B5） | 起きない | 空集合代表パスの選び直しが要る |
| `docs/hooks.md` 発火一覧・BUDGETS・repro 正規化 | 不要 | 3 点すべて要る |
| 先例 | `ADR-dependents-reminder-at-edit-time`（ユーザー裁定済み・2026-08-19） | 無し |
| 「合否は CI が持つ」という issue の要求 | 素直に満たす | 合否が 2 か所に生まれる |

**判別子はすべて reminder 枝を指す。** ただし reminder 枝は「鳴っても人が読まなければ効かない」という #1140 と同じ残余を引き継ぐ——その残余は `ADR-retire-area-budget` が面積で採った形と同型であり、**このリポジトリが既に受容している判断の型**である。

---

## 6. 検証計画（実装後に何で測るか）

1. **配線の変異注入**: `main()` の `warnings.push` 2 行を削り、統合テスト（A10）が赤になることを確認。ユニットテストだけでは緑のままになる（`ADR-dependents-reminder-at-edit-time.md:30` の実測）。
2. **帰属フィルタの両方向**: (a) 索引に無い `.rs` を Write → 鳴る、(b) 索引に在る `.rs` を Write → 鳴らない、(c) **無関係な `.rs` を Write したときに他ファイルの stale finding が出ない**（不混入）。
3. **`G-references` 側**: (a) 壊れた参照を含む `.md` を編集 → 鳴る、(b) 同じ壊れた参照が**別の** `.md` に在るとき、こちらを編集しても出ない（帰属）、(c) `governanceDocs` 外の `.md`（例 `PERFORMANCE.md`）では鳴らない＝**射程の実測**。
4. **費用の実測**: `.md` 1 回編集・`.rs` 1 回 Write の hook 全体を計測し、`ADR-dependents-reminder-at-edit-time.md:29` の 330〜386 ms と比較する。
5. **発火率の実物測定**（4-6）: 直近 N コミットに対して回し、ノイズ率を出す。
6. **`npm run governance:check` と `npm test` の全体**: `governance:check` の検査数が変わっていないこと（`checks/` に何も足していないこと）を evidence 行で確認。
7. **PR 本文チェックリストへ送る項目**: CI の `governance-check` job が依然赤くなること（＝前倒ししても CI が外れていないこと）は **PR ができてからしか測れない**（`.claude/rules/safety-nets.md`「CI の実測は PR が在って初めて行える」）。

---

## 7. 未解決の問い（実装前に決めるべきこと）

1. **reminder / gate の裁定**（§5）。issue の「決めること」に列挙されていない第 3 の論点である。
2. **`isSourceFileWrite` を撤去するか残すか**（A7）。撤去しないと同じ trigger で 2 つの WARN が出る。
3. **`dependents.mjs` と 1 プロセスに畳むか**（4-7）。責務分離 vs 起動コスト 2 倍。
4. **`.rs` の trigger を Write だけにするか Edit も含めるか**（4-3 / 4-8）。改名・移動は Edit として現れうる。
6. **reminder をエージェントへ届けるか**（4-10）。`warnings` は systemMessage（人間向け）にしか流れない。届けるなら `additionalContext` 側にも積む配線が要り、そのとき「context は errors と sections だけ」という現在の意味づけを変えることになる。
5. **数え上げ散文の直し方**（2-1 / 2-5 / 2-6 / 2-7）。「2→3」で直すと次の追加でまた偽になる。**非列挙化を既定にする**——`.claude/rules/governance-docs.md:19` と #1091 の両方が同じ向きを指している。
