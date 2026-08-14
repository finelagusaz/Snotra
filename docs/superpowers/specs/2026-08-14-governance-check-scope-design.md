# `governance:check` の責務を、捕獲実績で剪定し、母集団の縮小を機構へ載せる

作成日: 2026-08-14 / ブランチ: `chore/governance-check-scope` / issue: #1088 / 前提: #587（導入）・#1085（G-module-linkage 新設で #1088 が生まれた）

#1088 は「検査の登録配列から 1 行落ちても沈黙する」という配線の穴を問うている。呼び出し側はそこへ
**責務が過大ではないか**という上位の問いを重ねた。本設計は後者を主題に置き、#1088 をその内側で解く。

なお `docs/superpowers/` は `governance:check` の照合母集団の外にある（#589 で非規範化）。
**本ファイルの参照は機構に守られない**——引用の実在は書いた時点の実測でしか担保されない。

## 1. 対象 — 規模と、捕獲実績

`scripts/governance-check.mjs` は 2142 行 / 137KB、`scripts/governance-check.test.mjs` は
1935 行 / 115KB、検査は現状 20 件（うち 1 件は判定を持たない計器であり、3.2 で検査配列から外す）。
実行は 1.3 秒で、**実行費用は問題ではない**。

各検査 ID を触ったコミット数（`git log -S` 実測）は 1〜4 件、大半が 1。**コードの改修頻度という意味での
保守コストも出ていない**。

痛みが在るのは捕獲の側である。#587 の導入（2026-07-28）以降、CI run 785 件を走査して
`governance-check` job が赤くなったのは **3 回**だけだった。

| 日付 | 検査 | 内容 | 結末 |
|---|---|---|---|
| 08-03 | G-references | ADR 内の `test-results/.last-run.json`・`.claude/settings.local.json` | 文書からバッククォートを剥がして解消 |
| 08-10 | G-workspace-lints | `[workspace.lints.rustdoc]` の deny 欠落 | 実質的な捕獲 |
| 08-14 | G-references | `docs/hooks.md` の `.claude/settings.local.json` | 文書からバッククォートを剥がして解消 |

**この 3 件は下限である**——手元で赤くなって修正したものは痕跡を残さない。それでも読み取れる事実が 1 つある:
**3 件中 2 件は文書の誤りではなく、gitignore 済みのファイル名を「実在しない」と見なした誤爆**であり、
**対処が「散文の表記法を検査に合わせて曲げる」方向だった**。傷跡は現在も `docs/hooks.md` に残っている
（「gitignore 済みゆえバッククォートで参照しない——CI のチェックアウトに存在せず `G-references` が
赤くなる」）。機構が散文の書き方を支配し始めており、これが「本質的な発見をしにくい」の実体である。

## 2. 責務の混在

20 件は 1 つの層に見えて、性質が 4 つ混ざっている。

| 性質 | 件数 | 検査 |
|---|---|---|
| 参照の実在・着地 | 5 | G-references, G-heading-refs, G-near-heading-refs, G-adr-citations, G-stale-identifiers |
| 索引・表 ⇄ 機構の集合照合 | 11 | G-module-index, G-module-linkage, G-architecture-table, G-ci-table, G-rules-globs, G-skill-table, G-check-skill-enumeration, G-build-commands, G-hook-commands, G-hook-fires, G-spec-sections |
| リポジトリ規約（文書ではない） | 2 | G-workspace-lints, G-clippy-disallowed |
| **判定を持たない計器** | 1 | G-area-instrument |
| 命名規約 | 1 | G-adr-file-names |

## 3. C — 剪定（先頭・最小工数）

### 3.1 G-references に gitignore の 3 分類を入れる

`git check-ignore` は**ファイルの存在に依らずパス名だけで判定する**（2026-08-14 実測。
`test-results/never-created-file.json` が不在のまま `.gitignore:34` に当たり、
`docs/nonexistent-typo.md` は当たらない）。ゆえに CI のチェックアウトでも手元と同じ判定が出る。

| 状態 | 判定 |
|---|---|
| 実在する | 緑（現行どおり） |
| 実在しないが ignore 対象 | **緑**（生成物・ローカル設定を意図して指している） |
| 実在せず ignore 対象でもない | 赤（typo の検出——本来の目的） |

- spawn は 1 回に束ねる。実在しなかったパスだけを集めて `--stdin -z` で一括判定する。
- **exit 1 は「該当なし」であって失敗ではない**（失敗は 128）。素直に書くと正常系で例外になる。
- 純関数の契約は「ignore 判定関数を snapshot へ注入する」形で保つ（production は batch した
  subprocess、テストは fixture の述語）。列挙そのものは現行どおり `fs` に問う——`makeSnapshot` が
  git 列挙を避けた理由（pathspec の glob 意味論）は引数なしの `check-ignore` には当たらない。
- **受容する残余**: ignore 対象ディレクトリ配下の typo は緑になる（`target/ryo.rs` は素通りする）。
- 残る手元 / CI の乖離は「未追跡かつ非 ignore のファイルへの参照」だけになる。**それは CI が赤くて
  正しい**（`git add` 忘れの検出）。未追跡だが存在するファイルは現在 4 件（実測）。

### 3.2 G-area-instrument を検査配列から外す

面積に合否は無い（`ADR-retire-area-budget`）。`.claude/rules/governance-docs.md` は既に
「`governance:check` は実測値を報告するだけで、判定はこの約束が持つ」と書いており、**機構を規範の
記述へ揃える変更**である。evidence の出力自体は残し、「全検査 passed」の件数からのみ外す。

### 3.3 docs/hooks.md の同期

3.1 が入ると「gitignore 済みゆえバッククォートで参照しない」という主張は**偽になる**。同じ差分で直す。
**ADR 側（`ADR-stale-identifier-detector-scope`）は戻さない**——凍結された歴史である
（`ADR-adr-frozen-history`）。

### 3.4 却下 — Cargo.toml 系 2 件の帰属見直し

`G-workspace-lints` / `G-clippy-disallowed` は「ガバナンス文書の検査」ではないが、**唯一の検出器で
移す先も無い**。変わるのは名乗りだけなので行わない。スクリプト冒頭の契約コメントに 1 行添えるに留める。

## 4. B — 構造母集団の manifest 差分

### 4.1 なぜ構造だけに当てるか（実測）

直近 20 コミット（2026-08-12〜08-14）で各母集団を測った。

| 母集団 | 変動回数 | 性質 |
|---|---:|---|
| 検査件数 | 1（#1085 の 19→20 のみ） | 構造 |
| 対象文書 35 / rules 8 / skills 12 | 0 | 構造 |
| 見出し参照 193→209 | 11 | 散文 |
| 恒久規範の文字数 15006→15595 | 6 | 散文 |

**全項目に当てれば 20 コミット中 11 回以上赤くなり、承認は確実にゴム印化する**（測定窓は 3 日・20 コミット
であり、この窓では構造母集団の変動は 0〜1 回だった）。散文側の件数は evidence のまま残す。

### 4.2 件数ではなく集合を吐く

manifest は sorted な**集合**（検査 ID・対象文書・rules・skills のパス列）とする。件数では
「1 消して 1 足す」が沈黙する。集合なら diff がそのまま承認の材料になる。

副産物として、A の完了後に**検査ファイルごと削除された場合**——A 単独では沈黙する残余——も
B が捕まえる。A と B は重複ではなく補完である。

### 4.3 承認チャネル

PR 本文に delta を宣言し、**CI が main と PR の両 manifest を独立に計算して宣言と突き合わせる**。
機械が両側の実測を持つので、期待値の著者と変更の著者が同一であることによる誤りの相関を避けられる
（宣言だけを信じる形は `grounding-test-becomes-fixed-point` と同型になる）。squash マージで
main のコミットメッセージにも残る。

**宣言セクションを持たない PR が既定の経路である**。fail-closed の向きだけをここで決める——
**diff が無ければ green、diff が在って宣言が無ければ赤**（宣言セクションの書式は実装計画で決める）。

**`governance-check.mjs` の外に置く**（別スクリプト + CI step）。PR 本文の読取や main の checkout を
核へ入れると「依存ゼロ・決定的（ネットワーク・時刻・環境変数に非依存）」の契約が壊れる。

## 5. A — per-check 分割（最後）

`scripts/governance/checks/*.mjs` へ 1 検査 1 ファイル。各ファイルは `id` と `run` を export し、
registry は `readdirSync` から導出する——**忘れうる登録行が存在しなくなり、#1088 が構造的に消滅する**。
ID の写しも生まれない。

- **`readdirSync` の順序は ext4 で不定**。明示 `sort` しないと CI と手元で出力順が割れる。
- registry は import 時に形（`id` と `run` を持つか）を検証する。
- `G-hook-fires` が `.claude/hooks/post-edit.mjs` の `selectChecks` を import する例外は保存する。
- テスト側（115KB）も同じ構造へ分割する。

## 6. 順序と、各段の検算

**C → B → A** の 3 PR に分ける。A は工数が最大で最後に置くため、**途中で止めても C と B は残る**。

フォールトインジェクションは稼働中のガードを弱めず複製に当てる（`.claude/rules/safety-nets.md`
「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」）。

| 段 | 変異 | 期待 |
|---|---|---|
| C | 実在しない非 ignore パスを注入 | 赤のまま |
| C | ignore 対象の不在パスを注入 | 緑（新） |
| C | 08-03 / 08-14 に赤くなった形を再現 | 緑（新） |
| B | 複製で登録を 1 本消す | manifest diff が発火 |
| A | 検査ファイルを 1 本削除 | B が赤にする |

B の変異が、#1088 が求めた「その検知器が発火しうるかを先に測る」の実測そのものになる。

## 7. 受容する残余

- ignore 対象ディレクトリ配下の typo（3.1）。
- 散文側の母集団（見出し参照・文字数）の増減は誰も承認しない——変動が多すぎて機構に載せられない。
- 意味の側（責務の妥当性等）は従来どおり `/health-check` に残る。
