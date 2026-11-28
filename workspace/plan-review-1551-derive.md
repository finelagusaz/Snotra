# #1551 独立導出: pre-bash.mjs に 3 判定を足すとき、変えるべきもの

前提: `workspace/` の下は本ファイル以外読んでいない。計画の内容は目にしていない。行番号は HEAD `0c253c3c` の作業ツリーで測った。

検索範囲の逸脱の申告: `git grep` を 1 回、指定外の pathspec（`'*.md'`・`.claude/agents`・`.githooks`）も含めて打った（`':!workspace'` で除外し、`workspace/` は untracked なので git grep の対象外でもある）。計画の内容は出力に現れていない。ほかに `scripts/race-boundaries.test.mjs`・`scripts/merge-pr.mjs` を `scripts` の下として読んだ。

## (a) コードとテストで足す・変えるもの

### `.claude/hooks/pre-bash.mjs`

1. **新しい述語（純関数・I/O なし）** — 既存の `usesNoVerify`(384)・`pullWithoutFfOnly`(391) と同じ型で足す。
   - `discardsUncommitted(command)`（名は仮）: `gitSegmentsInBothViews`(379) の各セグメントに対し、
     - `git rm` かつ `-f`/`--force`/結合短縮形（`-rf`・`-fr`・`-qf`）。短縮形は `SHORT_FLAG_N`(365) と同型の `-[A-Za-z]*f[A-Za-z]*` を **rm セグメントに限って**使う（commit の `-n` を commit に限った理由と同じ）
     - `git checkout` で、引数に単独の `--` がある、または単独の `.` がある
     - `git checkout-index`（引数を問わない）
     - `git restore` で、`--staged`/`-S` だけで `--worktree`/`-W` を伴わない形以外
   - subcommand の判定は `GIT_COMMIT_FORMS`(369)・`GIT_PULL_FORMS`(370) と同じく `[FLAG, FLAG_QUOTED].map(flag => new RegExp(`^git\\s+${flag}<sub>\\b`))` で作る（`git -C x rm -f`・`git -c a=b checkout -- .` を読むため）。
   - `ghPrMergeSites`/`usesGhPrMerge(command)`: `GH_PR_CREATE`(122)・`GH_PR_CREATE_QUOTED`(125)・`GH_PR_CREATE_FORMS`(126)・`ghPrCreateSites`(153) と同型で `merge\b` を探す。元の文字列と `unquotedView`(142) の両方で探す。
   - `removesWorkspace(command)`（名は仮）: rm セグメントの pathspec に `workspace` / `workspace/` / `workspace/…` / `./workspace…` が含まれるか。`workspaceX` は当てない（`verifiedSummaryState` の `startsWith("workspace/")`(673) と同じ境界の考え方）。
2. **`SHAPE_REMEDY`(401)** に理由を足す: `discard`（「WIP コミットを作る、ステージだけ取り消すなら `git restore --staged` / `git reset`」）、`ghPrMerge`（「`/merge-pr`（`npm run merge:pr`）を使う。手打ち・`--auto` は使わない」）。
3. **`judgeCommandShape`(438)** の platform ゲート(443)より前に、`discard` と `ghPrMerge` の 2 判定を足す（Windows 固有ではない）。
4. **`decide`(461)** — `git rm` × plan の判定は I/O（plan.md の読み）が要るので `judgeCommandShape` には入れず、`shape` の後(472) と `ghPrCreateSites`(474) の間に置く:
   - `removesWorkspace(command)` が真なら `readPlanState` が関数であることを確かめ（無ければ配線欠陥として block・既存 486-488 と同じ文言の型）、`plan.ok` が偽なら block、`exists && unchecked > 0` なら件数と次の一手を持つ理由で block。そうでなければ以降の判定へ進む（`git rm -r workspace && gh pr create` のような連結を考えると、ここで ALLOW を返さない）。
   - 既存の plan 文言(494-495) は「PR を作成してください」で終わるので、そのまま流用すると次の一手が違う。git rm 用の文言を別に持つ（件数は `plan.unchecked`）。
5. **`main()`(691)** は変えなくてよい見込み。`readPlanState: () => readPlanState(cwd)`(719) は既に全コマンドに対して遅延で配線されている。`verify-tree.mjs` の動的 import(709-715) は `gh pr create` 検出時だけのままでよい（新判定は verify-tree を要さない）。
6. **冒頭コメント・doc の更新**（偽になる散文。(b) にも再掲）
   - 1-6 行: 「(3) コマンドの形だけで判定できる規範 5 件」— 件数が偽になる。`governance-docs.md` の「数え上げを写さない」に従い、件数を書かない形へ直す。「(1)(2)(4) は `gh pr create` 検出後のみ」— plan.md の読み（(2)）が `git rm` の経路でも走るようになるので偽になる。
   - 14-19 行「判定単位」: コマンド位置で判定するものに `gh pr merge` と `git rm`/`checkout`/`restore` が加わる。`unquotedView` で見る判定の列挙（`ghPrCreateSites`・`usesNoVerify`・`pullWithoutFfOnly`）も増える。列挙をやめて帰結だけにするのがよい。
   - 40-54 行「受容する未対応リスク」: `gh pr create` 専用の書き方。`gh pr merge` にも同じ残余（`sh -c`・ラッパ・`gh.exe`・`gh api …/merge`）が当たる。
   - `judgeCommandShape` の doc(426-437): 「I/O を要さないので git / plan.md を読む前に走る」は真のまま。
   - `decide` の doc(450-460): 「管轄外 … allow（対象ツール以外・`gh pr create` 以外）」「reader は 3 つとも必須で、`gh pr create` の経路で渡されていなければ block する」— `git rm` の経路でも `readPlanState` が必須になるので不完全になる。
   - 257-264 行のセクション見出しコメント「コマンドの形だけで判定できる規範（#768）」: `#1551` を添える程度。
   - 475 行 `// 管轄外` は `gh pr create` も `git rm workspace` も無いときの意味になる。

### `.claude/hooks/pre-bash.test.mjs`

- import(9-35) に新しい export を足す。
- **理由ごとに、その理由だけに当たる入力で、理由の文言まで assert する**（`.claude/rules/safety-nets.md`「検出器のカバー範囲は、欠落のパターンごとに検算する」・#1319）。既存の型は「止めた理由まで確かめる（#1523）」(1524-1540) の `it.each([cmd, needle])`。
  - discard: `git rm -f x`・`git rm --force x`・`git rm -rf src/x`・`git checkout -- a.rs`・`git checkout .`・`git checkout-index -a -f`・`git restore a.rs`・`git restore --staged --worktree a.rs`・`git restore -W a.rs`。`workspace` を含まないパスにして rule 1 と重ねない。
  - gh pr merge: `gh pr merge 1551 --squash`・`GH_TOKEN=x gh pr merge`・`gh --repo o/r pr merge`・`gh pr "merge"`。
  - git rm × plan: `git rm -r workspace/`・`git rm -r workspace`・`git rm workspace/plan.md`・`git rm -r ./workspace` を `PLAN_OPEN(3)`(54) で block し、文言に件数「3 件」と次の一手を assert。同じ入力を `PLAN_DONE`(52)・`NO_PLAN`(50) で allow、`PLAN_UNREADABLE`(56) で block（理由の文言を assert）、`readPlanState` を渡さないと配線欠陥で block。
- **通すべき形**（過剰検出の境界・要件の「止めない」）: `git checkout main`・`git checkout -b feat origin/main`・`git checkout -`（直前のブランチ。`-` 単独を `--` と取り違えない）・`git checkout --orphan x`・`git restore --staged a.rs`・`git restore -S a.rs`⚠️・`git restore --staged -- .`・`git rm -r src/x`（plan 未完でも workspace 外なら allow）・`git rm -r workspaceX`・`git rm --cached x`・`npm run merge:pr -- 1551 --closes none --body-file p`・`npm run merge:pr '--' …`（PowerShell 形・`/merge-pr`「打つ」節）・`grep -n "gh pr merge" x`・`rg "git checkout --" docs`。
- 既存の表を更新:
  - 1005-1007 のコメント「#768 コマンドの形で判定する規範 5 件」と describe「5 述語は全域関数である」(1347) — 件数が偽になる。`PREDICATES`(1348-1354) に新述語を足し、`PATHOLOGICAL`(1355-1360) に `"git rm "`・`"git checkout -- ".repeat(5000)`・`"gh pr merge; ".repeat(5000)` 等を足す。
  - `ALL_REASONS`(1277-1283) に新しい理由を足す（`$env:` パスの区切り・ドライブレター絶対パスの検査を新文言にも当てる）。
  - 「拒否文言は代わりの手段を含む」(1254-1266) に新文言の needle を足す。
  - 「無害なコマンドは null」(1268-1271) に `git restore --staged x`・`npm run merge:pr …` を足すとよい。
  - 時間の検査（1376-1383・1585 以降の `within`）に、新述語の病的入力を足す。
- 既存テストで結果が反転するものは見つからなかった（`pr merge|pr view|checkout|restore|git rm|管轄外|toBe("allow")` で grep して確かめた）。ただし次は名前・注記が偽になるか、新判定にも当てるべきもの:
  - 520-525「gh pr create を検出しなければ plan.md を読まない」— `git rm -r workspace` では読むようになるので、名前が偽になる（入力の `echo hi` では通り続ける）。「gh pr create も workspace の git rm も無ければ読まない」へ直す。
  - 155-162「別サブコマンドは検出しない」（`gh pr list`・`gh pr view 1`）— `GH_PR_CREATE` 単体の検査。新しい `gh pr merge` の検出にも同じ負例（`gh pr view 1`・`gh pr list --search merge`）を置く。
  - 1202 `git checkout main && git pull --ff-only` — ブランチ切り替えなので、discard の判定でも通り続けること（`judgeCommandShape` 全体で null になることの検査に使える）。
  - 1236-1250 `PLATFORM_FREE` 表 — discard と gh pr merge の代表入力を足し、win32・darwin・省略の 3 つで block を固定する。
  - 325-347・360-389・452・514-553 の allow の検査はいずれも `echo hi`・`gh pr create` 系で、新判定に当たらない。
- `decide` の配線テスト: 「形の判定は plan.md を読む前に決まる」(1335-1339) は `git pull` なので真のまま。新しく「`git rm -r workspace` 以外では plan.md を読まない」（`vi.fn(PLAN_OPEN(3))` が `git status` などで呼ばれない）を足すと、全コマンドで plan.md を読む退行を捕まえられる。
- プロセス級 e2e（`runHook`(809)・818 以降）: 実ファイルの `workspace/plan.md`（889-896・912-925 の型）を置いた一時リポジトリで、`git rm -r workspace/` が exit 2 で stderr に件数を含むことを 1 本。`main()` の `readPlanState` 配線が git rm の経路でも効くことを固定する。

### その他のコード

- `scripts/merge-pr.mjs:33` が `verifiedSummaryState` を import している。新しい top-level の定数・正規表現が import 時に throw すると `npm run merge:pr` まで壊れる（爆発半径）。変更不要だが不変条件として守る。`merge-pr.mjs` は `gh pr merge` を `spawnSync`（252・367 付近）で呼ぶので、hook の視界に入らない＝新判定に止められない。
- `scripts/race-boundaries.test.mjs:224` には `emitBlock` の行（687）の写しがあるが、`run()` へ渡す fixture の文字列であって `pre-bash.mjs` を読んでいない。`emitBlock` を変えても落ちない（fixture の出所というだけ）。
- 他の機構の拒否・復帰文言が、新しく止まるコマンドを勧めていないか: `scripts`・`.claude/hooks` の非テスト `.mjs` を `git (restore|checkout|rm)|gh pr merge|checkout-index` で grep した。当たりは `scripts/merge-pr.mjs:20,38,373,378` だけで、どれも「ツールが内部で `gh pr merge` を呼んだ結果」の説明であり、手で打てとは勧めていない。⚠️ `.githooks/` の拒否文言は検索範囲外なので未確認（`git restore`・`git checkout -- ` を復帰手順に書いていれば、拒否→復帰が新判定に止められる非対称になる）。
- `.claude/settings.json` は変更不要（matcher `Bash|PowerShell`・timeout 30）。新判定は git を呼ばないので、`git 呼び出しの上限の和`(1421-1424) の係数は変わらない。

## (b) 偽になる・直すべき散文

- `.claude/skills/merge-pr/SKILL.md:65`「hook は `gh pr merge` を見ない」— **偽になる**。必ず直す。
- `.claude/skills/implement/SKILL.md:42`（「散文＋証跡」の「マージしない」行・「無し（候補: `gh pr merge` を permission の deny で止める）」）— 候補が機構になる。ただし止まるのは手打ちの `gh pr merge` だけで、`npm run merge:pr` は通る（要件）ので、「マージしない」全体は機構で止まらない⚠️。行を「機構で止める」へ移すのではなく、証跡欄・候補の記述を直すのが正しい見込み。
- `.claude/skills/implement/SKILL.md:63`（「散文のみ」表の唯一の行・「候補: pre-bash の形の判定に足す」）— 機構になる。「機構で止める」表(53-57) へ移す。「散文のみ」節(59-63) が空になるので、節ごと消すか、20-30 行の 4 分類の宣言（「項目は、守らせる手段で次の 4 つに分ける」）とあわせて直す。この節は「変えるときは、ユーザーと合意する」と書いてある（SKILL.md 冒頭「守ること」）。
- `.claude/skills/implement/SKILL.md:49`「PR 前ゲート…（`plan.md` の未チェック項目を数える。`plan.md` を消すと外れる）」— `git rm` で消す経路は止まるようになる。シェルの `rm -r`・`Remove-Item` では今も外れる（要件の範囲外）ので、「`git rm` 以外で消すと外れる」程度へ直す⚠️。
- `.claude/skills/implement/SKILL.md:199`「未チェックを残したまま削除すると、PR 前ゲート（PreToolUse hook）が計画を見られなくなる」— 真のまま。hook が `git rm` を止めることを添えるかは任意（写しを増やさないなら書かない）。
- `.claude/skills/implement/SKILL.md:201-205`（`git rm` が止まったときの 4 項目）— 真のまま。hook の拒否文言と内容が重なるので、どちらを正本にするか決める（`governance-docs.md` (1) かぶりなく書く）。
- ルート `CLAUDE.md:16`「`gh pr merge` を手で打たない」— 真のまま。hook が止める旨を添えるなら規範文書の変更になり、ルート `CLAUDE.md`「最重要ルール」2 によりマージ前にユーザー合意が要る。
- `.claude/hooks/pre-bash.mjs` 冒頭・`decide`/`judgeCommandShape` の doc（(a) 6 を参照）。
- `.claude/hooks/pre-bash.test.mjs:1006`・`1347` の「5 件」「5 述語」。
- `docs/adr/ADR-squash-merge-issue-autoclose.md:43`「hook で `gh pr merge` を検出して照合する: 却下」— 凍結した歴史なので直さない。今回は「照合」ではなく「止めて `/merge-pr` へ送る」ので矛盾はしない。同行の「ツールを経ないマージは `--no-verify` と同格の人間専用の迂回とした」は、hook が `--no-verify` を止めているのと同じ扱いになるので、整合する。PR 本文でこの関係に触れるとよい⚠️。
- 確認したが変更不要: `docs/build-commands.md:11,241,246`（`merge:pr` の使い方）・`.claude/skills/start-issue/SKILL.md:39`・`.claude/skills/deps-update/SKILL.md:68`・ルート `CLAUDE.md:20`（正本は hook とテスト、と指すだけ）・`.claude/rules/`（該当語なし）・`docs/development-principles.md`・`docs/architecture.md`（hook の判定を列挙していない）。

## (c) 守る不変条件と境界

- **全域性**（257-264 の注釈）: 形の判定は全コマンドで走る。throw すると `main()` の catch が exit 2 を書いてセッションの全コマンドが止まり、遅いと hook の timeout（exit≠2＝非ブロッキング）で fail-open になる。新しい正規表現は `FLAG`(91)/`FLAG_QUOTED`(117)/`ENV_PREFIX`(101)/`ENV_PREFIX_QUOTED`(112) を組み合わせるだけにし、排他でない選択肢を `*` の下に入れない（91 行・109-112 行の指数爆発の実測）。セグメント（`segmentEnd`(199)）に閉じて判定する。
- **`decide` の fail-closed**: `git rm` × plan で `readPlanState` が無い・`ok: false` なら block。plan.md が無いなら管轄外として先へ進める（`readPlanState` 566 行の意味づけ）。plan.md の読みは `removesWorkspace` が真のときだけ行う（全コマンドで I/O を足さない）。
- **順序**: `judgeCommandShape`（純）→ `git rm` × plan（I/O）→ `gh pr create` の経路。`git rm -rf workspace/` は discard が先に止める。テストで「その理由だけに当たる入力」を作るときは、この順序を前提にする（rule 1 の入力は `-f` を含めない、rule 2 の入力は `workspace` を含めない）。
- **2 つの見え方**: `gitSegmentsInBothViews` を使う。見え方だけで判定すると、引用の内側の言及で止まる過剰検出がある（378 行の doc）。`git commit -m "x; git restore y"` は見え方で止まる。fail-closed 方向として受容し、テストで意図を固定する（1507-1522 の型）。一方、元の文字列だけだと `git "restore" x`・`gh pr "merge"` を通す。
- **止めないもの**（要件）: ブランチ切り替え（`git checkout <branch>`・`-b`・`-`・`--orphan`・`--detach`）、`git restore --staged`、`npm run merge:pr`。`git checkout -`（単独のダッシュ）と `--` を取り違えない。`checkout\b` は `checkout-index` にも当たる（`-` が語境界）ので、`checkout` と `checkout-index` の判定を書き分けるときに注意する。
- **復帰手順が適用できる拒否**（`.claude/rules/safety-nets.md`「拒否する機構の誤爆は一様でない」）: discard の文言が勧める `git restore --staged`・`git reset`・WIP コミットは、どれも新判定にも既存判定にも止められないこと（WIP コミットに `--no-verify` を勧めない）。gh pr merge の文言が勧める `npm run merge:pr` は通ること。PowerShell の `npm run merge:pr '--' …` も通ること。拒否文言に `\` パス・`$env:X\` を書かない（`ALL_REASONS` の検査）。
- **`cwd` の決め方**: plan.md は `payload.cwd` から `findUp(".git")`（564）で求める。harness の worktree では `.git` がファイルだが `existsSync` で見つかる。
- **hook の版**: 59-61 行のとおり、hook は本体の作業ツリーの版で走る。この PR の新判定は worktree のセッションでは効かず、自分で e2e を確かめられない。確認は `runHook` のプロセス級テストで行い、PR 本文にその旨を書く。
- **他スクリプトとの結合**: `scripts/merge-pr.mjs:33` の import（import 時に throw しない）。`scripts/race-boundaries.test.mjs:224` は fixture の文字列なので結合ではない。
- **セーフティネットの手続き**: `.claude/hooks/**`・`.claude/skills/**` は `.claude/rules/safety-nets.md` の `paths` に当たる。PR 本文の冒頭に「マージは合意してから」（ルート `CLAUDE.md`「最重要ルール」2）。新判定ごとに、判定を消す変異でテストが赤になること（理由の文言で赤になること）を確かめる。ガバナンス文書を触るので `npm run governance:check`。

## (d) 確信の持てない点 ⚠️

- ⚠️ `git rm --cached -r workspace/`: 作業ツリーの plan.md は残るので PR 前ゲートは見え続ける。止めるか通すか。要件の「removes `workspace/`」は index からの削除も含むとも読める。
- ⚠️ `git rm -r .`・`git rm -r '*'`・`git rm -r :/`・絶対パス（`C:/…/workspace`）・`../workspace`（サブディレクトリの cwd から）・`git -C <other> rm -r workspace`: 暗に workspace を含む・別ツリーを指す形。pathspec を解決するには git を呼ぶか cwd を解釈する必要があり、全コマンドで走る判定の費用と衝突する。受容する残余として冒頭コメントへ書くのが妥当と見るが、決めるのは計画側。
- ⚠️ `git restore -S`（`--staged` の短縮形）を `--staged`-only と同じく通すか。要件は `--staged` だけを名指しする。通さないと、文言が勧める操作の短縮形で止まる。
- ⚠️ `git checkout -f`/`--force`（ブランチ切り替えで変更を捨てる）・`git checkout <tree-ish> <path>`（`--` なし）・`git checkout ./src`・`git switch --discard-changes`/`-f`・`git stash drop`: 要件に無い破棄の形。前の 2 つは `--` なしだとブランチ切り替えと区別できない。範囲外として残余に書くか。
- ⚠️ `git rm -- -f`（`-f` という名のファイル）・`git checkout -b x -- `（実在しにくい）: `--` の後ろのフラグ風の語を止める過剰検出。受容でよいと見る。
- ⚠️ `implement/SKILL.md` の「守ること」表の移し替え先。「マージしない」は `npm run merge:pr` が通る以上、機構で止まるのは迂回の形だけ。表の分類をどう書くかは利用者の合意事項（SKILL.md「この節を変えるときは、ユーザーと合意する」）。
- ⚠️ git rm × plan の判定を `judgeCommandShape` の外（`decide`）に置く前提で書いた。`judgeCommandShape` に plan の読みを注入する設計もありうるが、「I/O を要さない」という同関数の doc(426) と platform を値で受ける理由(428-430) を崩すので、私は採らない。
- ⚠️ discard と gh pr merge の判定を `platform` に依らず走らせる前提（`judgeCommandShape` の 443 行より前）。Windows 固有の失敗様態ではないので。
