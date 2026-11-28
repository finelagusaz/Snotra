# 計画 — issue #1551

## 目的と受け入れ条件

`.claude/hooks/pre-bash.mjs` に、どの機構も守っていなかった 3 つの規則の判定を足す（PR #1550 の「守ること」）。

- A. `git rm` が `workspace/` 配下を消すコマンドで、`workspace/plan.md` に未チェック項目が 1 件以上あれば止める。拒否文言は、未チェックの件数と、完了させるか計画から外して理由を ADR か issue へ記録する、という次の一手を持つ
- B. 未コミットの変更を捨てる次の形を止める。拒否文言は、WIP コミットにすること、ステージだけ外すなら `git restore --staged` / `git reset` を使うことを案内する
  - `git rm` に `-f` / `--force` を付けた形（`-rf`・`-fr` などの束ねも含む）
  - `git checkout` で、`--` の後にパスを渡す形と、`.`（`./…` を含む）を渡す形
  - `git checkout-index`
  - `git restore` で、`--staged`（`-S`）が無い形と、`--worktree`（`-W`）を伴う形
- C. コマンド位置の `gh pr merge` を止める。拒否文言は `/merge-pr`（`npm run merge:pr`）へ案内する
- 止めないもの: `git checkout <branch>`・`git switch <branch>`・`git restore --staged <path>`・`git rm --cached <path>`（`-f` なし）・`git rm -r <workspace 以外>`・`npm run merge:pr`・`gh pr view` など merge 以外の `gh pr`
- 判定は既存の判定と同じく、元の文字列と `unquotedView` の両方で見る
- 理由ごとに、その理由だけに当たる入力のテストを置き、拒否の理由（文言）まで assert する
- PR 前ゲートの (2)（`plan.md` の未チェック）は残す

## 変更するファイル

- `.claude/hooks/pre-bash.mjs` — 判定 A・B・C と拒否文言、冒頭の責務の記述
- `.claude/hooks/pre-bash.test.mjs` — 判定ごとのテスト、全域性・時間の describe への追加、件数を写した describe 名の改名
- `.claude/skills/implement/SKILL.md` — 「守ること」の表の行の移動、Step 4 の `workspace/` 削除の項
- `.claude/skills/merge-pr/SKILL.md` — 「ツールが守らないもの」の `gh pr merge` の記述

ルートの `CLAUDE.md`「`gh pr merge` を手で打たない」は、判定が入っても真のままなので変えない。`scripts/merge-pr.mjs` は `pre-bash.mjs` を静的 import するが、import する関数（`verifiedSummaryState`）を変えないので変えない。

## 実装の順序

1. 判定 B・C（I/O なし）を `judgeCommandShape` に足す
2. 判定 A を `decide` に足す（`readPlanState` を使う）
3. 文書を合わせる

## 設計

- B: `GIT_RM_FORMS`・`GIT_CHECKOUT_FORMS`・`GIT_CHECKOUT_INDEX_FORMS`・`GIT_RESTORE_FORMS` を `GIT_COMMIT_FORMS` と同じ形（`^git\s+${flag}<sub>\b` × `FLAG` / `FLAG_QUOTED`）で置き、`discardsUncommitted(command)` が `gitSegmentsInBothViews` の各セグメントを判定する。`git checkout-index` を `checkout` と取り違えないよう、`checkout\b` の後に `-index` が続かないことを確かめる
- C: `GH_PR_MERGE` を `GH_PR_CREATE` と同じ形（`AT_CMD_POS` + 前置 + `gh\s+FLAG pr\s+FLAG merge\b`）で 2 形持ち、`mergesPrByHand(command)` が元の文字列と `unquotedView` の両方で探す
- A: `removesWorkspace(command)`（I/O なし）が、`git rm` のセグメントに `workspace` で始まるパス語（`workspace`・`workspace/…`・`./workspace…`。`--` の後のパス語も含む。`workspaceX` には当てない）があるかを見る。`--cached` を伴う形は対象外とする（作業ツリーの `plan.md` が残り、PR 前ゲートが読める）。判定 A に当たらなかったときや、当たっても未チェックが 0 件のときは ALLOW を返さず、後ろの判定（`gh pr create` の経路）へ進む（`git rm -r workspace && … && gh pr create` の連結があるため）。拒否文言は、PR 前ゲートの文言（「PR を作成してください」で終わる）とは別に持つ。`decide` は `judgeCommandShape` の後で `removesWorkspace` を見て、真なら `readPlanState` を呼ぶ（reader が渡されていなければ止める）。`exists` が偽なら許可、読めなければ止める
- 並び: B は A より先に判定する（`git rm -rf workspace/` は B の文言で止まる。`-f` を外した形を打ち直せば、A が計画の状態を見る）

- B の `git rm --cached -f` も止める（ステージした内容を失いうるので、fail-closed の向きに倒す）
- C は `unquotedView` でも探すので、区切りを含む引用の内側の言及（`git commit -m "x; gh pr merge"`）でも止まる。#1523 と同じく、過剰検出として受容し、テストで意図として固定する

## 残余（拾わないと決めた形）

- `git checkout <path>`（`--` も `.` も無い形）。ブランチ名と区別できない
- `git rm -r .`・`git rm -r *` のように、`workspace` を含みうるが名指さない形
- `git -C <dir> rm workspace`。`readPlanState` が読むリポジトリと違いうる
- `Remove-Item` などで消してから `git add -A` する形（ユーザーの裁定）
- `git reset --hard`・`git clean -f`・`git checkout -f`・`git switch --discard-changes`（ユーザーの裁定。足りないと分かったときに足す）

## 不変条件と異常系

- 新しい述語はすべて全域関数で、入力長に対して線形に終わる（throw しない・病的入力で時間内に終わる）
- `pre-bash.mjs` のモジュールの先頭に、ロード時に throw しうる処理を置かない（`scripts/merge-pr.mjs` が静的 import する）
- A の経路で reader が欠けていたら、TypeError ではなく理由つきで block する
- A で `plan.md` が読めなければ block（既存の `readPlanState` の `ok: false`）
- 既存の判定の結果を変えない（既存テストがすべて通る）

## テスト方針と検証コマンド

- `pre-bash.test.mjs` に、判定ごとに当たる入力・当たらない入力・止めた理由の文言の assert を置く
- 全域性（no-throw）と病的入力の時間の describe に、新しい述語を加える
- 検証: `npm test`（`pre-bash.test.mjs` を含む）、`npm run governance:check`
- 変異注入は `/implement`「3b. 委譲へ渡すもの」の委譲先が行う

## SPEC.md・関連文書

- `SPEC.md`: 変更なし（製品の挙動ではない）
- 関連文書は「変更するファイル」のとおり

## フェーズ 1 — 形の判定（B・C）

- [x] `discardsUncommitted` と、その正規表現の組を足す
- [x] `mergesPrByHand` と `GH_PR_MERGE` の 2 形を足す
- [x] `judgeCommandShape` に B・C を足し、`SHAPE_REMEDY` に文言を足す
- [x] B・C のテスト（当たる・当たらない・理由の文言）を足す
- [x] 全域性と病的入力の describe（`PREDICATES`・`PATHOLOGICAL`・時間の検査）に B・C を足し、件数を写した describe 名と、件数を写したコメント（「規範 5 件」）を件数なしへ直す
- [x] `PLATFORM_FREE` の表に B・C を足し、win32・darwin・platform 省略のどれでも block することを固定する
- [x] `ALL_REASONS` に新しい文言を足し、「拒否文言は代わりの手段を含む」「`\` パスを含まない」の検査を新しい文言にも当てる。「無害なコマンドは null」に、通すべき形（`git checkout main`・`git checkout -b f origin/main`・`git checkout -`・`git restore --staged a`・`git restore -S a`・`git rm --cached x`・`git rm -r src/x`・`npm run merge:pr -- 1 …`・`npm run merge:pr '--' …`・`grep -n "gh pr merge" x`・`gh pr view 1`）を足す
- [x] 拒否文言が勧める手（`git restore --staged`・`git reset`・WIP コミット・`npm run merge:pr`）が、どの判定にも止められないことをテストで固定する

## フェーズ 2 — workspace の削除（A）

- [x] `removesWorkspace` を足す
- [x] `decide` に A の分岐を足す（reader の確認を含む）
- [x] A のテスト（未チェックあり・なし・plan.md なし・読めない・reader 欠け・`workspace` 以外の `git rm`・`workspaceX`・`--` の後のパス・`--cached`）を足す
- [x] 配線のテストを足す: `git rm` で `workspace` を消すコマンド以外では、`gh pr create` を含まない限り `plan.md` を読まない（`vi.fn` で確かめる）。既存の「gh pr create を検出しなければ plan.md を読まない」は、名前が偽になるので、この新しいテストへ置き換える
- [x] A の process 級 e2e を足す（`payload.cwd` に fixture のリポジトリを渡す）

## フェーズ 3 — 文書

- [x] `pre-bash.mjs` 冒頭の責務の記述（「(1)(2)(4) は `gh pr create` 検出後のみ」を含む）・「判定単位」の段落・「受容する未対応リスク」（`gh pr merge` にも当たる `sh -c`・ラッパ・`gh.exe`・`gh api` の残余を足す）と、`judgeCommandShape`・`decide` の doc（「管轄外」の説明を含む）を、件数を書かずに直す
- [x] `implement/SKILL.md`「守ること」の 3 行を、未確定欄の裁定のとおりに直す
- [x] `implement/SKILL.md` Step 4 の削除の項の末尾と、`git rm` が止まったときの箇条を、hook が止めることに合わせて直す
- [x] `merge-pr/SKILL.md`「ツールが守らないもの」を直す

## 未確定（実装前に潰す）

- [x] （裁定）候補 2 の対象の範囲 — 2026-11-28 の会話で「候補2の対象は今のままで、足りないと分かったときに追加しよう」。`git reset --hard`・`git clean -f`・`git checkout -f`・`git switch --discard-changes` は範囲外
- [x] （裁定）`git rm` の判定の範囲 — 同じ会話で「git rm の判定の変更も現状維持で」。拾わない消し方（`Remove-Item` → `git add -A` など）は残余として受け入れる
- [x] （裁定）`implement/SKILL.md`「守ること」の直し方 — この節の変更はユーザーとの合意が要る。提案: (1)「マージしない」は「散文＋証跡」に残し、手段の欄を「手打ちの `gh pr merge` は hook が止める。`npm run merge:pr` を打つことは止めない」に直す (2)「散文のみ」の `git rm` の行は「機構で止める」へ移し、「散文のみ」の節は「（該当なし）」とする (3)「未確定ゼロ」の行の「`plan.md` を消すと外れる」を「`git rm` で消すときは hook が止める。それ以外の消し方では外れる」に直す — 計画の承認の問いに含めて聞いた。回答「Ok」で提案どおり
- [x] 敵対的調査（3b）の所見を反映する — 採否と理由は `workspace/research.md`「敵対的調査（3b）の採否」。B1〜B3・U1・U3・U5 を採り、U4 は一部、U2 は採らない

## plan-review 結果

- リスク: 高（hook・skills を変更する）
- レビュー方式: 独立導出1体（`workspace/plan-review-1551-derive.md`。計画は目にしていないと申告。`workspace/` を除外した git grep を 1 回、指定外の pathspec で打ったと申告）
- エージェント数: 1

### 要対処

- テストの更新の漏れ（`PLATFORM_FREE`・`ALL_REASONS`・「無害なコマンドは null」・名前が偽になる「gh pr create を検出しなければ plan.md を読まない」・配線のテスト） — 計画の修正（フェーズ 1・2 の作業項目） — `pre-bash.test.mjs:520-525` を読んで確かめた
- `decide` で判定 A の後に ALLOW を返さず後ろへ進むこと、A の文言を PR 前ゲートの文言と分けること — 計画の修正（設計）
- `git rm --cached -r workspace/` は作業ツリーの `plan.md` を残すので A の対象外 — 計画の修正（設計）
- 冒頭の「判定単位」「受容する未対応リスク」の段落 — 計画の修正（フェーズ 3）
- 「守ること」の「マージしない」は、`npm run merge:pr` を打つことを止めないので、行を「機構で止める」へ移せない — 未確定欄へ追加（（裁定）。この節の変更は合意が要る）

### 軽微

- ADR-squash-merge-issue-autoclose（凍結した記録）は、却下したのが「照合」で今回は「止めて `/merge-pr` へ送る」なので矛盾しない。PR 本文で触れる
- `git rm -- -f` などの過剰検出は受容する

### 未検証

- `.githooks/` の拒否文言が新しく止まるコマンドを勧めていないか — 主が `grep -nE "checkout|restore|rm -f|--force|gh pr merge" .githooks/*` で確かめ、該当なし

### 判断

- 実装着手: 可（裁定は提案どおりと回答を得た）

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー1体
- エージェント数: 1
- 要対処: 5 件（4 件は計画へ反映、1 件は未確定欄へ（裁定）として追加）
- 未検証: なし
- check スキル: なし（計画の段階で当たる check スキルが無い。`/dry-check` は関数を定義する実装の段階で、`/implement` 3a が回す）
- 散文の全件列挙: 語 6 / 総件数 176 行 / ★未掲載 5（計画へ足した 0。5 件とも PR 前ゲートの要約や merge:pr 自身の gh 呼び出しを述べた行で、変更で偽にならない） / ☆行未掲載 0

## 承認

- [x] 承認済み — 2026-11-28 / 問い: "「守ること」をこの提案どおりに直し、計画を承認していただけますか。それとも、直したい点をお聞かせいただけますか。" / 回答: "Ok"
