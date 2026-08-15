# 独立導出レビュー — #1094

> 本レビューは `workspace/plan.md` / `workspace/research.md` / `workspace/adversarial-1094.txt` を読まずに、
> issue 本文とコードだけから再導出したものである。実測はすべて使い捨ての git worktree
> （`scratchpad/wt-a`・作業後に `git worktree remove --force` 済み）で行い、稼働中の
> `scripts/governance/checks/` には一切変異を当てていない。

## 対象 issue
#1094

---

## 独立に導出した「残すべき公開面」

**結論: 4 名。`buildChecks` / `runAll` / `makeSnapshot` / `governanceDocs`。**
（うち `buildChecks`・`runAll` は facade 自身の定義であり再輸出ではない。実際に痩せるのは
re-export ブロックで、56 名 → **2 名**〔`makeSnapshot`・`governanceDocs`、いずれも `lib.mjs` 由来〕になる。）

### 消費者の全走査（`.mjs` / `.ts` / `.js` / `.json` / `.yml` / `.ps1` / `.claude/`・動的 import 含む）

`grep -rn "governance-check\.mjs\"" --include={*.mjs,*.js,*.ts,*.json,*.yml,*.ps1}` の全出力：

| 消費者 | file:line | 取っている名前 |
|---|---|---|
| manifest 本体 | `scripts/governance-manifest.mjs:14` | `makeSnapshot` / `buildChecks` / `governanceDocs` |
| manifest テスト | `scripts/governance-manifest.test.mjs:2` | `makeSnapshot` |
| facade テスト（静的） | `scripts/governance-check.test.mjs:8` | `MODULE_INDEX_CRATES` / `governanceDocs` / `makeSnapshot` / `runAll` / `buildChecks` |
| facade テスト（**動的**） | `scripts/governance-check.test.mjs:83` | `await import(...)` → `makeSnapshot` のみ |
| facade テスト（**動的**） | `scripts/governance-check.test.mjs:98` | `await import(...)` → `Object.keys(mod)`（カナリア自身。名前を指定しない） |
| lib テスト | `scripts/governance/lib.test.mjs:11` | `runAll` |
| 配線カナリア | `scripts/governance/checks/G-references.test.mjs:4` | `buildChecks` |
| 配線カナリア | `scripts/governance/checks/G-stale-identifiers.test.mjs:5` | `runAll` / `buildChecks` |

**import ではない出現（消費者ではない）**:
- `package.json:11` `"governance:check": "node scripts/governance-check.mjs"` — CLI 起動のみ
- `.github/workflows/ci.yml:74` `run: node scripts/governance-check.mjs` — CLI 起動のみ
- `scripts/governance/checks/G-build-commands.test.mjs:6` / `G-rules-globs.test.mjs:13` — フィクスチャ文字列の中のパス
- `scripts/governance/lib.mjs` 内の言及はコメント

**`.ps1` / `.ts` / `.claude/` に消費者は無い。** 56 名それぞれについて `git grep -- <name> -- :!scripts/ :!workspace/ :!.superpowers/ :!docs/superpowers/` を回した（実行スクリプトは
`scratchpad/scan-names.mjs`）。ヒットは**すべて散文**（`.md` と `src-tauri/clippy.toml` のコメント）で、
コードとしての消費者はゼロ。

### `MODULE_INDEX_CRATES` を公開面から外す根拠

これは **`checks/` 配下のモジュール（`G-module-index.mjs`）由来の唯一の再輸出**であり、
残すと facade の静的 import が 1 本残って #1094 の狙いが崩れる。**消費者は
`governance-check.test.mjs:8` の 1 か所だけ**なので、そこを

```js
import { MODULE_INDEX_CRATES } from "./governance/checks/G-module-index.mjs";
```

へ付け替える。`checks/*.test.mjs` の 19 本すべてが既にこの形（自分の検査モジュールから直接 import）を
採っており、様式は揃う。**実測: この付け替えを含む完全版を使い捨て worktree へ当て、
`npx vitest run` が `Test Files 31 passed / Tests 721 passed` で通ることを確認した**
（変異なしのベースラインも同じ 31 / 721）。

### issue 本文の訂正（数え直しの結果）

> 「`scripts/governance/checks/*.test.mjs` のうち **3 本**（配線カナリア）が `buildChecks` を呼ぶ」

**実測は 2 本**（`G-references.test.mjs:4` / `G-stale-identifiers.test.mjs:5`）。3 本目に見えていたのは
`scripts/governance/lib.test.mjs:11` で、これは `checks/` 配下ではなく、取っている名前も
`buildChecks` ではなく `runAll` である。issue 自身が「着手時に数え直すこと」と書いている通りの結果。

---

## 独立に導出した変更ファイル・シンボル一覧

### 必須

| # | ファイル | 対象シンボル・行 | 内容 |
|---|---|---|---|
| 1 | `scripts/governance-check.mjs` | 41〜68 行（`checks/` からの静的 import 20 行） | **全削除**。`registry.mjs` の走査だけが検査を組む |
| 2 | 〃 | 66〜67 行のコメント（「evidence 専用の導出は…ファイルが消えれば import が失敗して鳴る」） | **偽になる**。削除ないし書き換え |
| 3 | 〃 | 91〜92 行のコメント（「既存の import 元を壊さないための再輸出」） | 消費者 4 名へ縮んだ事実へ書き換え |
| 4 | 〃 | 93〜148 行 `export { … }`（56 名） | `export { makeSnapshot, governanceDocs };` へ |
| 5 | 〃 | 209 行 evidence 中の `clippyDisallowedCount(snapshot)` / `adrFiles(snapshot).length` | ctx sink 経由（`ctx.clippy` / `ctx.adrFiles`）へ。**この 2 つが静的 import の最後の砦**（下記「要対処 A」） |
| 6 | `scripts/governance/checks/G-clippy-disallowed.mjs` | 7〜9 行 `run()` | `ctx.record("clippy", { findings: checkClippyDisallowed(snapshot), checked: clippyDisallowedCount(snapshot) })` |
| 7 | `scripts/governance/checks/G-adr-file-names.mjs` | 7〜9 行 `run()` | `ctx.record("adrFiles", { findings: checkAdrFileNames(snapshot), checked: adrFiles(snapshot).length })` |
| 8 | `scripts/governance-check.test.mjs` | 8 行 | `MODULE_INDEX_CRATES` を `./governance/checks/G-module-index.mjs` から取る 1 行へ分離 |
| 9 | 〃 | 99〜156 行の凍結配列 | `["buildChecks", "governanceDocs", "makeSnapshot", "runAll"]` へ**同じ差分で**更新（凍結は外さない・issue の注意 (1)） |
| 10 | 〃 | 91〜96 行の describe コメント | 「per-check 分割で検査を 1 本移すたびに書き足す唯一の面」という前提が消える。役割を「消失検知が manifest へ移った後、公開面の変更を意図的な編集に留める」へ書き換え |
| 11 | `scripts/governance-manifest.test.mjs` | 64〜74 行のコメント | **明示的に「効いてくるのは facade が検査ごとの静的 re-export をやめた後」と書いてある**。切り替わった今の状態へ書き換える（この PR がその「後」である） |

### 判断が要る（要対処 B）

| # | ファイル | 対象 | 内容 |
|---|---|---|---|
| 12 | `scripts/governance/instrument.mjs` | 6 行 `import { skillFiles, modelHiddenSkills } from "./checks/G-skill-table.mjs";` | **facade を完全に痩せさせても、ここ経由で `G-skill-table.mjs` だけは静的 import が残る**（実測 H）。`skillFiles` / `modelHiddenSkills` を `lib.mjs` へ移すか、残余として明記するかの判断が要る |

### 散文（下記「偽になる散文」に詳細）

13. `docs/build-commands.md:30`
14. `src-tauri/clippy.toml:2` および `:50`
15. `.claude/skills/health-check/SKILL.md:23`
16. `.claude/skills/health-check/references/mechanized-checks.md:9`（条件つき）

---

## 独立に導出した「偽になる散文」

概念ラベル（`再輸出` / `re-export` / `公開面` / `一次防御線` / `静的 import` / `ERR_MODULE_NOT_FOUND`）と
識別子の両方で走査した。

### 確実に偽になる

| file:line | 現在の記述 | なぜ偽になるか |
|---|---|---|
| `scripts/governance-check.mjs:66-67` | 「evidence 専用の導出は、その検査のファイルから名指しで取る。**登録行と違い、ファイルが消えれば import が失敗して鳴る**」 | 名指しの静的 import をやめるので、鳴らなくなる。この一文が **#1094 が壊す機構そのものの説明** |
| `scripts/governance-check.mjs:91-92` | 「既存の import 元（`governance-manifest.mjs` と `governance-check.test.mjs`）を壊さないための再輸出」 | 消費者は 4 名へ縮み、`governance-check.test.mjs` は `MODULE_INDEX_CRATES` を facade から取らなくなる |
| `scripts/governance-check.test.mjs:92-96` | 「per-check 分割で検査を 1 本 `checks/` へ移すたびに書き足す唯一の面」 | 移送は終わり、面は 4 名で固定される |
| `scripts/governance-manifest.test.mjs:65-74` | 「facade は各検査を…静的に名指し re-export しているため…**それが今日の一次防御線**であり、この diff より先に、より大きな音で発火する」「効いてくるのは facade が検査ごとの静的 re-export をやめた**後**」 | この PR がその「後」を作る。**この段落は、この PR の差分で状態を反転させることを自分で予告している** |
| `docs/build-commands.md:30` | 「**名指しの母集団は `scripts/governance-check.mjs` の `REQUIRED_DISALLOWED_METHODS` である**」 | #1093 で宣言は `scripts/governance/checks/G-clippy-disallowed.mjs` へ移っており、facade には再輸出しか無い（既に所在として不正確）。再輸出が消えると facade に痕跡がゼロになる |
| `src-tauri/clippy.toml:2` | 「群を足すときは `scripts/governance-check.mjs` の `REQUIRED_DISALLOWED_METHODS` へも登録すること」 | 同上。**しかもこれは規範の手順書**であり、指す先が空になると次の実装者が登録先を見つけられない |
| `src-tauri/clippy.toml:50` | 「G-clippy-disallowed が見るのは `REQUIRED_DISALLOWED_METHODS` が名指す件の在否であって、…」 | 同上（所在の明示は 2 行目だけなので、こちらは軽微） |
| `.claude/skills/health-check/SKILL.md:23` | 「per-check 分割・#1088 でここへ移った。**旧 `scripts/governance-check.mjs` は facade として re-export するのみ**」 | 直前で `MODULE_INDEX_CRATES` を facade 由来のように読ませている。付け替え後は `G-module-index.mjs` が唯一の口になる |

**注意: `governance:check` はこれらを 1 件も捕まえない。** `REQUIRED_DISALLOWED_METHODS` という識別子は
`G-clippy-disallowed.mjs` に在り続けるので G-stale-identifiers の語彙から消えず、
`scripts/governance-check.mjs` というパスも実在し続けるので G-references も緑のままである。
**所在が偽になる形は機械検査の射程外**——手で直すしかない。

### 条件つき（残す公開面の選び方で変わる）

| file:line | 記述 | 判定 |
|---|---|---|
| `.claude/skills/health-check/references/mechanized-checks.md:9` | 「走査元の正本は `scripts/governance/lib.mjs` の `governanceDocs()`（`governance-check.mjs` は **re-export のみ**・#1088）」 | 本レビューの案（`governanceDocs` を残す）なら**真のまま**。`governance-manifest.mjs` を `lib.mjs` 直取りへ変えて facade から `governanceDocs` を外す案を採るなら偽になる |

### 更新不要と判定したもの

- `docs/adr/ADR-*.md`（`REQUIRED_DISALLOWED_METHODS` 等を語る 10 件超）— **凍結された歴史**（`ADR-adr-frozen-history`・`.claude/rules/governance-docs.md`「ガバナンス文書の参照と命名のルール」）。`governanceDocs` の母集団からも `docs/adr/` は除外されている（`scripts/governance/lib.mjs:161`）
- `docs/superpowers/plans/2026-08-15-governance-check-per-check-split.md` — 履歴資料（`lib.mjs:161` で除外）
- `AGENTS.md` / ルート `CLAUDE.md` — 56 名のいずれも現れない（scan-names.mjs 実測）

---

## 切り替わりの実測設計（自分の案）と、実行結果

### 設計

消したいのは「**import エラーで落ちる**」から「**manifest 差分で赤くなる**」への切り替わりであり、
対照は 2 種類要る。

- **対照 A（今日の姿）**: 縮小前の複製で `checks/` から 1 本消し、`node scripts/governance-check.mjs` が
  `ERR_MODULE_NOT_FOUND` で落ちること。**同時に `governance-manifest.mjs` も落ちること**を測る
  ——manifest は facade を import するので、今日は「差分を出す機会が無い」のではなく
  **manifest 自身が起動しない**。
- **対照 B（常に赤いゲートでないこと）**: 縮小後・変異なしで `--compare` が「差分なし」を返すこと。
- **処置**: 縮小後に検査**とそのテストの両方**を消す（テストだけ残すと vitest の import 失敗が
  manifest 層の測定を汚す）。`governance:check` が **exit 0・検査 18 件**で緑になり、
  `--compare` が `-G-<id>` で赤くなり、PR 本文に `-G-<id>` を書けば緑へ戻ること。

削除対象は **`G-ci-table`（`governance-manifest.test.mjs:78` が名指す）だけでは足りない**——
それはローカルの `npm test` に別の口を持つので、**名指しの無い検査（例: `G-hook-fires`）でも測る**。

### 実行結果（すべて使い捨て worktree で実測）

| ラベル | 状態 | 結果 |
|---|---|---|
| A-0 | 現行 HEAD・変異なし | `governance:check — 全検査 passed（検査 19 件 …）` |
| **A-pre** | 現行 facade + `G-ci-table.mjs` 削除 | `governance-check.mjs` → `ERR_MODULE_NOT_FOUND`。**`governance-manifest.mjs --compare` も同じ例外で落ちる**（差分を出す前に死ぬ） |
| B-0 | 完全縮小・変異なし | `検査 19 件 … clippy 禁止 8 件 … ADR 50 本` — **evidence 文字列は 1 文字も変わらない**（record sink 化しても値が保たれることの実測） |
| **CONTROL B** | 完全縮小・変異なし・`--compare` | `governance manifest — 構造母集団に差分なし`（exit 0）。**この PR 自身が manifest delta を生まないことの実測でもある** |
| **B-1** | 完全縮小 + `G-ci-table` を検査ごと削除 | `governance:check — 全検査 passed（検査 18 件 …）` **exit 0**（＝この緑こそ manifest が埋める穴） |
| **B-2** | 同上・`PR_BODY="宣言のない PR 本文"` | `PR 本文で宣言されていない差分 1 件: -G-ci-table` **exit 1** |
| **B-3** | 同上・`PR_BODY` に `-G-ci-table` を含む | `差分 1 件はすべて PR 本文で宣言済み: -G-ci-table` exit 0 |
| **C-1/C-2** | **部分縮小**（export 面だけ削り evidence の静的 import を残す）+ `G-clippy-disallowed` 削除 | `governance-check.mjs` も `governance-manifest.mjs` も **`ERR_MODULE_NOT_FOUND`**。→ **export 面を空にするだけでは切り替わらない**（要対処 A の一次証拠） |
| **H** | **完全縮小**後に `G-skill-table` を削除 | `ERR_MODULE_NOT_FOUND`（imported from **`scripts/governance/instrument.mjs`**）。→ facade を痩せさせても 1 本だけ静的 import が残る（要対処 B の一次証拠） |

**「切り替わった」と言える条件**: A-pre が赤（import エラー）→ B-1 が緑（exit 0・18 件）→ B-2 が赤
（manifest 差分）→ B-3 が緑（宣言で解除）、かつ CONTROL B が緑。**この 5 点すべてを実測した。**

再現手順（使い捨て複製の作り方は `.claude/rules/safety-nets.md`「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」に従うこと）:

```
git worktree add --detach <scratchpad>/wt-a HEAD
cd <scratchpad>/wt-a && node scripts/governance-manifest.mjs > <scratchpad>/base.json
# 縮小を当てる → 検査 1 本とそのテストを rm
node scripts/governance-check.mjs                       # 緑・件数が 1 減る
PR_BODY="宣言なし" node scripts/governance-manifest.mjs --compare <scratchpad>/base.json  # exit 1
```

**CI での実測は PR が在って初めて行える**（`ci.yml:112` の `if: github.event_name == 'pull_request'`）。
上のローカル実測は CI が実行するのと同じスクリプト・同じ引数だが、**base worktree の作り方
（`ci.yml:122` の `git worktree add --detach /tmp/gov-base "$BASE_SHA"`）までは再現していない**。
PR 本文のチェックリストへ「governance-check job が実際に走ったこと」の確認を 1 行置くこと
（`.claude/rules/safety-nets.md`「検出器のカバー範囲は、欠落のパターンごとに検算する」）。

---

## 守りが弱くなる面の判定

### 縮小後に「検査モジュールの消失」を捕まえうる層（全列挙）

| 層 | 実体 | 射程 | 走る場所 |
|---|---|---|---|
| L1 | 各検査の隣の `*.test.mjs` が自分の検査を import | 19 本すべて。**ただしテストを道連れに消せば消える** | `npm test`（ローカル + CI） |
| L2 | `scripts/governance/lib.test.mjs:7-10` が 4 本を直接 import | `G-heading-refs` / `G-near-heading-refs` / `G-references` / `G-adr-citations` | `npm test` |
| L3 | `scripts/governance/instrument.mjs:6` が `G-skill-table.mjs` を import | `G-skill-table` のみ。**import エラーとして残る**（実測 H） | すべての経路 |
| L4 | `governance-check.test.mjs:8`（付け替え後）が `G-module-index.mjs` を import | `G-module-index` のみ | `npm test` |
| L5 | `governance-manifest.test.mjs:19` が `"G-references"` を、`:78` が `"G-ci-table"` を逐語で名指す | この 2 本のみ | `npm test` |
| L6 | 配線カナリア（`G-references.test.mjs:110` / `G-stale-identifiers.test.mjs:258`） | 自分の検査のみ（道連れに消える） | `npm test` |
| L7 | **manifest 差分**（`ci.yml:111-129`） | 19 本すべて。ただし PR 本文に `-G-<id>` と書けば通る | **PR CI のみ**（`push` では走らない・`ci.yml:112`） |

### 「捕まえられなくなる形」は在るか

**在る。ただし沈黙ではなく、「検知の場所と強さが変わる」形である。**

- **今日**: 検査ファイルを 1 本消すと `node scripts/governance-check.mjs` が **どの経路でも即座に**
  `ERR_MODULE_NOT_FOUND` で落ちる。ローカルでも CI でも push でも。**宣言では解除できない。**
- **縮小後**: 検査とそのテストを同じコミットで消すと、L1〜L6 に口を持たない **12 本**は
  ローカルの `npm test` も `npm run governance:check` も**完全に緑**になる
  （実測 E: `G-hook-fires` を検査ごと削除 → `Test Files 30 passed / Tests 698 passed`、
  `governance:check — 全検査 passed（検査 18 件 …）` exit 0）。捕まえるのは L7 だけで、
  それは **PR CI 限定**であり、**PR 本文に `-G-hook-fires` と 1 行書けば緑になる**。

**L1〜L6 に口を持つ 7 本**: `G-skill-table`（L3）・`G-heading-refs`・`G-near-heading-refs`・
`G-references`・`G-adr-citations`（L2）・`G-module-index`（L4）・`G-ci-table`（L5）。
**残る 12 本**: `G-adr-file-names`・`G-architecture-table`・`G-build-commands`・
`G-check-skill-enumeration`・`G-clippy-disallowed`・`G-hook-commands`・`G-hook-fires`・
`G-module-linkage`・`G-rules-globs`・`G-spec-sections`・`G-stale-identifiers`・`G-workspace-lints`。

**判定**:
1. **「誰にも捕まらない沈黙」は残らない** — main への直接 push は GitHub ruleset が拒む（ルート
   `CLAUDE.md`「Git/GitHub 運用」）ので、変更は必ず PR を通り、L7 が走る。
2. **ただし性質は変わる**: 「どこでも・大音量で・解除不能」から「**PR CI でだけ・宣言で解除可能**」へ。
   これは #1092 の設計意図そのもの（消失を事故ではなく承認事項にする）であり、**受容する残余として
   PR 本文へ明記すべき**トレードである。
3. **`push` イベントでは L7 が走らない**（`ci.yml:112`）。ruleset が在る限り到達しないが、
   「ruleset が緩む」「fork の main」といった前提の外では 12 本が無防備になる。**規範の前提に
   ruleset がぶら下がる**ことを PR 本文へ書くこと。
4. **`検査 19 件` という件数を凍結しているテストは無い** — `governance-check.test.mjs:74-78` は
   `buildChecks` から取った `ids.length` を evidence と突き合わせる自己参照であり、
   件数が 19→18 へ落ちても永久に緑である（実測 B-1）。この穴は縮小の有無に関わらず既存だが、
   縮小後は**唯一の件数の見張りが L7 になる**ので、性質が変わる。

---

## 文書更新の要否 / manifest delta 宣言の要否

### `SPEC.md`
**不要。** `SPEC.md` は製品の挙動の意図を持つ文書であり、開発ツールの内部構造は射程外。
`SPEC.md` 中に 56 名のいずれも現れない（scan-names.mjs 実測）。

### `AGENTS.md` / ルート `CLAUDE.md`
**不要。** 両者に 56 名のいずれも現れない。`AGENTS.md:65` は `npm run governance:check` という
コマンド名を指すだけで、facade の構造には触れていない。

### `.claude/skills/`
**要**（上記「偽になる散文」の通り）:
- `.claude/skills/health-check/SKILL.md:23` — `MODULE_INDEX_CRATES` の所在
- `.claude/skills/health-check/references/mechanized-checks.md:9` — 条件つき

### `docs/`・その他
**要**: `docs/build-commands.md:30`、`src-tauri/clippy.toml:2`（`:50` は軽微）。

### `.claude/rules/safety-nets.md`
**不要と判定。** 「検出器のカバー範囲は、欠落のパターンごとに検算する」の条項は今回の変更で
偽にならない（むしろこの変更がその条項の適用例になる）。**ただし本 PR の実測結果は
`RETROSPECTIVE.md` ではなく PR 本文へ**（`AGENTS.md`「RETROSPECTIVE.md の運用」の不変条件）。

### manifest delta 宣言（PR 本文の `+X` / `-X`）

**不要。実測済み。**

`manifest()` が返す 4 列は `scripts/governance-manifest.mjs:23-33`:

```js
checks: buildChecks(snapshot, {}).map((c) => c.id).sort(),
docs:   [...governanceDocs(snapshot)].sort(),
rules:  files(/^\.claude\/rules\/[^/]+\.md$/),
skills: files(/^\.claude\/skills\/[^/]+\/SKILL\.md$/),
```

この PR は **検査 ID を 1 つも増減させず**（`checks/` のファイル集合が不変）、
**`docs/` / `.claude/rules/` / `.claude/skills/` のファイル集合も増減させない**
（`docs/build-commands.md` と `health-check` の 2 枚は既存ファイルの編集）。
issue の注意 (2) の通り、**公開面（export 名）は manifest の 4 列のどれにも現れない**。

**実測（CONTROL B）**: 完全縮小を当てた worktree で
`node scripts/governance-manifest.mjs --compare base.json` → `構造母集団に差分なし`（exit 0）。

⚠️ **`workspace/` 配下の成果物は manifest にも `governanceDocs` にも入らない**
（`lib.mjs:161` の `docs/` 限定・`headingRefDocs` は `workspace/` を明示除外）。本レビュー文書を
置いても delta は生じない。

---

## 要対処

**A. export ブロックを空にするだけでは切り替わらない（一次証拠あり）。**
`scripts/governance-check.mjs:209` の evidence が `clippyDisallowedCount(snapshot)`（G-clippy-disallowed）と
`adrFiles(snapshot).length`（G-adr-file-names）を呼び、66〜68 行の**意図的な**コメントが
「evidence 専用の導出はその検査のファイルから名指しで取る」と宣言している。
実測 C-1/C-2: export 面だけ削って evidence の import を残した版で `G-clippy-disallowed.mjs` を消すと、
`governance-check.mjs` も `governance-manifest.mjs` も `ERR_MODULE_NOT_FOUND` のまま。
→ **19 本中 2 本は切り替わらず、issue の狙い（manifest を load-bearing にする）が部分的にしか達成されない。**
2 つの値を `ctx.record` 経由（既存の `ctx.headingRefs` / `ctx.adrCitations` と同型）へ移すこと。
移した版で evidence 文字列が一字一句同じになることは実測済み（B-0）。

**B. `scripts/governance/instrument.mjs:6` が `G-skill-table.mjs` を静的 import しており、facade を
完全に痩せさせてもこの 1 本だけ import エラーが残る**（実測 H）。
issue 本文はこの経路に触れていない。`skillFiles` / `modelHiddenSkills` を `lib.mjs` へ移すか、
**「19 本中 18 本が切り替わり、`G-skill-table` だけは import ガードが残る」と明記して受容する**か、
どちらかを**この PR で決める**こと。黙って残すと、次の人が「全 19 本が manifest で守られている」と
読む（全称の嘘・`AGENTS.md`「検証の作法（全タスク共通）」）。

**C. `ctx.record` 化は「evidence に `undefined` が出るのに exit 0」という沈黙経路を新設する。**
実測 F: facade が `ctx.clippy` / `ctx.adrFiles` を読むのに検査側の `record` 呼び出しが無い状態で、
`governance:check` は
`… clippy 禁止 undefined 件 … ADR undefined 本の名前 …` を印字して **exit 0**、
`npx vitest run` も **31 files / 721 tests 全緑**。
今日の `clippyDisallowedCount(snapshot)` は `undefined` になりえない形なので、**これは縮小が持ち込む
新しい沈黙**である（同型の穴は `ctx.headingRefs` 等で既存だが、増やす側に回る）。
`governance-check.test.mjs` へ **「evidence に `undefined` が含まれない」1 行のカナリア**を足すこと
（`.claude/rules/safety-nets.md`「これまで無意味だった状態に意味を与える変更は、その状態に到達する全経路を列挙する」）。

**D. 「切り替わりの実測」の削除対象に `G-ci-table` を選んではならない。**
`governance-manifest.test.mjs:78` がその ID を逐語で名指しているため、削除すると `npm test` が
別の理由で赤くなり（実測 D-2）、「ローカルは緑・manifest だけが赤」という切り替わりの核心が
測れない。**名指しの無い検査（実測では `G-hook-fires`）を使うこと。**

**E. `src-tauri/clippy.toml:2` は規範の手順書であり、指す先が空になる。**
「群を足すときは `scripts/governance-check.mjs` の `REQUIRED_DISALLOWED_METHODS` へも登録すること」は
再輸出が消えると宛先を失う。**`governance:check` はこの腐りを捕まえない**（識別子は
`G-clippy-disallowed.mjs` に在り続け、パスも実在し続けるため G-stale-identifiers も G-references も緑）。
同じ差分で `scripts/governance/checks/G-clippy-disallowed.mjs` へ書き換えること。

---

## 軽微

- **`governance-manifest.mjs:14` を `lib.mjs` 直取りへ変える案**（`makeSnapshot` / `governanceDocs` を
  `./governance/lib.mjs` から取り、facade の公開面を `buildChecks` / `runAll` の 2 名まで削る）。
  狙いには影響しない（`lib.mjs` は検査ではない）。**採るなら `mechanized-checks.md:9` の
  「`governance-check.mjs` は re-export のみ」が偽になる**ので同じ差分で直すこと。
  本レビューは差分の小ささを優先して**採らない**案を測った（721 テスト緑）。
- `scripts/governance-check.mjs:14-15` の契約行「facade だけでなく `checks/` 配下の各検査…も含む全層が
  同じ制約を負う」は真のまま。19〜23 行の登録の記述も真のまま。**触らないこと。**
- 検査件数（19）を凍結する見張りがローカルに無い（`governance-check.test.mjs:74-78` は自己参照）。
  L7 が持つので新設は不要と判断するが、**PR 本文へ「件数の見張りは PR CI だけになった」と書く**価値がある。
- `scripts/governance-check.mjs` の編集に **PostToolUse hook は 1 本も割り当てられていない**
  （`.claude/hooks/post-edit.mjs` の `selectChecks`・134〜168 行を実読。`scripts/` は `.githooks/` でも
  `.claude/hooks/` でも `CHECK_DEFINITION` でもない）。**沈黙は「何も走らなかった」である**——
  実装者の検証は `npm test` と上記の使い捨て複製での実測が全部である。

---

## 未検証

- **CI 上での実際の切り替わり**（`ci.yml:111-129` の `governance manifest delta` step が
  `git worktree add --detach /tmp/gov-base "$BASE_SHA"` 経由で base 側を走らせる経路）。
  ローカル実測は同じスクリプト・同じ `--compare` 引数だが、base worktree の作り方と
  `gh pr view --json body` の取得は再現していない。**`ci.yml` は `pull_request` でのみ起動する**ため
  PR が在って初めて測れる（`.claude/rules/safety-nets.md`「検出器のカバー範囲は、欠落のパターンごとに検算する」）。
  → PR 本文のチェックリストへ送ること。
- **`.superpowers/` 配下**（`git grep` の除外に入れた）。作業記録であり実行されるコードではないと
  判断したが、全文は見ていない。
- **`vitest.config.ts` の include 外にあるテスト**は存在しないと仮定した（`include` は
  `.claude/hooks/**` `.githooks/**` `scripts/**` の 3 つ・`vitest.config.ts` 実読）。
  この 3 つの外に `*.test.mjs` が在れば消費者を数え落とす可能性がある。
- **`REQUIRED_DISALLOWED_METHODS` 以外の 51 名について、散文中の「所在」記述の網羅性**。
  識別子名での `git grep` は全 56 名に対して回したが、「facade が持っている」ことを
  **名前を出さずに**書いた散文（例:「`governance-check.mjs` が全部を持つ」）は捕まえていない。
  概念ラベル 6 種での走査で補ったが、全称は主張しない。
