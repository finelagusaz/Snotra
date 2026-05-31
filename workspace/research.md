# research.md — issue #347 config↔派生状態コヒーレンシ所有の一元化（StaleSet 契約）

> このサイクルは **設計先行**（ユーザー合意済み）。成果物は本 research.md と
> `docs/design/2026-05-31-coherence-staleset.md` の設計メモまで。実装は合意後の別サイクル。
> #348 の 2 症状は #347 の StaleSet 機構に**統合**する前提で設計する（ユーザー合意済み）。

## issue の要約

config は単一の source of truth だが、その派生消費者が **2 つの異なる整合契約**に分かれ、
それを束ねる「コヒーレンシ所有者」がいない。

- **カテゴリ A（live-read 型）**: 毎操作で config を読み直す → 設定変更が自動で即時整合
- **カテゴリ B（構築時焼き込み型）**: config 値が長命オブジェクトに凍結される → 整合には再構築が必要

`Engine` は `config` + `search_engine` + `history` を単一 Mutex で共同所有する自然な所有者だが、
`update_config` は **config を差し替えるだけで派生状態に触れない**。結果、カテゴリ B の整合責任が
Engine の外（config_watcher → needs_reindex → start_index_build の非同期ループ）へ追放され、
しかも **index 由来（entries+kana）だけ**がカバーされ、`HistoryStore.top_n` は漏れている。

→ `update_config` を「config を差し替え、**どの派生物が stale になったか**を記録/反映する」
**単一のコヒーレンシ・チョークポイント**にする。重い再構築はロック外（PrebuiltIndex パターン）を維持。

## 関連コード（コヒーレンシ・アーキテクチャの精査）

### カテゴリ A（live-read 型）— 即時整合。**変更しない**

すべて `Engine` が毎操作で `self.config` から読み直す（`engine.rs`）:

| 派生値 | 読み取り箇所 | config ソース |
|---|---|---|
| `mode` | `engine.rs:80` `SearchMode::from(config.search.normal_mode)` | `normal_mode` |
| `normalization` | `search.rs:68` `SearchOptions::from` | `history_normalization` |
| `fuzzy_history_cap_ratio` | `search.rs:69` | `fuzzy_history_cap_ratio` |
| **`migemo_enabled`（フラグ）** | `search.rs:70` → `kana_query` を計算するか否か | `migemo_enabled` |
| `migemo_min_chars` | `search.rs:71` | `migemo_min_chars` |
| **検索の取得上限 `fetch_limit`** | `engine.rs:83` `effective_top_n_history()` | `top_n_history` |
| `recent_history` の `max` | `engine.rs:89` `effective_max_history_display()` | `max_history_display` |
| folder ctx（mode/hidden/max） | `engine.rs:95-97` | `folder_mode`/`show_hidden_system`/`top_n_history` |

> **重要な分裂**: `migemo_enabled` は **フラグとしては A**（`kana_query` を作るかの即時判定）だが、
> **`kana_lower_names` の構築入力としては B**。同様に `top_n_history` は **検索取得上限としては A**
> （`fetch_limit` live-read）だが **`HistoryStore.top_n` としては B**。
> 一つの config キーが両カテゴリにまたがるため、「キー単位」での整合判断は誤りを生む。
> stale は **派生オブジェクト単位**で追う必要がある。

### カテゴリ B（構築時焼き込み型）— 再構築が必要

| 派生オブジェクト | 焼き込み箇所 | config 入力 | 整合の現状 |
|---|---|---|---|
| `SearchEngine`（entries + lower_names + char_masks + normalized_keys + **kana_lower_names**） | `engine.rs:46/63` `new_with_migemo` / `new_with_cached_masks`、構築は `compute_wave1`/`compute_wave2`（`search.rs`） | `scan` / `show_hidden_system` / `include_path_env` / **`migemo_enabled`** | ✅ config_watcher の非同期ループでカバー |
| **`HistoryStore.top_n`** | `history.rs:37-47` `load(top_n)`、`main.rs:404` で起動時焼き込み。`prune()`（`history.rs:192-214`）が使用 | `top_n_history` | ❌ **reconcile ループに含まれていない**。setter も無い |

### index 由来 B の整合ループ（現状）と 3 つの同期軸

`config_watcher.rs::apply_config_change`:
1. `Config::load_reporting()` で新 config 読込
2. `update_config(new_config)`（`engine.rs:148-150`）— **config 差し替えのみ**
3. `needs_reindex(old, new)`（`config_watcher.rs:211-217`）が true かつ `!indexing` なら `start_index_build`

`indexing.rs::start_index_build`（背景スレッド）:
1. engine ロック内で index 入力 5 つをキャプチャ（`scan`/`show_hidden_system`/`show_icons`/`include_path_env`/`migemo_enabled`、`indexing.rs:32-42`）
2. ロック外で `rebuild_and_save` + `PrebuiltIndex::new`（`indexing.rs:44-74`）
3. engine ロック内で `apply_prebuilt_index`（O(1) スワップ、`indexing.rs:75-78`）
4. engine ロック内で **完了後 needs_rebuild 再判定**（`indexing.rs:81-90`、キャプチャ値 vs 現在 config）
5. `finish_index_build()`（`state.rs:32-35`）
6. `if needs_rebuild { start_index_build(再帰) }`（`indexing.rs:106-108`）

**この整合は 3 つの分離した同期軸にまたがる**:
- **軸1: engine Mutex**（`state.rs:7`）— `update_config` / `apply_prebuilt_index` / 完了後 needs_rebuild 読み取り
- **軸2: `indexing` / `index_build_started` AtomicBool**（`state.rs:8-9`）— UI 表示 + 二重ビルド防止 CAS（`try_begin_index_build` / `finish_index_build`）
- **軸3: `INDEX_WRITE_LOCK`**（`indexer.rs:520`）— `index.bin` の単一書き手

**コヒーレンシの正しさが軸1と軸2の両方に依存しているのが構造的弱点**（→ lost-update 窓 = #348 欠陥 A）。

### キー集合の二重メンテ（migemo 特別扱いの正体）

index 入力キーの集合が **2 箇所**に重複定義されている:
- `config_watcher.rs:211-217` `needs_reindex`（変更検出側）
- `indexing.rs:85-90` 完了後 `needs_rebuild`（in-flight 再判定側）

#337 で migemo を追加したとき、**両方**に同時に入れる必要があった（`snotra-core/CLAUDE.md` /
`.claude/rules/snotra-core-search.md` に「両方に含める」と明記）。この「同一キー集合の対称二重メンテ」が
新しい index 入力を足すたびに対称漏れリスクを生む。StaleSet ではこの集合を **1 箇所**に集約できる。

## 既存パターン（再利用できるもの）

- **PrebuiltIndex（ロック外構築 → atomic swap）**: 重い再構築をロック外に追い出す確立パターン（`engine.rs:28-36`/`160-162`）。StaleSet の「重い drain」はこれをそのまま使う
- **「開始時キャプチャ vs 完了後の現在値」比較**（`indexing.rs:32/81`、AGENTS.md パターン）: 設計の「完了後 re-diff で stale を条件付きクリア」はこの一般化
- **CAS 二重起動防止**（`state.rs:18-28` `try_begin_index_build`）: drain の単一実行保証に流用。StaleSet を入れても CAS はそのまま
- **FolderListContext（スナップショット）**（`engine.rs:93-111`）: ロック外処理に config スナップショットを渡すパターン。index snapshot も同型
- **Engine facade（単一 Mutex）**（`state.rs:7`、`src-tauri/CLAUDE.md`）: config + 派生状態が同一ロック下にあるため、stale ledger を engine ロック軸に載せれば「変更検出軸 = 整合判断軸」に統合できる
- **`#[must_use]` によるフラグ戻し漏れ検出**（AGENTS.md「状態フラグも真偽ペア」）: stale bit の set/clear ペア設計に適用

## 技術的制約

- **ロック最小化原則（絶対）**: 重い再構築（秒単位）を engine ロック内でやってはならない（`src-tauri/CLAUDE.md` ロック最小化節）。変えるのは「整合の所有と網羅」であって「同期 vs 非同期」ではない
- **レイヤー境界**: `snotra-core` は Tauri に依存できない。スレッド spawn・`AppHandle`・イベント emit は `src-tauri` の責務。→ **コヒーレンシ判断（何が stale か・ループ要否）は Engine（snotra-core）、重い drain の駆動（スレッド）は src-tauri** に分離せざるを得ない
- **`icons.bin` は src-tauri の資源**（`snotra-core` ルール「icons.bin に触れない」）。現状 `show_icons` が `needs_reindex` に含まれるのは index ビルドのついでにアイコンキャッシュを prune するため。厳密には `show_icons` は **icon-stale（src-tauri 所有）であって index-stale ではない** → スコープ境界として要明示
- **`INDEX_WRITE_LOCK` 単一書き手契約を維持**（`indexer.rs`）。drain も `rebuild_and_save` 経由でこのロックを通る
- **CAS 二重ビルド防止を維持**（`state.rs`）。`index_build_started` は CAS 専用、`indexing` は first-run でビルドスレッド不在でも true になる UI 表示用——2 フラグの役割分担を壊さない
- **後方互換**: `HistoryStore.top_n` 追従を足しても history.bin フォーマットは不変（top_n は永続化されない実行時パラメータ）。マイグレーション不要
- **Win32 非依存でテスト可能**: stale 判定ロジック・history top_n 追従は `snotra-core` 内ユニットテスト可能。lost-update 窓の状態遷移は `state.rs` のフラグ機構でテスト可能（AppHandle 非依存）

## 未解決の疑問（→ 設計メモで意思決定）

1. **StaleSet の粒度**: bitflags（index-stale / history-stale / …）か、最小実装（index-dirty フラグ + config snapshot ＋ history はインライン reconcile）か。2 カテゴリしかない現状で bitflags は YAGNI か？
2. **`show_icons` の扱い**: index-stale に含めたまま（現状維持・スコープ最小）か、icon-stale として分離（概念的に正しいが src-tauri 側の別機構）か
3. **history-stale の drain モード**: 軽量（O(top_n)）なのでインライン reconcile（update_config 内で `set_top_n` + 必要なら prune）か、StaleSet の「軽量 drain クラス」として統一モデルに載せるか
4. **失敗時の回復**: drain（ビルド）が失敗したとき stale bit は set のまま残す設計だが、再試行契機は「次の config 変更」のみで十分か、明示的リトライが要るか
5. **収束性**: 完了後 re-diff で「config がビルド中に変わったらループ」する設計の停止性（config が落ち着けば収束する、の厳密化）
