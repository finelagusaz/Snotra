# 調査: #1281 検索の照合で結果に寄与しない CPU 時間

## issue の要約

#1277 から切り出したもの。確保ではなく CPU 時間の話で、候補は 2 つあり、どちらもまだ測っていない。

1. **`.` を含むクエリで、file name が name と同じエントリを同じ文字列で 2 回照合している**（`score_one_entry` の `has_dot` 枝）。代替案は、`needs_fn_score` に `!v.entry.file_name_is_lower_name` を足すこと
2. **Substring・kana・パスの照合で、候補ごとに `str::find` が Two-Way の前処理をやり直している**。代替案は、`memchr::memmem::Finder` を検索 1 回につき 1 度だけ作ること。`snotra-core` の直接依存に `memchr` を足すかどうかの判断が要る

受け入れ条件は 2 つ。壁時計を同一セッションで交互に 3 回ずつ取り、レンジで読むこと（前例は `PERFORMANCE.md`「打鍵ごとの一時確保（2026-10-24 計測・HAL9000）」）。そして、検索結果が 1 件も変わらないこと。

## 関連ファイル・シンボル（grep で実在を確認済み）

- `snotra-core/src/search/scoring.rs`
  - `SearchEngine::score_one_entry`（378 行）: 照合の本体
    - `has_dot` 枝（441〜464 行）と `needs_fn_score`（449 行）
    - パスの照合 `key.find(pq)`（494 行）
  - `SearchEngine::entry_view`（336 行）: `file_name_is_lower_name` が立っていると、`lower_file_name` として `Some(lower_name)` を返す。返すのは同じ参照なので、バイト一致する
  - `match_score_single_cached`（218 行）: Substring の腕にある `lower_name.find(query)`（233 行）
  - `kana_substring_score`（206 行）: `kana_lower_name.find(kana_query)`
- `snotra-core/src/search/query_plan.rs`
  - `QueryPlan<'a>` と `prepare_query_plan`
  - 照合に使う needle は `norm_query: Cow<'a, str>`、`kana_query: Option<String>`、`path_query: Option<String>` の 3 つ
- `snotra-core/src/search.rs`
  - `search_with_options`（326 行で `prepare_query_plan`、396 行で `score_one_entry`）
  - `IncrementalCache::can_reuse` は `kana_query` と `norm_query` を単調性の判定に読む。照合の手段を替えても、この判定は変わらない
- `snotra-core/src/search/path_store.rs`: `CompactEntry::file_name_is_lower_name`（95 行）と `mark_file_name_is_lower_name`（141 行）。`query.rs` 111 行の同名フィールドは共有判定（`measure_derived_sharing`）の結果の型で、別物である
- 計器
  - `snotra-core/src/search/tests/performance.rs` の `bench_search`、`bench_substring_search_scaling`、`bench_fuzzy_search_scaling`、`bench_fuzzy_path_query_scaling`
  - `snotra-core/tests/path_query_cost.rs` の `measure_path_query_frame_cost_at_operating_point`（実 `index.bin`。パスクエリだけを扱う）
  - `snotra-core/tests/search_allocations.rs`
    - 検知器 `fuzzy_full_scan_does_not_allocate_per_candidate` と `path_query_full_scan_does_not_allocate_per_entry` は、小規模と大規模の**差**で判定する。検索 1 回あたりの定数は判定に効かない
    - 計器 `measure_search_allocations` は確保の回数を表で出す
- フォルダの絞り込み（`folder.rs` 230 行）は `contains` を使っており、検索の照合とは別経路である。この issue の対象外とする

## 候補 1 の等価性（構造で証明できる）

フラグが立っていると `v.lower_file_name == Some(v.lower_name)` で、`match_score_single_cached` は純関数である。したがって `fn_score == name_score` になり、`match (name_score, fn_score)` の各腕は次のように潰れる。

- `(None, None)` → `None`
- `(Some(s), Some(s))` → `Some(s.max(s)) = Some(s)`
- `fn_score` を飛ばした場合: `(Some(s), None)` → `Some(s)`、`(None, None)` → `None`

結果は一致する。bitmask の pre-filter は、フラグが立っていると `fn_mask == name_mask` なので影響を受けない（関数の先頭で評価され、この変更の外にある）。

**モードごとの額**

- Substring: `find` が 2 回走る
- Fuzzy（既定）: pre-filter を通ったフォルダで nucleo の照合が 2 回走る。nucleo のスコアは 9000 を下回るので、短絡は効かない
- Prefix: ヒットすれば 1 回目のスコアが 9000 を超えて短絡する。外れたときだけ `starts_with` が 2 回走るが、これは安い

## 候補 2 の等価性

`str::find(&str)` と `memmem::find` は、どちらも最左のバイト一致位置を返す。needle が妥当な UTF-8 であれば、最左の一致は必ず文字境界に来る（UTF-8 は自己同期するため）。位置ペナルティ（`- idx` / `- pos`）が使う値も一致する。needle が空でないことも構造で保証されている。

- `norm_query`: 空なら `prepare_query_plan` が `None` を返す
- `kana_query`: `to_kana` の結果が `norm_query` と異なるときだけ `Some`
- `path_query`: パス区切りを含むときだけ `Some`

⚠️ `kana_query` が空になる入力は、構造からは排除しきれていない。仮に空でも、`str::find("")` と `memmem` はどちらも `Some(0)` を返すので一致する。

## 実測（この調査で取ったもの）

- **機体は GPDWINMini**（`hostname`）。前例の表は HAL9000 のものなので、この機体のベースラインにはならない
- **実 config**（`%APPDATA%\Snotra\config.toml`）: `normal_mode = "substring"`、`folder_mode = "fuzzy"`、`migemo_enabled = false`
- **実 `index.bin`**: 17,323,918 B
- **フラグの件数**: `entry_view_shared_strings_match_derivation_over_real_index`（release・2026-10-29）で、312,108 件中 `lower_file_name` の共有が 255,578 件（81.9%）。issue の「フォルダのほぼ全件に立つ」と整合する
- **find 単体のマイクロベンチ**（scratchpad。合成 300,000 件・単一スレッド・各 3 回）
  - `memmem::Finder::find` は `str::find` よりおおむね 1.5〜3 倍速かった（例: 平均 106 B・needle `sett` で 29〜37 ms 対 13〜17 ms）
  - GPD のノイズは大きく、20 B・`vis` の 3 回目のように逆転した回もある
  - 位置の総和は全ケースで一致した（`assert_eq!`）
  - **これは照合単体の数字であり、検索全体の壁時計ではない**

## 技術的制約

- **既存の計器は候補 1 を測れない**
  - `make_bench_entries` は `is_folder: false` だけで構成され、name（`Microsoft Studio N`）と file name（`appN.lnk`）が一致しないため、フラグが 1 件も立たない
  - `SCALING_QUERIES` は `.` を含まない
- **実 `index.bin` で、パスでないクエリの壁時計を測るハーネスが無い**
  - `measure_real_index` が数えるのは確保だけである
  - `path_query_cost` はパスクエリしか扱わない
- **`memchr`**: Cargo.lock には 2.8.3 が nucleo-matcher 0.3.1 経由で既に入っている。直接依存として足しても、新しい crate は増えない。`deny.toml` は無い
- **`Finder` の持ち方**
  - `QueryPlan<'a>` が `norm_query` を `Cow` で持っているので、それを借用する `Finder` を同じ struct に入れると自己参照になる
  - **(a)** `Finder::into_owned()` で `'static` にして `QueryPlan` に持つ。確保は検索 1 回につき needle ごとに 1 回で、差分型の検知器には効かない。`measure_search_allocations` の表は +1〜3 だけ動く
  - **(b)** `search_with_options` で `plan` の隣に作り、`score_one_entry` へ渡す。確保は無いが、シグネチャが広がる
- **実データを載せない**: 実 `index.bin` の結果を出力するハーネスに、パスの原文を出してはならない（AGENTS.md「足場」の行）。照合はダイジェストで行う
- **SPEC.md**: 挙動は変えないので更新は要らない。§4 の照合方式の記述は変わらない

## 未解決の疑問

- 候補 1・2 は、それぞれ実 `index.bin` と合成のラダーで壁時計に差を出せるか（交互 3 回のレンジが分離するか）
- Finder の持ち方は (a) と (b) のどちらにするか
- `kana_query` に `Finder` を足す価値はあるか。実運用点では migemo が無効なので、この機体の計器では効果が出ない

## 敵対的調査（3b）の反映

出力は `workspace/adversarial-1281.txt`（sonnet 1 体）。

**壊れた項目**

- **B1（採る・記述を訂正）**: 「`measure_real_index` は確保だけを数える」は誤りだった。`report_measured` が µs の列も出している。ただしその値は計数アロケータ・worker 1 本・標本 1・交互でない条件で取ったもので、壁時計としては読めない。「実 `index.bin` で、パスでないクエリの壁時計を交互に測れる計器が無い」と言い直せば成立する
- **B2（採る）**: incremental の再利用（`IncrementalCache::can_reuse`）を考えていなかった
  - `.` を含むクエリが全件走査になるのは、`.` を初めて打った打鍵か、単調でない編集の後だけである。それ以降の打鍵は前回の候補だけを走査する
  - 候補 2 も、全件に効くのは全件走査になる打鍵だけである
  - 計器では、各標本の前に `RESET_QUERY` で再利用を外すこと。試作ハーネスはそうしてある
  - **額は「全件走査になる打鍵 1 回あたり」として書く**
- **B3（採る）**: 確保の検知器の列挙が欠けていた
  - `split_search_allocations_do_not_grow_with_leaf_matches` と `result_assembly_allocates_once_per_path` が漏れていたが、どちらも差分型である
  - **`Finder` は rayon の葉や fold-init の中で作らず、`prepare_query_plan` で 1 度だけ作る**（葉ごとに作ると前者が赤になりうる）
- **B4（採る・注記）**: 81.9% は `new_with_migemo`（`Measured` の経路）で測った値である。製品は `from_material`（`Collapsed` の経路）と PATH マージを通る。PATH で足したエントリにはフラグが立たない。比率はわずかに下がる方向の差であって、結論は変わらない
- **B5（採る・言い換え）**: `PERFORMANCE.md` 1063 行付近に、GPD WIN MINI でパスクエリを測った前例がある。この機体には、パスでない Substring のクエリを測った前例が無い

**壊せなかった項目**

- P1: 候補 1 はすべてのモードで等価
- P2: 位置は非 ASCII でも空の needle でも一致する。`&str` を needle にした `find` は 3 か所だけである
- P3: 合成ベンチではフラグが立たない
- P4: `memchr` を足してもパッケージは増えない（ci.yml にも deny・audit は無い）
- P5: 差分型の検知器は、検索 1 回あたり定数個の確保なら赤にならない（B3 の範囲内で）
- P6: 機体は GPDWINMini
- 引用した行番号は正しいと判定された。ただし `CompactEntry::file_name_is_lower_name` を `query.rs` 111 行と書いた所在の誤りは見逃された（後に訂正した）。実 config も調査の前提と一致した
- `Engine::search` はフォルダにも `normal_mode` を使う（`folder_mode` はフォルダ展開の中だけで効く）。したがって、この機体の実運用点（Substring）でも候補 1 は発火する

**⚠️ 確信の持てない所見**

- **E1（採る）**: 実運用点では migemo が無効なので、kana の照合は発火しない。パスの照合は、キーの導出がフレームの支配項である中の一部にすぎないとみられる。`kana_query` の `Finder` は、効果を測れる計器がこの機体に無いまま、一貫性のために足すことになる → 計画で判断する
- **E2（採る）**: 既定の Fuzzy では候補 1 の方が効き（nucleo が 2 回走る）、候補 2 はほぼ効かない → 合成ラダーで Fuzzy の側も測る
- **E3（採る）**: `norm_query` の `Finder` は Substring のときだけ作る（`Option`）。Fuzzy や Prefix で使われない確保を打鍵ごとに乗せないためである
- **E4（採る）**: 結果の一致を確かめるときは、PATH マージを通した索引も含める。PATH で足したエントリはフラグが立たないので、それを含む索引でも確かめる（フラグの立たないエントリは PATH 由来に限らず、通常のファイルにも多い。マージなしの実測でも `.exe` が 11 件に当たっている）
- **E5（却下）**: `contains` を第 3 の基準にする案。製品は位置（スコアの `- idx`）を要するので `contains` では足りず、`contains` と `find` の 2 段にすれば一致した候補を 2 回走査することになる。比べる意味が無い
