# research — #858 cargo fmt をゲートへ戻す

## issue の要約

`cargo fmt --all -- --check` が clean な `main` で不合格になる。CI にも `docs/build-commands.md` にも無いため
「ゲートである」とも「ゲートでない」とも記録されておらず、Rust を触った人が個別にこの赤へ出くわす（PR #857 で実際に起きた）。
A（ゲートに戻す）/ B（使わないと明記）のどちらかを決めて記録する。

**ユーザー裁定（2026-08-01）**: **A**。ローカル側は **PostToolUse hook に検査を追加する**（自動整形ではない）。

## 一次証拠（すべて本サイクルで実測）

### drift の由来 — 「未整形」ではなく「習慣の途切れ」である

同一 rustfmt（`1.8.0-stable (4a4ef493e3 2026-03-02)`）で過去コミットを検査した結果:

| コミット | 日付 | `cargo fmt --all -- --check` のハンク数 |
|---|---|---|
| `5589c13` init | 2026-02-16 | **0** |
| `92804be` | 2026-02-17 | **0** |
| `fd6cd42` | 2026-04-02 | 193 |
| `a1a95a7` | 2026-08-01 | **460** |

初期コミットが drift 0 になる。**このリポジトリの様式は最初から rustfmt 既定そのもの**であり、
2026-02-17〜04-02 のどこかで `cargo fmt` を回す習慣が途切れ、以後 monotonic に積み上がった。
ユーザーの証言（「別マシンでやっていた時に動いていた。だからこのマシンでは記録にない」）と一致する。
**ゆえに A は新しい様式の導入ではなく、リポジトリ本来の様式への復旧である。**

計測方法: `git worktree add --detach <commit>` した使い捨てツリーで実行し、`git worktree remove --force` で撤去。

### 対立仮説はいずれも否定された

- **edition 2021→2024 移行が原因** ではない: `fd6cd42`（edition 混在）で `style_edition = "2021"` を強制すると
  **202**（既定 193 より悪化）。現在も同様で、2015 / 2018 / 2021 はいずれも **495**、既定の 2024 が **460** で最小。
- **設定で吸収できる**（既存様式に合わせた `rustfmt.toml`）でもない: 7 通り測って既定が最小。

| 設定 | ハンク数 |
|---|---|
| **既定（設定なし）** | **460** |
| `max_width = 110` | 459 |
| `style_edition = "2015" / "2018" / "2021"` | 495 |
| `use_small_heuristics = "Max"` | 527 |
| `use_small_heuristics = "Off"` | 545 |
| `use_small_heuristics = "Max"` + `style_edition = "2015"` | 558 |
| 上記 + `max_width = 120` | 814 |

  → **A′（既存様式へ寄せた `rustfmt.toml` を置く）は成立しない。** 追加の設定ファイルは要らない。

### 整形コミットの実寸

`cargo fmt --all` の実測: **63 ファイル / +2159 −810**（`git diff --shortstat`）。
issue 本文の「460」は**ハンク数**であってファイル数でも行数でもない。

### 履歴上の事実

- `rustfmt.toml` / `.rustfmt.toml` / `rust-toolchain.toml` は **git 履歴に一度も存在しない**（`git log --all -- <各パス>` が空）
- `cargo fmt` は**追跡ファイルに全履歴を通じて 0 件**（`git grep`・`git log -S`）
  - 補足: `docs/superpowers/specs/` は untracked（`git ls-files` で 0 件）。Grep ツールでの 1 件はそこにあった

## 関連ファイル・シンボル（すべて grep で実在確認済み）

### 変更対象

| ファイル | 対象シンボル / 行 | 変更内容 |
|---|---|---|
| `*.rs` 63 ファイル | — | `cargo fmt --all`（機械的・単独コミット） |
| `.git-blame-ignore-revs` | — | **新規**。整形コミットの SHA を 1 行 |
| `.github/workflows/ci.yml` | `rust-check` job（`:60-124`） | `cargo fmt --all -- --check` step を追加 |
| `.claude/hooks/post-edit.mjs` | `selectChecks`（`:118`）/ `BUDGETS`（`:35`）/ `buildCommand` の `switch`（`:287`） | `fmt` 検査を追加 |
| `.claude/hooks/post-edit.test.mjs` | `:103,107,116,120,124,125` **＋ `:232`（`checksForPayload`）`:458`（`resolveTarget`）** の `toEqual` | 期待配列へ `"fmt"` を追加（**8 箇所**。当初 6 箇所としたのは誤りで、独立レビューが worktree 実測で発見） |
| `docs/build-commands.md` | カテゴリ A（`:13-21`）/ 参考コマンド（`:147-148` 付近）/ CI 対応表（`:184`）/ hook 記述（`:24`）/ blame 設定 | fmt を 5 箇所へ記載 |
| `docs/hooks.md` | `:46` の `*.rs` 行 | `selectChecks` の**写し**（`:44` が自己申告）。**内容を検査する機構が無い**ため手で直す |
| `.claude/rules/src-tauri.md` | `:28` の `A（clippy/test）` | `A（clippy/test/fmt）`（軽微） |
| `docs/adr/ADR-rustfmt-gate.md` | — | **新規**。否定の知識（B・A′・hook 自動整形の却下理由） |

### 触らないと決めたもの

- `rustfmt.toml` / `rust-toolchain.toml` — 上の実測により**不要**（既定が最小・style_edition は edition 2024 から既に決まる）
- `e2e.yml` — Rust の整形は smoke の関心事ではない
- `.claude/settings.json` — `matcher` は `Edit|Write` のままでよい（fmt は `*.rs` 編集で発火するので既存 matcher の内側）

## 再利用できる既存パターン

- **hook の検査追加**: `selectChecks` が id を返す → `BUDGETS` に予算 → `buildCommand` の `switch` に `cargoSpec([...])`。
  3 点セットであることは `BUDGETS 完全性カナリア`（`post-edit.test.mjs:644`）が固定する
  ——`selectChecks` の代表パスが発行する全 id に予算があることを検査するので、**予算を忘れると赤になる**（沈黙しない）
- **CI step の追加**: `rust-check` の既存 step と同形（`run:` 1 行）
- **`git diff` を使わない検出**: `cargo fmt -- --check` は exit code で判定し、差分は証拠として stdout へ出る
  ——`docs/development-principles.md`「検出は exit code、出力は証拠」と同型

## 技術的制約

1. **`G-hook-commands`（`scripts/governance-check.mjs:604`）** は hook の `cargoSpec([...])` を
   **`docs/build-commands.md` カテゴリ A の cargo 行の部分集合**として照合する（片方向）。
   hook へ `["fmt","--all","--","--check"]` を足すなら、カテゴリ A に
   `cargo fmt --all -- --check` を**トークン列が一致する形で**置く必要がある（行末 `#` コメントは除去される）。
   逆向きの制約は無い（カテゴリ A に足すだけなら壊れない）。
2. **`G-ci-table`（同 `:424`）** は CI 対応表の 1 列目のバッククォート内 `cargo ...` / `npm ...` が
   **workflow の本文に verbatim で現れること**を要求する。表へ書く文字列と `ci.yml` の `run:` を一字一句揃える。
3. **`G-build-commands`（同 `:386`）** は `npm run X` / `npm test` / `cargo test -p <crate>` だけを見る。fmt は対象外。
4. **`dtolnay/rust-toolchain@stable` は rustfmt を保証しない**（`action.yml` を確認・2026-08-01）。
   action は `rustup toolchain install <toolchain> ... --profile minimal` を実行し、`minimal` は rustfmt も clippy も含まない。
   現行 CI の `cargo clippy` が通っているのは **runner イメージにプリインストールされた toolchain のおかげ**であって
   action の保証ではない（rustup は既存 toolchain の component を削らない）。
   → `Setup Rust toolchain` へ **`components: rustfmt` を明示する**。
5. **セーフティネットの変更はフォールトインジェクションで一度実測する**（`.claude/rules/safety-nets.md`）。
   稼働中のガードを弱めず、**故意に規則違反の入力を与えて拒否されることを見る**のは「行使」であり許される（#482）。
6. **`.git-blame-ignore-revs` の SHA は整形コミットを作るまで確定しない**——記載は整形コミットの後になる（順序制約）。

## 未解決の疑問

→ `workspace/plan.md`「## 未確定（実装前に潰す）」へ送る。
