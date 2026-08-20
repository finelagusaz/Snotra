# 実装計画 — issue #1155: 撤去した錨の層の語彙の後始末

ブランチ: `chore/governance-anchor-vocab-aftermath`。調査は `workspace/research.md`、敵対的調査の全文は `workspace/adversarial-1155.txt`。

## 目的

#1152 が錨の層を撤去したあと、生きた層の散文に残った**「もう無い機構が今も保証を与えている」と読める記述**を、消えた保証を消えたものとして書き直す。あわせて、**同じ形の腐りが次に起きたとき沈黙で推移しないよう、`.mjs` を指す正準形の参照を機械照合の対象にする**（U3 の決着・人間の判断）。

**錨の層は戻さない**（`ADR-governance-anchor-layer-discarded` / `ADR-governance-meta-demotion` の裁定は動かさない）。この issue は撤去そのものではなく撤去の後始末である。

## 受け入れ条件

1. クラス C（C1〜C8）が、**守り手ゼロの性質を守り手ゼロとして**書かれている。単に指し先を消した形はこの条件を満たさない。
2. **幽霊識別子がゼロである** — `crateSources` / `skillDocs` は現行コードに実体が無い（実体は `crateSourceFiles` / `skillFiles`）。`git grep -nE '\b(crateSources|skillDocs)\b'` の出力が空になる。
3. クラス A（7 行・撤去の描写）とクラス B（4 行・「ルート錨止め」等の別語義）は**変更されない**。
4. `G-module-index.mjs` の残余の記述が、**件数の減算ではなく性質ごとの守り手**で書かれている（`research.md`「3 つの性質と、今日の守り手（C2 の書き換えに要る対応表）」の表が原型）。
5. **`.mjs` を対象にした正準形が機械照合される** — 機構を入れた時点で C1 の指し先（実在しない `domains.test.mjs`）が**赤くなることを実測**し、修正後に緑へ戻ることを実測する。実在する 3 件は着地する。
6. `npm run governance:check` exit 0・`npm test` 全 pass。
7. 母集団に残る撤去層の語彙が、分類のどれかに帰属している（帰属しない出現がゼロ）。**母集団は 3 軸で取る** — issue の 5 語・撤去層の実装識別子・**旧 17 ドメイン名**。

## 変更ファイルと対象シンボル

### 機構（U3 = (A)）

| ファイル | 対象 | 変更 |
|---|---|---|
| `scripts/governance/lib.mjs` | **`HEADING_REF` の doc（`:170`）** | 「対象は `<path>.md` か `/skill-name`」が変更で偽になる（概念ラベルの grep で発見） |
| `scripts/governance/lib.mjs` | `isRefTargetSpelling`（`:179-180`） | `.mjs` を対象綴りとして認める（doc の綴りの列挙も同じ行にある） |
| `scripts/governance/lib.mjs` | `resolveRefTarget`（`:417-431`） | `.md` 決め打ちの早期 return（`:422` `if (!target.endsWith(".md")) return null;`）を対象綴りの判定へ替える。**既存の解決経路（文書ディレクトリ基準 → ルート基準 → suffix 一致）で 4 件すべてが正しく落ちる**（机上検証済み・下表） |

`resolveRefTarget` の 4 件の解決先（`:422` を替えるだけで足り、新しい経路は要らない）:

| 参照元 | 対象 | 解決経路 | 結果 |
|---|---|---|---|
| `G-rules-script-coverage.mjs:25` | `G-rules-script-coverage.test.mjs` | 文書ディレクトリ基準（`checks/` + ベア名） | `scripts/governance/checks/G-rules-script-coverage.test.mjs` |
| `governance-check.test.mjs:118` | `governance/evidence.test.mjs` | 文書ディレクトリ基準（`scripts/` + 相対） | `scripts/governance/evidence.test.mjs` |
| `ADR-facade-evidence-static-imports.md:9` | `scripts/governance-manifest.test.mjs` | ルート基準 | `scripts/governance-manifest.test.mjs` |
| `lib.mjs:498`（C1） | `domains.test.mjs` | どれも一致しない | **null → 「対象が解決できない」で赤**（狙いどおり） |
| `scripts/governance/lib.mjs` | `ANCHOR_SPECS`（`:397-401`） | `describe(` / `it(` の第 1 引数をアンカーとして取る腕を足す |
| `scripts/governance/checks/G-folded-heading-refs.test.mjs` | `:82-87`（負の fixture） | 「`.mjs` は対象綴りでない」を固定するテストの意図が反転する。プレースホルダへ替える（先例: `ADR-canonical-heading-references` の 2026-08-19 追記） |

**この fixture が壊すのは `npm test` だけで、`governance:check` は壊さない**（実測）——`:83` は `expect(run("…"))` の**文字列リテラル**内にあり、検出器の走査は `linesOfComments` でコメント行に限られるため実リポジトリ走査の母集団外である（#1138 の追記が「負の fixture が赤くなるのは文字列リテラルに書かれたデータだから」と記録した通り）。**赤くなる層を取り違えないこと。**

**検出器自身のコメントは赤くならない**（実測）。`G-heading-refs` / `G-near-heading-refs` / `G-folded-heading-refs` の例示はすべて `<対象>` というプレースホルダで、`.mjs` を足しても対象の形に当たらない。#925 が却下 (1) で挙げた「検出器の説明が検出器を赤にする」は、**対象の綴りの拡張では再現しない**。

**`.ps1` / `.psm1` / `.rs` は足さない** — 対象にした正準形が今日 0 件であり（実測）、足しても照合が 1 件も生まれず機構の面積だけ増える（`detector-scope-only-as-tight-as-needed`）。**doc で「対象綴りに入らない」ことを宣言する**（宣言する死角）。

**`isRefTargetSpelling` の消費者は 4 つある**（実測）: `G-heading-refs:49`・`G-near-heading-refs:68`・`G-folded-heading-refs:75,82`・`dependents.mjs:43`。**4 つすべてが `.mjs` を見るようになる**——これは「1 行も変えていない下流が初めて走る」形であり（`newly-live-branch-is-unverified`）、Phase 1 で全消費者の出力差を実測する。

### 散文

| ファイル | 対象 | クラス |
|---|---|---|
| `scripts/governance/lib.mjs` | `crateSourceFiles` の doc（`:494-498`）・`ruleDocs` の doc（`:489`） | C1・C7・D |
| `scripts/governance/checks/G-module-index.mjs` | `MODULE_INDEX_CRATES` の doc（`:19-29`）・`moduleIndexSources` の doc（`:37-40`） | C2・C7・D |
| `scripts/governance/checks/G-adr-citations.mjs` | `adrCitationDocs` の `@param`（`:33`）・`skillTreeDocs` の doc（`:47-51`）・`nonDocSources` の doc（`:57`） | C3・C5・C8・D |
| `scripts/governance/checks/G-rules-script-coverage.mjs` | `//!` ヘッダ（`:33-40`）・`COVERAGE` の doc（`:61`） | C4・D |
| `scripts/governance/checks/G-check-skill-enumeration.mjs` | 母集団のコメント（`:68-73`）・**finding 文字列（`:86`）** | C6・C8・D |
| `scripts/governance/checks/G-module-linkage.mjs` | 除外の理由コメント（`:200`） | C7 |

## 実装順序 — 機構を先に入れて Red を実測し、散文で Green にする

`AGENTS.md`「開発ワークフロー」5（Red → Green）を機構と散文の関係へ当てる。**現に在る欠陥（C1）を新しい検知器が捕まえることを、直す前に実測する**——これは変異注入より強い証拠であり、稼働中のガードも弱めない（まだ main に入っていない）。

1. **Phase 0**: ベースライン
2. **Phase 1**: 機構を足す（**Red**: C1 が赤くなる・3 件が着地する・4 消費者の波及を実測）
3. **Phase 2**: C1〜C6 の保証の記述（**Green**)
4. **Phase 3**: 幽霊識別子（C7・C8）と語彙（D）
5. **Phase 4**: 母集団の閉じと全検証

## 不変条件と異常系

- **クラス A・B を触らない。** 触ると「撤去の記録」または別語義の説明が壊れる。差分に A・B の行が現れないことを確認する。
- **消えた保証を「無かったこと」にしない。** C1・C2 が名指す性質（`crateSources` と `moduleIndexSources` を畳まない／母集団が `src/` の外へ出ない／母集団が黙って縮む）は**今日も守られていない**。書き換えの向きは「参照を消す」ではなく「守り手がゼロだと書く」である。
- **測定値を落とさない**（`ADR-measurement-canon-in-code-doc`）。C4 の A/B（錨の無い版 exit 0 / 錨のある版 exit 1 / finding 2 件・2026-08-20）と、**C2 の括弧内（`exts` を狭めて 30 件を落としても 3 つとも緑・2026-08-20）**の両方が対象。
- **`ANCHOR_SPECS` は `dependents.mjs` の `sectionsOf` と同じ一覧を読む**（#1140 で畳んだ・`lib.mjs:394` が正本）。腕を足すと節境界の計算にも及ぶ。**「`.md` に `it(` 行は無いはず」は推測である**——Phase 1 で `dependents.mjs` の出力が不変であることを実測する。
- **`G-check-skill-enumeration.mjs:86` は実行時の finding 文字列＝利用者に届く出力である。** 逐語の期待値を持つテストは無い（実測）。
- **`scripts/**` の編集は `.claude/rules/safety-nets.md` が自動配送される**（`paths` に `scripts/**` を持つ・実測）。Phase 1 は機構の射程変更ゆえ、同 rule の全条項が当たる。

## テスト方針と検証コマンド

- `npm run governance:check`（ベースライン実測済み: exit 0 / 検査 21 件 / 見出し参照 322 件 / 折れうる位置 21 件）
- `npm test`（**ベースラインを Phase 0 で取る**。`.mjs` の doc 編集では PostToolUse hook の沈黙が「合格」を意味しない）
- `node scripts/governance/dependents.mjs`（相当の起動経路）で Phase 1 前後の出力を比較する
- 母集団の取り直し（**3 軸すべて**・いずれも `git grep`。`grep -r` は `.claude/worktrees/` の追跡外残骸を混ぜる）:
  - `git grep -nE 'ドメイン|錨|ctx\.domains|domains\.test\.mjs|G-domain-anchors' -- scripts/`
  - `git grep -nE 'DOMAIN_SPECS|buildDomains|duplicateDomains|DOMAIN_NAMES|checkDomainAnchors|META_CHECK_IDS'`
  - `git grep -nE '\b(crateSources|skillDocs|workspaceMemberDirs)\b'`

**pathspec は `-- scripts/` と書く。`-- 'scripts/**/*.mjs'` を使ってはならない**——この git の pathspec では `**` が 0 段に当たらず、**ルート直下の `scripts/governance-check.mjs` が丸ごと落ちる**（実測: 同じパターンで 26 行 vs 30 行。落ちるのはクラス A の 4 行）。独立導出が同じ穴を踏んで報告した。**母集団を測る道具自身が母集団を削る形**であり、`AGENTS.md`「検証の作法（全タスク共通）」の「列挙も SSOT のツール自身に問う」が当たる。

## `SPEC.md`・関連文書の更新要否

- `SPEC.md`: **不要**（製品の挙動を変えない）
- `docs/adr/ADR-canonical-heading-references`: **追記が要る**。同 ADR は却下 (1)「`.mjs` へも広げる」を持ち、#1138 が**走査元**について覆した追記を既に持つ。今回は**対象の綴り**について覆すので、同じ場所へ 1 追記する（凍結規約により本文は書き換えない）
- `.claude/rules/governance-docs.md`: **正準形の規約が「対象は `<path>.md` か `/skill-name`」と書いていないか確認する**（書いていれば `.mjs` を足す）
- `AGENTS.md`: **不要**（U3 が (A) に決まったため、気づく契機は機構が持つ）

## 作業項目

### Phase 0 — ベースライン

- [ ] `npm test` のベースラインを取る（`governance:check` は取得済み: exit 0 / 21 件 / 322 件）
- [ ] `dependents.mjs` の出力ベースラインを取る

### Phase 1 — `.mjs` を対象綴りに足す（Red）

- [ ] `.claude/rules/governance-docs.md:13` の正準形の規約に `.mjs` を足す（**確認済み**——同行が「対象は `<path>.md` か `/skill-name`」と明記しており、ここが規約の正本である）。**同時に、同じ行の隣接主張「走査元は `.md` だけではない——コード（`.rs`）のコメントに書いた参照も同じ検査に載る（#925）」が #1138 のスクリプトの腕（`.mjs` / `.ps1` / `.psm1`）を反映していないことを直す**——触る行の隣にある主張が今も真か見るのは同 rule の「書く約束」(3) の要求である
- [ ] `isRefTargetSpelling` に `.mjs` を足す（`.ps1` / `.rs` を足さない理由を doc に宣言する）
- [ ] `HEADING_REF` の doc（`lib.mjs:170`）の対象綴りの列挙を追随させる
- [ ] `resolveRefTarget` の `.md` 決め打ちを解き、`.mjs` の解決経路を通す
- [ ] `ANCHOR_SPECS` に `describe(` / `it(` の第 1 引数を取る腕を足す
- [ ] `G-folded-heading-refs.test.mjs:82-87` の負の fixture をプレースホルダへ替える
- [ ] **Red の実測**: `governance:check` が **C1 の指し先（`domains.test.mjs`）を「対象が解決できない」で赤にする**ことを確認する
- [ ] **着地の実測**: 実在する 3 件（`governance-manifest.test.mjs`「フォールトインジェクション …」/ `evidence.test.mjs`「配線:」/ `G-rules-script-coverage.test.mjs`「母集団の下界」）が着地し、照合件数が 322 → 326 へ増えることを確認する
- [ ] **4 消費者の波及の実測**: `G-heading-refs` / `G-near-heading-refs` / `G-folded-heading-refs` / `dependents.mjs` それぞれについて、C1 由来以外の新規 finding が出ていないか確認する（出たら分類し、この計画の射程内か判断する）
- [ ] `npm test` を通す（`ANCHOR_SPECS` の腕でアンカー数が変わるテストがあれば追随させる）

### Phase 2 — 保証の記述（C1〜C6・Green）

- [ ] C1 `lib.mjs` `crateSourceFiles` の doc — `domains.test.mjs` への参照を、守り手ゼロの記述へ替える（**これで Phase 1 の Red が緑へ戻る**）
- [ ] C2 `G-module-index.mjs` `MODULE_INDEX_CRATES` の doc — 3 機構の数え上げを性質ごとの守り手の記述へ替える（残るのは #701 のカナリア 1 本）。**括弧内の実測値と実測日は落とさない**
- [ ] C3 `G-adr-citations.mjs:33` の `@param` — `ctx.domains` 経由という偽の経路を、実際の受け渡し（`run` が `adrFiles(snapshot)` を直接渡す）へ直す
- [ ] C4 `G-rules-script-coverage.mjs:33-40` — A/B の値と実測日を保ったまま構図を過去形へ畳み、今日の守り手（`npm test` の「母集団の下界」）を現在形で書く
- [ ] C5 `G-adr-citations.mjs:51` — 「`references/` の腕には錨を置いていない」を、錨の無い今日の言い方へ替える
- [ ] C6 `G-check-skill-enumeration.mjs:70` — 「錨が名指しで鳴らす」を、鳴る主体が無い今日の記述へ替える

### Phase 3 — 幽霊識別子と語彙

- [ ] C7 `crateSources` → `crateSourceFiles`（`lib.mjs:494`・`G-module-index.mjs:40`・`G-module-linkage.mjs:200`）
- [ ] C8 `skillDocs` → `skillFiles`（`G-adr-citations.mjs:48,50`・`G-check-skill-enumeration.mjs:68`・**finding 文字列 `:86`**）
- [ ] D 6 行の「ドメイン」を落とす（`74ae45fc` が `judgingScripts` で示した方針の完遂）

### Phase 4 — 閉じと検証

- [ ] C9 `docs/adr/ADR-governance-meta-demotion.md` へ日付つき追記（`:53` の先例と同じ形）— `META_CHECK_IDS` は #1152 で撤去され、格下げ後の姿は `metaAuditEnabled` と `metaFindings` が持つこと、`:61` の復帰手順の読み替えを書く。**本文は書き換えない**（凍結の契約・`ADR-adr-frozen-history`）
- [ ] `AGENTS.md`「条件別チェック（トリガー → 参照先）」へ「機構・層・ファイル群を撤去する」行を足す（U5）
- [ ] `docs/adr/ADR-canonical-heading-references` へ追記する（対象の綴りについて却下 (1) を覆した根拠・受容する残余＝`.ps1` / `.rs` は対象外・**今日の有効検知は 0 であり価値は将来の沈黙推移の防止にあること**）
- [ ] 母集団を **3 軸**で取り直し、全出現がクラス A〜E のどれかに帰属することを確認する
- [ ] `npm run governance:check` exit 0
- [ ] `npm test` 全 pass（Phase 0 のベースラインと比較する）
- [ ] `dependents.mjs` の出力を Phase 0 のベースラインと比較する

## 未確定（実装前に潰す）

- [x] **U1: クラス D の「ドメイン」を残すか消すか — 消す。** 撤去コミット `74ae45fc` 自身が `G-rules-script-coverage.mjs` で「`judgingScripts` ドメインのメンバー」→「判定を持つスクリプトの全体」と語を落としており、**同じファイルの `:61` に取りこぼしを残している**（実測）。方針は撤去側にあり、残る 6 行を消すのは意図の完遂である。
- [x] **U2: C4（A/B 実測の記録）の時制 — 消さず、過去形へ畳んで今日の守り手と分ける。** `ADR-measurement-canon-in-code-doc` が「測定値の正本は `PERFORMANCE.md` に限らない——寄せ先が無いときはコードの doc を正本にしてよい」と裁定し、「数値を落として害の説明だけ残す」案を「規模感が失われる」として却下している。**C2 の括弧内の実測値も同じ扱いとする。**
- [x] **U3: `.mjs` を正準形の対象綴りに足すか — 足す（人間の判断・2026-08-20）。独立導出は却下を推奨しており、その根拠は下に残す。**
  - **独立導出の反対意見（実測つき）**: `lib.mjs` の部品を import した読み取り専用の写しで 3 消費者に当てたところ、新 finding は `G-heading-refs` **3 件のみ**（`G-near` / `G-folded` は 0 件）。うち 1 件は本 issue が直す C1 そのもので、**残る 2 件は `ANCHOR_SPECS` に腕を足していない設定ゆえ `collectAnchors` が anchors=0 を返した偽陽性**。ゆえに「**C1 を閉じた後に残る有効検知は 0**」。
  - **裁定**: 両者は**違う設定を測っている**（腕あり／腕なし）ので矛盾しない。腕を足せば 2 件は着地し、finding は出ない。**独立導出の結論「今日の有効検知 0」は、腕を足した設定でも正しい**——今日この機構が捕まえる腐りは C1 の 1 件だけであり、それは本 issue が直す。**したがって (A) の価値は今日の検知数ではなく、次に同じ形が起きたときに沈黙で推移しないことにある。** 人間の判断はこの規準（`AGENTS.md`「足す前に『壊れたとき緑が緑のまま推移するか』を問う」）で下されている。
  - **腕を足すことは必須条件である**——足さずに対象綴りだけ広げると、実在する 2 件が偽陽性として恒久的に赤くなる（独立導出の実測）。 判断規準は `AGENTS.md`「条件別チェック（トリガー → 参照先）」の「足す前に『壊れたとき緑が緑のまま推移するか』を問う」で、本件の腐りは #1152 から #1154 の副産物で偶然発見されるまで**沈黙で推移した実績がある**。`G-heading-refs` はメタ層ではなく 21 本のゲートの 1 本ゆえ `ADR-governance-meta-demotion` の射程外。**範囲は `.mjs` のみ**（`.ps1` / `.rs` は対象の正準形が 0 件・実測）。
- [x] **U4: `docs/design/2026-08-20-governance-meta-demotion-derivations.md`（22 行）を射程に入れるか — 入れない。** 当該文書は冒頭で「ここに置くのは、その結論を出した 2 つの**導出の原文**である——要約に落とすと、そのとき必要になる根拠が消える」と宣言する凍結された記録である（`ADR-adr-frozen-history` と同型）。22 行の大半はコードフェンス内の外部エージェントの報告原文であり、**書き換えれば引用を改竄することになる**。
- [x] **U5: 気づく契機の名指し — 要る（独立導出の指摘で訂正）。** 「U3 が (A) に決まったので機構が契機を持つ」と一度は判断したが**誤りである**——`.mjs` の対象綴りが守るのは**正準形の参照だけ**であり、母集団 33 行のうち正準形は **1 行（C1）のみ**。残る 32 行（散文の語彙・幽霊識別子・偽の経路説明）は機構を足しても依然として沈黙で推移する。**`AGENTS.md`「条件別チェック（トリガー → 参照先）」へ「機構・層・ファイル群を撤去する」トリガーを 1 行足す**（削除ファイル名と層の語彙を `scripts/` 込みで grep してトリアージする／**「撤去した識別子の残存 0 件」はコード識別子しか見ておらず根拠にならない**）。既存 3 行では代替できないことを独立導出が各行の射程で確認している。**これは規範文書の変更ゆえ、ルート `CLAUDE.md` 最重要ルール 2 により人間の承認が要る。**
- [x] **U6: `META_CHECK_IDS`（独立導出が発見した 7 件目の偽）を射程に入れるか — 入れる。** `docs/adr/ADR-governance-meta-demotion.md:3`（「格下げ後の姿は `META_CHECK_IDS` と `metaAuditEnabled` が持つ」）と `:61`（「その項目**だけ**を `META_CHECK_IDS` から外してゲートへ戻す」）が、**`74ae45fc`＝本 issue が扱う撤去コミット自身が消した識別子**を指している（`git log -S` で実測）。しかも `.claude/skills/health-check/SKILL.md:94` が「判定規則（撤去 / 復帰）の正本は同 ADR「戻す条件・撤去する条件」である」と**生きた層から委譲**しており、**復帰手順が実行不能**である。射程外の理由（ADR は凍結）は当たらない——`74ae45fc` 自身が同 ADR へ日付つき追記をした先例があり（`:53`）、**本文を書き換えず追記で解く**。

## セルフレビュー

- リスク: **高**（網羅性が要件＝母集団の全数トリアージ / ガバナンス文書の変更 / セーフティネットの機構変更）
- 自己レビュー 5 点の照合:
  1. **issue の全要件に作業項目が対応する** — トリアージ（Phase 2・3）・偽の修正（Phase 2）・`.mjs` 照合の判断（U3 決着 → Phase 1）・気づく契機（U5 で解消）の 4 点すべてに対応がある
  2. **境界条件と検証** — 母集団の 3 軸（Phase 4 で全数）・クラス A/B の不変（差分レビュー）・finding 文字列（逐語期待値の不在を実測済み）・4 消費者の波及（Phase 1 で実測）
  3. **新しい状態・リソース** — `ANCHOR_SPECS` の新しい腕。正常系＝3 件の着地、失敗系＝C1 の赤、破棄経路に相当するのは「対象綴りに入らない拡張子」の宣言
  4. **より単純な既存パターン** — C7・C8 は名前の置換のみ。C1・C2 は `G-rules-script-coverage.mjs:47-50` の「守り手ゼロ」の書き方を先例として流用する。機構側は #1138（走査元の拡張）が直接の先例
  5. **不変条件の検知手段** — 受け入れ条件 2（幽霊識別子ゼロ）は `git grep` の出力が空であることで測れる。受け入れ条件 5 は Red/Green の実測で測れる。受け入れ条件 1・4（守り手ゼロの記述）は**機構では測れない**——人間レビューが唯一の検知点である（受容する残余）
## plan-review 結果

- リスク: **高**
- レビュー方式: **独立導出 1 体**（Step 2b・親モデル継承。`workspace/` を読ませず issue の WHAT だけを渡した）
- エージェント数: 2（Step 3b の敵対的調査 1 + 独立導出 1）
- 成果物: `workspace/plan-review-anchor-vocab.md`

### 要対処（すべて再照合済み・計画へ反映済み）

- **検証コマンドの pathspec が母集団を削っていた** — `-- 'scripts/**/*.mjs'` は `**` が 0 段に当たらず `scripts/governance-check.mjs` の 4 行を落とす（**再照合: 26 行 vs 30 行**）。`-- scripts/` へ修正。
- **7 件目の偽 `META_CHECK_IDS`** — `ADR-governance-meta-demotion.md:3,:61` が指す識別子を消したのは **`74ae45fc`＝本 issue が扱う撤去コミット自身**（**再照合: `git log -S 'META_CHECK_IDS' -- scripts/` が `74ae45fc` と `7506f093` を返す**）。`health-check/SKILL.md:94` が同 ADR を運用 SSOT として委譲しており**復帰手順が実行不能**。→ C9 として Phase 4 へ。
- **U5 の判断が誤りだった** — 機構（`.mjs` 対象綴り）が守るのは正準形の参照だけで、母集団 33 行のうち正準形は 1 行のみ。残り 32 行は機構を足しても沈黙で推移する。→ `AGENTS.md` への 1 行を復活。
- **U3 の効果の記述が過大だった** — 「3 件が照合される」は正しいが「今日 3 件の腐りが見つかる」ではない。今日の有効検知は 0。→ U3 の欄へ独立導出の反対意見と裁定を明記。

### 軽微

- 独立導出は偽を 7 件、こちらは C1〜C8（幽霊識別子 2 群を含む）と数えた。**マッピングは 1 対 1 で衝突しない**（独立導出の偽 1〜6 = C1〜C6、偽 7 = C9、こちらの C7・C8 は独立導出の「語彙の残骸 8 行」に含まれるが幽霊性は名指されていない）。**幽霊識別子の発見はこちらが深く、`META_CHECK_IDS` は向こうが深い。**

### 未検証

- **「21 本」の数え上げ**（`governance-check.mjs:102,174` ほか）— 独立導出が「#1152 で 20 になり #1156 で 21 に戻った**たまたま真**の数え上げ」と報告。`AGENTS.md`「数え上げも同じ強さである——数ではなく正本を指す」に触れうるが、**本 issue の母集団（撤去した錨の層の語彙）ではない**。射程外として別 issue 候補に残す。
- `docs/design/2026-08-20-…-derivations.md` が `governanceDocs` と `staleIdentifierTargets` の**両方に居ながら 338 行中フェンス外が 25 行しかなく照合 0 件**（独立導出の実測）。U4 で射程外と決めた文書だが、**母集団に居るのに実質不可視**という別の論点であり、別 issue 候補に残す。
- 独立導出は独立性の汚染を 1 件自己申告している（`grep -rn .claude/` が追跡外 worktree の #1154 レビュー文面を拾った）。**汚染前に出ていた所見（偽 1・偽 2・fixture:83）と、汚染後の所見（偽 4〜7・機構の追加発見）は分離して報告されており**、要対処に採ったのはすべて後者＝汚染源に無い所見である。

### 判断

- 実装着手: **人間の裁定待ち**（`.mjs` 対象綴りの追加と `AGENTS.md` への 1 行が、どちらもセーフティネットの変更にあたるため）

## 人間レビュー

- [x] 承認済み — 2026-08-20 / 問い: "`workspace/plan.md` に注釈を追加していただくか、**(a)(b)(c) を含めて明示的にご承認いただく**か、いずれかをお願いいたします。承認をいただくまで実装へは渡しません。"（(a) `.mjs` を正準形の対象綴りに足す / (b) `AGENTS.md` へ撤去トリガーを 1 行 / (c) ADR 2 本へ日付つき追記） / 回答: "OK"

承認と同じ提示に含めた訂正 2 件（U3 の効果説明の精緻化＝「今日の有効検知は 0」・U5 を「不要」から「要る」へ戻したこと）も、この承認の範囲に入る。
