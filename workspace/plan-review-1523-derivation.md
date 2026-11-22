# #1523 独立導出レビュー（plan を見ずに導出）

- **workspace/ の計画（plan.md・research.md など）は目にしていない。** 読んだのは `gh issue view 1523`・`.claude/hooks/pre-bash.mjs`（全文）・`.claude/hooks/pre-bash.test.mjs`（380-460・1150-1210・1340-1390）・自動配送された `.claude/rules/safety-nets.md` と `.claude/rules/governance-docs.md`。grep は `.claude/`（worktrees 除外）と `docs/` に限った。workspace/ に書いたのはこのファイルだけ。
- 実測は worktree の `pre-bash.mjs`（HEAD = 本ブランチ、未変更）を node から import して行った。プローブは scratchpad の `probe.mjs`・`chain.mjs`・`redos.mjs`（`C:\Users\Eoh\AppData\Local\Temp\claude\d9e233791a56fb00\180a97f6-65b1-44dd-b615-5b4a2b79409d\scratchpad\`）。
- `decide` の実測は readers を「plan 無し・verify ok・upstream 無し」に固定した。ALLOW と出たものは hook 全体を素通りする。

## 1. 変えるべきファイル・シンボルと理由

### `.claude/hooks/pre-bash.mjs`

| シンボル | 位置 | 理由 |
|---|---|---|
| `usesNoVerify` | :310 | `gitSegments(command)` を元の文字列でしか取らない。`git commit --no-ver""ify -m x` → false → ALLOW（実測） |
| `pullWithoutFfOnly` | :317 | 同上。`git "pull"` → false → ALLOW（実測） |
| `gitSegments` / `GIT_AT_CMD` | :290 / :279 | 見え方（`ghPrCreateView` 相当）の上でも切り出す必要。`"git" pull`・`g'it' pull`・`\git pull` は `GIT_AT_CMD` が `git` を見つけられない（実測、すべて ALLOW） |
| `ghPrCreateView`（名前と doc） | :96-110 | git の判定でも使うなら、名前（`gh pr create` 専用の見え方）と doc が偽になる。改名するなら呼び出し元（:121・テスト :13 ほか）を LSP で列挙 |
| `hasSafeChain` | :169-190 | `tokenStart(GIT_PUSH, …)`（:173）が最初の一致で、引用の内側の `&& git push` を拾う。**GH 側（:170）も最初の一致**（下の 2 の s-bothquoted）。見るべきは「引用の外の push」と「すべての gh」 |
| `decide` の cwd 判定・安全チェーン判定 | :403-408・:431 | `ghPrCreateSites` が見え方ごとに**最初の 1 件**しか返さない（:122）ので、`decide` も最初の gh しか比べない |
| `ENV_PREFIX` | :85 | 値が `\S*`。`GH_TOKEN="a b" gh pr create` → 検出 0（実測） |
| `FLAG` | :82 | フラグ・値が `\S*`。`gh --repo "a b" pr create`・`git -C "a b" pull` → 検出 0（実測） |
| 共有している正規表現 | `GH_PR_CREATE`:90・`PYTHON_AT_CMD`:262・`GIT_AT_CMD`:279・`GIT_COMMIT`/`GIT_PULL`:305-306 | `ENV_PREFIX`/`FLAG` を直すと全部が動く。`GIT_PUSH`（:130）は独自の `(?:-\S+\s+)*` で FLAG を使わない（直さなくても fail-closed 側：検出しないだけで allow は作らない） |
| 冒頭コメント | :15-18・:34-36・:38-45 | 5 を参照 |

### `.claude/hooks/pre-bash.test.mjs`
- 3 綴りのテスト（`decide` で理由まで assert：`noVerify`/`pull` の SHAPE_REMEDY 文言、upstream 文言）。
- ReDoS の時間テスト（:1376-1383）は `--f=v` の形しか縛らない。引用対応の値を入れるなら、引用の連続の形（3 の quotes40 など）の時間テストが要る。
- 既存テストで動きうるもの: :1183（引用内 `;` の受容。見え方でも `;` は残るので不変のはず）、:404-414（引用で作ったチェーンは動的判定へ）、:449-454（`--title "a b"` の安全チェーンは allow のまま）、:1167（`grep -n "--no-verify"` は git がコマンド位置に無いので不変）。

## 2. 同じ根を持つ、issue に無い素通り（すべて実測で ALLOW）

引用で語を割る（git 系）:
- `"git" pull` / `g'it' pull` / `\git pull` / `git pu""ll`
- `git "commit" -n -m x` / `git commit "-n" -m x` / `git commit \--no-verify -m x`
- `git push "--no-verify"` / `git push --no-verify""`

空白を含む引用値（前置・フラグ）:
- `git -C "a b" pull` / `git -c 'user.name=a b' pull` / `git -C "a b" commit -n -m x` / `git -c "user.name=a b" commit -n -m x`
- `GIT_DIR="a b" git pull` / `X="a b" git commit --no-verify -m x`（ENV_PREFIX 経由で `GIT_AT_CMD` も素通り）
- `gh --repo "a b" pr create` / `gh pr --repo "o r" create` / `GH_TOKEN='a b' gh pr create`
- `X="a b" python -c "print('日本')"`（`PYTHON_AT_CMD` も ENV_PREFIX 共有。win32 の pyEncoding が素通り）
- エスケープした空白: `GH_TOKEN=a\ b gh pr create` / `git -C a\ b pull`（見え方で `\` を消すと `a b` に割れて、どちらの見え方でも当たらない）
- 引用内のエスケープ引用: `GH_TOKEN="a\" b" gh pr create`（素朴な `"[^"]*"` では直らない。⚠️ `"(?:[^"\\]|\\.)*"` が要るが PowerShell は `` ` `` エスケープで規則が違う）
- 対照: `X="$(a b)" gh pr create` は今も検出される（見え方で `)` がコマンド位置を作る）

最初の一致（hasSafeChain / decide）:
- `git commit -m "a && git push && gh pr create" ; gh pr create` → ALLOW。**push と gh の両方が引用の内側**で、本物の gh は `;` の後ろ。「push〜gh の間に引用符が無いこと」を足すだけでは直らない（この形は間に引用符が無い）。**すべての gh の一致**を見る必要がある
- `git push && echo "x && gh pr create" ; gh pr create` → ALLOW（GH 側の最初の一致）
- `git commit -m 'a; git push' && gh pr create` / `git log --grep "x; git push" && gh pr create` / `echo "&& git push" && gh pr create` → ALLOW
- ⚠️ cwd 判定も最初の gh だけと比べる: `echo "; gh pr create" && cd ../other && gh pr create` は cwd 理由では止まらず、動的判定（今の cwd の upstream）へ落ちる。upstream 設定済み・未 push 無しのツリーなら ALLOW になるはず（readers を CLEAN にした実測はしていない）。引用の無い `gh pr create --help; cd ../o; gh pr create` も同じで、こちらは引用の根ではない

追記（advisor の指摘を受けて実測・`p3.mjs`）:
- `DRY_RUN`（:133）も元の文字列だけを見る。`git push "--dry-run" && gh pr create` / `git push --dry""-run && …` / `git push -"n" && …` / `git push '-n' origin HEAD && …` は **`hasSafeChain` 単体では true**（実測）。ただし `decide` は「検出したすべての見え方で `hasSafeChain` が真」（:431）を要求し、見え方では `--dry-run` が素で現れて false になるので、**今は動的判定へ回って block**（実測）。この保護は :431 の `every` に依存する。`hasSafeChain` や :431 の組み方を変えるなら、この 4 形を回帰テストに入れる
- cwd の最初の一致: readers を CLEAN（upstream 有り・未 push 無し）にすると `echo "; gh pr create" && cd ../other && gh pr create` → **ALLOW**、`gh pr create --help; cd ../o; gh pr create` → **ALLOW**（実測）。対照の `cd ../o && gh pr create` は cwd 理由で block。前者は引用の根、後者は引用と無関係な最初の一致の根

範囲外（同根ではないが、同じ判定を素通りする。⚠️ 参考）:
- 行継続 `git commit -m x \` + 改行 + `--no-verify`（:286-288 で受容済み）
- git の長オプション省略形 `--no-verif`、`-c core.hooksPath=…`（形の判定では見ていない。未実測）

## 3. 直したときの退行・指数爆発

### 指数爆発（実測）
- 引用を 1 語として読む値トークンを `(?:"[^"]*"|'[^']*'|\S)*` のように**引用符を `\S` でも食える**形で書くと、分割が曖昧になる。FLAG の `*` の下で `git --a=""""…"" " x`（引用 41 個）が **15 秒で終わらない**、`--f="a'b"'c"d'` × 24 も 15 秒超、`--f=a"b` × 24 は 86ms（実測。FLAG コメントの 747ms と同じ機序）
- 開始字が互いに素な形 `(?:"[^"]*"|'[^']*'|"(?![^"]*")|'(?![^']*')|[^\s"'])*` は同じ入力で 0.4ms 以下、検出 8/8（実測）
- **落とし穴**: 今の FLAG はフラグ・値の先頭を `[^-\s]` で 1 字消費する。その 1 字が開く引用符を食うので、トークンだけ直しても `git -C "a b" pull` / `gh --repo 'o r' pr create` は当たらない（実測 0）。先頭は `(?![-\s])` の先読みにする必要（直した版で検出、実測）
- 閉じない引用を区切りにする形 `(?:"[^"]*"|'[^']*'|[^\s"'])*` は `git -C a"b pull`・`gh --repo o/r'x pr create` を取り落とす（今は検出。実測）。シェルでも閉じていない引用なので実害は薄いが、fail-closed を減らす向き

### 既存の二乗（⚠️ 直す前からある・今回の変更で悪化しうる）
- `ENV_PREFIX` の値 `\S*` が区切り `;` を越えて食う。`("; A=x'").repeat(n) + " gh x"` で `decide` が 90KB で 6.3 秒、**180KB で 24.6 秒**（実測。hook の timeout は 30 秒 = 超えると非ブロッキング = fail-open）。トークンを替えても二乗は残った（3 形とも 210KB で約 4.6 秒、`redos.mjs` quad2）
- 実 hook は `decide` の外の `main()` でも `ghPrCreateSites` をもう 1 回呼ぶ（:632。単体で 180KB に 9.2 秒・実測）。合計は約 24.6 + 9.2 ≒ 34 秒で、**今の時点で 30 秒の予算を超える**（概算・プロセス級では未実測）。180KB のコマンドが harness から実際に届きうるかは確かめていない
- 見え方の上でも `gitSegments` を回すと、`GIT_AT_CMD` の走査が 2 倍になり、さらに悪化する。値トークンから区切り文字 `[;&|\n\r]` を外すと二乗の原因が消えるはず（⚠️ 未実測）

### 検出が減る形（判定の組み方による）
- `pullWithoutFfOnly` を**見え方だけ**で判定すると、`git pull origin "x --ff-only"`（今は block。実際はブランチ名 `x --ff-only` への非 FF pull）が `--ff-only` 付きに見えて allow になる。**元の文字列と見え方の OR**（どちらかで「ff-only の無い pull」なら block）にする
  - 実測: `git pull origin "x --ff-only"` は raw pull=true / 見え方 pull=false。`git pull "--ff-only"` も raw true / 見え方 false（こちらは見え方が正しい）
- 同じく `usesNoVerify` も OR でないと、元の文字列にしか現れない検出を落としうる（見え方は区切りを消さないので減る例は見つけていない。⚠️）
- 実測（見え方の上で既存の述語を呼んだ）: `git "pull"` → 見え方 pull=true、`git commit --no-ver""ify -m x` → 見え方 nv=true。issue の 2 綴りは「見え方でも判定する」だけで止まる。`git commit -m "a;b" --no-verify` は見え方でも false（受容済みの穴は残る・テスト :1183 は不変）
- `hasSafeChain` の安全条件を広げる向き（例: 見え方で判定する）は #1499 の `'&'&` 事故（:165）を再現する。今回は狭める向きだけにする

## 4. 新しく正当なコマンドを止める形

見え方を git 判定に足すと（`usesNoVerify`）:
- `git log --grep "--no-verify"` / `git commit -m "--no-verify"` / `git commit -m "-n"`（いずれも今は ALLOW。見え方では nv=true になる・実測）。拒否文言「迂回せずその理由を解消」は当てはまらないが、引用符で割らない書き方へは自力で戻れない（⚠️ 復帰手順が適用できない誤爆。safety-nets.md「拒否する機構の誤爆」）
- 既に止まっている形（不変）: `git commit -m "fix -n flag"`（元の文字列で ` -n ` が当たる）、`git pull "--ff-only"`（今も block。OR にすると block のまま）

`hasSafeChain` を「push〜gh に引用符が無い」＋「すべての gh」で狭めると（`chain.mjs` 実測）:
- `git push origin "feat/x" && gh pr create` / `git push -u origin 'HEAD' && gh pr create` → 動的判定へ（push 前なので未 push → block）
- `git push && gh pr create --body "a; gh pr create"` → 2 つめの gh が `;` の後ろとして数えられて動的判定へ
- 最初の push が引用内で 2 つめが本物: `git commit -m "a && git push" && git push && gh pr create` → 候補 A（最初の push）で false＝動的判定へ（実測）。「引用の外の最後の push」を選ぶ設計なら allow のまま保てる
- **ENV_PREFIX を引用対応にすると**: `git push && GH_TOKEN="a b" gh pr create` は今は gh 自体を検出しない（hasSafeChain=false・素通り）。直すと raw で検出され、`tokenStart` は ENV の後ろ（`gh` の位置）を返すので、「push〜gh に引用符が無い」条件はこの前置の引用で落ち、動的判定へ回る（候補 A は今の版でも false・実測。直した版での挙動はコード読み ⚠️）。引用の検査範囲は「gh のコマンド位置（前置の前）」までにする必要。対照 `git push && GH_TOKEN=x gh pr create` は候補 A でも true（実測）
- **拒否文言の不一致**: 上の形が動的判定で止まると、`REMEDY`（:138）「`git push … && gh pr create` と `&&` で繋いでください」を返す。利用者は既に `&&` で繋いでいるので、**復帰手順が適用できない拒否**になる（safety-nets.md「拒否する機構の誤爆」）。回避の道（`git push -u origin HEAD` を先に単独で打つ）は文言の前半にあるので回復は可能だが、狭めた理由（引用の内側の push は数えない）を文言で伝えるかどうかを決める必要
- 対照（不変）: `git commit -m "msg" && git push -u origin HEAD && gh pr create --title "t b"`、テスト :449 の形は allow のまま（実測）

## 5. 変更で偽になる散文

`.claude/hooks/pre-bash.mjs`:
- :15-18「`gh pr create` の判定**だけは**…`ghPrCreateView` の上でもコマンド位置を探す」— git 判定も見え方を使えば偽
- :34-36「引用符とエスケープ文字で語を割る形（`gh pr "create"` …）は検出する（`ghPrCreateSites`）」— git の形も足すなら所在が増える
- :38-45「受容する未対応リスク」— 引用値の素通りは列挙に無い（今は暗黙の穴）。直した後に残す穴（エスケープ空白・PowerShell の引用規則・行継続）を書くかどうか
- :74「値がスペース区切り（`--repo o/r`）でも読み飛ばす」— 引用値を足すなら補足
- :76-82 FLAG のコメント「`--?[^-\s]` はダッシュの直後に非ダッシュを要求するので分割は 1 通り」— 先頭を先読みに替えるなら書き直し
- :84「`GH_TOKEN=x gh pr create` のような環境変数の前置」— 引用値の扱い
- :96-107 `ghPrCreateView` の doc（`gh pr create` 専用の書きぶり）
- :113-117 `ghPrCreateSites` の doc「位置は見え方ごとに測る」— 全一致を返すなら形が変わる
- :159-168 `hasSafeChain` の doc（「最初の」とは書いていないが、引用の内側の push の扱いを書き足す）
- :281-289 `gitSegments` の doc「引用・継続の解釈は shell パーサ相当になる」— 見え方を入れても引用内 `;` の受容は残るので、その文はまだ真（⚠️ 書き分けが要る）
- :299-307 `NO_VERIFY`/`GIT_COMMIT` 付近のコメントは不変の見込み

- `REMEDY`（:138）: 4 の「拒否文言の不一致」。安全チェーンを狭めるなら、文言が偽（「`&&` で繋げば通る」）になる場合がある

その他:
- 概念語の grep（`gh pr create`・`GH_TOKEN`・`--ff-only`・`--no-verify`・`pre-bash`）を `.claude/skills/`・`.claude/rules/`・`.claude/agents/`・`AGENTS.md` に掛けた。`.claude/rules/`・`.claude/agents/` は 0 件。skills のレシピ `.claude/skills/implement/SKILL.md:170`（`git push -u origin HEAD && gh pr create --title "<…>" --body-file <path>`）は引用が gh の後ろだけなので、候補 A でも allow のまま（同形を実測）。同行の「機械で止まる条件の正本は `pre-bash.mjs` の冒頭」は真のまま。`deps-update`:26 の `git merge --ff-only` は pull ではないので無関係。`start-issue`:39・`deps-update`:67・`implement`:151 は要約ゲート・upstream の話で、今回の変更で偽にならない
- ルート `CLAUDE.md`「フック」: 判定の中身は書かず hook を正本として指すだけ — 偽にならない
- `.claude/skills/`・`docs/`: `usesNoVerify`・`hasSafeChain`・`ghPrCreateView` などの識別子は出現 0 件（`.claude/` と `docs/` を grep。docs の `pre-bash` 言及は `docs/superpowers/` の凍結済み計画・設計書と ADR のみで、引用・語割りの判定内容には触れていない）
- テスト名 `describe("ghPrCreateSites / ghPrCreateView — …")`（test :183）は改名に追従が要る

## 計画の差分のレビュー

- この節は、独立導出の後に coordinator の指示で `workspace/plan.md` を読んで書いた（ここから先は独立ではない。research.md は読んでいない）。
- 試作は scratchpad の `plan1523.mjs`（正規表現の設計）・`p4.mjs`（今の hook の素通り）・`p5.mjs`（`hasSafeChain` の計画案と代案）。計画の表の文言から正規表現を起こしたので、**解釈が実装とずれる可能性がある**（とくに `ENV_PREFIX_QUOTED` の値が「選択肢 1 つ」か「部品の連接」か）。

### 覆った所見

- 2 の語割り（git 系）: 受け入れ条件 1 と、`usesNoVerify`・`pullWithoutFfOnly` を元の文字列と見え方の OR にする変更。見え方の上で既存の述語を呼ぶ実測で、`git "pull"`・`--no-ver""ify`・`git "commit" -n`・`"git" pull` は止まる
- 2 の空白を含む引用値: `ENV_PREFIX_QUOTED`・`FLAG_QUOTED` と `*_QUOTED` の OR。試作で `GH_TOKEN="a b"`・`gh --repo "a b"`・`git -C "a b" pull`・`git -c "a b=c" pull`・`git -C 'my dir' pull` を検出した
- **`FLAG_QUOTED` の値の先頭**: 表の書き方（値の選択肢は `"…"` / `'…'` / `-`・空白・引用で始まらない語。旧 `[^-\s]` のように開きの引用符を食わない）で試作すると、`git -C "a b" pull` は**検出する**（実測）
- 2 の最初の一致（引用の内側の push / gh）: 受け入れ条件 2 の 4 形。計画の「最後の gh より前に引用・エスケープがあれば false」の試作で 4 形とも false、true を保つ 2 形は true（実測）
- 3 の指数爆発・既存の二乗: 受け入れ条件 5 と `ENV_PREFIX` から区切りを外す変更。試作で `("; A=x'").repeat(30000)+" gh x"` が 4900ms → 1ms、`('; A="x;').repeat(30000)+" gh x"` も 5883ms → 2ms。引用 41 個・`--f="a'b"'c"d'` × 24・引用値フラグ 2000 個も 1ms 以下（実測）
- 3 の OR でないと減る形（`git pull origin "x --ff-only"`）: OR の方針と受け入れ条件 3 が覆う
- 2 の DRY_RUN: 受け入れ条件 2 の回帰テストが覆う
- 4 の新しく止まる形: 受け入れ条件 4 で意図として固定。`REMEDY` の不一致: `REMEDY` の文言変更で覆う
- 5 の散文: 変更表の最終行（冒頭・各 doc・`gitSegments` の書き分け・受容する未対応リスク）でおおむね覆う

### 覆っていない所見

1. **最初の gh より後ろの gh の区切りを見ていない（要対処）**: 計画は区切りの判定を `firstCommandAt` の gh まで、引用の判定を最後の gh までで行う。次の形は計画の試作でも ALLOW のまま（今の hook でも ALLOW。`p4.mjs`・`p5.mjs` で実測）:
   - `git push && gh pr create; gh pr create`
   - `git push && gh pr create || gh pr create`
   - `git push && gh pr create & gh pr create`
   - `git push && gh pr create --fill; gh pr "create"`（2 つめは語割り。元の文字列では gh として当たらないので、引用の判定範囲（最後の gh = 1 つめ）の外に出る。**引用の根そのもの**）

   どれも push が失敗すれば 2 つめの gh が走る。issue の「`hasSafeChain` が最初の一致位置を使う」と同じ根。代案: **区切りの判定も最後の gh まで**にする（push の終端から最後の gh までの区切りがすべて `&&`）。試作でこの 4 形は false、受け入れ条件 2 の true の 2 形と `git push && gh pr create && gh pr create` は true のまま（実測）。最後の gh は全部の見え方で測る（`decide` はすべての見え方で `hasSafeChain` を要求するので、見え方の中で最後を取れば 4 つめも閉じる）
2. **テストのコメントが偽になる**: `pre-bash.test.mjs` :481「正規化は gh pr create の判定だけに通す。形の判定へ渡すと `\` パスの判定が外れる」。noVerify/pull に見え方を通すので前半が偽になる（`\` パスの判定には通さないので、後半の理由は真のまま）。変更表に無い
3. 範囲外の記述の誤り: 「`false && git push && gh pr create` が `hasSafeChain` で true になる（push が走らないのに gh が走る）」は、bash でも PowerShell 7 でも**gh は走らない**（`&&` は左結合で、`false` の失敗が伝わり gh も飛ばされる）。`hasSafeChain` が true なのは事実だが穴ではない。穴なのは `echo a || git push && gh pr create` のほう（こちらは計画どおり）
4. ⚠️ 範囲外へ足す候補（引用符を使わない「実行されない文脈」。今の hook で `hasSafeChain` が true と実測。計画の引用の判定では閉じない）:
   - `echo $(false && git push) && gh pr create`（`)` が区切りの文字クラスに無い）
   - `echo ${x:- && git push } && gh pr create`（パラメータ展開の既定値。引用に近い）
   - PowerShell のブロックコメント `echo x <# && git push #> && gh pr create`
   - `git push --help && gh pr create`（何も送らない。DRY_RUN と同じ型）

### 直し方の誤り（計画の文言のままだと起きる）

1. **`ENV_PREFIX` の値から区切りを外すと減る形がある（`ENV_PREFIX_QUOTED` の値が「選択肢 1 つ」なら）**: 試作で今は検出していて、計画では検出しなくなった形（実測）:
   - `A=a"b;c"d gh pr create`
   - `A="x;y"z gh pr create`

   シェルでは 1 つの代入で、gh は走る（本当の検出の減少）。新しい `ENV_PREFIX` は `;` で値が切れ、`;` の後ろは `c"d` / `y"z` で始まって `AT_CMD_POS` が拾い直せない（計画の「区切りの後ろは `AT_CMD_POS` が拾い直すので検出は減らない」は、区切りが引用の内側にあると偽になる）。`ENV_PREFIX_QUOTED` の値を**部品の連接**（`(?:"[^"]*"|'[^']*'|[^\s"'<区切り>])*`）にすると、2 形とも検出し、`A=x"a b" gh pr create` も新しく検出する（実測。時間は 1-2ms）。計画の表の「`"…"` / `'…'` / … / 空」は選択肢 1 つとも読めるので、連接であることと、この 2 形のテストを足す。`GIT_AT_CMD`・`PYTHON_AT_CMD` も同じ前置を使うので同じ（`A="x;y"z git pull` など）
2. ⚠️ **`FLAG_QUOTED` はフラグ名に `=` で付いた引用値を読めない**: フラグ名の部分が旧来の `--?[^-\s]\S*` のままだと、`gh --repo="a b c" pr create`・`git --git-dir="a b c" pull` は検出しない（実測）。`--git-dir="a b"`（空白 1 つ）は、旧い分割が `--git-dir="a` + 値 `b"` と読むので偶然当たる。フラグ名の部分も部品の連接（`--?(?![-\s])(?:"[^"]*"|'[^']*'|[^\s"'])+`）にすると検出し、時間も 1ms 以下（実測）。退行ではなく新しい検出の漏れなので、範囲外に置くかどうかを決める
3. ⚠️ `firstCommandAt` で gh の位置を取るのは区切りの判定には不十分（覆っていない所見 1 と同じ）。位置は「最初の gh」（push より前に gh が無いことの確認）と「最後の gh」（区切りと引用の判定範囲）の 2 つが要る

### 確かめて問題が無かったもの

- 受け入れ条件 3（`A=it's …`・`A=" git pull`）: 新しい `ENV_PREFIX` は値に引用符を許すので保たれる（試作で `A=it's gh pr create` を検出）
- 受け入れ条件 4 の止まらない側: `git log --grep="--no-verify"` は見え方で `--grep=--no-verify` になり当たらない。`echo "git pull"` は見え方でも `git` がコマンド位置に無い。`git commit -m "pull request"` は `GIT_PULL` が `git` の直後（フラグの後）の `pull` しか見ないので当たらない（コード読み）
- 既存テスト :449（`--title "a b"`）・:483（gh の後ろの `\` パス）: 引用もエスケープも最後の gh の後ろなので、計画の `hasSafeChain` でも結果は変わらない（コード読み。:449 と同形を試作で確認）
