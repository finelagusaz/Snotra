---
name: merge-pr
description: "PR を squash マージするときに使用。ツール（npm run merge:pr）が閉じる issue の宣言との照合・ツールの要約の照合・マージ後の検証を行い、このスキルは終了コードごとの次の一手を持つ。"
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

マージで閉じる issue を決めるのは PR 本文である。`gh pr merge` の引数では、閉じるのを止められない。PR テンプレートが `Closes` を埋めるので、書いた覚えが無くても残る。確認とマージ後の検証は、ツール `scripts/merge-pr.mjs` が行う（使い方・照合の中身・終了コードの正本は、そのファイルの冒頭にある）。この機構を選んだ理由と却下した案は `docs/adr/ADR-squash-merge-issue-autoclose.md` にある。

## 1. 宣言と本文を用意する

- このマージで閉じる issue を決める。閉じないなら `none`
- squash commit の本文を一時ファイルへ書く（`--body-file`。省くと PR の説明文がそのまま squash の本文になるので、省けない）。closing keyword を書かない（ツールは見つけたら止める）
- 件名は PR タイトルから組まれる。変えるなら、`gh pr edit <PR> --title` で PR タイトルの方を直す（件名を逐語で渡すと `(#N)` が付かない。main は保護されているので、後から付け直せない）

## 2. ツールを走らせる

打つ（Git Bash）: `npm run merge:pr -- <PR> --closes <N,...|none> --body-file <path>`

`--dry-run` を足すと、マージ前の検査をすべて行う。通れば、マージする内容（PR・head の sha・閉じる issue・squash 本文の先頭）を出して exit 0 で止まる（`gh pr merge` を呼ばない）。止める理由があれば、本番と同じく exit 1 になる。`--dry-run` は `--` の後ろに置く。前に置くと、npm が自分の設定として受け取ってしまう（ツールは `npm_config_dry_run` を見て、その場合も dry-run として扱う）。

PowerShell からは、`--` を引用して `npm run merge:pr '--' <PR> --closes ... --body-file ...` と打つ。素の `--` は npm.ps1 の経路で消え、`--closes` などが npm に取られる（#1296）。`'--'` は、文として打ってもパイプ入力で渡しても、`--dry-run` まで届く（2026-11-03 に実測）。

## 3. 終了コードで次の一手が決まる

| exit | 意味 | 次の一手 |
|---|---|---|
| 0 | マージ済み。宣言した issue がすべて閉じ、このリポジトリで宣言外の issue は閉じていない（他リポジトリの close はマージ前の照合で防ぐだけで、事後には見ない）。続けて、そのブランチのツールの worktree と要約を片づける（#1359） | なし。「片づけられなかった」と出たら、理由に従って `git worktree remove <worktree>` で消す（終了コードは 0 のまま。マージは済んでいる） |
| 1 | マージしていない。止めた理由が全件並ぶ | 理由ごとに直して再実行する（下の「止められたとき」） |
| 2 | マージしていない。ツールが走らなかった（使用法・linked worktree・gh / git の失敗） | メッセージに従って前提を直し、再実行する |
| 3 | マージを試みたあとの要確認。出力の最初の行（判定）が「マージ済み」なら、事後検証が赤である。「成否を確かめられない」なら、マージされたかどうかも分からない。gh 自身の失敗の通知は、判定のあとに添えられる | 下の「マージ後に要確認が出たとき」 |

事後検証は、GitHub への反映の遅れを待ちながら、最大 30 秒まで観測し直す。出力の「観測 N 回・待ち計 S 秒」は、GitHub の反映の遅れの実測でもある。exit 3 で待ち時間を使い切っていたら、時間を置いてから下の手作業で引き直す。

### 止められたとき（exit 1）

- 宣言に無い issue を PR 本文が閉じる: 閉じたくないなら、PR 本文から closing keyword を外す（`gh pr edit <PR> --body-file <tmp>`）。認識されるのは `close` / `fix` / `resolve` 系の 9 形である。大文字小文字を問わず、表やチェックリストの中の行でも、1 行に並ぶ複数の参照でも効く。どの語が効いたかを推測しない。外したらツールを再実行し、一覧で確かめる
- PR タイトルが issue を閉じる形: `fix: #985 …` のように、closing keyword である型名の直後へ番号を置くと閉じる（コロンを挟んでも効く）。番号は末尾へ回すか、括弧で包む。`chore:` / `refactor:` が安全なのは、型名が keyword でないからである。形が安全だからではない
- このブランチを base にする open PR がある: リポジトリの設定により、マージ後に head が自動で消える。base を消された PR は `CLOSED` になり、reopen できない（#830 は #829 を作り直したもの）。下流を先にマージするか、retarget する
- ツールの要約の照合に通らない: 出力の「復帰」の手順どおりに、PR の head で `verify:tree` を回し直す（PR を作ったあとに足したコミットや、Dependabot の PR もこの形で止まる）
- CLEAN でない: 必須チェックの未通過・衝突・レビュー待ちを解消する。`--auto` は確認とマージの時点を引き離すので、使わない

### マージ後に要確認が出たとき（exit 3）

- 「成否を確かめられない」: まず `gh pr view <PR> --json state,mergedAt --jq .` で状態を見る。`MERGED` でなければ、原因を直してツールを再実行する。`MERGED` なら、ツールの事後検証は 1 つも走っていない（再実行しても、PR が OPEN でないので exit 1 で止まる）。そのため、次の 3 つを手で行う
  1. 宣言した issue ごとに、`gh issue view <N> -R <owner/repo> --json state` が `CLOSED` であること
  2. `mergedAt` の生の文字列（`--jq .mergedAt`）で `gh pr list --state merged --search "merged:>=<mergedAt>" -L 200` を打ち、当の PR が現れること（現れなければ時刻の形が効いていない。その場合は次の検索を信用しない）
  3. `gh issue list --state closed --search "closed:>=<mergedAt>" -L 200` に、宣言外の issue が無いこと。ただし、宣言したこのリポジトリの issue がまだ現れていなければ、索引がまだ反映されていない。時間を置いて打ち直す（0 件を「宣言外なし」と読まない）。200 件に達したら、打ち切りと区別できないので信用しない
- 宣言外の issue が閉じていた: その issue の timeline で、閉じた主体を確かめる。このマージが閉じたなら `gh issue reopen <N>` する。close イベントは履歴に残る。close を契機に動く下流の処理は巻き戻らない。reopen は回復の手段であって、事前の確認を省いてよい理由にはならない
- 「検算が通らない」「索引が未反映」「事後検証を確かめられない」（最後のものは、上限への到達・gh の失敗・`mergedAt` の形のどれか）: 出力が名指す検索や状態を、上の 1〜3 の形で手で引き直す。検索の 0 件を「該当なし」と読まない（ISO 8601 でない時刻を渡すと、エラーを出さずに 0 件を返す）

## ツールが守らないもの

- ツールを経ないマージ（Web UI・手打ちの `gh pr merge`）は、`--no-verify` と同じく人間専用の迂回である。hook は `gh pr merge` を見ない
- 確認からマージまでの間の PR 本文の変化: head は `--match-head-commit` で固定される。しかし本文を凍結する機構は無い。ツールを走らせているあいだに本文を編集しない
