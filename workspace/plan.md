# plan — #475 + #476 Rust/CSP の検査集合と SSOT の整合（PR-2/3 系列）

系列全体の合意: #497 コメント（2026-07-10）。本 PR は PR-2。plan-review（レイヤー検証 + 独立再導出）の指摘を反映済み（末尾参照）。

## 変更ファイル一覧

| ファイル | 変更 |
|---|---|
| `.claude/hooks/post-edit.mjs` | (1) `tauri-test` 新設: `selectChecks` に `isRust && rel.startsWith("src-tauri/")`、`buildCommand` に `cargoSpec(["test", "-p", "snotra"])`、`BUDGETS` に `{ lines: 8, from: "tail" }`（settings-test と同型・9 モジュール crate）。(2) `csp-test` 新設: `rel === "src-tauri/tauri.conf.json"`（完全一致）を config-warn 行の直後に配置 → push 順 `["config-warn", "csp-test"]`、`buildCommand` は `resolveBin` で vitest 解決 + `nodeSpec([vitest, "run", "ui/src/lib/cspValidation.test.ts"])`（I11/I17 準拠・不在時は null → HOOK ERROR 経路）、`BUDGETS` `{ lines: 30, from: "head" }`。(3) `core-test` の `--lib`（L228）を削除 → **hook = SSOT = CI の三点一致** |
| `.claude/hooks/post-edit.test.mjs` | (a) L85-88「src-tauri の .rs は clippy のみ」→ `["clippy", "tauri-test"]`・**タイトルも改名**。(b) **L152-161 `checksForPayload` §1 回帰テスト**: `src-tauri/src/main.rs` の期待値 `["clippy"]` → `["clippy", "tauri-test"]`（証明対象「old_string の語で発火しない」は不変）。(c) L90-92 → `["config-warn", "csp-test"]`・タイトル改名。(d) 負例追加: `foo/tauri.conf.json` → `["config-warn"]`（csp-test は完全一致のみ — 両方向検算）。(e) 統合テスト L466-475 の payload を `path.join(REPO, "config.toml")` へ差し替え（実在の tauri.conf.json のままだと csp-test が vitest を再帰 spawn し「cargo も tsc も起動しない」契約を破る） |
| `.github/workflows/ci.yml` | rust-check の test ステップ群に `cargo test (snotra)`（`cargo test -p snotra`）を追加。rust-cache の `workspaces: src-tauri` は既存のまま（check/clippy が snotra をキャッシュ済み） |
| `docs/build-commands.md` | カテゴリ A に `cargo test -p snotra`（src-tauri を変更した場合・必須）追加。L20「両テスト」→ 全 crate へ一般化（「snotra-core は TDD 重視で…」も「変更した crate のテストはフックが自動実行」へ）。L21 の発火列挙に src-tauri テストと CSP 契約テストを追記。**規約を 1 文明文化**: 「フックの cargo コマンドは本ファイル記載と一字一句一致させる。npm 系検査は SSOT コマンド（`npm test` / `npm run typecheck`）の部分集合ラッパー（単一テストファイルの vitest 実行・tsc 直接起動）を許容する。乖離は `/health-check` Check 5 が検知する」。「Windows のみ」節と CI/CD 対応表 L109 に `-p snotra` 追加（**対応表を落とすと Check 10 が Warning を出す**） |
| `.claude/skills/health-check/SKILL.md` | Check 5 に追加: 「`.claude/hooks/*.mjs` が保持する検査コマンド（`post-edit.mjs` の `buildCommand`）を `docs/build-commands.md` と照合する。cargo コマンドは SSOT 記載と**一字一句一致**（フラグ差・crate 欠落を報告）。node/vitest 系は SSOT コマンドの部分集合ラッパーを許容（対象ファイルが SSOT コマンドの実行対象に含まれることを確認）」 |
| `CLAUDE.md` | フック表: `*.rs` → clippy（core/settings/**src-tauri** 配下ではその crate のテストも）、`tauri.conf.json` → WARN（**`src-tauri/tauri.conf.json` はさらに CSP 契約テスト**）。L85 の「WARN の真陽性」行に csp-test の併走を追記 |

## 実装順序

1. **Phase A — hook 本体 + テスト（原子的に 1 コミット）**: post-edit.mjs + post-edit.test.mjs。hook-selftest green
2. **故障注入（安全網の実測・コミットしない）**: (a) src-tauri のテストを一時的に壊し `tauri-test: 失敗` の報告を確認、(b) `tauri.conf.json` の `connect-src` から `ipc:` を一時除去し `csp-test: 失敗` の報告を確認。両方とも revert
3. **Phase B — CI・docs・skill 同期**: ci.yml / build-commands.md / health-check SKILL.md / CLAUDE.md
4. **Phase C — workspace 削除・コミット整理**

## 不変条件

- **検査の追加は他 id の条件・コマンドを変えない**（`--lib` 削除は実行内容を変えない — lib 468 + doc-test 0 + integration 0 で走行集合一致を実測済み）
- **csp-test は完全一致**: 契約テストが読む `src-tauri/tauri.conf.json` のみ。false green を作らない
- **config-warn の役割は不変**: WARN は Windows 互換の人間向け注意喚起（backgroundThrottlingPolicy 事例）で、CSP 契約とは独立の関心。残す
- **統合テストの契約**: vitest も含め「検査プロセスを起動しない payload だけを使う」
- **失敗時の挙動**: 新検査は runCheck の既存骨格（300s timeout・ENOBUFS/ETIMEDOUT 報告・fail-closed）に乗る。新しい状態・リソースなし

## テスト方針

- per-file 断言（正例 + 負例）+ 故障注入 2 件（上記）
- `npx vitest run .claude/hooks` green、`npm test` green、`cargo test -p snotra` green（実測済み）
- CI 初回 run で `cargo test (snotra)` ステップの green を確認

## SPEC.md 更新要否

不要（PR-1 と同判断。SPEC に cspValidation・cargo test への言及なし）。

## E2E への影響

なし（検査経路の追加のみ）。

## plan-review の結果（統合）

- **要対処（反映済み）**: `checksForPayload` L152-161 の断言更新 — レイヤー検証と独立再導出が**独立に同一箇所へ到達**（一致 = 能動的証拠）
- **採用した再導出の優位案**: 規約は「cargo 一字一句一致 / npm 系のみ部分集合ラッパー許容」— 当初案「表示フラグのみ許容」は hook-selftest/csp-test 自身を乖離と誤検出する欠陥があった。`--lib` 削除で cargo 側が機械判定可能になる
- **軽微（反映済み）**: テストタイトルの改名、BUDGETS tauri-test 8/tail、CLAUDE.md L85 追記、build-commands.md L20 の一般化
- **スコープ外と記録**: `implement/SKILL.md:51` の例示（`cargo test -p snotra-core`）は不完全だが偽ではない — PR 本文に記録のみ、変更しない
