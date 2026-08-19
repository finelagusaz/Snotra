# governance ドメイン Phase 2 — 台帳

> **For agentic workers:** この文書は**進みながら埋める台帳**であって、事前に全タスクを確定した計画書ではない。設計が「事前のクラスタリングは行わない（extract-on-second-use）」を決めているため、クラスタの形は移行しながら決まる。

**Spec:** `docs/superpowers/specs/2026-08-19-governance-population-anchors-design.md`（Phase の定義は §3、錨の作り方は §2.1）
**前段:** `docs/superpowers/plans/2026-08-19-governance-domains-phase1.md`（Phase 2 の入口 2 件と Phase 3・4 の着手条件は同文書「Phase 2 への申し送り」が正本。ここへ写さない）

**Goal:** 自前で `snapshot.files` を filter している検査をドメインへ寄せ、未移行マーカーを固定パスの 3 本だけへ減らす。

## 移行 1 件あたりの手順（毎コミット同じ）

1. 移行前の母集団を**旧 filter 式のまま**実 snapshot 上で評価し、件数と集合を控える
2. ドメインを `DOMAIN_SPECS` へ足す（**錨を併設する**——腕が複数あるなら腕ごとに 1 本。単一の腕でも `holds([], s)` が false になる形にする）
3. 検査を `ctx.domains.get(<name>).members` の消費側へ書き換え、`export const domains` を実名へ変える
4. **移送前後で集合が同一**であることを突き合わせる（`docs/superpowers/specs/…-design.md` §3 Phase 2 の完了判定）
5. `registry.test.mjs` の `FROZEN_UNMIGRATED` から当該 id を**同じコミットで**消す
6. 旧 filter 式の写しを木に残さない

## 状態

| 検査 | 母集団 | ドメイン | 状態 |
|---|---|---|---|
| `G-rules-globs` | `.claude/rules/*.md` | `ruleDocs` | 済 |
| `G-rules-script-coverage` | 判定を持つスクリプト | `ruleDocs` / `judgingScripts` | 済 |
| `G-skill-table` | `.claude/skills/*/SKILL.md` | `skillDocs` | 済 |
| `G-check-skill-enumeration` | 同上（実在の照合） | `skillDocs` | 済 |
| `G-module-index` | crate 配下の production `.rs` | `moduleIndexSources` | 済 |
| `G-module-linkage` | 同上（crate の出所が違う） | `workspaceMemberDirs` / `crateSources` | 済 |
| `G-build-commands` | workspace member | `workspaceMemberDirs` | 済 |
| `G-workspace-lints` | 同上 | `workspaceMemberDirs` | 済 |
| `G-clippy-disallowed` | clippy の禁止メソッド | — | 見送り（下の Ruling） |
| `G-hook-fires` | `selectChecks` の発火表の行 | — | 見送り（下の Ruling） |

`G-architecture-table` / `G-ci-table` / `G-hook-commands` も同じ理由で見送る。

## Ruling — 残る 5 本は Phase 2 では移行しない（2026-08-20）

計画は「自前 filter の 10 本」を移行対象としていたが、**実測が分類を訂正した**。残る 5 本は
同じクラスであり、母集団が走査（`snapshot.files`）ではなく**名指しされたファイルの中身**から出る
——4 本は `snapshot.files` を一度も読まず、`G-hook-fires` の使用も表の代表パスの実在照合であって
母集団の filter ではない。

**このクラスへ錨を足すと、検査自身の判定の言い直しになる。** clippy の禁止メソッドなら、縮み方の
全モード（ファイル消失・配列消失・行消失）を検査が既に fail-closed で赤にしており、ドメインは
新しい被覆を持たず同じ欠陥に finding を 2 件出すだけである。Phase 1 の全体レビューが 5 回捕まえた
「常に真になる錨」を、こちらの手で再生産することになる。

設計 §4 はこの 2 本を名指して移行可能と見込んでいた。**そこからの意図的な逸脱である**——設計は
歴史資料ゆえ直さず、生きた記録はこの台帳と `registry.test.mjs` の凍結リストが持つ。
**着手の条件は Phase 3 と同じ**: 完了判定を「腕ごとの絞り込みで発火を測る」形へ書き換えること。

**間違えたら何を失うか**: この 5 本の母集団が縮む沈黙が残る。ただし縮み方の主要モードは各検査の
fail-closed な canary が既に赤にしており、残余は「その canary 自身が消される」形に限られる。

## 作業項目

- [x] I3 — manifest へ `domains` 列（名前の集合）を足す
- [x] I4 — 未移行 id を凍結し、実際の集合との一致を検めるテストを置く
- [x] `.claude/rules/` のクラスタを移行する（`ruleDocs` 8 件 / `judgingScripts` 96 件・移送前後で集合同一）
- [x] skills のクラスタを移行する（`skillDocs` 12 件）
- [x] crate と `.rs` のクラスタを移行する（`workspaceMemberDirs` 4 / `crateSources` 95 / `moduleIndexSources` 95）
- [x] 走査から母集団を作る検査がすべて移行済みであることを、`snapshot.files` の使用箇所で検算する
- [x] 未移行が上記 5 本だけになったことを `governance:check` の evidence で確かめる（ドメイン未移行 5 本）

## 人間レビュー

- [x] 承認済み — 2026-08-19 / 問い: Phase 1 の PR 本文「Phase 2 へ送ったもの」の提示 / 回答: "Phase 2 にすすもう"
