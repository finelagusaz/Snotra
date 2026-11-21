# 調査 — issue #1495 implement の軸の違うレビューを出力の母集団に入れ、対象を固定する

## issue の要約

#1492 で `/implement`「3c. 報告を受けてから」に足した「軸の違うレビュー」（条件が立つとき `/code-review high` を 1 回通す）について、2 点を直す。

1. 「出力」節のレビューの母集団（「3a の check スキル全件と 3b の委譲」）と表に、軸の違うレビューの行を足す。条件が立たなかったときは「該当なし」と根拠を書く
2. `/code-review high` へ渡す対象を名指す。渡し方は、スキルの受け付ける形を試して決める

## 関連するファイル・シンボル

- `.claude/skills/implement/SKILL.md`
  - 「3c. 報告を受けてから」の最後の箇条: 「委譲のレビューが clean になっても、次のどれかが立つなら、Step を出る前に軸の違うレビューを 1 回通す: (1)…(2)…(3)…。主が Skill ツールで `/code-review high` を呼ぶ（上の「1体だけ」の例外…）」。対象の指定は無い
  - 「出力」節の冒頭: 「レビューの母集団は、3a の check スキル全件と 3b の委譲である」
  - 「出力」節の雛形: `### レビュー（3a の check スキル全件 + 3b の委譲）` の表
  - 「出力」節の箇条: 「レビューの母集団は、3a の列挙と一致する。…」「委譲の行には、アンカーにした sha と変異注入の結果を書く…」
- `code-review` スキル（harness 同梱。リポジトリの外）の description: 「Review the current diff, or a PR number/branch/path target …」。引数の先頭が effort、その後ろが対象

## 対象の渡し方の実測（このセッション・2026-11-21）

- #1493 で `/code-review high chore/1493-seam-contracts`、#1494 で `/code-review high chore/1494-delegate-rules` を Skill ツールで呼んだ。どちらも展開された指示の冒頭が「Review target: `<ブランチ名>`」になり、「If a PR number, branch name, or file path was passed as an argument, review that target instead」に従って、ブランチの差分（`main...<ブランチ>`）をレビューした
- Step 3 を出る時点の作業ツリーは clean で、引数なしだと「現在の diff」は空になりうる（`git diff @{upstream}...HEAD` は upstream の有無で変わる）

## 敵対的調査（3b）の反映

出典: `workspace/adversarial-1495.txt`

- 採る（命題 1 は部分的に偽）: 展開された指示は「Review target: `<ブランチ名>`」と「branch name … review that target instead」だけで、差分の取り方は書いていない。差分の git コマンドは、`code-review` を実行する主エージェントが自分で選んでいた（#1493 は `git diff 182714bc...HEAD`、#1494 は `git diff main...HEAD`）。渡したブランチは、どちらもチェックアウト中で HEAD と同じだった
- 採る（追加所見）: ローカルの `main` は古いことがある。2026-11-21 時点で `main` は `6903204e`、`origin/main` は `9c125eaa` で、独自のコミットが無いブランチでも `git diff main...HEAD` が他の PR の 2 ファイルを出した。→ 差分は `origin/main` を起点に取らせる
- 採る（命題 2 の後半は偽寄り）: Step 3 は Step 5 の push より前なので upstream が無く、`code-review` の Phase 0 は `main...HEAD` に倒れ、空にはならない。空になるのは push 済みで upstream が HEAD と同じときだけ。対象を名指す理由は「空になる」より「起点が古い main になる・作業ツリーの状態で変わる」である
- 採る（Q2）: 条件が立たなかった根拠は、3 条件それぞれの判定を 1 行で書く（一言では「該当なし」と「確かめていない」を区別できない）
- 壊せなかった（維持）: 出力節の母集団に軸の違うレビューの行が無い／軸の違うレビューは check スキルではないので別に扱う（ただし「3a の列挙と一致する」の箇条は狭める必要がある。計画 D3）
- 測定環境: `code-review` は harness 同梱で、版で文面が変わりうる。→ SKILL.md には「対象を名指す」「差分の起点を `origin/main` にする」だけを書き、harness の内部手順は写さない

## 再利用できる既存のパターン

- 委譲の行の書き方（「アンカーにした sha と変異注入の結果を書く」）。軸の違うレビューの行も、対象（ブランチ）・件数・直した件数を書く形にそろえられる
- 「1 検査 1 行」「該当なし / 未実施」の値の語彙

## 技術的な制約

- `.claude/skills/**` はセーフティネット。マージは合意してから
- 母集団の記述を「3a の列挙と一致する」とする既存の箇条がある。軸の違うレビューは 3a の check スキルではないので、その箇条の対象（check スキルの列挙の同期）とは別に扱う
- `code-review` スキルの引数の形はリポジトリの外（harness）が持つ。版で変わりうる

## 未解決の疑問

- Q1 対象はブランチ名か、`main...HEAD` のような範囲か、PR 番号か（Step 5 の前なので PR はまだ無い）
- Q2 条件が立たなかったときの根拠に何を書くか（3 条件それぞれの判定か、一言か）
