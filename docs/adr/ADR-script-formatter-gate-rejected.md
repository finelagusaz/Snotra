# ADR-script-formatter-gate-rejected: scripts の .mjs / .ps1 / .psm1 に formatter の CI ゲートを置かない

#1012。捕捉実績を数えてから、新設するかを決めた。

## 文脈

PR #1011 のブランチで、`REQUIRED_RUSTDOC_LINTS = [` と `STALE_EXTRA_DOCS = [` の 2 行が `=[` に変わった。`governance:check` も vitest も通り、目視だけがこれに気づいた（1 行はレビュアーが、もう 1 行は実装者が `git diff` を通しで読んで見つけた）。Rust 側には CI の `cargo fmt --all -- --check` がある（`ADR-rustfmt-gate`）。一方、`scripts/**` の `.mjs`・`.ps1`・`.psm1` と `.claude/hooks/*.mjs` には、書式を見る機構が無い。

## 決定

これらのファイルに、formatter のゲート（CI の job・step・hook）を置かない。

## 捕捉実績の数え方と結果

`.claude/rules/safety-nets.md`「検出器を置く・動かすとき」は「検知器を足す前に、その検知器が単独で検出する経路を数える」と定めている。それに従って数えた（2026-11-18）。

- **母集団**: main の全コミットと、全 PR のブランチ先端（`refs/pull/*/head` の 945 本）の `merge-base..head` を合わせた非マージコミットのうち、対象パス（`scripts/**` の `.mjs`・`.ps1`・`.psm1`、`.claude/hooks/*.mjs`、`.githooks/*.mjs`）に触れた 714 本である。
- **結果**: 書式の逸脱は 3 PR に 4 行あった。

  | PR | 行 | 経緯 |
  |---|---|---|
  | #1011 | `governance-check.mjs` の `=[` 2 行 | ブランチ上の `f2e4b04c` で混入し、同じブランチの `b68ce6b7` で直った |
  | #1305 | `race-boundaries.test.mjs` の `const g =(` | ブランチ上の `27da65b9` で混入し、同じブランチの `c0461a0e` で直った |
  | #132 | `bench-startup.ps1` の `$numericSettings= @(… }\| ForEach-Object` | `bc30cbbe` で main に入り、約 3 日後に `ce580ea9`（#195）が行ごと消した |

  main へ届いたのは #132 の 1 行だけで、書式を直されたのではなく、コードごと消えた。どの逸脱も挙動には影響していない。

数え方には、結果を 0 件に見せる罠が 2 つあった。

- **main の履歴だけを数えると、目視が捕まえた逸脱は 0 件に見える。** このリポジトリは squash 専用なので、ブランチの中で直った逸脱は main の履歴に残らない。#1011 の件も、main だけを数えたときは現れなかった。
- **−行と＋行の対（空白を除くと一致する対）だけを見る述語では、2 種類を取りこぼす。** 直されないまま消えた逸脱（#132）と、直すときに同じ行の別の箇所も変えた修正（#1305）である。追加行を直接見る検出器で、この 2 件を拾った。

この数は下限である。force-push で消えた途中のコミットと、同じコミットの中で混入して直ったものは見えていない。

## 検討した代替案と却下理由

- **formatter（prettier など）を devDependency に入れ、CI でゲートする**: 却下した。守るものが 714 コミットで 4 行、しかも害が無かったのに対して、費用は常に掛かる。
  - devDependency と CI の step が 1 つずつ増える。
  - 既定の様式に合わせるために、全体の整形が要る見込みである。prettier の既定の行幅は 80 桁である。`scripts` 配下の `.mjs`（43 本・7,316 行）で、表示幅（全角を 2 桁と数える）が 80 桁を超える行は 1,883 行ある。prettier はコメントと文字列を折り返さないので、このすべてが整形で変わるわけではない（コメントを除くと 968 行）。逆に、80 桁に収まる行も変わりうる。整形するなら、`ADR-rustfmt-gate` と同じく、整形だけの PR を先にマージし、その SHA を `.git-blame-ignore-revs` へ書く 2 段の手順が要る。prettier そのものを当てたときの drift の量は、測っていない。
  - PowerShell 側の formatter（PSScriptAnalyzer の `Invoke-Formatter`）は手元に無く、#132 の形を直すかも測っていない。
  - `ADR-rustfmt-gate` とは前提が違う。あちらは 460 ハンクの drift が実際に積み上がり、PR #857 で関係の無い差分が「不合格」として報告される実害が出ていた。初期コミットの drift も 0 で、守るべき様式が在った。
- **formatter の検査を編集時の hook に置く**: 却下した。編集時の自動検証（PostToolUse）は #1255 で撤去済みである（#1256 の ADR（削除済み））。それを書式のためだけに戻す理由は無い。

## 受容する残余

- 書式の逸脱は、レビューの目視で捕まえるか、捕まえなければ main に残る。どちらでも挙動は変わらない。
- 見直すのは、書式の逸脱が main に届き、それを直すためだけの PR が繰り返し要るようになったときである。

---

status: Accepted
関連: #1012・#1011・`ADR-rustfmt-gate`・`.claude/rules/safety-nets.md`
