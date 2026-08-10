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

PR を squash マージする。

対象: $ARGUMENTS

## この手順が唯一の防御である理由

マージで閉じる issue を決めるのは **PR 本文**である。`gh pr merge` の `--subject` / `--body-file` では抑止できない（#488 実測）。

auto-close は本文の**どこにあっても**走る。対象は `close` / `fix` / `resolve` 系の 9 形で、大文字小文字を問わない。表やチェックリストの中の行も効く。

PR テンプレートが `Closes` を埋めるため、**書いた覚えが無くても残る**。

hook もこれを見ていない（ルート `CLAUDE.md`「フック」の (A2)）。

**機構がここに無いので、下の手順が唯一の防御である。**

なぜこの機構になるかは `docs/adr/ADR-squash-merge-issue-autoclose.md` が持つ。2 経路の可視性の非対称・マージ方式では逃げられないこと・squash 設定と復元レシピ。

## 手順

squash マージでは常にこの順で行う。`<PR>` は PR 番号、`<issue>` は issue 番号。

### 1. 閉じる issue の一覧を見る

打つ: `gh pr view <PR> --json closingIssuesReferences`

**マージ直前に必ず打つ。** これが GitHub の計算した「いま閉じる issue」である。

### 2. 閉じたくない issue が在れば、本文を編集する

打つ: `gh pr edit <PR> --body-file <tmp>`

編集したら手順 1 をやり直す。**一覧から消えるまで繰り返す。**

⚠ どの行のどの語が効いたかを推測しない。認識されるのは 9 形（`close/closes/closed`・`fix/fixes/fixed`・`resolve/resolves/resolved`）で、大文字小文字を問わない。表やチェックリストの中の行も、1 行に同居する複数の参照も効く。

**編集を終えてよいと決めるのは一覧であって、自分のキーワード走査ではない。**

マージ時の `--subject` / `--body-file` では止められない。

### 3. squash commit のメッセージを整える

`--subject` / `--body-file` はこのためだけに使う。

⚠ **closing keyword を書いてはならない。** 散文の "partially fixes #N" も効く。書くと手順 1 の一覧に現れないまま閉じる。

省けば squash 本文は **PR 説明文そのもの**になる（表・チェックリスト込みで冗長）。

### 4. マージ後に 3 点を確認する

⚠ **`gh pr view <PR> --json closingIssuesReferences` を数えるだけでは足りない。** それは PR 本文からその瞬間に再計算される値であって、閉じた事実そのものではない。

**(1) 取り直した一覧の全件が、意図どおり閉じたか**

**(2) 残すと決めた issue が今も `OPEN` か**

打つ: `gh issue view <issue> --json state`

正しく動いていれば、それらは (1) の一覧に現れない。**ゆえに一覧を数えるだけでは、守りたい当の issue を一度も見ないことになる。**

**(3) 知らないうちに閉じた issue が無いか**

打つ: `gh issue list --state closed --search "closed:>=<mergedAt>"`

時刻: `gh pr view <PR> --json mergedAt --jq .mergedAt`

これは 2 つの一覧のどちらにも属さない issue を拾う、唯一の接地した観測点である。

⚠ **0 件は「該当なし」を意味しない。** ISO 8601 でない時刻は、エラーを出さず 0 件を返す。

0 件を受け取る前に、検索が効いていることを確かめる。

- (1) に issue が在る → その issue が (3) の一覧にも現れることを見る。現れないなら、閉じなかったのではなく検索が効いていない
- (1) が空 → 突き合わせる相手がいない。既知の閉じた issue を含む境界（`closed:>=<日付>` の形）で同じ検索を打ち、形が効いていることを確かめる

実測 2026-08-10: PowerShell の `ConvertFrom-Json` は `gh pr view <PR> --json mergedAt` の値を DateTime へ変換する。`08/10/2026 10:26:23` の形へ崩れ、0 件で沈黙した。`--jq` で生文字列を取れば起きない。同日、(1) が空の PR でもこの検算で 0 件を受け取った。

### 誤って閉じていたら

打つ: `gh issue reopen <issue>`

close イベントは履歴に残る。close を契機に動く下流は巻き戻らない。**reopen は回復であって、事前確認を省く免罪符ではない。**

## 手順 1 の一覧が「閉じる issue のすべて」になる条件

手順 3 を守り、かつ確認からマージまで PR 本文が変わらないこと。

本文を凍結する機構は無い。`gh pr merge --auto` は確認とマージを引き離すため**使わない**。

## `--delete-branch` を付ける前に

打つ: `gh pr list --state open --base <branch> --json number`

**そのブランチを base にする open PR が無いことを確認する。**

⚠ base ブランチを消された PR は、GitHub が自動で `CLOSED` にする。**reopen できない**（`Cannot change the base branch of a closed pull request`・実測）。

復旧には下流を main の上へ rebase し、PR を取り直すことになる（#830 が #829 の取り直しである）。

stacked PR では、**下流を先にマージするか retarget してから**消す。
