# 計画: #1012 scripts に formatter の CI ゲートを新設しないことを記録する

## 目的と受け入れ条件

利用者の判断（2026-11-18）は「入れない」である。formatter ゲートを新設しない決定と、その根拠になった測定を ADR に残す。根拠は `workspace/research.md` にある。

- AC1: `docs/adr/ADR-script-formatter-gate-rejected.md` が在り、次の 4 点を書いている
  - 決定（新設しない）
  - 捕捉実績（3 PR・4 行、下限）と、その数え方。squash 専用なので、目視の捕捉はブランチの中でしか観測できないこと。−/＋の対だけを見る述語では取りこぼすこと
  - 却下した案（入れる）の費用。devDependency・CI step・全体の整形 PR と `.git-blame-ignore-revs` の 2 段
  - 受容する残余と、この決定を見直す条件
- AC2: `npm run governance:check` が通る（G-adr-file-names・G-references などが新しい ADR を受け入れる）
- AC3: ADR に書いた数とコミット SHA が、一次資料（`git show`・`refs/prhead/*`）と一致する

## 変更するファイル

| ファイル | 変更 |
|---|---|
| `docs/adr/ADR-script-formatter-gate-rejected.md` | 新規 |

触らないもの: `package.json`、`.github/workflows/ci.yml`、`docs/build-commands.md`。ゲートは置かないので、射程を書く対象が無い。「書式の検査は無い」と不在を書き足すのは、書く約束の「必要なことだけ」に当たらない。`.claude/rules/safety-nets.md` にも書き足さない。今回の手順は、既存の条項（捕捉実績で身の証を立てる・単独で検出する経路を数える）をそのまま当てたものなので、新しい規範は生じていない。

## ADR の骨子

- 文脈: #1011 の `=[`（2 行）を目視だけが捕まえた。Rust 側は `ADR-rustfmt-gate` で CI ゲートを置いている。
- 決定: scripts（`.mjs`・`.ps1`・`.psm1`）に formatter のゲートを置かない。
- 根拠となる測定:
  - 母集団: main の全コミットと、全 PR のブランチ先端 945 本の `merge-base..head` を合わせた 714 コミット
  - 書式の逸脱: #1011（2 行。`f2e4b04c` で混入し、`b68ce6b7` で直った）、#1305（`27da65b9` で混入し、`c0461a0e` で直った）、#132（`bc30cbbe` で main に入り、`ce580ea9` が行ごと消した）
  - 数え方の落とし穴を 2 つ書く。main だけを数えると 0 件に見えること（squash）。−/＋の対だけを見る述語では、直されなかった逸脱と、同じ行で別の箇所も変えた修正を取りこぼすこと
- 却下した案: 入れる。`ADR-rustfmt-gate` と比べて書く。あちらは 460 ハンクが実際に積み上がり、PR #857 で実害が出ていた。こちらは 4 行で、害は無かった。費用は、表示幅で 80 桁を超える `.mjs` の行が 1,883 行あるので（当初の 514 行・1,031 行は数え違い）、全体の整形が要る見込みである（prettier そのものでは測っていないことを書く）
- 受容する残余: 書式の逸脱は、目視で捕まえるか、main に残る。意味の誤りではないので、残っても挙動は変わらない
- 見直す条件: 書式の逸脱が main へ届き、それを直す PR が繰り返し要るようになったとき

## 実装の順序

1. ADR を書く
2. 数と SHA を一次資料で照合する
3. `npm run governance:check` を回す

## 不変条件と異常系

- ADR のファイル名は `ADR-<slug>` の形にする（G-adr-file-names）。連番は付けない。
- ADR から他の文書を指すときは、ファイル名と見出し名で書く（`.claude/rules/governance-docs.md`）。

## テスト方針と検証コマンド

- `npm run governance:check`（カテゴリ: ガバナンス文書）
- 数の照合: `git show f2e4b04c b68ce6b7 27da65b9 c0461a0e -- scripts | grep -E '=\[|=\('`、`git merge-base --is-ancestor bc30cbbe origin/main`

## 作業項目

- [x] `docs/adr/ADR-script-formatter-gate-rejected.md` を書く
- [x] ADR の数と SHA を一次資料で照合する（6 つの SHA・#1011 の 2 行・#1305 の修正行・#132 の main 到達と削除・945 本・80 桁超を表示幅で 1,883 行と再測定。514 行・1,031 行（バイト長）の数え違いを直した）
- [x] `npm run governance:check` を通す（主ツリーで passed・13 件）

## 未確定（実装前に潰す）

- [x] 入れるか入れないか — 利用者が「入れない（推奨）」を選んだ（2026-11-18）。ADR を残す案を含む選択肢である
- [x] ADR の slug が既存と衝突しないか — `ls docs/adr | grep formatter` は 0 件

## セルフレビュー

- リスク: 通常（ADR を 1 本足すだけ。永続化・並行性・状態遷移に触れない。網羅性を要件にしない）
- plan-review: 未実施（通常リスク）
- エージェント数: 1（3b の敵対的調査）
- 要対処: 0 件。5a の照合: (1) issue の論点のうち、費用・捕捉実績・入れない結論は ADR で受ける。射程の論点は、入れないので対象外 (2) 境界条件は「数が一次資料と合うか」で、AC3 が受ける (3) 新しい状態・リソースは無い (4) より単純な案は「issue のコメントだけ」で、利用者が退けた (5) 壊してはならない不変条件は ADR の命名と参照で、governance:check が検知する
- 未検証: prettier と PSScriptAnalyzer による drift の総量（ゲートを入れないので測らない）
- 散文の全件列挙: 未実施（通常リスク）

## 人間レビュー

- [x] 承認済み — 2026-11-18 / 問い: "workspace/plan.md の計画を承認なさいますか？" / 回答: "承認する"
- [x] 委譲ラウンド 1: sha 8eedbe02 / ツール green（E・F）/ 指摘 High 1・Medium 1・Low 2・⚠️3（High・Medium・Low をすべて修正。⚠️は数と結論に効かず不採用）/ 変異 2 件とも検知・生存 0
- [x] 委譲ラウンド 2: sha dadb97f4 / ツール green（E・F）/ 前回 4 件の解消を確認・新規 Low 1（測っていない不等式）を修正・⚠️1（L2 のレビュアー側は issue 本文が出所なので不採用）/ 変異 2 件とも検知・生存 0
- [x] 委譲ラウンド 3: sha cc560fec / ツール green（E・F）/ 指摘 0（Low 解消・⚠️ は issue 本文で裏取りされ外れた）/ 変異 2 件とも検知・生存 0
