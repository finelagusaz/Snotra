# #1499 導出（コードから独立に）

対象: ブランチ `fix/pre-bash-quoted-gh-pr-create`（HEAD 5f6e2697 の作業ツリー）の `.claude/hooks/pre-bash.mjs`（610 行）と `.claude/hooks/pre-bash.test.mjs`（1226 行）。行番号はこの版。
測定は、hook を node で import する probe と、hook の複製（scratchpad の `pre-bash.patched.mjs`。ライブの hook には触れていない）に候補の修正を当てた probe で行った。vitest は回していない（後述）。

## 導出した変更ファイルとシンボル

### 現状の実測（未修正の hook を import）
issue の表を再現した。`decide`（reader は「upstream 無し・未 push あり」= ゲートが働けば block になるもの）の結果:

| 綴り | `tokenStart(GH_PR_CREATE, c)` | `decide` |
|---|---|---|
| `gh pr create --fill` | 0 | block |
| `gh pr "create"` / `gh pr 'create'` / `gh "pr" create` / `"gh" pr create` / `gh pr cre""ate` / `gh pr cr\eate` | -1 | **allow** |
| `git push -u origin HEAD; gh pr "create" --fill` | -1 | **allow** |

素通りの原因は 1 か所: `GH_PR_CREATE`（pre-bash.mjs:82-84）が生の `command` に対して `gh\s+…pr\s+…create\b` を要求し、引用符・バックスラッシュを語の一部として扱わない。`AT_CMD_POS`（:64）の手前にある `"` も区切りではないので、`"gh"` はコマンド位置と見なされない。

### 検出の結果を使う下流（`GH_PR_CREATE` の消費者を全部たどった）
`grep -n GH_PR_CREATE` の結果、消費者は hook 内の 3 か所とテストだけである（`scripts/merge-pr.mjs` は `verifiedSummaryState` だけを import し、`GH_PR_CREATE` は使わない: merge-pr.mjs:31）。

1. `hasSafeChain`（:123-139）: `ghAt = tokenStart(GH_PR_CREATE, command)` と `pushAt = tokenStart(GIT_PUSH, command)`、`segmentEnd`、`DRY_RUN`、区切り列 `SEPARATOR_RUN` を、**同じ文字列の上の位置**で比べる
2. `decide`（:346-352）: `ghAt` で管轄の判定、`cwdAt = tokenStart(CWD_CHANGE, command)` と `ghAt` を位置で比べて cwd 変更を block。その後 `hasSafeChain(command)`（:369）を呼ぶ
3. `main`（:573）: `tokenStart(GH_PR_CREATE, command) >= 0` のときだけ `scripts/verify-tree.mjs` を動的 import して `readVerifyState` に渡す。**ここが `decide` と別の述語になると**、`decide` は検出するのに `tool` が null のまま `readVerifyState` が「verify-tree.mjs を読み込めませんでした」（:480-482）で block する——fail-closed だが理由が偽になり、要約が正しい安全なチェーンまで止まる

`judgeCommandShape`（:310。`decide` の :343 で `GH_PR_CREATE` より前に走る）は `GH_PR_CREATE` を使わない。ただし入力は同じ `command` なので、正規化を `decide` の先頭で全体へ掛けると巻き込まれる（不変条件 I2）。

### 導出した変更（「検出する」場合）
**`.claude/hooks/pre-bash.mjs`**
- 新設: 照合用の見え方を作る関数（仮名 `dequoteWords`）。`"`・`'` を取り除き、`\X`（X は改行以外）を `X` にする。複製での実装は 1 行:
  `c.replace(/\\([^\r\n])|["']/g, (_, x) => x ?? "")`
  `\`+改行（行継続）は残す（`SEPARATOR_CLASS` の改行を消さないため）。長さに線形で、全コマンドで走っても `judgeCommandShape` 冒頭注釈（:147-150「全域関数・線形」）に反しない
- 新設: 検出の単一通過点（仮名 `ghPrCreateAt(command)` = `tokenStart(GH_PR_CREATE, dequoteWords(command))`）。`main`（:573）と `decide`（:346）がこれを呼ぶ。2 か所に正規化を手書きすると、上の 3. の食い違いを作れる形が残る（原則 2「単一の通過点」）
- `hasSafeChain`（:123）: 冒頭で `command = dequoteWords(command)` とし、`GIT_PUSH`・`segmentEnd`・`DRY_RUN`・`SEPARATOR_RUN` を同じ見え方の上で測る。`ghAt` だけを正規化した位置にすると、生の文字列の `pushAt` と単位がずれる
- `decide`（:346-352）: `view = dequoteWords(command)` を作り、`ghAt` と `cwdAt` を `view` の上で取る。`judgeCommandShape(command, …)`（:343）には**生の** `command` を渡したままにする
- 注釈: (a) `AT_CMD_POS` の注釈（:62-63「引用の内側までは解釈しない」）を、「引用符とバックスラッシュは照合の前に外す。引用の内側の区切りは解釈しない」へ直す (b) 冒頭「判定単位」（:13-22）の「コマンド位置」の項に、`gh pr create` の判定は引用符を外した見え方で測ることを足す (c) 冒頭「受容する未対応リスク」（:33-39）に、正規化で拾わない残りを足す: PowerShell の backtick エスケープ（`` gh pr cr`eate ``）、`$'…'`（`gh pr $'create'`）、変数展開・空の展開（`gh pr cre${X}ate`）。いずれも複製で実測して素通りした（下表）
- 拒否文言（`REMEDY`:96 など）は変えなくてよい（検出するなら書き換えの誘因は止まる）

**`.claude/hooks/pre-bash.test.mjs`**
- import（:9-31）に新しい通過点を足す
- 新しい describe（仮:「引用符・バックスラッシュで語を割った綴り（#1499）」）を `GH_PR_CREATE` の describe（:121）の隣に置く。**既存の describe は `GH_PR_CREATE.test(raw)` を直接呼ぶ**（:124-175）ので、正規化を `decide` 側に足すと、そこへ足したテストは生の正規表現を測ってしまい常に false になる。新しいテストは通過点か `decide` を呼ぶ
- 既存 describe の名前（:121「過剰検出は許容・過小検出は許容しない」）と受容リストの it.each（:167-176）に、上の残余 3 形を「検出しない（受容）」として足す
- e2e（describe「プロセス起動 — exit code の契約」:643）に 1 本: `verifiedRepo()` で `git push -u origin HEAD && gh pr "create"` が exit 0（`main` の通過点の配線を固定）

**他のファイル**: `docs/` と `.claude/rules/` には、引用で割る形や `GH_PR_CREATE` に触れた記述は無い（`grep -rn "引用符\|GH_PR_CREATE\|pre-bash" docs/*.md .claude/rules/*.md` は 0 件）。`SPEC.md` は製品仕様で hook を持たない（未確認・⚠️ 参照）。

### 候補の修正を当てた複製での実測（`decide` の結果）
| 入力 | 未修正 | 修正後 |
|---|---|---|
| issue の 8 綴り（表のとおり） | 7 件 allow | **8 件すべて block** |
| `git commit -m "gh pr create を足す"`（issue の言及の例） | allow | allow（過剰検出しない） |
| `grep -n "gh pr create" CLAUDE.md`（既存 U4 の誤爆根治 :124） | allow | allow |
| `git commit -m 'x⏎"gh pr create" を足す'`（引用の内側で、行頭が引用符） | allow | **block**（新しい過剰検出） |
| `echo "a" && "gh pr create"`（1 語として引用＝シェルでは command not found） | allow | **block**（過剰検出） |
| `git "push" -u origin HEAD && gh pr "create"` | allow（素通り） | allow（`hasSafeChain` が真。位置が揃っている） |
| `cd x && gh pr "create"` / `"cd" x && gh pr "create"` | allow（素通り） | block（cwd 変更） |
| `` gh pr cr`eate `` / `gh pr $'create'` / `gh pr cre${X}ate` | allow | allow（残余。受容リストへ足す） |

既存テストの入力 18 件（:124-175 の全入力と、`sh -c "…"`・`bash -c '…'`・`gh pr list --search "create"`・`gh pr view "create"`）を複製の通過点に当て、期待との不一致 0 件。とくにラッパ（`sh -c 'gh pr create'`）は正規化後も `-c gh` で `gh` がコマンド位置に来ないので、受容リストの既存の意図は変わらない。

### 同じパターンを持つ別の判定（AGENTS.md「バグを見つけたら、同じパターンを持つコードパスをすべて探す」）
未修正の hook で実測:
- `usesNoVerify`: `git commit --no-ver""ify` / `"git" commit --no-verify` / `git commit "--no-verify"` → すべて false（素通り）
- `pullWithoutFfOnly`: `git "pull"` → false（素通り）
- `GIT_PUSH`: `git "push"` は検出しない。これは安全側（安全なチェーンと見なさない＝block 寄り）なので害は無いが、`hasSafeChain` を正規化するとあわせて検出されるようになる
同じ正規化を `gitSegments`（:237）にも掛けるかは、#1499 の範囲（`gh pr create`）の外として決める必要がある。掛けるなら `usesBackslashPath` / `usesHeredoc` は生のまま残す（I2）。既存テスト :982-986 は「引用内の `;` でセグメントが切れる」`--no-verify` の過小検出を「`sh -c` と同格の意図的迂回」として受容しており、#1499 で「事故で到達した」と判断を変えるなら、こちらの受容とも整合を取る必要がある。

## 不変条件と検知手段

| # | 不変条件 | 検知手段（テスト / 変異注入） |
|---|---|---|
| I1 | 引用の内側の言及では発火しない（#482 D2 の根治）。`grep -n "gh pr create"`・`git commit -m "gh pr create を足す"` は allow | 既存 :124 + 新規 1 本（issue の例）。変異: 正規化を「コマンド位置を問わず `gh pr create` を探す」に替える → 赤 |
| I2 | `judgeCommandShape`（`usesBackslashPath`・`usesHeredoc`・`needsPyEncoding`）は生の `command` を見る。正規化を全体に掛けると `C:\workspace` が `C:workspace` になり、`\` パスの判定が消える（fail-open） | 既存 decide テスト :1126-1133（`Get-Content C:\workspace\x.md` を `decide` 経由で block）。変異: `decide` 先頭で `command = dequoteWords(command)` → この既存テストが赤になるはず（**未実行**） |
| I3 | `decide` と `main` は同じ検出述語を使う（食い違うと verify-tree が読み込まれず偽の理由で block） | 新規 e2e: `verifiedRepo()` で `git push -u origin HEAD && gh pr "create"` が exit 0。変異: `main`:573 を生の `tokenStart` のまま残す → exit 2（「読み込めませんでした」）で赤。理由まで assert する |
| I4 | `hasSafeChain` の `ghAt`・`pushAt`・区切り列は同じ文字列の上で測る | 新規: `git "push" -u origin HEAD && gh pr "create"` を未 push reader で `decide` → allow。変異: `hasSafeChain` を正規化しない → `ghAt` -1 で false → block で赤 |
| I5 | `cwdAt` と `ghAt` は同じ見え方で比べる | 新規: `"cd" x && gh pr "create"` → block、**理由に「作業ディレクトリを変更」を assert**（safety-nets.md「理由ごとに assert」）。変異: `cwdAt` を生の `command` で取る → cwd を見落とし、別の理由（upstream 未設定など）で block になる。action だけの assert では緑のまま残る |
| I6 | 既定は block（exit 2）。正規化の関数は throw しない全域関数 | 既存の「5 述語は全域関数」describe（:1148）に新関数を足す。`String.prototype.replace` だけなので throw は非文字列入力のみで、`decide` は :338 で文字列を確かめた後に呼ぶ |
| I7 | 受容する残余（backtick / `$'…'` / `${X}` / ラッパ）は「検出しない」ことを意図としてテストに固定する（「過小検出ゼロ」と誤読させない。:167-168 の既存の意図） | it.each に足す。将来だれかが検出を広げたら赤になり、一覧の更新を強制する |
| I8 | 線形時間（全コマンドで走る） | 正規化は 1 パスの replace。`GH_PR_CREATE` 自体は FLAG の一意性（:66-74）で線形。正規化で長いフラグ列が新たに生まれることはない（引用符を消すだけ） |

hook は `CLAUDE_PROJECT_DIR` の本体の版で走る（:44-46）。worktree のセッションでは、自分の修正をライブのゲートで確かめられない。フォールトインジェクションは複製かテストで行う（safety-nets.md）。

## 検出か受容か

**推す: 検出する。** 根拠:

1. **受容リストの前提を満たさない。** 冒頭の一覧（:33-39）は「いずれも『gh がコマンド位置に現れない』形。意図的迂回であり事故モードではない」を受容の条件にしている。引用で割った綴りは、シェルの意味では `gh` がコマンド位置にあり、しかも issue の実例で hook に止められたエージェントが自然に到達した（事故モード）。一覧に足すと、一覧の見出しの条件が偽になる
2. **`AT_CMD_POS` の注釈（:63）「過剰検出は許容、過小検出は不可」と、テスト describe（:121）の題名が、検出を求めている。** 現状はこの宣言に反する過小検出である
3. **原則 6（判定は構文の位置で決める・#482）を壊さない。** 正規化しても位置の判定は残り、#482 の誤爆（`grep -n "gh pr create"`）は再発しない（実測）。増える過剰検出は「引用の内側で、行頭や区切りの直後に引用符が来る」形に限られ（実測 2 形）、fail-closed 方向で、回復は自明（`-F` で本文をファイルから渡す。ルート CLAUDE.md「commit と gh の本文は `-F` / `--body-file` で渡す」）
4. **原則 7（失敗の向きを既定値に）と強さの序列。** 受容すると、この形を防ぐのは拒否文言の言い回し（「書き手の記憶」の段）だけになる。しかも素通りは黙って起きる（safety-nets 欄の問い「壊れたとき緑のまま推移するか」に、推移する）。検出は「テストで検出」の段で守れる
5. **費用が小さい。** 1 行の正規化関数と通過点 1 つ、呼び出し点 3 か所（`decide`・`hasSafeChain`・`main`）。原則の「やりすぎない」（通過点の関数 1 つで 8 割を取る）に合う。shell パーサ相当へ広げるのではない（:40 の懸念には当たらない）。残余（backtick・`$'…'`・展開）は受容リストへ足して閉じる

受容を選ぶ場合に要るもの（比較のため）: 一覧の前提文の書き換え（「事故モードではない」を外すか、この形だけ例外と明記）、拒否文言（`REMEDY`・plan / verify の各文言）に「引用で割って回避しない」旨、それを守る手段が無いこと。issue 自身が「どこで防ぐか」を未解決の問いとして挙げており、受容は答えを持たない。

## 確信の持てない所見（⚠️）

- ⚠️ **既存の fail-open（正規化の前から在る・同じ族）。** `hasSafeChain` は**最初の** `gh pr create` の一致と、**最初の** `git push` の一致を使うので、引用の内側の言及が位置を前へずらす。未修正の hook で `decide` が allow を返した（reader は未 push）:
  - `git push && echo "&& gh pr create" ; gh pr create`（本物の gh は `;` の後ろ。push が失敗しても走る）
  - `git commit -m "a && git push" && gh pr create`（push は走らない）
  正規化すると、この族に新しい入り口が 1 つ増える: `git push && echo x'&&'gh pr create ; gh pr create` は未修正で block、修正後に allow（複製で実測）。どれも作為的な綴りで、事故で書く形かは疑わしい。直すなら「`ghAt` より前の区切りを見るのではなく、すべての `gh pr create` の一致に対して安全なチェーンを要求する」など別の設計が要り、#1499 の範囲を越える。別 issue 候補
- ⚠️ **過剰検出の増え方を実データで測れていない。** 過去セッションの transcript から Bash/PowerShell のコマンドを集めて、修正前後で検出が変わる件数を数えようとしたが、auto mode の分類器に拒まれた（`~/.claude/projects/` の読み取り）。上の 2 形は推論と合成入力での実測であり、頻度は不明
- ⚠️ **vitest を回していない。** ファイル編集の制約で複製をリポジトリ内に置けず、scratchpad には `node_modules` が無い。既存テストの入力 18 件と `decide` の入力を node で直接当てただけである。不変条件表の「変異 → 赤」はすべて未実行の予測である（I3〜I5 は `decide` / `hasSafeChain` を複製で直接呼んだ結果から推した）。実装時に実行して確かめる必要がある（とくに I2 の既存テストが本当に赤になるか）
- ⚠️ **PowerShell の意味論。** PowerShell では `"gh" pr create` は式として解釈されパースエラーになる（実行には `& "gh"` が要る）と理解しているが、実測していない。PowerShell tool では過剰検出になるだけで害は無い（fail-closed）。逆に PowerShell の backtick エスケープは正規化に入れていない。入れると bash のコマンド置換（`` `gh pr create` ``、受容リストの既存項目）とマークダウンの backtick の言及まで巻き込むので、tool_name で分けるかは要判断
- ⚠️ **`\\`（エスケープされたバックスラッシュ）の扱い。** 候補の正規表現は `\\x` を `\x`→… ではなく 1 回の置換で `\` を 1 つ落とすだけで、シェルの意味と完全には一致しない。照合で困る実例は思いつかないが、網羅は確かめていない
- ⚠️ **`gitSegments` 系（`--no-verify`・`git pull`）に同じ正規化を掛けるか**は、#1499 の受け入れ条件の外。掛けないなら、その素通り（上で実測）を受容リストかテスト :982 の隣に明記しないと、「同じパターンを持つコードパスをすべて探す」の完了条件を満たさない可能性がある
- ⚠️ この変更はセーフティネット（hook）の変更なので、ルート CLAUDE.md の最重要ルール 2 により、マージはユーザーと合意してから（PR 本文冒頭に「マージは合意してから」）。`SPEC.md` に `pre-bash` / `gh pr create` の出現は 0 件（Grep ツールで確認）
- ⚠️ **作業中に既存の過剰検出を自分で踏んだ。** `grep -n "pre-bash\|gh pr create" SPEC.md` を Bash で打ったところ、ライブの hook（本体の版）が引用の内側の `|` を区切りと見て `gh pr create` を検出し、plan.md ゲートで block した。これは正規化と無関係の既存の過剰検出（:63 で受容済みの「引用の内側の区切り」）で、回復は Grep ツールへの切り替えで済んだ。「過剰検出は回復が自明」という主張の 1 例にはなるが、`\|` を含む grep の正規表現は日常的なので、頻度は低くない可能性がある

## workspace/ の計画を目にしたか

計画の内容は目にしていない。`workspace/` 配下で読んだファイルは無い（このファイルへの書き込みだけ）。ただし 1 度、ライブの hook の拒否メッセージで「workspace/plan.md に未チェックの作業項目が 10 件残っています」という件数だけを見た（中身は見ていない。上の ⚠️ の過剰検出による）。grep・sed の対象は `.claude/hooks/`、`docs/development-principles.md`、`docs/*.md`、`.claude/rules/*.md`、`scripts/` に限った。
