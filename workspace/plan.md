# 計画: #1277 結果の組み立てで、パスを長さちょうど 1 回で確保する

射程はユーザーの裁定（2026-10-24「Cでいこう」）で**本文の (a) だけ**。葉の数に比例する確保 (b) は #1280 へ、追記 2・3（CPU 時間）は #1281 へ切り出した。根拠の内訳は `workspace/research.md`。

## 目的と受け入れ条件

- **目的**: `heap_into_results` と `recent_history` が結果 1 件ごとに `PathStore::to_path`（`String::new()` から倍々に伸びる）で払っている確保を、**使い回しのバッファへ組み立てて長さちょうどで 1 回だけ確保する**形にする。表示名の `to_string`（1 件 1 回）は `SearchResult.name: String` が決めるので残す
- **受け入れ**:
  1. `measure_search_allocations`（worker 1 本・実 `index.bin`）で、各行の確保回数が減る。期待値の目安は「パスの組み立て分が結果件数 + 定数（バッファの成長）になる」——`c` 723 → 約 440、`s` 1,053 → 約 440、`c:\` 740 → 約 420。**総数は ±10 ほど揺れる**（`PathCursor` の初回成長・research.md の 3b 反映）ので、厳密一致ではなく「パスの組み立てが 1 件あたり 1 回 + 定数」で判定する
  2. 検索結果が 1 件も変わらない（既存の検索テスト全部 + 下の等価性テスト）
  3. 新しい CI 検知器が、変更前の実装で赤・変更後で緑になる
  4. 壁時計を同一セッションで交互に 3 回ずつ取って記録する（**ノイズの内側に留まる見込みが高く、合否には使わない**——issue 本文の受け入れどおり）

## 変更ファイルと対象シンボル

| ファイル | シンボル | 変更 |
|---|---|---|
| `snotra-core/src/search/scoring.rs` | `heap_into_results` | 関数ローカルの `String` 1 本を作り、各件で `PathStore::raw_into(&mut buf, i)` → `buf.as_str().to_owned()`。下の共通関数を通す |
| `snotra-core/src/search/scoring.rs` | 新設 `result_at(paths: &PathStore, scratch: &mut String, i: usize) -> SearchResult`（`pub(super)`） | 「索引の index → `SearchResult`」の組み立てを 1 か所に置く。`heap_into_results` と `recent_history` が同じ 5 フィールド（`name`・`path`・`is_folder`・`is_error: false`・`icon: FromPath`）を手書きしているのを束ねる |
| `snotra-core/src/search.rs` | `recent_history` | 末尾の `map` を `scoring::result_at` + 関数ローカルのバッファへ置き換える |
| `snotra-core/src/search/path_store.rs` | `PathStore::to_path` | 製品の呼び出し点が 0 になるので `#[cfg(test)]` にし、doc を「テストの参照用」へ直す（`-D warnings` 下の `dead_code` を避ける）。呼び出し点は `search/tests/ranking.rs:71-72` の 2 つだけ |
| `snotra-core/tests/search_allocations.rs` | 新設の CI 検知器 | 下の「テスト方針」 |
| `PERFORMANCE.md` | 「打鍵ごとの一時確保（2026-10-24 計測・HAL9000）」 | 「採用: 結果の組み立てでパスを長さちょうど 1 回で確保する」の小節を足す（確保の表の前後・内訳・壁時計・検知器） |

**触らない**: `raw_path_into`（組み立て規則の唯一の正本。長さを先に数えて `with_capacity` する形は規則の写しになるので採らない）・`CMP_BUFS`（tie-break 専用のセル。目的の違う借り手と共有しない）・`SearchResult` の型・rayon の fold / reduce（#1280）。

## 実装順序

1. 検知器を先に書き、変更前の実装で赤になることを確かめる（Red）
2. `result_at` を新設し、`heap_into_results` と `recent_history` を同じタスクで移行する。`to_path` を `#[cfg(test)]` へ（Green）
3. `measure_search_allocations` を変更前（`9a19e6e`）と変更後で取り、壁時計を交互に 3 回ずつ取る
4. `PERFORMANCE.md` に記録する。コメントを整える（`heap_into_results` の doc の「K 件ぶんゆえホットパスへの寄与はない」は、確保の額を言っていないので残してよいが、`to_path` を指す文を直す）

## 不変条件と異常系

- **組み立ての結果は `to_path` とバイト一致する**: `raw_path_into` は先頭で `buf.clear()` するので、前の件の残りは混ざらない（3b で確認済み）。`to_owned` は `len` ちょうどの 1 回の確保
- **バッファは関数ローカル**: 両関数とも reduce の後に呼び出しスレッド 1 本で走る（`search.rs` の `top_k.into_results`・`recent_history` の逐次 `map`）。スレッドローカルにしても消えるのは 1 回の検索あたりの成長数回だけで、常駐の対価を払う
- **0 件**: `max_results == 0`・一致 0 件・`recent` が空のときはバッファを作るだけで確保しない（`String::new()` は確保しない）
- 異常系は増えない（I/O・失敗経路を持たない純粋な組み立て）

## テスト方針と検証コマンド

- **CI 検知器（新設・`tests/search_allocations.rs`）**: `fuzzy_full_scan_does_not_allocate_per_candidate` と同じ形で、**同じ索引・同じクエリで `max_results` を 2 点（例: 10 と 40）変え**、確保の増分を結果の増分で割る。閾値は「1 件あたり 2.5 回未満」（表示名 1 + パス 1 = 2 と、変更前の 3 以上の間）。
  - **fixture は親フォルダのエントリを持ち、パスが複数段の組み立てになる形にする**。既存の `C:\apps\x.exe` だけの索引は親が索引に無く、根 1 段で 1 回の確保になってしまう。変更前の実装でこの検知器が赤になることで、fixture が成長を起こす形であると確かめる
  - 名前の通りに「全件が一致し、結果が上限まで埋まる」ことを assert する（既存の `full_scan_allocations` と同じ前提確認）
- **等価性**: 既存の `search/tests/` 全部（`recent_history_*`・ranking・path）。加えて `path_store_raw_matches_target_path_over_real_index`（`#[ignore]`）を手元で走らせる
- **変異注入**は `/implement`「3b. 委譲へ渡すもの」の委譲先が行う（主エージェントの作業項目にしない）。候補: `result_at` を `paths.to_path(i)` 相当（`String::new()` から伸ばす）へ戻す → 検知器が赤になること
- 検証コマンド: `docs/build-commands.md`「変更後の検証チェックリスト」のカテゴリ A（Rust）と F（`PERFORMANCE.md` を触るので `governance:check`）

## SPEC.md・関連文書の更新要否

- `SPEC.md`: **不要**——挙動（検索結果・順序）を変えない。「SPEC に当該挙動の記述があるか」で判定すると、確保の回数は SPEC の語彙に無い
- `snotra-core/CLAUDE.md`: **不要**——ファイルの追加・削除なし。`tests/search_allocations.rs` 行の記述（検知器と計器）は射程が変わらない
- `PERFORMANCE.md`: 要（上の表）
- `tests/search_allocations.rs` の `//!`: 正本を指す「`PERFORMANCE.md`「採用: 検索の候補ごとの確保をやめる」が正本」に、新しい小節を並べる

## 作業項目

### Phase 1 — 検知器（Red）
- [ ] `tests/search_allocations.rs` に、複数段のパスを持つ fixture と、`max_results` の 2 点比較の検知器を足す
- [ ] 変更前の実装で検知器が赤になることを確かめ、増分の実測値をメモする

### Phase 2 — 実装（Green）
- [ ] `scoring.rs` に `result_at` を新設し、`heap_into_results` を関数ローカルのバッファ + `result_at` へ移行する
- [ ] `search.rs` の `recent_history` を同じ形へ移行する
- [ ] `PathStore::to_path` を `#[cfg(test)]` にし、doc を直す
- [ ] LSP の findReferences で `to_path` と `result_at` の参照を列挙し、製品の呼び出し点が想定どおりか確かめる。`/dry-check` を通す
- [ ] 検知器と既存テストが緑になる

### Phase 3 — 測定と記録
- [ ] `measure_search_allocations` を変更前（`9a19e6e`）と変更後で取る（実 `index.bin`・worker 1 本）
- [ ] 壁時計を同一セッションで A/B 交互に 3 回ずつ取る（`tests/search_frame_cost.rs` の `measure_search_frame_cost`）
- [ ] `PERFORMANCE.md` に小節を足す（確保の表・内訳の帰属・PATH マージを通さない計器であることの注記・壁時計・検知器）
- [ ] `tests/search_allocations.rs` の `//!` の正本の指し先を更新する
- [ ] `docs/build-commands.md` のカテゴリ A・F を実行する

## 未確定（実装前に潰す）

- [x] 射程 — ユーザー裁定「Cでいこう」（2026-10-24）。(a) だけを本 issue に、(b) は #1280、追記 2・3 は #1281
- [x] バッファを関数ローカルにするかスレッドローカルにするか — **関数ローカル**。両関数とも reduce の後に呼び出しスレッド 1 本で走り、スレッドローカルで消えるのは検索 1 回あたりの成長数回だけ（research.md「再利用できる既存パターン」）
- [x] 長さを先に数えて `with_capacity` する形と比べるか — **採らない**。組み立て規則の写しを持つことになる（`raw_path_into` が唯一の正本）。使い回し + `to_owned` でも 1 件 1 回に届くので、得るものが無い
- [x] `to_path` を消すか残すか — **`#[cfg(test)]` で残す**。テスト 2 か所が ScoredEntry の index からパスを引くのに使っており、`normalized_into` と同じ「製品は呼ばない参照用」の型に揃う
- [x] 実運用点（PATH マージあり）で結論が変わるか — 非パスクエリの総数はほぼ変わらない。パスクエリは PATH エントリが根で 1 回/件ゆえ削減が小さい（3b の E2）。**計器は PATH マージを通さないまま測り、記録にその旨を書く**（計器の改修は射程外）

## 人間レビュー

- [x] 承認済み — 2026-10-24 / 問い: "`workspace/plan.md` に注釈を書き込んでいただくか、このままで承認かどうかをお知らせくださいませ。" / 回答: "承認、進めて"

## セルフレビュー

- リスク: 通常（永続化・並行性・状態遷移・ガバナンス文書の移動・網羅性要件のどれにも当たらない。rayon の fold / reduce には触らない）
- plan-review: 未実施（通常リスク）
- エージェント数: 1（3b の敵対的調査のみ）
- 照合: (1) issue 本文の要件（確保回数の減少・結果不変・`recent_history` も同じ形・バッファの置き場の決定・`with_capacity` との比較・`PERFORMANCE.md` への記録）はすべて作業項目か未確定欄の裁定に対応する (2) 境界は 0 件・一致 0 件・`recent` 空・1 段のパス（根）・複数段のパスで、前 4 つは既存テスト、最後は新しい検知器が持つ (3) 新しい状態・リソースは関数ローカルの `String` 1 本だけで、関数の終わりに破棄される (4) より単純な既存パターンは `to_path` のままにすることだが、それが確保の原因そのもの (5) 壊してはならない「`to_path` とのバイト一致」は既存の検索テストと `path_store_raw_matches_target_path_over_real_index` が、確保の退行は新しい検知器が見る
- 条件別チェック: 「関数・型を新規定義」→ findReferences + `/dry-check`（作業項目に入れた）。他のトリガー（対称ペア・状態・永続・race・件数 N）には当たらない
- 要対処: 0 件
- 未検証: 新しい検知器の fixture が変更前に赤になるかは、Phase 1 で測るまで分からない（赤にならなければ fixture を直す——作業項目の 2 行目がそれを担う）
