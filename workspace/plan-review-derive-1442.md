# #1442 の独立導出（plan-review-derive）

対象: `bdf7f0ee`（ブランチ fix/path-cursor-store-generation・作業ツリーは workspace/ 以外無変更）。
workspace/ の plan.md・research.md・adversarial-*.txt は読んでいない。コードは変更していない。

## 0. 欠陥の機序（コードで裏取り）

- `PathCursor`（`snotra-core/src/search/path_store.rs:367-372`）は `buf` と `stack: Vec<(u32, u32)>` だけを持ち、どの `PathStore` で組み立てたかを持たない。
- `PathCursor::normalized`（同 391-431）の分岐は 3 つ: 根（`parent == NO_PARENT`・396-404 で clear）／当たり（`stack` に `parent` の index がある・409-412 で truncate のみ）／外れ（414-427 で clear して根から作り直し）。**当たりの判定は index の数値一致だけ**（407 `rposition(|&(idx, _)| idx == parent)`）なので、別の `PathStore` の同じ番号でも当たる。
- `CURSOR` は `thread_local!`（同 461-466）で、`with_cursor`（453-460）が唯一の入口。呼び手は `scoring::with_normalized_key`（`scoring.rs:140-142`）経由で `score_one_entry`（`scoring.rs:521`）と `recent_history`（`search.rs:473-479`）。
- 索引の差し替えは `Engine::apply_prebuilt_index`（`engine.rs:257-259`）/`replace_entries`（252・test）が `SearchEngine` を丸ごと置き換える。スレッドは生き続ける: 検索は「プロセス寿命の 1 本」の search worker（`src-tauri/src/egui_shell/search_worker.rs:1`・60 行 `engine.search`）と rayon のグローバルプールの worker で走る（次節）。
- `PathStore` の内容は構築後に変わらない: 構築口は `adopt`（`path_store.rs:519-550`。`build` は cfg(test) で `adopt` を通る・558-561）だけ。構築後の `&mut self` は `mark_file_name_is_lower_name`（147-149・カーソルが読まない `file_name_is_lower_name` だけ）・`shrink_to_fit`（162-166・内容不変）・`force_unsorted_for_test`（175-177・cfg(test)・`sorted_prefix_len` のみ）。→「構築ごとに世代 1 つ」で足りる。

### rayon のどのスレッドの thread_local が使われるか（rayon 1.12.0 / rayon-core 1.13.0 のソースで確認）

- `bridge_producer_consumer`（`rayon-1.12.0/src/iter/plumbing/mod.rs:385-435`）の `helper` は、`splitter.try_split` が偽なら**呼び出したスレッドの上で** `producer.fold_with(...)` を走らせる（432 行）。真なら `join_context` へ。
- `LengthSplitter::try_split`（同 329-332）は `len / 2 >= self.min && inner.try_split(..)`。`inner.splits` の初期値は `current_num_threads()`（260-264）。
- 版は `Cargo.lock:4076-4089`（rayon 1.12.0 / rayon-core 1.13.0）で確認済み。
- `join_context`（`rayon-core-1.13.0/src/join/mod.rs:115`）は `registry::in_worker`（同 132 行）を呼ぶ。自由関数 `in_worker`（`registry.rs:946-961`）は、呼び手が**いずれかのプールの worker なら op をその場で**走らせ、そうでなければ `global_registry().in_worker(op)` へ渡す。後者（`Registry::in_worker`・`registry.rs:496-514`）は呼び手が非 worker なので `in_worker_cold`（517-539）でグローバルプールへ inject して待つ（作業は**グローバルプールの worker** で走る）。
- `ThreadPool::install`（`thread_pool/mod.rs:137-143`）は `self.registry.in_worker(|_, _| op())`——op はそのプールの worker 上で走り、その中の `join_context` は自由関数 `in_worker` の「worker ならその場で」に入る。
- 帰結:
  - `search_with_options` は `.with_min_len(MIN_PAR_CANDIDATES)`（`search.rs:384`・`MIN_PAR_CANDIDATES = 512`・`search.rs:42`）。**候補が 1024 未満（incremental の再利用を含む）なら分割されず、呼び手のスレッド（製品では search worker、テストでは libtest のテストスレッド）の `CURSOR` を使う。** 1024 以上ならグローバルプールの worker 群の `CURSOR`（最初の葉は join_context の A 側として worker 上で走る）。
  - `recent_history` は `with_min_len` を持たない（`search.rs:473-475`）ので、`len >= 2` で分割され、呼び手が非 worker なら**常にグローバルプールの worker** で走る。
  - `pool.install(..)` の中では呼び手自身がそのプールの worker になり、1 スレッドのプールなら全部が同じ 1 本の `CURSOR` を使う。
- ⚠️ 既存 doc の「rayon の worker ごとに 1 本」（`path_store.rs:462`）は不正確——小さい走査は非 worker の呼び手スレッドで走る。どちらにせよ寿命はスレッドの寿命で、索引を跨ぐ。

## 1. 変更すべきファイルとシンボル

### `snotra-core/src/search/path_store.rs`

| シンボル | 変更 |
|---|---|
| 新 `static`（例: `NEXT_STORE_GENERATION: AtomicU64 = AtomicU64::new(1)`） | 構築ごとの一意な世代の採番元。`fetch_add(1, Relaxed)`（一意性だけが要る・他メモリとの順序は不要）。**1 から始める**——0 を `PathCursor::new()` の「未使用」の番兵に取っておくため（`new` は `const fn` で `thread_local!` の `const` 初期化に使われる・375-380・464-465） |
| `PathStore` に `generation: u64` フィールド | `adopt`（544-549 の `Self { .. }`）で採番。doc に「内容（`parent`/`aux`/`names`/`table`）が構築後に不変であることが前提。内容を書き換える口を足すなら世代を進める」を書く |
| `PathStore::footprint_rows`（189-226） | 網羅の分配（192-197・`..` を使わない規約）へ `generation: _` を足す（足さないと compile-fail するので漏れない） |
| `PathCursor` に `generation: u64` フィールド | どの `PathStore` で `stack` を組んだか。`new()` は 0 |
| `PathCursor::normalized`（391-431） | 先頭で `if self.generation != store.generation { self.stack.clear(); self.generation = store.generation; }`。`stack` を空にすれば当たりが起きず、根なら根の分岐、そうでなければ外れの分岐で `buf` も clear される（`buf` の clear は不要）。**`with_cursor` 側ではなくここに置く**——単体テストは `cursor.normalized` を直接呼ぶ（`tests/path.rs:286-288`）ので、ここに置かないと単体テストで守れない |
| `PathCursor` の struct doc（355-366） | 「全件走査のあいだ…持ち回る」→ 寿命はスレッドで、索引を跨ぐこと・世代で捨てることを書く |
| `PathCursor::stack` の doc（369-371） | 「`generation` の索引の index である」を足す |
| `PathCursor::normalized` の doc（382-390） | 「チェーンの状態に依らず」の適用範囲に「別の `PathStore` を跨いでも」を足し、新しい回帰テストを名指し |
| `CURSOR` の doc（462-463） | 「rayon の worker ごとに 1 本」→ 呼び手スレッド（候補が少ない走査）と worker のどちらでも、スレッドの寿命で残る。索引を跨いだ持ち越しは世代で捨てる |

`PathStore` は 1 件あたりではなく全体で 8 B 増えるだけ（`CompactEntry` は不変・12 B の主張は崩れない）。

### `snotra-core/src/search/tests/path.rs`

| シンボル | 変更 |
|---|---|
| 新テスト（単体）: 例 `path_cursor_discards_chain_across_stores` | 1 本の `PathCursor` で `normalized(&A, i)` の後に `normalized(&B, j)` を呼び、`normalize_entry_key` と比べる（組み方は §4） |
| 新テスト（結合・issue の再現）: 例 `history_boost_survives_index_swap_on_same_thread` | `pool_of(1)` の**同じプール**で A を検索 → B を検索、新しいスレッド（または別プール）での B 単独の結果と一致させる |
| 既存 `path_store_cursor_matches_full_rebuild`（247-298）の 285 行コメント | 「走査ごとにチェーンを空から始める（カーソルの寿命 = 1 走査）」は**テストの置き方としては今も正しい**が、製品の寿命ではないことを issue が指摘している。「製品ではスレッドの寿命。索引を跨ぐ経路は〇〇テストが受け持つ」に直すと誤読を防げる |

⚠️ 命名: `snotra-core/CLAUDE.md`「`normalize_entry_key` の冪等性契約」が「`path_store_cursor_matches_*` の 2 本が守る」と**本数**を書いている。新テストをこの glob に当たる名前にすると本数が偽になる。glob の外の名前にするか、CLAUDE.md の数え上げをやめて glob だけを指すよう直す（AGENTS.md「数ではなく正本を指す」）。

## 2. 変更で偽になる（または不正確な）記述

| 場所 | 記述 | 状態 |
|---|---|---|
| `snotra-core/src/search/path_store.rs:355-356` | 「全件走査のあいだ、直前に組み立てた祖先のチェーンを持ち回るカーソル」 | 寿命の記述が製品と違う（現状すでに偽）。直す |
| 同 `:362-363` | 「外れたときは根まで辿って作り直すので、**順序が乱れた入力でも結果は変わらない**」 | 同じ索引の中では真。索引を跨ぐ当たりの誤判定を前提に入れていない。適用範囲を書く |
| 同 `:369-371` | `stack` の doc「いま `buf` に載っているチェーン」 | 世代の前提を足す |
| 同 `:382-384` | 「チェーンの状態に依らない素直な組み立てと**必ず同じバイト列**を返す」 | **現状は偽**（バグそのもの）。修正後に真になる。検証テストの名指しに新テストを足す |
| 同 `:386-390` | 検証の名指しが `path_store_cursor_matches_full_rebuild` と実 index 版の 2 本 | 新テストを足す |
| 同 `:462-463` | 「全件走査のカーソル。rayon の worker ごとに 1 本である。…worker ごとにチェーンがウォームアップされる」 | 不正確（§0）。直す |
| 同 `:248-251`（`normalized_into` の doc） | 「チェーンの状態に依らないこの素直な形と 1 バイトも違わないことを…順・逆順・乱順の 3 通りで検証する」 | 検証の範囲に索引を跨ぐ経路が無い。新テストを名指しするなら足す（必須ではない） |
| `snotra-core/src/search/tests/path.rs:285` | 「カーソルの寿命 = 1 走査」 | テストの前提としては可、製品の寿命と誤読される。上記どおり |
| `snotra-core/src/search/tests/path.rs:248-253` | テスト doc「3 通りを通すのが要点…」 | 索引を跨ぐ経路は受け持たないことを書くと良い（任意） |
| `snotra-core/CLAUDE.md`「`normalize_entry_key` の冪等性契約」 | 「一致は `search/tests/path.rs` の `path_store_cursor_matches_*` の **2 本**が守る」 | 新テストの名前次第で偽。§1 の ⚠️ |
| `PERFORMANCE.md:477-480` | 「外れたら作り直すので順序が乱れた入力でも結果は変わらない」 | 歴史記録（採用時点）。同じ索引の中の話として真。**直さない**のが妥当（記録の規約は `PERFORMANCE.md:31`「この文書へ記録するときの規約」） |
| `PERFORMANCE.md:1345-1347` / `docs/adr/ADR-path-query-forward-single-pass.md:83` | 「rayon の chunk 境界ごとに worker の `PathCursor` のチェーンがコールド」という対立仮説 | 偽にはならない。ADR は凍結。直さない |
| `docs/architecture.md:281` | 「`PathCursor` が祖先のチェーンを持ち回り…チェーンが外れたときだけ根まで辿り直す」 | 偽にはならない（世代違いは「外れ」と同じ扱いになる）。直さなくてよい |
| `snotra-core/src/index_tree.rs:22-23, 358` | `PathCursor` は別の走査を持つ | 影響なし |
| ⚠️ 既存の古い記述（本件と無関係） `snotra-core/src/search/scoring.rs:158` | 「索引から外し `KEY_BUF` へ導出する形へ移した」 | `KEY_BUF` は存在しない（`git grep KEY_BUF` は `scoring.rs:158` と `docs/superpowers/specs/2026-08-07-...` だけ）。実体は `CURSOR`。同じ doc を触るついでに直す候補（範囲外にするなら触らない） |

## 3. 同じ欠陥パターンの他の箇所

母集団: `git grep -nE '^\s*(pub(\([a-z]+\))? )?static ' -- '*.rs'`（`static` 宣言そのもの。`thread_local!` の中身も行頭 `static` で拾える）。最初に使った `Atomic|OnceLock|LazyLock` の grep は `static X: Mutex<..>` を落としていた（`INDEX_WRITE_LOCK` が出なかった）ので、こちらで数え直した。

| 箇所 | 持ち越す状態 | 判定 |
|---|---|---|
| `path_store.rs:461-466` `CURSOR` | チェーン（index 列 + buf） | **本件** |
| `path_store.rs:469-470` `CMP_BUFS` | 2 本の `String` | 安全: `raw_into` → `raw_path_into` が毎回 `buf.clear()`（`index_tree.rs:388-399`） |
| `scoring.rs:81-86` `MATCHER`（`FuzzyScratch`） | `Matcher` と `haystack` | 安全: `haystack` は `utf32_haystack` が毎回 `clear`（`scoring.rs:101-109`）。`Matcher` の設定は定数 `matcher_config()`（索引にもクエリにも依存しない） |
| `scoring.rs:144-149` `PATH_MATCH_BUF` | `String` | 安全: `compose_for_path_match` が書くときは `buf.clear()`（`query.rs:58`）、書かないときは入力をそのまま返す（`query.rs:49-56`） |
| `src-tauri/src/egui_shell/window_coordinator.rs:1121-1123` `LAST` | 前フレームの clamp 観測 | 挙動を決めない（trace の行数だけ・doc 1097-1101 に明記） |
| `tests/search_allocations.rs:44-47` | 計数器 | テスト専用 |
| `history.rs:47-48`・`indexer/cache.rs:1102`（`INDEX_WRITE_LOCK: Mutex<()>`）・`indexer/test_support.rs:14`（`INDEX_LOCK_TEST_GUARD`）・`startup.rs:524-525`・`trace.rs:36,48`・`scan_log.rs:217`・`heap_trace.rs:79-83,113`・`font_stack.rs:18,122`・`visual.rs:36`・`input.rs:44`・`tests/memory_footprint.rs:56-59,107`・`tests/search_allocations.rs:76` | 連番・ロック・設定・計数器・アロケータ | 索引の index を持たない |
| `SearchEngine::incremental_cache`（`search.rs:170`・`IncrementalCache`） | 前回候補の index 列 | 安全: `SearchEngine` のフィールドなので差し替えで一緒に捨てられる |

結論: **上の母集団（`.rs` の `static` 宣言）の中で、索引の index を鍵に状態をスレッド/プロセス大域へ持ち越すのは `CURSOR` だけ**。`static` を介さない持ち越し（長寿命の構造体のフィールド等）はこの母集団の外である。
⚠️ src-tauri 側で「索引の index を UI やキャッシュに持ち、差し替えを跨いで使う」箇所は、上の grep（static / thread_local）では拾えない形（構造体フィールド）なので網羅していない。`SearchResult` は index ではなくフルパスを運ぶ（`scoring::result_at`）ので本件の機序とは別。

## 4. 回帰テストの組み方と落とし穴

### 単体（`PathCursor` 直）

```
A = build([C:\aaa, C:\aaa\zzz, C:\aaa\zzz\two.exe])      // idx 0,1,2
B = build([C:\bbb, C:\bbb\abc, C:\bbb\abc\one.exe, C:\bbb\one.exe])  // idx 0..3
let mut cursor = PathCursor::new();
cursor.normalized(&A, 2);                 // stack = [0,1,2]（A の祖先）
assert_eq!(cursor.normalized(&B, 2), normalize_entry_key("C:\\bbb\\abc\\one.exe"));  // 親 1 が「当たる」経路
assert_eq!(cursor.normalized(&B, 3), normalize_entry_key("C:\\bbb\\one.exe"));       // 親 0 が「当たる」経路
```
現状のコードでの値（コードから手で追った）: `B,2` → `c:\aaa\zzz\one.exe`、続く `B,3` → `c:\aaa\one.exe`（同じ葉の後続も誤る＝issue の「根に当たるか外れるまで」）。

- 期待値は `normalize_entry_key(&target_path)` か `B.normalized_into`（参照実装・cfg(test)）のどちらかで作る。**カーソル自身の出力を期待値にしない**。
- 逆向き（B → A）と往復（A → B → A）も 1 本で回すと、「捨てた後に世代を更新し忘れて毎回捨てる」（正しいが遅い）以外の誤り——「世代を更新するが stack を捨てない」等——を両方向で拾える。

### 結合（issue の再現・製品経路）

- `let pool = pool_of(1);`（`tests/common.rs:32-37`）を**1 つだけ作り**、A の検索と B の検索を**同じ `pool` の `install`** で走らせる。`pool_of(1)` を 2 回呼ぶと別スレッドになり再現しない。
- A を `"two"` で検索 → B に `C:\bbb\one.exe` の起動履歴を 3 回（`HistoryStore::empty()` + `record_launch`。実 `%APPDATA%` に触れない・`snotra-core/CLAUDE.md`「開発ルール」）→ B を `"one"` で検索 → 1 位が `C:\bbb\one.exe`。比較対照として、新しいスレッド（`std::thread::spawn` か別の `pool_of(1)`）で B だけを検索した結果と全件一致させる。

### 落とし穴（バグがあっても通ってしまう組み方）

1. **B で最初にカーソルへ渡るエントリが根だと、根の分岐（`path_store.rs:393-404`）でチェーンが空になり再現しない。** 例: B を 0 から順に舐める（既存テストの「順方向」の形）・パスクエリ（`c:\` 等。全候補が `with_normalized_key` を通り、idx 0 の根から始まる）・B に対する `recent_history`（`with_min_len` なしで 0 から舐める）を先に呼ぶ。結合テストは**名前でだけ当たるクエリ**にする（`score_one_entry` は名前・履歴の候補だけキーを組む・`scoring.rs:513-521`）。
2. **B の親の index が A のチェーンに無いと「外れ」で作り直され、再現しない。** A のチェーンは B の親の index を含む深さが要る（上の fixture は A の葉が深さ 2、B の親が 0 と 1）。
3. **A と B の祖先の文字列が同じだと、誤ったチェーンでも出力が一致する。** 同じ `entries` から 2 回 `build` する・根が同じ `C:\aaa` を共有する fixture は無効。同じ index の祖先が**異なる文字列**であること。
4. **A の検索がカーソルに触れていないと、チェーンが空のまま。** A のクエリはどれかのエントリに当たり、そのエントリが非根の深い葉であること（`"two"` は `two.exe` だけに当たる）。A に当たりが無いと `with_normalized_key` は呼ばれない（`scoring.rs:513-515` の早期 return）。
5. **スレッドが変わると再現しない。** 候補 < 1024 の `search` は呼び手スレッドで、`recent_history` はグローバルプールで走る（§0）。プール無しで書くと、`search` 同士なら libtest のテストスレッド上で偶然再現するが、`recent_history` を混ぜるとグローバルプールの worker のどれに乗るかが非決定になる。**1 スレッドのプール 1 つ**に閉じるのが決定的。
6. **fixture を 1024 件以上にすると分割が起きる。** `pool_of(1)` なら分割しても同じ worker だが、葉ごとの走査順が変わり 1. の根リセットが先に起きうる。小さく保つ。
6b. **対照側（新しいスレッドで B だけを検索）も汚さない。** 候補 1024 未満・`recent_history` を呼ばない形に保つ。破るとグローバルプールの worker に出て、他テストが残したチェーンを拾い、対照自体が誤った値になりうる（修正前は「両方誤って一致」で通る恐れ）。対照は期待値を `normalize_entry_key` 由来の固定の期待（「1 位が `C:\bbb\one.exe`」）でも書いておくと、対照の汚染に依存しない。
7. 履歴は B のキーで記録する。誤ったキー（`c:\aaa\one.exe`）に当たる履歴を A 側で偶然作らない（作ると「別のエントリの履歴が当たる」形で別の結果になり、期待の書き方次第で通ってしまう）。
8. ⚠️ ABA（A を drop した後に B が同じアドレスに確保される）を突くテストは、アドレスの再利用が保証されないので決定的には書けない。アドレス比較による修正を弾く検知器にはならない。世代（`AtomicU64`）にする理由は doc とレビューで担保する。
9. Red の確認: 新テストを**修正前のコードで先に走らせて赤**であること、修正後に `normalized` 先頭の世代比較だけを外す変異で**再び赤**になることを確かめる（AGENTS.md「値の出所を変更」行の「検知器が落ちることまで確かめる」）。

⚠️ libtest は `--test-threads=1` のとき main スレッドで逐次実行する（と理解しているが本件で実測していない）。その場合、**既存テスト同士でも** 候補 < 1024 の `search` が前のテストのチェーンを引き継ぎうる。現在 CI が緑なのは、各テストの最初のキーが根であるか外れるかに依存している可能性がある（未検証）。`recent_history` を使う既存テスト（`engine.rs:423` 等）は既定の並列実行でもグローバルプールの worker を他テストと共有しており、同じ持ち越しを受けうる（未検証・フレーキーの温床候補）。

## 5. 守るべき不変条件と検知手段

| 不変条件 | 検知 |
|---|---|
| I1: `PathCursor::normalized(store, i)` は、カーソルがそれまでにどの `PathStore` を見てきたかに依らず `normalize_entry_key(target_path_i)` と一致する | 新しい単体テスト（§4）＋既存 `path_store_cursor_matches_full_rebuild`（同一 store 内の順・逆・乱順）＋実 index 版 `path_store_cursor_matches_normalize_entry_key_over_real_index`（自動スキップあり） |
| I2: `stack` の index はすべて `self.generation` の store のもの | I1 のテストが間接に守る。直接の assert は置かない（debug_assert にするなら `normalized` 冒頭で世代一致後に stack の各 index < `store.len()` だけは安く確かめられるが、別 store の同じ件数は捕まえられない） |
| I3: `PathStore` の世代は構築ごとに一意（アドレスに依らない） | `AtomicU64::fetch_add` の構造で保証。テストは「同じ entries から 2 回 build した 2 つの store の世代が異なる」を 1 行で固定できる |
| I4: `PathStore` の内容（`parent`/`aux`/`names`/`table`）は構築後に不変 | **機械的な検知は無い**（private フィールドと口の少なさで守っているだけ）。⚠️ 内容を書き換える `&mut self` を足す変更（例: 索引のその場更新）が来ると世代が嘘になる。`generation` の doc に「内容を変える口を足すなら世代を進める」を書くのが現実的な手段 |
| I5: 世代 0 は「未使用」の番兵で、store には割り当てない | 採番を 1 から始める。`PathCursor::new()` が 0 |
| I6: `with_cursor` の借用は `f` の中に閉じ再入しない（既存） | 既存（`borrow_mut` の二重取得で panic） |

## 6. 性能への影響と測り方

- 増分: `normalized` 1 回につき `u64` 比較 1 回（`store` の 1 フィールド読み。`store.entries[i]` を読む直前ゆえ同じ構造体のキャッシュラインに近い）＋世代違いのときだけ `stack.clear()`。`PathStore` は全体で 8 B 増、`CompactEntry` は不変。常駐メモリの測定値（`PERFORMANCE.md`）は動かないはず。
- キーを組む回数が最大になるのはパスクエリの全件走査（`c:\` 等で全候補が `with_normalized_key` を通る・`scoring.rs:513-521`）と `recent_history`。312,377 件で比較 31 万回 ≒ 数十〜百数十 µs 未満の見込み（⚠️ 推定・未測）で、p50 の 1 万 µs 台（`PERFORMANCE.md:470-480`）に対しノイズ以下と見込む。
- 測り方（コマンドの正本は `docs/build-commands.md:257-263`。ここに写すのは名指しのため）:
  - パスクエリの全走査: `--test path_query_cost`（`at_operating_point` で実運用点の 2×2 も）——**本件で最も効くはず**の経路。このハーネスは製品の層（`Engine::search` / `SearchEngine::recent_history`）を叩く（`tests/path_query_cost.rs` の `//!`「層を混ぜない」・`sample_frame_cost` 483-491・`measure_recent_history_cost` 519）。同ファイルの写し `normalize_into`（53）/`normalize_into_ascii_fast`（68）は `derives_same_bytes_as_normalize_entry_key`（76）の照合専用で、計測は通らない。`recent_history` の全件走査も同ファイルで測れる
  - 計測はテストスレッドから `engine.search` を叩くので、`c:\` 等の全件候補（≥1024）はグローバルプール、少数候補は呼び手スレッドで走る（§0）。世代比較のコストはどちらにも等しく乗る
  - 名前検索の全走査: `bench_`（`PERFORMANCE.md` の検索計測）・`--test match_cost`
  - 確保が増えないこと: `--test search_allocations`（世代違いの `stack.clear()` は確保しない。容量は残る）
  - 受け入れ基準は `PERFORMANCE.md`「計測と受け入れ基準」（3098 行の節）。冒頭の「計測コードが測る分岐と、変更が触る分岐が同じか先に確かめる」が本件に当たる——上の通り `path_query_cost` は `CURSOR` を通る分岐を測る。A/B は変更前後で交互に、p50 を比べる（**A 側の標本は実装前にしか取れない**・同節）。
- 記録: `PERFORMANCE.md`「この文書へ記録するときの規約」（31-44 行）は「記録するなら出所（日付・機体・規模・版・標本数）を添える」と「引用されうる値はここへ着地させる」を定めるが、正しさの修正に記録そのものを義務づける条項は無い。測った A/B を PR 本文や doc で引用するなら、出所付きで PERFORMANCE.md に着地させる（引用しないなら新節は不要）。
- `size_of::<PathStore>` / `SearchEngine` の大きさを固定する検知器は無い（`git grep -n 'size_of::<' snotra-core` に該当なし）。`PathStore` の 8 B 増で落ちるテストは無い見込み。
- 検証カテゴリ: `.rs` の変更なのでカテゴリ A（`docs/build-commands.md` の該当フェンス・`npm run verify:tree`）。doc コメントを変えるので `cargo doc` 行（`.claude/rules/comments.md`）。`snotra-core/CLAUDE.md` を触るなら `npm run governance:check`。
