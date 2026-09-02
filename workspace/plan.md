# plan — issue #761: 「文書としての plan.md」レンズを常設しない決定を ADR へ凍結し、issue を閉じる

ブランチ: `chore/close-761-plan-doc-lens-adr` ／ 調査: `workspace/research.md` ／ 敵対的調査: `workspace/adversarial-761.txt`

## 目的

ユーザー裁定（2026-09-02「常設しない・ADR で閉じる」）を、否定の知識として `docs/adr/` へ凍結する。issue の考えどころ 4（レンズの定義を残す判断）を ADR が引き受け、issue は PR で閉じる。**スキル・rules・hook は変更しない。**

## 受け入れ条件

1. `docs/adr/ADR-plan-document-lens-not-permanent.md` が新設され、次を持つ
   - 決定: 4 レンズを `/plan-review` へ常設しない
   - 経緯（実測）: #749 で標準構成が拾えなかった 15 件超と 4 型（issue の表）、当時の標準構成が #849 で撤去されたこと、4 型のうち 2 型が別の受け皿を得たこと（`research.md` F2）
   - 却下した案（否定の知識）: (1) 4 レンズの常設、(2) 高リスク時の Step 2 観点候補への 1 行追加、(3) Step 1 自己照合への「節間の覆い」1 問追加、(4) 内部矛盾レンズの機械化
   - 受容する残余: 節間の覆い・責務分割 MECE に検知手段が無いこと。レンズ定義の所在
   - 関連: `ADR-risk-tiered-plan-review`・`ADR-retire-norm-review`・`ADR-window-coordinator-split-rule`（責務分割 MECE レンズの成果が着地した唯一の生きた層）・#749・#759・#762・#914
2. ADR 本文に**序数参照**（「Step 1 項目 6」型・ADR 連番）と、消滅した節の正準形参照が無い（`.claude/rules/governance-docs.md`）。旧構成（スカウト 3 + 独立導出 1）は散文で書く
3. ADR 内の全称表現（「どこにも無い」「唯一」）は、それぞれ「何が増えたら偽になるか」を 1 つ添えるか下限の主張へ弱めてある（`AGENTS.md`「検証の作法（全タスク共通）」）
4. ADR が `docs/development-principles.md` から `ADR-plan-document-lens-not-permanent` の短縮引用で引かれている（U1 裁定）。計器分割設計書の冒頭注記にも同じ引用が在る（U2 裁定）
5. `npm run governance:check` が全検査 passed、`git diff --check` が空
6. PR 本文に `Closes #761` を置く（`/merge-pr` の手順で closingIssuesReferences を確認する。マージで閉じる issue は #761 のみ）

## 変更ファイル一覧と対象シンボル

| ファイル | 変更 | 備考 |
|---|---|---|
| `docs/adr/ADR-plan-document-lens-not-permanent.md` | **新設** | 書式は `ADR-retire-norm-review.md` に倣う（根拠の集約 → 却下 n → 受容する残余 → 関連） |
| `docs/development-principles.md` | 「検証の層と、層と層の隙間」へ 1 文（ADR の短縮引用） | U1 裁定。`governanceDocs` に含まれ G-adr-citations が実在を照合する |
| `docs/superpowers/specs/2026-07-29-plan-review-instrument-split-design.md` | 冒頭に注記 1 行 | U2 裁定。先例: `2026-07-28-plan-review-loop-design.md` 冒頭の `> **Superseded:**` |
| `workspace/research.md` / `workspace/plan.md` | 本サイクルの成果物 | `/retrospective` で撤去 |

触らないもの: `.claude/skills/**`・`.claude/rules/**`・`AGENTS.md`・`CLAUDE.md`・既存 ADR（凍結）・`RETROSPECTIVE.md`・`SPEC.md`。

## 実装順序

### Phase 1 — ADR の作成

- [ ] `docs/adr/ADR-plan-document-lens-not-permanent.md` を書く。構成:
  1. 見出し `# ADR-plan-document-lens-not-permanent: 「文書としての plan.md」レンズを /plan-review へ常設しない`。日付 2026-09-02 ／ 状態: 承認
  2. **決定**（1 段落）
  3. **経緯（実測）**: #749 の 4 レンズ・15 件超・所見の 4 型と重み（issue の表を要約。数値は issue の引用として書き、現在形の主張にしない）。標準構成の撤去（#849）——issue が引く「Step 2b をトリガ列挙 0% で常時実施へ倒した前例」（#495/#502）も同じ決定で高リスク限定へ戻っている。受け皿の現状: 実行可能性 → `/plan-review` Step 1 の「タスク分割の境界が既存トリガーを跨いでいない」〔#914〕、内部矛盾 → `AGENTS.md`「検証の作法（全タスク共通）」の「数ではなく正本を指す」、節間の覆い → 同 Step 1 の概念ラベル grep〔#914〕が**部分的に**（変更ファイル一覧と散文の照合であって、計画内の節どうしは見ない）、責務分割 MECE → 個別 ADR（`ADR-window-coordinator-split-rule` 規則 R）に成果だけが着地し汎用の受け皿は無い
  4. **却下 1: 4 レンズを常設する**——通常リスクで 0 → 4 体。`ADR-risk-tiered-plan-review` の却下案と同形。捕捉実績は #749 の 1 サイクルのみで、`ADR-retire-norm-review` が記録した「敵対的読者の指摘は対象の質と無関係に増える」機序が当たる
  5. **却下 2: 高リスク時の Step 2 の観点候補へ 1 行足す**——Step 2 は「該当するリスク観点だけを渡す」形で観点を固定しておらず、責務分割 MECE は高リスク条件「複数モジュール間のインターフェース」に既に含まれる。足すのは写しになる
  6. **却下 3: Step 1 自己照合へ「節間の覆い」を 1 問足す**——#914 と同型だが、捕捉記録が #749 の 1 件で、`ADR-two-class-reader-discrimination` が記録した「義務を足す方向は正しい修理ではない」に照らして実績が足りない。再提案の条件を書く: 同型の手戻り（節で宣言した編集が手順に無いまま `/implement` へ渡る）が**もう 1 度**記録されたとき
  7. **却下 4: 内部矛盾の機械化**——`AGENTS.md` の「数ではなく正本を指す」の下では計画が数を運ばないため、突き合わせる対象が消える
  8. **受容する残余**: 節間の覆い・責務分割 MECE を見る検知手段は無い（主エージェントの自己照合と人間レビューが残余の受け皿）。レンズ 4 本の定義は `git show 5ef346f:workspace/plan-review/<name>.md`（4 ファイル名を列挙）にだけ在る——**生きた層へ写さない**理由（書く約束の「必要なことだけ」——常設しないレンズの定義は生きた層の誰も読まない・`ADR-doc-promise-over-area-ratchet`）
  9. **関連**: 計器分割設計書が #761 を「plan.md の機械可読性」と読み替えていること、その問いは本 ADR の対象外であり spike が支持した時点で別 issue を起こすこと
- [ ] 序数参照・消滅した節の正準形・全称表現を自分で grep して検算する（`grep -nE "項目 [0-9]|ADR-[0-9]{4}|どこにも|唯一|すべて" docs/adr/ADR-plan-document-lens-not-permanent.md`）

### Phase 2 — 生きた層からの引用と歴史資料への注記（U1/U2 の裁定済み）

- [ ] `docs/development-principles.md`「検証の層と、層と層の隙間」の「ゆえに層を選ぶときは 2 つ問う」段落の直後へ 1 文を足す。趣旨: 「計画文書そのものの内部整合（節どうしの覆い・責務分割の判別規則）を見る層は常設しない——要る計画では臨時に回し、常設しない判断と観点の索引は `ADR-plan-document-lens-not-permanent`」。序数・件数を書かない。書く約束（かぶりなく・必要なことだけ・古い情報を残さない）に照らし、同 doc 内に同趣旨の文が無いことを grep で確かめてから足す
- [ ] `docs/superpowers/specs/2026-07-29-plan-review-instrument-split-design.md` の冒頭（見出し直下）へ注記 1 行: `> **注記（2026-09-02）:** #761 は「文書としての plan.md」レンズの常設可否を扱い、常設しない決定で閉じた（` `ADR-plan-document-lens-not-permanent` `）。本設計書が #761 に置いた「plan.md の機械可読性」の問いは未着手のままであり、§5 の spike が仮説を支持した時点で別 issue を起こす`。本文は変えない

### Phase 3 — 検証

- [ ] `npm run governance:check`（カテゴリ F）全検査 passed
- [ ] `git diff --check` が空
- [ ] `git grep -n "ADR-plan-document-lens-not-permanent"` で引用が ADR 自身・`docs/development-principles.md`・計器分割設計書の 3 か所に在ることを確認

## 不変条件と異常系

- 既存 ADR は編集しない（`ADR-adr-frozen-history`）。`ADR-risk-tiered-plan-review` へ追記したくなるが、新 ADR の「関連」から一方向に引く
- ADR から生きた層への参照は照合されない。本文で SKILL.md の条項を指すときは**文言**で指し、序数を使わない
- ADR 名は連番禁止・kebab-case（G-adr-file-names が赤にする）
- `*.md` の編集に PostToolUse 検査は走らない。reminder の沈黙を緑と読まない

## テスト方針と検証コマンド

- `npm run governance:check` ／ `git diff --check` ／ 上記 grep 2 本
- CI: PR の governance-check job（`skip-ci` 非対象）

## SPEC.md・関連文書の更新要否

- `SPEC.md`: 不要（プロダクト挙動に触れない）
- `docs/architecture.md` / `docs/build-commands.md`: 不要
- `RETROSPECTIVE.md`: 本 PR では触らない（サイクル末の `/retrospective`）

## 未確定（実装前に潰す）

- [x] **U1. ADR を生きた層のどこから引くか** — 裁定（2026-09-02・ユーザー）: **(d) `docs/development-principles.md` へ 1 文**。候補 (a) SKILL.md はスキル変更ゆえ、(b) 設計書注記は歴史資料ゆえ恒久の引用先に向かず、(c) 引かないは #895 型の削除リスクを負う。置き場は「検証の層と、層と層の隙間」の「ゆえに層を選ぶときは 2 つ問う」段落の直後に 1 文（計画本文 Phase 2 に反映）
- [x] **U2. 計器分割設計書の「#761 に依存」の扱い** — 裁定（同上）: **冒頭に注記 1 行を足す**（loop-design 設計書の Superseded 注記が先例）。独立導出の「触らない」は採らない——読者が §7 手順 3 から閉じた issue へ導かれるのを、文書自身が止める形を優先する
- [x] **U3. ADR に 4 レンズの定義を写すか** — 裁定（同上）: **1 行要約 + `5ef346f` へのポインタ**。各レンズ「何を見るか」を 1 行ずつと 4 ファイル名。定義文の全文は写さない
- [x] **敵対的調査（`workspace/adversarial-761.txt`）の所見を反映した** — 壊せた 0 / 壊せなかった 7 / ⚠ 3。採否は `research.md`「敵対的調査（3b）の所見と採否」。反映: ADR の経緯に「責務分割 MECE レンズの成果は `ADR-window-coordinator-split-rule` の規則 R として生きた層に残る」を足し、関連にも同 ADR を現行 slug で加える（issue 本文の旧番号形式は写さない）

## plan-review 結果

- リスク: 高（ガバナンス文書＝ADR の新設）
- レビュー方式: 独立導出1体（`workspace/plan-review-761-derivation.md`・計画と調査を読ませず導出）
- エージェント数: 1

### 要対処
- 節間の覆いの受け皿を「無し」→「Step 1 の概念ラベル grep が部分的に」へ — 計画の修正（Phase 1 の経緯へ反映済み） — 再照合: `/plan-review` SKILL.md Step 1 の 7 番目の条項は変更ファイル一覧 ↔ 散文の照合であり、計画内の節どうしは見ない。「部分的」が正確
- issue の前提「Step 2b 常時実施の前例」が #849 で戻っている事実を ADR の経緯へ — 計画の修正（反映済み） — 再照合: SKILL.md「リスク判定」に「Step 2 または Step 2b のどちらか一方」が実在

### 軽微
- 引用先の候補 (d) `docs/development-principles.md` を U1 へ追加（反映済み）
- issue 本文の旧見出し「Step 2b — 独立導出 + 差分…」を ADR に正準形で写さない（受け入れ条件 2 が既に覆う）
- 範囲外: `scripts/plan-review-ledger.mjs` / `npm run plan:ledger` が #849 以降どのスキルからも呼ばれていない。本 issue では触らず報告のみ

### 未検証
- 判断の不一致 1 件（U2: 計器分割設計書へ注記するか）は根拠が両立するため機械では決まらない。人間の裁定へ

### 判断
- 実装着手: 可（U1〜U3 は 2026-09-02 に人間が裁定し、計画へ反映済み）

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー1体（独立導出）
- エージェント数: 1（plan-review）+ 1（`/start-issue` 3b 敵対的調査・sonnet）
- 要対処: 2 件（上記・反映済み）
- 未検証: U2 の判断の不一致は人間の裁定に委ねる

## 人間レビュー

- [x] 承認済み — 2026-09-02 / 問い: "以上を反映した workspace/plan.md（ADR 1 本新設・スキル不変更・PR で #761 を Closes）を承認しますか？" / 回答: "承認する"（同時に U1 "docs/development-principles.md へ 1 文"・U2 "冒頭に注記 1 行を足す (Recommended)"・U3 "1 行要約 + 5ef346f へのポインタ (Recommended)" を裁定）
