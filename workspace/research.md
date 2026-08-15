# 調査 — #1094 facade の再輸出を実際の消費者まで絞り、manifest 差分を load-bearing にする

## issue の要約

`scripts/governance-check.mjs`（facade）の `export { … }` は 56 名を再輸出している。これは検査の実装が facade に在り、テストが facade から import していた時代の遺物である。#1093 で 19 検査が `scripts/governance/checks/` へ移り、各検査のテストも隣へ移って自分の検査モジュールから直接 import するようになったため、再輸出の消費者は縮んでいる。

「掃除」以上の意味は次の 1 点にある。**再輸出は 19 本すべての検査モジュールを facade へ静的に名指し import させており、検査ファイルが物理的に消えると `buildChecks` へ到達する前に `ERR_MODULE_NOT_FOUND` で落ちる。** これが今日、消失を捕まえている一次防御線である。一方 #1092 の manifest 差分は、この静的 import が在る限り消失に対して発火する機会が無い。**再輸出を絞ると、manifest 差分が消失の検知器として初めて load-bearing になる。**

## 現在の消費者（着手時に測り直した — 2026-08-16）

`grep -rn 'governance-check\.mjs"' --include=*.mjs .` で facade から import している箇所を全走査した（node_modules 除外）。**静的 import 6 ファイル + 動的 import 2 箇所**である。

| ファイル | import する名前 |
|---|---|
| `scripts/governance-manifest.mjs:14` | `makeSnapshot` / `buildChecks` / `governanceDocs` |
| `scripts/governance-manifest.test.mjs:2` | `makeSnapshot` |
| `scripts/governance-check.test.mjs:8` | `MODULE_INDEX_CRATES` / `governanceDocs` / `makeSnapshot` / `runAll` / `buildChecks` |
| `scripts/governance-check.test.mjs:83`（動的） | `makeSnapshot` |
| `scripts/governance-check.test.mjs:98`（動的） | モジュール全体（公開面カナリア。`Object.keys(mod)` を凍結一覧と比較） |
| `scripts/governance/lib.test.mjs:11` | `runAll` |
| `scripts/governance/checks/G-references.test.mjs:4` | `buildChecks` |
| `scripts/governance/checks/G-stale-identifiers.test.mjs:5` | `runAll` / `buildChecks` |

**必要な公開面の和集合は 5 名**: `makeSnapshot` / `buildChecks` / `governanceDocs` / `runAll` / `MODULE_INDEX_CRATES`。
このうち `buildChecks` と `runAll` は facade 自身が `export function` で定義しているため、`export { … }` ブロックに要るのは **3 名**（`makeSnapshot` / `governanceDocs` / `MODULE_INDEX_CRATES`）である。

### issue の起票時観測との差分

issue は「`scripts/governance/checks/*.test.mjs` のうち **3 本**（配線カナリア）が `buildChecks` を呼ぶ」と書いているが、**実測は 2 本**である（`G-references.test.mjs` / `G-stale-identifiers.test.mjs`）。3 本目に見えるのは `scripts/governance/lib.test.mjs` だが、これは `checks/` 配下ではなく、また import するのは `runAll` であって `buildChecks` ではない。issue 自身が「この一覧は起票時点の観測であり、正本ではない。着手時に `grep` で数え直すこと」と指示しているとおりの差分であり、**計画は実測側を採る**。

## facade が「自分で使う」名前と「再輸出のためだけに import している」名前

`governance-check.mjs` の本体（`buildChecks` / `runAll` / CLI 起動）が実際に呼ぶのは次だけである。

- `./governance/registry.mjs` — `CHECK_MODULES`
- `./governance/lib.mjs` — `makeSnapshot` / `finding` / `gitIgnoredPaths` / `governanceDocs` / `headingRefDocs` / `headingRefSourceDocs` / `staleIdentifierDocs` / `staleIdentifierGuideDocs` / `staleIdentifierTargets` / `workspaceMembers`
- `./governance/instrument.mjs` — `checkNormativeAreaInstrument` / `normativeArea`
- `./governance/checks/G-clippy-disallowed.mjs` — `clippyDisallowedCount`（evidence 行）
- `./governance/checks/G-adr-file-names.mjs` — `adrFiles`（evidence 行）

**残りの 19 検査モジュールからの import（`checkArchitectureTable` ほか）は、すべて再輸出のためだけに存在する。** `buildChecks` は `CHECK_MODULES.map(...)` で走査由来の登録表から検査を組むため、検査関数を名指しで持つ必要が無い（`scripts/governance-check.mjs:183`）。

## 制約（この変更の射程 — 全称にできない点）

### 制約 1: 静的 import は 3 本残る（うち 1 本は facade の外の経路）

**3b で 1 本追加された。** `grep -rn 'from ".*checks/G-' --include=*.mjs scripts/ .claude/ .githooks/ | grep -v '\.test\.mjs:'` の全走査で、facade 以外に検査モジュールを静的 import する非テストファイルが 1 本だけ在る。

```
scripts/governance/instrument.mjs:6:import { skillFiles, modelHiddenSkills } from "./checks/G-skill-table.mjs";
```

facade は `instrument.mjs` を（`checkNormativeAreaInstrument` / `normativeArea` のために）import するので、**`G-skill-table.mjs` は再輸出を絞っても推移的に静的 import され続ける。この経路は facade の `export { … }` とは無関係であり、この PR が触る面の外にある。**

したがって絞った後も import エラーで落ちるのは次の 3 本である。

| ファイル | 経路 | 理由 |
|---|---|---|
| `G-clippy-disallowed.mjs` | facade 直接（`clippyDisallowedCount`） | evidence 行 |
| `G-adr-file-names.mjs` | facade 直接（`adrFiles`） | evidence 行 |
| `G-skill-table.mjs` | facade → `instrument.mjs`（`skillFiles` / `modelHiddenSkills`） | 計器の算出 |

前 2 本は意図的な設計であり、facade の該当箇所（`scripts/governance-check.mjs:66-67`）に「evidence 専用の導出は、その検査のファイルから名指しで取る。**登録行と違い、ファイルが消えれば import が失敗して鳴る**」と書かれている。3 本目（`instrument.mjs` 経由）はその設計の射程外で、単に計器が `G-skill-table` の導出を再利用しているだけである。**この PR で外すことは検討しない**——外すには `skillFiles` / `modelHiddenSkills` を複製するか `lib.mjs` へ移す必要があり、issue の射程を超える。

帰結:

- 切り替わりの対象は **19 本のうち 16 本**である。
- 実測で消すのはその 16 本のいずれかでなければならない。
- 文書へ書く主張は**下限**の形にする。「検査ファイルの消失は manifest 差分として現れる」を全称で書くと、この 3 本について偽になる（ルート `CLAUDE.md`「検証の作法」の全称表現ルール。`universal-claim-fix-regenerates-itself` の再演を避ける）。

### 制約 1b: 切り替わるのは「ペア消失」だけである（3b の最大の所見）

**3b が、issue と当初の research.md の両方が持っていなかった条件を暴いた。** #1093 の per-check 分割で、19 本すべての `G-X.test.mjs` が隣の `G-X.mjs` を静的 import している（19/19 を実測。`for f in scripts/governance/checks/*.test.mjs; do grep -q "from \"./$b.mjs\"" ...` が全件 OK）。そして `vitest.config.ts` の `include` は `scripts/**/*.test.mjs` を含む。

ゆえに**検査モジュールの消失は、消え方によって捕まる層が違う**。

| 消え方 | 絞る前 | 絞った後 |
|---|---|---|
| `G-X.mjs` だけ消える（テストは残る） | facade の import エラー + `npm test` の import エラー | **`npm test` の import エラー**（facade とは無関係に残る） |
| `G-X.mjs` と `G-X.test.mjs` がペアで消える | **facade の import エラー**（`governance:check` step が赤） | **manifest 差分だけ**（`npm test` も `governance:check` も緑） |

**issue の主張が真になるのは下段（ペア消失）だけである。** そして検査を 1 本やめるという実際の操作はペア消失の形を取るので、**これは主要な脅威に対して真である**——ただし「検査ファイルが消えれば manifest 差分が捕まえる」と無条件に書くと、上段について偽になる。

この 2 つの条件（16/19 かつペア消失）を掛けると、この PR が manifest 差分へ移す保証は次の形になる。

> **`checks/` の検査モジュールが隣のテストごと消えたとき、16 本については manifest 差分が唯一の検知器になる。**

### 制約 1c: 検知器の「強さ」も変わる（層をまたいだ移動）

`docs/development-principles.md`「検証の層と、層と層の隙間」の枠で見ると、これは同じ強さの検知器への付け替えではない。

- **今日**: `ERR_MODULE_NOT_FOUND`。`.github/workflows/ci.yml` の `governance check` step は `push` でも `pull_request` でも走り、**宣言では回避できない**ハードエラーである。
- **絞った後**: `governance manifest delta` step。`if: github.event_name == 'pull_request'` を持ち **PR でしか走らず**、しかも**差分を PR 本文へ逐語で書けば通る**ゲートである（`scripts/governance-manifest.mjs` の `undeclared`）。

つまり「**不可能にする**」から「**意図的だと宣言させる**」への移行である。#1092 の設計意図そのものではあるが（「差分がそのまま承認の材料になる」）、**下がる側の面**を書かずに「守りが増えた」とだけ書けば嘘になる。

### 制約 2: この変更自体は manifest 差分の対象外

`scripts/governance-manifest.mjs` の `manifest()` が返す 4 列は `checks` / `docs` / `rules` / `skills` であって、export 名は含まない。**公開面の縮小は manifest に一切現れず、PR 本文の delta 宣言も不要である。** 見張りは `scripts/governance-check.test.mjs` の公開面カナリア（`Object.keys(mod).sort()` の凍結一覧）ただ 1 つになる。issue の注意書きどおり、**凍結を外すのではなく、同じ差分で一覧を縮める。**

### 制約 3: `registry.mjs` の走査元は `import.meta.url` 起点、`makeSnapshot` は cwd 起点

`scripts/governance/registry.mjs:12` は `CHECKS_DIR` を `import.meta.url` から解決する（cwd 起点にすると #1092 H1 型の「読むコードと読む木がずれる」が起きるため意図的）。一方 `makeSnapshot(process.cwd())` は cwd を見る。**使い捨てコピーで実測するときは、コピー側のスクリプトを cwd = コピーで走らせなければ混成ツリーになる。**

## 再利用できる既存パターン

- **フォールトインジェクションは複製へ当てる**（`.claude/rules/safety-nets.md`「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」）。稼働中の `checks/` は触らない。`git worktree` でスクラッチパッドへ使い捨てを作る。
- **公開面の凍結カナリア**は `scripts/governance-check.test.mjs:91-157` にある形をそのまま維持し、配列だけ縮める（sorted equality）。
- **検査 ID の走査由来登録**（`registry.mjs`）は既に消失に対して沈黙する。だからこそ manifest 差分が引き受けられる。

## 影響を受ける散文（同じ差分で直す必要がある 3 箇所）

いずれも「静的再輸出が今日の一次防御線である」という**この PR が偽にする**前提を書いている。

1. `scripts/governance-check.mjs:91-92` — 再輸出ブロックの導入コメント（「既存の import 元（`governance-manifest.mjs` と `governance-check.test.mjs`）を壊さないための再輸出」）。消費者が 6 ファイルへ広がっている点も含めて現状へ合わせる。
2. `scripts/governance-check.test.mjs:92-96` — 公開面カナリアの根拠コメント（「per-check 分割で検査を 1 本 `checks/` へ移すたびに書き足す唯一の面」）。絞った後は「移すたびに書き足す」面ではなくなる。
3. `scripts/governance-manifest.test.mjs:64-74` — **最重要**。「facade は各検査を静的に名指し re-export しているため……それが今日の一次防御線であり、この diff より先に、より大きな音で発火する」「効いてくるのは facade が検査ごとの静的 re-export をやめた後」と、未来形で書かれている。**この PR がその「後」である。** 現在形へ書き換え、制約 1（evidence 経由の 2 本は例外）を同じ場所へ書く。

## 影響を受けない散文（確認済み）

- `.claude/skills/health-check/references/mechanized-checks.md:9` / `SKILL.md:23` — 「`governance-check.mjs` は re-export のみ」と書いている。`governanceDocs` / `MODULE_INDEX_CRATES` はどちらも絞った後の公開面に残るため、**真のまま**。
- **G-stale-identifiers は鳴らない。** 現行語彙は production ソース（`.mjs` 含む・`.test.mjs` 除く）の非コメント本文から作られる（`scripts/governance/checks/G-stale-identifiers.mjs:143-157`）。`checkArchitectureTable` などの識別子は `checks/` 配下の定義として語彙に残り続けるので、facade から再輸出を消しても散文側の言及が腐り扱いにならない。

## 技術的制約

- facade の契約「依存ゼロ・決定的」は変わらない。import を減らすだけである。
- `export { … }` を `export *` にしない契約（`scripts/governance-check.mjs:92`）は維持する。
- 実 CI の赤は PR が在って初めて測れる（`.claude/rules/safety-nets.md`「CI の実測は PR が在って初めて行える」）。ローカルの比較経路シミュレーションが計画内の証拠であり、実 CI の確認は **PR 本文のチェックリスト**へ送る（#858 / #749）。

## 未解決の疑問（計画の未確定欄で潰す）

1. `MODULE_INDEX_CRATES` を facade に残すか、`governance-check.test.mjs` を `G-module-index.mjs` からの直接 import へ付け替えるか。
2. 使い捨てコピーでの実測が**両側**で成立するか——(a) 削除後に `node scripts/governance-check.mjs` が緑（旧防御線が消えたことの証拠）、(b) `governance-manifest.mjs --compare` が exit 1 で `-G-ci-table` を出す（新検知器が発火することの証拠）。**(a) を省くと「差分が出た」しか示せず、load-bearing になったことの証拠にならない。**

## 敵対的調査（3b）の結果

`general-purpose` / `model: sonnet` を 1 体。出力は `workspace/adversarial-1094.txt`。使い捨て `git worktree` でフォールトインジェクションを実施し、稼働中の `checks/` は触れていない（実施後に撤去済み・リポジトリは untracked な `workspace/adversarial-1094.txt` のみ）。

### 壊せた項目（採用 3 件）

| 所見 | 一次証拠（メイン側で再実測） | 採否 |
|---|---|---|
| 「絞った後に静的 import が残るのは 2 本」は偽。`instrument.mjs:6` が `G-skill-table.mjs` を facade と独立の経路で import している | `grep -rn 'from ".*checks/G-' --include=*.mjs scripts/ .claude/ .githooks/ \| grep -v '\.test\.mjs:'` が facade 20 行 + `instrument.mjs:6` の 1 行を返す | **採用**。制約 1 を 2 本 → 3 本へ訂正 |
| 19 本すべての `G-X.test.mjs` が隣の `G-X.mjs` を静的 import しており、`vitest.config.ts` の `include` が `scripts/**/*.test.mjs` を含むため、**`npm test` が facade と無関係に消失を捕まえている**（今日も、絞った後も） | 19/19 を実測（全件 OK）。`vitest.config.ts:8-12` に `"scripts/**/*.test.mjs"` | **採用**。制約 1b を新設 |
| ゆえに `G-ci-table` を消して測る計画は、`.mjs` だけ消す形では切り替わりを示せない | 上の 2 件の帰結 | **採用（ただし機序は裁定し直した）**。下記 |

### 機序の裁定（採るのは所見であって説明ではない）

3b は「manifest 差分は一般には初めて load-bearing になるのではない」と一般化した。**所見は正しいが、この一般化はこの PR の射程を過小に述べている。**

3b が測ったのは `G-ci-table.mjs` **だけ**を消した形である。しかし検査を 1 本やめるという実際の操作は `.mjs` と `.test.mjs` のペア消失であり、その形では:

- **絞る前**: facade の静的 import が `governance:check` を赤にする（`npm test` は緑——テストごと消えているので import 元が無い）。
- **絞った後**: `governance:check` も `npm test` も緑になり、**manifest 差分だけが残る**。

したがって issue の主張は**ペア消失について真**であり、3b が示したのは「単独消失という別の消え方には独立の防御が在る」ことである。両者は両立する。**計画は 3b の所見を採り、機序は「消え方で場合分けする」形へ書き直した**（制約 1b の表）。

### 実測の設計への反映（3b の指摘が計画を変えた点）

1. フォールトインジェクションは **`.mjs` と `.test.mjs` をペアで消す**。単独消失では切り替わりを示せない。
2. 複製での確認に **`npm test` も加える**——`governance:check` が緑なだけでは「他の層も黙っている」ことの証拠にならない。
3. 対照として **単独消失（`.mjs` のみ）も測り、`npm test` が赤になることを確かめる**。これが制約 1b の表の上段の裏取りであり、同時に「絞った後も単独消失は捕まる」という下限主張の根拠になる。
4. 複製の起動形は **複製へ `cd` してから相対パスでスクリプトを叩く**（絶対パス起動だと `registry.mjs` の `import.meta.url` 起点と `makeSnapshot` の cwd 起点がずれ、#1092 H1 型の混成ツリーになる）。

### 壊せなかった項目（研究の主張が持ちこたえた 6 件）

- facade の外部消費者は 6 ファイル（静的）+ 2 箇所（動的）で全部である。`.mjs` / `.ts` / `.js` / `.json` / `.yml` / `.ps1` / `.cjs` を横断して検算し、取りこぼしなし（`package.json` と `ci.yml` に在るのは CLI 起動の参照、`.claude/hooks/*.mjs` に在るのはコメントでの言及のみ）。
- `manifest()` の 4 列に export 名は混入しない（`governance-manifest.mjs:23-33`。`checks` は `buildChecks(...).id` = `registry.mjs` のディレクトリ走査由来）。
- G-stale-identifiers は絞っても鳴らない（`VOCAB_SOURCE_EXT` が `.mjs` を含み `VOCAB_TEST_FILE` が除くのは `.test.mjs` だけ）。**他のどの検査も facade の import 行・export 行・行数を見ていない。**
- `.claude/skills/health-check/` の 2 箇所は、未確定 #1 をどちらへ倒しても真のまま。
- CI の `governance manifest delta` step とローカル複製のシミュレーションは cwd 整合である（`ci.yml:111-129` を読み、複製で再現）。
- 現状の `node scripts/governance-check.mjs` と `npm test` はどちらも緑（research.md が測っていなかった項目）。

### ⚠️ 確信の持てない所見（メイン側で裁定済み）

| ⚠️ 所見 | 裁定 |
|---|---|
| `.claude/hooks/post-edit.mjs` の `selectChecks` が facade の構造に紐付いた分岐を持つかもしれない（未読） | **否定**。`selectChecks`（`post-edit.mjs:125-170`）を全文実測。分岐は `.rs` / `Cargo.toml` / `tauri.conf.json`・`config.toml` / `CHECK_DEFINITION` / `.claude/hooks/` / `.githooks/` / `.claude/lsp/`・`rust-analyzer.toml` のみで、facade への言及は無い |
| PostToolUse hook が `checks/*.mjs` の編集で vitest を走らせるかもしれない | **否定**。`selectChecks` は `scripts/**` に検査を割り当てない。**このタスクでは hook の沈黙は「何も走らなかった」を意味する**（`CLAUDE.md`「フック」・#497）——`npm test` と `governance:check` は毎回手動で回す |
| 未確定 #1 の倒し方が `SKILL.md:23` から読者が引く**含意**を変えるかもしれない（字面は両方とも真） | **影響なし**。当該行が名指すのは `MODULE_INDEX_CRATES` の**中身**（載っている crate だけが照合される）と、それを守る `governance-check.test.mjs` の母集団カナリアである。カナリアはどちらの倒し方でも同じファイルに残り、import 元が変わるだけである |
