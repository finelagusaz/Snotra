# plan-review — レイヤー: 文書・セーフティネット規範（issue #713 G-workspace-lints）

## ドキュメント同期

- **問題なし**: `mechanized-checks.md` 更新不要の判断は妥当。同ファイルは「旧 Check → G の移行記録」専用（`.claude/skills/health-check/references/mechanized-checks.md:1,3`「読むのは『Check N はどこへ行ったか』を問うときだけでよい」）。`G-workspace-lints` は退去元の旧 Check 番号を持たない完全新設検査であり、掲載対象外。
- **問題なし**: `docs/build-commands.md:121`（governance:check の守備範囲を列挙する散文）の更新不要判断は妥当。実測: `buildChecks` 登録 17 件中、同行の列挙（参照実在・モジュール索引・スキル表・SPEC 番号・rules glob・コマンド写像・恒久規範の面積 ratchet・見出し参照の着地）は `G-architecture-table` / `G-config-reachability` / `G-check-skill-enumeration` / `G-adr-file-names` / `G-adr-citations` / `G-stale-identifiers` の 6 件を**既に**含んでいない（`scripts/governance-check.mjs:1359-1375` の `id: "G-…"` 列と突き合わせ）。この行は元々非網羅の例示列挙であり、`G-workspace-lints` を足さないことは既存の欠落パターンと整合し新たな破綻ではない。
- **問題なし**: `npm run governance:check` 自身が新しい文書変更で赤くならないか。(1) `ALWAYS_LOADED_FILES = ["CLAUDE.md", "AGENTS.md"]`（`scripts/governance-check.mjs:542`）に `docs/build-commands.md` は含まれず、rules 面積（`.claude/rules/*.md`）にも属さない → G-area-budget 対象外。(2) 追加句の `` `npm run governance:check` `` はバッククォート内だが `/` を含まないため G-references の対象外（同ファイル `:186-191` `if (!t.includes("/")) continue;`）。`G-workspace-lints` はバッククォートなし平文なので同様に対象外。(3) 追加句は `「」` を使わないので G-heading-refs / G-near-heading-refs の走査対象外。ID 表記の様式（バッククォートなしで括弧内に ID）は既存の `docs/build-commands.md:27`「G-hook-commands・#589」・`:191`「G-ci-table」と同型で逸脱なし。
- **問題なし**: research.md の引用訂正（`.claude/rules/safety-nets.md` に「カナリアで守るのは沈黙する経路だけでよい」は存在せず、実在は `.claude/skills/retrospective/SKILL.md:61`）を grep で確認済み。`safety-nets.md`（全 37 行）に該当文字列は無く、`retrospective/SKILL.md:61` に完全一致で存在する。訂正は正しく、対処不要。
- **問題なし**: `SPEC.md` を grep（`lint|governance|workspace.lints|Cargo\.toml`）した結果ヒット 0 件。SPEC.md は製品挙動のみを記述しており、CI/governance の内部検査に関する記述を元々持たない。計画の「SPEC.md 更新不要」判断は妥当。

## 規範の変更にあたるか

- **問題なし**: `.claude/rules/safety-nets.md`「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」への適合。計画 Phase 3 は `snap()` の最小フィクスチャにのみ変異を注入し「稼働中の `Cargo.toml` は一切変異させない」と明記（`plan.md:41`）。実 git 操作・実ファイルを経由しない形はこのルールの手本（`.githooks/githooks.test.mjs`）と同型。
- **軽微な懸念**: `/norm-review` 起動要否について計画は言及していない（セルフレビュー節は空・`plan.md:88-90`）。独自の判定: `.claude/rules/safety-nets.md`「セーフティネットが『規範』の場合」の条項は `.claude/rules/` `.claude/skills/` および規範文書へ**判定を足す変更**で起動し、「索引の追随・改名」には仕事が無いとする。本変更の主体は `scripts/governance-check.mjs`（コード・機構であり規範ではない）であり、`docs/build-commands.md:26` への一句追加は「G-workspace-lints という検知経路が存在する」という事実の追記であって、読者に新しい判断基準を課す文ではない（＝索引の追随に近い）。したがって `/norm-review` は不要と判断できるが、**計画がこの判断を明示していない**点は詰めが甘い。対処は必須ではないが、Phase 4 のチェックリストに一行「`/norm-review` 不要（規範への判定追加ではなくコード検査のため）」を足すと根拠が残る。

## スコープ

- **問題なし**: crate を新設した開発者が「opt-in を忘れると CI が赤くなる」に気づく経路は、PR CI の `governance-check` job 自体である（`skip-ci` 非対象で常時実行・`docs/build-commands.md:181,188`）。計画の finding メッセージ（`plan.md:33`「`[lints] workspace = true` が無い（ルート `[workspace.lints]` の deny がこの crate だけ黙って無効になる・#713）」）が原因と対処を明示しているため、追加の事前告知ドキュメントは過剰。文書変更は 1 行追加のみで、不足・過剰のいずれでもない。
