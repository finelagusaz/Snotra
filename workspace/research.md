# 調査 — issue #1504 retrospective の運用とタイミングを見直す

## issue の要約

`/retrospective` は「開発サイクル完了時」に使う user 起動専用スキル。次の 4 点が噛み合っていない。

1. サイクルの単位と起動の契機がどこにも書かれていない
2. `RETROSPECTIVE.md` が古いまま、毎サイクルの入口（`AGENTS.md`・`/start-issue` 3a・`/implement` 1b）で「前回のサイクル」として読まれる
3. Step 1 の範囲（`origin/main..HEAD`）は、マージ後や複数 PR のサイクルで空になる
4. ほかの仕組みとの役割の重なり

## 前提とする裁定（会話で決定・2026-11-22）

- 経緯（ユーザー）: このスキルは当初 `/implement` とセットで毎回回していた。1 回ごとに発見があり、スキル・CLAUDE.md の更新と積み残しの洗い出しをしていた。開発方法が安定し、Claude Code / Claude の発展もあって、頻度が自然に下がった
- 入口での扱い: (a) 入口で `RETROSPECTIVE.md` を読むのをやめる。ファイルは振り返りの記録として残す（ユーザー回答「(a) で進めて」）
- 起動の契機: マージ時に促す案は撤回。人の判断のまま（user 起動専用を保つ）。頻度低下は「契機の欠落」ではなく「安定化」による、というユーザーの説明に従う

## 実測した事実

- 振り返りの書き換え履歴（`git log -G'^# Retrospective' -- RETROSPECTIVE.md`）: 8〜9 月はほぼ 1 issue ごと（9/2 は 1 日に振り返り 5 本: #1216・#1218・#1221・#1227・#1232 と、訂正 1 本 #1229）。以降 9/24（#1248）→ 11/01（#1350）→ 11/03（#1360）
- `d2a0b9a4`（#1360）から `origin/main`（`cfd9a851`）まで 96 コミット
- `RETROSPECTIVE.md` を最後に触ったコミットは `0265463c`（#1421・語の直し）で、振り返りではない。`git log -1 -- RETROSPECTIVE.md` では前回の振り返りを特定できない。見出し行の変化（`-G'^# Retrospective'`）なら今は `d2a0b9a4` を返す（見出しの語を直す変更が入ると壊れる）
- 当初 `/retrospective` が担っていた役目のうち、次の 4 つは、いま `/implement` の中にある（下限。Step 2・3 の構造的パターンの抽出と配置、Step 6 のメモリの鮮度、Step 7 の health-check には `/implement` に置き場が無い。敵対的調査で訂正）
  - 積み残し・退けた所見: PR 本文の「残したもの」（`.github/pull_request_template.md`、`/implement` Step 3 の消し込み 1 行 → Step 5）
  - 否定の知識: `/implement` Step 4 の ADR / PR 本文への回収
  - 範囲外の作業: issue（`/implement` Step 4 の `workspace/` 削除条件）
  - 規範の更新: チーム憲章「その場でリポジトリの記録へ書く」（ルート `CLAUDE.md`）

## 関連するファイル・箇所

`git grep -n "RETROSPECTIVE\|retrospective\|振り返り"`（`docs/superpowers/`・`docs/design/`・`docs/adr/` は凍結記録なので除外）で列挙した。

| 箇所 | 現状 | (a) との関係 |
|---|---|---|
| `AGENTS.md`「開発ワークフロー」冒頭の 1 文 | サイクル開始前に `RETROSPECTIVE.md` を読む | 削る |
| `.claude/skills/start-issue/SKILL.md`「3a. 調査を書く」 | 読む文書の列挙に `RETROSPECTIVE.md` | 列挙から外す |
| `.claude/skills/implement/SKILL.md`「1b. 分岐」の「計画なしの経路でだけ行う調査」 | 先頭項目で `RETROSPECTIVE.md` を読む | 項目を削る |
| `AGENTS.md`「RETROSPECTIVE.md の運用」 | 持ち越しタスクを置かない・手順の正本は `/retrospective` | 真のまま。触らない |
| ルート `CLAUDE.md`「チーム憲章」 | 恒久的な情報の置き場所に `RETROSPECTIVE.md` を含める | 上書きされるファイルを「恒久」と書いており、もともと「RETROSPECTIVE.md の運用」と食い違う。(a) で入口でも読まれなくなるので、列挙から外す |
| ルート `CLAUDE.md`「利用できるスキル」表の `/retrospective` 行 | 「サイクル終了後の教訓抽出・`RETROSPECTIVE.md` 上書き」 | 契機の書き方を新しい定義に合わせる |
| `.claude/skills/retrospective/SKILL.md` | description「開発サイクル完了時」・Step 1 は `origin/main..HEAD` | 単位・契機の定義（受け入れ条件 1 の 1 か所）と Step 1 の範囲を書き直す |
| `.claude/skills/health-check/SKILL.md` 7d | 「今サイクルで…」「実施は `/retrospective`「Step 6」が担う」 | Step 番号・見出しを変えなければ真のまま |
| `.claude/rules/governance-docs.md`「条項を足す・消す・移すとき」#1222 の項 | サイクル境界を `RETROSPECTIVE.md` の更新で数えた事例 | 歴史の記述で真のまま。触らない |
| `snotra-core/src/search/build.rs`（`new_from_tree` の doc） | 「`RETROSPECTIVE.md` が前サイクルの取りこぼしとして記録している」 | 既に偽（上書きで消えた）。本 issue の変更が偽にしたものではない。範囲外として PR 本文に残す |
| `snotra-core/tests/path_query_cost.rs` | 「`RETROSPECTIVE.md` を参照先にしない」 | 真のまま |

機構（`scripts/`・`.github/`・`.githooks/`・`.claude/hooks/`・`package.json`）に `retrospective` / `RETROSPECTIVE` の参照は無い（`git grep -i` 0 件）。スキル表を照合する governance 検査も今は無い（`scripts/governance/` に該当なし）。

## 再利用できる既存のパターン

- 範囲の起点は `origin/main`、`log` は 2 点形・`diff` は 3 点形（`AGENTS.md`「検証の作法（全タスク共通）」）
- squash マージなので、`origin/main` の各コミットの件名末尾に `(#N)` が付き、PR 番号を取り出せる（直近 96 件すべてに `(#` を確認）
- 引数で範囲を受ける形: `/merge-pr` の `argument-hint: "[PR 番号]"`

## 技術的な制約

- `/retrospective` は user 起動専用（`disable-model-invocation: true`）。Skill ツールから呼べない
- 対象はセーフティネット（`.claude/skills/**`）と規範文書（`AGENTS.md`・ルート `CLAUDE.md`）。マージは合意してから（ルート `CLAUDE.md` 最重要ルール 2）
- 序数で他を指さない（`.claude/rules/governance-docs.md`）。health-check が `/retrospective`「Step 6」を名指すので、Step の番号と見出しは変えない
- `allowed-tools` は `Bash(git *)` と `Bash(gh *)` を持つ。範囲の列挙に要るのは git と gh だけ

## 未解決の疑問

1. 前回の振り返りの終点をどう特定するか。案 A: `RETROSPECTIVE.md` に範囲（`<起点>..<終点>` の sha と日付）を書き、次回の既定の起点にする。案 B: 見出し行の変化（`git log -G`）。案 A は明示的で、読む人にも「いつの記録か」が分かる。案 B は見出しの語の直しで壊れる
2. 今の `RETROSPECTIVE.md` には範囲の行が無い。初回は引数で範囲を渡させるか、案 B を初回だけ使うか
3. マージ前のブランチ（`origin/main..HEAD`）も範囲に含めるか。含めないと「PR を出す前に振り返る」使い方で空になる
4. Step 6・7 を別の周期に切り出すか。範囲指定の振り返りが「必要を感じたときにまとめて回す」ものになれば、メモリの鮮度と health-check もその周期で回して不自然ではない

## 敵対的調査（`workspace/adversarial-1504.txt`）の採否

- 採る: 「当初の役目の多くは `/implement` にある」は過大だった。置き場があるのは積み残し・否定の知識・範囲外作業・規範更新の 4 つだけである。構造的パターンの抽出と配置（Step 2・3）、メモリの鮮度（Step 6）、health-check（Step 7）は `/retrospective` にしか無い。事実の行を下限へ直した。計画への帰結: これらが `/retrospective` を残す理由であり、Step 6・7 を切り出さない判断と整合する
- 採らない: health-check の description とスキル表の `/health-check` 行の「サイクル完了後」。本変更で偽にならない（health-check は単独でも人が起動でき、「大きなサイクル完了後や定期メンテナンス時」は契機の例示として真のまま）
- ⚠️ `argument-hint` と `$ARGUMENTS` の既定: 計画の変更表に含めた
- ⚠️ 「出力」節の項目 5: Step 7 を切り出さないので真のまま
- ⚠️ 「RETROSPECTIVE.md の運用」と Step 5 の上書き: 上書き運用を続けるので真のまま。「記録として残す」は追記の意味ではない
- ⚠️ governance:check を実走していない: 計画の Phase 3 で実走する
- 壊せなかった: 参照の表（11 行）・機構の不在・`d2a0b9a4` と 96 件と `(#N)` の全件・Step 番号を名指すのは health-check 7d だけ
