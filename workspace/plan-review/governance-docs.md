## 問題なし

- `e2e.yml` の行範囲はバイト単位で実測と一致する: 順序制約コメントは実際に 9 行（`.github/workflows/e2e.yml:65-73`）、first-run 受容の注記は実際に 4 行（`:77-80`）。plan.md の行番号引用は正確。
- `-SeedConfig` / `-RequireResults` / 「順序制約」をリポジトリ全体で grep した結果、非 workspace ファイルでの出現は `scripts/smoke-egui.ps1`・`.github/workflows/e2e.yml`・`docs/build-commands.md`・`docs/adr/ADR-config-dir-env-seam-rejected-alternatives.md`・`scripts/visual-check-colors.ps1`（後述）・`docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md` 等の日付付き過去 PR 設計文書のみ。後者は #671/#673 サイクルで `-SeedConfig` を導入した時点の記録であり、他の `docs/superpowers/{specs,plans}/*` 同様に日付固定の過去スナップショットとして扱われている（撤去済み WebView2/e2e の記述を残す先例と同型）。本 issue のスコープ外として計画が触れていないのは妥当。
- ADR §3 の**決定そのもの**（seed 共有ヘルパー化を却下）は #804 で覆らない、という計画の判断は正しい——#804 は seed の書き先（APPDATA→プロファイル）を変えるだけで、2 つの seed（`[[paths.scan]]` の有無）を統合しない。ADR §3 が挙げる「2 つの seed は同型ではない」という却下理由は今も真のまま。
- `CONTRIBUTING.md:92` は実在し（「results 窓 show/hide の trace 観測」というフレーズを含む）、内容は #804 後も偽にならない——results 窓の観測自体は無条件化されるだけで削除されないため。ただし `docs/build-commands.md:160` の参照はバッククォート無しの散文形（`` CONTRIBUTING.md の「...」 ``、正準形は `` `CONTRIBUTING.md`「...」``）で、そもそも `governance-check.mjs` の `HEADING_REF`（G-heading-refs）に照合されない。この行自体が phase 4 で全面書き換え対象なので実害なし。
- `-SeedConfig`/`-RequireResults` 撤去は `G-ci-table`（`checkCiTable`）・`G-build-commands`・`G-stale-identifiers` のいずれも壊さないことをロジックで確認した: `G-ci-table` は `npm run smoke:egui` という部分文字列が workflow に現れるかしか見ず引数は無関係、`G-stale-identifiers` の母集団は `.claude/{skills,rules,agents}/*.md` に限られどのファイルもこれらのフラグ名を参照していない。

## 軽微な懸念

- `docs/build-commands.md:160`（results 検査の bullet）はチェックリストが指示する以上の書き換えが要る。plan.md フェーズ4の項目は末尾の「どちらも無ければ…skip…」文の削除だけを指示するが、同じ bullet の前半「索引内容を制御できるときだけ 1 文字クエリを注入して」と、「「索引内容を制御できるとき」は `-SeedConfig` で…または `-ResultsQuery` で…」という条件説明も、seed が無条件化される以上そのままでは偽になる。チェックリストを字面通り実行すると、条件節だけ残って説明が欠落した文章が残りうる。
- `scripts/smoke-egui.ps1` 冒頭のヘッダコメント（`:43-55`、特に `:47` の「（索引内容を制御できるとき）」、`:52-54` の「-SeedConfig（CI 用）: …既存 config は決して上書きしない」）が phase 1 のチェックリストに明示されていない。同ファイルは大きく書き換え対象なので実装時に気づく可能性は高いが、`-SeedConfig` パラメータ撤去の項目とは別立てのチェック項目になっていないため、レビュー観点としては見落とし得る。
- `docs/adr/ADR-config-dir-env-seam-rejected-alternatives.md` §3 の却下理由の一節「`smoke-egui.ps1` は `e2e.yml` の `-RequireResults` ゲートに載る CI 経路であり」は、`-RequireResults` 撤去後は事実として古くなる（フラグが無くなる）。ADR は決定時点の記録であり継続同期の対象ではない、という扱いなら問題ないが、その判断を計画側は明示していない（「ADR §3 は #804 では覆らない」＝決定は覆らない、という主張のみで、理由文中の識別子が古くなる点には触れていない）。

## 要対処

- `scripts/visual-check-colors.ps1:93` に、ADR §3 が導入した相互参照コメント対のもう半分が残っている: `` `scripts/smoke-egui.ps1` の `-SeedConfig` が同型の seed を持つ（必須セクションの根拠は共通・片方だけ直さないこと）``。plan.md はこのファイルを「触らない（根拠つき）」に分類し、理由を「既に分離済み（#803）。本issueで触る理由が無い」としているが、この理由は line 93 の内容には当てはまらない。#804 で smoke-egui.ps1 側は `-SeedConfig` パラメータ自体を撤去する（phase 1）ため、`visual-check-colors.ps1:93` は存在しない識別子を指す stale reference になる。これはまさに ADR §3 が相互参照コメントを置いた動機（「片方だけ直る事故を防ぐ」）が名指しする失敗パターンそのもの。**対処**: `visual-check-colors.ps1:93` 付近の 1 行を「`-SeedConfig`」ではなく新しい無条件 seed の実態を指すよう更新する（ADR §3 の「共有ヘルパーにしない」という決定自体は変えずに、識別子の呼称だけ直す）。ファイル全体を触らない方針は維持してよいが、この 1 コメントは例外として扱う必要がある。

## 未検証

- `npm run governance:check` を実際に編集後の状態で走らせた確認はしていない（ロジック読解のみ）。plan.md phase 5 で実行予定なので、そこで実測されることを前提とする。
- `docs/superpowers/plans/` `docs/superpowers/specs/` 配下の `-SeedConfig`/`-RequireResults` 言及ファイル群を全件は開いていない（1 件のみ確認）。日付付き過去設計文書という repo の慣行（既に撤去済みの WebView2/e2e 記述を残したまま放置されている先例が多数ある）から見て非同期でよいと判断したが、全件の目視はしていない。
