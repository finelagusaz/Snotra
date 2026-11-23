# 計画: issue #1527 — race:boundaries の ④ が read_config 経由の config の読みを捕まえる

調査は `workspace/research.md`、敵対的調査は `workspace/adversarial-1527.txt`。

## 目的と受け入れ条件

`/race-check` の母集団（`npm run race:boundaries`）の ④「managed state の読み書き」に、`read_config(` を手がかりとして足す。src-tauri の config の読みは `AppState::read_config` / `egui_shell::read_config` を経由し、呼び出し行に `.read()` が現れないため、現状は ④ に立たない。

- AC1: `AppState::read_config` を呼ぶ行（`state.read_config(|cfg| ...)`）と `egui_shell::read_config` を呼ぶ行（`crate::egui_shell::read_config(` / `super::read_config(`）が、差分を `parseDiff` から通したとき、パターン `read_config\(` のラベルで ④ に立つ（行とパターンの組で assert）
- AC2: 既存の ④ のテスト（RwLock 系の組・I/O の read/write の不検出）が変わらず通る
- AC3: パターンを外す変異で AC1 のテストが赤になる（assert で落ちることを出力で確かめる）

## 変更するファイル

- `scripts/race-boundaries.mjs` — `KINDS` の ④ の `patterns` の**末尾**に `"read_config\\("` を足し（既存のラベルを変えない）、その上のコメントに理由を 1〜2 行足す（`AppState.config` は private なので、src-tauri の外から config を読む行には `.read()` が出ず `read_config(` が出る・#1527）。**「config の読みはすべて read_config を通る」と全称で書かない**——`Engine::config_handle().read()` のような迂回の読みが反例になる。契約は `src-tauri/src/state.rs` の `AppState::read_config` の doc を指す
- `scripts/race-boundaries.test.mjs` — `describe("④ の RwLock 系 …")` の後に、`read_config` 経由の読みの describe を 1 つ足す。fixture は現行ソースから写した 2 形（`src-tauri/src/commands/icon.rs` の `state.read_config(|cfg| ...)`、`src-tauri/src/egui_shell/window_coordinator.rs` の `let hex = super::read_config(`）と、`crate::egui_shell::read_config(` の形（`search_flow.rs`）

## 実装の順序

1. テストを先に足し、赤（④ に立たない）を確かめる
2. `KINDS` ④ にパターンを足し、緑を確かめる
3. 変異（パターンを外す）で赤になることを確かめ、元へ戻す（`/implement`「3b. 委譲へ渡すもの」の委譲先が複製で行う）

## 不変条件と異常系

- 序数 ①〜⑧ と種別名は変えない（`docs/check-skill-skeleton-design.md` が序数で参照する）
- 既存パターンの順序を変えない（ラベルは `findIndex` の最初の一致。既存テストが組で固定している）
- 出力は常に全種（`formatReport`）——パターンの追加で形は変わらない

## テスト方針と検証コマンド

- `npx vitest run scripts/race-boundaries.test.mjs`（赤 → 緑）
- `npm test`
- `npm run governance:check`（scripts/ はセーフティネット）
- 実リポジトリで `npm run race:boundaries -- --base origin/main` を走らせ、本 PR の差分で落ちないこと（任意の確認）

## SPEC.md・関連文書の更新

- 不要。種別とパターンの SSOT は `scripts/race-boundaries.mjs`（`.claude/skills/race-check/SKILL.md` は SSOT を指すだけで、パターンを写していない。敵対的調査で文書側の写しが無いことを確認済み）

## 作業項目

### フェーズ 1 — テスト（Red）
- [ ] `scripts/race-boundaries.test.mjs` に read_config 経由の読みの describe を足す（3 形・行とパターンの組で assert）
- [ ] 赤になることを確かめる

### フェーズ 2 — 実装（Green）
- [ ] `KINDS` ④ に `read_config\(` を足し、コメントに理由を書く
- [ ] `npm test` と `npm run governance:check` が通る
- [ ] 変異（パターンを外す）でテストが assert で赤になることを確かめる

## 残すもの（PR 本文へ）

- 書き込み側: `.lock(` と `update_config` が別の行に割れる形は ④ に立たない（現行の src-tauri の 2 行は同じ行なので立つ）。issue の範囲は読みなので足さない
- テストの文字列リテラル（`activation/tests.rs` の `"read_config("`）はその行を触る差分で偽陽性になる（入口の粗さとして受容）

## 未確定（実装前に潰す）

- [x] fixture に使う行が現行ソースに実在するか — `git grep` で確認済み（icon.rs:18・window_coordinator.rs:247・search_flow.rs:46）
- [x] パターンの綴りで全呼び出しが立つか — 敵対的調査がスクリプトの関数を import して全ソース行に通し、25 行すべてが立つことを確認

## 承認

- [x] 自己承認 — 2026-11-23 / 仕様変更: なし — `SPEC.md` を変更ファイルに含まない / 戻せない変更: なし — 永続形式・識別子/キー形式に触れない（node スクリプトの検出パターンとテストだけ） / 人の裁定: なし

（`scripts/**` はセーフティネットなので、マージはユーザーと合意してから。PR 本文の冒頭に「マージは合意してから」と書く。）

## plan-review 結果

- リスク: 高（`scripts/**` はセーフティネット）
- レビュー方式: 独立導出1体（`workspace/plan-review-1527-derive.md`。委譲先は計画・調査を読んでいないと申告）
- エージェント数: 1

### 要対処
- なし。導出 ∖ plan・plan ∖ 導出ともに空（変更ファイル 2・パターン `read_config\(`・差分 fixture を行とパターンの組で assert する形が一致）

### 軽微
- パターンを配列の末尾に置く — 計画へ反映（既存のラベルを変えない）
- コメントで全称（「すべての読みが read_config を通る」）を書かない — 計画へ反映
- 書き込み側（`update_config` の行が `.lock(` と別の行になる形）は漏れる — 残すもの（PR 本文）に既に記載。issue の範囲は読み
- ⚠️ `\b` を付けるか — 付けない。別名 0 件で結果は同じ。付けると `isCommentLine` 等と無関係に綴りが重くなるだけ
- ⚠️ 定義行が当たらないことをテストで assert するか — しない。定義行は読みの境界ではなく、当たっても害が無い（固定する価値が無い）
- ⚠️ turbofish（`read_config::<T>(`）は漏れる — 現行 0 件。入口の粗さとして受容

### 未検証
- なし

### 判断
- 実装着手: 可

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー1体
- エージェント数: 1
- 要対処: 0 件（軽微 2 件を計画へ反映）
- 未検証: なし
- check スキル: なし（`/race-check` は計画段階では起動しない。永続形式・UI モード・対称ペア・関数の新設に当たらない）
- 散文の全件列挙: 語 3 / 総件数 76 行 / ★未掲載 18（計画へ足した 0） / ☆行未掲載 8（計画へ足した 0）——★☆ はすべて `read_config` の利用箇所か、パターンの SSOT をスクリプトへ指すだけの記述（`docs/build-commands.md`・`.claude/skills/race-check/SKILL.md`）で、変更で偽にならない
