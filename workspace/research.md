# research — #475 + #476 Rust/CSP の検査集合と SSOT の整合（PR-2/3 系列）

## issue の要約

- **#475-1**: `src-tauri/src/` の 68 個の `#[test]` が hook・CI・docs のどの自動経路でも実行されない
- **#475-2**: `tauri.conf.json` を編集しても契約テスト `ui/src/lib/cspValidation.test.ts` が走らず、無関係な WARN だけ出る
- **#476-1**: hook の `core-test` は `--lib` 付き、SSOT（docs/build-commands.md）は `--lib` なしで乖離
- **#476-2**: hook の検査コマンドと SSOT のドリフトに検知器が無い（Check 5 の grep 対象は AGENTS.md と skills のみ）

本 PR は #497 コメント（2026-07-10）で合意した 3 PR 系列の PR-2。

## 実測（2026-07-10）

- `cargo test -p snotra` → **68 passed / 0 failed / 0.01s**（Windows・green。Win32 依存で落ちるテストなし — #475 確認事項 1 を再実測）
- `cargo test -p snotra-core`（`--lib` なし）→ lib の 468 tests（459 passed + 9 ignored bench）+ **doc-test 0 件**の 2 ハーネス。`--lib` の有無は実行内容に差が無い（no-op）。`snotra-core` に `tests/`・`benches/` は存在しない
- 現状の検査経路:
  - hook（`post-edit.mjs:89-91`）: `*.rs` → clippy、`snotra-core/` → core-test、`snotra-settings/` → settings-test。**src-tauri のテスト検査は無い**
  - CI（`ci.yml:65-69`）: `cargo test -p snotra-core` / `-p snotra-settings` のみ
  - docs カテゴリ A（`build-commands.md:16-17`）: 同上 2 crate のみ
- `tauri.conf.json` の hook 発火（`post-edit.mjs:97`）: `config-warn` のみ（`systemMessage` に WARN、`additionalContext` なし）。`cspValidation.test.ts` は `src-tauri/tauri.conf.json` を読む本物の契約テストで、`npm test` 経由で CI では走るが編集時の即時フィードバックが無い

## 関連コード

- `.claude/hooks/post-edit.mjs` — `selectChecks`（L85-110）、`buildCommand`（L216-243）、`BUDGETS`（L28-34）
- `.claude/hooks/post-edit.test.mjs` — per-file 断言（`src-tauri/src/lib.rs` → `["clippy"]` を固定 L74-75）、統合テスト「config-warn は systemMessage に出る」（実在の `src-tauri/tauri.conf.json` を payload に使う）
- `.github/workflows/ci.yml` — rust-check ステップ
- `docs/build-commands.md` — カテゴリ A、Windows のみ節、CI/CD 対応表
- `.claude/skills/health-check/SKILL.md` — Check 5
- `CLAUDE.md` — フック表（発火条件の記述）

## 既存パターン

- 検査 id の追加: `settings-test` が先例（cargoSpec + BUDGETS tail 予算 + selectChecks の crate プレフィックス判定）
- vitest 実行: `hook-selftest` が先例（`resolveBin` で vitest.mjs を解決し `nodeSpec`）
- 統合テストの契約: 「cargo も tsc も起動しない payload だけを使う」（post-edit.test.mjs:428）— PR-1 で I16 例示を合成パスへ差し替えたのと同じ制約が、csp-test 追加で config-warn 統合テストにも生じる

## 技術的制約

- **csp-test を追加すると、統合テスト「config-warn は systemMessage に出る」が vitest を再帰 spawn する**: payload が実在の `src-tauri/tauri.conf.json` のため、新条件では csp-test も発火する。`config.toml`（config-warn の regex にマッチし csp-test にはマッチしない）へ差し替える必要がある
- csp-test の対象は `src-tauri/tauri.conf.json` の**完全一致**にする（契約テストが読むのはそのパスのみ。任意深度の `tauri.conf.json` に発火させると、テストが読まないファイルの編集で「検査が通った」と誤読させる — PR-1 の ROOT_TS_CONFIG と同じ理由）
- config-warn の WARN は**残す**: 「Windows 互換性の確認」は CSP 契約とは独立の関心（`docs/development-principles.md:80` の backgroundThrottlingPolicy 事例）。csp-test は機械検査、WARN は人間向け注意喚起で役割が違う
- `cargo test -p snotra` は snotra crate のビルドを要する。clippy が既に全 crate をコンパイルするため warm では追加コストは小さい（実測 0.01s + リンク済みバイナリ）。cold は 300s の per-check timeout 内

## 未解決の疑問

- なし（68 テスト green・`--lib` no-op・csp テストの実在と読み先はすべて実測済み）
