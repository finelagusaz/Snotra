# plan.md — issue #347 StaleSet コヒーレンシ設計（設計先行サイクル）

> **このサイクルのスコープ（ユーザー合意済み）**: 設計メモ先行。成果物は
> `workspace/research.md`（現状精査）+ `docs/design/2026-05-31-coherence-staleset.md`（設計メモ）まで。
> #348 の 2 症状は #347 の StaleSet 機構に統合する前提で設計済み。
>
> **更新（2026-05-31 合意後）**: 設計合意（Q1〜Q4 承認、設計メモ status: **Agreed**）。
> ユーザー判断「Phase 1 先行マージ」に従い、**Phase 1（history を live-read 化）を本セッションで実装・検証済み**
> （snotra-core 367 / snotra 33 / clippy 正規ゲート green）。Phase 2（単一 `index_stale` bit）・Phase 3（残ドキュメント同期）は別サイクル。

## このサイクルの成果物（= 変更ファイル）

| ファイル | 内容 | 状態 |
|---|---|---|
| `workspace/research.md` | 現状コヒーレンシ・アーキテクチャの精査（カテゴリ A/B、3 同期軸、キー二重メンテ） | 作成済み |
| `docs/design/2026-05-31-coherence-staleset.md` | StaleSet 契約・所有者配置・3 軸統合・収束性・失敗モード・代替案・実装ロードマップ | 作成済み |
| `workspace/plan.md` | 本ファイル（サイクル計画 + セルフレビュー） | 本ファイル |

**このサイクルではコード（`*.rs`）・SPEC.md・CLAUDE.md を変更しない**（設計先行のため）。

## 設計の要点（設計メモの要約）

1. **中核契約**: `update_config` を全カテゴリ B 派生物の単一コヒーレンシ・チョークポイントにする。
   軽量は インライン reconcile、重量は stale ledger に記録 → ロック外 drain
2. **所有者**: 判断（何が stale か/ループ要否/swap/軽量 reconcile）= Engine（snotra-core）、
   重い drain の駆動（スレッド/イベント）= src-tauri。レイヤー境界を保つ
3. **3 軸統合**: stale ledger を engine Mutex 軸に置き、コヒーレンシの正しさを軸1 だけに閉じる。
   軸2（AtomicBool）は CAS+UI 専用、軸3（INDEX_WRITE_LOCK）は単一書き手専用に純化
4. **lost-update 解消**: bit-set（update_config）と bit-clear（drain 完了 re-diff）が engine ロックで相互排他 → 取りこぼし窓が閉じる
5. **#348 統合**: top_n ドリフトは **history を live-read 化して B から除外**（焼き込む場所を消す）、lost-update 窓は単一 `index_stale` bit で——「config 由来キャッシュの無効化を update_config が所有」という一契約の帰結に
6. **archaeology による精緻化**: git 史実で本質的制約（lock 最小化/レイヤー境界/2 AtomicBool/INDEX_WRITE_LOCK）と偶発的複雑さ（所有権追放/キー二重メンテ/top_n 漏れ）を切り分け。「カテゴリ B はキャッシュ」と再定義し、**B を既約核（SearchEngine 1 つ）へ縮小**＝StaleSet は単一 bit に収斂（設計メモ §1.5, §2）

## 実装ロードマップ（合意後の別サイクル・概略）

設計メモ §8 を SSOT とする。要約:
- **Phase 1**: history を **live-read 化**（`HistoryStore.top_n` フィールド削除、`save`/`prune` に引数渡し、`Engine` が config から渡す）。#348 欠陥 B を構造的に消す。最小・独立・先行可能
- **Phase 2**: 単一 `index_stale` bit 化（#347 中核 + #348 欠陥 A）。`needs_reindex` / in-flight `needs_rebuild` を 1 機構に統合
- **Phase 3**: ドキュメント同期（CLAUDE.md / rules / SPEC.md / architecture.md）

## 不変条件（実装時に守るべき・設計メモ §5 の要約）

- ロック最小化（重い build は engine ロック外）— 維持
- `INDEX_WRITE_LOCK` 単一書き手 — 維持
- CAS 二重ビルド防止（`try_begin_index_build`）— 維持
- カテゴリ A live-read — 不変
- 新しい同期軸を増やさない（stale ledger は engine Mutex 上の状態）
- stale bit の真偽ペア: set=update_config / clear=drain 完了 re-diff 一致時のみ / 失敗時は残す（戻せない経路を作らない）

## テスト方針（合意後の実装サイクル用・設計段階で確定）

- **Phase 1（history）**: top_n 縮小 → `prune` 容量追従 / 拡大 → 深さ追従のユニットテスト（`history.rs` + `engine.rs`、Win32 非依存）
- **Phase 2（index-stale）**: lost-update 窓の状態遷移ユニットテスト（`state.rs` フラグ + Engine stale 機構、AppHandle 非依存）。
  完了後 re-diff の収束性（config 変更中ループ → 停止）テスト
- 検証コマンドは `docs/build-commands.md` カテゴリ A（snotra-core）/ B（src-tauri）

## SPEC.md 更新要否

- **本サイクル: 不要**（コード挙動を変えない設計メモのみ）
- **実装サイクル: 要**（設計メモ §6 Q5）。top_n_history 変更が再起動不要で即時反映になるのは文書化された挙動変更
  → 実装 Phase 3 で SPEC.md の設定反映に関する記述を同期する

---

## セルフレビュー

### 5a. check スキルによる計画検証

| スキル | 実行 | 結果 |
|---|---|---|
| `/plan-review` 相当 | 実行済み（Explore 3 並列） | 設計の診断（カテゴリ A/B 分類・3 同期軸・キー二重メンテ・lost-update 窓・top_n ドリフト）が**全て実コードで裏付けられた**。「実在しない欠陥を直していないか」= No を確証 |
| `/symmetric-check` 相当 | 実行済み（同上に統合） | カテゴリ B 消費者の網羅（SearchEngine + HistoryStore.top_n の 2 個で漏れなし、他モジュールに B なし）・キー集合の二重メンテ（needs_reindex ↔ in-flight needs_rebuild、両 5 キー一致）・set/clear ペアの対称性を確認 |
| `/cache-check` | 不要 | incremental search キャッシュ（`prev_*`）には触れない。設計対象は config↔派生状態のコヒーレンシであり検索結果キャッシュではない。kana 空ガード等 #337 の incremental 不変条件は本設計で不変 |

#### レビュー結果の反映（要対処への対応）

| レビュー指摘 | 分類 | 対応 |
|---|---|---|
| HistoryStore.top_n に setter が無い / reconcile ループ外 | Agent 1・2「要対処」 | **設計の欠陥ではなく設計が直す対象そのもの**（#348 欠陥 B）。`set_top_n` は新規追加するメソッド（設計メモ §4）。対応不要 = 診断の確証 |
| 「history インライン化で StaleSet 型が不要 → Alt-1 の偽装では？配置だけ一元化で機構は散在では？」 | Agent 3「要対処」最重要 | 設計メモに **§7.1** を追加。型に依存しない 3 構造デルタ（D1 所有権の帰属 / D2 キー集合の単一定義 / D3 軸1 への判断集約）で Alt-1 と本質的に異なることを明示。「StaleSet は契約名であって容器型でない」「bitflags は 3 つめの重量 B が出たら（YAGNI）」を確定 |
| re-diff の cost model が曖昧 | Agent 3「要対処」 | 設計メモ §4 に**コストモデル**追記。`IndexInputs` snapshot = 現状 `indexing.rs:36` の `scan.clone()` と同コスト。比較 O(scan paths)、ループは収束性で 1〜2 反復 |
| 新同期軸を増やさないことの形式確認不足 | Agent 3「要対処」 | 設計メモ §8 Phase 2 着手時チェックリストに「変更後の全フィールド列挙・新 AtomicBool/Mutex 非増加の確認」を追加 |
| needs_reindex 削除の段階的リファクタ路が未明示 | Agent 3「軽微」 | §8 Phase 2 に「全 caller を grep 列挙・段階的置換」を追加 |
| SPEC.md 同期が実装サイクルで漏れるリスク | Agent 3「軽微」 | §8 Phase 3 に「SPEC.md の即時反映記述（top_n_history 再起動不要化）」を明示 |
| show_icons が icon-stale なのにコメント明示なし | Agent 1・2「軽微」 | 設計メモ §6 Q2 が「現状維持＋コメント明示」を既に推奨。実装 Phase で対応 |
| 失敗時リトライの透過性（Q4） | Agent 3「軽微」 | 設計メモ §5/§6 Q4 で既に open decision として明示。暫定「次の config 変更/次回起動で回復、データ損失なし」、明示リトライは別 issue 候補 |

### 5b. セルフレビューチェックリスト

1. **対称コードパス**: 本設計の主眼が対称性の構造的解消。①カテゴリ B 消費者の網羅（SearchEngine だけでなく HistoryStore.top_n を含めた）②キー集合の二重メンテ（needs_reindex ↔ in-flight needs_rebuild）を 1 箇所に集約 ③stale bit の set/clear ペアを engine ロック軸に対称配置。`/symmetric-check` で追加検証
2. **影響範囲の網羅性**: research.md でカテゴリ A 全消費者（8 個）・カテゴリ B 全消費者（2 個）を grep ベースで列挙。`show_icons`（icon-stale）の境界も明示。live-read は変更対象外として根拠付きで除外
3. **境界条件**: lost-update 窓の交錯表（t1-t5）・収束性（config 落ち着き後の停止）・失敗モード（build panic / spawn 失敗 / リトライ契機）を設計メモ §3.3/§3.4/§5 に列挙
4. **リソース管理**: stale bit を「戻せない経路を作らない」原則で設計（失敗時は残す＝wedge しない、AGENTS.md「状態フラグも真偽ペア」準拠）。新ロックを増やさず既存 3 軸を純化
5. **既存パターンとの整合**: PrebuiltIndex（ロック外構築 + atomic swap）/「開始時キャプチャ vs 完了後現在値」比較 / CAS 二重起動防止 / FolderListContext スナップショット——全て既存パターンの再利用。新規パターンは「stale ledger を engine ロック軸に置く」のみで、これも Engine facade の単一 Mutex 設計の自然な延長
6. **YAGNI 違反**: bitflags 型を避け最小実装（index_stale bool + snapshot）を推奨（§6 Q1）。history はインライン reconcile で StaleSet ledger に載せない（§6 Q3）。show_icons 分離はスコープ膨張回避で見送り（§6 Q2）。過剰抽象化を設計段階で削った
7. **シンプル化の挑戦**: 「StaleSet 型は本当に要るか？」を §6/§7 で自問し、2 カテゴリの現状では bitflags 不要・概念契約（update_config が全 B を所有）が本体、と結論。新状態は engine Mutex 上の bool 1 つに留め、新 Mutex/AtomicBool/子プロセスを増やさない。「この操作が失敗したらどうなるか」を §5 で全列挙
8. **破壊不変条件の明示**: ロック最小化・INDEX_WRITE_LOCK 単一書き手・CAS 二重ビルド防止・カテゴリ A live-read——「壊れたら即アウト」を §5 に列挙し、各々「維持」を明記。Win32 フック等「戻ってこない」系には触れない設計（純ロジック層 + 既存 src-tauri 配線のみ）。検知手段は Phase 2 の状態遷移ユニットテスト（AppHandle 非依存で書ける）

### セルフレビュー所見

- 設計先行スコープを厳守し、本サイクルではコード・SPEC・CLAUDE.md・`docs/architecture.md`（evergreen）を変更しない（合意前の既成事実化を避ける）
- 並列レビュー（Explore 3）で設計の診断が全て実コードで裏付けられ、最重要批判（Alt-1 偽装疑い）には設計メモ §7.1 で正面から回答した
- 設計メモは「契約の一元化」を主眼とし、データ構造（bitflags）は最小から始める方針で YAGNI を回避
- 残る要確認点は設計メモ §9 のオープンクエスチョン 3 点（推奨案 Q1〜Q5 への同意 / Phase 1 先行マージ可否 / `docs/design/` 常設採用可否）→ **合意フェーズでユーザーに確認する**
