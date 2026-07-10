# plan — #500 cargo の `-p` を真実源へ接地する

方針は 2026-07-10 のユーザー合意（「写しを消す + カナリア」）。

## 中核原理

**写しは消す。写像はカナリアで守る。**

- `check` / `clippy` の `-p` 列挙は「全メンバー」の写し → `--workspace` に置換して**消す**（cargo が `members` を読む = 真実源への接地）
- `test -p <crate>` は「編集した crate → そのテスト」の写像 → 消せない。`members` とパッケージ名の対応をカナリアで固定し、4 つ目の crate で落とす

## 変更ファイル一覧

| ファイル | 変更 |
|---|---|
| `.claude/hooks/post-edit.mjs` | (1) `buildCommand` の `clippy` を `["clippy", "--workspace", "--all-targets", "--message-format", "short", "--", "-D", "warnings"]` へ。(2) `cargo-check` を `["check", "--workspace"]` へ。(3) **`CHECK_DEFINITION` にルートの `"Cargo.toml"` を追加**（完全一致。カナリアへ到達させる） |
| `.claude/hooks/post-edit.test.mjs` | (a) per-file 断言: `Cargo.toml` → `["cargo-check", "hook-selftest"]`。**`snotra-core/Cargo.toml` / `a/b/Cargo.toml` は `["cargo-check"]` のまま**。負例 `Cargo.lock` / `cargo.toml` → `[]` は不変。(b) **新カナリア 1 本**: ルート `Cargo.toml` の `members` が `["snotra-core","src-tauri","snotra-settings"]` であること。**非マッチ時は握り潰さず必ず落とす**（`?? []` は fail-open）。(c) `buildSection` のサンプル repro 文字列（`cargo clippy -p snotra-core`）を `--workspace` 形へ更新（落ちないが、実在しないコマンド形をテストに残さない） |
| `.github/workflows/ci.yml` | L63 `cargo check --workspace`、L75 `cargo clippy --workspace --all-targets -- -D warnings`。test の 3 ステップは `-p` のまま |
| `docs/build-commands.md` | カテゴリ A の L14/L15、Windows のみ節の L77/L78 を `--workspace` へ。L22 の hook 発火記述に「`Cargo.toml` の編集では `cargo check` と hook-selftest（members カナリア）」を追記。CI/CD 対応表のコマンド名は `cargo check` / `cargo clippy` のままで整合 |
| `package.json` | `verify` を `cargo check --workspace && npm run build` へ |
| `.claude/skills/health-check/SKILL.md` | Check 5: 「`-p <crate>` のクレート名が存在するか」は **`test -p` のみが照合対象**である旨を明確化。乖離の例「`--lib` の付与・`-p` の欠落等」は **`--workspace` 化後に `-p` 不在が正しい状態になるため誤トラップ化する** → 例を `--workspace` / `--all-targets` の欠落へ差し替える |
| ルート `CLAUDE.md` | **3 箇所**（レイヤー検証が要対処として検出）: (1) フック表 L69 の `Cargo.toml` → cargo check を「cargo check + hook-selftest」へ。(2) L84「`tsconfig.json` / `vitest.config.ts` / `package.json` はそれぞれ対応するカナリアを持つ」に `Cargo.toml` を追加。(3) L87 の hook-selftest 自動発火セットの列挙に `Cargo.toml` を追加 |

## 実装順序

1. **Phase A — 写しを消す（`--workspace` への置換）**: post-edit.mjs / ci.yml / docs / package.json。`cargo check --workspace` と `cargo clippy --workspace --all-targets -- -D warnings` の green を実測済み（1.21s / 7.75s）
2. **Phase B — カナリアと発火（原子的に 1 コミット）**: post-edit.mjs の `hook-selftest` 条件 + post-edit.test.mjs のカナリア + per-file 断言
3. **故障注入（実測・コミットしない）**: `Cargo.toml` の `members` に 4 つ目のダミーを足す → カナリアが落ちることを確認。`src-tauri/Cargo.toml` の `name` を変える → カナリアが落ちることを確認。両方 revert
4. **Phase C — ドキュメント（CLAUDE.md / build-commands.md / health-check SKILL.md）**
5. **Phase D — workspace 削除・コミット整理**

## 不変条件

- **`--workspace` は現時点で `-p` 3 つと同一集合**（`cargo metadata` の `workspace_members` で実測）。挙動変更なし
- **整合規約（#476）を守る**: hook の cargo コマンドは SSOT カテゴリ A と合否・検査対象を変えるフラグで一致。`--workspace` は両方に入る
- **写像は消さない**: `cargo test -p <crate>` は hook・CI・SSOT に残る。`--workspace` にすると編集していない crate のテストまで走り、hook の即時性を損なう
- **発火とカナリアは対**（#497）: `Cargo.toml` に `hook-selftest` を足すのは、カナリアがそこに在るからである
- **カナリアは 2 つの事実を固定する**: `members` の集合と、ディレクトリ名 → パッケージ名の対応。後者を落とすと `selectChecks`（ディレクトリで判定）と `buildCommand`（パッケージ名で `-p`）がずれる
- **失敗時の挙動**: 新しい状態・リソースなし。`runCheck` の既存骨格（300s timeout・fail-closed）に乗る

## テスト方針

- per-file 断言（正例 + 既存の負例を維持）+ 新カナリア 1 本
- 故障注入 2 件（members に 4 つ目 / パッケージ名の改名）
- `npx vitest run .claude/hooks` green、`npm test` green
- `cargo check --workspace` / `cargo clippy --workspace --all-targets -- -D warnings` green（実測済み）
- CI で `cargo check --workspace` / `cargo clippy --workspace` ステップの green を確認

## SPEC.md 更新要否

不要（開発時検査はプロダクト仕様でない。#474/#475/#497 と同判断）。

## E2E への影響

なし（検査コマンドの置換のみ。アプリの挙動・IPC・設定形式は不変）。

## 受容する残余

- **`cargo test -p` の写像は残る。** 4 つ目の crate が生えたとき、カナリアは落ちるが**自動では直らない** — `selectChecks` の分岐・`buildCommand` の case・CI の step・SSOT の行を手で足す必要がある。カナリアの役割は「静かに漏れる」を「必ず落ちる」に変えることであって、更新の自動化ではない
- `docs/superpowers/plans/` 配下の `-p` 列挙は as-of アーカイブ（歴史記述）なので変更しない

## plan-review の結果（統合）

- **要対処（反映済み）**: `CLAUDE.md` に hook-selftest の発火セットを列挙する箇所が**3 つ**あり（L69・L84・L87）、計画は 2 つしか挙げていなかった。**これは #500 が根絶しようとしている「写しの片方だけが更新される」パターンそのもの**
- **独立再導出との差分（再導出を採用）**: `hook-selftest` を撃つのは**ルートの `Cargo.toml` のみ**（`CHECK_DEFINITION` へ完全一致で追加）。当初案は全 `Cargo.toml`（basename アンカー）だったが過剰だった。理由: **守るべきは沈黙する経路だけ**である。crate 追加は必ずルートの `members` を編集するのでそこで捕まる。一方パッケージ名の改名は `cargo test -p snotra` が "package did not match" で**loud に落ちる**ため、カナリアは要らない
- **一致（完全性の能動的証拠）**: 候補 3（`--workspace`）の採用、`test -p` を写像として据え置く判断、写しの所在 9 箇所・4 ファイル、`package.json` の `verify` が第 4 の写しであること、`docs/superpowers/plans/` を歴史記述として除外すること — すべて独立に一致
- **軽微（反映済み）**: カナリアは正規表現の非マッチを握り潰さず必ず落とす。`buildSection` のサンプル文字列を更新。health-check Check 5 の乖離例が `--workspace` 化後に誤トラップ化する
- **再導出が明示的に「触るな」と判定したもの**: `CONTRIBUTING.md` / `scripts/prepare-sidecar.ps1` / `release.yml`（単一 crate ビルド）、`PERFORMANCE.md` / bench コメント（単一 crate 実行）、`AGENTS.md:64`（#500 を原則の実例として引く恒久記述）、`docs/superpowers/plans/`（時点記録）

## セルフレビュー

1. **対称ペア**: 写し（check / clippy）⟺ 写像（test）。消す ⟺ 守る。発火 ⟺ カナリア
2. **影響範囲**: `-p snotra-core -p snotra -p snotra-settings` を全 grep（9 箇所・4 ファイル）。`docs/superpowers/plans/` は除外と明記
3. **境界条件**: メンバーの `Cargo.toml`（パッケージ名の真実源）、深い階層の `Cargo.toml`（basename アンカーで発火・過剰検出は無害）、`Cargo.lock`（非発火）
4. **リソース管理**: なし
5. **既存パターン整合**: カナリアは `tsconfig` / `vitest.config.ts` / `package.json` の 3 本と同じ場所・様式
6. **YAGNI**: TOML パーサは導入しない（カナリアは正規表現で `members` と `name` を読む。テストなので脆さの影響は「落ちる」方向）
7. **シンプル化**: 写しを消すので、正味の行数は減る
8. **破壊不変条件**: `--workspace` が将来 crate を自動で検査に含めるのは fail-safe 方向。逆に `-p` の列挙は fail-open（漏れが沈黙する）だった
