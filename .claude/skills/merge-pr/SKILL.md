---
name: merge-pr
description: "PR を squash マージするときに使用。道具（npm run merge:pr）が閉じる issue の宣言との照合・道具の要約の照合・マージ後の検証を行い、このスキルは終了コードごとの次の一手を持つ。"
disable-model-invocation: true
argument-hint: "[PR 番号]"
allowed-tools:
  - Bash(gh *)
  - Bash(npm run merge:pr *)
  - Bash(node scripts/merge-pr.mjs *)
  - Read
  - Write
---

PR を squash マージする。

対象: $ARGUMENTS

マージで閉じる issue を決めるのは **PR 本文**であり、`gh pr merge` の引数では抑止できない。PR テンプレートが `Closes` を埋めるため、書いた覚えが無くても残る。確認とマージ後の検証は道具 `scripts/merge-pr.mjs` が構造で持つ（使い方・照合の中身・終了コードの正本はその冒頭）。機構の理由と却下した案は `docs/adr/ADR-squash-merge-issue-autoclose.md`。

## 1. 宣言と本文を用意する

- **このマージで閉じる issue を決める。** 閉じないなら `none`
- squash commit の本文を一時ファイルへ書く（`--body-file`。省けないのは、省くと PR 説明文がそのまま squash 本文になるため）。**closing keyword を書かない**——道具は見つけたら止める
- 件名は PR タイトルから組まれる。変えるなら `gh pr edit <PR> --title` で PR タイトルの方を直す（件名を逐語で渡すと `(#N)` が付かず、main 保護ゆえ恒久的に失われる）

## 2. 道具を走らせる

打つ（Git Bash）: `npm run merge:pr -- <PR> --closes <N,...|none> --body-file <path>`

PowerShell からは `node scripts/merge-pr.mjs <PR> --closes ... --body-file ...` と打つ（`npm.ps1` が最初の `--` を食う・`docs/build-commands.md`「Windows/macOS/Linux で実行可能」）。

## 3. 終了コードで次の一手が決まる

| exit | 意味 | 次の一手 |
|---|---|---|
| 0 | マージ済み。宣言した issue がすべて閉じ、宣言外は閉じていない | なし |
| 1 | **マージしていない。** 止めた理由が全件並ぶ | 理由ごとに直して再実行する（下の「止められたとき」） |
| 2 | **マージしていない。** 走らなかった（使用法・linked worktree・gh / git の失敗） | メッセージに従って前提を直し、再実行する |
| 3 | **マージ済み。** ただし事後検証が赤か、確かめられなかった | 下の「マージ後に要確認が出たとき」 |

### 止められたとき（exit 1）

- **宣言に無い issue を PR 本文が閉じる**: 閉じたくないなら PR 本文から closing keyword を外す（`gh pr edit <PR> --body-file <tmp>`）。認識されるのは `close` / `fix` / `resolve` 系の 9 形で、大文字小文字を問わず、表やチェックリストの中の行も、1 行に同居する複数の参照も効く。**どの語が効いたかを推測せず、外したら道具を再実行して一覧で確かめる**
- **PR タイトルが issue を閉じる形**: `fix: #985 …` のように closing keyword の型名の直後へ番号を置くと閉じる（コロンを挟んでも効く）。番号は末尾へ回すか括弧で包む。`chore:` / `refactor:` が安全なのは型名が keyword でないからであって、形が安全だからではない
- **このブランチを base にする open PR がある**: リポジトリ設定でマージ後に head が自動で消え、base を消された PR は `CLOSED` になり reopen できない（#830 が #829 の取り直し）。下流を先にマージするか retarget する
- **道具の要約の照合に通らない**: 出力の「復帰」の手順どおり、PR の head で `verify:tree` を回し直す（PR を作ったあとに足したコミット・Dependabot の PR もこの形で止まる）
- **CLEAN でない**: 必須チェックの未通過・衝突・レビュー待ちを解消する。`--auto` は確認とマージを引き離すので**使わない**

### マージ後に要確認が出たとき（exit 3）

- 宣言外の issue が閉じていたら、その issue の timeline で閉じた主体を確かめる。このマージが閉じたなら `gh issue reopen <N>`。close イベントは履歴に残り、close を契機に動く下流は巻き戻らない——**reopen は回復であって、事前確認を省く免罪符ではない**
- 「確かめられない」なら、出力が名指す検索や状態を手で引き直す。**検索の 0 件を「該当なし」と読まない**（ISO 8601 でない時刻は、エラーを出さず 0 件を返す）

## 道具が守らないもの

- **道具を経ないマージ**（Web UI・手打ちの `gh pr merge`）は、`--no-verify` と同格の人間専用の迂回である。hook は `gh pr merge` を見ない
- **確認からマージまでの PR 本文の変化**: head は `--match-head-commit` で固定されるが、本文を凍結する機構は無い。道具を走らせているあいだに本文を編集しない
