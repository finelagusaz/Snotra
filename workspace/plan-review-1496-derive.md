# #1496 変更対象の独立導出

対象: GitHub issue #1496「chore(skills): start-issue Step 2 を worktree でも動く手順にする」
導出の材料: `gh issue view 1496`、`.claude/skills/`・`.claude/agents/`・`.claude/hooks/`・`.claude/rules/`・`docs/`・`scripts/`・`AGENTS.md`・`CLAUDE.md`。
`workspace/` 配下は読んでいない（計画は目にしていない）。このファイルへの書き込みだけを行った。
git の挙動はスクラッチパッドの使い捨てリポジトリ（bare remote + clone + worktree、git 2.56.1.windows.1）で実測した。このリポジトリの状態は変えていない。

## 実測した事実（以下の判断の根拠）

| # | 操作 | 結果 |
|---|---|---|
| E1 | worktree から `git checkout main`（本体が main を掴んでいる） | `fatal: 'main' is already used by worktree at …`（issue の再現） |
| E2 | worktree で `git fetch origin` → `git switch -c feat/a origin/main` | 通る。**`branch 'feat/a' set up to track 'origin/main'.`**（このリポジトリは `branch.autoSetupMerge` 未設定＝既定 `true`。`git config --get branch.autoSetupMerge` は exit 1） |
| E3 | `git switch -c feat/b --no-track origin/main` | 通る。upstream 未設定 |
| E4 | upstream が `origin/main` のブランチで素の `git push` | `fatal: The upstream branch of your current branch does not match the name of your current branch`（global `push.default` 未設定＝simple） |
| E5 | E2 のブランチで `git push -u origin HEAD` | `origin/feat/a` を追跡し直す（Step 6 の push で upstream は正しく上書きされる） |
| E6 | worktree から `git fetch origin main:main`（ローカル main の更新を試みる） | `fatal: refusing to fetch into branch 'refs/heads/main' checked out at …`。worktree からローカル main は安全に更新できない |
| E7 | 本 worktree の設定 | `push.autoSetupRemote=true`（global）、`push.default` 未設定。現ブランチ `chore/1496-…` は upstream 未設定。`main` と `origin/main` は現時点で同じ sha |

## 1. 変更が要るファイル（issue の要件を満たすため）

### `.claude/skills/start-issue/SKILL.md`
- **見出し `## Step 2 — main 最新化 & ブランチ作成`（30 行）**: 本文が main を更新しなくなるので、見出しの「main 最新化」が偽になる（`.claude/rules/governance-docs.md`「古い情報を残さない」）。例: `## Step 2 — 最新の origin/main からブランチを作る`。
  - 旧見出しを名指す現役の参照は 0 件（`.claude/`・`docs/`（`docs/superpowers/` を除く）・`AGENTS.md`・`CLAUDE.md`・`CONTRIBUTING.md` を grep）。旧名が出るのは `docs/superpowers/plans/2026-07-26-skill-workflow-boundary.md`（383・490・513 行）と `docs/superpowers/specs/2026-07-26-skill-workflow-boundary-design.md`（16 行）だけで、どちらも過去の設計記録。`governance:check` は見出し名を照合しない（`scripts/governance-check.mjs` に heading の処理なし、rule 本文も「検査は照合しない」）。
- **32 行「作業ツリーに未保存の変更が無いことを確かめてから実行する。」**: 要件どおり残す。確認手段を明示するなら `git status --porcelain` が空であること（`/implement`「3a」98 行と同じ言い方）。
- **34〜38 行のコードブロック**: `git checkout main` / `git pull --ff-only` / `git checkout -b …` を、例えば次へ置き換える。
  ```
  git fetch origin main
  git switch -c <feat|fix|chore>/<short-name> --no-track origin/main
  ```
  1 行目は E1 で止まる当の行。2 行目の `git pull --ff-only` は main 上で走る前提なので一緒に消える。`--no-track` の理由は 4 節。
- 13 行目以外の frontmatter（`description` の「ブランチ作成」、`allowed-tools: Bash(git *)`）は変更不要。`git fetch` / `git switch` も `Bash(git *)` に含まれる。

## 2. 波及先

### 2a. ローカル `main` の更新を前提にしている箇所（⚠️ 範囲に入れるかは判断が要る）
旧 Step 2 は、本体のチェックアウトで毎サイクル `main` を `origin/main` へ追従させていた。新手順ではローカル `main` が更新されない（worktree からは更新できない・E6）。`main` ref は全 worktree で共有なので、本体で誰かが pull するまで古いまま残る。
古い `main`（M0）と、`origin/main`（M1）から切ったブランチでは、`merge-base(main, HEAD)` = M0 になり、**M0..M1 の他人のコミットが差分に混ざる（取りこぼしではなく過剰に拾う）**。`origin/main...` を使えば、fetch で `origin/main` がさらに進んでも merge-base は分岐点のまま正しい。

| 箇所 | 現状 | 影響 |
|---|---|---|
| `.claude/agents/code-reviewer.md` 11 行（手順 1） | `git diff main...<sha>` | レビュー対象の差分に他の PR が混ざりうる。`origin/main...<sha>` へ寄せる候補 |
| `.claude/skills/implement/SKILL.md` 116 行（3b の rule 列挙） | `git diff --name-only main...<sha>` | 読ませる rule が過剰になる（害は小さい：「迷ったら読む」側に倒れる） |
| `.claude/skills/implement/SKILL.md` 140 行（軸の違うレビュー） | 既に「`git fetch` のあと `origin/main...HEAD`（ローカルの `main` は古いことがあり…）」 | 変更不要。むしろ上 2 件を揃える根拠になる正本の言い回し |
| `.claude/skills/retrospective/SKILL.md` 23〜24 行 | `git log main..HEAD` / `git diff main..HEAD --stat` | user 起動専用スキル。同じ過剰包含。直すなら `origin/main..HEAD` |
| `docs/build-commands.md` 239 行（`race:boundaries -- --base <分岐元ブランチ>`）と `scripts/race-boundaries.mjs` | `--base` に何を渡すかは呼ぶ側任せ | `main` を渡すと過剰包含（exit 3 の「基準が誤っている」は空のときだけ発火するので検出されない）。プレースホルダを `origin/main` の例にするかは任意 ⚠️ |
| `AGENTS.md` 81 行・`docs/check-skill-skeleton-design.md` 31 行 | `git diff main...HEAD` を例示 | 2 点形の罠・リビジョン明示の例であり手順ではない。変更不要と判断 ⚠️（規範文書は変えると合意が要る） |

この過剰包含は、worktree で作業していた従来から既にありえた（本体が pull しないかぎり共有の `main` は古い）。新手順が新たに生むのは「本体のチェックアウトで作業しても `main` が古いまま」の場合だけ。

### 2b. upstream の設定に依存している箇所
- **`.claude/skills/start-issue/SKILL.md` Step 6（195〜199 行）`git push -u origin HEAD`**: E5 のとおり、`--no-track` の有無にかかわらず通り、upstream を `origin/<branch>` に設定する。変更不要。
- **`.claude/hooks/pre-bash.mjs` `decide`（377〜378 行）と `readGitState`（383〜402 行）、冒頭コメント 12 行「動的 — upstream が設定済みで、未 push コミットが無い」、`REMEDY`（96 行）**:
  - `--no-track` あり: push 前の upstream は未設定 → 単独の `gh pr create` は「upstream が未設定」で block され、REMEDY（`git push -u origin HEAD`）がそのまま適用できる。従来（ローカル `main` から `checkout -b` ＝ upstream 無し）と同じ状態。
  - `--no-track` なし: push 前の upstream は `origin/main`。`@{u}..HEAD` は「origin/main より先のコミット」を数えるので、コミットがあれば block（正しい）、コミット 0 件なら allow（gh 側が「コミットが無い」で失敗する）。hook の判定は壊れないが、「upstream が設定済み」の意味（＝自分のブランチが remote に在る）が崩れる。
  - hook 本体は変更不要。`judgeCommandShape` の 5 判定（heredoc・`\` パス・python・`--no-verify`・`--ff-only` の無い `git pull`）は `git fetch` / `git switch -c` に発火しない。新手順から `git pull` が消えるので、`pull` 判定に当たる経路が 1 つ減るだけ。
- **`.claude/skills/implement/SKILL.md` 168 行** `git push -u origin HEAD && gh pr create …`: `hasSafeChain` で allow。upstream の状態に依存しない。変更不要。
- **`.claude/skills/implement/SKILL.md` 140 行**「引数を省くと、レビューの対象が作業ツリーと upstream の状態で変わる」: `/code-review` は作業ブランチ名を明示して呼ぶので影響なし。ただし `--no-track` なしだと、Step 6 の push までは upstream が `origin/main` になる ⚠️。
- **素の `git push`**（スキルには無いが手で打たれうる）: `--no-track` なしだと E4 で失敗する。`--no-track` ありなら global の `push.autoSetupRemote=true` が効く環境では通るが、これはリポジトリが保証する設定ではない（E7）。

### 2c. 同じパターン（main を checkout してから切る）を持つ手順
- **`.claude/skills/deps-update/SKILL.md` Step 1（25〜26 行）**「`git fetch` して `main` を最新にする」「`main` から `chore/deps-YYYYMMDD` ブランチを作る」: 同型。`git fetch` はローカル `main` を更新しないので文言が既に曖昧で、文字どおり `git checkout main` を打てば worktree で E1 になる。`AGENTS.md`「開発ワークフロー」の「同じパターンを持つコードパスをすべて探す」に当たる。直すなら Step 2 と同じ 2 行（`chore/deps-YYYYMMDD` を `origin/main` から `--no-track` で切る）。user 起動専用スキル。⚠️ 範囲に入れるか、issue の外として PR 本文に残すかは判断が要る
- **`.claude/skills/implement/SKILL.md` 99 行**「`main` 上にいる場合は、feature ブランチ…を作成する」: コマンドを書いていない。worktree では `main` 上にいられない（本体が掴んでいる）ので、この分岐に入るのは本体のチェックアウトだけ。そこでは「今の main から切る」で害は無い（未コミットの作業を持ったまま切るのが目的）。変更不要と判断。`/start-issue`「Step 2」を参照させる案はあるが、目的が違う（作業を持ち運ぶ）ので `origin/main` から切ると作業と起点がずれる ⚠️

## 3. 変えなくてよいと判断したファイル

- `.githooks/*`（pre-commit・pre-merge-commit・pre-push・pre-rebase）: 新手順は main 上でコミットもマージもせず、push 先は feature ブランチ名。post-checkout 系の hook は無い。
- `.claude/hooks/pre-bash.mjs` / `pre-bash.test.mjs`: 2b のとおり。テストの `git switch main && git pull --ff-only`（821・1003 行など）は判定の fixture であり、Step 2 の手順の写しではない。
- `scripts/verify-tree.mjs`・`scripts/merge-pr.mjs`・`scripts/clean-worktrees.mjs`・`scripts/governance-check.mjs`: ローカル `main` も upstream も参照しない（`diff` / `merge-base` / `"main"` / `origin/` を grep して 0 件。`verify-tree.mjs` の `mainRoot` は本体のルートを指す関数で、ブランチの `main` ではない）。`merge-pr.mjs` は `refs/pull/<n>/head` を fetch するだけ。
- `docs/superpowers/**`: 過去の計画・設計の記録。旧手順（`git checkout main && git pull …`、旧見出し名）を書いているが、現在形の手順として読者を送るものではない。
- `AGENTS.md`・ルート `CLAUDE.md`: Step 2 の手順を写していない。`CLAUDE.md`「Git/GitHub 運用」の「main を取り込むときは merge」は取り込みの話で、ブランチの作成には触れない。触らないので、規範文書のマージ合意も発生しない（ただし `.claude/skills/**` はセーフティネットの範囲なので、PR の冒頭に「マージは合意してから」が要る。ルート `CLAUDE.md` 最重要ルール 2）。
- `docs/build-commands.md`: ブランチ作成の手順を持たない（2a の `--base` の例だけが任意の候補）。

## 4. 設計上の判断が要る点

1. **`--no-track` の要否 → 付ける方を推す。** 付けないと E2 のとおり upstream が `origin/main` になる。害は (a) 素の `git push` が E4 で失敗、(b) Step 6 までの間 `pre-bash.mjs` の「upstream が設定済み」が「remote に自分のブランチが在る」を意味しなくなる、(c) `git status` が「ahead of origin/main」を出す。Step 6 の `push -u` で上書きされるので致命的ではない。付ければ従来（ローカル main から `checkout -b`）と同じ「push まで upstream 無し」の状態が保たれ、hook の REMEDY とも合う。代わりに `git config branch.autoSetupMerge false` をリポジトリに置く案は、全員の設定を変える永続設定なので不採用寄り ⚠️
2. **ローカル `main` を更新するか → しない。** worktree からは `git fetch origin main:main` が E6 で拒まれ、`git update-ref` は本体の index と作業ツリーを置き去りにする（`.githooks/` も走らない・ルート `CLAUDE.md`「Git/GitHub 運用」）。「本体と worktree で同じ手順」の要件とも両立しない。代わりに、`main...` を使う下流（2a）を `origin/main...` へ寄せるかを決める ⚠️（issue 本文は求めていない。範囲に入れないなら PR 本文に残す）
3. **`git fetch` が失敗したとき → 止まって報告する。** 失敗を無視して古い `origin/main` から切ると、issue の受け入れ条件「最新の `origin/main` を起点」を黙って満たさなくなる。コードブロックの 2 行を `&&` で繋ぐか、文で「fetch が失敗したら先へ進まない」と書く。⚠️ `&&` で繋ぐ複合形は、隔離 worktree の委譲先では拒まれた実績がある（ルート `CLAUDE.md`、#1324）。Step 2 を打つのは主エージェントなので影響は無いはずだが、文で書くほうが無難
4. **`git fetch origin` か `git fetch origin main` か**: 後者で `origin/main` も更新される（既定の refspec があれば opportunistic update が効く）。どちらでも要件は満たす。軽いのは後者 ⚠️（この worktree の remote 設定の refspec は確かめていない）
5. **clean の確認の形**: 散文のままか `git status --porcelain` が空、と書くか。⚠️ `workspace/` は `.gitignore` に無い（`git status` で `?? workspace/`）。`/start-issue` の手前で `workspace/` に何かを書くと、`--porcelain` の確認は落ちる。`git switch -c` 自体は未追跡ファイルを持ち運んで通るので、確認の目的（前サイクルの未保存の変更を新ブランチへ持ち込まない）と照らして、`workspace/` の扱いを書くかどうかを決める
6. **受け入れ条件「本体と worktree の両方で通る」の測り方**: 本体のチェックアウトは並行セッションが使っているので、そこで `git switch` を打つと他者の作業を壊しうる。スクラッチの使い捨てリポジトリ（bare remote + clone + `git worktree add`）で両方を再現して測るのが安全（本導出の E1〜E6 と同じ組み方）。harness の worktree での実地確認は、次に `/start-issue` を回したときに観測できる
7. **見出しの改名**: 1 節のとおり現役の参照は 0 件で、改名は安全。改名しないなら「main 最新化」が偽のまま残る
