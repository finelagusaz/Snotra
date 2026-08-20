# 調査 — issue #1155: 撤去した錨の層の語彙が生きた層の doc に残っている

観測時点: `585c0991`（main・2026-08-20）。ブランチ `chore/governance-anchor-vocab-aftermath`。

## issue の要約

#1152（`74ae45fc`）が錨の層を撤去したとき、**撤去されたファイル・撤去された経路を指す散文が生きた層（`scripts/**/*.mjs`）に残った**。機構は撤去を知っていた（manifest delta `-G-domain-anchors`）が、散文は追随しなかった。issue は偽 3 件を名指しし、母集団を「31 出現 / 9 ファイル」と見積もり、(a) トリアージ (b) 偽の修正 (c) `.mjs` を機械照合の対象にするか (d) しない場合の気づく契機、の 4 点を決めることとしている。

**射程外（issue が明記）**: 錨の層を戻さない。`docs/superpowers/` の設計文書は触らない。

## 母集団の再導出（issue の数と合わない — 理由つき）

issue の 5 語（`ドメイン` / `錨` / `ctx.domains` / `domains.test.mjs` / `G-domain-anchors`）で `scripts/**/*.mjs` を数え直すと **33 行 / 10 ファイル**（issue は 31 / 9）。差は #1156（`585c0991`・issue 作成後にマージ）が `G-folded-heading-refs.test.mjs:83` を足したこと、および issue が「出現」を数えたのに対しこちらが行数を数えたことによる。**数を issue に合わせる作業はしない**——母集団の SSOT は issue の散文ではなく、撤去コミットとリポジトリ自身である。

母集団の閉じを 2 方向で確認した。

- **撤去層の識別子からの補完**（`git show 74ae45fc^:scripts/governance/domains.mjs` の export と削除行から導出）: `DOMAIN_SPECS` / `buildDomains` / `duplicateDomains` / `DOMAIN_NAMES` / `checkDomainAnchors` / `domains.mjs` / `G-domain-anchors` を追跡下で `git grep` した結果、**生きた層（`scripts/`）にはゼロ**。ヒットは `docs/adr/` 2 本と `docs/design/2026-08-20-governance-meta-demotion-derivations.md` のみ（どちらも撤去を記録する側・後述）。
- **英語語彙**: `domain` は上の 3 件（`ctx.domains` / `domains.test.mjs` ×2）以外に生きた層に無い。`anchor` は `collectAnchors` / `ANCHOR_SPECS` / `dependents.mjs` の**正準形アンカー**という**別語義**であり、撤去層とは無関係。

**`.claude/worktrees/` を数えてはならない**——リポジトリ全体 `grep` では過去エージェントの worktree 残骸が大量にヒットする（追跡外）。母集団は `git grep`（追跡下）で取る。

### 訂正 — 5 語でも補完 grep でも捕まらない軸が 1 本ある（敵対的調査の所見・裁定済み）

**撤去された 17 ドメインの名前そのもの**が生きた層に残っている。issue の 5 語にも補完の 5 語にも当たらないため、上の 2 方向では閉じない。一次資料（`git show 74ae45fc^:scripts/governance/domains.mjs` の `name:` 17 件）から全数を取り、各名前に**現行コードの実体があるか**を突き合わせた（実測）。

| ドメイン名 | 現行の実体 | 生きた層の参照 |
|---|---|---|
| `governanceDocs` / `headingRefDocs` / `headingRefSourceDocs` / `headingRefCommentDocs` / `allHeadingRefDocs` / `staleIdentifierDocs` / `staleIdentifierGuideDocs` / `staleIdentifierTargets` / `adrFiles` / `ruleDocs` / `moduleIndexSources` / `skillTreeDocs` / `nonDocSources` / `judgingScripts` | **在る**（同名の export） | 実体を指す。偽ではない |
| `workspaceMemberDirs` | 無い | **0 件**（問題なし） |
| **`crateSources`** | **無い**（実体は `crateSourceFiles`・`lib.mjs:499`） | **5 件** |
| **`skillDocs`** | **無い**（実体は `skillFiles`・`G-skill-table.mjs:14`） | **4 件** |

**幽霊識別子は 2 つである。** これらは「指している母集団は今日も実在する」というクラス D の弁明が効かない——**名前そのものが存在しない**。とくに `G-check-skill-enumeration.mjs:86` は**実行時の finding 文字列**であり、利用者が受け取るメッセージが `git grep` で見つからない名前を名指す。

`G-stale-identifiers` はこの形（バッククォート内 camelCase で現行語彙に無い識別子）を捕まえる検査だが、**母集団が `.md` だけ**なので `.mjs` のコメントには一度も当たらない。issue が挙げた射程の穴の、具体的な帰結である。

### `.md` 側（生きた層）の全数 — 作業は発生しない

母集団を `.mjs` に閉じてよいかを測るため、`.md`（`docs/superpowers/` と `docs/adr/` を除く）でも全数を取った。

- `.claude/skills/health-check/SKILL.md:78` — **クラス A**。「錨の層は格下げではなく撤去したので、観測の対象ではない」。撤去コミット自身が足した行であり、正しい。
- `docs/comment-guidelines.md:108` — **クラス B**（別語義）。「両者の共通の錨は `font_covers_cjk` である」＝共通の基準点という比喩。
- `docs/design/2026-08-20-governance-meta-demotion-derivations.md` — 22 行。**撤去前の導出の原文**（→ 未解決の疑問 Q4）。

**`.mjs` 以外で書き換えが要るものは、Q4 の決着次第で `docs/design/` の 1 枚だけである。**

## トリアージ（33 行を 5 クラスへ）

issue は 2 クラス（撤去を描写 / 撤去されたものが在る前提）を想定しているが、**実際には 5 クラスある**。とくにクラス D（撤去された概念名を、保証の主張なしに母集団の呼称として使っている）は issue の 2 分法に無い。

### A. 撤去そのものを描写している（正しい・触らない）— 7 行

| 位置 | 内容 |
|---|---|
| `scripts/governance-check.mjs:25,26` | 「宣言（ドメイン）も、その縮みを見張る層も持たない」「錨の層ごと撤去した経緯は ADR」 |
| `scripts/governance-check.mjs:95` | 「錨の層は格下げではなく撤去した」 |
| `scripts/governance-check.mjs:168` | 「錨の層を撤去したので、残るのは下の 2 件」 |
| `scripts/governance/registry.mjs:5,6` | 「母集団の宣言（ドメイン）は要求しない……錨の層ごと」「撤去した経緯は ADR」 |
| `scripts/governance/checks/G-rules-script-coverage.mjs:49` | 「縮みを見張っていた錨の層は撤去した」 |

### B. 別語義（「ルート錨止め」= パスの先頭固定・無関係）— 3 行

`scripts/governance/lib.mjs:13`・`lib.mjs:35`・`lib.test.mjs:310`。いずれも `PATHS` の完全一致照合・`.claude/worktrees` の除外・#1089 のテスト名であり、撤去層の API もファイルも引いていない。

### C. 撤去されたものが在る前提で書いている（偽・直す）— 12 行

| # | 位置 | 何が偽か |
|---|---|---|
| C1 | `lib.mjs:498` | `domains.test.mjs`「moduleIndexSources は crateSources の部分集合」が「向きを縛る」と書くが、**そのファイルは存在しない**。この向きを縛るものは今日ゼロ（issue の偽 1） |
| C2 | `G-module-index.mjs:25,28` | 「カナリア・部分集合テスト・錨の 3 つ」を数えるが**2 つは撤去済み**。残るのは #701 のカナリア 1 本（issue の偽 2） |
| C3 | `G-adr-citations.mjs:33` | 「`run` は `ctx.domains` 経由で渡す」——`run` は `adrFiles(snapshot)` を直接渡し（同ファイル `:9`）、`ctx` に `domains` は無い（`governance-check.mjs:148`。実測: `const ctx = { docs, allRefDocs, staleTargets, gitIgnoredPaths, record }`）（issue の偽 3） |
| C4 | `G-rules-script-coverage.mjs:33-38,40` | **issue が挙げていない 4 件目**。「`SCRIPT_EXT` を狭める → 実行時に鳴るのは錨である」「錨のある版は exit 1」「錨の寄与は検出ではなく帰属」「『錨があるから捕まる』と一般化してはならない」——**すべて現在形**だが錨は無い。しかも**同じファイルの `:49` が「錨の層は撤去した」と書いており、ファイル内で矛盾している** |
| C5 | `G-adr-citations.mjs:51` | 「`references/` の腕には錨を置いていない——単一ファイルの錨に化けるためである（宣言する死角）」。錨を置く/置かないという選択自体が消えている |
| C6 | `G-check-skill-enumeration.mjs:70` | 「ドメインを見ていれば、同じ走査の欠落を**錨が名指しで鳴らす**」——鳴らす主体が無い |

| C7 | `lib.mjs:494`・`G-module-index.mjs:40`・`G-module-linkage.mjs:200` | **幽霊識別子 `crateSources`**。実体は `crateSourceFiles`。`G-module-index.mjs:40`「`crateSources` と畳んではならない」は**畳んではならない相手の名前が存在しない** |
| C8 | `G-adr-citations.mjs:48,50`・`G-check-skill-enumeration.mjs:68,86` | **幽霊識別子 `skillDocs`**。実体は `skillFiles`。`:86` は**実行時の finding 文字列**であり、利用者が受け取るメッセージが存在しない名前を名指す |

C4 の性質は C1〜C3 と違い、**当時の A/B 実測の記録**である（「2026-08-20 に A/B で実測」）。`ADR-measurement-canon-in-code-doc` の系列に当たるため、**消すのではなく「その日の実測」として時制を閉じる**のが筋（→ 未解決の疑問 Q2・**決着済み**）。

C7・C8 は敵対的調査が開けた穴から出た（issue も初稿の調査も名指していない）。**C1〜C6 とは直し方が違う**——保証の記述ではなく**名前**が偽なので、実体の名前（`crateSourceFiles` / `skillFiles`）へ替えれば済む。ただし `G-check-skill-enumeration.mjs:86` だけは finding 文字列＝**利用者に届く出力**なので、`.test.mjs` に逐語の期待値が無いかを確かめてから触る。

### D. 撤去された概念名を、保証の主張なしに母集団の呼称として使っている — 6 行

**幽霊識別子を含む 3 行を C7・C8 へ移した後の残り**: `lib.mjs:489`（`ruleDocs` ドメインのメンバー）・`G-module-index.mjs:37`（`moduleIndexSources`）・`G-adr-citations.mjs:47,57`（`skillTreeDocs` / `nonDocSources`）・`G-check-skill-enumeration.mjs:72,73`（「ドメインに無い」を「実在しない」と言ってはならない）・`G-rules-script-coverage.mjs:61`（述語がドメインのメンバーに当たる）。

**これらは「消えた保証を在るかのように書いて」いない**——`ruleDocs()` も `moduleIndexSources()` も同名で実在し、指しているのは今日も在る母集団である。偽なのは**語彙**だけ（「ドメイン」は `DOMAIN_SPECS` に登録された宣言済み母集団を意味していた語で、その登録機構が無い）。ただし C6 は D の文脈（`:68,72,73`）の中に埋まっており、**同じ段落が D と C にまたがる**。

**撤去コミット自身がこの語を落とす方針を持っていた**（実測・Q1 の決着）。`git show 74ae45fc -- scripts/governance/checks/G-rules-script-coverage.mjs`:

```
-/** `judgingScripts` ドメインのメンバー——判定を持つスクリプトの全体。
+/** 判定を持つスクリプトの全体。
```

**同じコミットが同じファイルの `:61` では残している。** 方針は在ったが完遂されなかった——**取りこぼしの証拠が同一ファイル内にある**。ゆえに残る D を消すのは「意図的なリファクタリングを元に戻す」（ルート `CLAUDE.md`「コミュニケーション原則」）に当たらず、むしろ意図の完遂である。

### E. テスト fixture（`.mjs` を対象綴りに足すなら赤くなる）— 1 行

`G-folded-heading-refs.test.mjs:83`。#1156 が足した**負の fixture** で、テスト名は「対象の綴りでないバッククォートは見ない（`.mjs` や識別子）」。入力は C1 の逐語（`` `domains.test.mjs` `` + 改行 + 「moduleIndexSources は crateSources の部分集合」）である。**`.mjs` を対象綴りに足すと、このテストは意図ごと反転する。**

## なぜどの機構も捕まえなかったか（issue の 2 つ + 実測で見つけた 3 つ目）

1. **`G-heading-refs` の視界外**（issue が指摘・実測で確認）。`lib.mjs:184` の `isRefTargetSpelling` は `.md` か `/skill-name` しか対象と認めず、`.mjs` 対象は `checked` にすら数えられない（`G-heading-refs.mjs:50` の `continue` が `checked += 1` より前）。ベースライン実測の「見出し参照 322 件」に `.mjs` 対象は 1 件も入っていない。
2. **`G-stale-identifiers` の母集団は `.md` だけ**（issue が指摘・実測で確認）。`staleIdentifierTargets` = `.claude/{skills,rules,agents}/*.md` + `docs/**`（`superpowers`/`adr` を除く）+ 固定 4 本。
3. **`G-references` も届かない（issue が挙げていない）。** 母集団は `ctx.docs` = `governanceDocs(snapshot)` で `.md` のみ（`lib.mjs:470-479`）。かつバッククォート参照の述語が **`/` を含むこと**を要求するため、`domains.test.mjs` のようなベア名は**走査元を `.mjs` へ広げても構造的に捕まらない**。ファイル実在の検査はこの検査の担当だが、この形は担当外である。

## `.mjs` を対象綴りに足す案の実測（issue の「決めること」3 点目）

### 却下理由の測り直し（issue が要求している手順）

`ADR-canonical-heading-references` の却下 (1)「`.mjs` / `.ps1` へも広げる」は**走査元**の話であり、**#1138 で既に覆されている**（スクリプトの腕を足し、走査はコメント行に限る）。ベースラインでも「スクリプトのコメント 111 件から照合」と出る。**issue の言うとおり、残っているのは「対象の綴り」という別問題であり、却下理由は当たらない。**

### 対象が `.mjs` の正準形は今日 4 件ある（advisor の事前予測「偽 1 を直せば 0 件」は外れ）

```
docs/adr/ADR-facade-evidence-static-imports.md:9  `scripts/governance-manifest.test.mjs`「フォールトインジェクション — …（#1088）」
scripts/governance-check.test.mjs:118             `governance/evidence.test.mjs`「配線:」
scripts/governance/checks/G-rules-script-coverage.mjs:25  `G-rules-script-coverage.test.mjs`「母集団の下界」
scripts/governance/lib.mjs:498                    `domains.test.mjs`「moduleIndexSources は crateSources の部分集合」  ← C1（偽）
```

**残り 3 件はすべて実在するテストの名前を指しており、しかも前方一致で着地する**（実測）。

- `governance-manifest.test.mjs:66` `describe("フォールトインジェクション — 検査 ID が manifest の集合から消えたときに diffManifest／undeclared が発火するかの実測（#1088）")` — 逐語一致
- `evidence.test.mjs:76` `it("配線: 生の袋を渡すと throw する（view を外す変異が落ちる）")` — 前方一致
- `G-rules-script-coverage.test.mjs:53` `it("母集団の下界: .ps1 / .psm1 も対象である（SCRIPT_EXT を狭める変異を捕まえる）")` — 前方一致

### 足すために要る改修は 3 か所（測って確認済み）

| 箇所 | 現状 | 要る変更 |
|---|---|---|
| `lib.mjs:184` `isRefTargetSpelling` | `.md` か `/skill-name` | `.mjs` を認める |
| `lib.mjs:422` `resolveRefTarget` | `if (!target.endsWith(".md")) return null;` | `.mjs` の解決経路（既存の suffix 一致がそのまま使える） |
| `lib.mjs:397` `ANCHOR_SPECS` | ATX / 番号付きリスト / 太字リード | **`describe(` / `it(` の第 1 引数**を取る腕。`.mjs` に ATX 見出しは無く、`//! - **…**` は行頭が `//!` なので既存 3 種はどれも当たらない（実測） |

さらに **`G-folded-heading-refs.test.mjs:83` の負の fixture が反転する**（クラス E）。#1156 が昨日固定した意図に触るので、**この 1 行は「ついでに直す」ではなく明示的な設計判断である**。

### コスト側

`.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」が当たる（セーフティネットの新設/変更）。`ANCHOR_SPECS` は `collectAnchors` と `dependents.mjs` の `sectionsOf` が**同じ一覧を読む**設計（#1140 で畳んだ）ため、腕を足すと `dependents.mjs` の節境界計算にも影響が及ぶ——**`.md` に `it(` 行は通常無いので実害は薄いが、確かめずに足してはならない**。

## 再利用できる既存パターン

- **測定記録の時制**: `ADR-measurement-canon-in-code-doc`、`PERFORMANCE.md`「この文書へ記録するときの規約」。C4 の書き換え方の先例。
- **プレースホルダへの置換**: `ADR-canonical-heading-references` の 2026-08-19 追記が「検出器自身のコメント（正準形の例示）は、対象の形に当たらないプレースホルダへ替えて解いた」と記録している。クラス E の解き方の先例。
- **守り手ゼロの性質を名指す書き方**: `G-rules-script-coverage.mjs:47-50`（「この母集団が黙って縮んでも、どの層も赤くしない——縮みを見張っていた錨の層は撤去した」）。C1・C2 の書き換えはこの形に揃えられる。
- **宣言する死角**: `G-rules-script-coverage.mjs:9-19` の「ここが見ないもの」列挙。

## 技術的制約

- **`.mjs` の doc 編集は PostToolUse の沈黙が「合格」を意味しない**（ルート `CLAUDE.md`「フック」）。検証は `npm run governance:check` と `npm test` を明示的に打つ。
- **ガバナンス文書の変更**に当たるので `npm run governance:check` が必須（`AGENTS.md`「条件別チェック（トリガー → 参照先）」）。`.mjs` のコメントの正準形もその射程。
- **`.mjs` を対象綴りに足す**なら `.claude/rules/safety-nets.md` の全条項（フォールトインジェクションでの実測を含む）が当たる。**足さないなら当たらない。**
- ベースライン: `npm run governance:check` exit 0（検査 21 件 / 見出し参照 322 件 / 折れうる位置 21 件）。

## 3 つの性質と、今日の守り手（C2 の書き換えに要る対応表）

C2 が数えている 3 機構は**同じ性質を守っていない**。件数の減算ではなく、性質ごとに守り手を書く。

| 性質 | 守っていた機構 | 今日の守り手 |
|---|---|---|
| `CLAUDE.md` を持つ member が `MODULE_INDEX_CRATES` と `governanceDocs()` の**両方**に載る | #701 の母集団カナリア（`governance-check.test.mjs`） | **在る**（`npm test`。ただし `skip-ci` の PR では走らない） |
| `MODULE_INDEX_CRATES` 由来の母集団が member の `src/` の**外へ出ない** | `domains.test.mjs`「moduleIndexSources は crateSources の部分集合」 | **ゼロ**（撤去済み） |
| 母集団が**黙って縮む**（`exts` / `excludeTest` の狭窄） | 錨（`G-domain-anchors`） | **ゼロ**。#1152 以前から 3 つとも赤にしなかった（2026-08-20 実測が同 doc に記録済み） |

C1 が名指す性質（`crateSources` と `moduleIndexSources` が畳まれない＝2 本目の導出であり続ける）も**守り手ゼロ**である。

## 未解決の疑問

- **Q1（クラス D の扱い）— 決着: 消す。** 撤去コミット `74ae45fc` 自身が `G-rules-script-coverage.mjs` で「`judgingScripts` ドメインのメンバー」→「判定を持つスクリプトの全体」と語を落としており、**同じファイルの `:61` に取りこぼしを残している**（実測・上の D 節）。方針は撤去側にあり、残る 6 行を消すのは意図の完遂である。
- **Q2（C4 の時制）— 決着: 消さず、過去形へ畳んで今日の守り手と分ける。** `ADR-measurement-canon-in-code-doc` が「測定値の正本は `PERFORMANCE.md` に限らない——寄せ先が無いときはコードの doc を正本にしてよい」と裁定しており、C4 は既にコードの doc に在る正本である。同 ADR は「数値を落とし害の説明だけ残す」案を「規模感が失われる」として却下している。**A/B の値（錨の無い版 exit 0・錨のある版 exit 1・finding 2 件）と実測日 2026-08-20 を保ち、構図を過去形で書き、今日の守り手（`npm test` の「母集団の下界」だけ）を現在形で書く。**
- **Q3（`.mjs` 対象綴り）**: 足すか。足すなら `ANCHOR_SPECS` の腕・`resolveRefTarget`・`G-folded-heading-refs.test.mjs:83` の 3 点が同時に要り、フォールトインジェクション実測が乗る。足さないなら `AGENTS.md`「条件別チェック（トリガー → 参照先）」で別の気づく契機を名指す（issue の 4 点目）。
- **Q4（`docs/design/` の同型）**: `docs/design/2026-08-20-governance-meta-demotion-derivations.md` が `DOMAIN_SPECS` / `buildDomains` / `duplicateDomains` を現在形で持つ（22 行）。`G-stale-identifiers` の母集団に**入っている**のに鳴らないのは、それらが**コードフェンス（```text）の内側**にあり `linesOutsideFences` が落とすため（実測）。**issue の射程は `.mjs` なので原則触らないが、「射程外」に明記されているのは `docs/superpowers/` だけである**——計画で射程を 1 行決める。

## 敵対的調査（Step 3b）の所見と採否

sonnet 1 体 / 全文は `workspace/adversarial-1155.txt`。命題 6 本を偽にしにいかせ、変異注入も許可した（作業ツリーは復元済みと報告・こちらでも `git status` で確認する）。

### 採った所見

- **命題 1 は部分的に偽（母集団が閉じていない）。採用。** 5 語でも補完 grep でも当たらない軸として**旧ドメイン名**を名指した。裁定のため 17 名の全数と実体の有無を自分で突き合わせたところ、**幽霊識別子は指摘された `skillDocs` だけでなく `crateSources` もあった**（合計 9 参照）。**所見は採り、機序（「`skillDocs` 1 件」）は自分の実測で置き換えた。** → クラス C7・C8 を新設。
- **`.md` 側にも撤去層の語彙がある。採用（ただし作業は 0 件）。** 指摘された `.claude/skills/health-check/SKILL.md:78` を読むと**撤去を正しく描写しており**（クラス A）、書き換えは発生しない。所見の価値は「母集団を `.mjs` に閉じた」という**宣言の側**が偽だった点にある——`.md` 全数を測り直し、作業が発生しうるのは `docs/design/` の 1 枚（Q4）だけだと確定した。

### 採らなかった所見・確信の持てない所見の裁定

- **⚠️「命題 3 の『着地する』がシミュレーションか実行結果か不明瞭」** — 妥当な疑いである。判定は `normAnchor`（バッククォート・`*`・`「」`・空白を落とす）後の前方一致で、`describe` / `it` の逐語と参照ラベルを突き合わせて確認したが、**機構が今日そこを照合していない以上これは手計算であって実行結果ではない**。Q3 を採るなら**着地は実装後に `governance:check` の出力で測る**（計画の検証項目へ送る）。
- **⚠️「`skillDocs` の finding 分岐が到達可能か未検証」** — 到達可能性は書き換えの要否を変えない（到達しなくても名前は偽である）。なお `G-check-skill-enumeration.mjs:74-78` 自身が「今日のフィクスチャからは到達できない——述語が狭まったときだけ到達する（宣言する死角）」と記録している。
- **「#1154 との作業範囲の衝突可能性」** — #1154 は `585c0991`（#1156）としてマージ済みで、本ブランチはその後の main から出ている。衝突は無い。ただし **#1156 が足した負の fixture（クラス E）が Q3 の費用である**点は調査済みで計画に載っている。
- **「`npm test` フル実行の baseline 未取得」** — 妥当。計画の Phase 2 に入れる。

### 壊せなかった項目（敵対枠が実際に当てて、なお立っているもの）

命題 2（「ルート錨止め」は別語義）・命題 3 の件数（`.mjs` 対象の正準形 4 件）・命題 4（`G-references` の `/` 必須述語）・命題 5（`G-rules-script-coverage.mjs:33-40` は偽＝4 件目の偽）・命題 6（クラス D に保証の主張は混ざらない）、および測定環境の 4 点（`ctx` の形・322 件の内訳・折れ fixture の副作用・`linesOutsideFences` の振る舞い）。**命題 5 と測定環境は変異注入で検証された。**
