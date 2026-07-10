# plan — #474 tsconfig include 拡張と型負債 9 件の返済（PR-1/3 系列）

系列全体の合意: #497 コメント（2026-07-10）。本 PR は PR-1。plan-review（レイヤー別 2 体 + 独立再導出 1 体）の指摘を反映済み（末尾参照）。

## 変更ファイル一覧

| ファイル | 変更 |
|---|---|
| `package.json` / `package-lock.json` | `@types/selenium-webdriver`（^4.35.6）を devDependencies に追加（エラー #1） |
| `ui/src/stores/search.test.ts` | listen モック 6 箇所（467-472/496-500/646-651/666-671/687-692/708-713）の明示パラメータ注釈を除去し推論に委譲（#2-7）。583: `"error"` → `"failed"`（#8）。697: 既存の `FOLDER_FRAME` 定数（731-736）を再利用（#9・DRY） |
| `tsconfig.json` | `include: ["ui/src", "e2e", "vite.config.ts", "vitest.config.ts", "playwright.tauri.config.ts"]`、`exclude: []`（フィールド削除ではなく空配列 — カナリアの `toEqual` 照合のため defined 値を残す）。**`incremental`/`tsBuildInfoFile` は触らない** |
| `.claude/hooks/post-edit.mjs` | `selectChecks` の typecheck 条件を新 include−exclude に追随。`TEST_FILE` 定数（L44-45）削除。docstring（L79-84）の I19 記述を「深さ 0 テストファイルは発火する側」に更新。「include 対象外」分岐（L326-329）は**残す**（program 外 TS への真陽性が残る） |
| `.claude/hooks/post-edit.test.mjs` | カナニア（L490-494）のリテラル更新。断言反転 3 箇所（L41-43/L45-49/L51-58、`it` 記述・コメントも書き換え）。`playwright.tauri.config.ts` の断言を**追加**。境界の負例を追加（`scripts/foo.ts` → `[]`、`e2e/README.md` → `[]`）。**統合テスト L440-448 の例示ファイルを program 外の合成パス（`scripts/example.ts` 等・実在不要）へ差し替え**（`vite.config.ts` のままだと断言が偽になり、実 tsc を spawn する） |
| `CLAUDE.md` | フック表 L69「`ui/src/**/*.{ts,tsx,mts,cts}`（`*.test.ts(x)` を除く）→ typecheck」を新集合へ更新。L82「include 対象外（`e2e/`・`*.config.ts`・`*.test.ts(x)`）」の例示を書き換え（3 種すべて対象になるため） |
| `ui/CLAUDE.md` | テスト基盤節に「テストファイルも tsconfig program に含まれ typecheck 対象」を一行追記 |
| `docs/build-commands.md` | カテゴリ B 見出しの対象範囲を実態に合わせ微修正 |

## 実装順序

1. **Phase A — 型負債の返済（tsconfig は未変更のまま）**
   - `npm install -D @types/selenium-webdriver`
   - `search.test.ts` の 8 件を修正（上表のとおり）
   - **Red→Green の検証器**: probe tsconfig（`tsc -p tsconfig.probe-474.json`）が Red（9 件）→ Green（exit 0）。`npm test` で挙動不変を確認
2. **Phase B — tsconfig 拡張 + selectChecks 追随 + ドキュメント（原子的に 1 コミット）**
   - tsconfig / selectChecks / カナリア・断言 / CLAUDE.md ×2 / build-commands.md を同時に変更
   - `npx tsc -p tsconfig.json` exit 0、`npx tsc -p tsconfig.json --listFilesOnly` で 14 テストファイル + e2e + config 3 件が program に入ったことを実測、`npx vitest run .claude/hooks` green、`npm test` green、`npm run build` green
3. **Phase C — probe 削除・workspace 削除・コミット整理**（probe `tsconfig.probe-474.json` はコミットに含めない）

## selectChecks の新条件（I7: tsconfig と一致）

```js
const ROOT_TS_CONFIG = new Set(["vite.config.ts", "vitest.config.ts", "playwright.tauri.config.ts"]);
// typecheck ⟺ ((ui/src/ ∨ e2e/) ∧ TS_LIKE) ∨ ROOT_TS_CONFIG.has(rel)
```

- ルート config 3 ファイルは **Set の完全一致**（`endsWith` だと `sub/vite.config.ts` を誤発火し TS 意味論と乖離）
- `e2e/` はディレクトリ include のため `TS_LIKE`（.mts/.cts 含む）を適用
- `TEST_FILE` の除外分岐は削除（exclude: [] に一致）

## 不変条件

- **I7（カナリア）**: selectChecks の typecheck 条件 = tsconfig の include − exclude。tsconfig・カナリア・selectChecks は同一コミットで変える（片方だけ変えると hook-selftest が落ちる — 設計どおりの発火経路の初実走）
- **テストの意味保存**: `search.test.ts` の修正は型注釈・fixture 形状のみ。`"error"`→`"failed"` は失敗判定 `status !== "ok"`（search.ts:496）・通知分岐（launchNotice.ts:36-40）の双方で同値。FolderFrame 修正は `listFolder(undefined, …)` を呼んでいた歪みの正確化。`npm test` の pass/fail 集合が不変であることで裏付ける
- **hook の他検査への波及なし**: clippy/core-test/settings-test/config-warn/hook-selftest の条件は触らない
- **統合テストの設計契約**: 統合ブロックは「cargo も tsc も起動しない payload だけを使う」（post-edit.test.mjs:419-420）。L440-448 の差し替えはこの契約を守るためでもある

## テスト方針

- probe tsconfig を Red→Green 検証器に使い、最終的に削除
- 発火の実測: per-file 断言（正例 + 負例の両方向 — 「検査の入力集合を具体対象で検算する」）
- CI: TypeScript は lock で 6.0.3（ローカル = CI）。frontend-check は `npm ci → npm test → npm run build` で自動追随、ci.yml の変更不要

## SPEC.md 更新要否

不要。SPEC はプロダクト仕様。開発時検査の対象範囲は `docs/build-commands.md`・CLAUDE.md（フック表）が担い、本計画で同期する。設計文書 §9/§10 の as-built 訂正は PR-3 のスコープ。

## E2E への影響

`e2e/tauri.slash.e2e.ts` は型検査対象になるだけで、実行条件・シナリオは不変。`@types/selenium-webdriver` は型のみ。

## plan-review の結果（統合）

- **要対処（反映済み）**: 統合テスト L440-448 の差し替え（hook 検証 + 独立再導出の両方が検出）。probe ファイルのコミット混入防止（TS 検証）
- **漏れ（導出 ∖ plan・反映済み）**: `CLAUDE.md` L69/L82 のフック表・例示のドリフト、`ui/CLAUDE.md` の一行追記、`playwright.tauri.config.ts` の断言追加、負例断言の追加、`it` 記述・コメントの書き換え明示
- **スコープ過剰（plan ∖ 導出）**: なし（docs/build-commands.md は導出側も「推奨」で一致）
- **一致（完全性の証拠)**: 9 件の判定（テスト側 8 + 依存欠落 1・実装側 0）は 3 体とも独立に一致。selectChecks の新条件・原子性・Red→Green 検証器も一致
- **採用した任意提案**: `FOLDER_FRAME` 定数の再利用（DRY）、`--listFilesOnly` での program 実測
