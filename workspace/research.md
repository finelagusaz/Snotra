# research — #500 cargo の `-p` を真実源へ接地する

## issue の要約

`cargo` の `-p snotra-core -p snotra -p snotra-settings` は **ワークスペースメンバーの写し**であり、真実源（`Cargo.toml` の `members`）とは照合されていない。4 つ目の crate を追加すると、hook・CI・SSOT が**同じ誤りを共有**し、相互照合（health-check Check 5）は「一致している」と報告する。沈黙 = false green。

## 決定的な区別（issue 本文が混同していた）

同じ `-p` に見えて 2 種類ある:

| 種別 | 例 | 性質 | 対処 |
|---|---|---|---|
| **写し** | `cargo check -p a -p b -p c` / `cargo clippy -p a -p b -p c` | 「全メンバー」を人手で列挙したもの | **`--workspace` で消せる**（cargo が members を読む） |
| **写像** | `cargo test -p snotra-core`（`snotra-core/` を編集したとき） | 「編集した crate → そのテスト」の対応 | 消せない。カナリアで守る |

`--workspace` は `cargo test` には使えない（編集していない crate のテストまで走り、hook の即時性を損なう）。

## 実測（2026-07-10）

- `cargo metadata --no-deps` の `workspace_members` = `snotra-core` / `src-tauri` / `snotra-settings` の **3 件**。`-p` の 3 つと完全一致
- `cargo check --workspace` → **exit 0 / 1.21s**（warm）
- `cargo clippy --workspace --all-targets -- -D warnings` → **exit 0 / 7.75s**（warm）
- ゆえに `--workspace` への置換は**現時点で挙動が同一**であり、将来 crate が増えたときだけ差が出る（自動で新 crate を検査する = fail-safe）

## 写しの所在（生きたファイルのみ・全 9 箇所）

`git grep '-p snotra-core -p snotra -p snotra-settings'`（`docs/superpowers/plans/` は as-of アーカイブのため除外）:

| ファイル | 箇所 | コマンド |
|---|---|---|
| `.claude/hooks/post-edit.mjs` | `buildCommand` の `clippy` case | clippy（配列形式） |
| 同上 | `buildCommand` の `cargo-check` case | check（配列形式） |
| `.github/workflows/ci.yml` | L63 | check |
| 同上 | L75 | clippy |
| `docs/build-commands.md` | L14 | check（カテゴリ A・必須） |
| 同上 | L15 | clippy（カテゴリ A・必須） |
| 同上 | L77 | check（Windows のみ節） |
| 同上 | L78 | clippy（Windows のみ節） |
| `package.json` | L17 `verify` | check |

**4 ファイル 9 箇所すべてが消える。**

## 消せない写像（`-p` が残る）

`cargo test -p <crate>` は hook（`core-test` / `tauri-test` / `settings-test`）・CI（`ci.yml` の 3 ステップ）・SSOT（カテゴリ A・Windows のみ節）に散在する。これは「どのディレクトリを編集したらどの crate のテストを走らせるか」という写像であり、`members` の写しではない。

ただし **4 つ目の crate が生えたら、`selectChecks` に分岐を、`buildCommand` に case を、CI に step を足す必要がある**。それを強制するのがカナリアの役目。

## カナリアが主張すべき不変条件

`post-edit.test.mjs` に置く（`tsconfig` / `vitest.config.ts` / `package.json` のカナリアと同じ場所・同じ様式）:

1. `Cargo.toml` の `[workspace] members` が `["snotra-core", "src-tauri", "snotra-settings"]` である
2. **ディレクトリ名 → パッケージ名**の対応が期待どおり（`src-tauri` → `snotra` の非自明な対応を含む）。`cargo test -p <pkg>` はパッケージ名を使い、`selectChecks` はディレクトリ名で判定するため、**両者の対応が崩れると発火と検査対象がずれる**

## 発火の追加（#497 の規律「発火とカナリアは対」に従う）

`Cargo.toml` は現在 `cargo-check` のみ発火する。カナリアへ到達させるには `hook-selftest` も撃つ必要がある:

```
Cargo.toml → ["cargo-check", "hook-selftest"]
```

`CARGO_MANIFEST`（basename アンカー）はメンバーの `Cargo.toml` にも当たる。**それでよい** — パッケージ名は各メンバーの `Cargo.toml` が真実源であり、カナリアはそちらも読む。

## 関連コード

- `.claude/hooks/post-edit.mjs` — `CARGO_MANIFEST`（L48 付近）、`CHECK_DEFINITION`（L52 付近）、`selectChecks`、`buildCommand`
- `.claude/hooks/post-edit.test.mjs` — per-file 断言、カナリア群（末尾）
- `.github/workflows/ci.yml` — rust-check
- `docs/build-commands.md` — カテゴリ A（L14-15）、Windows のみ節（L77-78）、整合規約（L23）、CI/CD 対応表
- `package.json` — `verify`
- `.claude/skills/health-check/SKILL.md` — Check 5（`-p <crate>` を `Cargo.toml` のメンバーと照合する規定）
- ルート `CLAUDE.md` — フック表、「検査の定義を変えるファイル」の箇条書き

## 技術的制約

- **整合規約（#476）に自ら従う**: hook の cargo コマンドは SSOT カテゴリ A と「合否・検査対象を変えるフラグにおいて一致」。`--workspace` は両方に入るので一致は保たれる
- **health-check Check 5 の文面**: 「`cargo` コマンドは `Cargo.toml` のワークスペースメンバーと照合する（`-p <crate>` のクレート名が存在するか）」は `test -p` に対して依然有効。`--workspace` は照合不要（cargo が真実源を読む）——文面に一言添える
- `cargo clippy --workspace` は `--all-targets` と併用しても挙動不変（実測）

## 未解決の疑問

なし（等価性・所要時間・写しの所在・members の真実源すべて実測済み）。
