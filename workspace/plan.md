# plan — issue #1234: 孤児化した plan:ledger を撤去する

ブランチ: `chore/retire-plan-ledger` ／ 調査: `workspace/research.md` ／ 敵対的調査: `workspace/adversarial-1234.txt`

## 目的

#849 で `/plan-review` から切り離されたまま撤去条件を持たなかった台帳 script（`npm run plan:ledger`）と、その契約だけを扱う ADR を撤去する。撤去の作法（`AGENTS.md`「条件別チェック（トリガー → 参照先）」の「機構・層・ファイル群を撤去する」）に従い、削除ファイル名と語彙の数え上げを根拠に残す。**ユーザー裁定（2026-09-02）: `ADR-plan-ledger-population-persistence` は同じ PR で削除する。**

## 受け入れ条件

1. 次の 4 ファイルが削除され、`package.json` から `plan:ledger` の行が消えている
   - `scripts/plan-review-ledger.mjs`
   - `scripts/plan-review-ledger.test.mjs`
   - `docs/adr/ADR-plan-ledger-population-persistence.md`
   - （`package.json` は編集）
2. 削除ファイル名（`plan-review-ledger`・`plan:ledger`・`ADR-plan-ledger-population-persistence`）の生きた層での残存が 0 件。歴史資料（`docs/superpowers/specs/2026-07-29-plan-review-instrument-split-design.md`）の出現は凍結ゆえ残す
3. 語彙（`台帳`・`ledger`）の生きた層での残存が、撤去された台帳が在る前提で書かれたものを含まない（`research.md` F2 の振り分けを撤去後に再実行して一致）
4. `npm test` 緑・`npm run governance:check` 全検査 passed・`git diff --check` 空
5. `.claude/rules/safety-nets.md` の `paths` は変更しない（`scripts/**` の glob が残るファイルを覆う）
6. PR 本文に `Closes #1234`。マージで閉じる issue は #1234 のみ

## 変更ファイル一覧と対象シンボル

| ファイル | 変更 | 備考 |
|---|---|---|
| `scripts/plan-review-ledger.mjs` | 削除 | 呼び手なし（`research.md` F1） |
| `scripts/plan-review-ledger.test.mjs` | 削除 | vitest の glob で拾われていた。個別列挙なし |
| `docs/adr/ADR-plan-ledger-population-persistence.md` | 削除 | 引くのは削除する script だけ。**script と同じコミットで消す**（片方だけ残すと G-adr-citations が赤） |
| `package.json` | `"plan:ledger"` の 1 行を削除 | `package-lock.json` は触らない（npm scripts は lock に載らない） |
| `workspace/*` | 本サイクルの成果物 | `/retrospective` で撤去 |

触らないもの: `.claude/skills/start-issue/SKILL.md:110`（「別の台帳」は汎用名詞）・`index_stale` ledger を語る `docs/architecture.md` / `snotra-core/CLAUDE.md` / `src-tauri/CLAUDE.md` / `engine.rs`（別概念）・`docs/superpowers/**`（歴史資料）・`.claude/rules/safety-nets.md`・`RETROSPECTIVE.md`・`SPEC.md`。

## 実装順序

### Phase 1 — 撤去（1 コミット）

- [ ] `git rm scripts/plan-review-ledger.mjs scripts/plan-review-ledger.test.mjs docs/adr/ADR-plan-ledger-population-persistence.md`
- [ ] `package.json` から `"plan:ledger": "node scripts/plan-review-ledger.mjs",` の行を削除（前後の行の JSON カンマを壊さない——`node -e "require('./package.json')"` で parse を確かめる）

### Phase 2 — 撤去の作法（数え上げ）

- [ ] 削除ファイル名の残存: `git grep -n -i "plan-review-ledger\|plan:ledger\|ADR-plan-ledger-population-persistence" -- . ':!docs/superpowers'` が 0 件（歴史資料の出現は別に数えて「撤去を描写している／当時の事実」と記録）
- [ ] 語彙の残存: `git grep -n "台帳" -- AGENTS.md CLAUDE.md .claude docs scripts ':!docs/adr' ':!docs/superpowers'` と `git grep -n -i "ledger" -- . ':!docs/adr' ':!docs/superpowers' ':!package-lock.json'` を打ち、各ヒットを「撤去を描写 / 別概念（`index_stale`）/ 汎用名詞 / 在る前提」へ振り分けて `workspace/plan.md` 末尾へ表で残す。「在る前提」が 1 件でもあれば直す

### Phase 3 — 検証

- [ ] `npm test` 緑（ベースライン 39 ファイル / 923 テストから、テストファイルが 1 本減ることを summary 行で確認）
- [ ] `npm run governance:check` 全検査 passed（ADR 82 → 81 本）
- [ ] `git diff --check` 空
- [ ] `npm run plan:ledger` が `Missing script` で落ちる（exit 非 0）——`package.json` の行の削り忘れはどの検査も鳴らない（独立導出が使い捨て worktree で実測）ため、手で 1 度打つ
- [ ] `node -e "require('./package.json')"` が通る（JSON の壊れ）

## 不変条件と異常系

- **script と ADR は同じコミットで消す**——script の冒頭コメントが ADR を短縮引用しており、順序を分けると G-adr-citations が赤になる
- `.claude/rules/safety-nets.md` の `paths` が `scripts/**` を含むため本作業は自動配送を受ける。撤去は「足す」ではないので、フォールトインジェクションの新設は要らない。確かめるのは「消しても他の検査が緑のまま推移するか」（Phase 3）
- `vitest.config.ts` の include（3 glob）は触らない。`post-edit.test.mjs` のカナリアはスコープを守るものでファイル数は数えない
- `G-rules-script-coverage` の母集団は動的列挙。ファイル減で赤にならない
- `package.json` の編集は PostToolUse の hook-selftest を走らせる（`selectChecks` に割り当てあり）——この編集に限り沈黙は合格。`scripts/*.mjs` の削除と ADR の削除には検査が走らない
- 撤去後の `npm test` は 38 ファイル / 886 テストになる見込み（独立導出の使い捨て worktree での実測。ベースライン 39 / 923）

## テスト方針と検証コマンド

- 新規テストなし——削除のみ。検知は Phase 2 の数え上げと Phase 3 の既存検査
- `npm test` ／ `npm run governance:check` ／ `git diff --check` ／ Phase 2 の grep 2 本

## SPEC.md・関連文書の更新要否

- `SPEC.md`: 不要（プロダクト挙動に触れない）
- `docs/build-commands.md` / `docs/hooks.md`: 不要（`plan:ledger` の言及 0 件・`research.md` F5）
- `.claude/skills/**`: 不要（参照 0 件）
- `RETROSPECTIVE.md`: 本 PR では触らない

## 未確定（実装前に潰す）

- [x] **敵対的調査（`workspace/adversarial-1234.txt`）の所見を反映した** — 壊せた 0 / 壊せなかった 15 / ⚠ 2（採否は `research.md`「敵対的調査（3b）の所見と採否」・どちらも採らない）。争点 4 の結果: ファイル数・ファイル名を固定する検査やテストは無く、G-build-commands は文書 → `package.json` の一方向照合。Phase 3 の期待値は変わらない。ベースライン（変更前）: `npm test` 39 ファイル / 923 テスト、`governance:check` 24 検査 passed・ADR 82 本

- [x] **U2. `/plan-review` の `allowed-tools` から `Write` を外した理由の行き先** — 裁定（2026-09-02・ユーザー）: **足さない**。スキルは触らない。以下は裁定前の検討: 削除する ADR の「受容する残余」が「成果物の偽造に加えて母集団の偽造も防ぐ」と書いており、#849 が SKILL.md の当該段落を消したため、削除後は凍結 ADR `ADR-norm-review-seeding` の引用にしか残らない。**推奨: 足さない**——2 つ目の職務（母集団の偽造防止）は台帳とともに消え、1 つ目（成果物の偽造防止）は `allowed-tools` の実体と Step 2/2b の「呼び出し側が指定した絶対パスへ書き出す」形が守っている。理由の 1 文を SKILL.md へ足すのはスキル変更ゆえ、人間レビューで裁定

## plan-review 結果

- リスク: 高（`scripts/` の撤去＝セーフティネット、ADR の削除＝ガバナンス文書、網羅性が要件）
- レビュー方式: 独立導出1体（`--deep`・`workspace/plan-review-1234-derivation.md`・計画と調査を読ませず導出。使い捨て worktree でフォールトインジェクションを実測）
- エージェント数: 1

### 要対処
- `package.json` の行の削り忘れを鳴らす層が無い — 計画の修正（Phase 3 へ `npm run plan:ledger` が落ちることの確認を追加） — 再照合: `G-build-commands` は `docs/build-commands.md` → `package.json` の一方向照合（`scripts/governance/checks/G-build-commands.mjs` の `checkBuildCommands`）。文書側に無い script の削除は見ない

### 軽微
- `package.json` の編集で PostToolUse の hook-selftest が走る（不変条件へ注記済み）
- 「別の台帳」（`start-issue` SKILL.md）は撤去後も真。語が指す実体は変わるが書き換え不要（導出・計画とも一致）

### 未検証
- なし

### 判断
- 実装着手: 可（U2 は 2026-09-02 に人間が「足さない」と裁定）

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー1体（独立導出・`--deep`）
- エージェント数: 1（plan-review）+ 1（`/start-issue` 3b 敵対的調査・sonnet）
- 要対処: 1 件（反映済み）
- 未検証: なし

## 人間レビュー

- [x] 承認済み — 2026-09-02 / 問い: "以上を反映した workspace/plan.md（script・テスト・ADR の 3 ファイル削除 + package.json の 1 行削除・語彙の数え上げ・PR で #1234 を Closes）を承認しますか？" / 回答: "承認する"（同時に U2 "足さない (Recommended)" を裁定）
