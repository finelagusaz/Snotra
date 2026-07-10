# research — #497（#484 含む）検査の定義を変えるファイルを検査集合に載せる（PR-3/3 系列）

## issue の要約

`selectChecks` は「検査を**受ける**ファイル」を鍵にしており、「検査の**定義を変える**ファイル」を鍵にしていない。例外は `.claude/settings.json` と `.claude/hooks/**` の 2 つだけ。結果:

- `tsconfig.json` を編集しても、そのドリフトを検出するカナリア（`post-edit.test.mjs`）が起動しない
- `package.json` / `vitest.config.ts` / `Cargo.toml` / `.githooks/**`（#484）はいずれも完全な沈黙
- `CLAUDE.md`「沈黙は合格を意味する」は**前提条件を書いていない全称主張**（`AGENTS.md`「全称表現は前提条件とセットで書く。書けないなら書かない」に反する）
- 設計文書 §9 は #474/#475/#476 を「Phase 3 に依存」と処遇したが、実際は独立に着手でき、PR-1/PR-2 で完了済み。§10 の Phase 3 は起票すらされていない — 処遇自体が as-built ドリフト

本 PR は #497 コメント（2026-07-10）で合意した 3 PR 系列の PR-3（最終）。

## 実測（2026-07-10）

- `npx vitest run .githooks` → **16 passed / 14.31s**（使い捨て repo を作るため遅い。300s 予算内）
- `cargo check -p snotra-core -p snotra -p snotra-settings`（warm） → **6.15s**。clippy（`--all-targets`）より軽い。codex の指摘どおり `Cargo.toml` には check を割り当てるのが妥当
- `git ls-files '*Cargo.toml'` → `Cargo.toml`・`snotra-core/`・`snotra-settings/`・`src-tauri/` の **4 件**（ルート + メンバー直下のみ。深いネストは無い）
- `git ls-files .githooks` → `_lib.sh`・`githooks.test.mjs`・`pre-commit`・`pre-merge-commit`・`pre-push`・`pre-rebase`
- `package.json` の `scripts`: `prepare`（`git config core.hooksPath .githooks` = Layer 1 の bootstrap）・`test`（`vitest run`）・`typecheck`（`tsc`）
- `vitest.config.ts` の `include`: `ui/src/**/*.test.{ts,tsx}`・`.claude/hooks/**/*.test.mjs`・`.githooks/**/*.test.mjs`

## 「hook-selftest を撃つ」だけでは足りない

`tsconfig.json` は既にカナリア（`post-edit.test.mjs` の「tsconfig ドリフト検出カナリア」）を持つため、`hook-selftest` を撃てば実質的な検査になる。

しかし `package.json` / `vitest.config.ts` には**カナリアが存在しない**。これらの編集で `vitest run .claude/hooks` を走らせても、当のファイルについては何も検証しない — **cargo-cult な発火**であり「沈黙 = 合格」を「何も検証していないが緑」に置き換えるだけになる。

ゆえに本 PR は**発火の追加とカナリアの追加を対にする**:

| ファイル | 破れうる不変条件 | カナリアの主張 |
|---|---|---|
| `vitest.config.ts` | include が縮むと hook-selftest / githooks-test / npm test が静かにテストを走らせなくなる | `include` が 3 パターン（`ui/src` テスト・`.claude/hooks` テスト・`.githooks` テスト）を含む |
| `package.json` | `prepare` が消えると Layer 1（`.githooks/`）の bootstrap が失われる。`test` / `typecheck` が変わると SSOT のコマンドが偽になる | `scripts.prepare` が `core.hooksPath .githooks` を設定し、`test` が `vitest run`、`typecheck` が `tsc` である |

## 関連コード

- `.claude/hooks/post-edit.mjs` — `selectChecks`（L89-118）、`buildCommand`（L221-257）、`BUDGETS`（L28-36）。vitest 解決が csp-test / hook-selftest で重複（code-reviewer L-1: 3 件目が生えたら畳む → 本 PR で `githooks-test` が 3 件目）
- `.claude/hooks/post-edit.test.mjs` — tsconfig ドリフト検出カナリア（末尾 describe）、per-file 断言
- `CLAUDE.md` — フック表（L69）、「沈黙は合格」（L69 表内）、「沈黙を『合格』と読めるのは…」（L81）
- `docs/build-commands.md` — L24「フックの沈黙は合格を意味する」（同じ全称主張）
- `docs/superpowers/specs/2026-07-09-hook-responsibility-layers-design.md` — §9 の処遇表（L309/L313）、§10 の Phase 3（L319）

## 技術的制約

- **`.githooks/` の検査は 14.31s** かかる（使い捨て repo で実 git を回すため）。編集時 hook としては重いが、`.githooks/` の編集頻度は低く、300s 予算内。CI（`npm test`）でも走る
- **`Cargo.toml` には clippy ではなく cargo check**（codex）。clippy は `--all-targets` で全 crate をコンパイルし cold では分オーダー。cargo check は SSOT カテゴリ A の L14 と一字一句一致するコマンドがある
- **`.githooks/githooks.test.mjs` 自身の編集も githooks-test を撃つ**（`.claude/hooks/post-edit.test.mjs` が hook-selftest を撃つのと同型）
- **`Cargo.lock` は対象外**（依存更新は `/deps-update` が CI で検証する）。`.github/workflows/*.yml` と `docs/build-commands.md` は実行時検査を持たない規範ファイルで、Check 5 / Check 10 の定期検知が担う

## 未解決の疑問

- **肯定的報告（Phase 3 原案）を採るか**: 入力集合を広げても「割り当ての無いファイル」（`*.md` 全般・`SPEC.md`・`scripts/`）は必ず残る。それらの沈黙は無害だが、エージェントは「検査が無いから沈黙した」のか「検査が通ったから沈黙した」のか区別できない。→ 本 PR は**採らない**と決め、根拠と残余を `CLAUDE.md` に記録する（受け入れ条件 4）
