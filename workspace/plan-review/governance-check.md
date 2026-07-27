# plan-review — governance-check レイヤー

## 問題なし

- `AREA_BUDGET.alwaysLoaded`/`.rules` は `scripts/governance-check.test.mjs:433,439` から**動的に**（`AREA_BUDGET.alwaysLoaded + 1` 等）参照されており、定数値そのものを引き下げてもテストは壊れない。ハードコードされた期待値も無い（`grep -n "14058\|8056" scripts/governance-check*.mjs` は定数定義行と由来コメントのみに一致）。
- 常時ロード面の母集団は `ALWAYS_LOADED_FILES = ["CLAUDE.md", "AGENTS.md"]`（`scripts/governance-check.mjs:542`）+ `skillDescriptionArea`（skill description）のみで、`docs/hooks.md` と新規 `docs/adr/0006-*.md` はいずれも入らない。Phase 4 で増える側（`docs/hooks.md` の追記・ADR 新設）は課税されないため、計画の「Phase 4 完了後に実測 + 100 字で引き下げる」は増加分を無視してよい前提と一致する（ADR-0005 §決定4「skills 本文・モジュール CLAUDE.md・docs・ADR は対象外」）。
- 本 PR は `.claude/skills/**` の `description` を変えないため skill 分の常時ロード面は不変（`disable-model-invocation: true` の 3 件除外ロジックにも影響なし）。
- Phase 4 が `CLAUDE.md`「シェル環境（Windows / PowerShell）」節を削除しても G11（見出し参照）は壊れない。正準形 `` `CLAUDE.md`「シェル環境（Windows / PowerShell）」`` で参照する文書は `docs/superpowers/plans/2026-07-24-su6.5-flip-hardening.md:20` のみで、`headingRefDocs`（`scripts/governance-check.mjs:801-805`）が `docs/superpowers/` を除外するため母集団に入らない（research.md の主張どおり）。独自に `.superpowers/sdd/task-9a-brief.md:37` の類似参照も grep したが、バッククォート直後に「」が続く正準形（`HEADING_REF` 正規表現・`governance-check.mjs:702`）に一致せず散文形（受容する偽陰性）だったため、こちらも無風。
- 「実測 + 100 字」の許容差算式は、由来コメントの既存 6 件のうち引き下げ方向の 3 件（16028→14712・14712→14261・14261→14058）すべてで `basis - measured = 100` になっており、引き下げでも同じ算式を使う計画（Phase 5）は前例と一致する（ADR-0005 は算式を引き上げ・引き下げで区別していない）。
- `.claude/rules/**` は計画の変更ファイル一覧（plan.md:19-26）に含まれず、`rules` 面の基準を据え置くという D 判断（plan.md 明記なし・研究の前提）は妥当。
- `.claude/hooks/post-edit.mjs` の `selectChecks`（`.claude/hooks/post-edit.mjs:108-133`）を読んだ結果、`scripts/governance-check.mjs` と `scripts/governance-check.test.mjs` はいずれも `CHECK_DEFINITION`（`.claude/settings.json`/`package.json`/`vitest.config.ts`/ルート `Cargo.toml`）にも `.claude/hooks/` 接頭辞にも当たらず、拾う `checks` は空。計画の「`scripts/*.mjs` の編集には検査が割り当てられていない」は実装ベースで正しい。
- `npm test` は `vitest run`（`package.json:8`）で、`vitest.config.ts` の `include` に `"scripts/**/*.test.mjs"` があるため `scripts/governance-check.test.mjs` は含まれる（Phase 6 の `npm test` 緑確認で実際に走る）。

## 軽微な懸念

- 由来コメント既存 6 件のうち日付入り 4 件（2026-07-26 以降）はすべて起因 issue/PR 番号（#725・#749・監査系は番号なしだが文脈で自明）を伴うが、計画（plan.md:101）は「引き下げ幅・実測値・理由」のみを明記し #768 の明示を求めていない。前例と完全に揃えるなら「#768 の機構化で 5 件を吸収」のように issue 番号を含めるのが望ましい（必須ではない・過去にも番号なし entry あり）。

## 要対処

- なし。

## 未検証（理由）

- 引き下げ後の実測値（新 `AREA_BUDGET.alwaysLoaded` の具体的な数字）は Phase 1〜4 の実装（`.claude/hooks/pre-bash.mjs` の変更・`CLAUDE.md` の実削除）が完了していないと確定しない。今回はソースコード・テスト・ADR・precedent のみを読む静的レビューであり、`npm run governance:check` を実行して実測を取ることはしていない（計画の担当範囲は Phase 5 であり実装前提のため）。
- 由来コメントの文言そのもの（Phase 4/5 完了後に実際に追記される文章）は未執筆のため、既存 6 件との様式一致は「要素構成（日付・幅・実測値・理由）」の観点でのみ照合した。実際の文章表現（語尾・粒度）の照合は執筆後でなければできない。
