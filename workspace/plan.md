# plan — #858 cargo fmt をゲートへ戻す

## 目的

`cargo fmt` を、リポジトリ本来の様式（初期コミットで drift 0）へ**復旧**したうえで、
人の習慣ではなく**機構**（PostToolUse hook + CI）に持たせる。マシンが変わっても途切れない状態にする。

## 受け入れ条件

1. `cargo fmt --all -- --check` が `main` で **exit 0**
2. `*.rs` を未整形の形で編集すると **PostToolUse hook が赤くなる**（フォールトインジェクションで実測）
3. 未整形のコミットを push すると **CI（rust-check）が赤くなる**（同・実測）
4. `npm run governance:check` が緑（`G-hook-commands` / `G-ci-table` の照合を含む）
5. B・A′・hook 自動整形を**なぜ採らなかったか**が ADR に残る
6. 既存の検査（clippy / 全 crate test / cargo doc）が整形後も緑

## 変更ファイルと対象シンボル

| # | ファイル | 対象 | 変更 |
|---|---|---|---|
| 1 | `*.rs` 63 ファイル | — | `cargo fmt --all`（機械的・単独コミット） |
| 2 | `.github/workflows/ci.yml` | `rust-check` job | `Setup Rust toolchain` へ `components: rustfmt` を明示 + `cargo fmt --all -- --check` step をその直後へ |
| 3 | `.claude/hooks/post-edit.mjs` | `selectChecks` / `BUDGETS` / `buildCommand` | `fmt` 検査を 3 点セットで追加 |
| 4 | `.claude/hooks/post-edit.test.mjs` | **`:103,107,116,120,124,125,232,458`（8 箇所）** | 期待配列の先頭へ `"fmt"` |
| 5 | `docs/build-commands.md` | カテゴリ A / 参考コマンド / CI 対応表 / hook 記述 / blame 設定 | 5 箇所 |
| 6 | `docs/adr/ADR-rustfmt-gate.md` | — | 新規（否定の知識）。**先頭行は `# ADR-rustfmt-gate: <題>`**（`G-adr-file-names` が stem と見出しの一致を要求・`governance-check.mjs:1396-1430`） |
| 7 | `.git-blame-ignore-revs` | — | 新規（整形コミットの SHA） |
| 8 | `docs/hooks.md` | `:46` の `*.rs` 行 | **`selectChecks` の写しであり、内容を検査する機構が無い**（下記） |
| 9 | `.claude/rules/src-tauri.md` | `:28` の `A（clippy/test）` | `A（clippy/test/fmt）`（軽微・同じコミットで） |

**#4 の 8 箇所について**: `:232`（`checksForPayload` 経由）と `:458`（`resolveTarget` 経由）は
`selectChecks` を直接呼ばない `it` ブロックに `.rs` の期待配列が埋まっている。
`npm test` で赤くなるので沈黙経路ではないが、当初の「6 箇所」は事実として誤りだった（独立レビューが worktree 実測で発見）。

**#8 が重要な理由**: `docs/hooks.md:44` は「**正本は `selectChecks` である**」と自己申告する索引だが、
**その内容を検査する機構は無い**——`G-hook-commands` が見るのは `docs/build-commands.md` だけで、
`G-references` / `G-heading-refs` は参照の実在と見出しの着地しか見ない。ゆえに `governance:check` は緑のまま
`docs/hooks.md` だけが黙って古くなる。**この失敗は同型で一度起きている**——`governance-check.mjs:710-712` の
`AREA_BUDGET` コメントが、ルート `CLAUDE.md` の同じ表がドリフトしたので `docs/hooks.md` へ退去させたと記す。
その退去先が今度は同じ形で古くなる。**#3 / #4 / #5 のカテゴリ A 行と同じコミットに入れる。**

**触らない**: `rustfmt.toml` / `rust-toolchain.toml`（実測で不要・research.md）、`e2e.yml`、`.claude/settings.json`。

## 実装順序

**Phase 1 → 2 → 3 → 4 の順を守る。** Phase 2（機構）を先に入れると、整形前のリポジトリで CI と hook が
即座に赤くなり、自分の変更の検証ができなくなる。

### コミット境界（**フェーズ境界と一致しない**）

`G-hook-commands` と `G-ci-table` は**照合の向きが逆**なので、素直にフェーズ単位でコミットすると途中が赤くなる。

| 照合 | 向き | 安全な順 |
|---|---|---|
| `G-hook-commands`（`governance-check.mjs:640-656` — `hookCommands` だけを反復） | hook の cargo ⊆ カテゴリ A | **同一コミット必須**。カテゴリ A の行が無いまま hook へ足すと赤 |
| `G-ci-table`（同 `:442-475` — 表の行を反復） | 表のコマンド ⊆ workflow 本文 | **workflow が先**（表だけ先に足すと赤・workflow だけなら緑） |

→ **`post-edit.mjs` + `post-edit.test.mjs` + `docs/build-commands.md` のカテゴリ A 行 + `docs/hooks.md:46`
+ `.claude/rules/src-tauri.md:28` は 1 コミットにまとめる**（前 3 者は `G-hook-commands` の要求、
後 2 者は**検査が無いがゆえに、同じコミットに入れないと直す機会を永久に失う**）。
`ci.yml` と CI 対応表は分けてよいが、分けるなら `ci.yml` を先にする。

なお `post-edit.mjs` を編集した時点で hook-selftest が走り、`post-edit.test.mjs` を直すまでの間は**一時的に赤い**
（どちらの順でも起きる）。想定内ゆえ追わない。

### 併走ブランチとの衝突（Phase 1 前に確認済み・2026-08-01）

`git diff --name-only main...agent/streamline-norm-review -- '*.rs'` は **0 件**（変更は skill / docs の 4 ファイルのみ）。
open PR も無い。**63 ファイル整形の衝突相手は存在しない。**

### Phase 1 — 整形（単独コミット）

- [x] `cargo fmt --all` を実行する（他の変更を一切混ぜない）
- [x] `git diff --shortstat` が **63 files / +2159 −810** の同型であることを確認する（大きく違えば止まって原因を見る）
- [x] 検証: `cargo check --workspace` / `cargo clippy --workspace --all-targets -- -D warnings` /
      `cargo test -p snotra-core` / `-p snotra-egui-runtime` / `-p snotra` / `-p snotra-settings` /
      `cargo doc --workspace --no-deps --document-private-items`
- [x] コミットする（メッセージに「機械的整形のみ・`cargo fmt --all` の出力そのもの」と、`.git-blame-ignore-revs` へ載せる旨を書く）

### Phase 2 — 機構

- [x] `ci.yml` の `rust-check`。**`Setup Rust toolchain` へ `components: rustfmt` を明示**したうえで、
      **その直後**へ step を足す（ビルド不要＝最速で落ちる）:

      - name: Setup Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt

      - name: cargo fmt
        run: cargo fmt --all -- --check

- [x] `post-edit.mjs`:
  - `selectChecks`: `if (isRust) checks.push("fmt");` を **`clippy` の push より前**へ（0.69s ゆえ先に落とす）
  - `BUDGETS`: `fmt: { lines: 20, from: "head" }` を追加
  - `buildCommand` の `switch`: `case "fmt": return cargoSpec(["fmt", "--all", "--", "--check"]);`
    ——**`--message-format` のような出力整形フラグを付けない**（`G-hook-commands` の除去リストは
    `--message-format` のみ・他を足すとカテゴリ A と不一致になる）
- [x] `post-edit.test.mjs` の `.rs` 期待配列 **8 箇所**（`:103,107,116,120,124,125` に加え `:232` `:458`）へ `"fmt"` を追加する
- [x] 検証: `npm test`
      **訂正（code-reviewer の指摘・2026-08-01）**: 当初ここに「`BUDGETS 完全性カナリア` が
      **3 点セット**の欠落を捕まえる」と書いたが、カナリアが捕まえるのは `BUDGETS` の欠落 **1 本だけ**である。
      `buildCommand` の case 欠落は手書きの `it.each` リストが唯一の検知点で、そこを更新し忘れれば緑のまま通った。
      → **母集団を `Object.keys(BUDGETS)` から導出する形へ変更し、輪を閉じた**
      （`selectChecks` → カナリア → `BUDGETS` → 導出された母集団 → `buildCommand`）。

### Phase 3 — 文書

- [x] `docs/build-commands.md` カテゴリ A のコードブロックへ 1 行。**トークン列を `cargoSpec` と一致させる**:

      cargo fmt --all -- --check    # 必須: 整形（#858・CI と PostToolUse フックが発火）

- [x] 同 `:24` 付近の hook 記述へ fmt の発火を追記する（`*.rs` 編集で clippy と fmt）
- [x] 同「参考コマンド」ブロックへ `cargo fmt --all`（修復側）を追加する
- [x] 同 CI 対応表（`:184` の行）へ `cargo fmt --all -- --check` を追加する
      ——**`ci.yml` の `run:` と一字一句同じ文字列**（`G-ci-table` は verbatim 照合）
- [x] 同「スモーク運用メモ」の手前あたりへ、**ローカル `git blame` の 1 行設定**を書く:
      `git config blame.ignoreRevsFile .git-blame-ignore-revs`（GitHub 側は設定不要・自動）
- [x] `docs/hooks.md:46` の `*.rs` 行へ fmt を足す（**Phase 2 のコミットに含める**——上の「コミット境界」）
- [x] `.claude/rules/src-tauri.md:28` の `A（clippy/test）` → `A（clippy/test/fmt）`（同上）
- [x] `docs/adr/ADR-rustfmt-gate.md` を新規作成する。**否定の知識**を書く:
  - **B（使わないと明記）を採らなかった理由** — 初期コミットが drift 0 であり、様式は元から在った。
    B は「無かったものを導入しない」ではなく「在ったものを捨てる」宣言になる
  - **A′（既存様式へ寄せた `rustfmt.toml`）が成立しない実測** — 7 設定の表（research.md から転記）
  - **hook を自動整形にしなかった理由** — 既存フックはすべて「検出して報告する」側であり、
    Edit 直後にファイルが黙って変わるとエージェントの読んだ内容とディスクがずれる
  - **`rust-toolchain.toml` を置かなかった理由（条件つき却下）** — style_edition が rustfmt の安定性機構であり、
    edition 2024 から既に決まる。古い style_edition はいずれも drift を増やす（実測済み）。
    **前提は「CI の `@stable` とローカルの rustfmt が同じ style_edition で一致すること」であり、
    その前提が破れた場合はこの却下も破れる**——判定の観測点は初回 CI（上の「異常系」表）
  - **差分限定の fmt 検査（変更ファイルだけ見る）を採らなかった理由** — `cargo fmt` の判定を
    自作の差分スコープで写す形になる（ルート `CLAUDE.md` の「ツールの判定を自作しない」）
- [x] `npm run governance:check`

### Phase 4 — フォールトインジェクション（`.claude/rules/safety-nets.md` の要求）

**稼働中のガードを弱めない。故意に規則違反の入力を与えて拒否されることを見る**（= 行使・#482）。

- [x] **hook**: 既存の `*.rs` を一時的に未整形へ崩して Edit し、`--- fmt: 失敗 (exit N) ---` が会話に届くことを確認 →
      `git checkout -- <file>` で戻す
      **実測（2026-08-01）**: `snotra-core/src/binfmt.rs:63` の `let bytes = ` を `let bytes    = ` へ崩して Edit →
      `--- fmt: 失敗 (exit 1) ---` が**再現コマンドと差分つきで**会話に届いた。復元後は作業ツリー clean・`cargo fmt --all -- --check` が exit 0。
- [x] **正常系の対照**: 整形済みの状態で hook が沈黙し、CI が緑であることを確認する
      （赤しか見ないと「常に赤い」検査と区別できない）
      **実測（2026-08-01）**: 復元の Edit で hook は**沈黙**（`*.rs` は検査が割り当てられているので沈黙 = 合格）。
      CI の緑は下の CI 項目と同時に確認する。
- [x] **新設した検知経路そのものの FI**（code-reviewer の M3 を受けて追加・`.claude/rules/safety-nets.md` の要求）
      **実測（2026-08-01）**: `buildCommand` の `case "fmt":` を `case "fmt-FAULT-INJECTION":` へ改名 →
      `post-edit.test.mjs` と `pre-bash.test.mjs` の**両方が赤**になり、後者は
      `fmt の spec が null（buildCommand の case 欠落）` と原因を名指しした。復元後は 538 passed。
      **これは今回新しく閉じた輪である**——母集団を手書きしていた間は、リストの更新漏れで緑のまま通った。
- [ ] **CI**: 未整形の 1 ファイルを含むコミットを PR ブランチへ push し、**rust-check が赤**になることを確認 →
      整形して戻すコミットを push し、緑に戻ることを確認（2 コミットは実測の記録として残す）

      **⚠ 計画が見落としていた順序の制約（実装中に判明・2026-08-01）**:
      `ci.yml` のトリガーは `pull_request`（branches: main）と main への push だけである。
      **PR が存在しない feature ブランチへ push しても CI は起動しない**（`workflow_dispatch` も無い）。
      一方 `gh pr create` は `workspace/plan.md` の未チェック `- [ ]` を PreToolUse hook が拒む（#749）。
      ゆえに **この項目は「PR 作成前に閉じられない」**——計画のフェーズ内では循環する。

      **振り分け**: `AGENTS.md`「RETROSPECTIVE.md の運用」の規則どおり、**PR 内で閉じるタスクは PR 本文の
      チェックリストへ送る**。この項目は `/implement` の射程外であり、PR 作成時に本文へ移して閉じる。
      **`workspace/` はこの項目が残るため削除しない**（未チェックのまま削除すると PR 前ゲートの視界から計画が消える）。

## 不変条件と異常系

| # | 不変条件 | 破れたときの検知 |
|---|---|---|
| 1 | 整形コミットは**機械的変更のみ**（意味を変えない） | Phase 1 の `cargo check` / 全 crate test / clippy / doc がすべて緑 |
| 2 | hook の `fmt` id は `BUDGETS` に予算を持つ | `BUDGETS 完全性カナリア`（`post-edit.test.mjs:644`）が赤 |
| 3 | hook の cargo コマンドはカテゴリ A に同一トークン列で在る | `G-hook-commands` が赤 |
| 4 | CI 対応表の文字列は `ci.yml` に verbatim で在る | `G-ci-table` が赤 |
| 5 | 未整形コードは**ローカルで止まる**（push まで行かない） | Phase 4 の hook FI |
| 6 | hook が沈黙するとき、それは「合格」である | Phase 4 の正常系対照 |

### 異常系

| 症状 | 原因 | 対処 |
|---|---|---|
| CI step が `error: no such command: fmt` | action の `--profile minimal` が rustfmt を含まない | `components: rustfmt` を明示（計画へ反映済み） |
| **Phase 1 直後の clean tree で CI の fmt step だけ赤** | **CI の rustfmt とローカルの rustfmt が非一致**。整形はローカル `1.8.0-stable` で行うが、CI は `@stable` を実行時に解決する | `rust-toolchain.toml` で toolchain を固定する（issue の「決めること」で挙がっていた選択肢。**実測により現時点では不要と判断しているが、その判断が偽であることの唯一の観測点がこの初回 CI である**） |

**この 2 行目は形式ではなく本番の検査である。** `style_edition` が rustfmt の安定性機構であることを根拠に
事前固定を見送っているので、**その前提が正しいかは初回 CI でしか分からない**。ADR にも
「現時点では不要（前提: CI とローカルの rustfmt が同じ style_edition で一致する）」と**条件つき**で書く。

## テスト方針と検証コマンド

- 新規のユニットテストは**書かない**。`post-edit.test.mjs` の既存カナリアと `governance-check.mjs` の
  既存検査が、今回足す 3 点セットと文書整合をすべて捕まえる（`research.md`「再利用できる既存パターン」）
- 検証: `docs/build-commands.md` カテゴリ A 全部 + `npm test` + `npm run governance:check`
- `npm run test:powershell` / smoke は**対象外**（`*.rs` の整形は smoke の関心事に触れない）が、
  Phase 1 の整形が `src-tauri` を含むため **CI の e2e は PR で自動発火する**（`paths` に `src-tauri/**`）——その緑も見る

## SPEC.md・関連文書の更新要否

- **`SPEC.md`: 不要**。製品の挙動・状態遷移は変わらない（整形は意味を変えない・開発プロセスの変更である）
- **`docs/build-commands.md`: 必要**（上記 Phase 3）
- **`AGENTS.md` / ルート `CLAUDE.md`: 不要**。条件別チェック表の「各言語ファイルを編集 → `.claude/rules/`」で
  既にカバーされ、コマンド本体の SSOT は `docs/build-commands.md` である（二重メンテを増やさない）
- **ADR: 必要**（否定の知識が 5 件生じる）

## 未確定（実装前に潰す）

- [x] **`dtolnay/rust-toolchain@stable` が rustfmt を含むか** — **含む保証は無い**。
      action の `action.yml` は `rustup toolchain install <toolchain> ... --profile minimal` を実行しており、
      `minimal` プロファイルは rustfmt も clippy も含まない。**それでも現行 CI の `cargo clippy` が通っているのは、
      runner イメージに完全な toolchain がプリインストールされているためである**（rustup は既存 toolchain の
      component を削らない）。つまり現状は runner イメージへの暗黙依存であり、保証ではない。
      → **`Setup Rust toolchain` へ `components: rustfmt` を明示する**（計画の #2 へ反映済み）。
      **付随観察**: 同じ理由で `clippy` も保証されていない（本 issue の射程外・下の「観察」へ記録）
- [x] **`.git-blame-ignore-revs` を GitHub が自動で honor するか / ローカルは何が要るか** —
      **GitHub は自動**（公式ドキュメント: 「All revisions specified in the `.git-blame-ignore-revs` file,
      which must be in the root directory of your repository, are hidden from the blame view」。opt-in 不要）。
      **ローカル git は自動ではない**（実測: `git config --get blame.ignoreRevsFile` は未設定で、
      `git blame` は `--ignore-revs-file` か `blame.ignoreRevsFile` を要求する）。
      → ファイルはルート直下に置き、**ローカル側の 1 行設定を `docs/build-commands.md` に書く**（計画の #5 へ反映済み）

## 観察（本 issue の射程外・記録のみ）

- **`cargo clippy` も runner イメージ依存である**。`--profile minimal` の下では clippy も保証されないが、
  現行 CI は 5 か月間緑で通っている＝ windows-latest イメージが提供している。本 issue は fmt の話ゆえ
  `components: rustfmt` だけを足す。clippy も明示したいなら別 issue（1 語の追加で済む）

## セルフレビュー

- **リスク: 高**（`/plan-review`「リスク判定」の「hook、CI、rules、skills、ガバナンス文書を変更する」に該当）
- **plan-review: 独立レビュー 1 体**（`/plan-review` Step 2・観点は「既存検出器の挙動」「変更ファイルの漏れ」の 2 つ）
- **エージェント数: 1**（`general-purpose` / `model: sonnet`）。成果物は
  `workspace/plan-review-fmt-gate.md`（呼び出し側が絶対パスを指定・到達確認済み）。
  ユーザーの明示指示（2026-08-01「a 独立レビューを起動しよう」）で起動した
- **自己照合の結果（Step 1 の 5 項目）**:
  1. issue の全要件に作業項目が対応する — issue の A の 4 項目すべてと、「決めること」の
     toolchain 固定（→ 実測により不要と判断し ADR へ記録）に作業項目がある ✓
  2. 境界条件に検証がある — 「不変条件と異常系」表の 6 件すべてに検知手段が対応 ✓
  3. 新しい検査に正常/失敗の両経路がある — Phase 4 が赤（FI）と緑（正常系対照）の両方を実測する ✓
  4. より単純な既存パターンで置き換えられないか — A′（`rustfmt.toml` で既存様式へ寄せる）は 7 設定の実測で否定、
     差分限定 fmt は自作判定ゆえ却下、hook 自動整形はユーザー裁定で不採用。いずれも ADR へ ✓
  5. 不変条件に検知手段がある — 3 件は既存機構（`BUDGETS` カナリア / `G-hook-commands` / `G-ci-table`）が
     すでに持っており、**新しい検査を書かずに済む** ✓
- **適用した条件別チェック**: `.claude/rules/safety-nets.md`（セーフティネット変更 → Phase 4 の FI に反映）、
  `npm run governance:check`（ガバナンス文書変更 → Phase 3 に反映）。
  `/dry-check` は不適用（新規関数を定義せず、既存 `switch` へ case を 1 つ足すのみ）
- **要対処: 4 件**（すべて計画へ反映済み）
  - 自己レビュー由来 2 件（未確定の解消）:
    1. `dtolnay/rust-toolchain@stable` は `--profile minimal` ゆえ rustfmt を保証しない
       → `components: rustfmt` を明示（当初計画には無かった）
    2. ローカル `git blame` は `.git-blame-ignore-revs` を自動では読まない
       → `docs/build-commands.md` へ 1 行設定を追記（当初計画には無かった）
  - 独立レビュー由来 2 件（**どちらも呼び出し側が一次証拠で検算済み**）:
    3. **`docs/hooks.md:46` が変更ファイル一覧から漏れていた** — `selectChecks` の写しでありながら
       内容を検査する機構が無い（同型のドリフトが `AREA_BUDGET` コメントに記録された既往）→ #8 として追加
    4. **`post-edit.test.mjs` の対象は 6 箇所ではなく 8 箇所** — `:232` `:458` が漏れていた → 訂正
- **独立レビューが実測で裏づけた計画の主張 3 件**（使い捨て worktree に計画を実適用して `governance-check` を実行）:
  `G-hook-commands` の片方向照合で通ること / CI 対応表が verbatim 一致で通ること /
  `BUDGETS` を忘れるとカナリアが赤くなること
- **軽微: 1 件**（採用）— `.claude/rules/src-tauri.md:28` の `A（clippy/test）` の陳腐化 → #9 として追加
- **未検証: 2 件**（どちらも実装フェーズで解消する）
  1. Phase 4 の CI 側 FI は push しないと測れない
  2. ADR 本文の見出し形（`# ADR-rustfmt-gate: <題>`）は本文を書くまで検査できない
     ——ただし `G-adr-file-names` が赤にするので**沈黙経路ではない**（`governance-check.mjs:1396-1430` で確認）

## 人間レビュー

- [x] 承認済み — 2026-08-01 / 問い: "workspace/plan.md を承認しますか。" / 回答: "承認する"

同時に裁定された分岐 3 件（いずれも計画の推奨どおり・本文へ反映済み）:

| 問い（逐語） | 回答（逐語） | 計画への効果 |
|---|---|---|
| "Phase 4 の CI 側フォールトインジェクション（CI が実際に赤くなることの実測）をどうやりますか。" | "PR ブランチへ push して残す（推奨）" | Phase 4 のとおり。**赤・緑の 2 コミットを PR 履歴に残す**（それ自体が検査が効いている証拠） |
| "cargo clippy も runner イメージ依存です（action は --profile minimal）。今回一緒に直しますか。" | "射程外のまま（推奨）" | `components: rustfmt` のみ明示。clippy は「観察」節に記録して別 issue 候補とする |
| "rust-toolchain.toml （toolchain の固定）はどうしますか。" | "置かない（推奨）" | 条件つき却下のまま。**初回 CI が前提の唯一の検査点**（「異常系」表） |

**先行して決まっていた要求判断 2 件**（本サイクル冒頭）:

- 問い: "cargo fmt をどう扱いますか。" / 回答: "A. ゲートに戻す（推奨）"
- 問い: "ローカル側（PostToolUse hook）をどうしますか。" / 回答: "検査を追加する（推奨）"
