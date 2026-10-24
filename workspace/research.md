# 調査: #1277 検索 1 回あたりの確保の帰属

## issue の要約

- 本文: 実 `index.bin`（22,417 件・Substring・`result_limit = 200`・migemo 有効・PATH マージなし）で、検索 1 回の確保が 324〜1,053 回ある。**大半は結果の組み立て**（`heap_into_results` の `PathStore::to_path` が `String::new()` から倍々に伸びる）と推定し、「200 件 × 約 3.6 ≈ 720 が `c` の 723 と合う」とする（未実測）。受け入れは `tests/search_allocations.rs` の `measure_search_allocations`（worker 1 本）の確保回数
- 追記コメント（#1278 の `/simplify` で見送った既存の固定費・未実測）: (1) `reduce` に identity を渡す (2) `.` を含むクエリで file name が name と同一のとき 2 回照合する (3) `str::find` が候補ごとに Two-Way の前処理をやり直す (4) reduce の木で一致 index を段ごとに写し直す
- ユーザーの指示（2026-10-24）: 「本文とコメントを分析して根っこの原因突き止めるところからいこうか」

## 一次証拠: 確保の帰属（使い捨てプローブ・2026-10-24・HAL9000・release）

計器: 確保 1 回ごとに `std::backtrace::Backtrace::force_capture` を取り、確保関数より上の最初の 3 フレームで集計した（`snotra-core/tests/zz_probe_alloc_attrib.rs`・**未追跡・この調査の後に消す**。release に `CARGO_PROFILE_RELEASE_STRIP=false` / `DEBUG=line-tables-only` を足しただけで、計器の総数は issue 本文の表と**全行一致**した: 723 / 975 / 942 / 837 / 1,053 / 740 / 324）。

前提の照合: 実 `%APPDATA%\Snotra\config.toml` は `normal_mode = "substring"` / `result_limit = 200` / `migemo_enabled = true`、`index.bin` は 22,417 件・`cache_hit = true`。issue の前提と一致する。

### worker 1 本（issue の計器と同じ条件）

| query | 計 | パス組み立て（`raw_path_into` + `push_separator` の成長） | `Engine::search` に inline された分（`name.to_string()` ×件数 + 結果 `Vec` 等） | fold の `local_matches` 成長 | reduce の `extend` | `TopK::new` | wana_kana |
|---|---:|---:|---:|---:|---:|---:|---:|
| `c` | 723 | 276 + 214 = **490**（2.45 回/件） | 203 | 23 | 3 | 4 | 0 |
| `co` | 975 | 510 + 233 = **743**（3.7 回/件） | 203 | 18 | 3 | 4 | 4 |
| `s` | 1,053 | 622 + 198 = **820**（4.1 回/件） | 203 | 23 | 3 | 4 | 0 |
| `c:\` | 740 | 331 + 196 = **527**（2.6 回/件） | 205 | 0 | 0 | 4 | 4 |
| `c:\users\` | 324 | 105 + 101 = **206**（2.0 回/件・103 件） | 108 | 0 | 0 | 4 | 6 |

### 既定プール（16 スレッド・製品と同じ）

各クエリ 3 回、毎回同じ前回状態から打った。**3 回とも内訳が同一**（`c` だけ総数 1,038〜1,042 で、差は `PathCursor` の正規化バッファの初回成長）。

| query | 計 | パス | inline 分 | `local_matches` | reduce `extend` | `TopK::new`（fold 識別 + reduce 識別） |
|---|---:|---:|---:|---:|---:|---:|
| `c` | 1,038 | 490 | 203 | **221** | 31 + 28 = **59** | 32 + 32 = **64** |
| `co` | 1,216 | 743 | 203 | 145 | 57 | 64 |
| `cod` | 942 | 716 | 203 | 12 | 3 | 4 |
| `code` | 988 | 635 | 180 | 52 | 53 | 64 |
| `s` | 1,373 | 820 | 203 | 227 | 59 | 64 |
| `c:\` | 800 | 527 | 205 | 0 | 0 | 64 |
| `c:\users\` | 384 | 206 | 108 | 0 | 0 | 64 |

## 判定（根っこの原因）

1. **確保は 3 つの層に分かれる。** (a) 結果 1 件ごと（パス + 表示名）、(b) rayon の葉 1 つごと（`TopK::new`・`local_matches` の成長・reduce の `extend`）、(c) 検索 1 回ごとの定数（wana_kana・結果 `Vec`）。**本文の推定は (a) のうちパスだけを見ており、計器が worker 1 本であるため (b) がほぼ見えていなかった**
2. **計器のスレッド数が内訳を変える。** `c` の 723 → 1,038（+315）は (b) の 3 行の差（`local_matches` +198・`extend` +56・`TopK::new` +60 = 314）でほぼ全額説明でき、残差 1 は `PathCursor` の初回成長である。葉の数は worker 1 本で 2、16 スレッドで 32 だった。**worker 1 本の計器は葉比例の確保を原理的に 1/16 に縮めて見せる**——追記 1・4 の形の修正は、issue の受け入れ基準の計器ではほとんど動かない
3. **本文の「200 × 3.6 ≈ 720 で 723 と合う」は偶然の一致である。** 実測ではパスは `c` で 490（2.45 回/件）、表示名の `to_string` が別に約 200 回ある。**パスの成長回数はパスの長さと区切りの数で決まり、クエリごとに 2.0〜4.1 回/件と動く**。長さちょうどの 1 回にすれば、消えるのは (a) のうち「1 回/件を超える分」: `c` で約 290・`co` で約 540・`s` で約 620・`c:\` で約 330
4. **表示名の 1 回/件は `SearchResult.name: String` の型が決めている**（`ui_types.rs`）。パスを直しても残り、`c` の下限は 200（名前）+ 200（パス）+ 定数になる。消すなら型の変更であり、results ウィンドウ側の `Arc<[SearchResult]>`（#1278）まで波及するので本 issue の射程外
5. **葉の税（b）が乗るのは、候補が `2 × MIN_PAR_CANDIDATES`（1,024）を超えて分割が起きる打鍵だけである。** `cod` は incremental の再利用で候補が少なく、16 スレッドでも葉が 2 つだった。乗るのは全件走査になる打鍵である: 1 打鍵目（`c`・`s`）、パスクエリ（毎打鍵）、**migemo の `kana_query` が None → Some に転んで再利用が外れた打鍵**（`co`・`code`。機序は下の E1）
6. **(b) の内訳の最大は `local_matches` の成長**（`c` で 221 回・196 KiB）であって、追記 1 が名指した `TopK::new`（64 回）ではない。前者は incremental の前回候補を葉ごとの `Vec` で集めて連結する形の帰結で、追記 4（段ごとの写し直し）と同じ仕組みの別の面である。**パスクエリでは 0**（`IncrementalCache::caches_candidates` が収集しない・#1070）
7. **追記 2・3 は確保ではなく CPU の話であり、この計器にも issue の計器にも現れない**（`.` を含むクエリの 2 重照合・`str::find` の前処理）。扱うなら受け入れは交互計測になり、本文の「確保回数で判定」と別の物差しを要する

## 敵対的調査（3b）の反映

所見の全文は `workspace/adversarial-1277.txt`。採るのは所見であり、機序は一次資料で裁定した。

| 所見 | 判定 | 採否と理由 |
|---|---|---|
| E1: `co`・`code` に葉の税が乗るのは「候補が広い 2 打鍵目」だからではない。migemo の `kana_query` が None → Some に転ぶと `can_reuse` が偽になり、全件走査へ倒れるためである | 壊せた | **採る。** `query_plan.rs:77-90`（`migemo_min_chars = 2` ゆえ `c` は None、`co` → `こ`・`cod` → `こd` は ASCII が残って None・`code` → `こで`）と `IncrementalCache::can_reuse` の `(Some(_), None) => false` を自分で読んで確かめた。**判定 5 を訂正する**: 葉の税が乗るのは全件走査になる打鍵であり、migemo 有効の利用者では、かなに変換できる長さに達した打鍵も全件走査になる |
| E2: 実 config は `include_path_env = true` で、製品は PATH マージを通す。パスクエリでは結果集合が変わる（`c:\users\` 103 → 200 件・パス確保 206 → 259）。PATH エントリは根なので 1 回/件で済む | 壊せた | **採る**（`config.toml:69` を自分で確かめた）。判定 3 の「消える量」は PATH マージなしの索引での値であり、パスクエリでは製品のほうが少ない。非パスクエリの総数はほぼ変わらない（`c` 1,037〜1,045） |
| E3: 回数では `local_matches` が最大だが、バイト数では `TopK::new`（約 617 KiB）と reduce の `extend`（約 440 KiB）のほうが大きい | ⚠️ | **採る。** 判定 6 の「最大」は回数で書いたと明記する。バイト数は 1 回あたりの単価が違う（`TopK::new` は 201 × 48 B = 9,648 B） |
| 命題 1 の「残差 1」は再現しない（`c` の総数は 1,031〜1,051 と揺れる） | ⚠️ | **採る。** `PathCursor` の正規化バッファの初回成長が実行ごとに違う。**受け入れを総数の厳密一致で書かない**（E7 も同旨） |
| 命題 3: 現在のコードでは `CMP_BUFS` を借りても二重借用は起きない（`into_sorted_vec` が先に走り切る） | ⚠️ | **採る。** 流用しない理由は「いま壊れる」ではなく「tie-break 専用のセルを別の目的と共有すると、将来の変更で二重借用になりうる」に弱める。関数ローカルで足りることは変わらない |
| 命題 5 の「1/16」は個別には 1/10〜1/20 | ⚠️ | **採る。**「1/10〜1/20」へ書き換える。「ほとんど動かない」は維持（`reduce_with` にしても worker 1 本の `c` は数回しか減らない） |
| 命題 6: 全順序は `target_path` の一意性が前提で、PATH マージ後の一意性は未実測 | ⚠️ | 未確定として残す。追記 1 を射程に入れるときだけ要る |
| E9: Windows ヒープの realloc はその場で伸びうるので、確保回数は実コストを過大に見せる可能性がある | ⚠️ | 記録する。受け入れは確保回数のままで、額は交互計測でしか言えない |
| 壊せなかった: 命題 2（`raw_path_into` は buf を clear する・`to_owned` は長さちょうど 1 回）・命題 4（inline 分の中身は結果 `Vec` 1 個 + 件数ぶんの小確保）・命題 7・E4（`name: String`）・E5（`to_path` は製品 2 + テスト 2）・E6（パスクエリで `local_matches` が 0）・環境の照合（config・`index.bin`・検索は `search_worker.rs` の専用スレッドからグローバルプールへ入る・他スレッドを塞いでも葉の数は変わらない） | 壊せなかった | 本文どおり維持 |

## 関連ファイル・シンボル（grep で実在を確認済み）

- `snotra-core/src/search/scoring.rs`: `heap_into_results`（結果組み立て）・`TopK::{new, push, merge, into_results}`
- `snotra-core/src/search/path_store.rs`: `PathStore::{raw_into, to_path, cmp_paths}`・`CMP_BUFS`（tie-break 用のスレッドローカル 2 本）
- `snotra-core/src/index_tree.rs`: `raw_path_into`（組み立て規則の唯一の正本）・`push_separator`・`walk_to_root`（鎖は固定長配列 `[u32; CHAIN_CAP]` で確保しない）・`TreeNodes`
- `snotra-core/src/search.rs`: `search_with_options`（fold / reduce・`MIN_PAR_CANDIDATES = 512`）・`recent_history`（`to_path` の 2 つ目の製品呼び出し点・`search.rs:465`）
- `to_path` の呼び出し点: 製品 2（`scoring.rs` の `heap_into_results`・`search.rs` の `recent_history`）+ テスト 2（`search/tests/ranking.rs:71-72`）
- 計器: `snotra-core/tests/search_allocations.rs`（`measure_search_allocations`・`on_single_worker`・CI の検知器 2 本）
- 記録先: `PERFORMANCE.md`「打鍵ごとの一時確保（2026-10-24 計測・HAL9000）」

## 再利用できる既存パターン

- バッファを使い回して組み立てる: `PathStore::cmp_paths` の `CMP_BUFS`（スレッドローカル）・`PathCursor`。ただし `CMP_BUFS` は tie-break 専用で、fold の中から借りられる——**結果組み立てで同じセルを借りると、将来組み立ての中で比較が走ったとき `RefCell` の二重借用になる**ので流用しない
- `heap_into_results` も `recent_history` の組み立ても reduce の**後**、呼び出しスレッド 1 本で走る。ゆえに関数ローカルの `String` 1 本で足り、スレッドローカルは要らない（スレッドローカルが消すのは関数ごとのバッファ成長 ≈ 数回だけで、常駐の対価を払う）
- 決定的な確保の計数: `tests/search_allocations.rs` の計数アロケータ。CI の検知器は「規模の 2 点で比べる」形

## 技術的制約

- **組み立て規則は `raw_path_into` が唯一持つ**（`TreeNodes` で索引とディスクが共有）。長さを先に数えて `with_capacity` する形は規則の写しになるので採らない（issue も同じ懸念を挙げている）。バッファ使い回し + `to_owned` は規則に触れない
- `to_path` の製品呼び出し点がすべて置き換わると、`-D warnings` 下で `dead_code` になる。新しい形の導入と呼び出し点の移行は 1 タスクに束ねる（`AGENTS.md`「条件別チェック」）
- 追記 1 を射程に入れるなら top-k の併合規則に触るので、`snotra-core/CLAUDE.md`「実装前チェック」の「入力順を変えても結果が不変であるテスト」が発火する。`TopK::merge` は `other` の各要素を `push` するだけで、`push` は「満杯なら worst より Less のときだけ置換」——併合の結果が葉の順に依らないかは `ScoredEntry::Ord` が全順序であることに拠る（tie-break の最後が `cmp_paths`）
- 計器の CI 検知器は Fuzzy の合成索引で「規模に比例するか」だけを見ており、結果 1 件ごと・葉 1 つごとの確保の数は見ていない。`MAX_RESULTS = 10` なので (a) の改善は検知器の閾値の内側に収まる

## 未解決の疑問

- **射程**: 本文（a のパス）だけか、(b) の葉の税まで含めるか、(b) の最大項（`local_matches`）まで踏み込むか。ユーザーに分析を見せてから決める
- **受け入れの計器**: (b) を射程に入れるなら、worker 1 本の `measure_search_allocations` では効果が 1/16 に縮む。スレッド数を揃えた行を計器へ足すか（計数がスレッドローカルなので、全スレッドを数える計数器が別に要る）、別の受け入れを決める必要がある
- 本文の「10 µs 前後」の額は 1 回 30 ns からの外挿で、未実測のまま。受け入れは確保回数なので額は記録の付記に留める
- (a) のパス組み立てを直したあと、CI で退行を捕まえる検知器を置くか（結果 1 件あたりの確保が 2 回を超えたら赤、のような形）。置かなければ退行は計器の手動実行でしか見えない
