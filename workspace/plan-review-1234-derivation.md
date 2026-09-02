# 対象 issue: #1234 — plan:ledger 撤去範囲の独立導出

導出者: derive-1234（`workspace/plan.md` / `workspace/research.md` は未読）。ブランチ `chore/retire-plan-ledger`（HEAD `6850855` = main）。
2026-09-02 実測。**削除の組み合わせ 3 通りを使い捨て worktree でフォールトインジェクションし、`governance:check` と `vitest` を実際に走らせた**（§3）。

## 1. 削除・編集の対象

| 対象 | 処置 | 根拠 |
|---|---|---|
| `scripts/plan-review-ledger.mjs`（314 行） | **削除** | issue 本文。呼び手は `package.json:14` のみ |
| `scripts/plan-review-ledger.test.mjs`（344 行） | **削除** | 同上。`vitest.config.ts` の `scripts/**/*.test.mjs` glob で拾われているだけ（固定の一覧は無い） |
| `package.json:14` `"plan:ledger": "node scripts/plan-review-ledger.mjs",` | **削除（1 行）** | 実行対象が消える |
| `docs/adr/ADR-plan-ledger-population-persistence.md` | **削除**（ユーザー裁定） | 被引用は `scripts/plan-review-ledger.mjs:20` の 1 件のみ（`git grep -n "ADR-plan-ledger"` 全拡張子）。script と同時に消せば被参照ゼロ |

**編集不要と確かめたもの**（名指しが無い）:

| 候補 | 結果 | コマンド |
|---|---|---|
| `vitest.config.ts` | glob のみ・ファイル名の固定無し | `cat vitest.config.ts` |
| `.claude/rules/*.md` の `paths` | `safety-nets.md` / `governance-docs.md` とも `scripts/**`（#837 で glob 化済み）。個別名の記載無し | `grep -n -A8 "^paths:" .claude/rules/*.md` |
| `.claude/settings.json` | hook 2 本のみ。script 名無し | `cat .claude/settings.json` |
| `.github/workflows/*` | `plan` / `ledger` の出現 0。`npm test` は glob 経由 | `grep -rn "scripts/\|npm run" .github/workflows/` |
| `scripts/governance/**`・`scripts/governance-manifest.mjs` | `plan-review-ledger` の名指し 0。`G-heading-refs.test.mjs:46` の `Step 2b` は fixture で無関係 | `git grep -n "ledger" scripts/governance` → 0 件 |
| `docs/hooks.md`・`docs/build-commands.md` | `plan:ledger` の出現 0（`npm test` の母集団は `vitest.config.ts` を指すだけ） | `grep -n "plan" docs/hooks.md docs/build-commands.md` |
| `.claude/hooks/post-edit.test.mjs` の package.json カナリア（838〜848 行） | 固定するのは `prepare` と `test` の 2 キーだけ | 実読 |
| `scripts/governance-check.test.mjs`・`governance-manifest.test.mjs` | ファイル数・特定ファイル名の固定無し | `grep -n "toHaveLength\|\.length).toBe"` |
| `.gitignore` | `workspace/` も `workspace/plan-review/` も未記載（`workspace/` は追跡外ディレクトリとして今在る）。触らない | `cat .gitignore` |

## 2. 語彙ヒットの振り分け

導出コマンド（`workspace/` を除外）:

```
git grep -n -i "ledger" -- ':!workspace/'
git grep -n "台帳" -- ':!workspace/'
git grep -n -E "plan-review/|ledger\.json|plan:ledger|母集団の永続化|#831|#826" -- ':!workspace/'
git grep -n "ADR-plan-ledger" -- ':!workspace/'
git grep -n "plan-ledger" -- '*.rs' '*.ts' '*.ps1' '*.yml' '*.json'   # → 0 件
```

| 綴り | 所在 | 振り分け | 理由 |
|---|---|---|---|
| `plan:ledger` | `package.json:14` | **削除** | 呼び手の行そのもの |
| `plan:ledger` / `plan-review-ledger.mjs` | `scripts/plan-review-ledger.mjs:1,20,32`・`.test.mjs` | **削除**（ファイルごと） | — |
| `plan:ledger` / `plan-review-ledger.mjs` | `docs/superpowers/specs/2026-07-29-plan-review-instrument-split-design.md:10,24` | 歴史資料ゆえ触らない | `docs/superpowers/` は #589 で非規範化・全 G 検査の母集団外 |
| `ADR-plan-ledger-population-persistence` | `scripts/plan-review-ledger.mjs:20` | **削除**（ファイルごと） | 唯一の被引用。script より先に ADR だけ消すと `G-adr-citations` が赤（§3 実測） |
| `workspace/plan-review/` | `docs/adr/ADR-plan-document-lens-not-permanent.md:18` | 歴史資料ゆえ触らない | #759/#762 当時のパスの記録。`plan-review-ledger` 以前（#754）の 4 レンズ成果物の話で、台帳 script の語彙ではない |
| `台帳` | `.claude/skills/start-issue/SKILL.md:110`「別の台帳・スナップショット・会話履歴を、未確定ゼロやレビュー完了の判定根拠にしない」 | **撤去後も真なので触らない** | 「plan.md 以外を根拠にしない」という一般則。台帳 script が消えてもむしろ強く成り立つ。⚠ ただし ADR 却下 2/3 の根拠として引かれていた見出し（§5） |
| `台帳` | `docs/adr/ADR-doc-promise-over-area-ratchet.md:10,27` | 別概念ゆえ触らない | `AREA_BUDGET` 引き上げの記録台帳 |
| `台帳` | `docs/superpowers/plans/*`・`specs/*`（opus5-skill-tuning・plan-review-loop・rescan・governance-domains phase2/3・norm-format-by-role） | 歴史資料ゆえ触らない／別概念 | plan-review-loop 系は当時の台帳機構の記述（歴史）。rescan 系は `index_stale` 台帳、governance-domains は進行台帳、norm-format は候補台帳＝別概念 |
| `ledger` | `docs/architecture.md:100`・`snotra-core/CLAUDE.md:15`・`src-tauri/CLAUDE.md:14,20`・`snotra-core/src/engine.rs:549`・`docs/design/2026-05-31-coherence-staleset.md`（8 件） | 別概念ゆえ触らない | `index_stale` ledger（#347/#348-A）。plan:ledger と無関係 |
| `ledger` | `docs/superpowers/plans/2026-07-24-*`・`07-25-*`・`08-10-explicit-scan-only.md`・`08-10-search-worker.md` | 別概念／歴史資料 | SU ledger（subagent-driven-development の進捗レジャー）・テスト名 `..._forge_the_ledger`（index_stale） |
| `.superpowers/`「進捗レジャー」 | `.gitignore` コメント | 別概念ゆえ触らない | superpowers プラグインの scratch |
| `#831` / `#826` | 生きた層の出現 0（削除 ADR と script のみ） | — | `git grep -n "#826\|#831"` で確認 |
| `母集団の永続化` | 出現 0 | — | issue が挙げた語だが逐語では在らず。概念としては ADR §却下 2 と `ledger.json` の説明が該当し、どちらも削除対象に閉じる |
| `Step 2 — 計画準拠の独立レビュー`「から撤去済み」 | `scripts/plan-review-ledger.mjs:1`（#849 が書き換えた行） | **削除**（ファイルごと） | この 1 行が「切り離したまま撤去条件無し」の実体。issue 本文の指摘どおり |

**幽霊識別子の走査**: 実体が別名で在る旧名は見つからなかった。`LEDGER_DIR` / `LEDGER_FILE` / `classifyEntries` / `readLedgerDir` / `writeLedger` は script と test 以外に出現 0（`git grep -n "LEDGER_\|classifyEntries\|readLedgerDir\|writeLedger" -- ':!scripts/plan-review-ledger*'` → 0 件）。

**生きた層から手順として委譲された凍結文書**: `docs/superpowers/specs/2026-07-29-plan-review-instrument-split-design.md:24` は「台帳の母集団固定・双方向照合・exit 3 値 → `npm run plan:ledger`」と書くが、これを手順として指す生きた層は無い（`.claude/**` から同 spec への参照: `git grep -n "instrument-split" .claude AGENTS.md CLAUDE.md docs/*.md` → 0 件）。触らない。

## 3. 赤になりうる検査と削除の順序（フォールトインジェクション実測）

使い捨て worktree（`git worktree add --detach`・`node_modules` は junction）で 3 通りを測った。ベースライン: `governance:check` 緑（検査 24 件・ADR 82 本・短縮引用 469 件）。

| 組み合わせ | `governance:check` | `vitest run` |
|---|---|---|
| (a) ADR だけ削除（script 残置） | **赤 1 件**: `scripts/plan-review-ledger.mjs:20  ADR の短縮引用が実在しない: ADR-plan-ledger-population-persistence` | 未測定（不要） |
| (b) script + test だけ削除（`package.json` の行と ADR は残置） | **緑**（見出し参照 375→370、コメント走査 118→116 と減るだけ） | 未測定 |
| (c) 4 点すべて削除 | **緑**（ADR 81 本・短縮引用 464 件） | **緑** 38 files / 886 tests |

帰結:

- **ADR と script は同一コミットに束ねる**（(a) の順序では `G-adr-citations` が赤。逆順＝script 先なら赤にならない）。1 コミットで 4 点同時が最も単純
- ⚠ **`package.json` の `plan:ledger` 行だけ残しても、どの検査も鳴らない**（(b) 実測）。`G-build-commands` / `G-ci-table` は `docs/build-commands.md` に書かれた `npm run X` の実在を見るだけで、`scripts[X]` の値が指すファイルの実在は誰も見ない。`post-edit.test.mjs` のカナリアも `prepare` と `test` しか固定しない。**行の削り忘れは沈黙する**——これは撤去チェックリストで人が確認する項目
- `vitest.config.ts` の `include` glob は削除に追随する。ファイル数を固定するテストは無い（§1）
- `G-rules-script-coverage`（実ファイル → rules glob）は母集団が縮む方向では鳴らない（ヘッダ「実ツリーが全件被覆である限り判定結果を変えない」）。撤去で赤にならないのは設計どおり
- `G-references` は `scripts/` を走査元に持たない（`governanceDocs` は md・rules・SKILL.md のみ）ので、script が消えても指し手側の赤は出ない。`G-heading-refs` はスクリプトのコメントも走査元だが、消えるのは走査元であって参照先ではない
- **`package.json` を編集すると PostToolUse が `hook-selftest` を走らせる**（`CHECK_DEFINITION` に載っている・`post-edit.mjs:77-82`）。沈黙＝合格として読んでよいファイル

## 4. 撤去の作法が要求する手順

`AGENTS.md`「条件別チェック」の「機構・層・ファイル群を撤去する」行、`docs/development-principles.md`「撤去（消す変更）の作法」から:

1. **削除したファイル名と持ち込んだ語彙を `git grep` で数え上げ、残った出現を「撤去を描写している / 撤去されたものが在る前提で書いている」へ振り分ける**（生きた層すべて・`scripts/` を含む）→ §2 の表。「在る前提」に該当する生きた層の出現は **0 件**（`package.json:14` を除く）
2. **「撤去した識別子の残存 0 件」を根拠にしない**——散文の語彙（`台帳`）・幽霊識別子・委譲された凍結文書まで見る → §2 末尾の 3 段落
3. **消す差分の中で新しく書いた 1 文は測る対象**——本撤去で残す記述に手を入れる箇所は無い（§1・§5 で編集不要）。⚠ PR 本文・commit message に「`plan:ledger` は X が引き継ぐ」等の主張を書くなら、その X の母集団が元の観点（配送の照合）を覆っているかを読んでから書く（同節「これは X が持つから消す」）。今回は引き継ぎ先が無い（#849 でリスク連動化がその観点自体を廃した）ので、「引き継がない」と書くのが真
4. **ADR を削除するときは、生きた層に残る引用を先に散文化してから消す**（`ADR-adr-frozen-history`「受容する残余」）→ 生きた層の引用は script のみで同時削除。散文化すべき生きた引用は無い
5. **`grep -r` ではなく `git grep`** を使う（`.claude/worktrees/` の追跡外を混ぜない）。本導出は全て `git grep`
6. 一時的な足場の撤去時は **`PERFORMANCE.md` への着地**を確かめる（同行の「撤去時」）→ 台帳 script は計装ではなく測定値を出していない。該当無し（`git grep -n "ledger" PERFORMANCE.md` → 0 件）

## 5. 削除 ADR の否定の知識の行き先

`ADR-plan-ledger-population-persistence` の内容を節ごとに分け、生きた層に既に在るかを `git grep` 全拡張子で当てた。

| 節 | 内容 | 生きた層での所在 | 判定 |
|---|---|---|---|
| 却下 1 | 契約「呼び出し側が渡した内容を書かない」は識別子ではなくバイト列を守る。**偽造しても緑にならないものは契約が守る対象ではない** | script ヘッダ 12〜20 行（削除対象）。`/plan-review` SKILL.md 本文からは **#849 が「オーケストレーターは Write を持たない（自作自演の防止）」の段落ごと削除済み**（`git show 4998405` の `-` 行 86〜87） | **失われる**。ただし守る対象（`ledger.json` の母集団）自体が消えるので、この否定の知識は対象を失う |
| 却下 1 の根拠 | `docs/check-skill-skeleton-design.md`「必須 1 — 母集団」（列挙アンカーを母集団に採らない） | `docs/check-skill-skeleton-design.md:22` に現存 | 吸収済み（正本が生きている） |
| 却下 2 / 3 | 台帳を `workspace/plan-review/` の外へ置かない・mtime ガードを置かない。根拠は「収束判定に要る状態はすべて `plan.md` に置く」 | `.claude/skills/start-issue/SKILL.md:108-110`（見出しと本文）に現存 | 吸収済み。台帳が無くなれば却下案 2/3 は立てようが無い |
| 却下 4 | SKILL.md のフェンス行を `parseArgs` に食わせるテストは実データで破れた | 無し | 失われる。対象（フェンス行）は #849 で既に消えている |
| 却下 5 | 意味を持たなくなった入力は黙って無視せず拒否して復帰手順を出す。根拠は「これまで無意味だった状態に意味を与える変更は全経路を列挙」 | 根拠は `.claude/rules/safety-nets.md:52` に現存。「拒否して復帰手順」の一般則はルート `CLAUDE.md`「フック」の「拒否メッセージが復帰手順を持つ」に同型 | 吸収済み（一般則として） |
| 帰結: exit 3 値 | `0/1/2` の行き先 | script と #849 前の SKILL.md（削除済み） | 失われる。対象が消える |
| 帰結: `allowed-tools` に `Write` が無いことの 2 つ目の職務 | 母集団の偽造防止 | `.claude/skills/plan-review/SKILL.md:5-10` は今も `Write` を持たない。**理由の記述は生きた層に無い**——`ADR-norm-review-seeding.md:51` が「`/plan-review` が `Write` を意図的に外した理由（自作自演の防止）」として引くのみ（ADR = 凍結） | ⚠ **1 つ目の職務（成果物の偽造防止）の説明が、本 ADR 削除後は生きた層のどこにも無くなる**。これは #849 が既に作った状態であり本撤去が作るものではないが、本 ADR が最後の説明先だった。plan-review SKILL.md へ 1 行足すかは**ユーザー判断**（射程外） |
| 帰結: `safety-nets.md` の `paths` が script を含まない残余 → 2026-08-02 追記で #837 が解消 | `scripts/**` glob | `.claude/rules/safety-nets.md:10`・`governance-docs.md:6` に現存 | 吸収済み。さらに `G-rules-script-coverage` が機械化 |

**引用の実在**: `git grep -n "ADR-plan-ledger" -- ':!workspace/'` → `scripts/plan-review-ledger.mjs:20` のみ。`.rs` / `.ts` / `.ps1` / `.yml` / `.json` は 0 件。削除で宙に浮く引用は無い。

## 6. 検証コマンド（`docs/build-commands.md` のカテゴリ）

| カテゴリ | コマンド | 理由 |
|---|---|---|
| B（node / セーフティネット） | `npm test` | `scripts/**/*.test.mjs` の母集団が 1 本減る。`package.json` 編集で PostToolUse の `hook-selftest` も自動発火 |
| F（ガバナンス） | `npm run governance:check` | ADR 削除・script 削除（`G-adr-citations` / `G-adr-file-names` / `G-heading-refs` のコメント走査） |
| — | `git grep -n -i "plan:ledger\|plan-review-ledger\|ADR-plan-ledger" -- ':!workspace/'` → **0 件**が終端条件 | 撤去の作法の最終 grep。`台帳` / `ledger` は別概念が残るので 0 件を条件にしない（§2） |
| — | `npm run plan:ledger` が `Missing script` で落ちること | 行の削り忘れは検査が沈黙する（§3 (b)）。手で 1 度打つ |

不要: カテゴリ A（Rust）・C/D（smoke）・E（PowerShell）。`.rs` / `.ps1` に触れない。`skip-ci` は貼れない（`scripts/**` を含むため・`docs/build-commands.md:287`）。

## 7. ⚠ 確信の持てない項目（再掲）

- ⚠ `package.json` の行削り忘れを鳴らす層が無い（§3 (b) 実測）。チェックリストに「`npm run plan:ledger` が落ちる」を置く
- ⚠ `/plan-review` の `allowed-tools` から `Write` を外した理由が、本 ADR 削除で生きた層から完全に消える（§5）。#849 由来の既存の穴だが、本 PR が最後の説明先を消す。足すか否かはユーザー裁定
- ⚠ `.claude/skills/start-issue/SKILL.md:110` の「別の台帳」は撤去後も真だが、「台帳」という語が指す実体は今後 `plan:ledger` ではなくなる。書き換え不要と判定したが、独立導出 2 体目が違う結論を出すなら争点はここ

## 導出に使ったコマンド（全件）

```
gh issue view 1234
git grep -n -i "ledger" -- ':!workspace/'
git grep -c -i "ledger" -- ':!workspace/'
git grep -n "台帳" -- ':!workspace/'
git grep -n -E "plan-review/|ledger\.json|plan:ledger|母集団の永続化|#831|#827|#834" -- ':!workspace/' ':!scripts/plan-review-ledger*' ':!docs/superpowers/'
git grep -n "ADR-plan-ledger" -- ':!workspace/'
git grep -n "plan-ledger" -- '*.rs' '*.ts' '*.ps1' '*.yml' '*.json'
git grep -n "#826\|#831" -- ':!workspace/' ':!docs/superpowers/' ':!scripts/plan-review-ledger*'
git grep -n "自作自演\|Write を持たない\|偽造" -- '.claude/' 'docs/*.md' 'docs/adr/'
git show --stat 4998405 ; git show 4998405 -- scripts/plan-review-ledger.mjs ; git show 4998405 | grep "^-" | grep "自作自演\|plan:ledger\|台帳"
git log --oneline -S "plan:ledger" -- package.json        # e7c6c66 (#827) が導入
grep -n -A8 "^paths:" .claude/rules/*.md
grep -rn "scripts/\|npm run\|npm test" .github/workflows/
grep -n "package.json" .claude/hooks/post-edit.test.mjs ; sed -n '838,848p' 同
grep -rln "package.json" scripts/governance/ scripts/governance-manifest.mjs
node scripts/governance-check.mjs                            # ベースライン緑
# worktree でのフォールトインジェクション（§3）
git worktree add --detach <scratch>/wt1234 HEAD ; mklink /J node_modules
(a) git rm docs/adr/ADR-plan-ledger-population-persistence.md ; node scripts/governance-check.mjs → exit 1
(b) git rm scripts/plan-review-ledger.mjs scripts/plan-review-ledger.test.mjs ; node scripts/governance-check.mjs → exit 0
(c) + sed -i '/"plan:ledger"/d' package.json ; node scripts/governance-check.mjs → exit 0 ; node node_modules/vitest/vitest.mjs run → 38 files / 886 tests passed
git worktree remove --force <scratch>/wt1234 ; git worktree prune
```
