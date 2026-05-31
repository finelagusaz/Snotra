# plan.md — issue #347 Phase 2: index_stale ledger 化（+ #348 欠陥 A）

> 設計 SSOT は `docs/design/2026-05-31-coherence-staleset.md`（status: Agreed）。本ファイルは Phase 2 サイクルの計画と記録。
> Phase 1（history live-read 化, #348-B）は #350 でマージ済み。本サイクルは **Phase 2 = #347 構造的中核 + #348 欠陥 A（lost-update 窓）**。

## ゴール

config 変更→index 再構築の整合を「engine の単一 `index_stale` ledger + 単一 `IndexInputs` 定義」に統合し、
config_watcher の `!indexing` ゲートを撤去して lost-update 窓を閉じる。コヒーレンシ判断を engine Mutex（軸1）に閉じ、
AtomicBool（軸2）を CAS+UI 専用に純化する。

## 変更ファイル

| ファイル | 変更 |
|---|---|
| `snotra-core/src/engine.rs` | `IndexInputs`（5 キー単一定義, `From<&Config>`, `PartialEq`）/ `index_stale: bool` フィールド / `mark_index_stale` / `begin_index_drain` / `complete_index_drain` / `is_index_stale` を追加。コンストラクタで `index_stale: false` 初期化。`update_config` は据え置き |
| `src-tauri/src/config_watcher.rs` | `needs_reindex` 削除（テストも）。index 判定を `IndexInputs` 差分に。`!indexing_in_progress` ゲート撤去（常に kick）。`Ordering` import 削除 |
| `src-tauri/src/indexing.rs` | `start_index_build` を drain ループに書き換え（mark→CAS→spawn→begin/build/complete ループ→finish→post-finish 再チェック）。`catch_unwind` で panic でも finish。in-flight `needs_rebuild` 削除 |
| `snotra-core/CLAUDE.md` / `src-tauri/CLAUDE.md` | engine の index_stale ledger / drain ループ / panic-safety / gate 撤去を記録 |
| `docs/design/...staleset.md` §8 | Phase 2 実装済み + スケッチからの確定点を追記 |

## スケッチ §4 からの確定点（マルチパースペクティブレビュー反映）

1. **bit を立てるのは `start_index_build`**（`update_config` でなく）。first-run / 手動 rebuild は config 変更を伴わないため。`update_config` の呼び出し元は config_watcher 唯一と確認済み → 取りこぼしなし
2. **finish 後に `is_index_stale` 再チェック → 再 kick**（complete clear〜finish の窓を閉じる）
3. **build を `catch_unwind` で包み panic でも `finish_index_build`**（panic wedge 対策＝レビュー Agent 1 検出。panic 経路は再 kick せず無限リトライ回避）

## TDD（実施済み）

- engine 層（スレッド不要・AppHandle 非依存）:
  - `fresh_engine_is_not_index_stale` / `mark_index_stale_makes_begin_return_current_inputs`
  - `complete_index_drain_clears_stale_when_config_stable`
  - **`complete_index_drain_keeps_stale_when_config_changed_during_build`**（lost-update #348-A の核。RED→GREEN: 仮実装の無条件 clear で失敗→条件付き clear で通過）
  - `index_inputs_differ_on_each_index_key` / `index_inputs_equal_when_unrelated_key_changes`（needs_reindex テストの移設先）
- 検証: snotra-core **373** / snotra **32** / clippy 正規ゲート **green**

## 不変条件（維持を確認）

ロック最小化（重い build はロック外・begin/complete/mark は O(1)〜O(scan)）/ INDEX_WRITE_LOCK 単一書き手 /
CAS 二重ビルド防止 / first-run・手動 rebuild が壊れない / 新しい同期軸を増やさない（index_stale は engine Mutex 上）/
panic で flag 固着しない（catch_unwind + 必ず finish）。

## マルチパースペクティブレビュー結果（実装前）

- **並行性**（Agent 1）: 全インターリーブで lost-update / 二重ビルド / 無限ループなしを確認。**panic wedge を検出→ catch_unwind で同梱修正**
- **呼び出しグラフ / first-run**（Agent 2）: first-run・手動 rebuild・キー集合・icon prune 冪等性すべて問題なし
- **不変条件 / 乖離**（Agent 3）: 乖離（start_index_build が bit を立てる）は first-run 由来の正当な確定。`update_config` 呼び出し元は config_watcher 唯一で取りこぼし穴なし。TDD 強化を要対処→実施

## 未テスト範囲（許容・記録）

finish 窓 / CAS / panic 経路はスレッド + AppHandle を要し単体テスト不可。engine 層の状態機械テスト + state.rs の CAS テスト
+ コードレビューで担保。決定論テスト不能な並行部分は既存方針（state.rs を分離テスト）に倣う。

## 残り（別サイクル）

- **Phase 3**: `docs/architecture.md` に StaleSet 契約 + 設計メモ参照、`.claude/rules/*` 同期（相談のうえ）
- #348 欠陥 A（lost-update 窓）は本 Phase で解消 → #348 はクローズ可。#347 は Phase 3 完了で完結
