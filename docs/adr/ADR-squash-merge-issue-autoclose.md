# ADR-squash-merge-issue-autoclose: squash マージでの issue 意図せぬ auto-close を、Layer 0 遮断＋手順で防ぐ

## 文脈

GitHub の auto-close は、PR 本文・squash commit 本文の**どこにあっても** `close/closes/closed` `fix/fixes/fixed` `resolve/resolves/resolved` の 9 形（大文字小文字問わず・表やチェックリスト内・1 行同居も可）でマージ時に走る。PR テンプレートが `Closes` 行を埋めるため、**書いた覚えの無い close が残る**。本リポジトリは squash のみ有効で、意図せぬ issue クローズ事故が起きた（#488）。制御点をどこに置くかを決める必要があった。

auto-close の経路は 2 本あり、可視性が非対称である:

- **PR 本文の closing keyword** → `gh pr view <PR> --json closingIssuesReferences` に**現れ**、マージした瞬間に閉じる。`gh pr merge` の `--subject` / `--body-file` では抑止できない（#488 実測）。
- **squash commit 本文の closing keyword** → main に載った時点で閉じる。ただし `--body-file` に書くと `closingIssuesReferences` に**現れないまま**閉じる。

## 決定

1. **Layer 0 で断つ** — `squash_merge_commit_message=PR_BODY` にし、ブランチのコミット本文が squash 本文へ流入する経路を全マージ経路から消した（#488 で設定変更）。なぜ hook では守らないかは下記「検討した代替案と却下理由」が SSOT（`CLAUDE.md`「フック」(A2) はここへのポインタ）。
2. **残余（PR 本文の closing keyword）は手順で守る** — マージ前に `closingIssuesReferences` を見て意図どおりになるまで PR 本文を編集し、マージ後に接地した観測点で確認する。**手順の本文は `/merge-pr` が持ち、`CLAUDE.md`「Git/GitHub 運用」には「`/merge-pr` に従う・`--auto` 不使用・#488 の核心」を 1 行で常時ロードに残す**（2026-07-28 改訂: 当初は「手順が視界に無いと #488 保護を失う」を理由に skill 化を却下し全文を常時ロードに置いていたが、常時ロード面の不変条件「手順本文を置かない」の新設に伴い移設した。旧懸念へはポインタの常時視界と、user 起動スキルが起動時に手順全文を注入することで応える。`/merge-pr` は user 起動専用ゆえモデルは自発起動できず、指示されたマージではポインタから `.claude/skills/merge-pr/SKILL.md` を読む）。ADR は「なぜ」だけを持つ。

## 検討した代替案と却下理由

- **`gh pr merge` の `--subject` / `--body-file` で PR 本文の closing keyword を抑止する**: 却下。PR 本文の closing keyword はマージした瞬間に閉じ、`--subject` / `--body-file` では抑止できない（#488 実測）。
- **`--body-file` に closing keyword を書いて squash 側で制御する**: 却下。squash 本文に書くと `closingIssuesReferences` に現れないまま閉じ、マージ前確認の一覧（＝唯一の制御点）を信頼できなくする。
- **マージ方式を変えて逃げる（`--merge` / `--rebase`）**: 却下、というより不可。本リポジトリは squash のみ有効（`allow_merge_commit` / `allow_rebase_merge` はいずれも `false`）で GitHub が `--merge` / `--rebase` を拒否する。
- **hook で merge / close を守る**: 却下（**意図的な非対称**——hook が守る外部 API の不可逆呼び出しは `gh pr create` だけである）。3 理由: (1) `merge` / `close` の誤りは人の意図にあり、機構が SSOT を問えない（`deny` が書けない）。(2) `ask` は PreToolUse の fail-closed 骨格と両立しない。(3) hook の視界は Web UI・ユーザー端末からのマージを覆わない。
- **squash 設定の read-back を監視する検知器を置く**: 却下。「セーフティネットの不在を検知するセーフティネット」の無限後退から降りる（#488）。

## 帰結

- 母集団は**派生した参照集合ではなく起きた事実**から取る — 2 経路の可視性が非対称ゆえ、確認は `closingIssuesReferences` だけでなく `gh issue list --search "closed:>=<mergedAt>"` で裏取りする（この一般形は `.claude/rules/safety-nets.md` が持つ）。手順の全文は `/merge-pr`。
- squash 設定は `squash_merge_commit_title=PR_TITLE` / `squash_merge_commit_message=PR_BODY`。設定の組み合わせは GitHub が制限する（実測 422）。**元に戻す**なら `gh api -X PATCH repos/:owner/:repo -f squash_merge_commit_title=COMMIT_OR_PR_TITLE -f squash_merge_commit_message=COMMIT_MESSAGES`。

## 追記（2026-10-31・#1312）— 決定 2 の「手順で守る」は道具へ移った

決定 2 が手順で守っていた残余は、`npm run merge:pr`（`scripts/merge-pr.mjs`）の構造へ移した。人は `/merge-pr` から道具を起動し、終了コードを読む。凍結規約により本文は書き換えず、ここで読み替える。同じ道具が #1304（マージ直前に、道具の要約の sha から PR の head までを照合する）も吸収した。照合は PR 前ゲートと共有し、写しは持たない（`.claude/hooks/pre-bash.mjs` の `verifiedSummaryState`）。

手順の頃に踏んだ実測（道具が構造で消したもの）:

- `gh pr merge --subject` を渡すと件名が逐語で使われ、`PR_TITLE` 設定が組む `(#N)` が付かない。main は保護されていて amend も force-push もできないので、マージの瞬間に恒久的に失われる。件名も CI もすべて緑のまま起きた（2026-08-27・`5a30985d`）。→ 道具は件名を渡す口を持たない
- PowerShell の `ConvertFrom-Json` は `mergedAt` を DateTime へ変換して `08/10/2026 10:26:23` の形へ崩し、`closed:>=` の検索はエラーを出さず 0 件を返した（2026-08-10）。`closed:` と `merged:` は ISO 8601 と日付のみを受け、ほかの形は沈黙で 0 件になる（2026-10-31 に #1312 の敵対的調査で再測）。→ 道具は ISO 8601 だけを検索へ渡し、同じ時刻で当の PR が `merged:` の検索に現れることを検算してから 0 件を受け取る
- リポジトリ設定が `delete_branch_on_merge: true` なので、`--delete-branch` を付けなくても head はマージの数秒後に消える（#830 の timeline）。→ base にする open PR の検査はフラグに依らず行う

### 検討した代替案と却下理由

- **道具が PR 本文を編集して closing keyword を外す**: 却下（2026-10-31 のユーザー裁定）。どの issue を残すかは人の意図であり、書き換えの規則を道具に持たせない。道具は止めて人へ返す
- **hook で `gh pr merge` を検出して照合する**: 却下。上の却下理由 3 点がそのまま当たる。とくに (3)、Web UI のマージは hook の視界の外にある。道具を経ないマージは `--no-verify` と同格の人間専用の迂回とした
- **閉じる issue の宣言を持たず、マージ後の検索だけで守る**: 却下。検出はできても予防にならない。宣言（`--closes`）と `closingIssuesReferences` の完全一致をマージ前に照合する
- **要約が無い PR（別の機体・Dependabot）を通す迂回フラグ**: 却下。`ADR-pr-gate-verified-sha` が PR 前ゲートで要約の無いブランチを通す案を却下したのと同型である。復帰は PR の head で `verify:tree` を回すことで、道具が出力に書く
- **要約を手元の HEAD と照合する**: 却下。マージされるのは PR の head であって手元ではない。照合した sha を `--match-head-commit` に渡し、照合とマージの対象を構造で一致させる

---

status: Accepted
関連: #488 ・`CLAUDE.md`「Git/GitHub 運用」「フック」(A2) ・`.claude/rules/safety-nets.md` ・`/merge-pr`
