# #1155 — 撤去した錨の層の語彙の独立導出（コードからの再導出）

作業者: 独立導出エージェント / 2026-08-20 / 対象 HEAD `585c0991`
一次資料は `gh issue view 1155` と `git show 74ae45fc` / `git show 74ae45fc^:<path>` のみ。`workspace/` 配下は本ファイル以外読んでいない（例外は「確信の持てない点」に開示する）。

## 導出した母集団

issue が挙げた 5 語（`ドメイン` / `錨` / `ctx.domains` / `domains.test.mjs` / `G-domain-anchors`）は使わず、撤去コミット自身から探索軸を立て直した。

**軸の立て方**: `git show --diff-filter=D --name-only 74ae45fc` で消えた 4 ファイル、`git show 74ae45fc -- <path>` で各ファイルから消えた識別子、`git show 74ae45fc^:scripts/governance/domains.mjs | grep -n "name:"` で 17 のドメイン名を取った。

| # | 軸 | 実際に打ったコマンド | 件数 |
|---|---|---|---|
| A | 削除されたファイル名 | `git grep -n -E "domains\.(test\.)?mjs\|G-domain-anchors" -- ':!workspace/'` \| `wc -l` | **63 行 / 9 ファイル**（うち生きた層は 4 行 / 4 ファイル。残りは `docs/adr/` と `docs/superpowers/` `docs/design/`） |
| B | 削除された識別子 | `git grep -n -E "DOMAIN_SPECS\|buildDomains\|duplicateDomains\|dupDomains\|META_CHECK_IDS\|unmigrated\|ctx\.domains\|sink\.domains" -- ':!workspace/'` \| `wc -l` | **65 行 / 7 ファイル**（生きた層は `G-adr-citations.mjs:33` の `ctx.domains` 1 件と ADR の `META_CHECK_IDS` 2 件だけ） |
| C | 日本語の語彙 `錨` | `git grep -l "錨" -- ':!workspace/'` \| `wc -l` | **22 ファイル**。うち `docs/design/` `docs/superpowers/` を除くと 10 ファイル |
| D | 日本語の語彙 `ドメイン` | `git grep -n "ドメイン" -- ':!workspace/' ':!docs/design/' ':!docs/superpowers/'` \| `wc -l` | **20 行**（うち `snotra-core/src/search.rs:43` は無関係の語義） |
| E | 撤去で変わった件数の主張 | `git grep -n -E "検査 ?(20\|21) ?(本\|件)\|21 本" -- ':!workspace/' ':!docs/superpowers/' ':!docs/design/'` \| `wc -l` | **10 行** |
| F | 撤去語彙の統合走査（scripts/ 全体） | `git grep -n -E "錨\|ドメイン\|\bdomains\b\|G-domain-anchors\|DOMAIN_SPECS\|buildDomains\|duplicateDomains\|META_CHECK_IDS\|unmigrated\|domains\.mjs\|domains\.test\.mjs" -- scripts/` | **33 行 / 10 ファイル** |

**軸 F の内訳**（`cut -d: -f1 | sort | uniq -c` で実測）:

```
      4 scripts/governance-check.mjs
      4 scripts/governance/checks/G-adr-citations.mjs
      4 scripts/governance/checks/G-check-skill-enumeration.mjs
      1 scripts/governance/checks/G-folded-heading-refs.test.mjs
      3 scripts/governance/checks/G-module-index.mjs
      1 scripts/governance/checks/G-module-linkage.mjs
      9 scripts/governance/checks/G-rules-script-coverage.mjs
      5 scripts/governance/lib.mjs
      1 scripts/governance/lib.test.mjs
      1 scripts/governance/registry.mjs
```

### issue の「31 出現 / 9 ファイル」との突き合わせ

**一致しない。差分は 2 方向あり、どちらも説明が付く。**

- **issue に無くて私に在る**: `scripts/governance/checks/G-folded-heading-refs.test.mjs`。`git log --oneline -1 -- <path>` で **`585c0991`（#1156）が追加**したファイルであり、issue の起草時点には存在しなかった。issue の母集団は 1 コミット分古い。
- **issue に在って私が「無関係」に分類する**: `scripts/governance/lib.test.mjs`。このファイルの唯一のヒットは `:310` の「生成物の除外は**ルート錨止め**である」で、`錨` の別語義（anchoring at root）である。同型が `lib.mjs:13` / `lib.mjs:35` にもあり、生きた層では**計 3 行が語義違いの誤検出**である。

**方法論の注意（自分が踏んだ穴）**: 最初 `git grep -- 'scripts/**/*.mjs'` で数えたところ **29 行 / 9 ファイル**が出て、ルート直下の `scripts/governance-check.mjs`（4 行）が丸ごと落ちた。この git の pathspec では `scripts/**/*.mjs` が 0 段のディレクトリに当たらない。`-- scripts/` へ替えて 33 行になった。**issue の 31 という数も、この階級の除外で説明が付く範囲にある。**

## 分類

判定軸は 3 つに割った——**(1) 撤去を正しく描写している / (2) 撤去されたものが在る前提で書いている＝偽 / (3) 撤去された層の語彙だけが残骸として残っているが、文の実質は今も真**。issue が求めた 2 分類に (3) を足したのは、`ドメイン` という語だけが残った箇所が実際に 8 件あり、「偽」と同列に扱うと直しの優先度と直し方が変わるためである。

### (2) 偽 — 消えた保証を在るかのように書いている

| # | file:line | 何が偽か | 根拠（file:line） |
|---|---|---|---|
| 偽1 | `scripts/governance/lib.mjs:498` | 「縛られている向きは `domains.test.mjs`「moduleIndexSources は crateSources の部分集合」が持つ」——**そのファイルは存在しない**。直前の `:496-497` が「同じ SSOT から導いてはならない」と書いており、その但し書きを支える検知器が今日ゼロ | 削除は `git show --diff-filter=D --name-only 74ae45fc` に `scripts/governance/domains.test.mjs` |
| 偽2 | `scripts/governance/checks/G-module-index.mjs:25` と `:28` | 「`domains.test.mjs` が本表由来の母集団が member の `src/` の外へ出ないことを見る」＋「**錨も部分集合テストも #701 のカナリアも赤にしない**」——数えている 3 機構のうち 2 つが撤去済み。残るのは #701 のカナリア 1 本 | カナリアの実体は `scripts/governance-check.test.mjs:19-49`（実読。`MODULE_INDEX_CRATES` と `governanceDocs` の両方に載ることだけを固定し、`src/` の外へ出ないことは見ていない） |
| 偽3 | `scripts/governance/checks/G-adr-citations.mjs:33` | `@param` が「`run` は `ctx.domains` 経由で渡す」と書くが、`run` は `adrFiles(snapshot)` を直接渡す。`ctx` に `domains` は無い | `G-adr-citations.mjs:9` の `run` 本体、`scripts/governance-check.mjs:145` の `const ctx = { docs, allRefDocs, staleTargets, gitIgnoredPaths, record }` |
| **偽4（issue に無い）** | `scripts/governance/checks/G-rules-script-coverage.mjs:33-40` | `//!` ヘッダが「`judgingScripts` **ドメインの錨**（Phase 2）が果たす役割は違う」と現在形で書き、「`SCRIPT_EXT` を `.mjs` へ狭める → `governance:check` の**実行時**に鳴るのは錨である。錨の無い版は exit 0 で完全に沈黙し」と続ける。**今日は「錨の無い版」しか無い**ので、読者は「実行時に鳴る」を今日の保証と読む。同じファイルの `:49` は正しく「錨の層は撤去した」と書いており、**1 ファイル内で自己矛盾している** | `git show 74ae45fc^:scripts/governance/checks/G-rules-script-coverage.mjs \| sed -n '28,42p'` で **:32-40 が撤去前と 1 バイトも変わっていない**ことを実測。撤去コミットは同ファイルの `:49`（`judgingScripts` の doc）だけを直した |
| **偽5（issue に無い）** | `scripts/governance/checks/G-check-skill-enumeration.mjs:70` | 「ドメインを見ていれば、同じ走査の欠落を**錨が名指しで鳴らす**」——鳴らす錨は存在しない。この文は `skillDocs` を照合先に選んだ理由の中核であり、理由が消えたまま残っている | `git show 74ae45fc -- scripts/governance/checks/G-check-skill-enumeration.mjs` の差分は `export const domains` 1 行の削除のみ（コメントは無傷） |
| **偽6（issue に無い）** | `scripts/governance/checks/G-adr-citations.mjs:51` | 「**`references/` の腕には錨を置いていない**——…（宣言する死角）」。**置いていない腕の存在を語ることが、置いてある腕の存在を含意する**。今日どの腕にも錨は無い | 同上（`domains.mjs` 削除） |
| **偽7（issue に無い・`.mjs` の外）** | `docs/adr/ADR-governance-meta-demotion.md:3` と `:61` | 「格下げ後の姿は `scripts/governance-check.mjs` の **`META_CHECK_IDS`** と `metaAuditEnabled` が持つ」「その項目だけを **`META_CHECK_IDS`** から外してゲートへ戻す」——`META_CHECK_IDS` は `74ae45fc` が削除した。**手順として実行不能である** | `git grep -n "META_CHECK_IDS" -- ':!workspace/'` の結果は当該 ADR の 2 行のみ（実装側 0 件）。削除は `git show 74ae45fc -- scripts/governance-check.mjs` の `-const META_CHECK_IDS = new Set(["G-domain-anchors"]);` |

**偽7 の凍結歴史との緊張**: `docs/adr/ADR-adr-frozen-history.md`「決定」は ADR→生きた層の腐りを**設計どおりの残余**とする。それでも直す側へ倒す理由は 2 つある。(a) `.claude/skills/health-check/SKILL.md`（最終行）が「判定規則（撤去 / 復帰）の正本は `docs/adr/ADR-governance-meta-demotion.md`「戻す条件・撤去する条件」である」と**生きた手順の SSOT として委譲している**——凍結された記述ではなく現役の運用指示である。(b) 同 ADR は `74ae45fc` 自身が `:51-55` に追記を足しており、**このサイクルで既に生きた文書として扱われている**。ゆえに本文の書き換えではなく、**日付つきの追記**（`74ae45fc` が使ったのと同じ形）で「格下げ後の姿を持つのは `metaAuditEnabled`（`scripts/governance-check.mjs:110`）と `runAll` の `metaFindings` であり、`META_CHECK_IDS` は撤去済みで、格下げ側へ入る検査は 1 本も無い」を足す。

### (3) 語彙の残骸 — 文の実質は真。触るなら「ドメイン」を落とすだけ

| file:line | 記述 | 判定 |
|---|---|---|
| `scripts/governance/lib.mjs:489` | 「`ruleDocs` ドメインのメンバー。」 | 残骸。`ruleDocs` は今も存在する関数で、返すものの説明は真 |
| `scripts/governance/lib.mjs:494` | 「`crateSources` ドメインのメンバー——workspace member の `src/` 配下の `.rs`。」 | 残骸。ただし**同じ doc の `:498` が偽1** なので同じ差分で触る |
| `scripts/governance/checks/G-adr-citations.mjs:47` | 「`skillTreeDocs` ドメインのメンバー」 | 残骸 |
| `scripts/governance/checks/G-adr-citations.mjs:57` | 「`nonDocSources` ドメインのメンバー」 | 残骸 |
| `scripts/governance/checks/G-module-index.mjs:37` | 「`moduleIndexSources` ドメインのメンバー」 | 残骸 |
| `scripts/governance/checks/G-module-linkage.mjs:200` | 「除外は `crateSources` ドメインの側ではなくここに置く」 | 残骸。実質（除外はこの検査に置く）は今も真 |
| `scripts/governance/checks/G-rules-script-coverage.mjs:61` | 「述語が…**ドメインのメンバーに当たる**」 | 残骸。実質（`narrow` は `judgingScripts` のメンバーに当たる）は真 |
| `scripts/governance/checks/G-check-skill-enumeration.mjs:68,72,73` | 「照合先は `skillDocs` ドメインである」「ドメインに無いを実在しないと言ってはならない」 | 残骸。ただし **`:70` が偽5** なので同じ差分で触る |

### (1) 撤去を正しく描写している — 触らない

`scripts/governance-check.mjs:25,26,95,168` / `scripts/governance/registry.mjs:5` / `scripts/governance/checks/G-rules-script-coverage.mjs:49` / `.claude/skills/health-check/SKILL.md:78` / `docs/adr/ADR-governance-anchor-layer-discarded.md`（全 11 行）/ `docs/adr/ADR-governance-meta-demotion.md:51-55`（追記節）。

### 語義違い — 触らない

`scripts/governance/lib.mjs:13` / `:35`（「PATHS の照合は…**ルート錨止め**」）、`scripts/governance/lib.test.mjs:310`（同）、`docs/comment-guidelines.md:108`（「両者の共通の**錨**は `font_covers_cjk`」）、`docs/adr/ADR-race-check-predicate-and-norm-hardening.md:12`、`docs/adr/ADR-source-text-probe-helper-locality.md:9`、`snotra-core/src/search.rs:43`（「マッチ方式のドメイン enum」）。

### 判断を保留した 1 件

`scripts/governance/checks/G-folded-heading-refs.test.mjs:83`:
```js
expect(run("`domains.test.mjs`\n「moduleIndexSources は crateSources の部分集合」\n")).toEqual([]);
```
**散文ではなく負例フィクスチャであり、偽の主張ではない**（`.mjs` が対象綴りでないことを固定する正しいテストである）。ただし**「決めること」3 と直結する**——`.mjs` を対象綴りに足すと**このテストが赤になる**。触らない側へ倒すが、決定 3 の費用として名指しする。

### 射程外の確認（触らない根拠を実測した）

- `docs/superpowers/plans/2026-08-19-governance-domains-phase{1,2,3}.md`（錨 29 + 8 + 34 件）: `makeSnapshot` に問うと `governanceDocs=false` / `allHeadingRefDocs=false` / `staleIdentifierTargets=false`。#589 の非規範化どおり全母集団の外。
- `docs/design/2026-08-20-governance-meta-demotion-derivations.md`（錨 12 件）: **`governanceDocs=true` / `allHeadingRefDocs=true` / `staleIdentifierTargets=true`**（実測）。**機構の定義では生きた層である。**それでも触らない——冒頭 `:3` が「ここに置くのは、その結論を出した 2 つの導出の**原文**である」と宣言し、`docs/adr/ADR-governance-anchor-layer-discarded.md:42` が「**Fable は錨を「原則が許す唯一の 1 段」として残す側に置いた**——その判断は今回の二択で覆っている」と**外側から訂正済み**だからである。書き換えれば当時を偽る。

## 変更が要るファイルとシンボル

| ファイル | シンボル / 位置 | 変更 |
|---|---|---|
| `scripts/governance/lib.mjs` | `crateSourceFiles` の doc（`:494-498`） | 偽1。「縛られている向きは `domains.test.mjs`…が持つ」を削り、**縛る機構が今日ゼロであること**を書く。`:494` の「ドメイン」も同時に落とす |
| `scripts/governance/lib.mjs` | `ruleDocs` の doc（`:489`） | 残骸。「ドメインのメンバー」→「`.claude/rules/` 直下の md」 |
| `scripts/governance/checks/G-module-index.mjs` | `MODULE_INDEX_CRATES` の doc（`:25-29`） | 偽2。「`npm test` の #701 カナリア（`scripts/governance-check.test.mjs`）1 本だけが、`CLAUDE.md` を持つ member が本表と `governanceDocs()` の両方に載ることを固定する」へ書き換え、**`exts` / `excludeTest` の縮小を見る層は今日存在しない**を残余として明記する（3 → 1 ではなく、残余の記述が変わる） |
| `scripts/governance/checks/G-module-index.mjs` | `moduleIndexSources` の doc（`:37`） | 残骸 |
| `scripts/governance/checks/G-adr-citations.mjs` | `adrCitationDocs` の `@param`（`:33-34`） | 偽3。「`run` は `adrFiles(snapshot)` を直接渡す」へ |
| `scripts/governance/checks/G-adr-citations.mjs` | `skillTreeDocs` の doc（`:47`・`:51`） | 偽6 + 残骸。`:51` は「`references/` の腕には錨を置いていない」を落とし、**単一ファイルの母集団は下界を持てない**という残った論点だけを残す |
| `scripts/governance/checks/G-adr-citations.mjs` | `nonDocSources` の doc（`:57`） | 残骸 |
| `scripts/governance/checks/G-rules-script-coverage.mjs` | ファイル `//!` ヘッダ（`:32-40`） | **偽4。最大の書き換え**。「2 つの狭窄で錨の役割が違う」節を、`:49` と整合する形へ落とす——`SCRIPT_EXT` を狭める形は `npm test` の「母集団の下界」だけが赤にし、`governance:check` は exit 0 で沈黙する（＝A/B の「錨の無い版」が今日の姿である）。`WALK_EXCLUDE_PATHS` 側は錨と無関係に赤いので事実として残せる |
| `scripts/governance/checks/G-rules-script-coverage.mjs` | `COVERAGE` の doc（`:61`） | 残骸 |
| `scripts/governance/checks/G-check-skill-enumeration.mjs` | 照合先の選定コメント（`:68-73`） | 偽5。`:70` の「錨が名指しで鳴らす」を落とし、**`skillDocs` を照合先にする理由を錨に依存しない形**（原因から遠い赤を避ける、の一点）で書き直す |
| `scripts/governance/checks/G-module-linkage.mjs` | `checkModuleLinkage` 内コメント（`:200`） | 残骸 |
| `docs/adr/ADR-governance-meta-demotion.md` | `:3` / `:61`（`META_CHECK_IDS`） | 偽7。**本文を書き換えず、日付つき追記**を足す（`74ae45fc` が `:51-55` で使った形） |
| `AGENTS.md` | 「条件別チェック（トリガー → 参照先）」表 | 決定 4。下記の行を足す |

**触らない**: `scripts/governance-check.mjs`・`scripts/governance/registry.mjs`・`.claude/skills/health-check/SKILL.md`・`docs/adr/ADR-governance-anchor-layer-discarded.md`・`docs/design/`・`docs/superpowers/`・`scripts/governance/checks/G-folded-heading-refs.test.mjs`・`scripts/governance/lib.test.mjs`。

## 捕まえられなかった機構と理由

**4 つの層すべてが、別々の理由で構造的に届かない。** 実装から特定した。

### 1. `G-heading-refs` / `G-near-heading-refs` / `G-folded-heading-refs` — 対象の綴りで弾かれ、照合が**生成されない**

`scripts/governance/lib.mjs:180`:
```js
export const isRefTargetSpelling = (target) => target.endsWith(".md") || /^\/[a-z0-9-]+$/.test(target);
```
3 消費者（`G-heading-refs.mjs:49` / `G-near-heading-refs.mjs:68` / `G-folded-heading-refs.mjs:75,82`）と `dependents.mjs:43` はすべてこの述語で `continue` する。`checked` すら進まないので、**「照合 322 件」という証跡にも現れない**。

`lib.mjs` の部品を import した読み取り専用の計器で実測（走査元 260 文書）: `isRefTargetSpelling` に落ちた正準形は **10 件**、うち `.mjs` 対象が **3 件**——
```
scripts/governance/checks/G-rules-script-coverage.mjs:25  `G-rules-script-coverage.test.mjs`「母集団の下界」
scripts/governance/lib.mjs:498                            `domains.test.mjs`「moduleIndexSources は crateSources の部分集合」
scripts/governance-check.test.mjs:118                     `governance/evidence.test.mjs`「配線:」
```
さらに `resolveRefTarget`（`lib.mjs:422`）が `if (!target.endsWith(".md")) return null;` で二重に閉じている。**述語を 1 つ緩めても届かない設計になっている。**

### 2. `G-references` — 母集団に `.mjs` が無く、かつベア名は述語の外

- 母集団は `ctx.docs` = `governanceDocs`。`makeSnapshot` に問うと **36 件すべて `.md`** で、`scripts/governance/lib.mjs` は `governanceDocs=false`（実測）。
- 仮に入れても `G-references.mjs:69` の `if (!t.includes("/")) continue;` が **`` `domains.test.mjs` `` を落とす**（同ファイル `:20` が「ベア名（`SPEC.md` 等）は構造的に対象外」と自称している）。

### 3. `G-stale-identifiers` — **母集団に入っている唯一の文書でも、313/338 行がフェンスの内側**

issue は「母集団は `.md` だけ」で説明を止めているが、実測するともう一段深い。

- `staleIdentifierTargets(snapshot)` は **34 件で `.mjs` は 0 件**（実測）——ここまでは issue と同じ。
- **しかし `docs/design/2026-08-20-governance-meta-demotion-derivations.md` は母集団に入っている**（`staleIdentifierTargets=true`・実測）。この文書は `DOMAIN_SPECS` 5 行 / `buildDomains` 3 行 / `duplicateDomains` 4 行 / `G-domain-anchors` 5 行 / `domains.mjs` 7 行を持つ。
- そして撤去した識別子は**現行語彙に 1 つも無い**（`currentVocabulary` へ当てて実測: `DOMAIN_SPECS` `buildDomains` `duplicateDomains` `checkDomainAnchors` `META_CHECK_IDS` `unmigrated` `dupDomains` すべて `false`）。**判定は成立するはずだった。**
- 鳴らなかった理由は `scanStaleIdentifiers`（`G-stale-identifiers.mjs`）が `linesOutsideFences` を通すためである。**実測: 全 338 行のうちフェンス外は 25 行だけ**で、上記の識別子は**フェンス外 0 行 / バッククォート囲み 1〜2 行**。この文書単体で走らせると **照合 0 件 / finding 0 件**。

  → **「原文を `text` フェンスで貼る」形の設計文書は、`governanceDocs` に入っていながら全散文検査から実質的に不可視である。** これは #1155 固有ではなく、母集団に居ることと検められることの乖離である。

### 4. `governance manifest` — 列に `scripts/` のファイルが 1 つも無い

`scripts/governance-manifest.mjs:36` の `export const KEYS = ["checks", "docs", "rules", "skills"];` と `manifest()`（`:25-33`）が実体で、`docs` は `governanceDocs`（`.md` のみ）、`checks` は `buildChecks` の id。**`scripts/governance/domains.mjs` / `domains.test.mjs` の削除はどの列にも現れない。**現れたのは `-G-domain-anchors` だけで、それは「検査が 1 本消えた」ことしか言わない。

### 5. 撤去コミット自身の検証の母集団が狭かった（気づく契機がここに在った）

`74ae45fc` のコミットメッセージは「**撤去した識別子の残存 0 件**」と書いている。**コードの識別子については真である**（`git grep DOMAIN_SPECS -- scripts/` は 0 件）。偽なのは**コメント散文の語彙と、コメント内のファイル名**で、そこに 33 行が残った。同じコミットが `G-rules-script-coverage.mjs:49` と `G-adr-citations.mjs:7` の doc は直しており、**直したのは差分が触った行の隣だけ**である（`:32-40` と `:33` は撤去前と 1 バイトも変わっていない・実測）。

## 「決めること」4 点への回答

### 1. 33 出現（issue は 31）をトリアージする

**やる。**上の「分類」がその結果である。**ただし 2 分類ではなく 3 分類にする**——「撤去を描写」7 行・「偽」7 件・「語彙の残骸」8 行・「語義違い」3 行・「保留（負例フィクスチャ）」1 行。残骸を偽と同列に置くと、直しが**一括置換に化けて `judgingScripts` や `moduleIndexSources` の説明まで削れる**（それらは今も実在する関数で、doc の実質は真である）。

### 2. 偽を直す（issue の 3 件 → 私の 7 件）

**やる。issue の 3 件に 4 件を足す。**とくに偽4（`G-rules-script-coverage.mjs:32-40`）は issue の 3 件より重い——**同じファイルの `:49` と正面から矛盾しており、読者は近い方（ヘッダ）を先に読む**。かつ「`governance:check` の実行時に鳴る」という**今日成り立たない運用上の保証**を名指しで与えている。

書き換えの方向は issue の指示どおり「消えた保証を消えたものとして書く」。偽2 は数を 3→1 に減らすのではなく残余の記述が変わる:
> 旧: 錨も部分集合テストも #701 のカナリアも赤にしない（3 つとも緑）
> 新: **#701 のカナリア（`scripts/governance-check.test.mjs`）だけが、`CLAUDE.md` を持つ member が本表と `governanceDocs()` の両方に載ることを固定する。`MODULE_INDEX_CRATES` 由来の母集団が member の `src/` の外へ出ないことを見る層も、`exts` / `excludeTest` の縮小を見る層も、今日は存在しない。**

（この新文が依拠する #701 カナリアの実在は、`G-module-index.mjs` の doc ではなく `scripts/governance-check.test.mjs:19-49` を直接読んで確かめた——直そうとしている当の写しを根拠にしないため。）

### 3. `.mjs` を指す参照を機械照合の対象にするか — **却下。効果が実測で ~0 だからである**

まず issue の前提を裏取りした。`docs/adr/ADR-canonical-heading-references.md`「帰結」の却下 (1)「`.mjs` / `.ps1` へも広げる」は**走査元**の話であり、しかも**同 ADR の 2026-08-19 追記（#1138）が既に覆している**（`.mjs` を含むスクリプトのコメントは今日の走査元である）。issue の言うとおり「対象の綴り」とは別物であり、加えて**却下理由は当たらない**。ゆえに ADR は障害ではない。判断は費用と効果で決めた。

**効果（実測）**——`lib.mjs` の部品を import した写しで、`isRefTargetSpelling` に `.mjs` を足し `resolveRefTarget` の `.md` ゲートを `.md|.mjs` へ広げた版を 3 消費者の全走査に当てた:

| 消費者 | 新たな finding | 内訳 |
|---|---|---|
| `G-heading-refs` | **3 件** | `lib.mjs:498`（解決できない＝**本 issue が直す偽1 そのもの**）／ `G-rules-script-coverage.mjs:25`（対象は実在するが `collectAnchors` が **anchors=0** ゆえ「着地しない」）／ `governance-check.test.mjs:118`（同じく anchors=0） |
| `G-near-heading-refs` | **0 件** | — |
| `G-folded-heading-refs` | **0 件** | — |

**つまり本 issue を閉じた後に残る有効な検知は 0 件で、代わりに偽陽性が 2 件立つ。** 原因は `collectAnchors`（`lib.mjs:397-412`）のアンカーが ATX 見出し / 番号付きリスト / 太字リードの 3 種しか無いこと——`.test.mjs` は 3 種のどれも持たず **anchors=0**（実測）。参照が指しているのは実際には `it("母集団の下界: …")` / `it("配線: …")` という**テスト名**である（`G-rules-script-coverage.test.mjs:53` / `evidence.test.mjs:76,80,86,96` で実読）。

**費用**:
1. `scripts/governance/lib.mjs:180` `isRefTargetSpelling`
2. `scripts/governance/lib.mjs:422` `resolveRefTarget` の `.md` ゲート
3. `scripts/governance/lib.mjs:397` `ANCHOR_SPECS` へ **4 本目**（`describe(` / `it(` の文字列）——これは既存 3 種と性質が違う**新しいアンカーの種類**であり、`ANCHOR_SPECS` を import する `scripts/governance/dependents.mjs:20` の逆引きも同時に変わる
4. `scripts/governance/checks/G-folded-heading-refs.test.mjs:83` が**赤になる**（`.mjs` が対象綴りでないことを固定する負例）。同ファイル `:9` の前提コメントも書き直す
5. `docs/adr/ADR-canonical-heading-references.md`「決定」1（「対象は `<path>.md` または `/skill-name`」）の改定＋`.claude/rules/governance-docs.md` の正準形の規約

**費用 5 項目 / 効果 0 件（偽陽性 +2）。却下する。**

**副次的に測った**: `G-stale-identifiers` を `.mjs` のコメントへ広げる案も効果 0 である。`staleTarget`（`G-stale-identifiers.mjs`）が `if (seg.includes(".")) return null;` を持つため `domains.test.mjs` は候補にならず、`DOMAIN_SPECS` 等の識別子は生きた `.mjs` に 1 件も残っていない（軸 B で実測）。**今日どの機械化経路も効果を持たない。**

### 4. 直さない以上、別の気づく契機を名指しする

**`AGENTS.md`「条件別チェック（トリガー → 参照先）」へ 1 行足す。**足す先はセーフティネット行の隣ではなく、既存の「調査・測定のための一時的な足場…を新設**または撤去**」行の**すぐ下**が適切である（同じ「撤去の後始末」の族だから）。

> | 機構・層・ファイル群を**撤去**する（検査・母集団の宣言・計器の削除を含む） | **削除したファイル名と、その層の語彙を `scripts/` を含めた散文へ当ててトリアージする**——`git grep -n -E "<削除ファイル名>\|<層の語彙>" -- scripts/ docs/ .claude/ ':!docs/superpowers/'`。**識別子の残存 0 件は根拠にならない**（#1152 は識別子 0 件を確認して 33 行のコメント散文を残した・#1155）。分類は「撤去を描写 / 撤去されたものが在る前提（偽） / 語彙の残骸」の 3 つ。**`.mjs` のコメントを見る機構は無い**（`isRefTargetSpelling` が `.md` と `/skill-name` に限る・実測で機械化は効果 0） |

**この行が既存の行で代替できないことを確かめた**: 「ガバナンス機構**自身**の配置を変える」行はフォールトインジェクションでの再測定を求めるが、**測るのは機構であって散文ではない**。「文書に事実の写しを増やす変更」行は写しの数え上げを求めるが、**撤去は写しを増やさない**。「ファイル（`.rs`）を追加/削除」行は `.rs` に限られ、索引と `mod` 宣言しか見ない。**どれも今回の形に当たらない。**

**機構を足さない理由**（`AGENTS.md` のセーフティネット行が求める問い「壊れたとき緑が緑のまま推移するか」への回答）: 推移する（今まさに推移した）。しかし**機械化の効果が実測で 0** なので、置ける機構が無い。ゆえに規範側の引き金で止める、が唯一の選択肢である。**これは `ADR-governance-anchor-layer-discarded`「受容する残余」（「気づく契機は人の目だけである」）と同じ性質の残余の、doc 散文についての反復である**——同 ADR の残余に**この形（撤去の後始末そのもの）が入っていることを追記してもよい**が、これは提案であって導出ではない。

## 確信の持てない点（⚠️）

1. **⚠️ 独立性の汚染を 1 件開示する。** 途中で `grep -rn "isRefTargetSpelling" .claude/` を打ったところ、追跡外の `.claude/worktrees/agent-a2d32b3d0662b6239/workspace/plan-review-fold-detector.md` と `workspace/plan.md` が結果に混ざり、**#1154 のレビューが偽1 と偽2 を同じ形で指摘している段落を読んでしまった**。ただし**偽1（`lib.mjs:498`）・偽2（`G-module-index.mjs:25`）・`G-folded-heading-refs.test.mjs:83` は、それ以前の最初の 2 コマンド（軸 A / 軸 B の `git grep`）で既に私の手元に出ている**。汚染後に出した所見は偽4〜偽7 と機構 3・5 で、これらは worktree の文面に無い。以後は `git grep`（追跡下のみ）へ戻した。**決着させるには**、偽4〜7 を別のエージェントが `git grep` だけで再導出できるかを見ればよい。

2. **⚠️ 偽4 の「今日 `SCRIPT_EXT` を `.mjs` へ狭めても `governance:check` は exit 0」は、変異注入ではなく構造で確かめた。** 本タスクが編集を禁じているためである。根拠は (a) `SCRIPT_EXT` は module-private で `judgingScripts` からしか読まれない、(b) `judgingScripts` の消費者は `git grep` で**同ファイルとそのテストだけ**、(c) 母集団が縮めば `checkRulesScriptCoverage` の findings は減る方向にしか動かない。**決着させるには**リポジトリを複製して `SCRIPT_EXT` を `/\.mjs$/` へ変異させ `node scripts/governance-check.mjs; echo $?` を測る。

3. **⚠️「21 本」（`scripts/governance-check.mjs:102`・`:174`・`scripts/governance-check.test.mjs:63`）は今日たまたま真である。** `74ae45fc` 直後は 20 本で、`585c0991`（#1156）が `G-folded-heading-refs` を足して 21 本へ戻った（`npm run governance:check` が「検査 21 件」と印字するのを実測）。**一時的に偽だった数え上げの散文**であり、`AGENTS.md`「検証の作法」の「数ではなく正本（分岐そのもの）を指す」に当たる。今回の射程に入れるかは判断が要る——**入れる方へ倒したいが、issue の射程は「錨の語彙」なので独断で足さない**。

4. **⚠️ `docs/design/2026-08-20-governance-meta-demotion-derivations.md` の扱いは、本 issue より広い問いを開く。** 「生きた母集団に居るが 338 行中 313 行がフェンス内で全散文検査から不可視」という性質は、この 1 枚に限らない（`docs/design/` の 2 枚のうち少なくとも 1 枚）。本 issue では触らないと結論したが、**「原文を貼る設計文書は `governanceDocs` から外すべきか」は別 issue の候補**である。**決着させるには** `docs/design/` の各文書についてフェンス外行比率と `scanStaleIdentifiers` の照合件数を測ればよい（本文書についてはそれぞれ 25/338・0 件を実測済み）。

5. **⚠️ 偽7（ADR への追記）は、凍結歴史の契約への例外を作る。** 私の根拠（生きたスキルが運用 SSOT として委譲している）は妥当だと考えるが、**`ADR-adr-frozen-history` の契約は「ディレクトリ単位で一様であるべき」と明示している**（同 ADR「却下した代替案」3）。ゆえに例外ではなく**「凍結されるのは決定の記述であって、生きた手順として参照される節ではない」という線引きの明文化**が要るかもしれない。ここは利用者の裁定が要る。

6. **⚠️ 私の 33 行という数え上げも腐る。** 軸 F の正規表現は `錨` / `ドメイン` / `domains` 系に閉じており、**「母集団の宣言」「未移行」「ラチェット」のような、識別子でも固有名でもない言い換え**を落としている可能性がある。`git grep -n "未移行\|ラチェット" -- scripts/` は 0 件だったが、**言い換えの母集団を私は知らない**（`AGENTS.md`「列挙の完全性」の言う「誰が母集団を知っているか」に、この軸は答えを持たない）。数ではなく**トリアージ表そのもの**を成果物として扱うのが正しい。
