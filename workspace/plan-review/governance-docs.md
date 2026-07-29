## 問題なし

- `scripts/visual-check-colors.ps1` 内で `-SeedConfig` に言及する行は `:93` の 1 箇所のみ（grep 実測）。`:21`（「results 窓の背景」の目視理由説明で `smoke-egui.ps1` の入力注入機構に触れる）も `smoke-egui.ps1` を名指すが `-SeedConfig` とは無関係（`SendInput` の話）で、`-SeedConfig` 撤去後も文意は壊れない。ゆえに計画の「`:93` の 1 行のみ」という範囲設定は正確。
- `docs/build-commands.md` の「スモーク運用メモ」節で `-SeedConfig` に言及するのは `:159`（`smoke-egui.ps1` の説明）`:160`（results 検査の skip 条件）`:161`（`-RequireResults`・順序制約）の 3 bullet で全て（grep 実測）。計画フェーズ 4 の 3 bullet 指定はこの構造と一致する。
- `docs/superpowers/**` を母集団から除外する根拠は明確に存在する: `scripts/governance-check.mjs:1036`「`docs/superpowers/` は歴史資料（#589 で非規範化）ゆえ除外」、`:1041,1051,1055` で G-references/G-spec-sections/その他 md 母集団から `!f.startsWith("docs/superpowers/")` を明示除外、`:1435` に同様の受容コメントあり。実際 `-SeedConfig` へ言及する `docs/superpowers/plans/2026-07-25-*.md` 2 本はこの母集団外であり、計画が触れなくても governance:check には現れない。
- `.claude/rules/safety-nets.md` の `paths` に `.github/workflows/**` が含まれる（`:6`）ため、`e2e.yml` を編集すれば同 rule が自動配送される。計画のフェーズ 5 は同 rule「効いていることは、フォールトインジェクションで一度は実測する」に沿ってフォールトインジェクション A/B と「CI 自身の緑を鵜呑みにせずログを読む」を明記しており、この観点は満たされている。
- `CONTRIBUTING.md:92` の「results 窓 show/hide の trace 観測」という句は `docs/build-commands.md:160` の対応注記と一致しており、計画が触れなくても現状は整合している（`-SeedConfig`/`-RequireResults` 固有の記述は含まないため今回の撤去でも壊れない）。
- `G-stale-identifiers`（`scripts/governance-check.mjs:1242` の母集団定義）は `.claude/skills/**` `.claude/rules/[^/]+.md` `.claude/agents/[^/]+.md` に限定され、`docs/*.md` や `docs/adr/**` を見ない。よって下記「要対処」で挙げる 2 件の取りこぼしは governance:check では機械的に検出されない。

## 軽微な懸念

- 計画（`workspace/plan.md`）自体が「触らない」節と「フェーズ 1」節の両方で `smoke-egui.ps1:78-81` の相互参照コメントに触れており（`:22` と `:42`）、内容は矛盾しないが同じ編集意図を 2 箇所に書いていて計画の肥大に寄与している。ラウンド 1 の指摘を吸収した結果、「触らない（根拠つき）」節が実質「軽く触る」項目まで抱えるようになっており、節の名前と中身がややずれている。

## 要対処

- **`docs/build-commands.md:45`（カテゴリ C 節）に `-RequireResults` への言及があるが、計画のフェーズ 4 編集箇所（`:159`〜`:161` の 3 bullet）に含まれていない。** 該当文: 「この 1 事例は `-RequireResults` が機構化した（#686・下記）が、『緑』が『検査が走った』を意味しない形は他にも作れる」。`-RequireResults` パラメータを撤去すると、この文は存在しない識別子を指したまま残る——ラウンド 1 で `visual-check-colors.ps1:93` について指摘したのと同じ種類の腐りが、計画の grep 範囲（「スモーク運用メモ」節）の外で再発する。`G-stale-identifiers` は `docs/*.md` を母集団に持たないため governance:check でも捕捉されない。フェーズ 4 に「`:45` の文言を、無条件化後の呼称に合わせて書き換える」タスクを追加する必要がある。
- **`docs/adr/ADR-config-dir-env-seam-rejected-alternatives.md` §3（`:33`）が「`-RequireResults` ゲート」を却下理由の一部として名指ししているが、計画はこのファイルを「変更ファイル一覧」にも「触らない（根拠つき）」にも一切挙げていない。** 計画本文（`plan.md:5`）は「ADR §3 の却下理由のうち『2 つの seed は同型ではない』は今も真」と**1 つの理由だけ**を検算しているが、§3 にはもう 1 つの理由（「`smoke-egui.ps1` は `e2e.yml` の `-RequireResults` ゲートに載る CI 経路であり…リスクを負う」）があり、この PR で `-RequireResults` フラグそのものが消える。決定（seed 共有はしない）自体は他方の理由で支持されるため覆らないが、**この文が名指す機構は本 PR で消滅する**。ADR は今後も #843 等から参照される規範であり、消えたフラグ名を理由文に残したままにするかどうかは未確定のまま計画に落ちていない。最低限、計画の「触らない」節に ADR ファイルを明示的に追加し、「§3 の `-RequireResults` への言及は更新するか、歴史的記述として残すか」を未確定欄で裁定すべき。

## 未検証

- `docs/build-commands.md:45` の書き換え方（具体的な代替表現）とそれによる周辺文（「#686・下記」の「下記」がどのバレットを指すか）の整合 — 実際に編集を書き下していないため、書き換え後に文意が破綻しないかは未検証。
- ADR §3 を編集する場合の `governance-docs.md`（`.claude/rules/governance-docs.md`）の正準形ルール適合——ADR は同 rule の `paths`（`docs/adr/**`）に含まれるため、編集すれば自動配送される。実際に編集が入った場合、他ドキュメントへの参照が正準形 `` `<file>.md`「<見出し>」 `` を満たすかは、その時点の diff を見ないと判定できない。
