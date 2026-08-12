# research — issue #1057 パスクエリが全件でフルパスを組み直す

**ブランチ**: `perf/path-query-forward-pass` / **起点**: `d631e0b` / **調査日**: 2026-08-12

## issue の要約

パスクエリ（`has_path_sep`）は Fuzzy のビットマスク pre-filter を丸ごとスキップするため、
312,108 件すべてがスコアリングを通る。実測（`tests/path_query_cost.rs` の p50）:

| クエリ | p50 |
|---|---|
| `users`（区切り無し） | 94–111 µs |
| `c:\users` | 11,830–13,080 µs |
| `\zzz-no-such-path\`（**0 件**） | 10,451–11,143 µs |
| `c:\` | 16,893–18,669 µs（**60fps の 1 フレーム 16,700 µs を超える**） |

issue が提案する手段は「KMP 状態を親から子へ伝播させる前向き 1 パス」で、1 件あたりのコストを
「フルパスの長さ」から「自分のセグメントの長さ」へ落とす、というもの。

## **最重要の所見: issue が記す機序は現在のコードと食い違う**

issue の「機序」節は 1 件ごとに次の 3 つが走ると書く:

1. `walk_to_root` で祖先の鎖を辿る
2. フルパスを組み立て直す（`PathCursor::append`）
3. `key.find(pq)` する

**1 と 2 は既に最適化済みである。** `search/path_store.rs` の `PathCursor::normalized`
（`snotra-core/src/search/path_store.rs:314-`）は祖先の鎖を持ち回る:

- 親が鎖に載っていれば `buf.truncate` で**巻き戻して自分の 1 段だけ書き足す**
- `walk_to_root` は**鎖が外れたときだけ**走る（索引はソート順ゆえ隣接エントリは祖先をほぼ共有）
- 小文字化も書き足した範囲にだけ当てる

つまり **issue が提案する「セグメント単位へ落とす」は、組み立てについては既に達成されている。**
残る全長依存は 3 の `key.find(pq)` だけである。

### 額の裏付け（既存の実測・`PERFORMANCE.md`「パスクエリ全走査のコスト」）

同ハーネス内の 2 層は別物であり、`//!`（`tests/path_query_cost.rs:20-28`）が明記する:

- **製品レベル**（`measure_path_query_frame_cost`）= 判定に使う層 → **10.4–13.1 ms**
- **走査だけを切り出した写し**（`measure_path_query_sweep_cost`）= 導出 + `find` だけ →
  `\zzz-no-such-path\` で **4.0 ms**（ASCII 高速路・0 hits）

`//!` 自身が「**見積もりが甘くなる（実測で写し +3 ms に対し製品は +8〜16 ms）**」と書いている。

→ **issue の提案が狙えるのは製品 ~11 ms のうち ~4 ms 側であり、残る ~6〜7 ms は別の場所にある。**

## 残り ~6〜7 ms の在り処（**仮説・未測定**）

`search/scoring.rs:337` の pre-filter スキップにより、`has_path_sep` のとき **312k 件すべてが
以下を通る**（`score_one_entry` の本体・`scoring.rs:349-` 以降）:

```rust
let v = self.entry_view(i);
let name_u32_owned = if mode == SearchMode::Fuzzy { Some(Utf32String::from(v.lower_name)) } else { None };
let name_score = MATCHER.with(|m| { ... match_score_single_cached(mode, ..., v.lower_name, norm_query_str, ...) });
```

- **1 件ごとに `Utf32String` の確保と UTF-32 変換**（コメントは「ビットマスク通過分（全体の
  1-5%）にのみ発生する」と書くが、**パスクエリではその前提が成り立たない**）
- **1 件ごとに nucleo の fuzzy マッチ本体**

`norm_query` は区切り文字を落とさない（`query.rs:134` の `normalize_query` は小文字化・
アクセント畳み・連続スペース潰しのみ）。ゆえに `c:\users` は `\` を含んだまま
**表示名**（`lower_name`）に対して部分列マッチを試みる。

→ **仮説: パスクエリのとき name / file_name の fuzzy マッチは構造上ほぼ必ず外れるのに、
312k 回走っている。** 成立すれば、issue の前向きパスより**小さい差分で大きい額**が取れる。

**この仮説は未測定であり、実装前に潰す（plan の未確定欄）。** 全称の形（「必ず外れる」）で
書けるかは、表示名が区切り文字を含みうるかに依存する——`push_segment`
（`path_store.rs`）が「セグメントを `/` → `\` だけ直して追記する」形を持つことは、
**生の名前が `/` を含みうる**ことを示唆する。裏取りするまで全称では書かない。

## 関連ファイル・シンボル（すべて grep で実在確認済み）

| ファイル | シンボル | 役割 |
|---|---|---|
| `snotra-core/src/search/scoring.rs` | `score_one_entry`（:318） | 1 件のスコアリング。pre-filter スキップは :337 |
| 同 | `with_normalized_key`（:79） | 正規化キーを得る**唯一の経路** |
| 同 | `EntryView`（:92） | `entry` / `lower_name` / `lower_file_name` |
| `snotra-core/src/search/path_store.rs` | `PathCursor`（:314）・`normalized`（:335）・`append`（:383） | 鎖を持ち回る組み立て |
| 同 | `with_cursor`（:409） | thread_local カーソル（rayon worker ごと 1 本） |
| 同 | `push_segment` | `/` → `\` のみ直す。ASCII 一括処理 |
| `snotra-core/src/search/query_plan.rs` | `has_path_sep`（:90）・`path_query`（:94）・`path_history_key`（:111） | クエリ側の派生 |
| `snotra-core/src/query.rs` | `normalize_query`（:134） | **区切り文字を落とさない** |
| `snotra-core/src/indexer.rs` | `normalize_entry_key_into`（:358） | 正規化規則の**正本** |
| 同 | `normalize_file_name_key_into`（:1578） | 末尾セグメントだけ通す派生（反復 9 の先例） |
| `snotra-core/src/index_tree.rs` | `walk_to_root`（:278） | 鎖が外れたときだけ走る |
| `snotra-core/src/search.rs` | :199 | incremental cache の `!has_path_sep` ガード |

## 再利用できる既存パターン

- **セグメント単位の正規化の先例**: `normalize_file_name_key_into`（末尾セグメントだけを
  `normalize_entry_key_into` と同じ規則へ通す・反復 9）。issue の成立条件が名指しする形
- **バイト一致の固定**: `search/tests/path.rs` の
  `path_store_cursor_matches_normalize_entry_key_over_real_index`（実 index 全件）と
  `path_store_cursor_matches_full_rebuild`（順・逆順・乱順の合成 fixture）
- **2 層を混ぜない計測**: `tests/path_query_cost.rs` の製品レベル / 写しの分離

## 技術的制約

- **`sorted_by_path` は前提にできない**——`include_path_env = true` の PATH 併合で偽になる
  （RETROSPECTIVE.md 反復 12「前提が崩れうるを理由に、崩れているかを測らずに却下していた」は
  **既定が `false`** であることを測れという教訓であり、実運用点の値を測って判断する）。
  ただし `IndexTree::from_parts` が `parent < i` を検証しており、**添字の昇順が親→子の順序を
  保証する**のは PATH 併合後も成り立つ（前向き 1 パスの前提はこちら）
- **前向き 1 パスは逐次になる**。現在の走査は rayon の fold/reduce で並列。「逐次 1 パス +
  並列スコアリング」の 2 段構えになり、**総和で勝つかは未実測**
- **incremental cache はパスクエリで無条件に無効**（`search.rs:199`）。この案は変えない
- **`with_normalized_key` の中から再帰的に呼べない**（`borrow_mut` 二重取得で panic）
- **履歴照合はマッチ成立後にのみキーを要求する**（`let base_score = score?;` より後）。
  ゆえにキー導出は「パスマッチの照合」と「マッチ後の履歴 3 種」の 2 用途
- `#1055` が受容した「パスクエリ 2 経路が +4.4% / +5.0%」はこの 11.5 ms の上に乗った
  約 500 µs である

## 未解決の疑問（plan の未確定欄へ引き継ぐ）

1. 製品 ~11 ms の内訳——(a) キー導出 + `find`、(b) name/file_name の fuzzy、(c) top-k・その他
   の 3 分割はどうなっているか。**issue の提案は (a) しか触らない**
2. 表示名（`lower_name`）が `\` / `/` を含みうるか。含まないなら (b) は丸ごと落とせる
3. 落とした場合に検索結果が 1 件も変わらないか（実 index 全件での突き合わせ）
4. `include_path_env` の実運用点の値（`sorted_by_path` の成否に効く）
