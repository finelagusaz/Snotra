## 問題なし

- `-SeedConfig`/`-RequireResults` は `e2e.yml` 全体（83 行）中 `:75` の 1 箇所にしか現れない（grep 実測）。他ステップ・concurrency ブロック・artifact 収集への波及は無い。
- `e2e.yml` に `actions/upload-artifact` 等の artifact 収集ステップは存在しない（全文読了で確認）。「artifact 収集が壊れる」経路自体が無い。
- `concurrency`（`group: e2e-${{ github.ref }}`, `cancel-in-progress: true`）はステップ引数・コメントと独立しており、本変更で影響を受けない。
- `paths:` トリガー（`:19,21-22`）は `scripts/smoke-*.ps1` のような glob ではなく、`scripts/smoke-startup.ps1` / `scripts/smoke-egui.ps1` / `.github/workflows/e2e.yml` を**個別ファイル名で列挙**している。本 PR が変更する 3 ファイルすべてが該当するため、smoke-egui job は自動起動する。「paths に含まれず自動起動しない」という懸念は該当しない。
- `cargo build --release -p snotra`（`:63`）は `SNOTRA_CONFIG_DIR` が設定される前に完了する。この env 変数は各 smoke スクリプトのプロセス内でのみ `$env:` 設定され `finally` で復元される想定（plan フェーズ1/2）であり、GitHub Actions の `run:` ステップはステップごとに新しいシェルプロセスを起こすため、ビルドステップとは元々プロセス境界で分離されている。ビルド成果物（`target/release/snotra.exe`）と smoke プロファイル（`target/smoke-egui/profile` 等）がパスとして衝突することもない。
- plan フェーズ5（`:64`）は不変条件1（実 config を汚さない）の検証を「実 config が在る開発機で」実施すると明記しており、CI runner の空の `%APPDATA%\Snotra` に依存する検証にしていない。CI 自体が e2e.yml の回帰を検証する経路は、本 PR 自身が smoke-egui job を自動起動すること（上記）に委ねられている。
- フォールトインジェクション計画（plan フェーズ5・`:66`）は「`smoke-egui.ps1` を一時ディレクトリへ複製し」と明記しており、`.claude/rules/safety-nets.md`「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」の要求と整合する。

## 軽微な懸念

- 撤去対象の順序制約コメント（`:65-73`）には issue 番号 `#686` への参照が2箇所ある。plan は代替として「プロファイルが分離されたので順序は自由である」の1行のみを残す方針（plan `:49`）で、issue 番号は引き継がれない。`git blame` や `#686`/`#804` 自体は残るが、`e2e.yml` 単体を読む将来の読者が「なぜ以前は順序が要ったか」に辿り着く手がかりが薄くなる。
- 新設予定の「順序は自由である」というコメントの主張は、実際に2ステップの順序を入れ替えて CI が green のままであることを経験的に確認してはいない（plan 未確定欄(d)は「入れ替えない」と裁定・入れ替えないこと自体は妥当）。各スクリプトが別プロファイル（`target/smoke-egui/profile` / `target/smoke-startup/profile`）を持つ構造からの妥当な推論ではあるが、恒久コメントとして残る主張が未検証のままである点は留意事項として残る。
- plan フェーズ5に、本 PR 自身の `e2e.yml` 上の smoke-egui job 実行（実際の Windows runner・実際のビルド）が green になることを確認する、という項目が明示的には無い。`paths` トリガーにより自動起動はされる（問題なしの項参照）が、「それを見て合格を判断する」手順としては書かれていない。

## 要対処

なし

## 未検証

- Swatinem/rust-cache（`e2e.yml:55-57`、`workspaces: src-tauri`）が `target/smoke-egui/profile` や `target/smoke-startup/profile` のような cargo 管理外のディレクトリをキャッシュに含めて保存するか、sweep ロジックで除外するかは action 本体のソースを確認していないため未検証。含める場合でも plan フェーズ1「前回の残骸を消す」ステップ（config.toml.bak・*.bin の削除）がスクリプト側で空振り合格を防ぐ設計にはなっているが、キャッシュサイズ肥大の可能性そのものは残る。なお `workspaces: src-tauri` は cargo workspace root がリポジトリルート（`./Cargo.toml`）にあるため実質 `./target` を指すと推測されるが（`cargo metadata` のワークスペース探索はディレクトリを遡る）、これも未確認。
- 「順序は自由である」という主張（軽微な懸念の2点目）— 実際に順序を入れ替えて確認していないため、構造的な妥当性のみで受容できるかは判断が割れうる。
