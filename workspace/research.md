# 調査: issue #1527

## issue の要約

`scripts/race-boundaries.mjs` の `KINDS` ④「managed state の読み書き」は、RwLock の取得を `\.read\(\)` / `\.write\(\)`（引数なし）で拾う（#1040 / PR #1487）。しかし `src-tauri` の config の読みは `AppState::read_config` / `egui_shell::read_config` を経由する。呼び出し側の行は `state.read_config(|cfg| ...)` / `super::read_config(` の形で `.read()` を含まないので、④ に現れない。受け入れ条件は「④ が `read_config` 経由の読みを列挙する／しない理由を書く」と、列挙するなら fixture のテスト。

## 関連するファイル・シンボル

- `scripts/race-boundaries.mjs` `KINDS`（④ の `patterns`）。種別表とパターンの SSOT はこのファイル（`.claude/skills/race-check/SKILL.md` は「SSOT は `scripts/race-boundaries.mjs`」と書き、パターンを写していない）
- `scripts/race-boundaries.test.mjs` `describe("④ の RwLock 系（#1040 …）")` — 差分を `parseDiff` から通し、行とパターンの組で assert する形
- `src-tauri/src/state.rs` `AppState::read_config` — `read(&self.config.read().unwrap())`。**read guard を取る地点は crate 全体でここ 1 つ**（`egui_shell::read_config` の doc）
- `src-tauri/src/egui_shell/mod.rs` `read_config` — `try_state::<AppState>()` 経由で上へ委譲（`try_state` は ④ が既に拾う。ただし定義側の 1 行だけ）
- 呼び出し点（コメント外）: `git grep -nE "read_config\(" -- '*.rs'` でコメント行を除くと 25 行。うち 24 行は config の読み、1 行は `activation/tests.rs` の文字列リテラル `"read_config("`（ソーステキスト検査の禁止語）

## 再利用できる既存のパターン

- #1040 と同じく、パターンを `KINDS` ④ へ 1 本足し、テストで「その行がそのパターンで立つ」を組で固定する
- 差分 fixture の形（`diff --git` → `@@` → `+` 行）をそのまま使う

## 技術的な制約

- `classify` は `findIndex` で**最初に当たったパターン**をラベルにする。`read_config(` の行は `.read()` を含まないので、他のパターンとラベルを奪い合わない（`mod.rs:470` の `s.read_config(read)` も `.read()` ではない——`read)` の前に `(` が無い）
- 名前の衝突: `git grep -nE "[A-Za-z0-9_]read_config\("` は 0 件（`try_read_config(` のような別名は無い）。TS / mjs / ps1 にも出現なし
- 定義行 `pub fn read_config<T>(` / `pub(crate) fn read_config<T>(` は `read_config\(` に当たらない（`<T>` が挟まる）。定義の変更は ④ の `RwLock` 等や `try_state` 側で拾われるので、困らない
- コメント行は `isCommentLine` で除外されるので、doc 中の `[`crate::egui_shell::read_config`]` は当たらない
- 書き込み側: config の書き込みは `Engine::update_config`（`snotra-core/src/engine.rs`、`.write()` で既に当たる）と、呼び出し側の `state.engine.lock().unwrap().update_config(..)`（`\.lock\(` で既に当たる）。**現行の src-tauri の書き込み 2 行（`config_watcher.rs`・`state.rs`）は、`.lock(` と `update_config` が同じ行にあるので立つ**。`let mut e = x.lock().unwrap();` の次行で `e.update_config(..)` と書く形は ④ に立たない（敵対的調査 #1 で採用。下記）。issue の範囲は読みなので、書き込みの入口は足さず、PR 本文の残したものに置く

## 列挙すべきか

列挙する。理由:
- `read_config` は RwLock の read guard を取る**唯一の地点**を包んだもので、呼び出し点はそれぞれが managed state の読みである。`/race-check` のトリガー「フレーム内 live-read」の典型（`egui_shell::read_config` は UI フレームから呼ばれる）
- 誤検出の母集団は現行で 25 行中 1 行（テストの文字列リテラル）。#1040 で `\.read\(` を退けた比率（119 行中真陽性 14 行）と比べて精度が高い
- 列挙しない理由（「`read_config` の契約が錠と I/O を禁じているので境界にならない」）は弱い: 契約が守るのは読みの中で待たないことであり、読んだ値がいつの値か（staleness）は契約の外である

## 未解決の疑問

- なし（パターンの綴り `read_config\(` で足りるかは上の制約で確認済み）

## 敵対的調査の反映（`workspace/adversarial-1527.txt`）

スクリプトの `selectPopulation` と `KINDS` を import して、追跡中の全ソース行に通して測った。

- 壊せなかった: 25 行の一致（`isCommentLine` を通しても同じ）・現行 ④ では 25 行すべて未検出・`read_config\(` で全行が立つ・別名 0 件・ラベルの奪取 0 件・パターンの写しは文書側に無い
- **採用**: 「書き込みの入口は要らない」は全称として偽（`.lock(` と `update_config` が別の行なら立たない）。現行 2 行が立つ、へ弱めた。足すのは issue の範囲外として PR 本文の残したものへ置く
- 採用（テストに反映）: 25 行には `AppState` への委譲（`mod.rs` の `s.read_config(read)`）も含む。テストの fixture は `AppState::read_config` を直接呼ぶ形と `egui_shell::read_config` を呼ぶ形の 2 つを使う
- 不採用: 定義行（`fn read_config<T>(`）を拾うようにパターンを `read_config` へ緩める案。定義の変更は読みの境界ではなく、RwLock の取得行 `self.config.read()` は既存の `\.read\(\)` が拾う
- 不採用（記録のみ）: テストの文字列リテラル 1 行の偽陽性・行末コメントの取りこぼし・将来の `.lock(` によるラベル奪取。いずれも ④ の他パターンと同程度の入口の粗さで、「入口であって網羅ではない」の範囲
