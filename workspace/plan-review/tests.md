# plan-review: テスト・フォールトインジェクション層（#713 G-workspace-lints）

対象: `workspace/plan.md` Phase 3 / `scripts/governance-check.test.mjs`

## 要対処

- **`workspaceMembers` の 5 error 条件のうち「members 行が無い」に対応する赤フィクスチャが Phase 3 に存在しない。** `plan.md:21` は `error` を返す条件を「ルート `Cargo.toml` が読めない / `[workspace]` セクションが無い / `members` 行が無い / 要素 0 件 / 要素に `*` を含む」の 5 つと明記するが、Phase 3 のフィクスチャ列挙（`plan.md:51-53`）は「読めない・セクション無し・0 件」（:51）と「glob」（:52）の 4 形しかカバーせず、「`[workspace]` セクションはあるが `members = [...]` 行自体が無い」（例: `[workspace]\nresolver = "2"\n`）という 5 番目の distinct な guard 分岐が未検証。`破壊不変条件と検知手段` 表（`plan.md:77`）も「4形（読めない・セクション無し・0件・glob）」と明記しており、実装側の宣言（5条件）とテスト側の宣言（4形）がそもそも数として食い違っている。この分岐は `snap({})` のような既存の空スナップショット（`governance-check.test.mjs:943` の ID 形テストが使う）では踏まれない——そちらは「ルート `Cargo.toml` が読めない」という**より手前の**ガードで先に return するため。したがって実装が `section[1].match(...)` の非マッチを未処理のまま `m[1]` にアクセスするような回帰（`throw` する経路——`plan.md:80` の不変条件「途中で `throw` する経路を作らない」に反する）を作っても、Phase 3 のどのフィクスチャも検知しない。フィクスチャを 1 件追加要: `[workspace]\nresolver = "2"\n`（`members` 行無し）→「母集団の欠落」。

## 軽微な懸念

- **フィクスチャ「不混入」2 形（`plan.md:49-50`）は実測 (research.md:69-74「述語の罠」) と 1 対 1 対応するが、それ以外の実測 D/B′（`research.md:55-56`、bin-only crate の deny 有効性）を裏付ける専用フィクスチャは無い。** ただし `hasWorkspaceLintsOptIn` の判定ロジックは bin/lib を区別しない字面判定なので、D/B′ は :45（`[lints]` 無し）のフィクスチャで暗黙にカバーされる——bin-only 用の別フィクスチャを追加で要求するのは過剰（YAGNI）。指摘のみ、対処不要。
- **実リポジトリ カナリア（`plan.md:54`）と #701 母集団カナリア（`governance-check.test.mjs:82-113`）は判定対象が異なり重複ではない**（前者は `checkWorkspaceLints` の 0 件と `workspaceMembers` の要素、後者は `MODULE_INDEX_CRATES`/`governanceDocs` の網羅）。ただし #701 カナリアは `workspaceMembers` 導入後も独自に `members` を正規表現で再導出したまま（:88-95）——Phase 1 で新設する共有ヘルパーへの一本化を今回のスコープでは求めていないため、「写しを増やさない」という研究側の動機（research.md:24-32）が #701 カナリア自身には及ばない状態が残る。指摘のみ、今回のプランの不備ではない。
- **`不混入` :50（ルート `[workspace.lints.rustdoc]` がどの member の opt-in にもならないことの検証）は意図がやや読み取りにくい。** 「ルート内容が誤って member 側の判定に混入しない（読み取りのスコープが `<dir>/Cargo.toml` に閉じている）」ことを検証する意図だと解釈したが、plan.md の一文だけでは「文字列としての `[workspace.lints.rustdoc]` に `hasWorkspaceLintsOptIn` を掛けても false になる」という述語単体テストとも読める。実装フェーズで両方カバーしておけば十分——ブロッキングではない。

## 問題なし

- **稼働中の `Cargo.toml` を変異させない**という規範（`.claude/rules/safety-nets.md`「複製に変異を当てる」）に、Phase 3 は全面的に従っている——`plan.md:41`「`snap()` で注入する最小フィクスチャに対して行う」「稼働中の `Cargo.toml` は一切変異させない」と明記され、実フィクスチャもすべて `snap()`／`makeSnapshot(リポジトリルート)`（読み取り専用）のみを使う設計。
- **既存テストへの波及は無い。** `検査 ID の形`（`governance-check.test.mjs:942-957`）の 3 つの assert はいずれも `ids.length` を動的参照する形（:946, :950, :956）で、`検査 18 件` のようなハードコードされた件数は存在しない——`grep` でリポジトリ全体を確認済み、`検査 1[0-9] 件` の固定文字列は `plan.md`/`research.md`/ドキュメント内にのみ存在し、テストコードには無い。
- **`G-build-commands` の既存テスト（`governance-check.test.mjs:254-279`）は載せ替え後も緑のまま。** 現行の inline 導出（`governance-check.mjs:268-272`）と `workspaceMembers` は同じ `[workspace]` セクション切り出し＋`members = [...]` 抽出のロジックであり、既存フィクスチャ（`cargoRoot = '[workspace]\nmembers = ["snotra-core", "src-tauri"]\n'`）は `[workspace]` セクションあり・`members` 行あり・0 件でも glob でもないので `workspaceMembers` はエラー無しで同じ 2 要素を返す。`lints` 関連の分岐は `checkBuildCommands` に一切関与しない。
- **`npm test` の対象範囲に漏れは無い。** `vitest.config.ts:8-12` の `include` は `scripts/**/*.test.mjs` を含み、`scripts/governance-check.test.mjs` は既存ファイルの編集（新規ファイルではない）なので検出漏れの経路自体が無い。
- **dogfood テスト（`governance-check.test.mjs:960-968`「現在のリポジトリで全検査が緑」）は新検査追加後も緑を維持する。** 実 `Cargo.toml`（ルート）と 4 member（`snotra-core`, `snotra-egui-runtime`, `src-tauri`, `snotra-settings`）を直接確認済み——4 件すべてが `[lints]\nworkspace = true`（テーブル形）を持ち、glob 要素も無い。`checkWorkspaceLints` は 0 件、`workspaceMembers` はエラー無しで 4 件を返すはずで、カナリア（`plan.md:54`）の期待とも整合する。
- **母集団の欠落系の赤ケースは（上記「members 行が無い」の欠落を除けば）別々の経路であり、1 つに畳めない。** `workspaceMembers` の実装は「読めない → セクション無し → members 行無し → 0 件 → glob」の直列 guard になる想定であり、各段は別々の early-return を通るため、意図的に別フィクスチャで踏む必要がある（畳むと分岐カバレッジが落ちる）。

## 未検証（理由）

- **`checkWorkspaceLints`/`workspaceMembers` の実装そのものは存在しない**（Phase 1/2 は計画段階でコード未着手）ため、Phase 3 のテスト文面が実装の関数シグネチャ・エラーメッセージ文言と実際に一致するかは実装後でないと確認できない。特に「母集団の欠落」の finding メッセージ文言が `checkWorkspaceLints` 内（member 単位）と `workspaceMembers` 由来（母集団単位）とで区別可能な文言になっているかは、テストの `message.includes(...)` assert の書き方次第であり、plan.md には正確な文言までは定まっていない。
