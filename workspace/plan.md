# PR 1: check 系スキルの骨格を確定する — 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: `superpowers:subagent-driven-development`（推奨）または `superpowers:executing-plans` でタスク単位に実装する。ステップは `- [ ]` で追跡する。

**Goal:** check 系スキルが満たすべき 4 スロット + 費用対称性を規範として確定し、骨格自体を敵対的読者レビューで検証する。

**Architecture:** 骨格の定義は設計書（`docs/check-skill-skeleton-design.md`・コミット済み）が正本として既に持つ。本 PR が足すのは (a) 却下した統廃合案を記録する ADR、(b) 執筆時に配送されるポインタ 1 行、(c) 骨格自体への `/norm-review` の結果である。**スキル本文（`race-check` / `symmetric-check`）は本 PR では触らない**——PR 2・PR 3 の範囲。

**Tech Stack:** Markdown 規範文書、`scripts/governance-check.mjs`（G1..G11）、`/norm-review` スキル、`gh` CLI。

## Global Constraints

- **`main` へ直接コミット・プッシュしない。** 作業ブランチは `chore/check-skill-skeleton-design`（設計書コミット `11f533d` を含む・作成済み）
- **並行セッションが `main` を進めている**（本サイクルで 3 回観測）。各 Task のコミット前に `git status` を確認し、自分が変更したファイルだけをステージする
- **ADR 番号は起票直前に `ls docs/adr/` で再確認する**——並行セッションが 0011 を取りうる（本サイクルで 3 回観測）
- **`npm run governance:check` はカテゴリ F の必須検査**（`.claude/rules/` と `*.md` の変更が発火条件・`docs/build-commands.md` が SSOT）
- **PR 本文に `#781` への closing keyword を書かない。** 骨格 PR は #781 を閉じない（設計書「受容する残余」に明記）。`gh pr merge` の `--subject` / `--body-file` では止められないため、本文の側で防ぐ（ルート `CLAUDE.md`「Git/GitHub 運用」手順 1〜4）
- **`gh pr create` は push 済みかつ本ファイルに未チェックの `- [ ]` が無いことが条件**（PreToolUse hook・#749）。鎖に `cd` を含めない
- **複数行テキストを `git commit` / `gh` へ渡すときは Write ツールで一時ファイルへ書き `-F` / `--body-file` で渡す**（bash heredoc は hook が拒否する・実測）
- **先頭が `/` の文字列を `gh` の引数へ渡すときは PowerShell を使う**——Bash（MSYS）はパス変換で壊す（#781 のタイトルで実測）

---

### Task 1: ADR-0011 を起こす（却下した統廃合 4 案）

**Files:**
- Create: `docs/adr/0011-check-skill-skeleton.md`

**Interfaces:**
- Consumes: `docs/check-skill-skeleton-design.md`「検討した代替案と却下理由」節（却下 1〜4）
- Produces: Task 2 のポインタ行が指す先ではない（ポインタは設計書を指す）。Task 4 が「帰結」節へ `/norm-review` の結果を追記する

- [ ] **Step 1: ADR 番号を確定する**

Run: `ls docs/adr/`
Expected: `0010-implement-step4-report-slot.md` が最後 → 次番は `0011`。0011 が既に存在するなら次の空き番号を使い、以降の手順のファイル名を読み替える。

- [ ] **Step 2: ADR を書く**

Create `docs/adr/0011-check-skill-skeleton.md`:

```markdown
# ADR-0011: check 系スキルの共通骨格を定めるとき、統廃合とランタイム配置を却下した

`/race-check` と `/symmetric-check` へ敵対的読者レビューを当てて計 102 件の指摘を得た結果、check 系スキル（6 本）の欠陥がすべて 4 スロット（母集団・証拠・停止・接続）のいずれかに落ちることが分かった。**この ADR が残すのは、そのとき却下した案である**——採った形（4 スロット + 費用対称性）は `docs/check-skill-skeleton-design.md` が持つ。

## 文脈

ユーザーの依頼は「骨格の設計、スキルの統廃合含めて」であり、統合するか否かが論点だった。加えて骨格をどの面に置くか（実行時テキストか執筆規約か）が、スキル本文の面積に直接効くため決める必要があった。

測定済みは `/race-check`（61 件）と `/symmetric-check`（41 件）の 2 本のみで、残り 4 本（`/cache-check` `/state-check` `/persistence-check` `/dry-check`）は未測定である。

## 検討した代替案と却下理由

### 却下 1: 6 スキルを少数へ統合する（`/check <軸>` の単一スキル + 軸別 reference）

6 本は母集団が互いに素であり、統合の利得は「重複した根拠の規律 1 文」に留まる。対して写しは**少なくとも次の 4 面**に分散している（列挙は網羅ではない・実測で数えていない）: `AGENTS.md`「条件別チェック（トリガー → 参照先）」表・`.claude/rules/` の各ファイル・`/implement`「4a. check スキルの実行」の列挙・各スキルの frontmatter `description`（harness のルーティング面）。**影響半径が利得に見合わない**。重複した 1 文は骨格が回収するので、統合の動機は残らない。

### 却下 2: `/dry-check`（63 行・最小）を他スキルへ吸収する

薄いのは停止・接続スロットが空だからであって、母集団は健全である（関数の主要操作＝構造物アンカー）。かつ「重複」の概念は他 5 本のどれとも異なり、吸収先が存在しない。**薄さは統合の理由にならない**——埋めるべきスロットが空なだけである。

### 却下 3: `/race-check` 4b のリソース対称の検査を `/symmetric-check` へ寄せる

重複は実在する（in-flight への insert と全終端での remove の対 ≒ 生成 → 登録 → 破棄の対）。しかし寄せると**片方しか起動されない経路**ができる——`AGENTS.md` の条件別チェック表は両者を別トリガーで振り分けており、並行性の変更で `/symmetric-check` が起動される保証は無い。ルーティング依存を新設する損が、重複 1 件の解消を上回る。**保留ではなく却下**とした。

### 却下 4: 骨格を実行時に読ませる（`_shared/check-skeleton.md` を各スキルが参照）

**これが本サイクルで最も価値のある否定の知識である。**

骨格を読む必要があるのは検査を実行するエージェントではなく、**スキルを書く人**である。実行側が「検査は母集団を定義しなければならない」を読んでも役に立たない——*その*スキルの母集団が定義されていることだけが要る。

加えて進行的開示は「読む契機」が弱いと読まれない。本 ADR に至った作業の途中で `/race-check` の理由を `references/rationale.md` へ退避する案を試し、**それが既存の `docs/adr/0003-*` の言い換え（しかも実測値を落とした劣化コピー）でしかないことに、書き終えてから気づいた**。理由を別ファイルへ出す判断は、正本探しを伴わなければ写しの製造になる。

ゆえに骨格は**執筆規約**とし、スキル本文には埋めた結果だけを残す。実行時の面積は増えない。

## 帰結

- `.claude/rules/safety-nets.md` へポインタ 1 行（実測 131 字）を足した。`AREA_BUDGET.rules` の余白（100 字）に収まらず引き上げた——理由は定数のコメント（`scripts/governance-check.mjs`）に書いた。新基準は過去 4 回と同じく**引き上げ後の実測値 + 100** である（ゼロ余裕は ADR-0005 が明文で却下している）。ADR-0005 が警告する「反射的な引き上げ」に当たらない根拠は、旧基準も同じ許容差を含むため**旧基準からの引き上げ幅がポインタ 1 行の実測値そのものになる**こと: `(7956+131+100) - (7956+100) = 131`
- **前後比較のベースラインを本 ADR へ記録した。** `/norm-review` の成果物は `workspace/` に置かれ `/implement`「Step 5 — コミット」が削除するため、削除されない面へ一次記録を残す必要がある（設計書「検証」）。基準は生ファイルではなく**「合格条件 N 件のうち何件が突破可能だったか」の対**である:

| 対象 | 合格条件 | 1 巡目時点の突破 | 総指摘数 |
|---|---|---|---|
| `/race-check`（肯定形書き換え版・破棄済み） | C1: ①②③だけで非該当 / C2: 実装レビューなのに計画レビュー側を選ぶ / C3: 手順 3 だけで 1・2 を飛ばす / C4: ADR で免除 / C5: ⑤⑥ で足りる | **5/5** | 61（19+20+12+10） |
| `/symmetric-check`（現行） | S1: 片方だけ変更で「適用不要」/ S2: 生成・破棄の非対称を見ずに終える / S3: 「対称ペアなし」で終える / S4: 根拠のない `[不要]` | **4/4** | 41（22+19） |
- **測定済みは 2 本のみである。** 残り 4 本の採点（設計書「現状の採点」）は文言の読みに基づく推定であり、2 クラス読者を当てていない。骨格が PR 2・PR 3 で効くことを確認してから測る
- **骨格の遵守を測る検出器は置かない。** `governance:check` が測れるのは見出しの存在までで、中身の妥当性（母集団が本当に一意か・費用が本当に対称か）は機械化できない。`docs/development-principles.md`「構造的設計原則と強制の階梯」に従って (a) 責務を検査可能な層へ移す・(b) 観測点を作る を検討したが、いずれも成立しない。**受容する残余として記録する**
- `#781`（`/race-check` の先行欠陥 20 件）は本 ADR の PR では閉じない。骨格で閉じるのは 11 件の見込みで、残り 9 件は `/race-check` 固有の中身である

---

status: Accepted
関連: #781 ・`docs/check-skill-skeleton-design.md` ・ADR-0003（却下 4「義務を足すときは母集団を絞る」の一般化元）・ADR-0005（面積 ratchet の引き上げ規律）・ADR-0010（skills 本文が面積の母集団外であること）・#593（否定の知識の ADR 回収）
```

- [ ] **Step 3: governance:check を走らせる**

Run: `npm run governance:check`
Expected: `G1..G11 passed`。ADR 内の見出し参照（`AGENTS.md`「条件別チェック（トリガー → 参照先）」・`/implement`「4a. check スキルの実行」・`docs/development-principles.md`「構造的設計原則と強制の階梯」）が実在照合を通ること。**赤なら参照先の見出し文言を実ファイルから copy して直す**（推測で直さない）。

- [ ] **Step 4: コミット**

コミットメッセージを Write ツールで一時ファイルへ書き、`git commit -F <path>` で渡す（パス区切りは `/`）。

```
docs(adr): check 系スキルの骨格を定めるとき統廃合とランタイム配置を却下する

6 本は母集団が互いに素で、統合の利得は重複した規律 1 文に留まる一方、
写しは AGENTS.md の表・rules・/implement 4a・frontmatter description に
分散しており影響半径が見合わない。dry-check の薄さは停止・接続スロットが
空であることの帰結で、統合の理由にならない。race-check 4b と
symmetric-check 2b の重複は実在するが、寄せると片方しか起動されない
経路ができるため保留ではなく却下する。

骨格を実行時に読ませる案も却下した。骨格を要するのは実行側ではなく
書き手であり、かつ理由を別ファイルへ出す判断は正本探しを伴わなければ
写しの製造になる（本サイクルで references/rationale.md が ADR-0003 の
劣化コピーになった実測）。

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
```

Run: `git add docs/adr/0011-check-skill-skeleton.md && git commit -F <path>`

---

### Task 2: rules へポインタ 1 行を足し、面積予算を実測値へ引き上げる

**Files:**
- Modify: `.claude/rules/safety-nets.md`（「セーフティネットが「規範」（ドキュメント・スキル・チェックリスト）の場合…」節の末尾）
- Modify: `scripts/governance-check.mjs:604`（`AREA_BUDGET.rules`）と直前のコメント

**Interfaces:**
- Consumes: Task 1 の ADR（参照はしない。ポインタが指すのは設計書）
- Produces: 骨格が「スキルを触ったときに配送される」状態。PR 2・PR 3 の作業者はこの行から設計書へ到達する

- [ ] **Step 1: 追加前の面積を実測する**

Run: `npm run governance:check`
Expected: 出力に `rules 7956/8056 字` が含まれる。**この 7956 を記録する**（以降の引き上げ幅の根拠になる）。値が違っていたら並行セッションが rules を変えているので、実測値を採用して以降の計算をやり直す。

- [ ] **Step 2: ポインタ行を足す**

`.claude/rules/safety-nets.md` の「規範は機構ではないので実行して測れない——…パスに属する規範（`.claude/rules/` `.claude/skills/` および規範文書）を新設・変更したら起動する。」の**直後**へ、次の 1 行を足す:

```markdown
- **check 系スキル（`*-check`）は 4 スロット（母集団・証拠・停止・接続）と費用対称性を満たす**——定義は `docs/check-skill-skeleton-design.md`
```

- [ ] **Step 3: 追加後の面積を実測し、引き上げ幅を確定する**

Run: `npm run governance:check`
Expected: **G10 が赤**になり `rules <新しい実測値>/8056 字` が出る。その実測値を記録する。

（見積もりは約 8070。ポインタ行は約 114 字で、余白 100 字を超えるため。**見積もりではなく出力の実測値を使う**。）

- [ ] **Step 4: AREA_BUDGET.rules を実測値へ引き上げ、理由を書く**

`scripts/governance-check.mjs:604` を **実測値 + 100** へ更新する。**`+100` を省いてはならない**——同ファイルの `AREA_BUDGET` 直前のコメントが「**ゼロ余裕で据えると、あらゆる編集が定数の書き換えを要求して摩擦が日常化し、赤の意味が失われる（ADR-0005）**」と明記しており、過去 4 回の引き上げもすべてこの形である。

```javascript
export const AREA_BUDGET = { alwaysLoaded: 13374, rules: <Step 3 の実測値 + 100> };
```

同ファイルの `AREA_BUDGET` 直前のコメントブロック（「上げるときに要るのは我慢ではなく理由であり、その理由をここへ書き足す摩擦が、合意の場を作るための設計である」で終わる段落）の末尾へ、次を追記する:

```
 * 2026-07-27: rules を 8056 → <実測値> へ引き上げた（+<差分> 字）。check 系スキルの骨格
 * （4 スロット + 費用対称性・docs/check-skill-skeleton-design.md）
 * へのポインタ 1 行を safety-nets.md へ足したため。ADR-0005 が警告する反射的な引き上げに
 * 当たらない根拠は、引き上げ幅がその 1 行の実測値ちょうどであること——本文は 1 文字も増えて
 * いない。骨格そのものは面積対象外の設計書に置き、rules へは到達経路だけを置いた（ADR-0011 却下 4）。
```

- [ ] **Step 5: governance:check が緑に戻ることを確認する**

Run: `npm run governance:check`
Expected: `G1..G11 passed`。参照した設計書のパスが実在照合を通ること。

- [ ] **Step 6: コミット**

コミットメッセージを一時ファイルへ書いて `git commit -F <path>`:

```
chore(rules): check 系スキルの骨格へのポインタを safety-nets へ置く

骨格を要するのは検査を実行する側ではなく書き手なので、定義は面積対象外の
設計書に置き、rules へは到達経路 1 行だけを置く（ADR-0011 却下 4）。

AREA_BUDGET.rules を実測値へ引き上げた。引き上げ幅はポインタ 1 行ちょうどで、
本文は 1 文字も増えていない——理由は定数のコメントへ書いた。

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
```

Run: `git add .claude/rules/safety-nets.md scripts/governance-check.mjs && git commit -F <path>`

---

### Task 3: 骨格自体に 2 クラス読者を当てる

**Files:**
- Modify: `docs/check-skill-skeleton-design.md`（成立した指摘を塞ぐ場合）
- Modify: `docs/adr/0011-check-skill-skeleton.md`（同上）
- Modify: `.claude/rules/safety-nets.md`（同上）

**Interfaces:**
- Consumes: Task 1・Task 2 の成果物
- Produces: Task 4 が ADR「帰結」へ記録する巡ごとの件数と等式

**この Task が PR 1 の本体である。** 骨格は規範であり、`.claude/rules/safety-nets.md` 自身が「パスに属する規範を新設・変更したら `/norm-review` を起動する」と要求している。骨格が骨格の要求を免れることはない。

- [ ] **Step 1: 停止条件を書き出す**

着手前に次を確定し、Task 4 で報告する形で記録する:

- **合格条件**（通してはならないシナリオ）5 件:
  - K1: 4 スロットのうち一部だけ埋めて「骨格に適合」と主張できる
  - K2: 「母集団」の定義が、差分アンカーのスキルで具体的に何をすればよいか決まらない
  - K3: 「空の母集団は結果ではない」が、母集団を意図的に空にして検査を回避する口実になる
  - K4: 費用対称性の検算（「母集団を固定して結論を反転させる」）が実行不能・または恣意的に判定できる
  - K5: 「執筆規約であって実行時テキストではない」が、実行時に骨格を無視してよいと読める
- **上限 2 巡**
- **塞ぎ 1 件あたり 1 文**（超えるものは受容する残余へ）
- **上限時点で残るものは受容する未対応リスクとして明記する**

- [ ] **Step 2: 1 巡目の 2 クラス読者を起動する**

各読者へ渡すもの: **対象の文言だけ**（設計書「骨格 — 4 スロット」節 + 「貫く性質 — 費用対称性」節 + Task 2 のポインタ行を本文へ貼る。実装コードもスキル本文も読ませない）、Step 1 の合格条件、**書かせる絶対パス**。

書き出し先（**巡ごと・読者ごとに別パス**）:
- 手を抜く読者: `<scratchpad>/norm-review/skel-r1-lazy.md`
- 規則を全部守る読者: `<scratchpad>/norm-review/skel-r1-strict.md`

指示に含めるもの: **抜け道は 1 件 1 番号の箇条**で書かせる（出力の等式は番号を数え直して立てるため）。`name:` パラメータは渡さない——teammate 化して最終テキストが届かなくなる（実測）。

- [ ] **Step 3: 件数をファイルから数え直す**

Run: `grep -cE '^ *[0-9]+\. ' <各ファイル>`
Expected: N（lazy）と M（strict）が得られる。**会話へ写した件数で左辺を立ててはならない**（`/norm-review`「出力」）。不着・様式違反のクラスは件数ではなく「不成立」と書き、等式はそのクラスを除いて立てる。

- [ ] **Step 4: 各指摘の成立を自分で確認し、成立したものだけを塞ぐ**

各指摘について、引用された文言を**自分で開いて**成立を確認する。成立しないものは降格し、**降格した事実も記録に残す**。塞ぎは 1 件 1 文。

**塞ぎが列挙の形になっていないか確認する**——保護対象を列挙すると逃げ道は列挙の隣へ移る。列挙ではなく原理で書けないかを先に問う（`/norm-review`「Step 3」）。機構への言及も検出面の列挙として働く。

- [ ] **Step 5: 2 巡目を、塞ぎ後の文言で起動する**

Step 2 と同じ手順。パスは `skel-r2-lazy.md` / `skel-r2-strict.md`（**前巡のパスを再利用しない**）。渡すのは**塞ぎ後の文言**である。

**1 巡目の修正自身が新しい抜け道を作る**（#764 実測・本サイクルでも `/race-check` で実測）。特に「実行不能を作っていないか」を見る——義務を足す方向の塞ぎは `ADR-0003` 却下 4 の失敗様態を再現する。

- [ ] **Step 6: 2 巡目を Step 3・Step 4 と同じ規律で処理する**

上限 2 巡に達したら止める。**最終巡の塞ぎは検証されないまま残る**——受容する残余に書く。

- [ ] **Step 7: 塞ぎがあった場合のみ governance:check とコミット**

Run: `npm run governance:check`
Expected: `G1..G11 passed`。**塞ぎで rules の面積が増えていたら、Task 2 Step 3〜5 と同じ手順で予算を再調整する**（理由コメントを追記する。既存の 1 行を書き換えない）。

塞ぎが 0 件なら、このステップはコミット不要。その事実を Task 4 に記録する。

---

### Task 4: レビュー結果を ADR へ記録し、PR を作る

**Files:**
- Modify: `docs/adr/0011-check-skill-skeleton.md`（「帰結」節）
- Delete: `workspace/plan.md`

**Interfaces:**
- Consumes: Task 3 の巡ごとの件数・等式・降格・残余
- Produces: PR。PR 2（`/race-check` を骨格へ寄せる）の入口

- [ ] **Step 1: ADR「帰結」へレビュー結果を追記する**

`docs/adr/0011-check-skill-skeleton.md` の「帰結」節へ次の形で足す（`<>` は Task 3 の実測値で埋める）:

```markdown
- `/norm-review` を <巡数> 巡回した。1 巡目 <N>+<M> = 塞いだ <K> + 降格 <L> + 残余 <R>、2 巡目 <同形>。件数は読者が書いたファイルの番号を数え直して立てた。<2 巡目に 1 巡目の塞ぎ由来の指摘があればその件数と、縮める方向だったか広げる方向だったかを 1 文で>
```

- [ ] **Step 2: 受容する残余を ADR へ書く**

Task 3 で塞がなかった指摘を、**番号ごとに理由を付けて**「帰結」節へ足す。最終巡の塞ぎが未検証であることも書く。

- [ ] **Step 3: plan.md の全項目が `- [x]` であることを確認する**

Run: `grep -c '^- \[ \]' workspace/plan.md`
Expected: `0`。**1 件でも残っていれば PR を作らない**——完了させるか、やらないと決めた項目を計画から外して理由を ADR か issue へ記録する。

- [ ] **Step 4: workspace/ を削除してステージする**

否定の知識は Task 1 の ADR-0011 が既に回収済みなので、追加の ADR は起こさない。

Run: `git rm -r workspace/`

- [ ] **Step 5: 記録と削除をコミットする**

コミットメッセージを一時ファイルへ書いて `git commit -F <path>`:

```
docs(adr): 骨格への敵対的読者レビューの結果を ADR-0011 へ記録する

骨格は規範であり safety-nets.md 自身の要求対象なので、骨格が骨格の要求を
免れることはない。巡ごとの件数・降格・残余を記録した。

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
```

- [ ] **Step 6: PR 本文を書く**

Write ツールで一時ファイルへ書く。**`#781` への closing keyword（`close`/`fix`/`resolve` の 9 形）を書かない**——骨格 PR は #781 を閉じない。参照するときは「`#781` の 20 件のうち 11 件が骨格で閉じる見込み（本 PR では閉じない）」のように**動詞を避けて**書く。

本文に含めるもの:
- 目的（骨格の確定）と、スキル本文を触らないこと
- ADR-0011 の却下 4 案の見出しだけ
- `/norm-review` の巡ごとの等式
- `AREA_BUDGET.rules` の引き上げ幅とその根拠
- 次の PR（PR 2: `/race-check` を骨格へ寄せ、C1〜C5 で前後比較する）

- [ ] **Step 7: push して PR を作る**

Run: `git push -u origin HEAD && gh pr create --title "..." --body-file <path>`

**鎖に `cd` を含めない**（対象リポジトリを判定できず hook に拒否される）。**リダイレクト（`2>&1`）を挟まない**（単独 `&` が hook の `hasSafeChain` で非-`&&` 区切りと数えられ block される・#578 実測）。タイトルに先頭 `/` のスキル名を含めるなら PowerShell を使う。

- [ ] **Step 8: closing keyword の一覧を確認する**

Run: `gh pr view <PR> --json closingIssuesReferences`
Expected: **空**。`#781` が現れたら PR 本文を編集し、**一覧から消えるまで**繰り返す（`gh pr edit <PR> --body-file <tmp>`）。自分のキーワード走査ではなく**この一覧**が編集を終えてよいと決める。

- [ ] **Step 9: `#781` が OPEN のままであることを確認する**

Run: `gh issue view 781 --json state`
Expected: `OPEN`。

---

## セルフレビュー

**1. 設計書のカバレッジ:** 設計書の「移行の順序」1（ADR + rules ポインタ）＝ Task 1・2、2（骨格自体に 2 クラス読者を当てる）＝ Task 3。3・4（`/race-check` `/symmetric-check` の書き換え）は PR 2・PR 3 で本計画の範囲外——設計書がそう分けている。「配置と面積予算」表の 4 行のうち、設計書は commit 済み、ADR は Task 1、rules は Task 2、スキル本文は範囲外。**漏れなし。**

**2. プレースホルダ:** Task 2 Step 4 の `<Step 3 の実測値>` と Task 4 Step 1 の `<N>` 等は、**実行時に測って埋める値**であって未定の設計判断ではない。見積もり（約 8070）を併記したうえで「見積もりではなく実測値を使う」と明示している。ADR 本文・rules の 1 行・コミットメッセージ・PR 本文の方針はすべて実文で書いてある。

**3. 型の一貫性:** ADR 番号は Task 1 Step 1 で確定し、Task 3・4 が同じファイル名を参照する。`AREA_BUDGET.rules` は Task 2 が実測値へ上げ、Task 3 Step 7 が「増えていたら同じ手順で再調整」と参照する。読者の成果物パスは Task 3 Step 2・5 で巡ごとに別名（`skel-r1-*` / `skel-r2-*`）。**不整合なし。**
