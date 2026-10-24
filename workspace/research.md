# #1280 調査

## issue の要約

検索の rayon の `fold` / `reduce` は葉ごとに確保する（fold の `local_matches` の成長、reduce の identity の `TopK::new` と空の `Vec` への写し、fold の identity の `TopK::new`）。`tests/search_allocations.rs` の計器は worker 1 本で数えるため、この層が 1/10〜1/20 にしか映らない。受け入れは、全スレッドを数える計数器で確保回数が減り、検索結果が 1 件も変わらないこと。計器を足すなら、スレッドローカルの既存計数器とは別に要る。

## 関連ファイル・シンボル（grep で実在確認済み）

- `snotra-core/src/search.rs` の `SearchEngine::search_with_options`
  - `(0..candidate_count).into_par_iter().map(..).with_min_len(MIN_PAR_CANDIDATES).fold(..).reduce(..)`
  - `MIN_PAR_CANDIDATES = 512`（`search.rs:41`）
- `snotra-core/src/search/scoring.rs` の `TopK`
  - `new` は `BinaryHeap::with_capacity(limit + 1)` で先に確保する
  - `push` と `merge` は変えない（`merge` は相手の各要素を `push` へ通すだけ）
- `IncrementalCache::update` / `take_candidates`（`search.rs`）
  - `all_match_indices` がそのまま `prev_candidates` として残り、次の打鍵で `reused` として読まれる
  - したがって、この `Vec` の容量は次の打鍵まで残る
- `snotra-core/tests/search_allocations.rs`
  - 計数器 `Counting` はスレッドローカル
  - 実行の入れ物は `on_single_worker`
  - 検知器 3 本（CI）と計器 `measure_search_allocations`（`#[ignore]`）がある
- 既存の不変テスト
  - `search/tests/ranking.rs` の `topk_merge_is_order_independent` と `search_top_k_is_independent_from_input_order`
    - どちらも `MIN_PAR_CANDIDATES` 未満の規模なので、分割を通らない
  - `search/tests/incremental.rs` の `incremental_reuse_matches_a_fresh_engine_when_the_candidates_split`
    - 4 × 512 件で分割を通るが、既定プールで走らせており、スレッド数を固定していない
  - `search/tests/ranking.rs` の `rank_cmp_breaks_full_tie_with_target_path` と `topk_full_tie_keeps_first_inserted_and_does_not_thrash`
    - 完全 tie の扱いを固定する
  - `search/tests/basic.rs` の `search_no_entries_returns_empty`
    - 候補 0 件をカバーする
- `PERFORMANCE.md`「採用: 結果の組み立てでパスを長さちょうど 1 回で確保する（#1277）」の「内訳の帰属」
  - この issue の出所である
- 壁時計の計器: `tests/search_frame_cost.rs` の `measure_search_frame_cost`（合成）と `tests/path_query_cost.rs` の `measure_path_query_frame_cost_at_operating_point`（実運用点）

## 再利用できる既存パターン

- **全スレッドの計数は、新しいアロケータを作らずに得られる。**
  - `rayon::ThreadPool::broadcast`（rayon-core 1.13.0）で、各 worker のスレッドローカル計数を読んで足す。呼び出し側のスレッドの分も足す
  - broadcast 自身の確保は、空打ち 2 回の差で差し引く
  - プールが専用なので、libtest が並行に走らせる他のテストの確保は混ざらない
  - 既存の `Counting` をそのまま使えるので、`#[global_allocator]` を足さずに済む（1 バイナリに 1 つしか置けない）
  - 「スレッドローカルの既存計数器とは別に要る」という issue の文は、broadcast で合計する形でも満たせる。計数の主体（アロケータ）は同じで、数える範囲が違う
- 検知器の形は既存の 3 本に揃える。確保回数を数え、閾値は変更前と変更後の間に置き、変更前の実装で赤になることを確かめる

## 実測（spike・HAL9000・2026-10-24・release・main = `0e97509`）

生の表は `C:\Users\Eoh\AppData\Local\Temp\claude\C--workspace-Snotra\62d88718-9e4d-4011-b6ac-b89cd4db5110\scratchpad\spike-1280-numbers.md` にある。試作の差分は同じディレクトリの `spike-1280-combined.diff` と `spike-1280-harness.diff`。

### 全スレッドの確保回数（既定プール 16 スレッド・実 `index.bin` 22,417 件・`Engine::search`・PATH マージなし・3 巡で ±2）

| query | 変更前 | `reduce_with` のみ | 採る形（下記） |
|---|---:|---:|---:|
| `c` | 754 | 691 | 500 |
| `co` | 680 | 615 | 483 |
| `code` | 534 | 472 | 430 |
| `s` | 757 | 691 | 494 |
| `c:\` | 477 | 445 | 446 |
| `c:\users\` | 284 | 252 | 253 |

- worker 1 本の値（#1277 の後）は `c` 437・`c:\` 417 で、葉に比例する分は `c` で 317 → 63 回になる
- issue の 1,031〜1,051 は #1277 より前の値である。#1277 が削った約 290 回を引くと、この表の 754 に合う

### 採る形（3 点の組）

1. `reduce(identity, op)` を `reduce_with(op).unwrap_or_else(|| (TopK::new(max_results), Vec::new()))` にする
   - reduce 側の identity の `TopK::new` と、葉の一致を空の `Vec` へ写す確保が消える
2. join の `extend` の直前で `a_matches.reserve_exact(b_matches.len())` を呼ぶ
3. fold の最初の一致で `local_matches.reserve_exact(MIN_PAR_CANDIDATES)` を呼ぶ
   - `with_min_len` があるので、葉の長さは（候補が 512 件以上なら）512 以上になる

### 発見: 1 だけでは壁時計が退行する（機序は実測済み）

`reduce_with` だけの版は、100k 件の `a` の p50 が全件走査で 1,319〜1,474 → 1,709〜1,754 µs に悪化した。再利用の枝でも同じ程度悪化し、どちらも 2 巡で再現した。50k 件以下では差がノイズに埋もれる。

- **最終容量**（`all_match_indices`・100k 件・一致 73,078 件）は次のとおりで、21 回とも同じ値だった
  - 変更前: 73,216
  - `reduce_with` のみ: **131,072**
  - 2 を足した版: 73,078
- 変更前は reduce の identity が空の `Vec` で、そこへの `extend` は容量 0 から必要量ちょうどを確保していた。`reduce_with` にすると、葉が倍々で伸ばした `Vec` がそのまま join の左辺になり、根で 2 の冪へ膨らむ
- 2 を足すと、退行は消えた（`spike_wall` で 1,308〜1,376 µs）
- 容量が膨らむことと、2 で直ることは実測した。**ただし、最終容量そのものが壁時計の原因だという説は敵対的調査で崩れた。** `reduce_with` の後に `shrink_to_fit` で最終容量だけを長さちょうどに縮めた版 (d) は、退行が消えなかった（100k・Substring・全件走査で 1,615〜1,658 µs。変更前は 1,422〜1,481 µs）
  - 消えるのは、join で `reserve_exact` する版だけである
  - 効いているのが join ごとの成長の形なのか最終容量なのかは、分離できていない（(d) は shrink 自身の realloc を含むので、純粋な対照ではない）
  - 採るのは「join で `reserve_exact` すれば退行が消える」という実測であり、機序は書かない
- 退行は Substring（実運用の mode）で顕著だった。Fuzzy の 100k 全件走査では、(b) は退行しなかった（敵対的調査）
- 容量は `prev_candidates` として次の打鍵まで残るので、`reduce_with` だけの版は常駐でも約 1.8 倍になる

### 壁時計（採る形 vs 変更前・`search_frame_cost`・交互 2 巡・p50 µs）

**この表は「再利用の枝 × Fuzzy」の値である**（敵対的調査）。`measure` は同じクエリを 22 回連打するので、`can_reuse` の `starts_with` が等号で真になり、計測の 20 回はすべて再利用の枝を通る。また `Config::normalized_default()` の mode は Fuzzy で、実運用の Substring と違う。Substring の全件走査と再利用の枝は、敵対的調査の `adv_wall`（既定のアロケータ・3 巡交互）で測った。採る形 (e) は、全件走査 1,365〜1,406 µs（変更前 1,422〜1,481）、再利用 1,174〜1,214 µs（変更前 1,252〜1,281）だった。

| entries / query | 採る形 | 変更前 |
|---|---|---|
| 100k `a` | 1,000 / 1,008 | 1,082 / 1,506 |
| 100k `app` | 1,430 / 1,472 | 1,518 / 1,731 |
| 50k `a` | 554 / 560 | 566 / 602 |
| 10k `a` | 216 / 214 | 218 / 222 |

どの行でも、採る形は変更前のレンジの内側か下にある。実運用点（`path_query_cost` の PATH マージあり）はまだ測っていない。

### 検知器の試作（合成・全件一致 `alpha`・`MAX_RESULTS` 10・5 回）

| threads | n | 採る形 | 変更前 |
|---|---|---|---|
| 1 | 8,000 | 36 | 53 |
| 2 | 8,000 | 50 | 103 |
| 4 | 8,000 | 56 | 127 |
| 8 | 8,000 | 56 | 127 |
| 4 | 32,000 | 132–142 | 365–402 |

- n = 8,000 では、葉の数が `with_min_len(512)` で上から抑えられる（4 と 8 スレッドの値が一致した）。差（多スレッド − 1 本）は、採る形で 14〜20、変更前で 50〜74 になる
- **2 スレッドでは、負荷下で変更前を見逃す**（敵対的調査）。busy-loop を 32 本かけると、変更前の差が 26〜29 に落ちて、閾値 32 を下回った
  - 4 スレッドと 8 スレッドでは、負荷下でも変更前は 74〜77、採る形は最悪 29 だった
  - 検知器は 4 スレッドのプールに固定する
- n = 32,000 は揺れが大きい（±10）ので、検知器には使わない

## 技術的制約

- **top-k の結果は不変である。** `TopK::merge` は相手の各要素を同じ `push` 規則へ通す。`ScoredEntry::Ord` の下で、最終集合は併合の順序に依らない
  - identity を外すと、葉の heap を空の heap へ入れ直す工程が消える。押し込む要素の集合は変わらない
  - 完全 tie（`Ord` の 4 キーがすべて等しい）では、どれが残るかが併合の順序に依存し**うる**（理屈の上で。観測はしていない）
    - 敵対的調査は、完全 tie で limit 1〜4・スレッド 1/2/4/8/16・各 200 反復を変更前と採る形で走らせ、すべて一致し、スレッド間でも結果は 1 通りだった
  - **`Ord` は `name` ではなく `lower_name` を見る**ので、完全 tie の 2 件は `name` が違いうる（`fOo` と `foO`・敵対的調査）。走査で作る索引では `name` が path から導かれるので、`target_path` が同じなら `name` も揃う
  - 実データの検算: 12 クエリについて、PATH マージ（243 件）の有無それぞれで 2 回ずつ、変更前と採る形の top-200 の name + path のハッシュが全組で一致した（敵対的調査）
- **`all_match_indices` の並びは変えない**（`reduce` も `reduce_with` も左右の順序を保つ）。並びに依存する読み手は見つかっていないが、変える理由も無い
- `snotra-core/CLAUDE.md`「実装前チェック」により、top-k の更新ロジックを変えるなら、入力順不変のテストを追加または更新する必要がある。分割が起きる規模で、スレッド数を変えても入力順を逆にしても結果が一致するテストを足す
- worker 1 本の計器（`measure_search_allocations`）では、この変更はほとんど動かない。全スレッドの列が要る

## 敵対的調査（3b）の採否

出所は `workspace/adversarial-1280.txt`。worktree HEAD は `0e97509` で、実 `index.bin` の mtime は前後で変わっていない。

| 争点 | 判定 | 採否 |
|---|---|---|
| 1. top-k の結果は不変 | 中核は壊せなかった（実データ 12 クエリのハッシュ一致）。付随する 2 文は崩れた | 採る。「完全 tie は変更前から非決定的」を「依存しうる（観測なし）」へ弱めた。「name がバイト一致」は前提（走査由来の索引）つきへ直した |
| 2. 退行の原因は最終容量 | 壊れた（`shrink_to_fit` の版で退行が残る）・⚠️ 純粋な対照ではない | 採る。機序を書くのをやめ、「join で `reserve_exact` すれば消える」という実測だけを残した。容量のテストは機序の検知器ではなく「`reserve_exact` を外す変異」の検知器として位置づける |
| 3. broadcast の合計 | 壊せなかった（全スレッド共通の `AtomicUsize` と比べ、差は broadcast 自身の 1 回だけ） | 採る |
| 4. 閾値 32 | 部分的に壊れた（2 スレッドが負荷下で見逃す） | 採る。検知器を 4 スレッドに固定する |
| 5. 葉の予約 | 壊せなかった | 採る。懸念（一致の少ない葉でも 4 KiB 取る・join で左辺に余りがあると容量が長さを超える）は、容量のテストの閾値 1.25 倍が許す範囲として受容する |
| 測定環境 | 実 config は `include_path_env = true`。`spike_all_threads` は PATH マージを通っていない。`search_frame_cost` は再利用の枝 × Fuzzy | 採る。確保の表には「PATH マージなし」を明記する（PATH マージありでも `c` 756 → 503 で結論は同じ）。壁時計は Substring の全件走査と再利用、実運用点で取り直す |
| 6. その他 | 関連シンボルと既存テストの射程は合っていた。完全 tie のテスト 2 本が一覧から漏れていた | 採る。一覧へ足した |

## 未解決の疑問

- 実運用点（PATH マージあり・16 スレッド）の壁時計 → 実装後に `path_query_cost` で A/B する
- `TopK::new` を最初の `push` まで遅らせる案は、一致 0 件の葉でしか減らない（実データの 1〜2 文字のクエリでは、ほぼ全部の葉が一致を持つ）。成長させれば 1 葉あたり約 8 回に増える。採らない（測っていない）
- 全一致の添字をビット集合（`AtomicU64` の列）で集める案は、確保が葉に依らなくなる。ただし `fetch_or` がホットループに入り、`IncrementalCache` の表現も変わる。採らない（測っていない）
