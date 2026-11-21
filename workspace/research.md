# research — #1507 verify:tree remove が origin/main の祖先の委譲先ブランチを消せない

## issue の要約

`npm run verify:tree -- remove --delegate-tree <worktree>` は、委譲先の worktree を消したあと、そのブランチを `git branch -d` で消す。ブランチがローカルの `main` より新しい `origin/main` の先端を指していると、`-d` は「not fully merged」で失敗し、ブランチが残る（2026-11-21 に 2 回発生。手で `-D` した）。`-d` は、ブランチに upstream があればそれと、無ければ HEAD と比べて merged かを判定するからである。ツールは本体のルート（`root`）で git を打つので、HEAD は本体が checkout しているブランチになる。本体は `main` で、`/start-issue` Step 2 がローカルの `main` を更新しなくなったので（#1496）、古くなりやすい。

受け入れ条件（issue の本文）:
- ローカルの `main` が古く、委譲先のブランチが `origin/main` の祖先のとき、`remove` がブランチを消す（テストで再現）
- 委譲先のブランチに固有のコミットがあるときは、消さずに報告する（テストで再現）
- `npm test` と `npm run governance:check` が通る

issue のコメント（#1514 の調査から）: 同じ判定を `scripts/clean-worktrees.mjs` も使う。squash マージでは、委譲先がコミットを作ったブランチは祖先にならない。harness が委譲先のブランチに upstream を付けるかは測っていない。

## 関連するファイル・シンボル

- `scripts/verify-tree.mjs` の `cmdRemove`。ブランチの削除は `git(root, ["branch", "-d", branch])`（405 行目付近）。失敗すると「ブランチを消せなかった」を出して exit 1
  - ブランチ名は `entry.branch` か、detach されていれば harness の命名 `worktree-<basename>` から導く
  - ファイル末尾の main ガードにより、import しても `main` は走らない（テストが import している）
- `scripts/clean-worktrees.mjs`: 孤児の `worktree-agent-*` ブランチを `git branch -d`（`--force` なら `-D`）で消す。冒頭のコメントで「成果保全の最終防衛は git 自身の拒否（`worktree remove` / `branch -d`）に委ねる」と宣言している。ブランチ削除の箇所のコメントは、squash マージ後にブランチだけ残るのを標準経路と呼ぶ。このスクリプトは `git` を cwd（呼び出し側）で打つ
- テスト: `scripts/verify-tree.test.mjs` の `describe("verify-tree remove")`（`--delegate-tree` のケースが既にある。`initRepo` はリモートを持たない）、`scripts/clean-worktrees.test.mjs`（`initRepo` はリモートを持たない）

## 再利用できる既存のパターン

- 祖先の判定: `git merge-base --is-ancestor <branch> <ref>`（exit 0 = 祖先、1 = 祖先でない、それ以外 = エラー）
- 規範: `AGENTS.md`「検証の作法」が、main から切ったブランチの差分の起点を `origin/main` と定める（#1514）。ここでも判定の基準を `origin/main` に揃えられる
- テストのリポジトリでリモートを作る: bare リポジトリを `git init --bare` して `remote add origin` し、push してから `fetch` する。あるいは `git update-ref refs/remotes/origin/main <sha>` で追跡ブランチだけ置く（どちらも、`-d` がリモート追跡ブランチを見ないことの再現には足りる）

## 技術的な制約

- `git branch -d` の merged の判定は、upstream があれば upstream、無ければ HEAD を基準にする。`origin/main` は見ない
- 祖先と確かめたあとに `-D` で消すと、確かめてから消すまでの間にブランチが動いた場合を拾えない。ただし、委譲先の worktree は直前に消してあり、そのブランチを checkout している者はいない
- `origin/main` が無い環境（リモートの無い一時リポジトリ、fetch したことのないクローン）では祖先の判定ができない。そのときは今までどおり `-d` に任せる
- `clean-worktrees.mjs` の `--force` は全部を `-D` で消すので、変える対象は `--force` の無い経路だけである

## 未解決の疑問

- 判定の基準を `origin/main` に固定するか、リモートの既定ブランチ（`refs/remotes/origin/HEAD`）から導くか。AGENTS.md の規範は `origin/main` と書いているので、固定でよいと見る
- 共通化: 2 つのスクリプトに同じ判定を置くなら、`verify-tree.mjs` から export して `clean-worktrees.mjs` が import するか、新しいモジュールにするか
- harness が委譲先のブランチに upstream を付けるか（付けていれば、`-d` は upstream を基準にする）。今は `worktree-*` ブランチが 0 本で、測れない。修正案の判定は upstream を見ないので、答えに依らず効く

## 敵対的調査（3b）の所見と採否

全文は `workspace/adversarial-1507.txt`。

壊せたもの（採る）:
- `-d` の基準の記述が粗い。upstream の参照先が無い（gone）と HEAD 基準に落ちる。upstream があれば HEAD を見ない。→ 上の「技術的な制約」の 1 行目はこの精度で読む。修正案は `-d` の基準に依らないので、計画は変えない
- `verify-tree.test.mjs` の `initRepo` は `feat/x` を checkout するので、root の HEAD は `feat/x` である。既存の align 版のテストは repo の HEAD を進めるので、委譲先のブランチは HEAD の祖先になり、`-d` が通る。既存のテストはバグの経路を踏まない → 新しいテストは「HEAD がブランチの先端を含まない」状態を作る
- `git branch X origin/main` や `git worktree add -b X path origin/main` のように、始点に追跡ブランチを使うと upstream が自動で付き、`-d` が通る（再現のテストが偽の緑になる）→ テストの始点には sha を使う
- `for-each-ref` で upstream を見る勧めは、今は対象 0 本で何も示さない → 未解決の疑問を上のとおり書き直した

壊せなかったもの: `cmdRemove` の `-d` とブランチ名の導出／main ガード／`clean-worktrees.mjs` の記述と `--force` の扱い／`--is-ancestor` の終了コード（0・1・参照が無ければ 128）／`update-ref` で追跡ブランチを置けば再現に足りる／`origin/main` が無いときは `-d` に任せる／基準を `origin/main` に固定する見立て。

計画へ持ち込む実装上の条件:
- `execFileSync` は 0 以外で throw するので、`--is-ancestor` の結果は `e.status` で 1・128 を分ける。それ以外の失敗は握りつぶさない
- 基準は曖昧さの無い `refs/remotes/origin/main` で指す（⚠ 曖昧になる実害は未測。指し方を変える費用が無いので採る）
- `clean-worktrees.mjs` の症状は、呼び出し側の HEAD が古いときだけ出る。判定を共通にしなければ直らない → 共通の関数にする

環境: 本体 `C:/workspace/Snotra` は `main` で、今は `main` と `origin/main` が同じ sha（23197681）。再現はテストの側で作る。

⚠ 未実測: 祖先の確認から `-D` までの間にブランチが動く競合（委譲先の worktree は消した後なので、論理上は起きない）。linked worktree から打ったときとの対比（`cmdRemove` は常に `root` で打つので、計画には影響しない）。
