# 調査: issue #1508 — 文書だけの PR で rust-check を早く終える

## issue の要約

文書（`.md`）だけを変える PR でも、`ci.yml` の `rust-check`（windows・約 7 分）が毎回走り、マージがその完了待ちで止まる（#1501・#1503・#1505）。受け入れ条件は 2 つ:

1. 文書だけの PR で `rust-check` が早く緑で終わり、PR が CLEAN になる
2. Rust のビルドにかかわるファイルを変えた PR では従来どおり走る（判定の境界を確かめた記録がある）

## 事実（実測）

- **issue の「落とし穴」の前提は今は当たらない。** ruleset `default`（id 12941497）の規則は `deletion` / `non_fast_forward` / `pull_request` だけで、required status checks を持たない。`main` に branch protection も無い（`gh api .../branches/main/protection` が 404）。`docs/build-commands.md`「CI/CD メモ」にも「CI は required status check ではない」とある。
- **待たされた本当の原因は `merge:pr` の CLEAN 要求である。** `scripts/merge-pr.mjs` の事前検査が `mergeStateStatus !== "CLEAN"` を拒む。必須でないチェックが実行中だと UNSTABLE になる。過去のセッション記録に「CLEAN でない（UNSTABLE）」が 40 件余り、「（UNKNOWN）」が 20 件余りある（数え方で 45/24 と 43/23 に分かれた）。ただし、UNSTABLE が pending 由来か failure 由来かは区別していない。`.md` だけの PR で pending 中に UNSTABLE を返したことは、まだ実見していない（敵対的調査 W1）。
- #1501・#1503・#1505 の差分は `.md` だけ（`.claude/skills/**/SKILL.md`・`AGENTS.md`・`.claude/agents/code-reviewer.md`・`docs/build-commands.md`・`.github/pull_request_template.md`）。3 件とも node-check は約 20 秒、governance-check は約 7 秒、rust-check は約 7 分だった。
- **rust-check は Rust 以外も検査する。** cargo の各 step に加えて、Windows の `npm test`（vitest: `.claude/hooks`・`.githooks`・`scripts`）と Pester（`npm run test:powershell`）を走らせる。Pester の統合テストは、同じ job の `cargo build -p snotra` が作る実バイナリに依存する。
- **Rust 側は `.md` を読まない。** `include_str!` / `include_bytes!` は全件（切り詰めなし）で `.rs` と `.png` だけを取り込む。`read_to_string` で `.md` を読むコードも無い（`.md` が出てくるのは拡張子のテストデータだけ）。`build.rs`（`snotra-settings/build.rs`・`src-tauri/build.rs`）も `.md` を参照しない。`src-tauri/capabilities/README.md` は tauri の ACL ビルドが読まない（同ファイルに tauri-utils のソースを根拠とした記述がある）。
- **vitest のうち、実物の `.md` を読むテストが少なくとも 3 つある。** `scripts/verify-tree.test.mjs` の「parseCategoryCommands — 実際の docs/build-commands.md」が `docs/build-commands.md` を読む。`scripts/governance-check.test.mjs` の実リポジトリのスモーク（dogfood）が、全検査を実リポジトリへ当てる。`scripts/governance/checks/G-module-linkage.test.mjs` のカナリアが各 crate の `CLAUDE.md` を読む（敵対的調査 B1）。いずれも node-check（ubuntu）と rust-check（windows）の両方で走る。Pester は `.md` を読まない。このため、`docs/build-commands.md`「CI/CD メモ」の「skip-safe なのは、node-check / rust-check がテスト対象に持たない … `docs/**`・`**/*.md` だけ」は、今でも厳密には偽である。
- **SKIPPED の job が `merge:pr` の CLEAN を通すかは未実測。** skip-ci を貼った #585（2026-07-18）では rust-check が SKIPPED で完了しマージされた。ただし `merge:pr`（#1312）より前で、mergeStateStatus を見ていない。
- `docs/` の下に `.md` 以外のファイルは無い。`.claude/skills`・`.claude/rules`・`.claude/agents` の下も、すべて `.md` である。
- 追跡されている `.md` は 140 件。大文字の拡張子（`.MD` 等）は 0 件。
- `docs/build-commands.md` の `governance manifest delta` step の記述は古い。その step は #1254（3a3e70f8）で撤去済みで、今の `ci.yml` の governance-check は 1 step だけである。

## 関連するファイル・シンボル

- `.github/workflows/ci.yml` — job `rust-check`（判定の追加先）・`node-check`・`governance-check`。`concurrency` は PR で `cancel-in-progress`。
- `.github/workflows/smoke.yml` — PR 側は `paths` で起動を絞る。`src-tauri/**`・`snotra-egui-runtime/**` を含むので、`src-tauri/CLAUDE.md` などの `.md` だけでも起動する（敵対的調査 B2。#1272・#1420 で実在）。`.md` を negation で外さないことは同ファイルのコメントが決めている（「拾いすぎ（配下 *.md 等）は過剰実行で安全側ゆえ negation を足さない」）。今回は触らない。
- `scripts/merge-pr.mjs` — CLEAN の要求（触らない）。
- `scripts/verify-tree.test.mjs` — 実物の `docs/build-commands.md` を読むテスト。
- `docs/build-commands.md`「CI/CD メモ」—「検証コマンド ↔ GitHub Actions workflow の対応」の表と、`skip-ci` の節。「変更後の検証チェックリスト」の「CI の rust-check が、PR ごとに workspace の全 crate のテストを実行する」。
- `.github/labels.yml` の `skip-ci`（説明文に撤去済みの `frontend-check` が残っている。今回の範囲外）。
- `scripts/governance/checks/G-module-index.mjs` の `//!`（「`skip-ci` ラベルの付いた PR では走らない」）。カナリアは `npm test` で走り、ubuntu の node-check は今回も常時走るので、偽にならない。

## 再利用できる既存のパターン

- job の `if` に `github.event_name == 'push' ||` を前置して、main への push では必ず走らせる（node-check / rust-check の現行形）。main の rust-check は rust-cache を保存する唯一の経路でもある（`save-if`）。
- 判定ロジックを `scripts/*.mjs` に置き、純関数を export して `*.test.mjs`（vitest）で境界を固定する（`merge-pr.mjs` 等）。`node scripts/...` で起動し、shebang を置かない（CRLF 化の理由が `merge-pr.mjs` の冒頭にある）。
- `actions/checkout` の既定は PR の merge commit（`refs/pull/N/merge`）。その第 1 親は base の tip、第 2 親は PR の head である。`fetch-depth: 2` で `HEAD^1` まで取れる。

## 技術的な制約

- 判定の母集団は PR 全体の差分でなければならない。直近の push 分だけを見ると、Rust を変えた PR に文書のコミットを足した run で省かれる（前の run は `cancel-in-progress` でキャンセルされる）。merge commit と第 1 親の diff は、PR 全体の変更を現在の base に対して表す。
- `git diff` はリネームを 1 行（新しいパス）にまとめうる。`.rs` を `.md` へ改名した PR を見落とさないよう、`--no-renames` で旧パスの削除も出す。
- fail-closed にする: 差分の取得失敗・0 件・判定の出力が無いときは、すべて「走らせる」へ倒す。
- skip を job の `if`（別 job の `needs`）で行う形は、上流の失敗・skip で下流も黙って skip になりうる。step の `if` で行う形なら job の conclusion は常に SUCCESS で、SKIPPED の扱い（未実測）に依存しない。
- Node はセットアップ後に使う（runner のプリインストールへ暗黙に依存しない。`ci.yml` の Rust toolchain の step のコメントと同じ方針）。

## 未解決の疑問

- Windows の runner の起動と checkout・setup-node だけで、早く終える run が何秒になるか（PR の CI で測る）。
- 早く終えた run で、`merge:pr` が CLEAN を受け取るか（PR の CI で測る）。
- 文書だけの PR で Windows の `npm test` が走らなくなる。実物の `.md` を読む vitest（上の 3 つ）の Windows 固有の壊れ方（CRLF 等）は、その PR では検出されなくなる。内容の壊れは ubuntu の node-check が検出し、マージ後の main への push では Windows でも走る。→ 受容する残余とする（issue が求める「文書だけの PR で rust-check を早く終える」の直接の帰結であり、push 側で必ず走るため、壊れても main の CI で次のマージ前に見える）。

## 敵対的調査（`workspace/adversarial-1508.txt`）の採否

- B1（`.md` を読む vitest は verify-tree だけではない）: 採る。上の事実と残余を直した。
- B2（`.md` だけでも Smoke が起動する PR がある）: 採る。`smoke.yml` は触らない（negation を足さない決定が同ファイルにある）。受け入れ条件 1 は「rust-check を早く終える」に限り、`src-tauri/**` 等の `.md` だけの PR では smoke-egui が走り続けることを計画と PR 本文に書く。
- W1（UNSTABLE が pending 由来かは未確認）: 測定を PR の CI へ置く。この PR 自身は `.md` 以外を含むので、rust-check の実行中に `gh pr view --json mergeStateStatus` を読めば、pending で UNSTABLE になるかを測れる。
- W2（全 step を飛ばした job が SUCCESS になり check 名が出るか）: この PR の CI では測れない。PR の差分は `ci.yml` と `scripts/` を含むので、文書だけにはならない。そこで、マージ後に最初に来る文書だけの PR で確かめることを PR 本文に書く。判定の step 単体の挙動は、ローカルの CLI 実行で確かめる。
- W3（`core.quotepath` で非 ASCII パスが引用される）: 採る。`-z` で読む（`-z` は引用しない）。テストに非 ASCII のパスを入れる。
- W4（衝突のある PR では workflow が起動しない）: 既知の GitHub の挙動で、今回の変更で変わらない。不採用。
- W5（ラベルの付け外しで再起動しない）: 今回の変更で変わらない。不採用。
- W6・W8（所要の再測定）: PR の CI で測る。
- W7（Windows の CRLF での dogfood）: 上の残余に含めた。
- K1〜K7（壊せなかった項目）: ruleset・Rust の `.md` 非依存・vitest/Pester の範囲・merge commit の diff と `--no-renames`・`.md` の分布・古い記述・skip-ci の記述は、主張どおりと確かめられた。
