## 問題なし

- **ADR ファイルの実在と §3 の内容**: `docs/adr/ADR-config-dir-env-seam-rejected-alternatives.md` は実在する（`docs/adr/` 一覧で確認）。§3（`:31-39`）は「却下（重複を選ぶ）。`smoke-egui.ps1` は `e2e.yml` の `-RequireResults` ゲートに載る CI 経路であり…」（`:33`）と実際に `-RequireResults` を名指ししている。決定 (g) の前提は正しい。加えて GitHub issue #843 の本文（「`ADR-config-dir-env-seam-rejected-alternatives.md` §3 の扱い」節）が独立に「`-RequireResults` ゲートに載る CI 経路」の理由を**「失効」**と評価しており、plan.md の追記予定文言（「その後（#804）: `-RequireResults` は撤去され、この理由は失効した」）と一致する。二重の裏取りが取れている。
- **`docs/build-commands.md` の行番号照合**: 3 箇所とも実際にその内容を指している。
  - `:45` — 「この 1 事例は `-RequireResults` が機構化した（#686・下記）が…」（カテゴリ C 節、フェーズ 4 が名指しする箇所と一致）
  - `:160` — 「`scripts/smoke-egui.ps1` は results 窓の表示も検査する…索引内容を制御できるときだけ…どちらも無ければ results 検査は自動的に skip され…」（「索引内容を制御できるときだけ」の枠組み全体の記述、フェーズ 4 の「`:160` は bullet ごと書き直す」の記述と一致）
  - `:161` — 「**`-RequireResults` は skip を失敗に変える（CI 専用・#686）**…`e2e.yml` では egui smoke を startup smoke より前に置くこと…」（順序制約・フォールトインジェクション手順、フェーズ 4 の「`:161` の `-RequireResults` bullet を書き換える」と一致）
- **`G-stale-identifiers` の母集団は `docs/**` を見ない（plan の主張を検算・確認）**: `scripts/governance-check.mjs:1241-1243` の `staleIdentifierDocs` は `/^\.claude\/(skills\/.*|rules\/[^/]+|agents\/[^/]+)\.md$/` でファイルを絞り込んでおり、`docs/**` は対象外。他の検査（`checkBuildCommands`・`scripts/governance-check.mjs:390-420`）は `npm run <script>` が `package.json` の `scripts` に在るか、`cargo test -p <crate>` の crate が workspace に在るかしか見ておらず、PowerShell パラメータ名（`-SeedConfig` 等）の陳腐化は一切検知しない。**plan の「`governance:check` では捕まらない」という主張は検算の結果、正しい。**
- **PostToolUse の沈黙 = 「何も走らなかった」（plan の前提を検算・確認）**: `.claude/hooks/post-edit.mjs:118-143` の `selectChecks` は `.rs`・`Cargo.toml`・`tauri.conf.json`/`config.toml`・`.claude/hooks/**`・`.githooks/**` にしか検査を割り当てておらず、`*.md`（`docs/build-commands.md`・`CONTRIBUTING.md` を含む）にマッチする分岐が存在しない。**plan のフェーズ 5「`*.md` の編集で PostToolUse は沈黙する＝『何も走らなかった』」という前提は正しい。**
- **`SPEC.md` 更新要否「不要」の判断**: `SPEC.md` 内で `smoke|SNOTRA_CONFIG_DIR|検証スクリプト` を grep すると §13（`:616`）の 1 件のみがヒットし、その内容は #803 で既に入った `SNOTRA_CONFIG_DIR` の一般契約（env ハッチの意味・展開しない等）であって、smoke スクリプトの個別パラメータ（`-SeedConfig` 等）や「smoke」という語自体への言及は無い。製品挙動（`SNOTRA_CONFIG_DIR` そのものの意味）を本 issue は変えないため、plan の「不要」判断は妥当。
- **`CONTRIBUTING.md`**: `-SeedConfig`/`-RequireResults`/`-ResultsQuery`/`$seededNow` の直接言及は 0 件（grep で確認）。`docs/build-commands.md:160` が参照する「results 窓 show/hide の trace 観測」という句は `CONTRIBUTING.md:92`（「`npm run smoke:egui`（egui show/hide + results 窓 show/hide の trace 観測）と…」）に実在し、相互参照は生きている。フェーズ 4 の「実在と整合を確認し、必要なら直す」は、確認した結果「直す必要なし」で閉じられる（この 1 行に `-SeedConfig` 等の識別子は含まれない）。
- **送り先 issue の実在確認**（`gh issue view` で本文照合）:
  - **#845**（OPEN）: 本文は `scripts/smoke-startup.ps1:100` の `$_.event -like "*:error"` が実イベント名に 1 件も一致しないこと、`docs/build-commands.md` の該当記述が実効を持たないことを扱っており、決定 (h) が指す内容と完全に一致する。
  - **#786**（本文確認）: 待機ループが `[index-load]` 行（`[trace]` 前置きでない）で抜けてしまい `event_count=0` のまま緑になる、という症状が本文に記載されており、フェーズ 2 最終項目が触れない理由（プロファイル分離と独立）と整合する。
  - **#843**（OPEN）: 本文の「#804 との分担」表が「射程: #843（後）＝共有モジュール本体・キー注入とキャプチャの結合・`check:colors` の results 背景判定・Pester・safety-nets の `paths`」と明記しており、決定 (c)（seed 健全性検査を共有ヘルパー化しない・射程は #843）の裏取りが取れる。
- **フェーズ 5 のカテゴリ列挙（A/B/D/E 該当なし、C/F 該当）**: `docs/build-commands.md` の各カテゴリ定義を実読して照合。A（`:11`、`*.rs`）・B（`:31`、TS）・D（`:47`、UI スタイル）・E（`:107`、`.githooks/**`）はいずれも本変更の対象ファイル（`scripts/*.ps1`・`.github/workflows/e2e.yml`・`docs/build-commands.md`・ADR）に当たらず、plan の「該当なし」は正しい。C（`:35-45`、`npm test`/`smoke:startup`/`smoke:egui` を要求）と F（`:118-125`、ガバナンス文書＝`governance:check`）は該当し、plan のフェーズ 5 が実際にこの 2 つを実行対象として列挙している。

## 軽微な懸念

- **`docs/superpowers/plans/*.md` への言及箇所を plan が一切名指ししていない**が、`docs/superpowers/README.md:1,3,6` が「このディレクトリは**歴史資料**であり、**現在の仕様ではない**」「鮮度維持の対象外: `/health-check`・`governance:check` はこのディレクトリを検査しない…ここの記述と実装の乖離は欠陥ではない」と明言しているため、実質的な問題ではない。該当箇所（grep 実測。行数が多いため代表を挙げる）:
  - `docs\superpowers\plans\2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md`（`-SeedConfig`/`-ResultsQuery`/`$seededNow` を計 20+ 箇所。`-SeedConfig` の元実装計画そのもの）
  - `docs\superpowers\plans\2026-07-25-646-pr2-results-window-split.md:779`（`-SeedConfig`）
  - `docs\superpowers\plans\2026-07-25-pr-a-prime-results-window-newtype.md:29,531`（`-ResultsQuery`）
  - `docs\superpowers\plans\2026-07-25-pr-b-read-visual-snapshot.md:40,525`（`-ResultsQuery`）
  結論として更新は不要（ADR と同じ「当時の決定文脈のまま」の扱いだが、こちらは README が明文で保証している分さらに強い）。plan の「触らない（根拠つき）」節にこの 1 行（`docs/superpowers/**` は歴史資料ゆえ対象外・根拠 README）を足しておくと、次のレビュアーが同じ grep を再実行して同じ疑問を持たずに済む——実装上の変更は不要。
- **`-SeedConfig`/`-RequireResults` の母集団に関する plan の言い回し「`.claude/**` の md だけ」は厳密には過大**: 実際の regex は `.claude/skills/**`・`.claude/rules/*.md`・`.claude/agents/*.md` の 3 系統のみで、例えば仮に `.claude/foo.md` や `.claude/hooks/*.md` があったとしても対象外（現状該当ファイルは無い）。結論（`docs/**` を見ない・governance:check では捕まらない）自体は正しく検算済みなので実害はない。
- **issue #843 本文が `docs/build-commands.md:147` を #804 の射程として引用している**が、現在の `:147` は `cargo run -p snotra` の行（無関係）で、plan.md が名指しする `:45`/`:160`/`:161` とは異なる。plan.md 自体の記述ではなく #843 側の記述だが、issue 本文の行番号引用が実ファイルと既にずれている実例であり、行番号引用一般の陳腐化リスクの傍証として記録しておく。

## 要対処

- なし（フェーズ 4 が名指しする 3 行番号・ADR の内容・機構の射程主張はすべて実測で裏付けが取れた）

## 未検証

- `docs/build-commands.md:161` の書き換え後の具体的な文面（フェーズ 4 は「順序制約と `-RequireResults` の記述は撤去し…」という方針のみを示し、確定文面は実装時に書く）が、周辺の見出し参照（`CONTRIBUTING.md` との相互参照句など）を壊さないかは実装後でないと確認できない——本レビューは「今の記述が指す内容」の照合までを担当し、書き換え後の文面の妥当性は範囲外とした
- `docs/superpowers/plans/*.md` が governance:check の母集団から実際に除外されているか（README の記述のみを根拠にし、`scripts/governance-check.mjs` 側の除外実装までは追跡していない）
