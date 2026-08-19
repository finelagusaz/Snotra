# plan — issue #1143（`.claude/rules/` の paths が `scripts/governance/**` を覆わない）

調査は `workspace/research.md`（一次証拠はすべて 2026-08-19 に実測）。敵対的調査は `workspace/adversarial-1143.txt`。

## 目的

1. `.claude/rules/` の 2 本の `paths` が `scripts/` 部分木を**取りこぼさない**形にする。
2. 同じ漏れ（判定コードが rule の paths の外へ出る）が**次に起きたとき機構が赤くする**——#837 が
   「その漏れを検出する機構は無い」と書いたまま再発した箇所を塞ぐ。

## 受け入れ条件

- **A.** `safety-nets.md` / `governance-docs.md` の paths が `scripts/governance/checks/G-*.mjs` を覆う。
- **B.** 覆っていない状態では `npm run governance:check` が**赤**になる（#1093 の移動と同じ形の変異で実測する）。
- **C.** **paths を広げた後の**作業ツリーで `npm run governance:check` が緑（誤検出 0）。
  ——**「今日の main で緑」ではない**（今日の main は穴が開いたままゆえ赤が正しい・3b の [BROKEN-1]）。
- **D.** 受け入れた誤配送と射程外（死角）が、検査ファイルの `//!` に宣言されている。

## 決定（issue の「決めること」3 点への回答）

1. **足す。** #837 の判別線（check スキルの判定がそれに依存するか＝規範文書からの参照数）に
   `scripts/governance/` 配下は掛かる（research §2c: 実装 1〜3 文書／検査 ID は 19 本すべてが 1 件以上、
   最大 11 文書。#837 が ✗ と裁定した `clean-worktrees.mjs` は 0 文書）。
2. **ディレクトリごとに足すのではなく `scripts/**` へ畳む。** 実測（research §2b）で `scripts/**` は
   現行 3 glob の和の**真の上位集合**、差分は governance 配下 51 件 + `run-codex.sh` 1 件、**失うもの 0 件**。
   harness の `**` が 3 段下まで届くことも実測済み（research §2a-2）。
   → **次にサブディレクトリが増えても穴が開かない**（#837 が予見できなかった再発形の構造的解消）。
   引き換えに `scripts/run-codex.sh` への誤配送 1 件を受け入れる（#837 が `clean-worktrees.mjs` について
   引き受けたのと同型で、有害になる相手ではない）。
3. **検出器を置く。** 新規検査 `G-rules-script-coverage`（**母集団被覆形**）。
   却下した案は §「却下した案」。

## 変更ファイルと対象シンボル

| ファイル | 変更 |
|---|---|
| `.claude/rules/safety-nets.md` | paths: `scripts/*.mjs` / `scripts/*.ps1` / `scripts/lib/**` の 3 行 → `scripts/**` 1 行 |
| `.claude/rules/governance-docs.md` | 同上 |
| `scripts/governance/lib.mjs` | (1) `globToRegex`（移送）と `rulePathPatterns`（新設）を受け入れる (2) **`headingRefCommentDocs` の doc コメント（`lib.mjs:533`）が旧 paths を逐語で写しており、本変更で偽になる**——glob の列挙をやめ、帰結（「規範はすでに `scripts/` 配下へ配送されている」）だけを残して正本（rule の frontmatter）を指す形へ直す |
| `scripts/governance/checks/G-rules-globs.mjs` | 上記 2 つを `lib.mjs` から import する形へ |
| `scripts/governance/checks/G-rules-globs.test.mjs` | import 元の追随。`globToRegex` の意味論テストは `lib.test.mjs` へ移送 |
| `scripts/governance/lib.test.mjs` | 移送されたテストと `rulePathPatterns` のテストを受け入れる |
| `scripts/governance/checks/G-rules-script-coverage.mjs` | **新規**（`id` / `run` / `checkRulesScriptCoverage` / `COVERAGE`） |
| `scripts/governance/checks/G-rules-script-coverage.test.mjs` | **新規** |
| `docs/adr/ADR-rules-paths-subtree-coverage.md` | **新規**（否定の知識＝却下 3 案・#593） |

`globToRegex` を `lib.mjs` へ移す根拠: `lib.mjs` の `//!` が「**複数の検査ファイルが import する**ものは
ここに残ってよい」と定めている。2 検査が使うので写しを作らない。現在の import 元は
`G-rules-globs.mjs` とその `.test.mjs` の 2 か所のみ（grep 実測）。

**`G-skill-table.mjs:34` の frontmatter ブロック正規表現には触らない**（`/dry-check` のトリガーで走査して
見つけた 3 か所目）。`rulePathPatterns` が束ねるのは「**rules の paths を取り出す**」という概念で、
あちらが取り出すのは別のキー（`disable-model-invocation`）である——片方だけが変わる将来を挙げられるので
別概念として据え置く（`AGENTS.md`「検証の作法」）。本 issue の射程外でもある。

## 検出器の設計

**母集団被覆形** = `G-rules-globs` の**逆向き**。あちらは glob → 実ファイルが 0 件、こちらは実ファイル → glob が 0 件。

- 縛る対（検査ファイル内の `COVERAGE` 表が SSOT）:

  | rule | 覆うべき母集団 |
  |---|---|
  | `.claude/rules/safety-nets.md` | snapshot 内の全 `.mjs` / `.ps1` / `.psm1` |
  | `.claude/rules/governance-docs.md` | `scripts/` 配下の同拡張子のみ |

  （`governance-docs.md` の射程は `.claude/hooks/` を含まないため母集団を部分木へ限る。#837 が価値を置いた
  「2 rules の配送対象の一致」を `scripts/` 部分木について保つ）

- **下界の canary**（被覆形の述語は母集団が縮む側で沈黙する・`docs/development-principles.md`
  「検証の層と、層と層の隙間」）——3 つとも finding にする:
  1. rule ファイルが snapshot に無い（`G-rules-globs` の `rules.length === 0` と同型）
  2. その rule の paths パターンが 0 件（→ finding 1 件を出して次の rule へ。全件未被覆で 90 件吐かない）
  3. 母集団が 0 件（走査の欠落）
- **出力を切り詰めない**——未被覆ファイルごとに 1 finding（壊れているときだけ長くなる）。
- **宣言する死角**（検査ファイルの `//!` に置く。散文の写しは作らない）:
  - 対象外の拡張子（`.sh`——`scripts/run-codex.sh` は判定の対象にならない）
  - `globToRegex` は **harness の配送判定の近似**である（判定するのは documented な glob 意味論での被覆であって、
    harness が実際に配送することではない）
  - 「意味的にセーフティネットか」は判定しない——誤配送は受け入れる側へ倒す（#837 の先例）
  - 母集団は `makeSnapshot` 由来ゆえ **git 未追跡ファイルも入る**
  - **走査が広がったときは赤側へ倒れる**——`lib.mjs` のヘッダが名指しする将来（2 つ目の npm パッケージで
    `ui/node_modules` が走査に入る形）が来ると、vendor の `.mjs` 数千件への被覆要求で赤くなる。
    向きは沈黙ではなく loud なので安全側である。**そのときは `WALK_EXCLUDE_PATHS` へ 1 行足すか、
    この検査の母集団の述語を絞る**——同ヘッダが言うとおり、向きを決めるのは走査器ではなく呼び出し点の述語であり、
    この検査がその述語である

## 却下した案（ADR に残す否定の知識）

1. **`scripts/governance/**` を 1 行足す（ディレクトリごとに足す）** — #837 が「1 件ずつ足す形は再発する」と
   書いた形の、ディレクトリ版の反復。実測で `scripts/**` が上位集合かつ失うもの 0 件と分かった以上、
   畳まない理由が無い。
2. **import 閉包（覆われたファイルの import 先も覆われていること）** — この repo の肝心な 2 辺を**両方**見ない:
   `.claude/hooks/post-edit.mjs:234` は構成した文字列を spawn し（静的 import は
   `post-edit.test.mjs` が禁止している）、`registry.mjs` は `readdirSync` + 動的 `import()` で `checks/` を走る。
3. **`G-rules-globs` を拡張する（新規検査を作らない）** — 1 検査 1 ファイル（#1093）に反し、失敗の意味も違う
   （死んだ glob vs 未被覆ファイル）。加えて
   `.claude/skills/health-check/references/mechanized-checks.md` の備考「マッチ 0 件の検知」が古びる。
4. **検出器を置かず死角として宣言する** — #837 が実際にそうして再発した。今回は被覆形が実装可能と実測できた。
5. **「深さ」形の検出器**（独立導出が提案した `G-rules-paths-depth`——単一 `*` のパターンについて、同じ
   ディレクトリの深部に取りこぼしたファイルが在れば赤くする ＋ アンカー集合による 2 部判定。現行ツリーで
   **発火 2 件・誤検出 0 件**と実測されている） — **却下**。理由 2 つ:
   (a) **パターンが 1 本も無い相手を見ない**——判定コードが `tools/` のような新しいトップレベルへ出た場合、
   「浅すぎる glob」が存在しないので沈黙する。被覆形はファイル側から見るので捕まえる。
   (b) `COVERED_ANCHORS` という**手で維持する列挙**を持つ（`AGENTS.md`「列挙の完全性」の腐る形）。
   ——ただし**発火 2 件は被覆形の 102 件より読みやすい**という利点は認める。被覆形は「どのファイルが
   覆われていないか」を全部名指す形（切り詰めない）でその読みにくさを引き受ける。

## 不変条件と異常系

- `governance-check.mjs` の契約（**依存ゼロ・決定的**——ネットワーク・時刻・環境変数に非依存）を守る。
- `registry.mjs` の形の契約: `id` を export し、ファイル名と一致し、`run(snapshot, ctx)` を持つ。
  id は `G-<kebab>`・数字を含まない（`governance-check.test.mjs` が全 id に当てている）。
- rule に frontmatter が無い / paths が空 / `snapshot.read` が null → **finding を返す。throw しない。**
- **稼働中のガードを弱めない**——変異はすべて in-memory の fixture snapshot（`test-helpers.mjs` の `snap`）へ当て、
  実ファイルの `.claude/rules/` は触らない（`.claude/rules/safety-nets.md`
  「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」）。

## テスト方針

`G-rules-script-coverage.test.mjs`:
- **赤（#1093 の実形の変異）**: paths が `scripts/*.mjs` だけの fixture rule + `scripts/governance/checks/G-x.mjs`
  → その未被覆を名指す finding が出る
- **緑**: paths が `scripts/**` の fixture → findings 0
- **canary 3 種**: rule 不在 / paths 0 件 / 母集団 0 件

`lib.test.mjs`: 移送した `globToRegex` の意味論（既存の代表入力表をそのまま持ち越す）と `rulePathPatterns`。

## 検証コマンド

```
npm run governance:check     # カテゴリ F。新検査を含めて緑（paths 修正後）
npm test                     # vitest 全件
npm run governance:manifest  # checks 列に +G-rules-script-coverage が出ることを確認し、PR 本文の宣言に使う
```

**PostToolUse hook は `scripts/` 配下の `.mjs` に検査を割り当てない**（`CLAUDE.md`「フック」——沈黙は
「何も走らなかった」である）。上の 3 コマンドは**手で実行する**。
PR 本文には `- checks: +G-rules-script-coverage` を逐語で宣言する（`governance-manifest.mjs` の `undeclared` が
要求する。**PR 作成時の作業であり、この計画の作業項目には置かない**）。

## SPEC.md・関連文書の更新要否

- `SPEC.md`: **不要**（プロダクトの挙動を変えない）。
- `docs/adr/`: **新規 1 本**（却下 4 案という否定の知識が生じた・#593）。
- `docs/build-commands.md`: **不要**（コマンドは増えず、検査数の写しを持たない）。
- `.claude/skills/health-check/references/mechanized-checks.md`: **不要**（旧 Check → G の写像表であり、
  新規検査は旧 Check 番号を持たない。`G-rules-globs` の備考は**拡張せず新規にしたので今も真**）。
- `docs/adr/ADR-canonical-heading-references.md`（37 行目）と `docs/adr/ADR-plan-ledger-population-persistence.md`
  （29・45 行目）が旧 paths を逐語で写しているが、**書き換えない**——ADR は凍結された歴史である
  （`ADR-adr-frozen-history`。後者自身が「上の記述は決定当時の事実であり書き換えない」と明記している先例）。
- **`.github/workflows/e2e.yml:26` の `'scripts/lib/**'` は触らない。** これは Smoke workflow の**起動条件**であって
  rules の `paths` ではない——同じ字面だが別機構であり、`scripts/**` へ広げると全 `scripts/` 変更で smoke が起動する。

### 写しの数え上げ（変更で偽になる散文）

`grep -rn 'scripts/\*\.mjs|scripts/lib/\*\*|scripts/\*\.ps1'` を**拡張子で絞らずに**打って母集団を取った
（`--include=*.md` を掛けた最初の走査は `lib.mjs:533` を落とした——`AGENTS.md`「検証の作法」の
「除外句は狙った以外まで落とす」の実例）。生きた層の写しは **rules 2 本 + `lib.mjs:533` の 3 か所**、
残りは凍結層（ADR 3 行）と別機構（`e2e.yml`）である。

## フェーズ

### Phase 1 — 検出器（先に赤を作る）

- [x] `globToRegex` を `lib.mjs` へ移し、`rulePathPatterns`（rule の frontmatter から paths を取り出す）を新設する
- [x] `G-rules-globs.mjs` / `G-rules-globs.test.mjs` / `lib.test.mjs` を移送先へ追随させる
- [x] **移送で `G-rules-globs` の判定が逐語不変であることを実測する**——インラインのパースを
      `rulePathPatterns` へ差し替える前後で、実ツリーに対する `checkRulesGlobs` の findings が
      同一であることを確認する（既存テストが緑なだけでは、実ツリーの入力に対する不変性は言えない）
- [x] `G-rules-script-coverage.mjs` を新設する（`COVERAGE` 表・canary 3 種・死角の `//!` 宣言）
- [x] `G-rules-script-coverage.test.mjs` を書く（赤 1 / 緑 1 / canary 3）
- [x] **paths を直す前に** `npm run governance:check` を実行し、**新検査が赤くなる**ことを実測する（Red・受け入れ条件 B）
      ——期待値は未被覆 **102 件**（51 × 2 rules・research §2f の実測値）。Phase 1 の時点で赤いのが正しい
- [x] ~~`npm test` 緑~~ ——**この見積もりは誤りだった（実装中に実測して訂正）。**
      `governance-check.test.mjs:109` が実ツリーに対して `expect(findings).toEqual([])` を持つため、
      **Phase 1 の Red は `npm test` も赤にする**（1 failed / 826 passed）。計画時の grep が `process.cwd()` を
      鍵に探し、この箇所は `import.meta.url` 起点だったため母集団から落ちていた（`AGENTS.md`「検証の作法」の
      「除外句は狙った以外まで落とす」の 2 度目の実例）。**Phase 1+2 を 1 コミットに束ねる判断はこれで補強される**
      ——分ければ `npm test` も赤いコミットが履歴に残っていた

**コミット境界**: Phase 1 の末尾は `governance:check` が**意図的に赤**（102 件）である。この Red は
**コミットする状態ではなく測定である**——**Phase 1 と Phase 2 は 1 コミットに束ねる**（Red の確認は
コミット前の検証手順として実施する）。分けると、コミット済み sha を対象にするレビュー委譲が
この赤を回帰として報告する。

### Phase 2 — paths を広げる

- [x] `safety-nets.md` / `governance-docs.md` の 3 行を `scripts/**` へ畳む
- [x] `npm run governance:check` 緑（Green・受け入れ条件 C）／`npm test` 緑
- [x] `npm run governance:manifest` の `checks` 列の差分を控える（PR 本文の宣言に使う）

### Phase 3 — 記録

- [x] `docs/adr/ADR-rules-paths-subtree-coverage.md` を書く（採用・却下 4 案・受け入れた誤配送・宣言する死角）
- [x] `npm run governance:check` 緑（ADR 追加後の参照・命名検査を含む）

## レビュー対応（ラウンド 1・アンカー `7261a4e9`）

委譲先の報告は `workspace/verify-1143.txt`。Critical 0 / High 1 / Medium 2 / Low 5 / ⚠️ 4（うち 1 件は本人が撤回）。

- [x] **High-1: 母集団の下界が縛られていない** — `SCRIPT_EXT` を `.mjs` へ狭める変異が**全層で沈黙する**
  （実ツリーは全件被覆ゆえ live も narrowed も 0 件、テスト 7 本も `governance:check` も緑）。
  **本 issue が直した欠陥と同じ形が検査自身に生まれていた。** 下界を縛るテストを 1 本足し、
  変異した複製に対して**その 1 本だけが落ちる**ことを実測（1 failed / 7 passed）。
- [x] **Medium-2: `COVERAGE` に載らない rule を見ないことが死角一覧に無い** — `//!` へ追記。
- [x] **Medium-1: 「切り詰めない」と「走査が広がったら赤」の組み合わせ** — 赤が長くなることを `//!` に明記。
- [x] **Low: ADR の被覆件数（32 → 84）が HEAD では既に 86** — 本 PR 自身が 2 ファイル足すため。
  件数を落とし、「失うもの 0 件」と差分の中身だけを残した（数え上げは腐る）。
- [x] **Low: `lib.mjs` の「19 検査」** — 本変更で 20 になるため「各検査」へ。
- [x] **⚠️-1（サブエージェントへ配送された）は本人が撤回** — 問い合わせたところ、プロンプト内の引用を
  配送と取り違えた自認。**PR のマージ前チェックリストは開いたまま**にする。
- 見送り: `ctx` 未使用（既存検査と同じ形）／`SCRIPT_EXT` の大文字小文字（露出 0・`commentFamilyOf` と非対称なのは既存）。

**測定環境の欠陥を 1 つ踏んだ**: 変異の複製をリポジトリ直下（`.tmp-*/`）へ置いたところ、その `.mjs` 自身が
検査の母集団に入り、実ツリーの findings を 0 件から 2 件へ汚した。**宣言済みの死角「未追跡ファイルも入る」が
実地で出た形**である。足場はスクラッチパッドへ置くこと。

## レビュー対応（ラウンド 2・アンカー `8dd4a311`）

Critical 0 / High 1 / Medium 0 / Low 3 / ⚠️ 3。ラウンド 1 の全指摘は委譲先自身の再測定で解消を確認
（High-1 の変異で 1 failed / 7 passed を独立に再現）。逆向きの監査（コメントを除いた行の多重集合の比較）で、
修正差分が**判定行を 1 行も消していない**ことも測られた。

- [x] **High-R2-1: 母集団を狭める道は 2 つあるのに、下界のテストは片方しか縛っていない** — `//!` は
  `WALK_EXCLUDE_PATHS`（走査）と `inPopulation`（述語）の両方を狭める道として挙げながら、私が足した下界テストは
  **fixture ベースで `makeSnapshot` を呼ばない**ため走査側に反応できなかった。**独立に再現**:
  `WALK_EXCLUDE_PATHS` へ `scripts/governance` を足すと #1093 の再発形の検知が **106 件 → 0 件**へ落ち、
  `governance:check` も manifest delta も沈黙する（manifest の 4 列はどれも `scripts/` を見ず、`checks` 列は
  `readdirSync` 由来）。**実ツリーの母集団を読む canary を 1 本追加**し、走査側の狭窄で落ちること（1 failed / 8 passed）と、
  `SCRIPT_EXT` の狭窄でも落ちること（2 failed / 7 passed）を実測した。`//!` も「どちらの道をどちらのテストが縛るか」へ書き直した。
  - **私の最初の再現は本来の回帰より強かった**（`safety-nets` の他の paths まで落とし 157 → 51）。
    HEAD~1 の逐語の frontmatter で測り直して 106 → 0 を得た——**変異の強さの裁定を自分でやり直した**形である。
- [x] **Low: ADR の「差分は governance 配下と run-codex.sh」が構造的な全称で再生する** — 新しい
  `scripts/<dir>/` が現れれば偽になる（`MEMORY.md` が記録する再生パターン）。内訳を落とし、
  「失うもの 0 件」という下限の主張だけを残した。
- ⚠️-2 / ⚠️-3 は委譲先が決着（前者は今日の入力では反証不能ゆえ finding にしない、後者は 3 宣言が別々の読み手に効く）。
- **⚠️-4 は真、かつ未解消として残す**: `governance-docs` の `inPopulation` は、あちらの `paths` が
  `.claude/hooks/**` を得た日に手で直す必要があり、その手落ちを縛る層が無い（`safety-nets` は同じ欠落を 6 件で捕まえる）。
  → 死角一覧の「`COVERAGE` に載らない rule は見ない／表に足すのは手作業である」が指しているのがこれである。

## レビュー対応（ラウンド 3・アンカー `528e6c6e`）

枠組みを変えて依頼（逆向きの監査 / canary 自身への疑い / 私の ⚠️-4 判断の検算）。
Critical 0 / High 0 / Medium 2 / Low 0 / ⚠️ 4。**High-R2-1 は本人の再測定で解消を確認**（1f/8p・2f/7p を独立再現）。
逆向きの監査では、判定側の変更が `const COVERAGE` → `export const COVERAGE` の**1 行だけ**と測られた。

- [x] **Medium-1: canary が責務を越えて赤くなる（実証済み）** — `.githooks/` は母集団へ `githooks.test.mjs` を
  **1 件しか出さない**ため、そのテストを移すだけでこの検査が赤くなり、しかもメッセージは
  `WALK_EXCLUDE_PATHS` を指して**原因から目を逸らさせる**。さらに `.claude/hooks/` と `.githooks/` の 2 脚は
  **どちらの狭窄の検知にも寄与していない**と測られた。→ 2 脚を外し、`scripts/` 部分木だけを縛る形へ。
  （`detector-scope-only-as-tight-as-needed`: 検知器は必要な分だけ縛る）
- [x] **Medium-2: 前方一致だと中間層の消滅が沈黙する（実証済み）** — 接頭辞 `scripts/governance/checks/` は
  `scripts/governance/` **直下**の 13 件（`registry.mjs` を含む——#1143 の発端そのもの）が消えても真のままになる。
  → 「そのディレクトリ直下」で見る形へ変更。**述語の差し替えを分離して実測**: 中間層 13 件を落とした母集団に対し
  旧（前方一致）は true のまま・新（直下一致）は false（＝検知）。
- **⚠️-4 についての私の判断は訂正する。** 委譲先が**試作して測った**——各 rule の `paths` が主張する部分木から
  母集団を導く形（約 10 行の試作）で、現行ツリー 0 件（誤検出なし）・#1093 の再発形 106 件（検知不変）・
  `governance-docs` が `.claude/hooks/**` を得た場合は接頭辞が自動追随（母集団 85 → 91・0 件）。
  **「機構で塞げないから宣言で足りる」は実測に反する。** 本 PR では**採らない**が、理由は別である:
  検査の意味が「手で選んだ母集団を覆え」から「自分が主張する部分木を覆え」へ変わり、#837 の
  「2 rules の一致」が明示から暗黙へ落ち、方針判断とテストが増える——**本 issue の射程を越える**。
  → **フォローアップの issue 候補**として残す（死角の宣言はそのまま有効）。

## レビュー対応（ラウンド 4・アンカー `81b81b17`）— 収束

修正差分（`528e6c6e..81b81b17`）だけを対象に、**指摘を出した枠組みを再実行**させた
（`AGENTS.md`「レビュー指摘へ修正（fix-forward）を当てた」）。**Critical 0 / High 0 / Medium 0 / Low 0。**
Medium 2 件は**元の変異で再測定**して解消を確認（偽陽性の変異 → 9 passed に戻り、中間層の変異 → 1 failed）。

- **中間層を落とす変異は、ミラーでは作れない**（`lib.mjs` / `test-helpers.mjs` がその層に居てテスト自身が import し、
  `WALK_EXCLUDE_PATHS` はディレクトリ単位の完全一致ゆえ `checks/` ごと落ちる）。私が一度嵌まった罠で、
  委譲先も同じ壁に当たり、**母集団の述語側で `/^scripts\/governance\/[^/]+\.mjs$/` を除く**形で分離した。
- **脚を 2 本外したことで無防備になるものは無かった**（ラウンド 3 で本人が「未測定」と断った点の決着）。
  4 つの縮小シナリオを全 20 検査へ通したところ、**すべてで何かが発火**した。とくに `G-rules-globs`
  （この対の**順方向**）が 4 中 3 で発火する——**外した脚が守っていた面は、対の反対側が構造的に覆っている**。
  外したのは無害だからではなく、対称性の上で正しかった。

## 受容する残余（実装で塞がないもの）

- **`scripts/run-codex.sh` への誤配送 1 件**（#837 の `clean-worktrees.mjs` と同型で、意図的なトレードオフ）。
- **`globToRegex` は harness の配送判定の近似である。** 検査が言えるのは「documented な glob 意味論で
  覆われている」までで、「harness が実際に配送する」ではない（3b の [UNSURE-5]）。
- **サブエージェントへの配送は未解明である。** 2 体で挙動が割れ（1 体は配送・1 体は非配送。後者は予算消費でも
  Read/Bash の取り違えでも説明が付かないことを本人の履歴で確認済み）、**paths を広げても委譲先へ届くとは限らない**。
  この検査も paths も、**メインセッションの配送を対象にした保証である**（research §2a-2）。
- **`.mjs` / `.ps1` / `.psm1` 以外の判定コード**（`.sh` 等）は検出器の母集団に入らない。

## 未確定（実装前に潰す）

- [x] **harness の `**` がディレクトリ 2 段以上を跨ぐか** — **跨ぐ**（3 段下で実測）。配送予算が未消費の
  サブエージェントへ `.claude/skills/health-check/references/mechanized-checks.md`（`.claude/skills/` から 3 段下）を
  Read させ、`safety-nets.md` 本文の配送を逐語で確認（2026-08-19・research §2a-2）。
  → 陰性なら `scripts/governance/**` と `scripts/governance/checks/**` の 2 行へ分ける必要があったが、その分岐は消えた。
- [x] **母集団被覆形の述語が実ツリーで意図どおり動くか** — メモリ上に組んで実測（research §2f）:
  今日の main で **102 件**（51 件 × 2 rules・すべて `scripts/governance/` 配下）、`scripts/**` へ畳むと **0 件**、
  canary も発火。敵対枠の追試で `4f5f4d3f`（#1093）**直前 0 件・直後 45 件**。
- [x] **新規検査 ID を足す波及の有無** — 生きた `.md` に検査数の写しは無い（`19 検査` の文字列は
  `.superpowers/` と日付付き設計書のみ＝歴史記録）。`governance-manifest` の `checks` 列が `+1` になり
  PR 本文での宣言が要る（#1092 の設計どおり・障害ではない）。id 形式 `G-<kebab>`・数字なしを満たす。
- [x] **敵対的調査（3b）の所見の採否** — research §8 に記録（壊せた 1・壊せなかった 8・⚠️ 5 のすべてに採否と理由）。
  唯一の「壊せた」項目は受け入れ条件 C の書き方へ反映済み。

## セルフレビュー

- リスク: **高**（セーフティネットの新設・変更＝rules の `paths` と `governance:check` の検査を同時に触る）
- plan-review: 独立導出1体（`/plan-review` Step 2b。移動・圧縮・分割ではないため `--deep` は使わない）
- エージェント数: 3（3b の敵対枠 1 + 配送測定 1 + 独立導出 1）
- 要対処: **5 件中 5 件を反映**（内訳は下記「plan-review 結果」）
- 未検証: **paths 修正後の配送そのもの**（新セッションでしか測れない。陽性のみを証拠として扱い、
  PR 本文のチェックリストへ送る。`**` が 3 段跨ぐことは測定済みなので、計画の分岐には影響しない）

### 5a の自己照合（主エージェント）

1. **issue の全要件に作業項目が対応する** — 決めること 3 点はすべて「決定」節で裁定し、Phase 1〜3 が実装を持つ。
2. **境界条件を列挙し、各条件に検証がある** — rule 不在 / paths 0 件 / 母集団 0 件 / 未被覆あり / 未被覆なし の
   5 条件に、それぞれテストがある。
3. **新しい状態・リソース・プロセスに正常/失敗/破棄経路がある** — 新設するのは純関数の検査のみ。
   リソース・プロセス・永続状態を持たない（`snapshot` は既存の読み取り専用オブジェクト）。
4. **より単純な既存パターンで置き換えられないか** — `G-rules-globs` の拡張を検討し、却下（§「却下した案」3）。
   frontmatter 解析の写しは `lib.mjs` への抽出で避ける。
5. **壊してはならない不変条件に検知手段がある** — 「rules の paths が判定コードを覆う」に検知手段が無かったのが
   本 issue であり、それを置くのがこの計画である。検知手段自身の母集団の縮小には canary 3 種を置く。

## plan-review 結果

- リスク: **高**
- レビュー方式: **独立導出1体**（Step 2b。`workspace/` を読ませず、走査範囲も限定して汚染を防いだ）
- エージェント数: 1（成果物 `workspace/plan-review-1143-derive.md`）

**導出 ∩ plan**: 変更ファイル 5 件が完全一致（rules 2 本・`lib.mjs`・`G-rules-globs.mjs` とそのテスト）。
`scripts/**` の推奨と実測値も一致（被覆 32 → 84・両案とも真の上位集合・B−A は `run-codex.sh` の 1 件）。

### 要対処（5 件・すべて反映）

- **`lib.mjs:532-534` の doc コメントが偽になる** — 変更ファイル一覧へ追加済み（主エージェントも
  独立に検出。**両者が別経路で同じ 1 件へ着地した**）。
- **`globToRegex` とフロントマターのパースを `lib.mjs` へ** — 元から計画済み。根拠（`lib.mjs` 冒頭の掲載条件）も一致。
- **検出器を新設** — 設計は割れた（下記「判断の不一致」）。
- **paths を `scripts/**` へ** — 元から計画済み。
- **PR 本文へ `+G-<id>` を逐語宣言** — 検証コマンド節へ追記（**PR 作成時の作業ゆえ作業項目には置かない**）。

### 判断の不一致（1 件・裁定済み）

**検出器の形。** 導出は「深さ」形（`G-rules-paths-depth`・発火 2 件）、計画は「母集団被覆」形（発火 102 件）。
**被覆形を採る**——却下理由は §「却下した案」5（パターンが 1 本も無い相手を見ない／手で維持する
アンカー列挙を持つ）。読みやすさで劣る点は明示的に引き受けた。

### 軽微（反映したもの / しなかったもの）

- 反映: PostToolUse hook が `scripts/` の `.mjs` に検査を割り当てない（検証コマンド節へ追記）。
- 反映: `ADR-canonical-heading-references.md:37` の扱い（凍結ゆえ書き換えない・文書更新要否の節）。
- 見送り: `docs/build-commands.md:163` の "rules glob" は今も真ゆえ変更不要。
- 見送り: 生きた文書に実体のない `G-*` 言及が 4 種（**既存の問題であり本 issue の射程外**）。

### 未検証（受容する残余として計画へ記載済み）

- harness の実配送（`globToRegex` は自称「近似」）——`**` の階層跨ぎだけは実測済み（research §2a-2）。
- 新検査のフォールトインジェクション——**Phase 1 で実施する**（受け入れ条件 B）。
- `checkRulesGlobs` のパース差し替えの逐語不変性——**Phase 1 の作業項目に追加した**。

### 判断

- 実装着手: **可**（人間の承認後）

## 人間レビュー

- [x] 承認済み — 2026-08-19 / 問い: "`workspace/plan.md` を承認しますか、それとも注釈を追加しますか。" / 回答: "承認"
