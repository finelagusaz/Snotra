# #1280 計画

## 目的と受け入れ条件

検索の rayon の葉ごとに乗る確保を減らし、それを全スレッドで数える計器と検知器を置く。

- 全スレッドの計数（既定プール・実 `index.bin`）で、全件走査になる打鍵の確保回数が減る（目安: `c` 754 → 約 500）
  - 総数は実行ごとに揺れるので、厳密一致では判定しない
- 検索結果は 1 件も変わらない（順序込み）
- 壁時計は退行しない。A/B は次の 3 つで取る
  - Substring の全件走査と再利用の枝: 使い捨ての計器で取る。敵対的調査の `adv_wall.rs` を scratchpad から一時的に置き、**コミットしない**
  - `search_frame_cost`: 再利用の枝 × Fuzzy
  - `path_query_cost` の実運用点: パスクエリ × PATH マージあり。`local_matches` を集めないので、reduce の identity の分だけを見る
- 葉に比例する確保の退行を CI で捕まえる検知器が、変更前の実装で赤になる

## 変更ファイルと対象シンボル

1. `snotra-core/src/search.rs` — `SearchEngine::search_with_options` の fold / reduce
   - fold の最初の一致で `local_matches.reserve_exact(MIN_PAR_CANDIDATES)` を呼ぶ（`collect_candidates` の内側）
   - `.reduce(identity, op)` → `.reduce_with(op).unwrap_or_else(|| (TopK::new(max_results), Vec::new()))`
   - op の中で、`extend` の直前に `a_matches.reserve_exact(b_matches.len())` を呼ぶ
   - コメントで次を述べる。3 つは組であり、join の `reserve_exact` を外すと Substring の 100k で壁時計が退行し、最終容量が 2 の冪へ膨らむこと（額は PERFORMANCE.md の節を指す）
     - **機序は書かない。** 最終容量だけを縮めても退行は消えなかった（敵対的調査）ので、「容量が原因」とは書けない
2. `snotra-core/tests/search_allocations.rs`
   - 全スレッド計数の補助 `allocations_on_all_workers(pool, f)` を足す
     - 各 worker のスレッドローカル計数を `ThreadPool::broadcast` で読み、呼び出し側のスレッドの分も足す
     - broadcast 自身の確保は空打ちの差で差し引く
   - 検知器 `leaf_allocations_do_not_scale_with_leaf_growth`（名前は実装時に決める・CI）を足す
     - 合成 n = 8,000（全件一致）で、4 スレッドのプールと 1 本の差を取り、閾値 32 未満であることを見る
     - 閾値の根拠は試作の値（採る形 14〜20・負荷下の最悪 29 / 変更前 74〜77）
     - **スレッド数は 4 に固定する。** 2 スレッドでは、負荷下で変更前の差が 26〜29 に落ち、閾値を下回った（敵対的調査）
   - 計器 `measure_search_allocations` に、既定プールの全スレッドの列を足す
   - `//!` の「1 本で数えてよいのは〜」の段落を、全スレッドの検知器と両立する形へ直す
3. `snotra-core/src/search/tests/ranking.rs`（または `incremental.rs`）— 分割規模の入力順不変テスト
   - `MIN_PAR_CANDIDATES` の数倍の規模で、1 スレッドと複数スレッドのプール、正順と逆順の入力の組み合わせで、検索結果（name と path の列）が一致することを見る
   - Fuzzy で tie を含む fixture にする
4. `snotra-core/src/search/tests/incremental.rs` — 容量のテスト
   - 4 スレッドのプールで分割が起きる規模（`MIN_PAR_CANDIDATES` の 8 倍・全件一致）を検索し、`incremental_cache.prev_candidates` の `capacity()` が `len()` の 1.25 倍未満であることを見る
   - 試作の実測（100k）は、変更前 1.002 倍・`reduce_with` のみ 1.79 倍・採る形 1.000 倍
   - これは壁時計の機序の検知器ではない。壁時計の退行とともに現れる「join の `reserve_exact` を外す変異」を、CI で捕まえるための観測点である（doc にそう書く）
   - 常駐にも意味がある。`prev_candidates` は次の打鍵まで残るので、膨らんだ容量はそのまま常駐になる
   - この規模で「`reserve_exact` を外す変異」が赤になることは、`/implement` の委譲先が変異注入で確かめる。赤にならなければ規模を上げる
5. `PERFORMANCE.md` — 新しい節「採用: 検索の rayon の葉ごとの確保を減らす（#1280）」を #1277 の節の直後に置く
   - 中身は次のとおり
     - 全スレッドの確保の表（PATH マージなしと明記。実 config は `include_path_env = true` なので、実運用点ではない）
     - `reduce_with` 単独の退行。容量 131,072 と `shrink_to_fit` の版の対照も書き、機序は未分離と書く
     - 壁時計の 3 つの A/B
     - 検知器
   - #1277 の節の「扱いは #1280 へ切り出した」は残す（歴史）

## 実装順序

1. 入力順不変テストを足す（変更前で緑であることを確かめる。これは性質の固定であって Red は要らない）
2. 全スレッド計数の補助と検知器を足し、変更前の実装で赤になることを確かめる（Red）
3. `search.rs` の 3 点を入れて緑にする（Green）
4. 計器に全スレッドの列を足し、A/B を取る
5. 壁時計の A/B（`search_frame_cost`・`path_query_cost` の実運用点・交互に各 2 巡以上）
6. 検索結果の A/B（実 `index.bin` で計器の打鍵の並びの結果を A と B で比べ、一致を見る）
7. PERFORMANCE.md に記録する

## 不変条件と異常系

- 候補 0 件では `reduce_with` が None を返し、空の `TopK` と `Vec` へ倒れる → `search_no_entries_returns_empty` が見る
- `collect_candidates` が偽（パスクエリ）では `local_matches` を確保しない → 予約は push の枝の内側に置く
- `all_match_indices` の並びは変わらない（左右の順序を保つ）
- 最終容量が長さを大きく超えない
  - join の `reserve_exact` を外す変異は、確保回数を**減らす**（試作で 500 → 492）ので、確保回数の検知器では原理的に捕まらない
  - 容量そのものを見るテストを置く（変更ファイルの 4）

## テスト方針と検証コマンド

- カテゴリ A（fmt / check / clippy / test / doc）と F（governance:check）。コマンドは `docs/build-commands.md`
- 変異注入は `/implement` の委譲先が行う。候補は次の 4 つ
  - identity 付きの `reduce` へ戻す
  - 遅延予約を外す
  - join の `reserve_exact` を外す
  - 予約を `collect_candidates` の外へ出す

## SPEC.md・関連文書

- `SPEC.md` は更新しない（挙動を変えない）
- `snotra-core/CLAUDE.md` のモジュール構成は、ファイルを増やさないので変えない。`tests/search_allocations.rs` の行の「スレッドローカルで数えてよい理由は `//!`」は、`//!` 側を直せば成り立つ

## 作業項目

### フェーズ 1 — テストと計器

- [ ] 分割規模の入力順不変テストを足し、変更前で緑を確かめる
- [ ] 全スレッド計数の補助を足す
- [ ] 葉の確保の検知器を足し、変更前で赤を確かめる
- [ ] `//!` を直す
- [ ] 容量のテストを足す（変更前で緑・`reduce_with` だけを入れた版で赤になることを確かめる）

### フェーズ 2 — 実装

- [ ] `search.rs` の 3 点と、そのコメント
- [ ] 計器 `measure_search_allocations` に全スレッドの列を足す

### フェーズ 3 — 計測と記録

- [ ] 全スレッドの確保の A/B（実 `index.bin`）
- [ ] 壁時計の A/B（使い捨ての Substring の計器・`search_frame_cost`・`path_query_cost` の実運用点。使い捨ての計器はコミットせず消す）
- [ ] 検索結果の A/B
- [ ] PERFORMANCE.md の節

## 未確定（実装前に潰す）

- [x] 敵対的調査の所見の反映 — 採否は research.md「敵対的調査（3b）の採否」。計画への反映は次の 4 つ
  - 検知器を 4 スレッドに固定した
  - 機序をコメントに書かないことにした
  - 壁時計の A/B に Substring の全件走査と再利用の枝を足した
  - 確保の表に「PATH マージなし」を明記することにした
- [x] 容量の膨張（join の `reserve_exact` を外す変異）を CI で捕まえるか — **捕まえる。** 確保回数の検知器には映らない（変異で回数が減る）ので、`prev_candidates` の容量を crate 内のテストで見る（変更ファイルの 4）

## セルフレビュー

- リスク: 通常
  - 永続形式・状態遷移・ガバナンスに触れない
  - 並行性に触れる。ただし rayon の併合の形を変えるだけで、共有状態は足さない
- plan-review: 未実施（通常リスク）
- エージェント数: 1（3b の敵対的調査）
- 5a の照合
  1. issue の要件（全スレッドで減る・結果不変・計器）には、作業項目がそれぞれ対応する
  2. 境界条件と、それを見る検証は次のとおり
     - 候補 0 件 → `search_no_entries_returns_empty`
     - パスクエリ → 予約が `collect_candidates` の内側にあり、変異注入で見る
     - 分割規模 → 入力順不変テスト
     - 完全 tie → 既存 2 本
  3. 新しいリソース: 無し（`Vec` の予約の仕方だけを変える）
  4. より単純な形: `reduce_with` だけの版は退行するので不可。join の予約を外す版も不可
  5. 壊してはならない不変条件と、その検知手段
     - 結果 → 入力順不変テストと結果の A/B
     - 葉の確保 → 全スレッドの検知器
     - 容量 → 容量のテスト
- check スキル: `/symmetric-check` と `/race-check` は実装後に `/implement` が走らせる（`AGENTS.md` の表による）
- 要対処: 敵対的調査から 4 件。すべて反映した
- 未検証: 実運用点の壁時計（実装後に測る）

## 人間レビュー

- [x] 承認済み — 2026-10-24 / 問い: "`workspace/plan.md` へ注釈を書き込まれるか、この計画をご承認くださいませ。" / 回答: "OK"
