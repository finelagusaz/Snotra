# ci-workflow レイヤー検証（#804 plan.md ラウンド 3）

## 問題なし

- **e2e.yml の行番号はすべて正確**。`:65-66` は flip 済みコメント（`.github/workflows/e2e.yml:65-66`）で「順序制約」とは別トピック、`:67-73` が順序制約コメント本体（`e2e.yml:67-73`）、`:77-80` が first-run 受容の注記（`e2e.yml:77-80`）。ラウンド2の訂正（65-66 と 67-73 を分離）は実ファイルと一致する。
- **`-SeedConfig -RequireResults` を渡す呼び出し点は `e2e.yml:75` の 1 箇所のみ**。grep 結果:
  ```
  .github\workflows\release.yml:83:        run: pwsh -NoProfile -File scripts/smoke-startup.ps1 -ExePath target/release/snotra.exe
  .github\workflows\e2e.yml:75:        run: npm run smoke:egui -- -ExePath target/release/snotra.exe -SeedConfig -RequireResults
  .github\workflows\e2e.yml:82:        run: pwsh -NoProfile -File scripts/smoke-startup.ps1 -ExePath target/release/snotra.exe
  ```
  `release.yml:83` は `smoke-startup.ps1` 呼び出しで、そもそも `-SeedConfig`/`-RequireResults`（`smoke-egui.ps1` 固有のパラメータ）を渡していない——`e2e.yml:82` の startup smoke 呼び出しと引数が一致している。よって release.yml 側は引数の変更を要さない（計画の「変更ファイル一覧」表に release.yml が無いことと整合）。package.json（`smoke:startup` / `smoke:egui` / `smoke:manual` の 3 script）・`scripts/` 配下（`grep -rn "SeedConfig\|RequireResults" scripts/` は `smoke-egui.ps1` 内と `visual-check-colors.ps1:93` のコメント言及のみ）にも他の呼び出し点は無い。
- **release.yml の行番号も正確**。`:53-54` は `cargo build --release -p snotra-settings`（`release.yml:53-54`）で `snotra-settings.exe` を `target/release/` へビルドしている。`:83` は `smoke-startup.ps1 -ExePath target/release/snotra.exe` の呼び出し（`release.yml:83`）。
- **`working-directory` / `CARGO_TARGET_DIR` は e2e.yml・release.yml のどちらにも存在しない**（全文読み取りで確認）。両 workflow ともリポジトリルートで `cargo build --release` を実行するため、`target/release/` の位置は両者で一致する。`SNOTRA_CONFIG_DIR` を `target/smoke-*/profile` へ向ける計画と runner 側の作業ディレクトリに不整合は無い。
- **不変条件 5（5 起動でプロファイル共有）の前提は現行 e2e.yml で成立している**。`Run egui smoke`（`e2e.yml:74-75`）が `Run startup smoke`（`e2e.yml:81-82`）より先に実行され、コメント（`:77-80`）が「上の egui smoke が seed した config.toml が既に在る」と明記している。実ファイルの構造と計画の記述が一致する。
- **不変条件 6 の release.yml 実在確認は正確**。`release.yml:53-54` で `snotra-settings.exe` を `snotra.exe` と同じ `target/release/` へビルドしている。first-run を踏んだ場合、release.yml には `Get-Process` 等での明示的な kill ステップが無いため、leak した `snotra-settings` プロセスの回収は runner（windows-latest, 単一 job）のジョブ終了（VM 破棄）に依存する——計画が「(a) ローカルでは設定 GUI が spawn されて取り残される」と書く前提（CI でも構造は同型で、より安全なのは CI では `snotra-settings.exe` がビルドされない e2e.yml 側だけ）と整合する。
- **撤去する順序制約コメントの `#686` は残す 1 行へ引き継がれる計画になっている**（plan.md フェーズ3: 「プロファイルが分離されたので順序は自由である」ことを 1 行残し、`#686` と `#804` を含める、と明記）。
- **`.github/workflows/` の PostToolUse 沈黙は実際に「何も走らなかった」である**。`.claude/hooks/post-edit.mjs` の `selectChecks`（`post-edit.mjs:118-143`）を実読した結果、判定条件は `.rs` / `Cargo.toml` 系 / `tauri.conf.json`・`config.toml` / `.claude/hooks/**` / `.githooks/**` のみで、`.github/workflows/e2e.yml` はどれにも一致しない（`checks = []`）。`docs/build-commands.md:28` 自身の記述（「割り当ての無いファイル（`*.md`・`scripts/`・`.github/workflows/` 等）の沈黙は『何も走らなかった』」）は実装と一致しており、計画がこの前提を引き継ぐことに問題は無い。plan.md フェーズ5は e2e.yml の変更検証を PostToolUse 沈黙に依拠せず、実際の CI run のログを読む手順（フェーズ5「本 PR 自身の CI（smoke job）が緑になることを確認し、ログの中身まで読む」）で担保しており、二重に安全側。
- **governance:check（ci.yml の `governance-check` job）は `.github/workflows/**` に対しても実質的な検査を持つ**。`G-ci-table`（`scripts/governance-check.mjs:428-477`）が `docs/build-commands.md` の「CI/CD メモ」対応表のコマンド文字列が workflow ファイル内に部分文字列として現れることを検査する。`npm run smoke:egui` / `npm run smoke:startup` の文字列自体は `-SeedConfig -RequireResults` を落としても `e2e.yml:75` に残るため、この検査に新規の赤は出ない。`governance-check` job は `ci.yml:41-45` で paths フィルタも `skip-ci` ガードも持たず全 PR で常時実行される。

## 軽微な懸念

- **plan.md フェーズ5（検証フェーズ）の「本 PR が触る 3 ファイルを含むため自動起動する」という文は、字義通りには正しいが誤読を招きうる**。実際の `paths:` 列挙（`e2e.yml:13-27`）と本 PR が触る 5 ファイルを 1 件ずつ突き合わせると:
  | 触るファイル | `paths:` に一致するか |
  |---|---|
  | `scripts/smoke-egui.ps1` | 一致（`e2e.yml:22`） |
  | `scripts/smoke-startup.ps1` | 一致（`e2e.yml:21`） |
  | `.github/workflows/e2e.yml` | 一致（`e2e.yml:19`） |
  | `scripts/visual-check-colors.ps1` | 不一致（`paths:` に無い） |
  | `docs/build-commands.md` | 不一致（`paths:` に無い） |
  「3 ファイルを含む」という記述自体は「5 件中 3 件が一致し、1 件以上の一致で workflow は起動する」という意味で読めば正確（GitHub Actions の `paths` は OR 条件で、1 件マッチすれば起動する）。ただし文脈だけを読むと「本 PR は 3 ファイルしか触らない」と誤読されうる。実装フェーズでこの 1 文を「5 ファイル中 3 ファイルが `paths:` に一致するため自動起動する」へ言い換えると、次に `paths:` を読む人が誤解しない。機能上の問題ではないため要対処ではなく軽微な懸念とする。

## 要対処

なし

## 未検証

- `CONTRIBUTING.md`「results 窓 show/hide の trace 観測」への参照の実在・整合（plan.md フェーズ4 step「`CONTRIBUTING.md` に...参照がある」） — 本レイヤーの担当ファイル（`.github/workflows/**`・`package.json`）の範囲外（docs レイヤーの担当と判断し、本スカウトでは `CONTRIBUTING.md` を開いていない）
- `scripts/smoke-egui.ps1` / `scripts/smoke-startup.ps1` 内部の行番号（`:297` の `try` 開始位置・`:126-133` の throw メッセージ・`:43-55` のヘッダコメント等、plan.md フェーズ1・フェーズ2が参照するもの） — ci-workflow レイヤーの担当外（scripts レイヤーの担当と判断し、本スカウトでは行単位の照合を行っていない。ファイル自体は存在し、workflow からの呼び出し引数のみ照合済み）
- `label-sync.yml` の内容精査 — grep で smoke 関連キーワードの不在は確認したが、全文は読んでいない（無関係と判断し省略）
