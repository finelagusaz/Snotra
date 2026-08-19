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
| `G-clippy-disallowed` | clippy の禁止メソッド（ファイルでない母集団） | — | 未 |
| `G-hook-fires` | `selectChecks` の発火表の行 | — | 未 |

固定パスの 3 本（`G-architecture-table` / `G-ci-table` / `G-hook-commands`）は Phase 3 であり、**完了判定を書き換えるまで着手しない**。

## 作業項目

- [x] I3 — manifest へ `domains` 列（名前の集合）を足す
- [x] I4 — 未移行 id を凍結し、実際の集合との一致を検めるテストを置く
- [x] `.claude/rules/` のクラスタを移行する（`ruleDocs` 8 件 / `judgingScripts` 96 件・移送前後で集合同一）
- [ ] 残りのクラスタを移行する（集合の比較が決める。着手のたびに上の表と本項目を更新する）
- [ ] 未移行が固定パスの 3 本だけになったことを `governance:check` の evidence で確かめる

## 人間レビュー

- [x] 承認済み — 2026-08-19 / 問い: Phase 1 の PR 本文「Phase 2 へ送ったもの」の提示 / 回答: "Phase 2 にすすもう"
