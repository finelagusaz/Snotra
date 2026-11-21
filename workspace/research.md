# 調査 — issue #1496 start-issue Step 2 を worktree でも動く手順にする

## issue の要約

`/start-issue`「Step 2 — main 最新化 & ブランチ作成」の手順（`git checkout main` → `git pull --ff-only` → `git checkout -b <branch>`）は、harness の worktree（`.claude/worktrees/*`）では 1 行目が止まる。本体が `main` を掴んでいるからである。`main` を checkout せずに最新の `origin/main` からブランチを切る手順にし、本体のチェックアウトでも worktree でも同じ手順で動くようにする。作業ツリーが clean であることの確認は残す。

## 実測（2026-11-21・このセッション）

- worktree で `git checkout main` → `fatal: 'main' is already used by worktree at 'C:/workspace/Snotra'`
- #1493 で `git switch -c chore/1493-seam-contracts origin/main` と打つと、`branch 'chore/1493-seam-contracts' set up to track 'origin/main'.` と出て、upstream が `origin/main` になった（git の `branch.autoSetupMerge` の既定。リモート追跡ブランチを起点にすると追跡を設定する）。`git branch --unset-upstream` で外した
- #1494・#1495・#1496 では `git fetch origin` のあと `git switch --no-track -c <branch> origin/main` と打ち、upstream は設定されなかった（`git rev-parse --abbrev-ref @{upstream}` が `fatal: no upstream configured`）。その後の `git push -u origin HEAD` で、upstream は同名のリモートブランチになった

## 敵対的調査（3b）の反映

出典: `workspace/adversarial-1496.txt`（使い捨てリポジトリ・git 2.56.1 で実測。グローバル設定は `push.autoSetupRemote=true` だけ）

- 壊せなかった（維持）: `git switch -c <b> origin/main` は upstream を `origin/main` にする／`--no-track` なら upstream は付かず、`git push -u origin HEAD` で同名になる／本体（`main` を checkout 中）で打つと作業ツリーが新しいブランチへ移り、worktree では `git checkout main` が止まるが新しい手順は通る
- 採る（書き方の訂正）: upstream が `origin/main` のままでも、`git push -u origin HEAD` が同名へ張り替えるので Step 6 で自己修復する。害が出るのは `-u` なしで push したときで、`pre-bash.mjs` は `@{u}..HEAD` のコミットを「未 push」と見なして `gh pr create` を誤って止める（実測）。害の向きは許可ではなく停止（fail-closed）。→ `--no-track` の理由は「誤検知を避け、upstream を Step 6 で決める」と書く
- 採る（書き方の訂正）: `git fetch origin main:main` は、`main` がどこかで checkout 中なら、本体からも worktree からも拒まれる（rc=128）
- 補足: このマシンは `push.autoSetupRemote=true` なので、`-u` なしの `git push` でも同名が upstream になる（手順は `-u` を明示するので依存しない）
- 範囲外のまま: `/deps-update` の Step 1（散文のみ。worktree での動きは未確認）

## 関連するファイル・シンボル

- `.claude/skills/start-issue/SKILL.md`「Step 2 — main 最新化 & ブランチ作成」のコードブロック（3 行）と、その前の「作業ツリーに未保存の変更が無いことを確かめてから実行する」
- 同「Step 6 — workspace をコミット & プッシュ」: `git push -u origin HEAD`（ここで upstream が決まる）
- `.claude/hooks/pre-bash.mjs`
  - `judgeCommandShape` の `pullWithoutFfOnly`: `--ff-only` の無い `git pull` を拒む。新しい手順は `git pull` を使わないので当たらない
  - `gh pr create` の判定の「動的」経路: 「upstream が設定済みで、未 push コミットが無い」なら、コミットが remote にあると見なす。upstream が誤って `origin/main` を指していると、この判定が実際の push 先と食い違う
- `.claude/skills/deps-update/SKILL.md`「Step 1 — 前提確認」: 「`git fetch` して `main` を最新にする」「`main` から `chore/deps-YYYYMMDD` ブランチを作る」。散文でコマンドは書いていない。worktree での動きは未確認（範囲外の候補）
- `.claude/hooks/pre-bash.test.mjs`: `git checkout main && git pull --ff-only` などを判定の入力として使っている（テストの fixture で、手順の写しではない）

## 再利用できる既存のパターン

- `/implement`「3a. 委譲の前に」の「`main` 上にいる場合は、feature ブランチを作成する」— ブランチ作成の別の入口
- 「`git fetch` のあと `origin/main...HEAD`」（#1495 で `/implement` 3c に入れた、ローカルの `main` を当てにしない書き方）

## 技術的な制約

- `.claude/skills/**` はセーフティネット。マージは合意してから
- 本体のチェックアウト（`main` を掴んでいる）でも同じ手順で動くこと。本体で `git switch --no-track -c <branch> origin/main` を打つと、本体の作業ツリーが新しいブランチへ移る（いまの手順の `git checkout -b` と同じ結果）
- ローカルの `main` は、この手順では更新されない（`origin/main` を直接起点にするため）。ローカルの `main` を読む他の手順（`/implement` 3b の `main...<sha>` など）は、古い `main` のままになりうる

## 未解決の疑問

- Q1 `--no-track` を手順に入れるか（入れないと upstream が `origin/main` になる）
- Q2 ローカルの `main` も更新すべきか（worktree からは `git fetch origin main:main` で更新できるが、本体が `main` を checkout していると拒まれる）
- Q3 `/deps-update` の Step 1 も同じ形に直すか
