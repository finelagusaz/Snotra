# research — issue #436 検索エンジンの拡張コスト削減

## issue の要約

SearchEngine の SoA（並列 Vec）レイアウトは性能実測に基づく意図的設計だが、派生フィールド 1 本の追加に多数の同時更新＋キャッシュスキーマの version バンプが必要で、拡張コストが構造的に最大の領域になっている。read 側 SoA は維持したまま、書き込み・構築・スコアリングの表現を集約したい。

issue が挙げる改善の方向性は 4 本:
1. 派生 Vec 生成を単一 builder に集約（「フィールド追加は 1 関数」に近づける）
2. `search_with_options`（約 330 行）を候補準備とスコアリング fold に関数分割
3. スコア階層（Prefix 10000 / Substring 5000 / Kana 4500 / Path 3000）を宣言的に定義
4. `SearchMode` の二重定義解消

## 一段抽象化 — 4 方向は「同一構造の別クラス」

4 方向は全て「**1 つの事実が N 箇所に手書きコピーされている**」の顕現。ただしコスト・リスクで階層が分かれる:

| 方向 | 本質 | リスク | 判断 |
|---|---|---|---|
| **2. fn 分割** | 1 関数が N 責務（逆向きの同型） | 低（挙動不変・serialize 非関与） | **今 PR** |
| **3. スコア階層宣言化** | スコア定数が 3 関数に散在 | 低 | **今 PR**（文書化された不変条件をコードで強制していない＝checklist は構造未吸収の診断信号） |
| **1. cache 三兄弟集約** | 同一カラム集合が owned/borrowed/optional の 3 定義 | 高（postcard 位置依存＋on-disk version） | **別 issue で再評価**（field-list マクロなしには畳めず＝過剰設計境界。build 側は #337/#437 で集約済み、残余は cache スキーマに集中） |
| **4. SearchMode 二重定義** | — | — | **偽陽性・畳まない**（下記） |

### direction 4 の裏取り（issue 前提の検証）

- `config::SearchModeConfig`（config.rs:199）: `#[derive(Serialize, Deserialize)] #[serde(rename_all = "snake_case")]` — **config.toml の wire 形式**に紐づく serde 型
- `search::SearchMode`（search.rs:32）: serde 非依存の**純ドメイン enum**
- `From<SearchModeConfig> for SearchMode`（search.rs:39）: config↔engine の**意図的アンチコラプション境界**。`From<&SearchConfig> for SearchOptions`（search.rs:68）と同じパターン

畳むと「engine が serde/wire 形式に依存」または「config が engine 内部に依存」となり層が結合する。→ **削除せず、`SearchMode` の doc コメントに「なぜ 2 定義か（層境界）」を明記**して checklist 化を防ぐ。issue 前提「二重定義解消」を「二重定義の *理由* を明文化」に読み替える（[[feedback_verify_issue_premises]]）。

## 関連コード

### 今 PR で触る（direction 2+3）

- `snotra-core/src/search.rs:370-699` — `search_with_options` 本体。フェーズ構造は既に明瞭:
  - **(a) クエリ準備** L382-443: `norm_query` / `has_dot` / `query_mask` / `kana_query` / `needle_u32` / `has_path_sep` / `path_query` / `path_history_key`
  - **(b) incremental 判定** L445-490: `kana_monotonic` / `use_incremental` / `candidate_indices` / `kana_available`
  - **(c) 並列 fold スコアリング** L492-678: bitmask pre-filter → name/file_name/kana/path スコア → 履歴ブースト → top-k heap
  - **(d) cache 更新 + 結果組立** L680-698: `prev_*` 更新 → `into_sorted_vec` → `SearchResult` 変換
- `snotra-core/src/search.rs:803-827` — `match_score_single_cached`。`10_000`（Prefix, L814）・`5_000`（Substring, L819）がインライン
- `snotra-core/src/search.rs:794-798` — `kana_substring_score`。`4500`（L797）がインライン
- `snotra-core/src/search.rs:604-608` — path マッチ。`3000`（L607）がインライン
- `snotra-core/src/search.rs:729-743` — `adjusted_history_boost`（既に抽出済みヘルパー、参考）

### 触らない（direction 1・別 issue）

- SoA レイアウト（`SearchEngine` struct / `compute_wave1/2` / `assemble` / `EntryView`）
- `indexer.rs` の cache スキーマ（`IndexCache` / `IndexCacheRef` / `CachedMasks` / `INDEX_CACHE_VERSION` / `load_cache` / `save_cache_sorted` / `extend_cached_masks`）

## 既存パターン（再利用可能）

- **フェーズ抽出は自由 fn へ**: `adjusted_history_boost` / `kana_substring_score` / `match_score_single_cached` は既に `impl` 外の自由関数。同じ形で `score_one_entry` を抽出できる（`self` を明示引数化）
- **`EntryView`**: fold ボディが既に `self.entry_view(i)` で per-entry データを束ねている。スコアリング抽出の引数はこの view + マスク配列参照 + クエリコンテキストで構成できる
- **スコア定数の集約先**: 現状の名前付き const（`GLOBAL_WEIGHT` 等 L17-23）と同じモジュールトップに `mod score` を置く

## 技術的制約

- **性能クリティカル・inline 保持が必須**: 抽出した `score_one_entry` は `#[inline]` を付け、rayon fold 内 codegen を変えない。すべて参照/Copy で受け取り、`Matcher` は既存の `thread_local! MATCHER` をボディ内で借りる（引数化しない）。挙動不変＝スコア値・順位・incremental 候補集合が完全一致すること
- **自動ベンチは無い**: `snotra-core/benches/` は存在せず、`PERFORMANCE.md` はプレイブック（数値ベースライン file ではない）。「ベースライン比較」の実体は **既存テストスイート＋不変条件**。Phase 1 は SoA/cache 形式に非接触ゆえ、テスト green ＝退行なしの十分条件に近い
- **スコア階層は不変条件**（`.claude/rules/snotra-core-search.md`）: Prefix(10000) > Substring(5000) > Kana(4500) > Path(3000) > Fuzzy。const 化してもこの全順序を崩さない。テスト `kana_search_direct_match_ranks_above_kana_match` が保証
- **incremental cache の単調性**（`.claude/rules`）: `use_incremental` 述語（`kana_monotonic` 等）と `prev_*` 更新を **(b) と (d) に分けても連動を保つ**。ロジックは移動のみ・変更なし。`/cache-check` で単調性の非退行を確認

## 未解決の疑問

- なし（射程はユーザー承認済み：段階分割・direction 2+3 を今 PR、1 は別 issue、4 は文書化）
