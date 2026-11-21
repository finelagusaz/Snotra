# 調査 — issue #1493 工程の継ぎ目に契約を置き、計画の承認をリスクに応じて変える

## issue の要約

開発フローの継ぎ目のうち、送り手の状態・受け手の機械的な確認・停止をそろえて持つのは start-issue → implement（`workspace/plan.md`）だけである。次の 5 点を変える。

1. `/implement` に PR 作成の工程を足し、責務を「実装・単体試験・PR 提出まで」にする（description も）
2. `.github/pull_request_template.md` を PR 本文の契約の正本として書き直し、`/implement` と `/deps-update` から指す
3. `/implement`「1b. 分岐」に、「検討:」「保留」の issue を `/start-issue`「Step 1」の判定へ渡す行を足す
4. 計画の承認をリスクに応じて変える（仕様変更・戻せない変更・人の裁定のどれかが立てば人間承認、どれも立たなければ自己承認）。自己承認の記録は `/implement`「1a」が機械判定できる形にする
5. `/plan-review`「リスク判定」の高リスクは独立レビューを厚くする理由として残し、人間承認の要否と切り離す

スコープ外: マージの自動化、#1494〜#1498。

## 関連するファイル・シンボル

- `.claude/skills/implement/SKILL.md`
  - フロントマター `description`:「実装からコミット作成まで自律的に行う」
  - 「1a. workspace/plan.md の由来を確かめる」の 3 条件のうち「人間承認（機械判定）」: `## 人間レビュー` 節に `- [x] 承認済み — <日付> / 問い: "…" / 回答: "…"` が 1 件。「エージェント自身の判断や、コミット済みであることを、承認の代わりにしない」
  - 「1b. 分岐」の表（行き先を決めるのはこの表だけ）。「同一性は立つが、未確定ゼロか人間承認が立たない」→ 1c-B
  - 「1c. 止まったときの引き渡し」1c-B:「`/implement` が未確定を潰したり、承認を自分で記録したりしてはならない」
  - 冒頭の段落「このスキルは計画を所有しない。動くのは、『未確定がゼロで、人間が承認済みの計画がある』か『計画が要らない』かのどちらかのときだけである」
  - 「Step 4 — 仕上げ」: 否定の知識は「PR がまだ無いなら、PR 本文へ転記するまで撤去コミットのメッセージに書いておく」。最後の要約から `workspace/` の外が動いたらツールを回し直す
  - 「出力」節の報告テンプレート（入口・停止・検証・レビュー・テスト・コミット）。PR の欄は無い
  - フロントマター `allowed-tools`: `Bash(cargo *)`・`Bash(npm *)`・`Bash(git *)`・Read・Edit・Write・Grep・Glob・Agent・Skill。`gh` が無い
- `.claude/skills/start-issue/SKILL.md`
  - 冒頭「明示的な人間の承認を得るまで、実装へ渡さない」
  - 「Step 1」: 「検討:」「保留」の判定（#1491 で追加）
  - 「Step 4」: `## 人間レビュー` の雛形 `- [ ] 承認待ち`。作業項目にコミット以降（push・PR 作成・マージ）を置かない
  - 「5c. 人間の注釈と承認」: 承認の記録形式。高リスクなら #1300 へ試行の行をコメント
  - 「Step 6」:「未確定欄に `- [ ]` がなく、人間レビューが承認済みであることを確かめてから」
  - 「出力」6・7 と「引き渡し契約」
  - フロントマター `allowed-tools` に `Skill` がある（`/implement` を呼べる）
- `.claude/skills/plan-review/SKILL.md`「リスク判定」: 高リスクの 6 条件。通常リスクは自己レビューだけ、高リスクは Step 2b（#1300 の試行）
- `.claude/skills/deps-update/SKILL.md`「Step 4 — コミット & PR」: push → `gh pr create`、「PR 本文に更新一覧を載せる。major bump は ⚠ で明示する」。user 起動専用（`disable-model-invocation: true`）
- `.github/pull_request_template.md`: 「対応Issue（`- Closes #<issue_number>`）」「変更内容」の 2 節だけ
- `.claude/hooks/pre-bash.mjs`: `gh pr create` 検出後に (1) 空 PR (2) `workspace/plan.md` の未チェック項目（`readPlanState`。plan.md が無ければ管轄外）(4) ツールの要約の sha 以降に `workspace/` の外の変更（`readVerifyState`）を止める。`judgeCommandShape` は全コマンドで heredoc を拒み、`--body-file` を案内する
- `scripts/merge-pr.mjs`: `closingIssuesReferences` を `--closes` 宣言と完全一致で照合
- `AGENTS.md`「開発ワークフロー」1（バグか仕様変更か）・「条件別チェック」の「永続形式・識別子/キー形式を変更 → `/persistence-check`」
- ルート `CLAUDE.md`: 最重要ルール 2（セーフティネットの PR 本文冒頭に「マージは合意してから」）・「利用できるスキル」表（user 起動専用 4 本）
- `.claude/rules/safety-nets.md` フロントマター `paths`: `.claude/skills/**` を含み、`.github/pull_request_template.md` は含まない（`.github/workflows/**` だけ）

## 再利用できる既存のパターン

- 継ぎ目の契約の手本: start-issue → implement の `plan.md`（状態の宣言・機械判定・fail-closed）
- PR 作成の手本: `/deps-update`「Step 4」（コミット → `verify:tree` → push → `gh pr create`）
- 事実上の PR 本文の型（#1480・#1485・#1487・#1491・#1492 を読んだ）: 冒頭の「**マージは合意してから**（対象のパス）」、概要・変更・理由・検証・残したもの（⚠️）、末尾の帰属行。対応 Issue は「Closes #N」を冒頭に置く版と、テンプレートの節を使う版とが混在
- 共通の契約は経路ごとの手順に書かず、共有の場所に持たせる（`RETROSPECTIVE.md`「`/deps-update` の経路では、検証の worktree がマージ後も残った」→ マージツールへ寄せた #1359）
- スキルからスキルの節を見出しで指す作法（implement は start-issue の節を複数指している）

## 技術的な制約

- `.claude/skills/**` はセーフティネット。マージは合意してから（PR 本文冒頭に明記）
- 規範文書（ルート `CLAUDE.md`・`AGENTS.md`）は常時ロード面。手順の本文を置かない（`/retrospective` の不変条件）
- ガバナンス文書の変更なので `npm run governance:check` を回す。G-references は `/` を含むパスの実在を照合する。見出しは照合されないので、見出しを直したら旧名を `git grep` する（`.claude/rules/governance-docs.md`）
- 文書へ書き足すときは書く約束（かぶりなく・必要なことだけ・古い情報を残さない）を守る
- PR 本文は `--body-file` で渡す（heredoc は hook が拒む）。`--body-file` を渡すと GitHub のテンプレートは使われない（`gh pr create` は `--body`/`--body-file` があればテンプレートを読まない）
- `/implement` の `allowed-tools` に `gh` が無い。PR 作成を足すなら `Bash(gh *)` が要る
- hook (2) は `workspace/plan.md` が無ければ管轄外。`/implement` Step 4 で `workspace/` を撤去した後に PR を作る順序なら、(2) は走らない（撤去の前に全項目 `- [x]` を確かめる手順が代わりに守っている）
- 実装の承認の記録を機械で確かめているのはスキル文書だけで、hook・スクリプトには無い（`承認済み` の grep は `.claude/skills/implement`・`.claude/skills/start-issue` と `docs/superpowers/` の過去の記録だけ）

## 敵対的調査（3b）の反映

出典: `workspace/adversarial-1493.txt`

- 採る: 「事実上の PR 本文の型」は統一されていない。直近 14 本で節名が割れ（概要/変更/検証・対応Issue/変更内容・何を変えたか/なぜ）、検証節の無い PR が 3 本、「残したもの」は 1 本だけ。安定しているのは合意行（該当時）と「概要」程度である。→ テンプレートの節は実践の写しではなく選ぶもので、その選択を計画に明記する
- 採る: 「テンプレートを読まない」は部分的に偽。14 本中 5 本がテンプレートの見出しを使っている。→ 主張を「`--body-file` の経路ではテンプレートが本文へ入る保証が無い」へ弱め、手順は「テンプレートを Read して節に従い本文ファイルを書く」にする（gh の挙動に依存しない形）
- 採る（⚠️ 由来）: `gh pr create` が `--body-file` でテンプレートを読まないかは、gh 2.105.0 の help に明記が無く未確認。上の手順なら結論に依存しないので、未確定から外す
- 採る: `allowed-tools` は事前承認の列挙で、無ければ確認が出るだけ。「`Bash(gh *)` が要る」を「足さないと毎回確認になる」に弱める。足すことは変えない（`/deps-update` と揃える）
- 採る: Q2・Q3・Q4・Q7 は issue と既存文書でほぼ決まっている（Q4 は hook (4) が `workspace/` の外の変更だけを見るので、PR 作成は撤去コミットの後ろに置く）。計画の D2・D3・D4・D7 と一致
- 壊せなかった（維持）: hook (2) は plan.md が無ければ走らない（(4) は撤去後も走る）／承認を見る hook・スクリプトは 0 件／implement に gh なし・start-issue に Skill あり／テンプレートはセーフティネットの paths 外
- 別件として記録: 委譲先は、`gh pr "create"` のように引用符で語を割ると `pre-bash.mjs` の `GH_PR_CREATE` を素通りすると報告した（help の表示だけで PR は作っていない）。本 issue の範囲外。hook 冒頭の「見えないもの」の一覧に引用の内側は「解釈しない（過剰検出は許容）」とあり、過小検出の側の扱いは要確認

## 未解決の疑問

- Q1 自己承認の記録形式と、`/implement`「1a」の判定をどう両立させるか。「エージェント自身の判断を承認の代わりにしない」「承認を自分で記録してはならない」は `/implement` 側の禁止であり、`/start-issue` が記録する自己承認とは主体が違う。1a の文言をどう直すか
- Q2 「人の裁定」の印をどこに付けるか。未確定欄は引き渡し時に全部 `- [x]` なので、裁定が「あった」ことを解消後も読める形が要る
- Q3 自己承認のとき `/start-issue` が `/implement` を自動で呼ぶか、呼べると案内して止まるか。issue は「そのまま `/implement` へ進む」と書いている
- Q4 PR 作成の工程を `/implement` のどこに置くか（Step 4 の後に Step 5 を足すか、Step 4 に含めるか）。PR を作ったあとのツールの照合（`verify:tree` の要約の sha と HEAD）との順序
- Q5 テンプレートの「対応 Issue」の書き方。`Closes #N` をテンプレートに残すと、`merge:pr` の照合に頼る形は変わらない。何も閉じない PR の書き方（「なし（理由）」）を雛形に入れるか
- Q6 「出力」節に PR の欄（URL）を足すか
- Q7 `/deps-update` が PR 本文の正本を指すとき、「更新一覧・major bump の ⚠」はテンプレートの「変更」節に収まるか
