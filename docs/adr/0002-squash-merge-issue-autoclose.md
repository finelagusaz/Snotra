# ADR-0002: squash マージでの issue 意図せぬ auto-close を、Layer 0 遮断＋手順で防ぐ

## 文脈

GitHub の auto-close は、PR 本文・squash commit 本文の**どこにあっても** `close/closes/closed` `fix/fixes/fixed` `resolve/resolves/resolved` の 9 形（大文字小文字問わず・表やチェックリスト内・1 行同居も可）でマージ時に走る。PR テンプレートが `Closes` 行を埋めるため、**書いた覚えの無い close が残る**。本リポジトリは squash のみ有効で、意図せぬ issue クローズ事故が起きた（#488）。制御点をどこに置くかを決める必要があった。

auto-close の経路は 2 本あり、可視性が非対称である:

- **PR 本文の closing keyword** → `gh pr view <PR> --json closingIssuesReferences` に**現れ**、マージした瞬間に閉じる。`gh pr merge` の `--subject` / `--body-file` では抑止できない（#488 実測）。
- **squash commit 本文の closing keyword** → main に載った時点で閉じる。ただし `--body-file` に書くと `closingIssuesReferences` に**現れないまま**閉じる。

## 決定

1. **Layer 0 で断つ** — `squash_merge_commit_message=PR_BODY` にし、ブランチのコミット本文が squash 本文へ流入する経路を全マージ経路から消した（#488 で設定変更）。なぜ hook では守らないかは下記「検討した代替案と却下理由」が SSOT（`CLAUDE.md`「フック」(A2) はここへのポインタ）。
2. **残余（PR 本文の closing keyword）は手順で守る** — マージ前に `closingIssuesReferences` を見て意図どおりになるまで PR 本文を編集し、マージ後に接地した観測点で確認する。**手順の実体は `CLAUDE.md`「Git/GitHub 運用」に常時ロードで置く** — 手順が視界に無いと #488 保護を失うため（マージ手順の skill 化＝常時視界からの退去は `RETROSPECTIVE.md` で却下済み）。ADR は「なぜ」だけを持つ。

## 検討した代替案と却下理由

- **`gh pr merge` の `--subject` / `--body-file` で PR 本文の closing keyword を抑止する**: 却下。PR 本文の closing keyword はマージした瞬間に閉じ、`--subject` / `--body-file` では抑止できない（#488 実測）。
- **`--body-file` に closing keyword を書いて squash 側で制御する**: 却下。squash 本文に書くと `closingIssuesReferences` に現れないまま閉じ、マージ前確認の一覧（＝唯一の制御点）を信頼できなくする。
- **マージ方式を変えて逃げる（`--merge` / `--rebase`）**: 却下、というより不可。本リポジトリは squash のみ有効（`allow_merge_commit` / `allow_rebase_merge` はいずれも `false`）で GitHub が `--merge` / `--rebase` を拒否する。
- **hook で merge / close を守る**: 却下（**意図的な非対称**——hook が守る外部 API の不可逆呼び出しは `gh pr create` だけである）。3 理由: (1) `merge` / `close` の誤りは人の意図にあり、機構が SSOT を問えない（`deny` が書けない）。(2) `ask` は PreToolUse の fail-closed 骨格と両立しない。(3) hook の視界は Web UI・ユーザー端末からのマージを覆わない。
- **squash 設定の read-back を監視する検知器を置く**: 却下。「セーフティネットの不在を検知するセーフティネット」の無限後退から降りる（#488）。

## 帰結

- 母集団は**派生した参照集合ではなく起きた事実**から取る — 2 経路の可視性が非対称ゆえ、確認は `closingIssuesReferences` だけでなく `gh issue list --search "closed:>=<mergedAt>"` で裏取りする（この一般形は `.claude/rules/safety-nets.md` が持つ）。手順の全文は `CLAUDE.md`「Git/GitHub 運用」。
- squash 設定は `squash_merge_commit_title=PR_TITLE` / `squash_merge_commit_message=PR_BODY`。設定の組み合わせは GitHub が制限する（実測 422）。**元に戻す**なら `gh api -X PATCH repos/:owner/:repo -f squash_merge_commit_title=COMMIT_OR_PR_TITLE -f squash_merge_commit_message=COMMIT_MESSAGES`。

---

status: Accepted
関連: #488 ・`CLAUDE.md`「Git/GitHub 運用」「フック」(A2) ・`.claude/rules/safety-nets.md`
