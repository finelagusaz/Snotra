# 調査 — issue #1523 pre-bash の検出に残る素通り 3 件を塞ぐ

## issue の要約

#1519 は `gh pr create` の検出を、引用符・エスケープで語を割った綴りにも効かせた。同じ型の素通りが 3 件残っている（#1519 の「残したもの」）。

1. `usesNoVerify`・`pullWithoutFfOnly` が引用で語を割った綴り（`git "pull"`・`--no-ver""ify`）を見ない
2. `hasSafeChain` が引用の内側の `git push` を安全なチェーンの push と取り違える（`git commit -m "a && git push" && gh pr create` が allow）
3. 空白を含む引用値の前置・フラグ（`GH_TOKEN="a b" gh pr create`）で検出を素通りする

## 関連するファイル・シンボル（`.claude/hooks/pre-bash.mjs`）

- `ghPrCreateView(command)`: 引用符とエスケープ文字（`"` `'` `\` `` ` ``）を取り除いた見え方。区切り文字は消さない。今は `ghPrCreateSites` だけが使う
- `ghPrCreateSites(command)`: 元の文字列と見え方の両方で `GH_PR_CREATE` を探す
- `GH_PR_CREATE`: `AT_CMD_POS` + `ENV_PREFIX` + `gh FLAG pr FLAG create`
- `ENV_PREFIX = (?:[A-Za-z_]\w*=\S*\s+)*`: 値は `\S*`。空白を含む引用値を読めない
- `FLAG = (?:(?:--?[^-\s]\S*|--)(?:\s+[^-\s]\S*)?\s+)*`: 値は `[^-\s]\S*`。空白を含む引用値を読めない。分割が一意になるよう組んである（`--x` の曖昧さで 747ms の指数爆発が起きた。`pre-bash.test.mjs` の「`git` の直後にフラグが並ぶ入力が指数爆発しない」が時間で縛る）
- `gitSegments(command)`: コマンド位置の `git` から次の区切りまで。`usesNoVerify`・`pullWithoutFfOnly` がこれを使う。`GIT_AT_CMD` は `ENV_PREFIX` を使う
- `GIT_COMMIT`・`GIT_PULL`: `^git\s+FLAG(commit|pull)\b`
- `hasSafeChain(command)`: `GH_PR_CREATE` と `GIT_PUSH` の最初の一致位置で、push が先にあり間の区切りがすべて `&&` か。`AT_CMD_POS` は引用の内側の区切りも区切りとみなすので、引用の内側の `&& git push` が push として当たる
- `decide`: `sites.some((s) => s.text === command) && sites.every((s) => hasSafeChain(s.text))` で静的に allow する。外れれば動的判定（upstream と未 push）
- `PYTHON_AT_CMD`: `ENV_PREFIX` を使う（`ENV_PREFIX` を変えると `needsPyEncoding` の検出も広がる）
- `.claude/hooks/pre-bash.test.mjs`: `ghPrCreateView` を import するテスト群（「ghPrCreateSites / ghPrCreateView — 引用符・エスケープで語を割った綴り（#1499）」）、`hasSafeChain` の describe、病的入力（`PATHOLOGICAL`）で throw しないこと、FLAG の指数爆発のテスト

## 再利用できる既存のパターン

- #1519 の形: 元の文字列と引用符を除いた見え方の**両方**で判定し、どちらかで当たれば検出する（`ghPrCreateSites`）。区切りは消さない
- 過剰検出は fail-closed 方向として受容し、テストで意図として固定する（冒頭コメント）
- 安全なチェーンを認めない形は、静的 allow を外して動的判定へ回す（#1519 で引用符で割った `gh pr create` をそうした）

## 技術的な制約

- hook は全 Bash/PowerShell コマンドで走る。判定は全域関数で、throw も hang もしてはならない（入力長に対して線形に保つ）
- `FLAG`・`ENV_PREFIX` に選択肢を足すと、分割の曖昧さで指数爆発しうる。足す選択肢は先頭の文字で互いに排他にする（引用で始まる値と、引用で始まらない値）
- shell パーサ相当まで広げない（冒頭コメント。#482 の D2）
- hook は本体の版で走る。この PR の変更はマージして本体が進むまで効かない（#1528 で `merge:pr` が本体を進めるようになった）
- `.claude/hooks/**` はセーフティネット。マージはユーザーの合意の後

## 未解決の疑問

1. `usesNoVerify`・`pullWithoutFfOnly` に見え方を足すとき、`ghPrCreateView` を流用するか。名前が `gh pr create` 専用に見えるので、汎用の名前へ改名するか
2. `hasSafeChain` の直し方: 引用の内側かを判定する（引用符の対応を数える小さな走査）か、push より前に引用符・エスケープ文字が 1 つでもあれば静的に認めない（保守的で単純）か。後者は `git commit -m "x" && git push && gh pr create` を動的判定へ回す（upstream が無い新しいブランチでは止まる）
3. `ENV_PREFIX`・`FLAG` に引用値を足すと、`needsPyEncoding` と `gitSegments` の検出範囲も広がる。広がる向きは過剰検出（fail-closed）か
4. 見え方（引用符を除く）で `ENV_PREFIX` を読ませる案は効かない（`GH_TOKEN=a b gh …` の `b` は代入に見えない）。元の文字列の側で引用値を読む必要がある

## 敵対的調査（`workspace/adversarial-1523.txt`）の採否

測った版: worktree の HEAD `b6196f2e`。本体の `pre-bash.mjs` と一致（cmp）。

- 壊せなかった: 3 件の素通りは実在する。見え方だけでは `ENV_PREFIX` を読めない。排他でない選択肢は爆発する。`hasSafeChain` は最初の一致位置を使う。静的 allow を外れると動的判定へ回る
- 採る（範囲を広げる）: issue 外の同型の素通りも実在する。`A="a b" git commit --no-verify`・`A="a b" git pull`・`git -C "my dir" pull`・`git -c "a b=c" pull`・`git -C "my dir" commit -n`。どれも `ENV_PREFIX`・`FLAG` が空白を含む引用値を読めないことによる。同じ修正で塞がるので範囲に入れる
- 採る（設計を変える）: `ENV_PREFIX`・`FLAG` の値に引用値の選択肢を排他に足すと、引用符が対にならない値（`A=it's git pull`・`A=" git pull`・`A=it's python -c 'あ'`・`A=it's gh pr create`）で今の検出が消える。`\S` を fallback に足すと排他でなくなり爆発する（`"a"` を 22 個並べて 5119ms）。→ 旧い正規表現を残し、引用値を読む正規表現を**別に**足して、どちらかで当たれば検出とする（各正規表現は単独で排他を保つので爆発しない。旧い正規表現が残るので退行しない）
- 採る（受容して固定する）: 見え方で `usesNoVerify`・`pullWithoutFfOnly` を判定すると、新しく止まる形がある（`git commit -m "--no-verify"`・`git log "--no-verify"`・`git commit -m "fix don't use -n"`）。どれも fail-closed の向きで、冒頭コメントの方針（過剰検出は受容してテストで固定する）に沿う。テストで意図として固定し、doc に書く。`git log --grep="--no-verify"`・`git commit -m "pull request"`・`echo "git pull"` は止まらないことも固定する
- 採る（保守案のまま）: `hasSafeChain` は、push より前に引用符・エスケープ文字があれば静的に認めない。`git commit -m "x" && git push && gh pr create` が動的判定へ落ち、新しいブランチでは止まる。この形はリポジトリの文書・テストに実例が無く、復帰手順（push してから PR を作る）がそのまま適用できる。奇偶で判定する狭い案は、引用の解釈を shell パーサへ近づけるので採らない
- 範囲外（PR 本文へ）: `echo a || git push && gh pr create`・`false && git push && gh pr create` が `hasSafeChain` で true になる（push が走らないのに gh が走る）。引用と無関係の既存の穴
- ⚠️ 実際に走る hook の版（本体か worktree か）は、今は両者が同一で区別できない。冒頭コメントの #1436 の観察（本体の版で走る）に従う
