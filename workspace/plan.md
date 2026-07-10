# plan — #497（#484 含む）検査の定義を変えるファイルを検査集合に載せる（PR-3/3 系列）

系列全体の合意: #497 コメント（2026-07-10）。本 PR は PR-3（最終）。

## 中核原理

**発火の追加とカナリアの追加を対にする。** カナリアの無いファイルに `hook-selftest` を撃っても、そのファイルについては何も検証しない（cargo-cult な緑）。`tsconfig.json` は既存カナリアを持つが、`package.json` / `vitest.config.ts` は持たないため、本 PR で新設する。

## 変更ファイル一覧

| ファイル | 変更 |
|---|---|
| `.claude/hooks/post-edit.mjs` | (1) `cargo-check` 新設: `CARGO_MANIFEST = /(^\|\/)Cargo\.toml$/`（**basename アンカー**。過小検出＝沈黙＝false green を避け、過剰検出は `cargo check` が走るだけで無害な fail-closed 方向。既存 `config-warn` と同型）→ `cargoSpec(["check", "-p", "snotra-core", "-p", "snotra", "-p", "snotra-settings"])`（SSOT カテゴリ A L14 と一字一句一致）、BUDGETS `{ lines: 20, from: "head" }`。(2) `githooks-selftest` 新設（`hook-selftest` と対称な命名）: `rel.startsWith(".githooks/")` → `vitest run .githooks`、BUDGETS `{ lines: 30, from: "head" }`（#484）。(3) `typecheck` 発火に `rel === "tsconfig.json"` を追加。(4) `hook-selftest` 発火に `tsconfig.json` / `package.json` / `vitest.config.ts` を追加。(5) vitest 解決を `vitestSpec(root, target)` ヘルパーへ集約（3 件目が生えたので畳む・code-reviewer L-1）。(6) 冒頭の契約コメント L7 に「割り当ての SSOT は `selectChecks`」の一行を添え、実行中検査の契約(i)とファイル網羅の主張(ii)を区別する |
| `.claude/hooks/post-edit.test.mjs` | (a) per-file 断言（正例）: `tsconfig.json` → `["typecheck", "hook-selftest"]`、`vitest.config.ts` → `["typecheck", "hook-selftest"]`、`package.json` → `["hook-selftest"]`、`Cargo.toml` / `snotra-core/Cargo.toml` / `a/b/Cargo.toml` → `["cargo-check"]`、`.githooks/pre-commit` / `.githooks/githooks.test.mjs` → `["githooks-selftest"]`。(b) 負例: `ui/package.json` → `[]`、`Cargo.lock` → `[]`、`cargo.toml`（小文字）→ `[]`、`.github/workflows/ci.yml` → `[]`（規範ファイル・受容する残余）。(c) **新カナリア 2 本**: `vitest.config.ts` の `include` が 3 パターンを含む / `package.json` の `scripts.prepare` が `core.hooksPath .githooks` を設定し `test` が `vitest run`・`typecheck` が `tsc` である。(d) 統合テストには新検査を踏む payload を**足さない**（既存 payload が新検査を spawn しないことは検証済み） |
| `CLAUDE.md` | (1) フック表 L69 の発火条件に新 4 系統を追記。(2) **「沈黙は合格を意味する」に前提条件**を書く（「検査が割り当てられているファイルでは」+ 割り当ての SSOT は `selectChecks`）。(3) L81「沈黙を『合格』と読めるのは…」に「検査が割り当てられている前提の下で」を補う。(4) L84 の hook-selftest 発火の説明に定義ファイルを追記。(5) **肯定的報告を採らない判断根拠と残余**を新規箇条書きで記録 |
| `docs/build-commands.md` | (1) L24 の「フックの沈黙は合格を意味する」に同じ前提条件（同一の全称主張が 2 箇所）。(2) L22 の hook 自動発火の記述に `Cargo.toml` → cargo check を追記。(3) カテゴリ E に「`.githooks/**` 編集で `vitest run .githooks` が自動発火する」を追記 |
| `docs/superpowers/specs/2026-07-09-hook-responsibility-layers-design.md` | 既存の「実測による訂正」様式（`:84-98` が先例。原文を残し引用ブロックを後置）に倣い 3 箇所: (1) **§1 `:53`** — この沈黙は PR-3 で閉じた。**ただし `:50` の matcher 外・`:51` の config-warn envelope・`:52` の構文エラーは残る**（4 経路のうち 1 経路のみ解消）。(2) **§9 `:309` / `:313`** — #474/#475/#476 は Phase 3 に依存せず独立に着手でき PR #498/#499 でマージ済み、#477/#479 は前提崩れで close、新規起票 ④ は本 PR（#484）で解決。(3) **§10 `:319`** — Phase 3 は「入力集合の拡張」で**部分的に**置換した。肯定的報告は採らなかった（根拠は `CLAUDE.md`）。残る沈黙経路を明記する |

## 実装順序

1. **Phase A — hook 本体 + テスト + カナリア（原子的に 1 コミット）**。`npx vitest run .claude/hooks` green
2. **故障注入（実測・コミットしない）**: (a) `vitest.config.ts` の include から `.githooks/**/*.test.mjs` を一時削除 → 新カナリアが落ちることを確認、(b) `.githooks/pre-commit` を一時的に壊す → `githooks-test: 失敗` が届くことを確認、(c) `Cargo.toml` に不正な行を足す → `cargo-check: 失敗` が届くことを確認。すべて revert
3. **Phase B — ドキュメント（CLAUDE.md / build-commands.md / 設計文書）**
4. **Phase C — workspace 削除・コミット整理**

## 期待される発火集合（push 順込み）

| rel | checks |
|---|---|
| `tsconfig.json` | `["typecheck", "hook-selftest"]` |
| `vitest.config.ts` | `["typecheck", "hook-selftest"]` |
| `package.json` | `["hook-selftest"]` |
| `Cargo.toml` / `snotra-core/Cargo.toml` | `["cargo-check"]` |
| `.githooks/pre-commit` | `["githooks-test"]` |
| `.claude/hooks/post-edit.mjs` | `["hook-selftest"]`（不変） |
| `snotra-core/src/lib.rs` | `["clippy", "core-test"]`（不変） |

## 不変条件

- **発火とカナリアは対**: カナリアの無いファイルに検査を割り当てない。`package.json` / `vitest.config.ts` の hook-selftest は本 PR の新カナリアが裏付ける
- **`cargo-check` のコマンドは SSOT カテゴリ A と一字一句一致**（PR-2 で明文化した整合規約に自ら従う）
- **`Cargo.toml` の判定は 2 段以上を含まない**（実測 4 件と一致。深いネストの `Cargo.toml` は存在せず、将来生えたら負例テストが警告する）
- **統合テストの契約**: 検査プロセスを起動しない payload だけを使う（新検査も cargo / vitest を起動するため、統合テストで実在の `Cargo.toml` / `.githooks/*` を payload にしない）
- **他検査への波及なし**: clippy / core-test / settings-test / tauri-test / csp-test / config-warn の条件は不変
- **全称主張には前提条件を書く**: 「沈黙は合格」は「検査が割り当てられているファイルでは」という前提の下でのみ真

## 受容する残余（明記する）

- `.github/workflows/*.yml`・`docs/build-commands.md`・`*.md` 全般・`scripts/`・`SPEC.md` は検査が割り当てられず、沈黙が「何も走らなかった」を意味する。**エージェントはこれを「検査が通った」と区別できない**。定期検知（health-check Check 5 / Check 10）が補うが、編集時の即時性は無い
- `Cargo.lock` は対象外（`/deps-update` が CI で検証する）
- **肯定的報告（Phase 3 原案）を採らない根拠**: 全編集で出力が増える（`.md` 1 行の編集でも「検査なし」と報告する）。入力集合の拡張で「検査の定義を変えるファイル」の穴は塞がり、残るのは検査を持ちようがないファイルのみ。前提条件を明文化することでこの区別を規範に委ねる。**これは機構ではなく規範であり、読者が前提条件を忘れれば false green は再発する**

## テスト方針

- per-file 断言（正例 + 負例）+ 新カナリア 2 本
- 故障注入 3 件（上記 Phase 2）— 安全網は一度壊して実測する
- `npx vitest run .claude/hooks` green、`npm test` green

## SPEC.md 更新要否

不要（PR-1/PR-2 と同判断。開発時検査はプロダクト仕様でない）。

## セルフレビュー

1. **対称ペア**: 「安全網そのものを編集したら安全網を検査する」が `.claude/hooks/**`（既存）→ `.githooks/**`（本 PR）へ対称に拡張される。発火 ⟺ カナリアの対称
2. **影響範囲**: `selectChecks` の全断言を Grep で列挙して負例も更新する。統合テストで新検査が spawn されないことを確認
3. **境界条件**: 2 段以上の `Cargo.toml`、`ui/package.json`、`.githooks` 直下でない `githooks/`、`tsconfig.probe-*.json`（typecheck は完全一致なので非発火）
4. **リソース管理**: 生成リソースなし（純関数の条件追加 + runCheck の既存骨格）
5. **既存パターン整合**: `vitestSpec` ヘルパーは既存 2 箇所の重複を畳むだけ。新パターンなし
6. **YAGNI**: `.github/workflows/` と `docs/` には検査を足さない（実行時検査を持たない）。`Cargo.lock` も足さない
7. **シンプル化**: 検査 id 2 つ・条件 4 つの追加。分岐構造は増えない
8. **破壊不変条件**: 新検査も `runCheck` の骨格（300s timeout・ENOBUFS/ETIMEDOUT 報告・fail-closed）に乗り、沈黙経路を増やさない
