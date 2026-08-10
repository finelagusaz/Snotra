---
name: merge-pr
description: "PR を squash マージするときに使用。issue auto-close の誤爆を防ぐ、マージ直前の closingIssuesReferences 確認・本文編集・マージ後 3 点検証の手順。"
disable-model-invocation: true
argument-hint: "[PR 番号]"
allowed-tools:
  - Bash(gh *)
  - Read
  - Write
---

PR を squash マージする。マージで閉じる issue を決めるのは PR 本文であり、`gh pr merge` の `--subject` / `--body-file` では抑止できない（#488 実測）。auto-close は本文の**どこにあっても** `close`/`fix`/`resolve` 系 9 形（大文字小文字問わず・表やチェックリスト内も）でマージ時に走り、PR テンプレートが `Closes` を埋めるため**書いた覚えが無くても残る**。hook も見ていない（ルート `CLAUDE.md`「フック」の (A2)）。**だから下の手順が唯一の防御である**。なぜこの機構になるか（2 経路の可視性の非対称・マージ方式では逃げられない・squash 設定と復元レシピ)は `docs/adr/ADR-squash-merge-issue-autoclose.md`。

対象: $ARGUMENTS

手順（squash マージでは常にこの順。`<PR>` は PR 番号、`<issue>` は issue 番号）:

1. **マージ直前に** `gh pr view <PR> --json closingIssuesReferences` を**必ず**見る。これが GitHub の計算した「いま閉じる issue」である
2. 一覧に閉じたくない issue があれば **PR 本文を編集して手順 1 を実行し直す**（`gh pr edit <PR> --body-file <tmp>`）。**一覧から消えるまで繰り返す。** どの行のどの語が効いたかを推測しない — 認識されるのは `close/closes/closed` `fix/fixes/fixed` `resolve/resolves/resolved` の 9 形で大文字小文字を問わず、表やチェックリストの中の行も、1 行に同居する複数の参照も効く。**編集を終えてよいと決めるのは一覧であって、自分のキーワード走査ではない**。マージ時の `--subject` / `--body-file` では止められない
3. `--subject` / `--body-file` は squash commit のメッセージを整えるためだけに使う。**closing keyword を書いてはならない**（散文の "partially fixes #N" も効く）— 書くと手順 1 の一覧に現れないまま閉じる。省けば squash 本文は **PR 説明文そのもの**になる（表・チェックリスト込みで冗長）
4. マージ後に**必ず**、次の 3 つを確認する。**`gh pr view <PR> --json closingIssuesReferences` を数えるだけでは足りない** — それは PR 本文からその瞬間に再計算される値であって、閉じた事実そのものではない:
   - 取り直した `gh pr view <PR> --json closingIssuesReferences` の全件が意図どおり閉じたか
   - **残すと決めた issue が今も `OPEN` か**（`gh issue view <issue> --json state`）。正しく動いていればそれらは上の一覧に現れない。**ゆえに一覧を数えるだけでは、守りたい当の issue を一度も見ないことになる**
   - `gh issue list --state closed --search "closed:>=<mergedAt>"`（時刻は `gh pr view <PR> --json mergedAt --jq .mergedAt` で得る）。どちらの一覧にも属さない「知らないうちに閉じた issue」を拾う、唯一の接地した観測点。**この観測点は空を返す形で沈黙しうる**——`closed:>=` は ISO 8601 でなければ**エラーではなく 0 件**を返し、PowerShell の `ConvertFrom-Json` は `mergedAt` を DateTime へ変換して `08/10/2026 10:26:23` の形へ崩す（2026-08-10 実測。`--jq` で生文字列を取れば起きない）。**ゆえに 0 件で終えてはならない**——上の 1 つ目で閉じたと確認した issue が**この一覧にも現れること**を突き合わせる。現れないなら、閉じなかったのではなく検索が効いていない
   誤って閉じていたら `gh issue reopen <issue>`（close イベントは履歴に残り、close を契機に動く下流は巻き戻らない。**reopen は回復であって、事前確認を省く免罪符ではない**）

**手順 1 の一覧が「閉じる issue のすべて」になるのは、手順 3 を守り、かつ確認からマージまで PR 本文が変わらなかったときだけである。** 本文を凍結する機構は無く、`gh pr merge --auto` は確認とマージを引き離すため**使わない**。

**`--delete-branch` を付ける前に、そのブランチを base にする open PR が無いことを確認する**（`gh pr list --state open --base <branch> --json number`）。base ブランチを消された PR は GitHub が自動で `CLOSED` にし、**reopen できない**（`Cannot change the base branch of a closed pull request`・実測）。復旧には下流を main の上へ rebase して PR を取り直すことになる（#830 が #829 の取り直しである）。stacked PR では**下流を先にマージするか retarget してから**消す。
