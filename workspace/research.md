# 調査: issue #1499 — pre-bash の gh pr create 検出が、引用符で語を割った綴りを素通りする

ブランチ: `fix/pre-bash-quoted-gh-pr-create`（origin/main 5f6e2697 から）

## issue の要約

`.claude/hooks/pre-bash.mjs` の `GH_PR_CREATE` は、コマンド位置に現れる `gh pr create` を検出し、(1) 空 PR (2) plan.md の未チェック (4) ツールの要約の照合を行う。語を引用符・バックスラッシュで割る（`gh pr "create"`・`cre""ate`・`cr\eate`・`"gh" pr create` など）と、シェルでは同じコマンドとして走るのに検出を素通りする。#1493 では、hook に止められた委譲先がこの形へ自然に書き換えた。決めることは「検出するか、受容する未対応リスクの一覧へ足すか」。

2026-11-21 にこのブランチで再現を確かめた: `GH_PR_CREATE.test('gh pr "create" --fill')` と `GH_PR_CREATE.test('gh pr cre""ate')` は `false`。

## 検出か受容か（文書から決まる）

検出する。根拠:

- hook 冒頭の「受容する未対応リスク」は、「gh がコマンド位置に現れない」形で、かつ「意図的迂回であり事故モードではない」ものに限っている。引用符で語を割った形では、`gh` はコマンド位置にある（引用符を外せば素の呼び出しになる）。そのうえ #1493 では、止められたエージェントがこの形へ自然にたどり着いた。つまり事故モードであり、一覧の前提に当てはまらない
- `AT_CMD_POS` の注釈「過剰検出は許容、過小検出は不可」
- `docs/development-principles.md`「規則を守らせる強さ」の原則 6（判定は構文の位置で決める・#482）とも矛盾しない。引用符を外しても、判定は依然として「コマンド位置か」で決まる。payload 全体を grep する形には戻らない

## 関連ファイル・シンボル（grep で実在を確認済み）

- `.claude/hooks/pre-bash.mjs`
  - `AT_CMD_POS`（64 行）、`FLAG`（74）、`ENV_PREFIX`（77）、`GH_PR_CREATE`（82）、`GIT_PUSH`（88）、`CWD_CHANGE`（94）
  - `tokenStart`（112）、`segmentEnd`（106）、`hasSafeChain`（123）。`hasSafeChain` は自分で `tokenStart(GH_PR_CREATE, command)` と `tokenStart(GIT_PUSH, command)` を呼び、位置を比べる
  - `decide`（333）。`judgeCommandShape` → `tokenStart(GH_PR_CREATE)` → `CWD_CHANGE` の位置比較 → reader → `hasSafeChain` の順
  - `main`（573 行付近）。**`tokenStart(GH_PR_CREATE, command) >= 0` のときだけ** `scripts/verify-tree.mjs` を動的 import する。`decide` と別に検出を呼ぶので、2 か所の検出が食い違うと、`decide` は検出したのに `tool` が null になる。そのとき `readVerifyState` は「verify-tree.mjs を読み込めませんでした」で block する（fail-closed だが理由が偽になる）
  - 冒頭コメントの「受容する未対応リスク」（32〜38 行）。「バッククォート / `$(...)` の内側」を挙げている
- `.claude/hooks/pre-bash.test.mjs`
  - `describe("GH_PR_CREATE — コマンド位置検出（過剰検出は許容・過小検出は許容しない）")`（121 行）。誤爆の回帰（`grep -n "gh pr create" CLAUDE.md` は検出しない）、引用内の `&&` の後ろは検出する（過剰検出を意図として固定）、ラッパ経由は検出しない（受容）
  - `describe("hasSafeChain …")`（190）、`describe("decide — 分岐表")`（246）、`describe("プロセス起動 — exit code の契約")`（643）
- `GH_PR_CREATE` / `tokenStart` / `hasSafeChain` の利用者は、`pre-bash.mjs` とそのテストだけ（`git grep` で確認。`docs/superpowers/plans/` の古い計画の言及は過去の記録）

## 再利用できる既存のパターン

- 判定を名前付きの関数へ切り出して export し、テストから直接呼ぶ形（`usesHeredoc`・`usesNoVerify`・`gitSegments` など）
- 過剰検出・受容する過小検出を `it.each` で意図として固定する形（121 行の describe）
- 判定単位を冒頭コメントで宣言する形（「コマンド位置」と「コマンド全体」）

## 技術的な制約

- hook は `CLAUDE_PROJECT_DIR` が指す本体の版で走る。この worktree の変更は、本体の作業ツリーが持つまで実際のゲートに効かない（冒頭コメント・#1436）。検証は単体テストとプロセス起動テストで行う
- hook の timeout（exit≠2）は fail-open になる。正規表現に曖昧な量化を足すと、指数的なバックトラックの恐れがある（`FLAG` の注釈・747ms の実測）。正規表現を広げるより、文字列を前処理して既存の正規表現に通すほうが安全
- 正規化した文字列で検出したなら、位置を使う後段（`CWD_CHANGE` の比較、`hasSafeChain`）も同じ正規化後の文字列で判定しなければ、位置がずれる
- `main` の import の判定と `decide` の判定は、同じ関数を通す必要がある（上記）
- セーフティネットの変更なので、マージはユーザーとの合意のあと（ルート `CLAUDE.md` 最重要ルール 2）。PR 本文の冒頭に「マージは合意してから」と書く

## 正規化の案

コマンド文字列から、シェルが語を組み立てるときに消える文字を取り除いてから、既存の正規表現に通す。

1. 行継続（`\` + 改行、PowerShell の `` ` `` + 改行）を取り除く（連結する）。改行を残すと区切り文字に見え、`hasSafeChain` の判定が変わる
2. 残った `"`・`'`・`\`・`` ` `` を取り除く

過剰検出の増え方: 取り除くのは引用符とエスケープだけで、区切り文字は増やさない。ただし区切り文字の直後に引用が来る言及は、引用が外れてコマンド位置になる（下の「敵対的調査」の争点 2・4）。また `\;`（`find -exec … \;`）の `\` を外すと `;` が裸になり、区切りとして見える。`git push && find … \; && gh pr create` のように push と gh の間に来ると安全なチェーンと見なされなくなり、動的判定（upstream と未 push）へ回る。fail-closed 方向なので受容する。`git commit -m "gh pr create を足す"` は、正規化しても `gh` の前が `-m ` でコマンド位置ではないので検出しない（誤爆の回帰にならない）。

## 未解決の疑問

- PowerShell のエスケープ文字 `` ` ``（`` gh pr cr`eate ``）も同じ事故の形として含めるか。hook の管轄は Bash と PowerShell の両方（`TARGET_TOOLS`）なので含めるのが筋と考える。Bash のバッククォート（コマンド置換）は、取り除いてもコマンド位置に `gh` が来る形（`` `gh pr create` `` で始まる）だけ検出が増え、過剰検出の側へ動く。冒頭コメントの「バッククォートの内側」の記述を合わせて直す必要がある
- `$'...'`（ANSI-C 引用）・`$"..."` の `$` は取り除かない。`gh pr $'create'` は `$create` になり検出しない。受容の一覧へ足すか
- 正規化を `judgeCommandShape` 側（`git` のセグメント判定など）にも当てるか。issue の範囲は `gh pr create` だけなので当てない（同じ形の素通りは `git commit --no-verify` などにもありうるが、別の判定である）

## 敵対的調査（3b）の所見と採否

全文は `workspace/adversarial-1499.txt`。sonnet 1 体。正規化のプロトタイプを `decide()` と各正規表現に通して測った。

### 壊せた項目

- **争点 1（検出が文書から導ける）— 一部を採る。** `gitSegments` の注釈は、引用内の区切りと行継続でセグメントが早く切れる過小検出を、「`sh -c` と同格の意図的迂回」として受容している（`pre-bash.mjs` の `gitSegments` の doc で確認した）。この先例があるので、「文書から一意に導ける」とは言えない。ただし先例が受容しているのは、`git` の**セグメントの切れ方**（引用の内側を解釈しないこと）であり、語の組み立て（`pr "create"` が `create` になる）ではない。#1493 で実際に起きた事故の形でもない。そこで検出を推す判断は変えないが、要求の判断として**（裁定）**に回す。`git "pull"`・`git commit "--no-verify"` の同じ形の素通りは範囲外とし、PR 本文に記録する
- **争点 2・4（言及は誤爆しない／過剰検出は正当なコマンドを止めない）— 採る。** 区切り文字（`(`・`{`・`;` など）の直後に引用が来る言及は、引用を外すとコマンド位置になる。`node -e "console.log('gh pr create')"` と `git commit -m "fix ('gh pr create' detection)"` は、現状は allow、正規化後は検出される。これは既存のテストが意図として固定している過剰検出（`echo "&& gh pr create"` は検出する）と同じ類で、fail-closed の向きである。検出されてもゲートが緑なら allow になる。受容し、テストに意図として固定する。主張「正当なコマンドを新たに止めない」は偽なので、research から外す
- **測定とは別の発見 — 採る。** `gh pr create'x'` は、現状は検出され、正規化後は検出されない。シェルでは `createx` なので、正規化後のほうが正しい。テストに固定する
- **`$(...)` の記述 — 採る。** 冒頭コメントは「`$(...)` の内側」を未検出としているが、`echo $(gh pr create)` は `(` が `AT_CMD_POS` の区切りなので、現状でも検出される（委譲先のコマンドが実際にライブの hook に止められた）。冒頭コメントの一覧を直す対象に含める

### 壊せなかった項目

- **争点 3（後段も同じ正規化を通す）— 採る。強めて採る。** `main()` と `decide` がずれると、偽の理由（「verify-tree.mjs を読み込めませんでした」）で block する。`CWD_CHANGE` の位置比較だけが元の文字列だと、**fail-open** になる（`echo "a" "b" "c" "d" "e" "f"; cd x; gh pr "create"` で cd の位置が gh の位置より後ろになり、cwd 変更の block が外れた）。さらに、**`decide` の入口でコマンド全体を正規化すると `judgeCommandShape` が弱まる**（`--body-file "C:\Users\x\y.md"` の `\` パスの判定が外れる）。正規化は、`gh pr create` の判定（検出・`CWD_CHANGE` の比較・`hasSafeChain`・`main` の import の判定）だけに通し、`judgeCommandShape` には元の文字列を渡す
- **争点 5（バックトラックの爆発を招かない）— 結論は採り、理由は直す。** n=2000 のフラグ連鎖・21 万字まで 2ms 未満。爆発しない理由は前処理だからではなく、`FLAG` のダッシュの分割が 1 通りに決まるからである。前処理は既存の正規表現の形を変えないので、その性質を保つ
- **争点 6（利用者は pre-bash.mjs とテストだけ）— 壊せなかった。** `scripts/merge-pr.mjs` が import するのは `verifiedSummaryState` で、別のシンボル
- 各節の行番号・describe の位置は一致した。`\;` の記述は妥当。行継続（`gh pr \` + 改行 + `create`）は issue の表に無い穴で、案が塞ぐ

### ⚠️ 確信なし

- `$'create'`・`$"create"`・`c$()reate`・`${X}create`・`cr$'e'ate`・cmd の `^` は、正規化しても素通りする。展開・置換の解釈はシェルのパーサ相当になるので、受容する未対応リスクの一覧へ足す（裁定の答えが「検出する」でも同じ）
- PowerShell ツールが hook に渡す文字列が逐語かどうかは、実機で確かめていない（settings の matcher は `Bash|PowerShell`、hook は `tool_input.command` を読む）。単体テストで両方の綴りを固定するに留める
- 実装案での `main` / `decide` を通した end-to-end は、実装時にプロセス起動テストで測る
