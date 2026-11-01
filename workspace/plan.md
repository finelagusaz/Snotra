# 計画 — issue #1352

## 目的と受け入れ条件

変異の実行に timeout を掛けるセッションが、timeout の後に孤児の木を残さないようにする。規範の正本は rule に置き、委譲の本文からそれを指す。

- [x] AC1: `.claude/rules/safety-nets.md`「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」に、次の内容の 1 行がある
  - 変異の実行に timeout を掛けるなら、根が生きているうちに木ごと止める
  - `spawnSync` の timeout には任せない
  - 止めた後に孤児が 0 であることを確かめる
  - 理由（`cmd.exe` が挟まると、その下が残る／timeout 後の pid へ taskkill しても空振りする）と `(#1352 実測)` を添える
- [x] AC2: `.claude/skills/implement/SKILL.md:111` の変異注入の行に、上の条項を指す一言がある。中身は写さない
- [x] AC3: 見出しは変えない。`npm run governance:check` が通る

## 変更ファイル

- `.claude/rules/safety-nets.md` — 上記の節の末尾へ 1 行
- `.claude/skills/implement/SKILL.md` — 111 行の末尾へ参照を 1 句

## 条文の草案

safety-nets.md:

> - 変異の実行に timeout を掛けるなら、`spawnSync` の timeout に任せない。`spawn` と自前のタイマーで、根が生きているうちに `taskkill /PID <pid> /T /F` で木ごと止める。終わったら、`Get-CimInstance Win32_Process` で、親が既に居ない vitest・ワーカーなどの孤児が 0 であることを確かめる。timeout が止めるのは直下だけである。`shell: true` や `npx`（Windows では `npx.cmd`）が挟む `cmd.exe` の下は、駆動スクリプトの終了後も残り、無限ループする変異では 1 コアを使い続ける。根が先に死ぬので、timeout の後に戻り値の pid へ taskkill しても空振りする（#1352 実測）

SKILL.md:111 の末尾:

> timeout を掛けて回すなら、後始末は `.claude/rules/safety-nets.md`「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」に従わせる（#1352）

## 実装順序

1. safety-nets.md へ条文を足す
2. SKILL.md:111 へ参照を足す
3. `npm run governance:check`（引用した節名の照合を含む）

## 不変条件

- 節見出し・他の行を変えない。節名を逐語で引く箇所は `git grep 稼働中のガードを弱めない` が正本（調査時点で 15 ファイル）

## テスト方針と検証コマンド

- 規範の追加だけで、製品の挙動は変わらない。検証は `npm run governance:check`
- 条文の手順は調査で実測済み: 根が生きているうちの `taskkill /T /F` で残存 0（`drive2.mjs shell-treekill`）。timeout 後の pid への taskkill は空振りし、残存 4（`drive4.mjs`）

## SPEC.md・関連文書

- `SPEC.md`: 不要

## 作業項目

### Phase 1

- [x] `.claude/rules/safety-nets.md` へ条文を足す
- [x] `.claude/skills/implement/SKILL.md:111` へ参照を足す
- [x] `npm run governance:check` を通す（全検査 passed・17 件）
- [x] scratchpad の再現の足場（`repro1352/` と、その中の `node_modules` junction）を撤去する。junction は `cmd /c rmdir` で外し、本体の `node_modules` を消さない

## 未確定（実装前に潰す）

- [x] `/implement` SKILL.md:111 に正本を指す一言を置くか — 置く。配送はリポジトリ内の原本を読んだときにしか当たらず、委譲先に届くかはリポジトリの内側から決まらない（敵対的調査 B5）。この行は主が委譲先へ渡す本文の一部なので、配送とは独立に届く。写しを増やさないよう、中身は書かず節名だけを指す
- [x] 敵対的調査の所見を反映する — B1〜B5 を採用し（research.md）、条文を「timeout 後に止める」から「根が生きているうちに止める」へ変えた

## 人間レビュー

- [x] 承認済み — 2026-11-01 / 問い: "`workspace/plan.md` へ注釈を追加するか、計画を明示的に承認してください" / 回答: "OK"

## セルフレビュー

- リスク: 通常（規範 2 か所への追記。永続化・並行性・状態遷移を含まず、見出しの移動もない）
- plan-review: 未実施（通常リスク）
- エージェント数: 0（3b の 1 体を除く）
- 要対処: 0（照合 1〜5: issue の要件 3 点のうち「再現」は調査で済み、「1 行足す」は AC1、「rule に置く」は AC1+AC2 に対応する。境界は、根が生きている場合と死んだ後の場合を実測した。新しい状態は無い。より単純な案として「直接起動を推奨」も検討したが、vitest の実装に依存するので主にしない。壊してはならない見出しは governance:check で検知する）
- 未検証: 機序（job object）は未裁定。条文には入れない
- 散文の全件列挙: 未実施（通常リスク）

## 巡の記録

- 巡 1: sha 065a4191 / 道具 green（E・F）/ 指摘 Medium 2・Low 5（⚠️ 2 を含む）/ 変異 1 本（SKILL.md の節名参照）を governance:check が検知、生き残り 0。Medium 2 件（孤児の判定手順・訳語「根」「木」）と Low（Windows の前提・`child.pid`・`shell: true` のまま）を反映。レビューが提案した「複製のパスで絞る」は、ワーカーの CommandLine に複製のパスが無いと実測したので採らず、`CreationDate` + `vitest` で絞る形にした
- 巡 2: sha c49e01b0 / 道具 green（E・F）/ 指摘 Medium 1（2-2: 名前で絞ると並走する vitest を拾う。2-1 の自己一致も同じ根）・Low 4 / 変異は巡 1 と同じ参照なので省略。確認の手順を「ルートプロセスが生きているうちに子孫の (ProcessId, CreationDate) を控え、止めた後に照合する」へ替えた。probe で実測: kill だと 4 組が残り、taskkill /T /F だと 0 組。条項を短くした（2-6）。2-3 は「数秒おいて」で吸収
- 巡 3: sha 4dbe0f98 / 道具 green（E・F）/ 前巡の所見は全部解消。新しい所見は Low 1・⚠️ Low 2。3-1（限定句「無限ループする変異では」の脱落）を戻した。3-2（控えた後に生まれた子孫は照合に入らない）と 3-3（ParentProcessId を辿るときの PID 再利用）は採らない: どちらも未測定で、控えから taskkill までの窓は短い。taskkill /T は生きている親の鎖を止める。手順を重くする費用に見合わない
