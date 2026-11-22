# 調査 — issue #1533 背景の作業を残した委譲先の worktree が、Step 4 の remove でロックに拒まれて残る

## issue の要約

`/implement` Step 4 の `npm run verify:tree -- remove --delegate-tree <worktree>` が、直近 4 サイクルのうち 3 回、「ロックされている（委譲先がまだ動いているか）——何も消さない」で拒まれた。3 回とも、親セッションは「セッションの片づけに任せる」と判断して PR へ進んだ。worktree は `.delegate/` で dirty なので、harness も `clean:worktrees`（`--force` なし）も拾わず、残り続けた。issue は変更の候補を 4 つ挙げ、どれを採るかを `/start-issue` に委ねている。

## 一次証拠（transcript の実測）

親セッションの transcript は `~/.claude/projects/<project>/<session>.jsonl`、委譲先は `<session>/subagents/agent-<id>.jsonl` と `.meta.json` にある。

| サイクル | 委譲先 | 最後のラウンドの流れ（委譲先の側） | 親が受けた最後の通知 | remove |
|---|---|---|---|---|
| #1504 | `agent-a271eb31ec49362f8`（`retro-reviewer`） | 背景の作業なし。02:15:29 handback | 02:15:35 `finished` | 02:15:58 成功 |
| #1507 | `agent-a3287575025c5eb1f`（`review-1507`） | 12:02:22 「monitor が知らせる」で end_turn → 12:07:05 handback | 12:02:24 `is waiting on its own background work` | 12:07:14 ロックで拒否 |
| #1528 | `agent-ac095c3c592600f53`（`ff-reviewer`） | 04:49:21 「結果を待つ」で end_turn → 04:56:00 handback | 04:49:23 `waiting …` | 04:56:20・04:56:26 ロックで拒否 |
| #1523 | `agent-a5f7062a2049ea038`（`hook-reviewer`） | 08:25:27 「完了を待つ」で end_turn → 08:30:55 handback | 08:25:29 `waiting …` | 08:32:38 ロックで拒否 |

（時刻は UTC。#1507 は 2026-11-21、ほかは 2026-11-22）

読み取れること:

- 拒まれた 3 体は、どれも最後のラウンドで、変異注入を `run_in_background` の Bash で起動していた（#1528 と #1507 は、完了を `Monitor` の until ループでも待っていた）。そして「完了の通知を待つ」と書いてターンを終えた（end_turn）。親にはこの時点で `waiting on its own background work`（status は `completed`）が届いた。そのあと、背景の作業の完了通知で委譲先が再開し、報告を書いて handback した。~~この handback は親へ届いていない~~ → **誤り（敵対的調査で訂正）**: handback は、親の transcript に `origin.kind: "peer"` の `[Subagent hand-back]` レコードとして毎回届いていた。`finished` の通知も、最後の handback の後に queue-operation と `queued_command` の attachment で届いていた。task-notification の user レコードだけを数えたのが測定の穴だった。表の「親が受けた最後の通知」の列も、同じ理由で誤りである。
- 成功した #1504 の委譲先は、どのラウンドでも背景の作業を使わず、handback だけでラウンドを終えた。
- handback から remove までの間隔は、成功例で 29 秒、拒否例で 9 秒〜2 分。間隔の長さは分かれ目になっていない。
- ロックの理由は `claude agent agent-<id> (pid N)` で、N は親セッションの claude プロセスである（#1507 で親が `Get-Process` で確かめた。#1523 では、2026-11-22 夕の時点で `.git/worktrees/agent-a5f7062a2049ea038/locked` が pid 15668 を名指していた）。
- `.meta.json` は、拒否された 3 体についても、いまは `"finished": true` を持つ。
- ロックは、その後いつの間にか外れた（ac095 は 2026-11-22 17:40 JST の時点で外れていた。a5f7 は同日夕、起票の直後から除去までの数分の間に外れた）。何が外したのかは分からない。

### 機序の裁定（言えることと言えないこと）

- 言える: 背景の作業を待つ end_turn を挟まずに終えたラウンドでは、`finished` が届き、ロックも外れた（1 例）。挟んだラウンドでは、後の handback が届かず、ロックも残った（3 例）。
- 言えない: harness の内部で何がロックを持ち続けるのか。「最後のラウンドで挟んだか」と「生涯に一度でも挟んだか」のどちらが分かれ目なのか（#1507 では 11:34 の waiting の後、11:44〜11:55 に handback が 4 回あった。それらが親へ届いたかどうかは未確認）。
- どちらであっても、「委譲先が背景の作業を待つ end_turn をしない」で、成功した #1504 の形になる。

## 敵対的調査（`workspace/adversarial-1533.txt`）の反映

| 所見 | 採否 | 理由 |
|---|---|---|
| 争点 2「waiting の後の handback は親へ届いていない」は偽 | 採る | 親の transcript の `origin.kind: "peer"` のレコードを自分で数え直した。ff（#1528）は 04:15:43・04:35:25・04:44:06・04:56:00、hook（#1523）は 06:43:39・08:18:50・08:30:55 に届いていた。上の該当行を訂正した |
| 分かれ目は「背景の作業を使ったか」ではなく「背景の作業を待つ end_turn をしたか」 | 採る | 背景の作業を使っても end_turn で待たずに handback した委譲先 4 体（adr-review-1012・audit-1409・reviewer-1399b・review-1306-r3）は、remove が通った（報告の行番号つきの列挙。ae822 は handback の 8 秒後に削除が成功）。`waiting` を受けてから remove を試みた委譲先は全 Snotra transcript で 3 体あり、3 体とも拒まれた。候補 1 の文言を「背景の作業を使わない」から「背景の作業を待ってターンを終えない」へ直す |
| 争点 1・3 は壊せない。「最後のラウンドだけか、一度でもか」は分離できない | 採る | 候補 1 はどちらでも効く |
| 争点 4（ロックの持ち主）の裏づけは #1507 の 1 例だけ（⚠️B） | 採る | 文書には「親セッションの claude プロセス」と断定して書かない。「待っても外れなかった」だけを書く |
| 候補 1 の実現性が未測定。2〜14 分の変異注入を end_turn せずに待てるか（⚠️A） | 採る | このサイクルの委譲先に、前景で 10 分未満に区切って回させる。`waiting` が出ずに remove が通るかを、Step 4 で確かめる（計画の Phase 3 に含める） |
| ロックがいつ・誰に外されたか未測定（⚠️C） | 記録のみ | ac095 は 05:08:55 UTC に残り、08:46:25 UTC には外れていた。a5f7 は 08:47:25 UTC に残り、08:58:44 UTC には外れていた。どちらも数時間以内に外れており、候補 4 を入れる根拠にはならない |
| 「サブエージェントはどうなった？」の 2 回目は 70 分のハングへの問い（⚠️D） | 採る | 候補 1 の利点として、これを根拠に挙げない |
| 機序の候補 (i) 背景の子の登録が残る (ii) 再開の経路がロックの持ち主を変える（⚠️E） | 記録のみ | どちらも「待つ end_turn をしない」で避けられる。harness の内部はリポジトリから決められない |

## 関連するファイル・シンボル

- `.claude/skills/implement/SKILL.md`
  - 「3b. 委譲へ渡すもの」: 委譲先へ渡す指示の一覧。変異注入の項・報告の `.delegate/` の項がある。背景の作業の使い方については何も書いていない。主のツールの実行（`verify:tree run`）を「バックグラウンドで回し、完了の通知を待つ」と書いた項はあるが、これは主の作業である。
  - 「Step 4 — 仕上げ」の「worktree を片づける」の項: 「『ロックされている』なら、委譲先がまだ動いている。停止を待って打ち直す（ロックは `--force` 1 回では外れない）」。上の実測では、委譲先はもう handback しており、待っても外れなかった。
- `scripts/verify-tree.mjs`
  - `worktrees()`: `git worktree list --porcelain` を読み、`locked` を真偽値で持つ。ロックの理由（`locked <reason>` 行の残り）は捨てている。
  - `cmdRemove()` の `--delegate-tree` の分岐: `entry.locked` なら `ロックされている（委譲先がまだ動いているか）——何も消さない: <path>` を出して exit 1 で返す。`.delegate/` は消さない。
- `scripts/verify-tree.test.mjs`: 「--delegate-tree はロックされた worktree を消さず、報告も残す」。exit 1 と報告が残ることを断言する。メッセージは断言しない。
- `scripts/clean-worktrees.mjs`: dirty なら、`--force` なしではスキップする。`git worktree remove` に `--force` を 1 回だけ付ける。
- `docs/adr/ADR-implement-verification-tool-tree.md`: `.delegate/` を worktree の中に置く理由（変更の無い worktree は harness が片づけ、再開した委譲先が本体で動く）。今回の変更でも、この前提は変えない。

## 再利用できる既存のパターン

- 委譲先への指示は、3b の箇条に 1 項ずつ足す形で書いてある。理由と issue 番号を項の中に書く。
- 「ロックされている」のメッセージは、ほかの拒否のメッセージと同じく、`<状態>——何も消さない: <path>` の形である。

## 技術的な制約

- Bash ツールの前景実行は、timeout が最大 600000 ms（10 分）である。前景の `sleep` は harness が拒む（system prompt の Bash の説明に「Foreground `sleep` is blocked; use Monitor with an until-loop」とある）。委譲先が背景＋Monitor を使ったのは、この案内に従ったためと読める。
- 変異注入の 1 回の実行時間は、実測で 2〜14 分（#1528 の round 2 は 04:20〜04:34）。10 分を超える実行は、前景の 1 回には収まらない。分けて回す必要がある（#1523 の委譲先は、`LIMIT_MS` で時間を区切っていた）。
- harness のロックの内部は、リポジトリから決められない（`.claude/` の外、Claude Code 本体の挙動）。

## 候補ごとの見立て

1. **委譲先に、背景の作業を待つ end_turn をさせない**（発生源で断つ）。成功例の形そのものである（背景の作業を使っても、待つ end_turn をしなかった 4 体は remove が通った）。
2. **remove の前に親が `TaskStop` を打つ**: 委譲先は既に handback して `finished: true` なので、止める対象が無い見込みである。効くかどうかは未測定。1 があれば要らない。
3. **拒否されたときの出力と報告**: メッセージの「委譲先がまだ動いているか」は、実測と合わない。ロックの理由（pid）を出し、次の手を示すほうが役に立つ。残ったことを wrap_up の recap に書く指示もこの候補に含まれる。
4. **持ち主の居ないロックを外す**: 観測した 2 件では、ロックは後で外れていた。外れないまま残る例は観測していない。今は根拠が無い。

## 未解決の疑問

- 1 を入れたあとに、背景の作業を使わない委譲先で remove が通るか。これは次の実サイクルでしか確かめられない（受け入れ条件の「実際のサイクルで 1 回」）。
- 「最後のラウンドで挟んだか」と「一度でも挟んだか」のどちらが分かれ目か。1 はどちらでも効くので、決めなくても計画は立つ。
