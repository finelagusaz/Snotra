# 調査: #1299 PR 前ゲートで「最後に委譲検証した sha」と HEAD のコード差分を照合する

## issue の要約と、ユーザーの裁定（2026-10-25）

`/implement` 3c は「Step を出るのは、いま HEAD にある差分がトリガーする検査をすべて通ったとき」と定めるが、照合は散文にしか無い。最終巡のあとにコード変更が載ったまま PR になった例が、少なくとも 9 件ある（issue 本文。#1302 自身も、最終巡のアンカー `76dafe04` のあとに `cd2b91a6`（ADR）と `eef7a7aa`（コード）が載った）。

issue の案は 3 項目。下の裁定で、何を作るかが定まった。

| 問い | 裁定 |
|---|---|
| 何を「検証した」と数えるか | **道具（`npm run verify:tree`）の A〜F だけ。** 要約の `sha` を照合する。委譲先のレビューが見た sha は照合しない |
| 「手直しが小さいので再検証しない」を例外として認めるか | **認めない。** `workspace/` 以外の差分があれば必ず止める |
| ブランチに道具の要約が無いとき | **止める。** 復帰手順は `npm run verify:tree -- run --sha HEAD`。`/deps-update` にも道具を回す 1 行を足す |

## 関連ファイル・シンボル（実在を grep で確認済み）

| 対象 | 所在 | 関わり |
|---|---|---|
| `decide(payload, readGitState, readPlanState, platform)` | `.claude/hooks/pre-bash.mjs:308` | 判定の SSOT。`gh pr create` を検出すると、cwd 変更 → plan.md → 安全な push の鎖（`hasSafeChain` なら git を読まずに allow）→ upstream / 未 push の順に見る。新しい判定は、`hasSafeChain` の allow より**前**に置く必要がある（鎖が安全でも照合を素通りさせない） |
| `readPlanState(cwd)`・`findUp` | `pre-bash.mjs:393`・`:377` | reader の手本。「管轄外」と「判定不能」を区別し、判定不能は block |
| `readGitState(cwd)` | `pre-bash.mjs:349` | `spawnSync("git", …, { timeout: GIT_TIMEOUT_MS })` の手本（`GIT_TIMEOUT_MS = 10_000`・`:42`） |
| hook の import | `pre-bash.mjs:32-35` | Node 標準だけ。`.claude/hooks/` から `scripts/` を import している hook は無い（`git grep` で 0 件） |
| hook の実行頻度 | `docs/hooks.md`「PreToolUse（pre-bash.mjs）の実装契約」 | **全 Bash / PowerShell コマンドで起動する**。重い import を頂上に置くと全コマンドが遅くなる |
| fail-closed の契約 | 同上 | exit 2 だけがブロック。未捕捉の例外は exit 1 で素通りする。既定を 2 に置き、allow が確定した経路だけが 0 を書く |
| テスト | `.claude/hooks/pre-bash.test.mjs`（`decide(` の呼び出し 29 か所） | 分岐表の単体テストは reader を注入する（`CLEAN`・`NO_PLAN` など）。プロセス起動の e2e（`:364-`）には、`gh pr create` が exit 0 になる前提のケースがある（「安全な鎖は git を読まずに exit 0」「全項目チェック済みの plan.md なら安全な鎖は exit 0」）。要約なしで止める裁定の下では、これらの期待は変わる |
| 要約の書き手 | `scripts/verify-tree.mjs` `cmdRun` | 既定の置き場は `<root>/.claude/worktrees/agent-verify-<slug>.summary.json`。slug はブランチ名の `[^A-Za-z0-9._-]` を `-` に置き換えたもの（`mainContext`）。要約は `sha`・`branch`・`status`・`results` などを持つ |
| 要約を消す側 | `verify-tree.mjs` `cmdRemove` | **木を消すときに、既定の要約とログも消す**（#1298 で入れた）。`/implement` Step 4 は PR を作る前に `remove` を打つので、**今のままでは PR を作る時点で要約が無い** |
| `/implement` Step 4 | `.claude/skills/implement/SKILL.md` | 木の片づけ → ADR の回収 → `workspace/` の撤去、の順。撤去のコミットは道具の最終実行より後に載るが、`workspace/` だけの差分なので照合を通る |
| `/deps-update` Step 4 | `.claude/skills/deps-update/SKILL.md:79-83` | `gh pr create` を打つ。道具は回していない |
| `/retrospective` | `.claude/skills/retrospective/SKILL.md` | PR を作るかどうかは書いていない（Step 4 は issue と PR 本文への振り分け） |
| 散文の索引 | ルート `CLAUDE.md:23` | 「`gh pr create` は未 push と `workspace/plan.md` の未チェック `- [ ]` で拒まれる」——新しい拒否理由が増えるので偽になる |
| hook の契約文書 | `docs/hooks.md`「PreToolUse（pre-bash.mjs）の実装契約」の「plan.md ゲート」の箇条 | 新しいゲートの箇条を足す |

## 再利用できる既存パターン

- reader 注入と `{ ok, … }` の戻り値（`readPlanState`・`readGitState`）。分岐表を git なしで単体テストできる
- 要約のパスの導き方は `verify-tree.mjs` の `mainContext` が持つ。hook がもう 1 つ導出を持つと写しになる。共有するなら、verify-tree から純関数を export し、hook が `gh pr create` を検出した後でだけ動的に import する（全コマンドの起動を重くしないため）
- `git diff --name-only <sha> HEAD` と、パスの接頭辞での除外

## 技術的制約

1. **要約は PR を作る時点まで残っていなければならない。** 今の `remove` は要約を消すので、要約（`.summary.json`）は残し、ログ（`.summary.log`）だけを消す形へ変える必要がある。要約は gitignore 済みの小さな JSON
2. **古い要約が残る。** 同じ名前のブランチを後で切ると、前の要約が見える。照合は「`sha` から HEAD までに `workspace/` 以外の差分が無い」なので、古い sha が今の履歴に無いか、差分を持てば止まる。`git diff` が sha を解決できなければ判定不能として止める
3. **slug の衝突。** 別のブランチが同じ slug になりうる（`feat/a-b` と `feat-a/b`）。要約の `branch` が今のブランチと一致することも照合する
4. **`hasSafeChain` より前に置く。** 安全な鎖は「push が先に走る」ことしか保証せず、検証とは無関係である
5. **全コマンドで起動する hook である。** 新しい処理は `gh pr create` の検出後だけで走らせる。import も同じ
6. **要約が無いときに止める裁定は、既存の e2e の期待を変える**（制約ではなく、計画に含める変更）
7. **照合は道具の A〜F に限る**（裁定）。委譲のレビューを飛ばした手直しは、このゲートでは止まらない。これは受容する残余として ADR に残す

## 未解決の疑問

- Q1: 要約の `status` が `red` のときも止めるか（issue は sha の照合だけを言う。赤のまま PR を作りたい場面はあるか）
- Q2: `git diff` は「`sha` → HEAD」の 2 点比較でよいか。merge commit を挟む履歴で、間の変更が漏れる形はあるか
- Q3: 道具の木の外へ `--out` で要約を出した場合、ゲートは既定の置き場しか見ない。`--out` を使う経路は今あるか
- Q4: PR を作ったあとに足したコミット（#1302 の `eef7a7aa` の形）は、このゲートの射程外である。`/merge-pr` 側に同じ照合を置くかは、この issue の範囲か
- Q5: 要約が残り続けることの掃除（`clean:worktrees` が、存在しないブランチの要約を消すか）

## 敵対的調査（`workspace/adversarial-1299.txt`）の採否

| 所見 | 判定 | 採否と理由 |
|---|---|---|
| 命題 2: `gh.exe pr create`・`& '…/gh.exe' pr create`・`gh api …/pulls` は検出されない | 壊せた | **所見は採る。直し方は採らない。** 検出の正規表現（`pre-bash.mjs:64-66`）が `gh\s+` を要求することを読んで確かめた。ただ、検出を広げるとこの issue より広い面（既存の push ゲートと plan.md ゲートすべて）を動かすことになる。代わりに `docs/hooks.md` の「見ないコマンド形」の一覧へ、この 3 形を書き足す（今の一覧は実装より強い全称になっている） |
| 命題 3: 既定の `git diff --name-only` は rename の移動元を出さない | 壊せた | **採る。** 使い捨て repo で実測した。`src/x.rs` を `workspace/x.rs` へ `git mv` すると、既定は `workspace/x.rs` だけを出し、`--no-renames` は両方を出した。`--no-renames` を必須にし、テストで固定する |
| 命題 5: reader の省略時を allow にすると、`main()` の配線忘れが単体テストでは緑のまま通る | 壊せた | **採る。** reader は必須の引数にし、関数でなければ block にする。本物の reader を通す e2e（本物の git repo と本物の要約ファイル）を最低 1 本置く |
| F1: Step 4 の ADR の回収コミットは `workspace/` の外にあり、`remove` の後に載る | 壊せた | **採る。** 例外は設けない（裁定）。`/implement` Step 4 の順序を変える: ADR の回収 → 道具を回し直す（最後の要約から `workspace/` の外が動いたときだけ。E と F なら数十秒）→ 木の片づけ → `workspace/` の撤去。#1302 の `cd2b91a6` はこの形だった |
| F2: `remove` が要約を残すと、`worktree` と `log` が消えたパスを指したままになる。`verify-tree.test.mjs` は「要約が消える」を断言している | 壊せた | **採る。** ゲートが読むのは `sha`・`branch`・`status` だけなので、残ったフィールドは害にならない。ただし `remove` は要約に `removed_at` を書き足し、木が消えたことを要約自身に記録する。テストの断言は「ログは消え、要約は残る」へ改める |
| ⚠️ 静的 import だと、構文エラー 1 つで全コマンドが exit 2 になる | ⚠️ | **採る（動的 import の理由を差し替える）。** 起動時間の差は約 3ms で、理由にならない（実測は委譲先）。爆発半径を `gh pr create` の検出後に閉じ込めることが理由である |
| ⚠️ `mainContext()` は `process.cwd()` を使い、linked worktree を拒む | ⚠️ | **採る。** 要約のパスを導く純関数 `summaryPathFor(root, branch)` を verify-tree から切り出し、`mainContext` と hook の両方がそれを使う |
| ⚠️ rebase や main の取り込みの後は sha が祖先でなくなり、止まる | ⚠️ | **受け入れる。** fail-closed の向きで、復帰は `run` のやり直しである |
| ⚠️ grep の引数に `gh pr create` を含むだけで検出され、「要約が無い」で止まる | ⚠️ | **受け入れる。** 過剰検出は既存の方針（「過剰検出は許容、過小検出は不可」・`pre-bash.mjs:45`）。拒否文言で「検出が過剰なら引用を外す」とまでは書かない（引用の内側を解釈しない設計は変えない） |
| ⚠️ linked worktree からの PR は、`run` が拒むので復帰できない | ⚠️ | **受け入れる。** PR は本体の作業ツリーから作る運用である。拒否文言に、本体から打つよう書く |
| ⚠️ Q1: `status` が red でも通る。`categories_run` が狭くても通る。要約は自己申告である | ⚠️ | **`status` は照合に入れる。** 裁定は「道具の A〜F を検証と数える」であり、赤い実行は検証を通っていない。`categories_run` の妥当さと、要約の改竄は照合しない（ADR の残余に書く） |
| ⚠️ `run` は本体がクリーンであることを要求する | ⚠️ | **受け入れる。** 復帰手順の文言に「コミットしてから」を含める |
| Q3（`--out` の呼び出しは 0 件）・Q5（`clean-worktrees.mjs` は要約を扱わない） | 壊せなかった | Q3 は射程外として ADR の残余に書く。Q5 は掃除を足さない（要約は 1 KB 前後の JSON で、gitignore 済み） |
| 命題 1（今の `remove` が要約を消す）・命題 4（import の費用）・関連ファイル表・制約 1〜7・Q4 | 壊せなかった | そのまま |
