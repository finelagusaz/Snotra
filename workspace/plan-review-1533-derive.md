# #1533 の変更範囲の独立導出

対象: worktree `issue-1504-organization-035b58`（branch `chore/delegate-tree-lock`、HEAD edb12a26 時点）。
読んだもの: `gh issue view 1533`（本文とコメント）、`.claude/skills/implement/SKILL.md`、`scripts/verify-tree.mjs`、`scripts/verify-tree.test.mjs`（remove 節）、`scripts/clean-worktrees.mjs`、`docs/adr/ADR-implement-verification-tool-tree.md`、`git grep`（`delegate-tree|verify:tree|clean:worktrees` と `ロックされ|worktree lock|run_in_background|Monitor|TaskStop|recap`）。

## 1. 変えるファイルとシンボル

### 1-1. `.claude/skills/implement/SKILL.md`「3b. 委譲へ渡すもの」の渡す項目の列挙（発生源を断つ・候補 1）
- 何を: 委譲先へ渡す項目を 1 つ足す。「背景の作業（Bash の `run_in_background`・`Monitor`）の完了を、ターンを終えて待たない。待つものは前景で待つ（Bash の foreground と `timeout`）。最後の報告を返す時点で、走っている背景の作業を残さない」。理由も添える（ターンを終えて待った委譲先は、報告を返したあとも worktree のロックが外れず、Step 4 の remove が拒まれた・#1533）。
- 根拠: 調査の事実（ターンを終えて待った委譲先だけがロックを残し、背景の作業を使ってもターンを終えなかった委譲先は remove が通った）。挿入先は SKILL.md:113-124 の箇条（とくに変異注入の項 :119 の近く。拒まれた 3 体はどれも変異注入を背景で回していた——issue 本文「実測した機序」）。
- 委譲先は 3c で `SendMessage` により再開されるが、指示は最初の委譲で渡したコンテキストに残るので、ラウンドごとの再掲は要らない（SKILL.md:137）。

### 1-2. `scripts/verify-tree.mjs` の `cmdRemove`（`--delegate-tree` のロック分岐 :394-398）と `worktrees()`（:145-154）（候補 3 の出力）
- `worktrees()`: porcelain の `locked <理由>` 行の理由も返す（例: `lockReason`）。今は `locked: /^locked\b/m.test(e)` の真偽だけ（:152）。
- `cmdRemove` のロック分岐: メッセージ「ロックされている（委譲先がまだ動いているか）——何も消さない」（:396）を、実際の機序に合う形へ直す。出すもの:
  - ロックの理由（`claude agent agent-<id> (pid N)` をそのまま）
  - 何が起きているか: 委譲先が背景の作業を待ってターンを終えると、報告を返したあともロックが残る（持ち主は親セッションの pid）。数時間で外れた例しか観測していない
  - 次の手: このサイクルでは消さずに先へ進み、残った worktree のパスを `/implement`「出力」へ書く／ロックが外れてから同じ `remove --delegate-tree` を打ち直す（`.delegate/` は残してあるので打ち直せる）
- 終了コードは 1 のまま（「消せなかった」。2 にしない——:13-14 の契約）。`.delegate/` を消す前に止める順序（:394 のコメント）も保つ。
- pid が「親セッション自身か」の判定は入れない（2 を参照）。

### 1-3. `.claude/skills/implement/SKILL.md`「Step 4 — 仕上げ」の worktree 片づけの項（:155）
- 「『ロックされている』なら、委譲先がまだ動いている。停止を待って打ち直す（ロックは `--force` 1 回では外れない）」を直す。今の文は、仕事を終えた委譲先には当てはまらず、待っても止まらない（issue「手順の側の食い違い」）。
- 新しい中身: ロックで拒まれたら、出力が示すロックの理由を読む。委譲先が報告を返し終えているなら、待たずに先へ進む。残った worktree のパスを「出力」の欄（1-4）に書く。後で打ち直すのは同じ `remove --delegate-tree`。`(ロックは --force 1 回では外れない)` は真のまま残してよい（`clean-worktrees.mjs` は `--force` を 1 回しか付けない・`clean-worktrees.mjs` の `worktree remove ...(force ? ["--force"] : [])`）。
- 1-1 の指示が守られていればここへは来ない、という位置づけを一言添える。

### 1-4. `.claude/skills/implement/SKILL.md`「出力」の報告テンプレート（:177-204）と、その下の注記（:206-211）（候補 3 の報告）
- テンプレートに worktree の片づけの欄を足す。例: `- 片づけ: 委譲先の worktree — 消えた | 残った（<パス>・<remove の出力の理由>）`（Step 4 に到達しなければ「未到達」）。置き場は「停止」の下か「PR」の前。
- 注記に 1 行: 残ったときは wrap_up の recap にも書く（issue 本文: 3 回とも recap に書かれなかった）。受け入れ条件「ロックで消せなかったときは、そのことがセッションの終わりの報告に出る」の担い手はこの欄である。今のテンプレートには worktree を書く欄が無い（:177-204 を通読）。

### 1-5. `scripts/verify-tree.test.mjs`「verify-tree remove」の `it("--delegate-tree はロックされた worktree を消さず、報告も残す")`（:681-692）
- `git worktree lock --reason "claude agent agent-abc (pid 12345)"` で理由つきにし、stderr の中身まで断言する（5 を参照）。今は exit 1 と `.delegate/round1.txt` の残存だけを見る。`.claude/rules/safety-nets.md`「検出器のカバー範囲は、欠落のパターンごとに検算する」（同じ exit 1 になる理由が複数ある関数では理由まで assert する）。

## 2. 迷ったが変えないもの

- `scripts/clean-worktrees.mjs`: 変えない。候補 4 を採らないため（3 を参照）。`--force` を 1 回しか付けないことは、SKILL.md:155 の「ロックは `--force` 1 回では外れない」と整合したまま。
- `docs/build-commands.md:238`（`clean:worktrees` の 1 行説明「dirty はスキップ、-- --force で強制」）: ロックに触れていないので偽にならない。
- `docs/adr/ADR-implement-verification-tool-tree.md`: 「受容した残余」にロックの話は無く、偽になる記述が無い（ADR は凍結された歴史でもある）。候補 2・4 の却下を新しい ADR にするかは、ADR の条件（`AGENTS.md`「ドキュメント参照」の意思決定記録の行）の (2)(3) を満たすかで決める。候補 4 の却下は「自然に外れた」観測に基づくが、試し直す費用は安く、PR 本文で足りると判断する。
- `scripts/verify-tree.mjs` の `discardBranchArtifacts` の doc（:341「dirty・ロック・cwd を握るプロセス」）: 対象はツールの worktree で、記述は真のまま。
- `.claude/skills/implement/SKILL.md:108`（主がツールを「バックグラウンドで回し、完了の通知を待つ」）: これは主エージェントの話で、主がターンを終えて待つのは委譲先のロックと関係しない。1-1 の指示と字面がぶつかって見えるので、1-1 の文に「委譲先が」と主語を明示する程度に留め、:108 は変えない。
- ロックの pid が親セッション自身かを判定するロジック（候補 3 の前半）: 入れない。`npm run` → node の祖先をたどって claude プロセスを特定する必要があり、Windows のプロセス木の走査を足すことになる。調査の事実ではロックは常に親セッションの pid で（issue「実測した機序」）、理由の文字列をそのまま出せば人が読める。判定を足しても次の手は変わらない。
- `.claude/hooks/pre-bash.mjs`（PR 前ゲート）でロック残りを止める: 入れない。ロックは数時間で外れ、残っても害は disk だけで、PR を止める理由にならない。受け入れ条件は「報告に出る」までである。
- ルート `CLAUDE.md`「サブエージェント委譲と worktree」: 委譲一般の運用で、`/implement` の 1 体へ渡す指示は SKILL が正本。写しを増やさない（規範文書を触るとマージに合意が要るという費用もある）。

## 3. issue の 4 候補の採否

1. 委譲先への指示（背景の作業を止めてから最後の報告を返す）: **採る**（1-1）。ただし文言は事実に合わせて「ターンを終えて待たない」を核にする。事実は「背景の作業を使ったか」ではなく「待つためにターンを終えたか」で分かれた（使ってもターンを終えなかった委譲先は remove が通った）。「止めてから報告」だけだと、ターンを終えて待ったあとに止めても足りるかは測っていない（ターンを終えた時点でロックが外れなくなった可能性がある）。発生源で断つ唯一の候補。
2. 親の `TaskStop`: **採らない**。ロックが外れるかは未測定（issue 本文が明記）。報告は毎回親へ届いているので、止める必要があるのは委譲先の仕事ではなくロックだけで、外れる保証の無い操作を手順に入れると「打ったのに残った」という新しい分岐が増える。候補 1 で発生源を断てば要らない。採るなら先に測る、を PR 本文に残す。
3. 拒まれたときの出力と報告: **採る**（1-2・1-3・1-4）。受け入れ条件「ロックで消せなかったときは、そのことがセッションの終わりの報告に出る」と「SKILL.md の『ロックされている』の行が、実際の機序と合っている」を直接満たすのはこれだけ。候補 1 は指示文で、守られない回が残る（ADR の「委譲先の `align` を打つのは指示文である」と同型）ので、その回を沈黙させない受け皿として要る。ただし「pid が親自身なら」の判定は入れず、理由の文字列をそのまま出す（2 を参照）。
4. 持ち主の居ないロックを外す（`clean:worktrees -- --force` の拡張／手順に `git worktree unlock`）: **採らない**。ロックは数時間で外れ、外れないまま残った例は観測されていない（与えられた事実・issue のコメント「先に測ってから決める」）。`--force --force` や `unlock` は、本当に動いている他のセッションの委譲先を消しうる（`clean-worktrees.mjs` の `--force` は dirty な `agent-*` をすべて対象にする）。守るものが無いのに破壊的な経路を足すことになる。

## 4. 変更で偽になる散文

- `.claude/skills/implement/SKILL.md:155`「「ロックされている」なら、委譲先がまだ動いている。停止を待って打ち直す」——仕事を終えた委譲先にも出るので偽。
- `scripts/verify-tree.mjs:396` のメッセージ「ロックされている（委譲先がまだ動いているか）」——同上（出力の文言だが読み手の行動を決める散文）。
- `.claude/skills/implement/SKILL.md`「出力」テンプレート（:177-204）——偽ではないが、受け入れ条件に対して欠けている（worktree を書く欄が無い）。
- 変えずに真のまま残るもの（確認済み）: `scripts/verify-tree.mjs:394` のコメント（`.delegate/` を先に消さない理由）、:341 の `discardBranchArtifacts` の doc、`verify-tree.test.mjs:681` のテスト名、SKILL.md:155 の「ロックは `--force` 1 回では外れない」、`docs/build-commands.md:238`、ADR 全文。
- 1-1 を足すと、SKILL.md:108（主が背景で回す）と並ぶ。主語の取り違えを防ぐ書き方にする（偽にはならない）。

## 5. テストで断言すること

`scripts/verify-tree.test.mjs`「verify-tree remove」:
- 理由つきでロックした委譲先（`git worktree lock --reason "claude agent agent-abc (pid 12345)"`）で `remove --delegate-tree`:
  - exit が 1（2 ではない）
  - stderr にロックの理由の文字列（`claude agent agent-abc (pid 12345)`）がそのまま出る
  - stderr に次の手（打ち直し・「出力」へ書く）が出る。断言は文言全体ではなく鍵になる語（例: `remove --delegate-tree` の打ち直し）に絞る
  - stderr に旧文言「まだ動いている」が出ない（旧メッセージへ戻す変異を落とすため）
  - `.delegate/round1.txt` が残る・worktree が残る・`worktree-agent-abc` ブランチが残る（今のテストは `.delegate` の残存だけ。ブランチは `rmSync` より後で消すので、順序を入れ替える変異を落とすには worktree の残存も見る）
- 理由なしのロック（`git worktree lock` のみ）でも exit 1 で止まり、理由欄が空でも落ちない（porcelain は理由なしだと `locked` だけの行になる）。
- 既存の「ロックされていない `.delegate/` だけの worktree は消える」テスト（:632）は緑のまま（`worktrees()` の解析変更が `locked` の判定を壊していないこと）。
- SKILL の指示文（1-1・1-3・1-4）は検知器ではないので変異注入の対象外。受け入れ条件「実際のサイクルで 1 回確かめる」は、背景で変異注入を回す委譲先がいるサイクルで remove が通ることを PR 本文のチェックリストで実測する。

## 6. 計画を目にしたか

見ていない。`workspace/plan.md`・`workspace/research.md` は開いておらず、grep・Glob・`git grep` は `workspace` を除外して打った。
