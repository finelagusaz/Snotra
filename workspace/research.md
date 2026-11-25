# 調査: issue #1526（性能の残り 3 件を測ってから要否を決める）

## issue の要約

2026-11-16〜17 の性能 PR 群が「測ってから判断する」として範囲外に残した 3 件を、1 件ずつ測って採否を決める。
受け入れ条件: 3 件それぞれに測定値と採否が記録されている（採るなら実装、採らないなら理由を `PERFORMANCE.md` か issue に残す）。
「検討:」issue。Step 1 でユーザーに確かめ、「3 件とも測る」と回答を得た（2026-11-25）。

## 候補 A: `skip_name` を Substring/Prefix へ広げるか（出所 PR #1462「範囲外」）

- 対象: `snotra-core/src/search/scoring.rs` の `score_one_entry` 内 `let skip_name = mode == SearchMode::Fuzzy && plan.norm_query_has_path_sep && !self.any_name_has_path_sep;`
  - 同所のコメントは Prefix/Substring について「`find` は連続一致を要求するので同じ論証が使えるが、安いので飛ばす価値が無いと見て入れていない（**測っていない**）」と記録している
  - `skip_name` は name に加え file_name の照合も飛ばす（`needs_fn_score = !skip_name && ...`）
- 論証: Prefix（`starts_with`）・Substring（`find`）は needle の連続一致を要求する。区切りを含む needle は、区切りを含まない名前・file name に現れない。ゆえに Fuzzy と同じく証明として飛ばせる（前提 `!any_name_has_path_sep` は構築時の実測）
- **実運用点が該当する**: 実 `%APPDATA%\Snotra\config.toml` は `normal_mode = "substring"`・`migemo_enabled = true`（2026-11-25 に読んだ）。#1057 の計測は `normal_mode = "fuzzy"` へ向け替えて測っており、今の利用者の経路（Substring）の額は未測定
- 計測手段（既存）: `snotra-core/tests/path_query_cost.rs` の `measure_path_query_cost_across_settings`（Prefix/Substring/Fuzzy × limit × migemo × PATH マージで `c:\` と `\zzz-no-such-path\` を測る）、`measure_path_query_frame_cost`（`normal_mode` は実 config に従う）
- 等価性の検知器（既存）: `snotra-core/src/search/tests/path.rs` の `skipping_name_scoring_changes_nothing_over_real_index`（`#[ignore]`・実 index・**Fuzzy のみ**）、`fuzzy_name_match_survives_when_display_name_contains_path_separator`（Fuzzy のみ）
- 正本の額: `PERFORMANCE.md`「採用: パスクエリで name の Fuzzy スコアリングを行わない（`c:\` が 1 フレームの内側へ・#1057）」（Fuzzy で -3.0〜4.3 ms/打鍵）

## 候補 B: 表示計測 `egui_show:done` に trace の書き込みが入る（出所 PR #1468「範囲外」）

- `src-tauri/src/main.rs` の `setup_hotkey_listener`: `let t0 = Instant::now();` の直後に `trace_main("hotkey:listener_enter", json!({}))`、その後 `on_event_loop` へ post。`t0` は post の前に取る（hop を含めるため・コメントあり）
- `src-tauri/src/egui_shell/window_coordinator.rs`: `egui_show:done` の `ms` は `t0.elapsed()`。ゆえに `listener_enter` の `eprintln!` の所要が `ms` に含まれる
- `src-tauri/src/trace.rs` の `trace()`: `SNOTRA_TRACE` 無効なら即 return。**製品の既定経路では費用ゼロ**（`json!({})` の構築だけ）。汚れるのは計測値だけ
- 規模の手がかり: `PERFORMANCE.md`「計測と受け入れ基準」は「フレームループ中の trace は 1 行あたり約 10 ms」（#1004/#1032・stderr をファイルへリダイレクト）と記録。ただし「フレームループの外へこの数を持ち出すときは、起動段の反例を見ること」とも書く。listener はフレームループの外（Win32 メッセージループの hotkey スレッドで同期実行・`src-tauri/CLAUDE.md`「`app.listen` のコールバックは emit 元のスレッドで同期実行される」）
- `egui_show:done` の p50 は 25.5〜27 ms（`PERFORMANCE.md`「採用: 表示時の 5 ms の sleep を撤去する（#1446）」）。10 ms 規模なら無視できない比率
- 同型の前例: #1468 は `hotkey_register` の区間終端を `RegisterHotKey` 直後へ移し、区間が 0 ms になった（中身は trace の書き込みだった）

## 候補 C: `compute_wave1` の kana 分岐を並列にするか（出所 PR #1477「別 issue の候補」）

- 対象: `snotra-core/src/search/build.rs` の `compute_wave1`。kana は `for e in entries { arena.push(&to_kana(..)) }` の逐次。ただし `rayon::join` で `lower_names`/`lower_file_names` の構築と**並走**する
- **前提の誤り（調査で発見）**: 同所のコメントと `PERFORMANCE.md`（「`compute_wave1` の kana 分岐は逐次のまま残してある（通るのは cache-miss ＝ 全走査の内側だけ…）」）、PR #1477 の「走査が約 6 秒になると比が変わる」は、**cache-miss がこの関数を通る**ことを前提にしている。現行の製品経路では:
  - 走査後の構築は `indexing.rs` → `build_index_from_material` → `SearchEngine::from_material`。`indexer::rebuild_and_save` → `save_cache_sorted` が派生データ込みの `IndexMaterial` を返すので、`new_with_cached_masks` → kana は `kana_for_cached`（**並列**）を通る（`PERFORMANCE.md`「採用: 保存が返した派生データを cache-miss がそのまま使う（構築 539 → 24 ms・反復 11）」）
  - `compute_wave1` へ来るのは (1) `save_cache_sorted` で `Config::config_dir()` が `None`、(2) `new_with_cached_masks` の `CachedLower` が `None`（派生文字列を持たない旧版からの昇格の 1 回）、(3) `SearchEngine::new` / `new_with_migemo`（テスト・計測ハーネス）
  - ~~実測の手がかり: 実 `scan-log.jsonl` の最新 2 件は `build_ms` 21〜23・`scanned` 30,932~~ → **敵対的調査で撤回**（下記「敵対的調査の反映」争点 5）
- 計測手段（既存）: `snotra-core/src/search/tests/performance.rs` の `bench_new_migemo_on_off`（`#[ignore]`・合成 1k〜100k・`new` 経由＝`compute_wave1`）
- `to_kana` 単価 3.08 µs/件（`PERFORMANCE.md` の #1056 節）

## 再利用できる既存のパターン

- A/B + drift 対照（A'）で p50 を比べる形（#1057 の表）
- 一時の計装はコミットしない形（#1446 の測定条件）
- 計測の後に採らない判断を `PERFORMANCE.md` へ「却下:」節として残す形

## 技術的な制約

- 計測ハーネスは実 `index.bin` を書き換えうる（`path_query_cost.rs` の `//!`）。走らせる前に退避する
- trace の計測は A/B の差分で読む（`PERFORMANCE.md`「計測と受け入れ基準」）
- `skip_name` を広げる場合、等価性の検知器（`skipping_name_scoring_changes_nothing_over_real_index`）と変異で落ちる検知器を全モードへ広げる必要がある

## 未解決の疑問

- A: Substring/Prefix で name/file_name の照合をパスクエリで飛ばした額は、drift の帯を越えるか
- B: `hotkey:listener_enter` の `eprintln!` 1 行の所要は（フレームループ外・stderr の行き先で変わる）。直すならどの形か（`t0` を trace の後へ／書き込み時間の控除／trace を post の後へ）
- C: 製品が `compute_wave1` の kana を通る頻度はほぼゼロ。並列化の額（実規模で逐次 vs 並列）を測る価値はあるか。コメントと `PERFORMANCE.md` は「通るのは cache-miss の内側だけ」と書く。上限の主張としては真だが、「cache-miss ならここを通る」と読める点は偽（通るのは保存先が引けない cache-miss だけ）。区別して直す

## 主エージェントの追加確認（一次資料）

- A の論証の要: `match_score_single_cached` は Prefix で `lower_name.starts_with(plan.norm_query)`、Substring で `plan.substring_finder.find(..)`。`substring_finder` は `query_plan.rs` で `Finder::new(norm_query.as_bytes())`。needle はどちらも `norm_query` で、ガード `norm_query_has_path_sep` と同じ値を見る。**論証は成立する**
- C の経路: `save_cache_sorted_in` は常に `(IndexTree, CachedMasks, bool)` を返し、書き込みの成否に関係なく派生データを持つ。`IndexMaterial` から派生データが落ちるのは `save_cache_sorted` で `Config::config_dir()` が `None` のときだけ。`IndexMaterial::extend_with_path_entries` はマスクを伸ばし、落とさない
- **B の経路（重要）**: 実 config のホットキーは `Alt+Q`。`plan_hotkey` が `ShowAfterAltRelease` を選ぶと、別スレッドで Alt 離しを待ち、`show_egui_main(app, el, Instant::now())` で **`t0` を取り直す**。このとき `listener_enter` の書き込みは区間に入らない。区間に入るのは `ShowNow`（listener の時点で Alt が離れている／Alt を含まないホットキー）のときだけである
  - どちらの経路でも区間に入る trace: `egui_show:ime_control`（`ime_off_on_show = true` のとき・実 config は true）。エラー時の trace（`window_coordinator.rs` の `Err(_) => crate::trace_main(..)`）
  - `egui_show:done` の `ms` を読む消費者: `scripts/` は事象の有無だけを見る（`SnotraTraceInvariants.psm1` の `EventShowDone`・`smoke-egui.ps1` の `Wait-SnotraTraceEvent`）。`ms` の値を読むスクリプトは無い（`data.ms` の grep 0 件）。値を引いているのは `PERFORMANCE.md` の #1446 節（「Alt を離してから `egui_show:done` まで」＝`ShowAfterAltRelease` 経路で、`listener_enter` を含まない）

## 敵対的調査の反映（`workspace/adversarial-1526.txt`・sonnet 1 体）

| 争点 | 判定 | 採否と理由 |
|---|---|---|
| 1. 区切りを含む needle は Prefix/Substring でも一致しえない | 壊せなかった | 採る。主エージェントも一次資料で確認済み |
| 2. 実運用点は Substring | 壊せなかった | 採る。モードは config の live-read だけで決まる |
| 3. `ms` に `listener_enter` が入る・無効時ゼロ | 部分的に壊せた | 採る。`ime_control` も入る。さらに主エージェントの確認で、Alt+Q の主経路では `listener_enter` は入らない |
| 4. cache-miss は `compute_wave1` へ来ない | 壊せなかった | 採る。来るのは保存先 None・初回の空ツリー・旧版昇格・テスト/ハーネス |
| 5. `scanned` 30,932 は索引件数 | **壊せた** | 採る。実 `index.bin`（版 10）の `names` は約 784,020 件（⚠️ 先頭 varint の読み）。scan-log は Nov 18 で、Nov 22 の再構築は記録に無い。**件数は計測でロードした実索引の件数で書く** |

⚠️ の所見の扱い:
- 784,020 の読み、`to_kana` 単価からの 2.4 s 見積り → 採らない（見積りで判断しない）。C は実索引で実測する
- M1（等価性の検知器は Fuzzy・migemo off・PATH 無しだけ）→ 採る。A を採るなら Substring/Prefix 版の検知器を足し、「`has_path_sep` だけで飛ばす」変異で落ちることまで確かめる
- M2（ハーネスは実 config・history・index を書きうる・config_hash 不一致なら黙ってスキップ）→ 採る。実行前に 3 ファイルを退避し、出力に件数が出ることを確かめる
- M3（`bench_new_migemo_on_off` は合成入力で `rayon::join` の並走込み）→ 採る。C は実索引を入力に、kana 単独と構築全体の両方を測る
- `ime_control` が入る条件の全文・`env_flag` の実装 → 主エージェントが読んだ（`ime_control` は `ime_off_on_show` 真かつ HWND 取得成功で出る。`env_flag` は `1|true|yes|on`）
