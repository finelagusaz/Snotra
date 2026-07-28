## 問題なし

- issue #831 自身が「スラグは呼び出し側が渡した『内容』ゆえ契約に触れる」対「スラグは『識別子』ゆえ契約に触れない」という二つの読みを提示し、「どちらを採るかが決定の中身であり、決めずに実装すると契約が黙って緩む」と明記している。これは `AGENTS.md`「ドキュメント参照」の ADR 要件（否定の知識＝なぜ B を却下したか、が生じた決定のみ）を満たす。plan.md が ADR に書くと宣言している内容（却下した読み＝「内容ゆえ書けば契約が緩む」)はこの issue の論点そのものであり、ADR 不要という結論にはならない。
- **写しの製造になっていないかを確認した。** `docs/adr/` 全 21 本に対し `grep -rln "スラグ\|ledger\|台帳" docs/adr/` は 0 件。`scripts/plan-review-ledger.mjs:12-18` の冒頭契約コメントも「呼び出し側が渡した**内容**を書かない」とだけ書かれ、識別子との区別には触れていない。`.claude/skills/plan-review/SKILL.md:89`（オーケストレーターが `Write` を持たない理由＝「返り値で受けた内容を自分で成果物ファイルへ転記でき…自作自演で通ってしまう」）も同様に「内容」を前提にした記述で、スラグが内容に当たるかは論じていない。既存 ADR・契約コメント・SKILL 本文のいずれにも今回の否定の知識の劣化コピーは見当たらず、`docs/adr/ADR-check-skill-skeleton.md` 却下 4 が警告する「正本探しをしない写しの製造」には当たらない。
- ファイル名 `ADR-plan-ledger-population-persistence.md` は既存 21 本（`ADR-area-metric-characters` 〜 `ADR-workspace-lints-canary-scope`）のいずれとも衝突しない。`scripts/governance-check.mjs:1387` の `ADR_FILE_NAME` 正規表現 `^ADR-([a-z][a-z0-9]*(?:-[a-z0-9]+)*)\.md$` にも一致する。命名も「plan-ledger」「population-persistence」でテーマ・目的が読み取れ、`.claude/rules/governance-docs.md` の命名規約（テーマ・目的が決まった時点で分かる形、連番を振らない）に沿う。
- plan.md の「新規 ADR は `governance:check` の `governanceDocs` と `headingRefDocs` の母集団に入る」という記述を実装で検証した。`governanceDocs()`（`scripts/governance-check.mjs:1037-1046`）は `docs/` 配下で `docs/superpowers/` を除く `.md` を拾い、`headingRefDocs()`（`:1053-1057`）は `docs/superpowers/` と `workspace/` を除く全 `.md` を拾う。`docs/adr/ADR-plan-ledger-population-persistence.md` はどちらの述語にも一致する。
- AGENTS.md・ルート CLAUDE.md は ADR を個別に索引していない（`grep -n "ADR" AGENTS.md CLAUDE.md` は `AGENTS.md:21` の総称ポインタ 1 行と、`ADR-squash-merge-issue-autoclose` への個別参照 2 行のみ）。新規 ADR 追加のたびに更新すべき中央索引は存在せず、plan.md の「変更不要」リストがこの点を（明示はしていないが）正しく扱っている。
- ADR の内容スコープが issue #831 の論点（識別子 vs 内容の契約境界）1 点に絞られている。plan.md の「未確定」節にある他の小さな決定（台帳ファイル名を `.ledger.json` でなく `ledger.json` にする理由・JSON 形式を `{slugs:[...]}` に留める理由・exit code 2 への統一）は ADR の変更ファイル一覧に現れず、`scripts/plan-review-ledger.mjs` 自身の契約コメント／JSDoc（既存パターン：`:20-27` が非冪等 mkdir・exit 2 の理由を既にこの形で書いている）に収まる性質の決定である。ADR に混ぜ込んでいないのは過不足のない切り分けである。

## 軽微な懸念

- **フッタ（`status:` / `関連:`）の慣行は全 ADR の 62%（21 本中 13 本）に留まる。** 実測: `status:` 行を持つのは 13 本、持たないのは 8 本（`ADR-config-default-fallback-references` `ADR-config-dir-env-seam-rejected-alternatives` `ADR-race-check-predicate-and-norm-hardening` `ADR-race-check-simplification` `ADR-results-presentation-two-stage` `ADR-stage3-split-rule-and-module-naming` `ADR-window-coordinator-split-rule` `ADR-workspace-lints-canary-scope`）。`関連:` 行も同じ 8 本が欠く。plan.md はこの新規 ADR にフッタを付けるかどうかを明言していない。最も近い先例である `docs/adr/ADR-race-check-population-tooling.md`（母集団のツール化という同型の決定）はフッタを持つため、それに倣うのが自然だが、`scripts/governance-check.mjs` にはこのフッタを検査する項目が無い（`grep -n "status:" scripts/governance-check.mjs scripts/governance-check.test.mjs` は 0 件）ため機構による強制ではなく、実装フェーズの裁量に委ねてよい水準の懸念である。
- 見出し行の形式要件（`# ADR-<slug>: <題>` で `<slug>` がファイル名 stem と厳密一致・`scripts/governance-check.mjs:1408-1416` の G-adr-file-names が検査）は plan.md に明記されていないが、実装時に機械的に守れる/govenance:check が拾う項目であり、計画レベルの欠落ではない。

## 要対処

- なし。

## 未検証

- ADR 本体を実際に書いた後、内容が「識別子 vs 内容」の境界決定 1 点に留まり、他の小さな決定（台帳ファイル名・JSON 形式・exit code）を再度書き込んで写しを作っていないか——ADR がまだ存在しないため本レビューでは確認不能。実装時に本人が確認する必要がある。
- ラウンド 2 の他レイヤー（スクリプト・SKILL.md 担当）が、台帳ファイル名や JSON 形式の否定の知識をスクリプト側の契約コメントへ実際に書く前提で計画しているか——このレイヤーの担当ファイル外のため未確認。
