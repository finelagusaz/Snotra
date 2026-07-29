## 問題なし

- **rust-cache は `target/smoke-egui/profile` を復元しない**——過去の実 CI run ログで実測（`gh run view 30413756317 --log`）: `Rust cache` ステップの `Cache Paths` は `C:\Users\runneradmin\.cargo\{bin,.crates.toml,.crates2.json,registry,git}` と `D:\a\Snotra\Snotra\src-tauri\target` の 6 行のみで、リポジトリルートの `target/`（`snotra.exe` の実際の出力先かつ本計画が `target/smoke-egui/profile` を置く場所）は含まれない。原因はソース側でも確認済み（`Swatinem/rust-cache` の `src/config.ts`: `workspaces` 入力を `path.resolve(root)` → `path.join(root, "target")` と**文字列結合のみ**で解決し `cargo metadata` の `target_directory` を参照しない）。`.github/workflows/e2e.yml:57` の `workspaces: src-tauri` は cargo workspace root（リポジトリルート、`Cargo.toml:1-6` の `[workspace]`）ではなく単なるメンバー crate のパスであり、`src-tauri/target` は実ビルド（`cargo build --release -p snotra` をリポジトリルートで実行）が書き込まない場所。したがって「前回 run の `config.toml`/`*.bin` がキャッシュから復元される」経路は構造的に存在せず、計画の「前回の残骸を消す」ステップが吸収すべき対象は同一 job 内の残骸（無い）に限られる。**この `workspaces: src-tauri` 自体は #804 と無関係な既存設定**（`release.yml:39` も同型）であり、この計画が触る対象ではない
- **`e2e.yml` は `snotra-settings.exe` をビルドしない**——`smoke-egui` job のビルドステップは `cargo build --release -p snotra`（`e2e.yml:62-63`）のみで `-p snotra-settings` を含まない。リポジトリ全体で `snotra-settings` を grep すると `ci.yml:88-89`（`cargo test -p snotra-settings`。exe は生成されない）と `release.yml:53-61,71,87`（release ビルド専用）にしか出現せず、`e2e.yml` には一切現れない。不変条件 6 の前提（CI では `snotra-settings.exe` が存在せず `:not_found` になる）は成立する
- **`paths:` トリガーは本 PR が触る 5 ファイルのうち 3 ファイルを覆い、それで自動起動に十分**——1 ファイルずつ照合: `scripts/smoke-egui.ps1`（`e2e.yml:22` に完全一致・covered）／`scripts/smoke-startup.ps1`（`e2e.yml:21` に完全一致・covered）／`.github/workflows/e2e.yml`（`e2e.yml:19` に完全一致・covered）／`scripts/visual-check-colors.ps1`（`paths:` のどのエントリにも一致しない・**not covered**）／`docs/build-commands.md`（`docs/**` や `**/*.md` のエントリが無く一致しない・**not covered**）。GitHub Actions の `paths:` は変更ファイルのいずれか 1 つが一致すれば起動するため、後者 2 ファイルが covered でなくても job は起動する。計画の記述（`plan.md:76`「本 PR が触る 3 ファイルを含むため自動起動する」）はこの 3/5 の内訳と正確に一致しており誤りではない
- **`-SeedConfig`/`-RequireResults` は `e2e.yml` 内で他に参照されていない**——`Grep` で repo 全体を検索した結果、`e2e.yml` 内の出現は `Run egui smoke` ステップの引数（`:75`）と、それを説明する `:65-73`/`:77-80` のコメントのみ。`ci.yml` には smoke 関連の記述自体が無い（rust-check job は `cargo check/test/clippy/doc` のみ）。撤去で見落としが生じる箇所は他に無い
- **`.claude/rules/safety-nets.md` のフォールトインジェクション要件を計画フェーズ 5 は満たす**——「稼働中のガードを弱めない」の要求に対し、A（`-ResultsQuery ''` を明示指定して赤を出す）はライブスクリプトへ変異を加えず**入力による行使**であり同 rule の適用除外（「意図的に規則違反となる操作を行い、拒否されることを確認する類はガードの行使であり、弱めていないので対象外」）に該当。B（seed 健全性）は明示的に「一時ディレクトリへ複製し」「稼働中のスクリプトを弱めない」と rule の文言をほぼそのまま踏襲している（`plan.md:75`）
- **ステップ順序を維持する判断は妥当**——プロファイル分離後は順序が自由になるが、それ自体は「順序を変えないこと」と両立する。無関係な差分を避ける判断として問題なし

## 軽微な懸念

- **撤去対象の「順序制約のコメント 9 行（`:65-73`）」は無関係な内容を巻き込む**——実測した現在の `e2e.yml:65-73` は次の 9 行:
  - `:65-66`: `# flip 済み（#532 SU7 PR2）: env なし＝「既定が egui であること」自体が検証対象（spec 決定 3）。` + 空コメント行 — これは**順序制約の説明ではなく**、egui smoke ステップに `env:` ブロックが無い理由（デフォルトが egui であること自体を検証している）を述べた別トピックであり、プロファイル分離後も真のまま残る
  - `:67-73`: 実際の順序制約（`-SeedConfig`/`-RequireResults`/#686 の説明）
  計画（`plan.md:56`）は「順序制約のコメント 9 行（`:65-73`）を撤去する」「代わりに...1 行残し」としか書いておらず、`:65-66` の「flip 済み」注記を保持・退避する指示が無い。行範囲をそのまま消すと、env なしでよい理由という別の設計判断の記録が失われる。実装時は `:65-66` を残す（または新コメントに統合する）よう一言足すことを推奨

## 要対処

なし

## 未検証

- **本 PR 自身の `e2e.yml` run が実際に緑になること**——`paths:` の一致は静的に確認したが、計画の実装後に実際の smoke job（seed 常時化・results 検査無条件化・first-run 検査追加を含む）が green で完走するかは、コード変更後の実行でしか確認できない。フェーズ 5 が予定しているとおり、実装後に必ず実行して確認すること
- **`workspaces: src-tauri` の誤設定が将来修正された場合の再検証**——現状はこの設定が `src-tauri/target` を指しリポジトリルート `target/` を素通りするため rust-cache 経由の残骸リスクは無いが、この設定自体は #804 の射程外の既存の問題（もしくは意図的な設定）であり、真の意図（`src-tauri/target` を指す設計なのか単なる誤記か）は本レビューでは確認していない。仮に将来 `workspaces: . -> target` 等へ訂正されると、rust-cache がリポジトリルート `target/`（`target/smoke-egui/profile` を含む）を対象にするようになり、本計画が前提とする「CI では毎回まっさら」が崩れる可能性がある——ただし #804 の変更範囲では発生しない
