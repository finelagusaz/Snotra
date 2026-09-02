# research — issue #1234: 孤児化した plan:ledger を撤去する

調査日: 2026-09-02 ／ ブランチ: `chore/retire-plan-ledger`

## issue の要約

`scripts/plan-review-ledger.mjs`（`npm run plan:ledger`）は #849（2026-07-30・`4998405`）で `/plan-review` から切り離され、「ロールバックを容易にするため削除せず」と PR 本文が記したまま撤去条件を持たずに残った。リスク連動化は 1 か月運用され戻す議論は無いので、撤去の作法（`AGENTS.md`「条件別チェック」の「機構・層・ファイル群を撤去する」）を踏んで消す。

**ユーザー裁定（2026-09-02）**: 被参照ゼロになる `ADR-plan-ledger-population-persistence` は**同じ PR で削除する**。

## 事実（一次資料つき）

### F1. 撤去対象（4 ファイル + 1 行）

| 対象 | 根拠 |
|---|---|
| `scripts/plan-review-ledger.mjs`（314 行） | `git grep` で `.claude/skills/**` からの呼び出し 0 件。自身の冒頭コメントが「`/plan-review`「Step 2 — 計画準拠の独立レビュー」から撤去済み」と書いている |
| `scripts/plan-review-ledger.test.mjs`（344 行） | `vitest.config.ts` の `scripts/**/*.test.mjs` glob で母集団に入る。個別列挙は無い |
| `package.json` の `"plan:ledger": "node scripts/plan-review-ledger.mjs"` | 唯一の生きた層からの参照 |
| `docs/adr/ADR-plan-ledger-population-persistence.md` | 引くのは `scripts/plan-review-ledger.mjs:20` だけ（`git grep -n "ADR-plan-ledger-population-persistence"`）。script を消せば被参照ゼロ。ユーザー裁定で削除 |

### F2. 語彙の数え上げ（生きた層・`docs/adr` と `docs/superpowers` を除く）

- `plan-review-ledger` / `plan:ledger`: `package.json` と script 自身以外に 0 件
- `台帳`: script とテスト以外では `.claude/skills/start-issue/SKILL.md:110`「別の台帳・スナップショット・会話履歴を、未確定ゼロやレビュー完了の判定根拠にしない」の 1 件。**汎用名詞であり、台帳機構が在る前提では書かれていない**（「別の」と言っているのは `plan.md` 以外の一切）。触らない
- `ledger`: `docs/architecture.md:100`・`docs/design/2026-05-31-coherence-staleset.md`・`snotra-core/CLAUDE.md:15`・`snotra-core/src/engine.rs:549`・`src-tauri/CLAUDE.md:14,20`——すべて `index_stale` ledger（config↔index コヒーレンシ）で**別概念**。触らない
- `RETROSPECTIVE.md`: 0 件

### F3. 歴史資料に残る出現（凍結ゆえ触らない）

- `docs/superpowers/specs/2026-07-29-plan-review-instrument-split-design.md:10,24`——`npm run plan:ledger` を「既に入っているもの」として引く。歴史資料（#589 で非規範化）。G-references の走査元外（`scripts/governance/lib.mjs` の `governanceDocs` が除外）
- `docs/adr/*`（削除する 1 本以外）: `ADR-plan-ledger-population-persistence` を引く ADR は無い

### F4. 機構が撤去をどう見るか

- **G-rules-script-coverage**: `scripts/**` の判定スクリプトが `.claude/rules/` の `paths` に覆われているかを見る。母集団は `snapshot.files` の動的列挙（`SCRIPT_EXT`）なので、ファイルが減っても赤にならない（件数の固定は無い——`scripts/governance/checks/G-rules-script-coverage.mjs:65`）
- **G-heading-refs**: script の冒頭コメントが `/plan-review`「Step 2 — 計画準拠の独立レビュー」「Step 3 — 結果の統合と報告」を正準形で引いており、スクリプトのコメントも走査元である（#925/#1138）。ファイルごと消えれば参照も消える——赤にならない
- **G-adr-citations**: script 内の `docs/adr/ADR-plan-ledger-population-persistence.md` の引用と ADR 本文が同時に消える。**片方だけ残すと赤になる**（script を残して ADR だけ消す順は無い）
- **G-adr-file-names**: ADR の減少は見ない
- **`.claude/rules/safety-nets.md` の `paths`**: `scripts/**` を含むので、この編集で自動配送される。撤去は「セーフティネットの変更」に当たる——`safety-nets.md` の手順（フォールトインジェクション）は**足す**ときの規範であり、消すときに測るのは「消しても他の検査が緑のまま推移するか」（`npm test` / `governance:check`）
- **`npm test`**: `scripts/**/*.test.mjs` glob。テストファイルが減るだけ。`post-edit.test.mjs` のカナリアは include の**スコープ**（3 パターン）を守るもので、ファイル数は数えない（`vitest.config.ts` のコメント）

### F5. `docs/build-commands.md` / `docs/hooks.md`

- `plan:ledger` の言及 0 件（`git grep`）。コマンド索引の更新は不要

## 関連ファイル

- 削除: F1 の 4 ファイル。編集: `package.json`（1 行削除）
- 読むだけ: `vitest.config.ts`・`scripts/governance/checks/G-rules-script-coverage.mjs`・`scripts/governance/checks/G-adr-citations.mjs`・`.claude/rules/safety-nets.md`

## 再利用できる既存パターン

- #895（被参照ゼロ ADR 6 本削除）: 全拡張子 grep で自分以外のヒット無しを検算してから消す
- #1155 の撤去の作法: 削除ファイル名と語彙を `git grep` で数え、「撤去を描写 / 在る前提」へ振り分ける（本調査 F2・F3 がそれ）

## 技術的制約

- `package-lock.json` は `scripts` を持たない（npm scripts は lock に載らない）。触らない
- `.claude/rules/safety-nets.md` の `paths` は変更不要（`scripts/**` の glob は残るファイルを引き続き覆う）

## 未解決の疑問

- **U1（解消）.** `AGENTS.md` / ルート `CLAUDE.md` / `docs/hooks.md` / `docs/build-commands.md` / `docs/development-principles.md` / `.claude/{skills,rules,agents}` を `双方向照合|配送欄|配送の照合|台帳` で grep——ヒットは F2 の `start-issue` SKILL.md:110 の汎用名詞 1 件のみ。間接表現の残存なし
- なし（他に未解決の疑問は無い）

## 敵対的調査（3b）の所見と採否

`workspace/adversarial-1234.txt`（sonnet 1 体）。壊せた 0 / 壊せなかった 15 / ⚠ 2。ベースライン（変更前・HEAD `6850855`）: `npm test` 39 ファイル / 923 テスト全通過、`governance:check` 全 24 検査 passed。

| ⚠ | 所見 | 採否 | 理由 |
|---|---|---|---|
| 1 | 削除する ADR 本文の「採った形はコードと SKILL.md が持つ」の含意を F1 が扱っていない | 採らない | ADR ごと消えるので、その一文が指す先を追う必要が無い |
| 2 | 同 ADR:45 が `safety-nets.md` の `paths` glob 化（#837）に言及 | 採らない | 歴史的事実の記録で現行 `scripts/**` と矛盾せず、ADR ごと消える |

補強された点: G-adr-citations の `nonDocSources` が `.mjs`（`.test.mjs` 除く）を含むため「script と ADR は同じコミットで消す」は実装で裏取りされた。G-build-commands / G-ci-table は文書 → `package.json` の一方向照合で、script 削減は検知しない。
