# 計画: issue #1041 明示スキャンの所要を記録し、走査中のフォルダ数を status 行に出す

## 目的

- 索引の構築（`/s`・設定変更・初回構築）ごとに、所要と内訳を `%APPDATA%\Snotra\scan-log.jsonl` へ残す。82.8 秒と 29.3 秒のような標本を、外から挟まずに製品の記録から読めるようにする。
- 構築中の status 行に、走査したフォルダ数と、前回の完走で走査したフォルダ数を出す（「インデックス構築中... 41,000 / 約 98,000 フォルダ」）。

要求の判断はこのセッションでユーザーが下した（`workspace/research.md`「要求の判断」と「要求の判断（追加）」）: 記録は永続 JSONL、進捗表示は本 issue に含める、残り時間は出さない、単位は走査したフォルダ数、分母は記録の直近の完走行から読む。

単位を登録件数からフォルダ数に替えたのは、実機の構成で `C:\` の全再帰が 1 件も登録しないからである（`workspace/research.md`「敵対的調査（3b）の所見と採否」）。

## 受け入れ条件

1. 構築の 1 反復（`drain_index` のループ 1 回）ごとに、走査の前に `start` 行、索引の差し替えの後に `end` 行が `scan-log.jsonl` へ追記される。2 行は `sid` で対になる。
2. 走査の途中でプロセスが終わったら、`start` 行だけが残る（`end` の無い `start` = 未完走）。
3. `end` 行は `scan_ms` / `sort_ms` / `save_ms` / `build_ms` / `unattributed_ms` / `total_ms` / `scanned` / `dirs` / `scan_id` を持つ。`total_ms` は終端で 1 回読んだ値で、恒等式 `total = scan + sort + save + build + unattributed` が生 `Duration` で閉じる。
4. ファイルは 200 行を超えたら、`start` を書く前に末尾 200 行を残して書き直す（古い方を捨てる）。読めない行も 1 行と数える。
5. 書き込みと読み込みの失敗（保存先が引けない・権限・ロック・壊れた行）は握り潰し、構築そのものは変わらない。**記録を読むのは進捗表示の分母だけで、構築のどの分岐も記録を読まない。** 記録が失われたときに起きるのは、分母が出ないことだけである。
6. 構築中、status 行が構築中案内を出しているあいだ（ランチャーが可視で Results ビューのとき）、走査したフォルダ数が 0.5 秒ごとに更新される。
   - 分母は、`scan-log.jsonl` の中で現在と同じ走査対象（`scan_id` が一致）の最後の `end` 行の `dirs` である
   - 分母が無い（記録が無い・壊れている・同じ走査対象の完走が無い）、または走査したフォルダ数が分母を超えたら、フォルダ数だけを出す
   - フォルダ数が 0 のあいだは従来の文言だけを出す
7. 走査のホットループに確保の増加が無い。
8. テストは実 `%APPDATA%\Snotra` に書かない。

## 設計の決定（未解決の疑問への答え）

`workspace/research.md`「未解決の疑問」の番号に対応する。

1. **1 対の単位は drain の 1 反復である。** 区間の内訳（走査・整列・保存・構築）と 1 対 1 に対応する。1 回の要求で 2 反復した場合（ビルド中の設定変更）は 2 対が続けて残る。ユーザーの待ちは 2 対の和になるが、通常は 1 反復で一致する。issue の標本（終端は `index.bin` の mtime = 保存の完了）と比べる区間は `scan + sort + save` であり、`build_ms` を分けて持つのでそのまま比べられる。
2. **契機（`/s`・設定変更・初回）は記録しない。** `/s` は構築中なら `ensure_not_indexing` で捨てられるが、設定変更は走行中のビルドに吸収され、drain の反復は複数の要求をまとめて片づける。帰属の規則を新設するほどの価値が、この issue の問い（所要の分布）には無い。
3. **起動時の cache-miss 経路（`load_or_scan_with_stats`）は対象外。** `AppHandle` 以前に走り、起動計器（`startup.rs`）が既に区間を持つ。待ちの形も違う（起動そのものが遅れる）。
4. **純粋核と書き手は src-tauri の新モジュール `scan_log.rs` に置く。** 呼び出し元は `drain_index`（src-tauri）であり、`build_ms` も src-tauri で測る。serde_json は src-tauri にだけある（core から撤去済み）。時刻は `trace.rs` と同じエポックミリ秒 `ts_ms` で出す（chrono を src-tauri に足さない）。保存先は `Config::config_dir()` から導く。
5. **`rebuild_and_save` の返り値を `(IndexMaterial, RebuildStats)` へ広げる。** 製品の呼び出し元は `drain_index` の 1 つだけなので、`_with_stats` の別関数は足さない（片方が死ぬ）。中身は `scan_and_sort_timed` を通す（区間の意味を cache-miss 分岐と揃え、計測コードを 2 部持たない）。`RebuildStats { scan, sort, save: Duration, scanned, dirs }`。**`save` を `Option` にしない**（計画レビュー R1）——保存先が引けない分岐は `save_cache_sorted` の内側にあり、`rebuild_and_save` からは観測できない。しかもその分岐では `drain_index` も記録を書かないので、`save_ms: null` は記録に載る経路で到達不能である。`save` は `save_cache_sorted` の呼び出しの前後を測った値と定義し、doc に「保存先が引けない分岐ではツリーの構築だけの時間になる（その場合は記録を書かない）」と書く。
6. **CPU 時間（`cpu_ms`）は持たせない。** 当初は構築スレッドの CPU 時間を `GetThreadTimes` で測り、壁時計との差で「待たされた」を読む案だった。しかし snotra-core は `index_tree.rs`（`par_windows` / `into_par_iter`）と `search.rs` で rayon を使い、保存と構築の一部は別スレッドで走る。構築スレッドの CPU 時間はその分を取りこぼすので、「壁時計との差 = 待ち」という読みが成り立たない。走査だけに絞って測るには Win32 の呼び出しを core の `rebuild_and_save` へ入れることになり、得るものに見合わない。**標本ごとの説明変数は `dirs`（仕事量）が担う**——同じ `scan_id` の標本どうしで `scan_ms / dirs` を比べれば、仕事が増えたのか遅くなったのかを分けられる。issue の「1 標本で遅いと判定しない」には、標本を残し続けること（200 行）で応える。
7. **進捗の口は `scan_all` の第 3 引数 `dirs: &AtomicUsize` とする。** `scan_directory_with_extensions` が `read_dir` に成功するたびに `dirs.fetch_add(1, Ordering::Relaxed)` を 1 回撃つ（根も 1 つと数える。`read_dir` に失敗したディレクトリは数えない）。確保は増えない。全呼び出し元を同じ変更で移す。数が要らない呼び出し元はその場の `AtomicUsize::new(0)` を渡す。`RebuildStats.dirs` は走査の後にこのカウンタを読んだ値である。
8. **分母は `scan-log.jsonl` から読む。** `drain_index` の各反復で、剪定の後・`start` 行の前に `last_completed_dirs_in(dir, scan_id)` を呼ぶ。中身は純粋関数 `last_completed_dirs(body, scan_id) -> Option<usize>`: 末尾から見て、JSON として読めて `ev == "end"`・`scan_id` が一致・`dirs` が非負整数の最初の行の `dirs`。`scan_id` は `format!("{:016x}", indexer::scan_identity_hash(&inputs.scan, inputs.show_hidden_system))`（走査対象だけの同一性。`include_path_env` / `migemo_enabled` を含まない）。根を足した・外したときは一致する行が無く、分母は出ない。Rust の版が変わって hash が変わったときも同じで、無害である。
   - **これは計器を表示が読む形である**（撤去前の設計 `2026-08-09-rescan-in-situ-instrument-design.md` §8.2 が却下したのは、計器を構築の振る舞いの入力に兼ねさせる形）。表示の分母は、誤っても欠けても「フォルダ数だけを出す」へ落ちるだけで、構築は変わらない。この区別を `scan_log.rs` の `//!` と `SPEC.md` §13.4 に書く
9. **再描画は、可視フレームで status 行が構築中案内を出したときだけ `request_repaint_after(SCAN_PROGRESS_REPAINT)`（500 ms）を撃つ。これは `src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」の「再要求してよいのは時間経過で解消する不成立だけ」に当たらない、worker が書く値のポーリングであり、例外として理由を doc に書く**（計画レビュー R5）。理由は 3 つ:
   - 間隔は固定の 500 ms で、`ZERO` にならない（永久スピンにならない）
   - 止まる条件は「構築中案内を出さないフレーム」である。構築が終われば `INDEXING_COMPLETE` の wake が来て、そのフレームで止まる。案内は `overlay_kind` で最優先なので、起動中（Launching）や通知に覆われて止まることは無い。Results 以外のビューへ移れば止まり、戻るのはユーザーの入力なのでそのフレームで再開する
   - wake のイベントには値を載せない（`register_config_wake_listeners` の前提を崩さない）。数をイベントで運ぶ形にすると、ビルドスレッドからの emit が数百回に増える
   hidden 中はフレームが走らないので撃たれない。開き直したときは show のフレームが最新を読む。egui の caret 点滅もフォーカス中はフレームを起こすが、それには頼らない（こちらが持たない既定値であり、フォーカスの無い可視状態では止まる）。進捗は `status_row_present` に渡さない（行の有無と高さは進捗に依存せず、show 経路の読みを増やさない）。
10. **進捗は view のフレームで 1 回だけ読む**（`read_scan_progress` → `FrameScanProgress { dirs, expected: Option<usize> }`）。`FrameIndexing` には混ぜない（あちらは起動ゲートへ配る値で、こちらは表示だけ）。`indexing` との読みの間に構築が終わっても、`finish_index_build` が進捗を 0 / 無しに戻すので従来の文言へ落ちるだけである。**進捗は `finish_index_build` で戻し、drain の各反復の先頭で `dirs = 0` → `expected = M` の順に置く。** ビルドの開始時は `finish_index_build` で既に 0 なので、前回のビルドの数は見えない。**1 回の要求で 2 反復する場合だけ、反復の境目で「前反復のフォルダ数と新しい分母」の組が最大 1 フレーム出うる**（2 つの atomic を別々に読むため。受容する。次のフレームで直り、表示の誤りにとどまる）（計画レビュー R6）。

## 変更するファイルと対象シンボル

### snotra-core

- `snotra-core/src/indexer/scan.rs`
  - `scan_all(scan_paths, show_hidden_system, dirs: &AtomicUsize)`: 引数を足す
  - `scan_directory_with_extensions`: `dirs` を受け取り、`read_dir` が `Ok` を返した直後に `dirs.fetch_add(1, Ordering::Relaxed)`。引数は 7 個になるが、束ねる構造体は作らない（再帰で引き回すだけで、束ねた型を読む消費者が無い。clippy の `too_many_arguments` は既定のしきい値 7 を**超えた**ときに立つので、7 個では立たない。既存の `src-tauri/clippy.toml` は `disallowed-methods` だけを設定し、しきい値も lint レベルも変えていない）（計画レビュー M4・差分レビュー M7）
- `src-tauri/src/startup.rs`: `to_ms` を `pub(crate)` にする（doc の「丸めはこの 1 か所」の射程を「この 2 つの計器で」へ直し、`scan_log.rs` と共有することを書く）
- `snotra-core/src/indexer/cache.rs`
  - `scan_and_sort_timed(scan, show_hidden_system, dirs)`: 引数を足す
  - `pub struct RebuildStats { pub scan: Duration, pub sort: Duration, pub save: Duration, pub scanned: usize, pub dirs: usize }`（新設・`#[derive(Debug, Clone)]`）
  - `rebuild_and_save(scan, show_hidden_system, dirs) -> (IndexMaterial, RebuildStats)`: `with_index_write_lock` のクロージャの中で `scan_and_sort_timed` → `compute_config_hash` → `save_cache_sorted` の呼び出しの前後を測る（ロックの範囲は変えない。cache-miss 分岐が同じ形の前例）（決定 5）。`save` は `derive_columns`（派生列の導出）を含み、cache-miss 分岐の `cache_save` と同じ区間であることを doc に書く。`dirs` は走査の後にカウンタを `load` した値。`RebuildStats { scan: .. }` は shorthand を使わない（引数 `scan` と衝突する・#1178）
  - cache-miss 分岐の `scan_and_sort_timed` 呼び出し（2 箇所）: その場の `AtomicUsize::new(0)` を渡す
  - `scan_identity_hash` の doc: 「呼び出し元は #1001 で撤去済みで、この関数は現在どこからも呼ばれない」から「消費者が戻る日のために残す」までの段落を書き直し、消費者（`scan-log.jsonl` の `scan_id`・分母の照合）を書く。`extensions` / `include_folders` を変えただけでも値が変わり、分母が出なくなることを受容として書く（計画レビュー M5）
  - `save_cache_sorted_in` の doc の「`rebuild_and_save` と cache-miss の分岐が実際にそうする」は返り値が組になっても真なので直さない
- `snotra-core/src/indexer.rs`: `RebuildStats` を `rebuild_and_save` と同じ re-export に足す
- 呼び出し元の移行: `snotra-core/src/indexer/scan/tests.rs`（3 件）・`snotra-core/src/search/tests/common.rs`・`snotra-core/tests/memory_footprint.rs`・`src-tauri/src/icon.rs`（`icon_pipeline_cost_probe` の `Err(_) =>` 分岐）
- `snotra-core/tests/memory_footprint.rs` の `report_scan_all_cost` の doc と末尾の `println!`: 「再構築経路の sort / save は cache-miss の起動で `LoadOrScanStats` が測る」を、再構築経路そのものは `scan-log.jsonl` が測る、へ直す（cache-miss の起動の `LoadOrScanStats` も測ることは残す）

### src-tauri

- `src-tauri/src/scan_log.rs`（新設）
  - 純粋核（時計も I/O も持たない）:
    - `ScanRecord { scan: Duration, sort: Duration, save: Duration, build: Duration, scanned: usize, dirs: usize }`
    - `unattributed(rec, total) -> Duration`（飽和減算）
    - `start_line(ts_ms, sid, scan_id, roots, expected: Option<usize>) -> String`
    - `end_line(ts_ms, sid, scan_id, rec, total) -> String`
    - `last_completed_dirs(body: &[u8], scan_id) -> Option<usize>`: **バイト列で受け、`b'\n'` で分割し、行ごとに `serde_json::from_slice` する**（計画レビュー R2）。`read_to_string` / `str::lines` を使うと、非 UTF-8 が 1 バイトあるだけでファイル全体が読めず、分母が永久に出なくなる
    - ms への丸めは `crate::startup::to_ms` を使う（`pub(crate)` にする。切り捨ての規約を 2 つの計器で共有し、写さない）
  - 書き手と読み手（best-effort）:
    - `log_path_in(dir)`（`scan-log.jsonl`）
    - `append_in(dir, line)`: **行末の `\n` まで 1 回の `write_all` で書く。** ファイルが空でなく末尾が `\n` で終わっていない（kill で行の途中が切れた）なら、先頭に `\n` を足して同じ 1 回で書く——さもないと次の行が切れた行に連結され、両方読めなくなる（計画レビュー R3）
    - `prune_in(dir)`（`MAX_LINES = 200`・末尾を残す）: バイト列で読み `b'\n'` で数える（R2 と同じ理由）。書き直しは同じディレクトリの固定名の tmp（`scan-log.jsonl.tmp`）へ書き、**`remove_file` を挟まずに** `std::fs::rename` だけで置き換える（Windows の `rename` は既存の宛先を置き換える）。`binfmt.rs` の `BinFile` の保存手順（`remove_file` → `rename`）を写してはならない——消した直後に終わると全行を失い、「途中終了で全行を失わない」が偽になる（差分レビュー D1）。rename に失敗したら tmp を消して握り潰す（ファイルは剪定されずに伸びるが、次の `start` で再び試みる）
    - `begin_in(dir, ts_ms, sid, scan_id, roots) -> Option<usize>`: 剪定 → 分母の読み → `start` 行の追記を**この順で**行い、分母を返す（計画レビュー M3。順序をソース文字列検査ではなく dir 注入のテストで固定するため、`drain_index` から切り出す）
    - `new_sid()`: pid + エポックミリ秒 + プロセス内の連番（`AtomicU64`）。同じミリ秒の連続反復でも衝突しない（計画レビュー M2）
  - `//!` に、責務・「構築の振る舞いの入力にしない。読むのは表示の分母だけ」・「消えても壊れても書けなくても、構築は変わらない」を書く
- `src-tauri/src/main.rs`: `mod scan_log;`
- `src-tauri/src/indexing.rs`
  - `drain_index`: `begin_index_drain` の後、`Config::config_dir()` を**反復ごとに 1 回だけ**引く（`start` と `end` で同じ値を使う）→ 引ければ `scan_id` を作り `begin_in` で分母を得る（引けなければ分母は無し）→ `state.begin_scan_progress(expected)` → `Instant` の起点 → `rebuild_and_save(&inputs.scan, inputs.show_hidden_system, state.scan_dirs_counter())` → `build_index_from_material` を測る → `complete_index_drain` → 引けていれば `end` 行。`ScanRecord` は名前付きフィールドで組み、`scanned` と `dirs` を取り違えない（実機の確認で両者が違う値になることを見る）
  - `drain_index` の doc / `//!` に、記録の位置と「記録は構築の振る舞いを決めない」を足す
- `src-tauri/src/state.rs`
  - `AppState` に private の `scan_dirs: AtomicUsize` と `scan_expected: AtomicUsize` を足す（計画レビュー R4。`indexing` と同じく書き手を限定する規約に揃える）。口は次の 4 つだけ:
    - `begin_scan_progress(&self, expected: Option<usize>)`: `scan_dirs = 0` → `scan_expected = expected.unwrap_or(0)` の順に、どちらも `Ordering::SeqCst` で書く（書き込み順は `Ordering` を指定しないと成り立たない。既存の `indexing` / `index_build_started` も `SeqCst`。差分レビュー D2）。`scan_progress` の読みと `finish_index_build` の戻しも `SeqCst`。`scan_all` の `fetch_add` だけは単調加算なので `Relaxed` でよい
    - `scan_dirs_counter(&self) -> &AtomicUsize`: `rebuild_and_save` へ渡す（加算するのは `scan_all` だけ）
    - `scan_progress(&self) -> (usize, Option<usize>)`: 読む口。`scan_expected == 0` は `None`（0 フォルダの完走は分母として役に立たないので「無し」と同じに扱う。この判断を doc に書く）。意味をそろえるため、`last_completed_dirs` も `dirs == 0` の `end` 行を飛ばして `None` 側に倒す（差分レビュー M6。テスト表にも足す）
    - `finish_index_build` の中で両方を 0 に戻す
  - doc の「書くのは … だけである」に新しい 2 フィールドの書き手を書く
  - 初期値は唯一の構築点 `AppState::new` に置く
- `src-tauri/src/egui_shell/window_coordinator.rs`: `FrameScanProgress { dirs: usize, expected: Option<usize> }` と `read_scan_progress(app) -> FrameScanProgress`
- `src-tauri/src/egui_shell/launcher_controller.rs`: `scan_progress(&self) -> FrameScanProgress`（`indexing()` と同じく委譲）
- `src-tauri/src/egui_shell/strings.rs`: `indexing_hint(l)` を `indexing_hint(l, progress: FrameScanProgress) -> String` へ置き換える（呼び出し元は `view.rs` の 1 か所）。分岐:
  - `dirs == 0` → 従来の文言
  - `expected == Some(m)` かつ `dirs <= m` → 「インデックス構築中... {dirs} / 約 {m} フォルダ」「Building index... {dirs} / ~{m} folders」
  - それ以外 → 「インデックス構築中... {dirs} フォルダ」「Building index... {dirs} folders」
  - 数は 3 桁区切り（`,`）
- `src-tauri/src/egui_shell/view.rs`
  - `indexing_raw` の隣で `self.controller.scan_progress()` を 1 回だけ読む
  - `OverlayKind::Indexing` の arm で新しい `indexing_hint` を使い、`ctx.request_repaint_after(SCAN_PROGRESS_REPAINT)` を撃つ
  - `SCAN_PROGRESS_REPAINT: Duration = 500 ms`（置き場所は `LAUNCH_TIMEOUT` など `egui_shell` の他の時間定数に合わせる）
  - テスト `scan_progress_is_read_exactly_once_per_frame`（`indexing_is_read_exactly_once_per_frame` と同じ `assert_read_once_in_this_file`）

### 文書

- `SPEC.md` §3.3: 「構築中はメインウィンドウに「インデックス構築中...」メッセージを表示」へ、走査したフォルダ数と前回の完走のフォルダ数の表示を足す（分母が無い・超えたときはフォルダ数だけ）
- `SPEC.md` §13: **§13.4「計器の記録（テキスト・使い捨て）」を新設**し、`scan-log.jsonl` を書く。中身は「消えても壊れても書けなくても構築は変わらない（失われると進捗表示の分母が出ないだけ）」「2 行 1 組で未完走が読める」に絞り、区間名の一覧は書かない。§13.2 には混ぜない（共通保存仕様の全称が崩れる）。後ろの節番号はずれない
- `src-tauri/CLAUDE.md`「モジュール構成」: `scan_log.rs` の行を足す
- `src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」の「再要求してよいのは「時間経過で解消する不成立」だけである」: 決定 9 で偽になるので、例外を 1 つ足す（可視中に worker が書く値を固定間隔で読み直すポーリング。条件が続く限り毎フレーム再要求する〔予約は単一スロットでフレームの到来を約束しないため〕。間隔は残余ではなく固定で、`ZERO` にしない。止まる条件を表示の条件に結びつける。実例は索引構築の進捗）。このファイルは `.claude/rules/safety-nets.md` の `paths` に当たらない（同ファイルのフロントマターで確かめた）
- `docs/architecture.md` の `%APPDATA%\Snotra\` の保存ファイルの列挙: `scan-log.jsonl`（計器の記録・`SPEC.md` §13.4）を足す
- `snotra-core/src/config/location.rs` の `//!` と `config_dir` の doc にある「…のすべてが `Config::config_dir` から派生する」の列挙: `scan-log.jsonl` を足す
- `docs/build-commands.md`「別プロファイルで起動するための env ハッチ（`SNOTRA_CONFIG_DIR`）」の「`config.toml` / … / `window.bin` のすべてがこの 1 点から導かれる」: `scan-log.jsonl` を足す
- `docs/build-commands.md`: 読み方を足す（`Get-Content "$env:APPDATA/Snotra/scan-log.jsonl" -Tail 20`・`start` だけの行 = 未完走・issue の外から挟んだ標本と比べるのは `scan + sort + save`・`unattributed_ms` には engine の錠と `INDEX_WRITE_LOCK` の取得待ちと swap が入る）。置き場所は起動計測の段落の後ろ
- `PERFORMANCE.md`「PATH マージ」の却下の根拠 1（「`rebuild_and_save` は `IndexMaterial` を返し、その `tree()` は…」）: 返り値が組になるので「材料（`IndexMaterial`）を返し」へ直す。同ファイルの `rescan-log.jsonl` の 2 行（背景再スキャン当時の測定記録）は歴史を述べて真のままなので直さない
- `docs/adr/`・`docs/superpowers/` は凍結層なので直さない

## 実装の順序

1. core: `scan_all` のフォルダ数の口と呼び出し元の移行（1 タスクに束ねる）
2. core: `RebuildStats` と `rebuild_and_save` の返り値の拡張（呼び出し元 `drain_index` も同じタスクで直す。ここではまだ記録は書かず、`let (material, _stats)` で受ける）
3. src-tauri: `scan_log.rs` の純粋核・書き手・読み手、そのテスト
4. src-tauri: `drain_index` に記録と分母の読みを結線し、`AppState` の進捗を置く
5. src-tauri: status 行のフォルダ数表示と再描画
6. 文書
7. 検証と実機での確認

## 不変条件と異常系

- **記録は構築の振る舞いを決めない。** 記録を読むのは `begin_in` の分母の読みだけで、その値は `begin_scan_progress` を通って `scan_expected` にしか入らない（`git grep scan_log` と `git grep scan_expected` で確かめる）
- **計器のために製品を落とさない。** `scan_log.rs` に `unwrap` / `expect` / 添字アクセスを置かない（release は `panic = "abort"`）
- **`start` は走査の前に書く。** 順序を逆にすると未完走が読めなくなる
- **`null` と `0` を分ける。** 分母が無い `start` 行の `expected` は `null`
- 行の途中で切れたファイル（kill）への追記は、切れた行と新しい行を分ける（`append_in` が先頭に `\n` を足す）。切れた行は読めない行として飛ばされる
- **恒等式は生 `Duration` で閉じる。** ms へ丸めた値の和は合わなくてよい（丸めは表示境界だけ）
- **剪定は最新を残す。** 剪定は分母の読みより前に行う（剪定で消えた行は分母にも使わない。200 行の中に同じ走査対象の完走が無ければ分母は出ない）
- 保存先が引けない（`Config::config_dir()` が `None`）→ 1 行も書かず、分母も出ない。構築は従来どおり
- 書き込み・読み込みに失敗（権限・ロック・ディスク満杯・壊れた行）→ 握り潰す
- 2 プロセスが同時に剪定する → 行が失われうる（受容。使い捨ての物体）
- ビルドスレッドの panic（unwind ビルド）→ `start` だけ残る（未完走と同じ読み方）。未完走の行は分母に使わない（`end` 行だけを見る）
- 走査の後（整列・保存・構築）の数秒はフォルダ数が止まる（受容。表示は走査したフォルダ数のまま）
- 初回起動で `is_indexing()` が真なのにビルド未開始の期間 → フォルダ数 0 → 従来の文言
- 分母が無い（初回・根の変更・記録の消失）・フォルダ数が分母超え（走査対象のフォルダが増えた）→ フォルダ数だけ
- `wake` は値を運ばないまま（`register_config_wake_listeners` の前提を崩さない）。数を運ぶイベントは足さない

## テスト方針と検証コマンド

### 単体テスト（`scan_log.rs`・dir は一時ディレクトリへ注入）

| テスト | 落とすべき変異 |
|---|---|
| `start_line` が `ev` / `ts_ms` / `sid` / `scan_id` / `roots` / `expected` を持ち、分母が無いとき `expected` は `null` | キーの欠落・`None` を 0 で埋める |
| `end_line` が全区間と `total_ms` / `scanned` / `dirs` / `scan_id` を持つ | キーの欠落 |
| 同じ型のフィールドに互いに違う値を入れ（`sid` ≠ `scan_id`・`scanned` ≠ `dirs`）、キーごとに値を照合する | 取り違えた配線 |
| `end_line` の出力を `last_completed_dirs` に通すと `dirs` が返る（書き手と読み手のキー名の一致） | 片方だけキー名を変える |
| 手で書いた凍結の `end` 行（`end_line` を通さないリテラル）から `last_completed_dirs` が `dirs` を読む（後の版の書き手が出す行ではなく、いまの形を固定する） | 読み手が新しいキーを必須にする変更 |
| 恒等式: 区間の和が `total` を超える入力で `unattributed` が 0 に飽和し、和が下回る入力で差が残余になる | `total` を部分和から作る実装・減算のオーバーフロー |
| `unattributed` が単調でない入力で飽和する | 減算のオーバーフロー |
| 丸め境界: 各 500,000 ns の 2 区間で `*_ms` が 0 + 0、`total_ms` が 1 | 区間ごとに丸めてから足す実装 |
| `prune_in` が末尾 `MAX_LINES` 行を残す（fixture は `MAX_LINES` を跨ぐ） | 先頭を残す実装 |
| 読めない行が混ざっても `prune_in` / `append_in` が続く | parse 失敗で早期 return する実装 |
| 存在しないディレクトリへの `append_in` / `prune_in` / `last_completed_dirs_in` が panic せず、読みは `None` | `unwrap` |
| `last_completed_dirs` が同じ `scan_id` の**最後の** `end` 行を採る（同じ `scan_id` の `end` を 2 つ置く） | 先頭から最初の行を採る実装 |
| `last_completed_dirs` が `start` だけの行・別の `scan_id` の行・読めない行を飛ばす（それらを末尾側に置く） | `ev` / `scan_id` を見ない実装・parse 失敗で `None` を返す実装 |
| `last_completed_dirs` が `dirs: null` の `end` 行と `dirs: 0` の `end` 行を飛ばす | `null` を 0 として採る実装・0 を分母に採る実装 |
| 末尾が `\n` で終わらない最終行を持つファイルを `prune_in` で剪定した後に `append_in` しても、新しい行が独立した 1 行として読める | 剪定が末尾の `\n` の有無を変える実装 |
| `prune_in` の後に tmp ファイルが残らない | tmp の後始末の漏れ |
| 非 UTF-8 のバイトを含む行があっても、`last_completed_dirs` と `prune_in` がほかの行を読む | `read_to_string` で全体を読む実装 |
| 末尾が `\n` で終わらないファイルへ `append_in` すると、新しい行が独立した 1 行として読める | 先頭の `\n` を足さない実装 |
| `begin_in`: 既存の `end` 行を持つファイルで、返り値の分母と、追記された `start` 行の `expected` が一致し、`MAX_LINES` を超えるファイルでは剪定の後に追記されている | 剪定と追記の順序の入れ替え・分母を `start` へ渡し忘れる |
| `new_sid` を続けて 2 回呼ぶと値が違う | 連番を外す実装 |

### 単体テスト（ほか）

- `strings.rs`: `indexing_hint` の 3 分岐（`dirs == 0`・`dirs < m` と境界の `dirs == m`・`dirs > m` と `expected == None`）を ja / en で。3 桁区切り（`1,234,567`）
- `scan.rs`: `scan_all` が数えたフォルダ数が、fixture の中で `read_dir` できるディレクトリの数と一致する（根を 2 つ・ネストしたディレクトリ・`extensions = []` かつ `include_folders = false` の根を含む fixture。最後の 1 つは登録 0 件でもフォルダは数えることを固定する）。変異: `fetch_add` を外すと落ちる／エントリの push の側で数えると落ちる
- `cache.rs`: `rebuild_and_save` は `Config::config_dir()` を内部で解決するのでテストから通さない。`RebuildStats` の `save: None` の分岐は保存の分岐（`save_cache_sorted`）の構造で決まることを doc に書く（テストを置かない理由を明記）
- `state.rs`: `finish_index_build` が `dirs` と `expected` を戻す。`expected()` が 0 を `None` として返す
- `view.rs`: `scan_progress_is_read_exactly_once_per_frame`

### 検証コマンド

`docs/build-commands.md`「変更後の検証チェックリスト」のカテゴリ A（Rust）・C（スラッシュコマンドの経路に触れる）・D（status 行のテキスト表示）・F（`SPEC.md` ほか）を実行する。コマンド文字列はここに写さない。

### 実機での確認（作業項目）

- 走査のホットループ: `tests/memory_footprint.rs` の `report_scan_all_cost`（`#[ignore]`）を変更前後で走らせ、確保回数が変わらないことを記録する
記録の発火は、表示より先に（Phase 2 の末尾で）走る場所で確かめる（issue の「発火しうるかを走る場所で先に測る」。Phase 3 の分母はこの記録に依存する）。

- 完走（Phase 2 の末尾）: 実プロファイルで `/s` → `scan-log.jsonl` に `start` と `end` が 1 対出る。`dirs` が 0 でなく、`scanned` と違う値である（実機の `C:\` の根は登録 0 件で多数のフォルダを走査する）
- 未完走（Phase 2 の末尾）: 実プロファイル（`C:\` の根を持ち、走査が数十秒かかる）で `/s` の数秒後にプロセスを終了 → `start` だけが残る。走査が数秒で終わる検証用プロファイルでは `end` まで進んでしまうので使わない（差分レビュー D3）。続けて起動して `/s` → 新しい `start` の `expected` が、直前の未完走ではなくその前の完走の `dirs` である
- 進捗表示（Phase 3 の後）: `/s` の後にランチャーを開き直し、status 行のフォルダ数が 2 枚のスクリーンショットの間で増えていること、2 回目以降の `/s` で「/ 約 N フォルダ」が出ること、完了後に案内が消えることを目で確かめる。Folder ビューでは案内が出ないことも見る。既定のウィンドウ幅で最長の文言（6 桁 / 6 桁の英語）が status 行に収まることを見る。設定の最小幅（200 px）では既存の文言も収まらないので、窓の端で切れるのは受容する（`painter.text` は clip しない）
- 温度の違う標本（Phase 3 の後）: 2 回以上 `/s` を走らせ、別の行として残ることを確かめる（静穏時と負荷時の 2 標本があれば、`scan_ms / dirs` の違いを記録する）

## SPEC.md・関連文書の更新

- **仕様変更である**（永続ファイルが 1 つ増える・status 行の文言が変わる）。順序は `SPEC.md` → コード → 文書
- 上の「文書」の項目のとおり

## 作業項目

### Phase 1 — core: 走査のフォルダ数と区間

- [ ] `scan_all` / `scan_directory_with_extensions` に `dirs: &AtomicUsize` を足し、全呼び出し元（`icon.rs` を含む）を移す
- [ ] `scan.rs` にフォルダ数を測るテストを足す
- [ ] `report_scan_all_cost` の doc と `println!` の「sort / save の測り手」を `scan-log.jsonl` へ直す
- [ ] `RebuildStats` を新設し、`rebuild_and_save` を `scan_and_sort_timed` 経由にして `(IndexMaterial, RebuildStats)` を返す。`drain_index` を新しい返り値へ合わせる
- [ ] `scan_identity_hash` の doc を消費者に合わせて直す
- [ ] `memory_footprint.rs` の `report_scan_all_cost` を変更前後で走らせ、確保回数を記録する

### Phase 2 — src-tauri: 記録

- [ ] `SPEC.md` §13.4 を新設する
- [ ] `scan_log.rs` の純粋核・書き手・読み手・テストを書く
- [ ] `AppState` に `scan_dirs` / `scan_expected` と 4 つの口を足し、`finish_index_build` で戻す。テスト（`finish_index_build` が戻す・`scan_progress` が 0 を `None` として返す）を足す
- [ ] `startup::to_ms` を `pub(crate)` にする
- [ ] `drain_index` に剪定・分母の読み・進捗の初期化・`start` / `end` 行・`build_ms` の計測を結線する
- [ ] 実機で完走と未完走の記録を確かめる（「実機での確認」の Phase 2 の末尾の 2 項目）

### Phase 3 — src-tauri: 進捗表示

- [ ] `SPEC.md` §3.3 にフォルダ数の表示を足す
- [ ] `FrameScanProgress` / `read_scan_progress` / `LauncherController::scan_progress` を足す
- [ ] `indexing_hint` をフォルダ数つきへ置き換え、テストを足す
- [ ] `view.rs` で進捗を 1 回だけ読み、構築中案内の arm で再描画を予約する。`scan_progress_is_read_exactly_once_per_frame` を足す

### Phase 4 — 文書と検証

- [ ] `src-tauri/CLAUDE.md` のモジュール構成に `scan_log.rs` を足し、「イベント駆動 wake の不変条件」に進捗のポーリングの例外を足す
- [ ] `docs/architecture.md`・`config/location.rs`・`docs/build-commands.md`（`SNOTRA_CONFIG_DIR` の節）の保存ファイルの列挙に `scan-log.jsonl` を足す
- [ ] `docs/build-commands.md` に読み方を足す
- [ ] `PERFORMANCE.md`「PATH マージ」の却下の根拠 1 の `rebuild_and_save` の返り値の記述を直す
- [ ] 検証チェックリストのカテゴリ A・C・D・F を実行する
- [ ] 実機で進捗表示と温度の違う標本を確かめ、Phase 2 の記録の確認とあわせて結果を PR 本文用に記録する

## 未確定（実装前に潰す）

（なし）

## 人間レビュー

- [x] 承認済み — 2026-11-15 / 問い: "issue #1041 の実装計画（workspace/plan.md。scan-log.jsonl に構築ごとの所要と内訳を記録し、status 行に「走査したフォルダ数 / 約 前回の完走のフォルダ数」を出す。4 フェーズ・SPEC §13.4 新設）を承認しますか？" / 回答: "承認する"

## plan-review 結果

- リスク: 高（スレッドをまたぐ共有状態・新しい永続ファイル・SPEC の変更・複数モジュール間のインターフェース）
- レビュー方式: 計画準拠レビュー 1 体（Step 2）。計画の前提に issue に書かれていない人間の裁定（永続 JSONL・進捗を含める・単位はフォルダ数・分母は記録から）があるため（`/plan-review`「リスク判定」）
- エージェント数: 1（全文 `workspace/plan-review-1041-step2.md`）

### 要対処

- R1 `RebuildStats.save` の `None` 分岐は `rebuild_and_save` から観測できず、記録に載る経路で到達不能 — 計画の修正（`save: Duration`・決定 5） — `cache.rs` の `save_cache_sorted` が `Config::config_dir()` の分岐を内側に持ち `IndexMaterial` だけを返すことを読んで確かめた
- R2 `read_to_string` / `lines()` だと非 UTF-8 の 1 バイトで剪定も分母も止まる — 計画の修正（バイト列で読み `b'\n'` で分割・テスト追加）
- R3 kill で切れた行に次の行が連結される／剪定の書き直しの途中終了で全行を失う — 計画の修正（`append_in` の先頭 `\n`・1 回の `write_all`・剪定は tmp → rename・テスト追加）
- R4 `ScanProgress.dirs` を `pub` にすると `state.rs` の「書き手を限定する」規約に反する — 計画の修正（private の 2 フィールドと 4 つの口・構築点は `AppState::new` だけ） — `state.rs` の `indexing` の doc（「書くのは … だけである」「private にしてある」）を読んで確かめた
- R5 500 ms の再描画は「時間経過で解消する不成立」ではなくポーリング — 計画の修正（決定 9 を例外として理由つきで書く）。**一部を降格**: 「Launching が Indexing を覆っている間は再要求が止まる」は偽。`notify.rs` の `overlay_kind` は `indexing` を最優先に返す
- R6 決定 10 の「窓は無くなる」は偽（2 反復の境目で 1 フレーム）— 計画の修正（書き込み順を固定し、残余として書く）

### 軽微

- M1 `dirs == m` の境界テスト → 反映
- M2 `sid` の同一ミリ秒での衝突 → 連番を足す（反映）
- M3 剪定 → 読み → start の順序を dir 注入で測れる形に → `begin_in` を切り出す（反映）
- M4 7 引数 → 束ねない判断と理由を書いた
- M5 `scan_id` は拡張子の変更でも変わる・`scan_identity_hash` の doc は段落ごと古くなる → 受容と doc の書き直しを反映
- M6 未完走を分母に使わない設計は妥当 → 変更なし
- M7 `unattributed_ms` に錠の待ちと swap が入る → `docs/build-commands.md` の読み方に書く（反映）
- M8 進捗を `status_row_present` に渡さない → 決定 9 に明記（反映）

### 未検証

- U1 長い文言が status 行の幅に収まるか → 実機の確認に最小幅の項目を足した
- U2 Launching に覆われた後の再開 → 降格（上の R5）。到達しない
- U3 hidden 中は `update()` が走らない → `src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」と `wake_main` の doc に依拠する。実機の確認（`/s` の後に隠したまま待ち、開き直したときに最新の数が出る）で間接に見る
- U4 `GetThreadTimes` の feature → 論点ごと消えた（レビュー後に `cpu_ms` を外した。決定 6）
- U5 smoke が実 `%APPDATA%` に書かないか → `scripts/smoke-egui.ps1` / `smoke-startup.ps1` / `bench-startup.ps1` は `SNOTRA_CONFIG_DIR` で検証用プロファイルを指す（grep で確かめた）。検証用プロファイルに `scan-log.jsonl` が生まれるが、smoke の検査は `*.bin` だけを見るので衝突しない
- U6 数十万フォルダでの `end` 行の大きさ・フレームごとのコスト → `end` 行は数値の固定キーだけで大きさはフォルダ数に依らない。フレームごとのコストは atomic の load 2 回

### 差分レビュー（Step 2 の後に入れた差分だけ・1 体）

全文 `workspace/plan-review-1041-delta.md`。対象は `save` の型・`AppState` の口・`begin_in` とバイト列の読み・`to_ms` の共有・`cpu_ms` の撤去・決定 9 / 10・文書の追加・テスト表・実機の確認の前倒し。

- 要対処 D1 `prune_in` が `binfmt.rs` の `remove_file` → `rename` を写すと「途中終了で全行を失わない」が偽になる — 計画の修正（`remove_file` を挟まず `rename` だけ・固定名の tmp・rename 失敗時の扱い・テスト追加） — `binfmt.rs` の保存手順（`remove_file` の後に `rename`）を読んで確かめた
- 要対処 D2 書き込み順は `Ordering` を指定しないと成り立たない — 計画の修正（`SeqCst` を明記。`fetch_add` だけ `Relaxed`）
- 要対処 D3 未完走の確認は走査の短いプロファイルでは `end` まで進む — 計画の修正（実プロファイルで行う）
- 軽微 M1〜M7（`save` が導出を含む・shorthand・`to_ms` の doc の射程・例外の文言に「毎フレーム再要求」・`finish_index_build` の呼び出し元 2 つ・`Some(0)` の扱い・clippy の文言）→ すべて反映
- 未検証: Windows の `rename` が既存の宛先を置き換えること（Rust の `std::fs::rename` の文書に依拠。テスト「`prune_in` の後に剪定されている」が実際の挙動を測る）・`append_in` の末尾バイト検査の実装形（実装で決める。テストが挙動を固定する）・`view.rs` の `Indexing` の arm で `ctx` が使えるか（実装で確かめる）

### 判断

- 実装着手: 可（人間の承認待ち）

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー1体（Step 2）
- エージェント数: 1（ほかに 3b の敵対的調査 1 体）
- 要対処: 6 件。すべて計画へ反映した（R5 は一部降格）。check スキル: `/persistence-check`（版を持たない追記ファイルなので版のバンプは不要。index.bin のバイト形式は変えない。後方互換は凍結の `end` 行のリテラルから読むテストで固定し、新形式の往復テストとは別に置く。読めない行は飛ばし、空で上書きしない）・`/symmetric-check`（`scan_dirs` / `scan_expected` の設定は drain の反復の先頭、戻すのは `finish_index_build` で、その呼び出し元は drain の後〔正常・unwind の両方〕と spawn 失敗の 2 つ。CAS 失敗は設定もしない。`start` と `end` は同じ `config_dir` の値で守る。同じ型の取り違え〔`sid` / `scan_id`・`scanned` / `dirs`〕はテストに互いに違う値を入れて検出する）・`/dry-check` 相当（ms の丸めは `startup::to_ms` を共有する。FILETIME の 64 bit 化は `startup.rs` が 1 か所だけで、時刻ではなく所要の変換なので写しにならない）。`/state-check` は新しいモード・ガードを足さないので対象外。`/race-check` は計画段階では起動しない（`/start-issue` 5a）
- 未検証: 上の「未検証」の U3（実機で間接に見る）
- 散文の全件列挙: 語 10 / 総件数 274 行 / ★未掲載 9（計画へ足した 1） / ☆行未掲載 0（計画へ足した 0・露出なし）
- 散文の全件列挙（2 回目・再描画と保存ファイルの概念語 `request_repaint_after` / `再要求` / `window.bin`）: 語 3 / 総件数 96 行 / ★未掲載 22（計画へ足した 2） / ☆行未掲載 0（計画へ足した 0・露出なし）
  - ☆ の「露出なし」: この plan.md は `path:N` の形で行を名指していないので、☆ はどちらの回も発火しえなかった。0 件は「☆ が見て何も無かった」ではない
  - 2 回目で足したのは `docs/architecture.md`（保存ファイルの列挙）と `snotra-core/src/config/location.rs`（`config_dir` から派生するファイルの列挙）。`src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」の全称（行 75）は、ファイルが既に計画に載っていたので ★ には出なかったが、決定 9 で偽になるので変更対象に足した。残る ★ 20 件は、局所の doc（`lifecycle.rs` の `BlurGrace` の規約・各 `request_repaint_after` の呼び出し）・`window.bin` 自体の記述・スクリプトの一般の注釈で、偽にならない
  - 足した 1 件は `src-tauri/src/icon.rs`（1 回目の sweep・`scan_all` の呼び出し）。残る ★9 件はどれも変更で偽にならない: `.claude/rules/snotra-core.md`（節の名指し）・`.claude/skills/state-check/SKILL.md`（一般の手順）・`docs/design/2026-05-31-coherence-staleset.md`（書いた後は更新しない設計メモ）・`snotra-core/CLAUDE.md`（見出し）・`snotra-core/src/indexer/cache/tests.rs`（結果を捨てる走査と整列の一般論）・`snotra-core/src/search/tests/build.rs`（`rebuild_and_save` が派生データを返さない分岐。組の返り値でも真）・`snotra-core/tests/search_allocations.rs`（`full_scan_allocations` で別物）・`src-tauri/src/commands/mod.rs` / `commands/window.rs`（構築中のエラーの一般論）
- 試行（#1300・高リスクのみ・独立レビュー 1 回ごとに 1 行）: 方式 Step 2 / 設計変更 3（R1 の返り値の型・R4 の公開面・M3 の `begin_in` の切り出し） / 主の見落とし 4（R2・R3・R4・R6）
- 試行（2 行目・差分レビュー）: 方式 Step 2 / 設計変更 0 / 主の見落とし 3（D1・D2・D3）
- エージェント数の合計: 3（3b の敵対的調査 1・Step 2 の 1・差分レビュー 1）
