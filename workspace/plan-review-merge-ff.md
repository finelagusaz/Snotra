# plan-review #1528（merge:pr の本体 main fast-forward・計画準拠レビュー）

実測は scratchpad の一時リポジトリ（bare origin + seed + 本体 clone, git for Windows）。ファイル・本体は変更していない。

## 要対処

1. 判定5 のコマンドがリネームを取りこぼす。`git diff --name-only --diff-filter=A HEAD origin/main` は、リネーム（R100）の移動先を A に出さない。実測: seed で `old.txt -> renamed.txt` と `new.txt` 追加をマージしたとき、既定では `new.txt` だけが出て、`--no-renames` を付けると `new.txt` と `renamed.txt` の両方が出る。本体に `renamed.txt` が無視ファイルとしてあれば、`--ff-only` は警告なしで上書きする（`.git/info/exclude` に `new.txt` を入れた実測で、内容 `mine` -> `n` に上書きされ rc=0）。直し: `diff --no-renames --name-only -z --diff-filter=A HEAD origin/main`。`-z` も付ける（quotepath 設定により非 ASCII パスが引用符つきで出て existsSync が空振りするのを避ける。今回の環境では日本語名が素で出たが、設定依存）。テスト (i) にリネーム移動先のケースも足す。
2. 「ローカル main が origin/main より先」を「既に最新」と誤報する。実測: ローカル main に未 push のコミットがある状態で `git merge --ff-only origin/main` は `Already up to date.` rc=0。判定7（前後の HEAD が同じなら「既に最新」）に落ち、受け入れ条件3（先行）の「理由と手で打つコマンド」を満たさない。直し: 判定6 の前に `rev-parse HEAD` と `rev-parse origin/main` を比べる。等しければ「既に最新」、`merge-base --is-ancestor origin/main HEAD` が真で等しくなければ「ローカル main が origin/main より先（push していないコミットがある）」で止める。テスト (b) は等しい場合しか見ていないので、(j) 先行を足す（HEAD 不変と理由まで assert）。
3. 案内する手打ちコマンドが、止めた理由によっては本体を壊す。止めたときの notes は一律に `git -C <root> merge --ff-only origin/main` を添えるが、判定1（別ブランチ・detached）で止めた場合にこれを打つと、実測どおり `--ff-only` は `origin/main` の祖先である別ブランチや detached HEAD を進めてしまう（F1 の害を人が実行して再現する）。判定2（dirty）・判定5（無視ファイル）でも、理由を解消する前に打てば同じ。exit 3 の案内（F4）は本体の状態を見ないまま同じコマンドを出すので、さらに広く当たる。直し: コマンドは理由ごとに書き分ける（判定1 は「本体を main へ切り替えてから」、判定2 は「変更を退避してから」、判定5 は「当該ファイルを退避してから」、判定3・4・6 はそのまま）。exit 3 の案内は「本体が main に居て追跡ファイルに変更が無いことを確かめてから」を前置する。この分岐を計画の「判定の順序」に書いておかないと、実装者が追加判断をすることになる。
4. `fake()` の `keyOf` が、`gh pr view --json state,mergedAt,mergeCommit` への変更で壊れる。`scripts/merge-pr.test.mjs` の keyOf は `args.includes("state,mergedAt")` で "pr view:post" を判定する（merge-pr.test.mjs 49 行付近）。引数の文字列を `state,mergedAt,mergeCommit` にすると完全一致しなくなり、全 post 経路が "pr view:pre" へ誤ルートされて大量に落ちる。計画の変更表に keyOf の修正（`includes("mergedAt")` など）と、既定の "pr view:post" に `mergeCommit: { oid }` を足すことが無い。さらに `run` のテストで「`syncMain` が `(root, mergeCommit.oid)` の引数で呼ばれる」ことを assert する項目も無い（計画の run のテスト列挙は「呼ぶ／呼ばない」のみ）。引数が undefined のままでも既存の「呼んだ」assert は通るので、`mergeCommit` の取得（`--json` フィールド名の綴り・`.oid`）を外す変異で落ちない。
5. 関数シグネチャが計画内で食い違う。変更表は `fastForwardMain(root, exec)`、判定の順序と `realDeps` は `fastForwardMain(root, mergeCommit, exec)`。実装者が決めることになる。あわせて、一時リポジトリのテストは実 git を使うので、`exec` の既定値を実 git（`spawnSync` の `shell:false`）にし、`realDeps.syncMain = fastForwardMain` と直に結べば、引数順の取り違えという配線ミスが 1 か所に減る。いまの計画は `realDeps.syncMain` の配線（引数順・exec の差し込み）をどのテストも縛らない（`run` は fake の syncMain を使い、`fastForwardMain` は直接呼ぶため）。`(root, mergeCommit)` を逆に渡す変異は全テストを通る。直し: 引数順を 1 つに決め、`realDeps.syncMain` を薄い再エクスポートにするか、`run` を実 `realDeps` で呼ぶ 1 本（dry-run ではなく一時リポジトリ）を足す。
6. 文書の取りこぼし: `/merge-pr` の「マージ後に要確認が出たとき（exit 3）」節（SKILL.md 53 行〜）。計画は exit 3 の出力に「本体の main は進めていない。確かめたあとに ... を打つ」を足すが、その次の一手を読む正本はこの節であり、変更表は「3. 終了コードで次の一手が決まる」の exit 0 の行しか直さない。exit 3 の行（表の 4 行目）か当該節に、「本体の main は進めていない」ことと復帰手順の 1 点を足す。足さないと、出力の案内だけが正本に無い手順になる（書く約束の「かぶりなく・正本 1 か所」に反する側ではなく、欠落の側）。

## 軽微

1. 判定1 の `symbolic-ref --quiet --short HEAD` は、detached で出力が空・rc=1（実測）。`git()` ヘルパのように「rc!=0 で throw」する実装を流用すると、detached が例外になり、理由が「本体が main に居ない（detached）」でなくなる。`spawnSync` の status を見て rc=1 を detached として扱うと書いておく。
2. 判定4 の理由が二つに割れる。`mergeCommit` が null（gh が返さなかった）と、`--is-ancestor` が 1（まだ origin/main に現れていない）は原因が違う。さらに `--is-ancestor` は不正な oid で rc=128（実測: `fatal: Not a valid object name`）を返す。前者は「マージ commit を取れなかった」、後者は 1 なら「現れていない」、128 なら「判定できない（git の stderr）」と分ける。PR の base が main でない場合も、この判定で「現れていない」と止まるだけで安全だが、理由の文言は誤解を招く。
3. 判定3 の `fetch` に timeout が無い。`realDeps.exec` は `spawnSync` に timeout を渡さない。認証待ち・ネットワーク断でブロックすると、exit 0 を返す前に `merge:pr` がぶら下がる（マージは済んでいる）。`verify-tree.mjs` の `git()` は `timeout` を取るので、それに倣って付ける。⚠️（既存のマージ前 fetch も同じなので、既存と同水準に留めてよいかは判断）
4. 判定2 は `--untracked-files=no` なので、本体に未追跡ファイルがあって、マージで同名パスが新規追跡になるとき、判定5（無視でない）は止めず、判定6 の git が `untracked working tree files would be overwritten by merge ... Aborting`（rc=1、実測で本体不変・内容 `x` のまま）で拒む。安全。ただし理由は git の stderr 頼みになるので、(k) として未追跡の衝突を 1 本足し、notes に stderr が載ることを assert する。
5. `git update-index --assume-unchanged` したファイルを変更すると、`status --porcelain` は 0 行（実測）。判定2 をすり抜ける。ff が当該ファイルを上書きするかは未測定。⚠️ まれなので計画は「既知の穴」として 1 行書けば足りる。
6. 判定の順序に、本体で merge/cherry-pick が途中のとき（MERGE_HEAD あり、HEAD は main のまま、status が空）の扱いが無い。`merge --ff-only` が拒む想定だが未測定（未検証に入れた）。
7. `.claude/skills/merge-pr/SKILL.md` の frontmatter `allowed-tools` は `Bash(gh *)` と `npm run merge:pr *` だけで、exit 0 の行に新しく書く `git merge --ff-only origin/main` は許可に無い（既存の `git worktree remove` も同様）。人が打つ場面なので偽にはならないが、エージェントが打つ形なら権限確認が出る。
8. 手打ち案内のパス: `git -C <root> ...` の `<root>` に空白があると、そのままでは打てない。引用符で囲む。

## 未検証

- 本体で `git merge --no-commit` 途中・cherry-pick 途中の状態での `--ff-only` の挙動。
- `skip-worktree` / `assume-unchanged` なファイルを `--ff-only` が上書きするか。
- 手打ちコマンド `git -C <root> merge --ff-only origin/main` が `.claude/hooks/pre-bash.mjs`（`pull` 判定は `--ff-only` を許す。FF_ONLY は 307 行）に拒まれないか。hook 本体を走らせていない。
- CI の Windows runner で一時リポジトリのテストが回るか（コミットの user.name/email・`init -b main` の git 版）。`verify-tree.test.mjs` の先例（`discardBranchArtifacts` の describe）に倣えば足りるはずだが、実走はしていない。
- `gh pr view --json mergeCommit` が squash マージ直後に `oid` を返すタイミング（反映遅れで null の可能性。上の軽微2 の「取れなかった」分岐で吸収する前提）。
- 実装に対する変異（`syncMain` の呼び出しを外す・`mergeCommit` を渡さない・判定1 を外す）をテストが落とすか。計画に変異を当てる手順が書かれていない。実装後に、(c)・(d)・(i) は判定1・5 を外す変異で落ちることを確かめるべき。

## 計画が正しかった点（確認済み）

- 判定1: 別ブランチ・detached で止める必要は実在する（detached は symbolic-ref rc=1）。
- 判定5 の方針（無視ファイル上書きの事前検査）は必要。`--ff-only` は無視ファイルを上書きする（実測）。ただし上の要対処1 の修正が要る。
- 分岐（diverge）は `fatal: Not possible to fast-forward, aborting.` rc=128 で、本体不変。
- `origin/main` の fetch は、`remote.origin.fetch` が既定の `+refs/heads/*:refs/remotes/origin/*` のとき `git fetch origin main` で `origin/main` も進む。非既定の refspec の clone では進まない可能性があるが、本リポジトリは既定。
- 本体が bare・存在しない場合は、`mainRoot` が `.git` の basename を要求して EnvError にするため、マージ前に exit 2 で止まる（後処理には来ない）。
- `.githooks/` に `post-merge` は無く（`pre-commit`・`pre-merge-commit`・`pre-push`・`pre-rebase` のみ）、ff で hook は走らない。
- 偽にならない記述: `pre-bash.mjs` 冒頭（50〜52 行）は真のまま。`docs/build-commands.md` 246 行・`merge-pr.mjs` 冒頭・SKILL の exit 0 行は、計画が直す対象に入っている。ADR・`CLAUDE.md`・implement/retrospective の SKILL に、後処理の範囲を述べた散文は無い（grep 済み）。
