# rules-delivery

## 問題なし
- `.claude/rules/safety-nets.md` の frontmatter `paths` は `.claude/skills/**` を含む — 根拠: `.claude/rules/safety-nets.md:8`。check 系スキル（`race-check`/`symmetric-check` 等）を触れば本ファイルは既に自動配送されており、計画の前提（「スキルを触ったときに配送される」）は成立する。
- 挿入位置に指定された文言「規範は機構ではないので実行して測れない——…パスに属する規範…を新設・変更したら起動する。」は実ファイルに一致 — 根拠: `.claude/rules/safety-nets.md:26`（省略記号部分含め完全一致）。
- ポインタ行が指す `docs/superpowers/specs/2026-07-27-check-skill-skeleton-design.md` は実在しコミット済み — 根拠: `git log --oneline -- docs/superpowers/specs/2026-07-27-check-skill-skeleton-design.md` → `11f533d docs(specs): check 系スキルの共通骨格を...`。
- ポインタ行が主張する「4 スロット（母集団・証拠・停止・接続）」「費用対称性」は設計書の実見出しと一致 — 根拠: 設計書 20行目 `## 骨格 — 4 スロット`、24/53/61/71行目の各スロット見出し、63行目「費用は母集団のサイズで決まる」。
- `.claude/rules/safety-nets.md` に「check 系スキル」「4 スロット」「費用対称性」への既存言及は無く、追加行との重複は無い — 根拠: `grep -rn "check-skill-skeleton|4 スロット|費用対称性" .claude/` → No matches。
- `AGENTS.md`「条件別チェック」表の「セーフティネット…を新設/変更」行は、行き先として `safety-nets.md` 自身を指しており、ポインタの行き先（設計書）とは経路上で衝突しない（AGENTS.md → safety-nets.md → 設計書の単一鎖） — 根拠: `AGENTS.md:62`。
- Task 2 Step 1 の期待値「rules 7956/8056 字」は実測と一致 — 根拠: `npm run governance:check` 出力 `恒久規範 常時ロード 13274/13374 字・rules 7956/8056 字`。
- Task 2 Step 4 が指す `AREA_BUDGET` 直前コメントの結び「上げるときに要るのは我慢ではなく理由であり、その理由をここへ書き足す摩擦が、合意の場を作るための設計である。」は実ファイルと一致 — 根拠: `scripts/governance-check.mjs:601-602`。

## 軽微な懸念
- ポインタ行の実測文字数は 131 コードポイント（CR 除く・`countChars` と同じ数え方で算出）であり、計画の見積もり「約 114 字」から 17 字（約 15%）ずれている — 根拠: Node で `[...line].length` を実測。計画は Step 3 で「見積もりではなく出力の実測値を使う」と明記しており実害は無いが、Task 2 冒頭の「約 8070」という合計見積もりも同程度ずれる（実際は 7956+131=8087 付近）。
- Task 2 の **Files** 節は挿入先を「節の末尾」と表現しているが、実際の挿入位置（Step 2 が指定する直後）は当該節の 1 個目の bullet（`safety-nets.md:26`）の直後であり、2 個目の bullet（`safety-nets.md:28`「条項を足す前に…」）より前＝節の途中である — 根拠: `.claude/rules/safety-nets.md:24-28`（見出し配下に bullet が 2 個あり、挿入は 1 個目と 2 個目の間）。Step 2 のアンカー引用自体は一意で誤りではないため実行時の迷いは生じないが、「末尾」という要約は不正確。

## 要対処
（なし）

## 未検証（理由）
- Task 2 Step 3〜5 実行後の実測値（新しい rules 合計字数・G10 の赤化・budget 引き上げ後の再緑化）— 理由: 本レビューは計画段階の静的検証が対象であり、計画のコード変更は未実施（`workspace/plan.md` の該当 Step は `- [ ]` のまま）。実行結果はコードレビュー時に確認する必要がある。
