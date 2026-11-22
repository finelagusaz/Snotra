# 調査 — issue #1528 マージ後に本体のローカル main を進める

## issue の要約

ユーザーが打つスラッシュコマンド・Skill ツール・hook は、作業中の worktree ではなく、本体のチェックアウト（`C:/workspace/Snotra`）の版を読む。本体はローカルの `main` に居て、`git fetch` だけでは進まない。スキルや hook を変える PR をマージしても、本体の `main` を進めるまで旧版が動く（2026-11-22、#1522 のマージ直後の `/retrospective` で観測。#1500 でも Skill ツールで観測）。

## 前提とする裁定（会話で決定・2026-11-22）

- 直し方: `merge:pr` が `main` を進める（`AskUserQuestion` の回答「merge:pr が main を進める (Recommended)」）。exit 0 の後処理で、本体が `main` に居て作業ツリーがきれいなときだけ fast-forward する。そうでなければ、理由と手で打つコマンドを出す。終了コードは変えない

## 関連するファイル・シンボル

- `scripts/merge-pr.mjs`
  - `run`: exit 0 の直前に、`deps.cleanup(root, pr.headRefName)` でツールの worktree と要約を片づける（#1359）。失敗しても終了コードを変えない。ここが今回の追加の置き場
  - `realDeps`: `exec`・`mainRoot`・`cleanup` などの差し替え口。テストは `fake()` で差し替える
  - `root = deps.mainRoot(cwd)`: 本体の作業ツリー
  - 冒頭のコメント「構造で消している落とし穴」に、exit 0 の後の片づけの記述がある
- `scripts/verify-tree.mjs`
  - `mainRoot(cwd)`: `git rev-parse --git-common-dir` の親を本体とする
  - `discardBranchArtifacts(root, branch)`: `{ ok, notes }` を返す。新しい関数もこの形にそろえる
- `scripts/merge-pr.test.mjs`: `fake()` の deps。`run — マージ済みのブランチのツールの worktree と要約を片づける（#1359）` の describe が手本（exit 0 でだけ呼ぶ・失敗しても 0・事後検証の後に呼ぶ）
- `scripts/verify-tree.test.mjs`: `discardBranchArtifacts` を一時リポジトリで確かめる describe がある（実 git の手本）
- `.claude/skills/merge-pr/SKILL.md`「3. 終了コードで次の一手が決まる」の exit 0 の行: 片づけと、片づけられなかったときの次の一手を書く
- `.claude/hooks/pre-bash.mjs` 冒頭（50〜52 行）: hook は本体の版で走り、修正が効くのは本体がその変更を持ってから、と書く。真のまま
- `.claude/skills/deps-update/SKILL.md`: 本体で `git fetch` のあと `git merge --ff-only origin/main` で `main` を最新にする手順を持つ（同じ git 操作の先例）

## 再利用できる既存のパターン

- 後処理の結果は `{ ok, notes }` で返し、`run` は `ok` で `deps.out` / `deps.err` を使い分ける。例外も catch して終了コードを変えない
- git の呼び出しは `shell: false`（`deps.exec`）
- 本体を最新にする git 操作: `git fetch origin` → `git merge --ff-only origin/main`（`/deps-update`）

## 技術的な制約

- refs は worktree 間で共有される。どの worktree から `git fetch origin` しても、本体の `origin/main` も進む
- `git merge --ff-only` は、追跡ファイルの変更が衝突するとき、未追跡ファイルを上書きするときに拒む。拒まれたら何も変わらない
- 本体が `main` 以外のブランチや detached HEAD に居るときは、触らない（ユーザーが本体で作業している）
- `merge:pr` は harness の worktree からも本体からも走る。本体から feature ブランチで走らせた場合は、本体は `main` に居ないので触らない
- 終了コードの意味（0 = マージ済みで事後検証が緑）を変えない。exit 3 では、片づけと同じく何もしない（人が調べる材料を残す）
- `scripts/**` はセーフティネット。マージはユーザーの合意の後（ルート `CLAUDE.md` 最重要ルール 2）

## 未解決の疑問

1. 追跡ファイルに変更があるときは、merge を試さずに止めるか、`--ff-only` に任せるか。止めるほうが説明が明確で、作業中の本体に触らない
2. ローカルの `main` が `origin/main` より先にある、または分岐しているとき: `--ff-only` が拒む。理由を出して手で解かせる
3. 既に最新のとき: 「既に最新」と出す
4. fetch に失敗したとき（ネットワーク）: 理由を出し、終了コードは変えない

## 敵対的調査（`workspace/adversarial-1528.txt`）の採否

- 測定環境: 本体は今 `main`・きれい・`29af7e73`（ユーザーが pull 済み）。実装後の実地の確かめでは、一時リポジトリで本体を意図して古くしないと no-op で緑になる → 計画のテストは一時リポジトリで状態を作る形にした
- 採る F1: `--ff-only` は detached HEAD や `origin/main` の祖先である別ブランチでも成功し、そのブランチを動かす（一時リポジトリで実測）。ブランチが `main` であることを事前に明示判定する（計画どおり。理由を計画に書いた）
- 採る F2: 追跡ファイルの変更が ff の差分と重ならなければ `--ff-only` は成功する。dirty で止めるのは ff の失敗を避けるためではなく、作業中の本体に触らないための設計判断として書く
- 採る F3: 無視された（ignored）ファイルが、マージで新たに追跡されるパスと重なると、`--ff-only` は警告なしで上書きする（実測）。事前に `git diff --name-only --diff-filter=A HEAD origin/main` の各パスが本体に既にあるかを見て、あれば止める
- 採る F4（⚠️ の判断）: exit 3 でも「マージ済み。ただし要確認」の経路ではマージは済んでいて、旧版が動く害は残る。本体には書き込まず、手で打つコマンドの案内だけを出す。成否不明の経路では何もしない
- 採る F5: 既存の fetch は `refs/pull/N/head` だけで、`origin/main` を進めない。後処理で `git fetch origin main` する。fetch 後の `origin/main` がマージ commit を含むか（`git merge-base --is-ancestor <mergeCommit> origin/main`）を確かめてから ff する。含まなければ「まだ現れていない」と止める（「既に最新」の誤報を防ぐ）。マージ commit は `gh pr view --json state,mergedAt,mergeCommit` で取る
- 記録だけ F6 ⚠️: `.claude/settings.json`（hook の登録・permissions）を変える PR は、起動中のセッションに効かない可能性がある（未測定）。本 issue の範囲外
- 壊せなかった: 本体以外に古い版が残る経路は見つからない（`~/.claude/skills`・plugins・グローバルの hooks）／worktree からの fetch で本体の `origin/main` も進む／分岐・重なる変更・衝突する未追跡ファイルでは `--ff-only` が拒む／`mainRoot` は本体・worktree・`scripts/` のどこからでも本体を返す／dry-run は後処理の手前で return する／ff では `.githooks` の hook は走らない
