# 調査: #1448 cache-miss の起動を待たせず、背景で構築する

## issue の要約

初回起動でない起動は、`main()` の中で `indexer::load_or_scan_with_stats` を同期に呼ぶ。`index.bin` が使えない（cache-miss）と、全走査と保存（22〜85 秒）が終わるまでトレイ・ホットキー・ウィンドウのどれも出ない。

方針（2026-11-17 決定・issue のコメント）: 待たせてよい。cache-miss でもトレイとホットキーを先に出し、索引は背景で構築する。構築中は「インデックス構築中…」と走査したフォルダの数が出るので、ユーザーはそれを見て待てる。

cache-miss の契機（issue）: `INDEX_CACHE_VERSION` を上げた版への更新後の初回起動（全ユーザーが 1 回ずつ踏む）・止めている間の走査対象の変更・`index.bin` の破損や削除・ツールチェインの更新による `DefaultHasher` の変化。

## 関連するファイル・シンボル

- `src-tauri/src/main.rs`
  - `main()`: `is_first_run` なら空の材料・`initial_indexing = true`・`cache_in_sync = false`。そうでなければ `load_or_scan_with_stats` を呼び、`startup::set_index_load_stats_total` / `startup::set_branch` / `startup::mark(Phase::IndexLoad)` を刻む。続いて PATH マージ・`HistoryStore::load`・`Engine::from_material`・`set_cache_in_sync`・`AppState::new(engine, initial_indexing)`
  - setup の中: `PlatformBridge::begin` → ウィンドウ生成 → `finish_platform_thread`（`PlatformBridge` を manage）→ … → `setup_first_run(&app_handle, is_first_run)` → `setup_hotkey_listener`
  - `setup_first_run`: first-run なら設定アプリを `--first-run` で起動し、失敗したときだけ `start_index_build(Rescan)`
- `src-tauri/src/commands/window.rs` の settings 監視: 設定アプリの終了時、`is_indexing() && !is_index_build_started()` なら `start_index_build(Rescan)`（first-run の構築の主経路）
- `src-tauri/src/state.rs`: `AppState::new(engine, initial_indexing)` の `indexing` フラグ。`is_indexing` は UI の構築中表示・`/s` のガード（`ensure_not_indexing`）・設定を開くガードに使われる
- `src-tauri/src/indexing.rs`: `start_index_build(app, BuildRequest)`・`drain_index`（#1456 以降、`must_scan` か cache-miss なら走査する）
- `snotra-core/src/indexer/cache.rs`
  - `load_or_scan_with_stats`（`Config::config_dir()` を解決）→ `load_or_scan_with_stats_in`: 照合キーを計算 → `load_cache_in(.., LegacyUpgrade::Write)` が当たればそれ（旧版なら昇格して書き戻す）、外れたら `with_index_write_lock` の中で走査・整列・保存
  - `LoadOrScanResult { material, cache_changed, cache_in_sync, stats }`・`LoadOrScanStats { cache_hit, hash, cache_load, cache_read, scan, sort, cache_save, total }`
  - 呼び出し元: 製品は `main.rs` の 1 か所。ほかは `snotra-core/tests/` の計測ハーネス（`match_cost.rs`・`memory_footprint.rs`・`path_query_cost.rs`・`search_allocations.rs`）
  - `INDEX_WRITE_LOCK` の doc: 形式昇格の読みはロックの外で、そのウィンドウは「昇格は `main` の起動段だけで、索引ビルドのスレッドはまだ居ない」という順序で閉じている
- `src-tauri/src/startup.rs`: `Branch { first_run, cache_hit, include_path_env }`・`set_index_load_stats_total`（`index_load_unattributed_ms` = 外側の区間 − 内側の `total`。契約は非負）
- `SPEC.md`「3.3 インデックス構築タイミング」: 通常起動は「キャッシュを即時ロードして終わり」、索引が更新される契機は 3 つ（初回構築・`/s`・設定変更）。cache-miss の起動は書かれていない
- スクリプト
  - `scripts/measure-memory-stages.ps1`: 段階 A は cache-hit の起動で測る。外れたら「走査が保存したキャッシュに当てるため 1 度だけ再起動する」——**cache-miss の起動が `startup:ready` までに保存を終える前提に立っている**
  - `scripts/bench-startup.ps1`・`scripts/smoke-egui.ps1`・`scripts/smoke-startup.ps1`・`scripts/visual-check-colors.ps1`: `New-SnotraVerificationProfile` が `*.bin` を消すので、検証用プロファイルの最初の起動は cache-miss（非 first-run）になる

## 再利用できる既存のパターン

- first-run の起動形（空の材料 + `initial_indexing = true` + 背景で `start_index_build`）。構築中の表示（status 行・トレイの「インデックス再構築中」）と、構築完了で検索が戻る振る舞いはそのまま使える
- `start_index_build(app, BuildRequest::Rescan)`。`cache_in_sync = false` なので `Rebuild` でも走査するが、意図を名指すため `Rescan`
- `load_or_scan_with_stats_in` の cache-hit の分岐。cache-miss のときに走査せず空を返す入口を、同じ関数の分割で作れる

## 技術的な制約

- 起動の経路で走査しない。cache-miss の判定（照合キーの計算と `index.bin` の読み）だけを `main()` に残す
- 旧版の昇格（`LegacyUpgrade::Write`）は起動の読みに残す。昇格は cache-hit の分岐なので、背景構築は走らず、`INDEX_WRITE_LOCK` の順序の論証は変わらない。cache-miss の起動では昇格は起きず、書き手は背景構築（`rebuild_and_save`・ロックの内側）だけになる
- 背景構築の開始は setup の中で、`PlatformBridge` を manage した後（`notify_indexing_started` がトレイへ届く位置）。`setup_first_run` と同じ位置
- first-run の設定アプリのオンボーディングは起動しない（issue）
- 計測ハーネスは、`load_or_scan_with_stats`（走査して保存する）を使い続ける

## 未解決の疑問

- `scripts/measure-memory-stages.ps1` の再起動の前提を、どう直すか（背景構築の完了を待ってから再起動する）
- 検証用プロファイルで起動する smoke 群は、最初の起動が cache-miss になる。背景構築の間、plain 検索は抑止される（SPEC 4.7）。smoke-egui の「results が出る」検査は再注入を 1 度持つが、背景構築の所要（極小の索引なので ms の桁）と競合しないか
- `startup:ready` の `cache_hit=false` は、これからは「走査して待った」ではなく「背景構築へ回した」を意味する。読み手（`measure-memory-stages.ps1` ほか）の解釈を変えるか
- 構築中は、トレイの「設定」「終了」がグレーアウトする（SPEC 3.3）。cache-miss の起動でも同じになる（22〜85 秒）。first-run と同じなので受容の範囲か
