# plan-review: 検査本体（実装）— #713 G-workspace-lints

対象: `scripts/governance-check.mjs`（`checkBuildCommands` 現 268-282 行付近・`buildChecks`・`runAll`）。実装未着手（`git status`/grep で `workspaceMembers`/`checkWorkspaceLints`/`hasWorkspaceLintsOptIn` は 0 件確認済み）のため、以下は plan.md の記述どおりに述語を自前で再実装し、実ファイル・フィクスチャに対して `node -e` で実測した結果。

## 問題なし

- **述語は実 4 member すべてで true を返す**: plan の記述（`[lints]` 完全一致配下の `workspace = true` / ルート直下 dotted）どおりに実装した predicate を `snotra-core/Cargo.toml:6-7`・`snotra-egui-runtime/Cargo.toml:9-10`・`src-tauri/Cargo.toml:6-7`・`snotra-settings/Cargo.toml:6-7` に対して実行し、全件 `true`。4 ファイルとも BOM なし。実行結果: `snotra-core/Cargo.toml hasBOM=false opt-in=true`（他 3 件も同型）
- **実測 B/C/D/E/F 相当のフォールトインジェクション 14 ケースを自前 predicate で実行し全件期待どおり**（`[lints]` 無し→false、`[lints.rustdoc]` のみ→false、`workspace=false`→false、`[package]` 配下 dotted→false、ルート直下 dotted→true、CRLF→true）。plan が挙げる Phase 3 の赤/緑フィクスチャと矛盾する挙動は見つからなかった
- **`workspaceMembers` 載せ替えは現在の入力（実 `Cargo.toml` および `governance-check.test.mjs:256` の `cargoRoot` フィクスチャ）に対して `checkBuildCommands` の判定を変えない**: 現在のルート `Cargo.toml`（`Cargo.toml:1-7`）も既存テストの `cargoRoot`（`'[workspace]\nmembers = ["snotra-core", "src-tauri"]\n'`）も `[workspace]` セクションが単純単一で、全文正規表現とセクション切り出し正規表現が同一の `members` 行を拾う。両手法の抽出結果は一致することを確認した
- **他ファイルからの export 依存は無い**: `grep -rn "governance-check"` で `scripts/`・`.claude/hooks/`・`.github/workflows/` を確認したが、`governance-check.mjs` を import しているのは `scripts/governance-check.test.mjs` のみ（`ci.yml:58` は `node scripts/governance-check.mjs` を呼ぶだけで export 名に依存しない）。ヘルパー新設・シグネチャ変更で壊れる呼び出し元は無い
- **buildChecks に 1 件足す影響面は限定的**: `検査 ${checks.length} 件` は `buildChecks` から動的計算（`governance-check.mjs:1391`）、ID 形テスト（`governance-check.test.mjs:945-951`）は配列を動的 map、件数一致テスト（`:953-957`）も `ids.length` を使うため、いずれも自動追従する。evidence 文字列を厳密一致（`toBe`）で照合しているテストは無い（`grep -n "evidence"` で該当箇所は `toContain` のみ）
- **`G-workspace-lints` の ID 形・重複なし**: `/^G-[a-z][a-z0-9]*(-[a-z0-9]+)*$/` に一致し、既存 17 ID（`G-module-index`〜`G-near-heading-refs`）と衝突・紛らわしさなし
- **issue #713 とスコープが一致**: `gh issue view 713` を確認。「機構化するか」「粒度（全 member か lib のみか）」の 2 点が判断待ちだったが、plan 冒頭に「ユーザー裁定：機構化する・粒度は全 member（2026-07-28）」と明記されており過不足なし。glob 展開器を作らない判断も issue の費用対効果の懸念と整合（YAGNI）
- **`.claude/skills/health-check/references/mechanized-checks.md` は更新不要**という research.md の主張を確認: 同ファイルは「旧 Check → G」の移行記録のみを持ち、直近追加された `G-adr-file-names`（旧 Check を持たない新設検査）も掲載されていない先例と整合する
- **`docs/build-commands.md:26` の追記位置は正しい**: 該当行は既に「deny 化は各 crate の `[lints] workspace = true`（`Cargo.toml`）→ root `[workspace.lints.rustdoc]`」を説明する箇所で、同じ段落・スタイルで `G-build-commands` を裸で参照する先例（`:27`）もあるため、`G-workspace-lints` を同形式で足すのは既存プローズ規約と整合。見出し参照ではないため `governance-docs.md` の正準形ルールは対象外
- **異常系は fail-closed で一本化されている**（plan の設計どおり読める限り）: `workspaceMembers` のエラー条件（ルート `Cargo.toml` 不読 / `[workspace]` 節なし / `members` 行なし / 0 件 / glob 混入）と `checkWorkspaceLints` 側のエラー条件（member `Cargo.toml` 不読）はいずれも「finding を返して終わる」設計で、`runAll(snap({}))`（`governance-check.test.mjs:426-431`、空スナップショットで全 `run()` を実行する既存テスト）で `checkWorkspaceLints` が例外を投げない前提と整合する

## 軽微な懸念

- **`[lints]` 完全一致の判定がトリム/コメント除去を明示していない**: plan は「`[lints]` **完全一致**」とだけ書き、実装が `line.trim() === "[lints]"` のような厳密文字列比較にすると、将来 `[lints]  # opt-in` のような行末インラインコメントや行末空白（どちらも有効な TOML）を持つ crate が **false 判定**（誤って opt-in 漏れ扱い）になりうる。現 4 member にはこの表記揺れは無いため今は顕在化しないが、Phase 3 のテスト一覧にもこのケースが無い。`[lints]` 行・`workspace = true` 行の双方でトリム＋行末コメント除去を明示することを推奨
- **CRLF の明示フィクスチャが Phase 3 リストに無い**: CI は Windows checkout（`autocrlf=true`）で CRLF 化される（`governance-check.mjs:2-4` の shebang コメント、および `checkHookCommands` の CRLF 回帰テスト `governance-check.test.mjs:413-419`・PR #595 の実例）。自前 predicate では CRLF 入力でも正しく判定できたが、plan の Phase 3 チェックリストには `G-workspace-lints` 用の CRLF フィクスチャが明記されていない。同じ「行を舐める」設計の既存検査（`checkHookCommands`）が過去に CRLF で実際に壊れた実例がある以上、同型の回帰テストを 1 件追加することを推奨
- **不変条件 5「G-build-commands の findings は載せ替えの前後で変わらない」が全称として成立しない入力が存在する**: `workspaceMembers` は glob 要素混入時に「母集団の欠落」として **members 配列全体を空にする**（Phase 1 の設計）。旧 `checkBuildCommands`（`:268-273`）はメンバーごとに個別評価するため、glob 要素が 1 件混じっても **他の正当な member は crateNames に残る**（読めない member だけが `if (name)` で個別に落ちる）。載せ替え後は glob が 1 件でも混じれば `crateNames` が丸ごと空になり、既存の正当な `cargo test -p <crate>` 参照まで新たに finding 化される。現リポジトリは glob 要素 0 件（research.md 実測）ゆえ**今は顕在化しない**が、不変条件 5 の文言は無条件の「変わらない」であり、`.claude/rules/`（AGENTS.md「全称表現は前提条件とセットで書く」）に照らすと「glob 要素が無い場合」という前提を明記すべき。Phase 3 のテスト一覧にもこの分岐（glob 混入時の `checkBuildCommands` 挙動）を固定するケースが無い

## 要対処

（本レイヤーでは検出されなかった）

## 未検証（理由）

- **実装コード自体は存在しない**（`workspaceMembers`/`checkWorkspaceLints`/`hasWorkspaceLintsOptIn` は grep 0 件・実装着手前）。本レビューの実測はすべて plan.md の記述を私が忠実に再実装した predicate に対するものであり、実装コミット後に同じ実測（4 member ファイル・Phase 3 の赤/緑/不混入フィクスチャ・CRLF・トレイリングコメント）を実コードに対して再実行し、一致を確認する必要がある
- **`npm test` / `npm run governance:check` の実行結果**: 検査本体が未実装のため実行できない。Phase 4 で計画されている実行（`検査 18 件` の印字確認を含む）は、コード実装後に別途検証が必要
- **Phase 3 のテスト実装そのものの網羅性・フィクスチャの厳密さ**（赤 8 / 緑 2 / 不混入 2 の実ファイル配置・アサーション文面）はテストファイル未実装につき評価対象外。テストレイヤー担当のレビューに委ねる
