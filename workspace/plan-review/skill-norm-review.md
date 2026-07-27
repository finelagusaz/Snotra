# skill-norm-review スカウト報告

担当: `.claude/skills/norm-review/SKILL.md`（plan.md の唯一の変更ファイル）

## 1. 問題なし

- **影響範囲の数え上げは正確**: `grep -rn "norm-review" C:/workspace/Snotra` 実行結果、本文（見出し参照ではない散文含む）を引く箇所は `.claude/rules/safety-nets.md:26`・`docs/adr/0009-command-shape-norms-in-hook.md:27`・`.claude/hooks/pre-bash.test.mjs:763`（コメントのみ）・`scripts/governance-check.mjs:585`（履歴コメント）・`RETROSPECTIVE.md`（今サイクル記述）・自身（`SKILL.md:2,51`）のみ。research.md の主張どおりで漏れなし。
- **I2 成立を実測で確認**: `docs/adr/0009-command-shape-norms-in-hook.md:27` の参照は `` `/norm-review`「Step 3」 ``。`scripts/governance-check.mjs:716,727` の `HEADING_REF`/`normAnchor` は前方一致照合であり、"Step 3" は現行見出し `## Step 3 — 成立した指摘だけを塞ぐ`（`SKILL.md:38`）の正規化前方一致に当たる。変更 2 は見出し文言を変えず箇条末尾へ 1 文を接ぐだけなので、この参照は保たれる。
- **I3 成立を実測で確認**: `npm run governance:check` 実行結果 `常時ロード 13274/13374 字`。`scripts/governance-check.mjs:657-661` の `alwaysTotal` は `ALWAYS_LOADED_FILES`（`CLAUDE.md`/`AGENTS.md`）+ `skillDescriptionArea`（各 skill の `description` フィールドのみ）で決まり、`SKILL.md` 本文は算入対象外（`skillDescriptionArea` は `description:` 行だけを読む・`governance-check.mjs:643`）。本文だけを触る変更は面積を動かさない。
- **I1 成立を確認**: `collectAnchors`（`governance-check.mjs:723`）の太字リード捕捉は `^\s*(?:[-*]|\d+[.)])\s+\*\*(.+?)\*\*` で、箇条**先頭**の `**...**` だけを捕まえる。変更 2 は `SKILL.md:42` の箇条**末尾**（「先に問う」の後）に文を接ぐ設計であり、先頭リード「免除句は名指しした対象しか守らない。」は逐語で残る。ADR-0009:27 はこのリードを名指ししていないため現時点で被参照は無いが、計画の記述どおり。
- **I5 の懸念は実際の配置には当たらない（誤りではなく計画の記述が正確）**: 変更 1 で新設される他文書参照（`` `/plan-review`「Step 3 — 結果の統合と報告」 ``）はフェンス**外**の段落（`:64` 相当）に置かれる設計であり、`linesOutsideFences`（`governance-check.mjs:60-71`）はここを除外しない。実際に `/plan-review`「Step 3 — 結果の統合と報告」見出しは `plan-review/SKILL.md:114` に実在し、正規化前方一致で解決する。**この新規参照は G11 が検査する**——I5 が「検知手段は無い」と書くのはフェンス内に書いた場合の仮定であり、計画が実際に配置する場所（フェンス外）には当たらない。矛盾ではないが、Phase 2 の証跡記録時に「この参照が G11 の 121 件 → 122 件に反映される」ことを一言添えると精度が上がる（軽微）。
- **G8 不要判断は妥当**: `.claude/skills/norm-review/SKILL.md` に `disable-model-invocation` は無い（`grep -n disable-model-invocation` で該当なし）。`governance-check.mjs:417,433` の `modelHiddenSkills` はこのフィールドを持つ skill だけを G8/G10 の「表が索引すべき対象」母集団に入れる。`norm-review` は対象外であり、`CLAUDE.md`「利用できるスキル」表への追加不要は正しい。
- **`.claude/hooks/pre-bash.test.mjs:763` は SKILL.md 本文に依存しない**: 該当テスト（`:745-786` 読了）は `judgeCommandShape` の拒否文言のみを検証し、`/norm-review` はコメント中の由来記述のみ。`npm test` 全件走行の判断は保守的で問題ないが、依存が無いことは確認済み。
- **SPEC.md 更新不要は妥当**: `grep -n norm-review SPEC.md` → 0 件。SPEC.md はプロダクト（Snotra 本体）の意図のみを持ち、エージェント運用規範への参照は無い。
- **issue #775 とのスコープ一致**: `gh issue view 775` の提案 (a)（等式を様式へ置く）と「あわせて記録する観察」（機構への言及の同族性）の 2 点が、plan.md の変更 1・変更 2 とそれぞれ 1 対 1 で対応する。過不足なし。(b)（`Write` を足す）の却下は issue 本文にも「`/norm-review` は現在 `Write` を持たない」との記述があり、plan.md の却下理由（`/plan-review` が同穴を意図的に閉じている）と整合。
- **`.claude/rules/governance-docs.md` の正準形規約に適合**: 新設参照 `` `/plan-review`「Step 3 — 結果の統合と報告」 `` は対象 `<path>.md` 形の正準形（同ファイル `:9`）に合致し、序数のみでの参照ではない。

## 2. 軽微な懸念

- Phase 2 の証跡記録（`workspace/plan.md:41`）に「G11 照合件数」を残す指示はあるが、現況 121 件からの増分（新規参照 1 件で 122 件になる想定）を明記していない。実装後の記録時に期待値を添えると答え合わせが楽になる。根拠: 現況 `npm run governance:check` 出力「見出し参照 121 件を 55 文書から照合」。

## 3. 要対処

なし。

## 4. 未検証（理由）

- **実際の編集後の `governance:check` 再実行はこのスカウトでは行っていない**（担当は現状ファイルの静的検証であり、実装前の計画レビューという役割上、対象ファイルを編集する権限を持たない——出力先は本ファイル 1 つのみと指示されている）。Phase 2 でのオーケストレーター自身の実行が必要。上記の判定は regex の静的トレースによる推論であり、`npm run governance:check` を変更後に実行するまでは経験的検証ではない。
- **Phase 3（自己適用 `/norm-review`）の結果は本レビューの範囲外**——2 クラスの読者が新文言に対してどう反応するかは、実装後の実行でしか測れない（plan.md I4 が明記する通り）。
