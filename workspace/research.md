# research — #474 tsconfig include 拡張と型負債 9 件の返済（PR-1/3 系列）

## issue の要約

`tsconfig.json` の `include: ["ui/src"]` / `exclude: [test files]` により、テスト 14 ファイル + `e2e/` + ルートの config `.ts` がどの型検査にも掛かっていない（vitest は esbuild 変換で型検査をしない）。include を広げると既存型エラーが 9 件出る。これを 1 件ずつ判定して修正し、include を拡張し、hook の `selectChecks` とカナリアを追随させる。

本 PR は #497 コメント（2026-07-10）で合意した 3 PR 系列の PR-1。codex レビューにより「型負債だけ先行」は成立しない（tsconfig 拡張がカナニア更新＝hook 変更を強制する）ため、型修正 + tsconfig 拡張 + selectChecks 追随を一体で行う。

## 9 件の再現と判定（probe tsconfig で実測・2026-07-10）

probe: `extends ./tsconfig.json` + `include: ["ui/src", "e2e", "vite.config.ts", "vitest.config.ts", "playwright.tauri.config.ts"]` + `exclude: []` で `tsc --noEmit` → exit 2、以下 9 件。

| # | 位置 | エラー | 判定 | 根拠 |
|---|---|---|---|---|
| 1 | `e2e/tauri.slash.e2e.ts:9` | TS7016 selenium-webdriver に型定義なし | ツーリング欠落 | `@types/selenium-webdriver` を devDependencies に追加 |
| 2-7 | `search.test.ts:468,497,647,667,688,709` | TS2345 listen モック型不整合 | **テスト側** | モック実装のパラメータ注釈 `(...args: unknown[]) => void` が `EventCallback<unknown>`（`(event: Event<unknown>) => void`）と非互換。実装契約は正しい |
| 8 | `search.test.ts:583` | TS2322 `"error"` が `LaunchStatus` に無い | **テスト側** | Rust `LaunchStatus`（`src-tauri/src/commands/launch.rs:16-20`）は `Ok/Failed/Timeout` の 3 値（snake_case serialize）。TS 型 `invoke.ts:69` と完全一致。`"error"` はバックエンドが決して返さない値。失敗判定は `search.ts:496` の `status !== "ok"` なので `"failed"` へ置換すればテスト意図（失敗時の候補復元）は不変 |
| 9 | `search.test.ts:697` | TS2769 `dir` が `FolderFrame` に無い | **テスト側** | `FolderFrame`（`folder.ts:4-8`）= `currentDir` + `savedQuery` + `SavedViewState`（`savedResults`/`savedSelected`）。`parentDir` というフィールドも存在しない（`computeParentDir` で導出）。folder 分岐は `fs.currentDir` を読む（`search.ts:223`）ため、現テストは `listFolder(undefined, …)` を呼んでいた — モックが引数を無視するため緑のまま契約が壊れていた（#474 が塞ぐ欠陥クラスの実例） |

**実装側の欠落は 0 件**。9 件すべてテスト/ツーリング側。

## 関連コード

- `tsconfig.json` — include/exclude（拡張対象）
- `ui/src/stores/search.test.ts` — 8 件の修正対象
- `package.json` — `@types/selenium-webdriver` 追加
- `.claude/hooks/post-edit.mjs` — `selectChecks` の typecheck 条件（I7: tsconfig の include−exclude と一致必須）、`TEST_FILE` 正規表現
- `.claude/hooks/post-edit.test.mjs` — tsconfig ドリフト検出カナリア（L490-494 が include/exclude をリテラル固定）+ per-file 断言（`e2e`・`vite.config.ts` 等が「発火しない」と固定している L51-57 ほか）
- `docs/build-commands.md` カテゴリ B — 型検査の対象範囲の記述（拡張後の実態に合わせて要確認）

## 既存パターン

- カナリア更新の先例: #471 で導入されたカナリアは「tsconfig を触った人がここで気づく」設計。今回それが**設計どおり発火する初のケース**
- `.claude/hooks/**` の編集は hook-selftest が自動発火する（`selectChecks` 変更の回帰はテストで担保）

## 技術的制約

- **カナリアが tsconfig 変更と hook 変更を原子的に束ねる**: tsconfig だけ変えると hook-selftest が落ちる。同一コミットで両方を変える
- TypeScript ローカルは `^6.0.2`（package.json）。CI も同じ node_modules を使う（`npm ci`）
- `.test.tsx` は jsdom 前提だが tsconfig 既定 lib（ESNext full）に DOM が含まれるため型検査は通る（probe で実証: エラーは 9 件のみ）
- vitest.config.ts の `include` は `.mjs` テスト（`.claude/hooks`・`.githooks`）も含むが、これらは tsc の対象外（TS_LIKE に非該当）

## 未解決の疑問

- なし（probe で全件再現済み・全件判定済み）
