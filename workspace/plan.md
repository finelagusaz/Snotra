# 計画: #1448 cache-miss の起動を待たせず、背景で構築する

## 目的と受け入れ条件

`index.bin` が使えない起動（cache-miss・非 first-run）でも、起動の中で走査しない。空の索引で起動してトレイとホットキーを先に出し、索引は背景で構築する（2026-11-17 の決定・issue のコメント）。

- [受け入れ 1] cache-miss の起動で、`startup:ready` までが cache-hit と同じ桁になる
- [受け入れ 2] 構築が終わると検索できる（first-run と同じ振る舞い）。構築中は status 行に「インデックス構築中…」と走査したフォルダの数が出る
- [受け入れ 3] first-run の設定アプリのオンボーディングは、cache-miss では起動しない
- [受け入れ 4] 旧版の `index.bin` は、これまでどおり起動の読みで昇格する（cache-hit の分岐）

## 設計

### core: 走査しない起動の読み（`snotra-core/src/indexer/cache.rs`）

- `load_or_scan_with_stats_in` の cache-hit の分岐を、`fn try_load_with_stats_in(dir, scan, show_hidden_system) -> Result<LoadOrScanResult, MissedLoad>` へ切り出す。`MissedLoad` は外れるまでに測った区間（`total_started`・`hash`・`cache_load`）を運ぶ private な struct
- `load_or_scan_with_stats_in` は `try_load_with_stats_in` が外れたら、今までどおり走査して保存する（計測ハーネスの経路。挙動は変えない）
- `pub fn load_cache_with_stats(scan, show_hidden_system) -> LoadOrScanResult` を足す（`Config::config_dir()` を解決。テスト用に `load_cache_with_stats_in(dir, ..)`）。当たれば `try_load_with_stats_in` の結果、外れたら**走査せず**空の材料を返す: `cache_hit = false`・`cache_changed = false`・`cache_in_sync = false`・`stats` の `scan` / `sort` / `cache_save` は 0、`hash` / `cache_load` / `total` は測った値。保存先が引けなければ同じく空を返す
- 昇格は `LegacyUpgrade::Write` のまま（cache-hit の分岐でだけ起きる。`INDEX_WRITE_LOCK` の順序の論証は変わらない）

### src-tauri: 起動の分岐（`src-tauri/src/main.rs`）

- 非 first-run の読みを `load_or_scan_with_stats` から `load_cache_with_stats` へ替える
- 起動時の索引の状態を 1 つの値で表す: `enum InitialIndex { Loaded, FirstRun, Deferred }`（`main.rs` に置く）。純粋な導出 `InitialIndex::of(is_first_run, cache_hit)` と、`initial_indexing()`（`FirstRun` / `Deferred` で真）を持つ。`cache_in_sync` は `Loaded` のとき `LoadOrScanResult.cache_in_sync`、それ以外は偽
- `startup::set_branch(first_run: false, cache_hit: false, ..)` と `startup::mark(Phase::IndexLoad)` は、`Deferred` でも今までどおり刻む（契約: `index_load` が null でよいのは first-run だけ）
- setup の中の `setup_first_run` を、`InitialIndex` の網羅の `match` で構築を起こす関数（`start_initial_build`）へ替える。`FirstRun` は今の `setup_first_run` の中身（設定アプリを `--first-run` で起動し、失敗したら `start_index_build(Rescan)`）、`Deferred` は `indexing::start_index_build(app, indexing::BuildRequest::Rescan)` の 1 行（設定アプリは起動しない・`rescan_callers_request_a_rescan` が行ごとに `Rescan` を求めるので 1 行で書く）、`Loaded` は何もしない。起こす位置は今の `setup_first_run` と同じ（`finish_platform_thread` の後・hotkey listener の前）
- `initial_indexing` を「first-run でしか立たない」と書いているコメント（`startup::set_branch` の上）を直す

### スクリプト

- `scripts/measure-memory-stages.ps1`: cache-miss の起動の後、再起動の前に背景構築の完了を待つ。完了の判定は `index.bin` の更新時刻が起動の時刻より後になること（上限 300 秒・超えたら throw）。コメントの「走査が保存したキャッシュに当てるため」を、背景構築が保存するまで待つ形に直す
- smoke 群（`smoke-egui.ps1`・`smoke-startup.ps1`・`visual-check-colors.ps1`）の「`*.bin` が生成された」検査: 検証用プロファイルの最初の起動は cache-miss になり、`index.bin` は背景構築が保存する。検査は起動から数秒後に 1 回だけで、検証用の索引は ms の桁で建つので、直さない。カテゴリ C をローカルで回して確かめる
- `scripts/bench-startup.ps1`: 検証用プロファイルの最初の 1 回は cache-miss の起動を測る（今は背景構築へ回る）。2 回目以降が `index.bin` を読む前提は、最初の起動の背景構築が終了待ち（`SettleMs`）の間に終わることに依る。検証用の索引は ms の桁なので直さない

### 文書

- `SPEC.md`「3.3 インデックス構築タイミング」: 通常起動で `index.bin` が使えないとき（旧形式からの版上げ・破損・止めている間の走査対象の変更）は、空の索引で起動して背景で構築する（初回構築と同じ表示。設定画面は開かない）。「索引が更新される契機」に、この起動を足す。3.4 のアイコンの無効化の契機と、13.4 の scan-log の契機の列挙にも足す
- `src-tauri/src/main.rs` の `//!`（索引の更新の経路）・`setup_first_run` の doc と呼び出しのコメント・`engine.set_cache_in_sync` のコメント、`src-tauri/src/state.rs`（`indexing` の初期値の説明）、`src-tauri/src/indexing.rs`（`start_index_build` の doc の経路の列挙）
- `snotra-core/src/indexer/cache.rs` の `//!`（走査の契機）・`compute_config_hash` の doc・`LoadOrScanStats` の `cache_save` / `total` の doc（起動の読みを指す記述）
- `docs/architecture.md`「起動シーケンスと初期化順序」・`docs/build-commands.md`（scan-log の契機の列挙）
- `PERFORMANCE.md`「計測と受け入れ基準」: `cache_hit=false` の起動時間は #1448 の前後で意味が変わる（走査を待つ → 背景へ回す）ので、前後で比べない旨を 1 行
- `snotra-core/src/indexer/cache.rs` の doc: `INDEX_WRITE_LOCK` の書き手の列挙（「cache-miss save」は製品の起動経路から消え、計測ハーネスだけになる）・`load_or_scan_with_stats` を起動経路として説明している文
- `src-tauri/src/startup.rs`（`index_load_unattributed_ms` を導くコメント・417〜426 行）: 内側の呼び出しを `load_or_scan_with_stats` と名指しているので、`load_cache_with_stats` へ直す。`total_started` の起点の説明も、切り出した `try_load_with_stats_in` に合わせて確かめる
- prose:sweep の残り（直さない）: `PERFORMANCE.md`・`index_tree.rs`・`str_arena.rs`・`search/build.rs`・`search/tests/build.rs` の「cache-miss ＝ 22〜30 秒の全走査」は、走査が背景へ移るだけで変更後も真。`snotra-core/CLAUDE.md`・`smoke-egui.ps1`・`state.rs`・`snotra-core/tests/*` は、名指す経路の意味が変わらない。`docs/adr/`・`docs/superpowers/` は凍結された記録
- `PERFORMANCE.md`: cache-miss の起動の `startup:ready` を前後で記録する

## 変更するファイルとシンボル

1. `snotra-core/src/indexer/cache.rs`: `try_load_with_stats_in`（新規・private）・`MissedLoad`（新規・private）・`load_or_scan_with_stats_in`・`load_cache_with_stats` / `load_cache_with_stats_in`（新規）・`INDEX_WRITE_LOCK` の doc
2. `snotra-core/src/indexer.rs`: `load_cache_with_stats` の re-export
3. `src-tauri/src/main.rs`: 起動の読み・`InitialIndex`（新規）・`start_initial_build`（`setup_first_run` を替える）・`//!` とコメント・テスト
4. `scripts/measure-memory-stages.ps1`
5. `src-tauri/src/startup.rs`（コメント）
6. `src-tauri/src/state.rs`・`src-tauri/src/indexing.rs`（doc）
7. `SPEC.md`（3.3・3.4・13.4）・`PERFORMANCE.md`・`docs/architecture.md`・`docs/build-commands.md`

## 不変条件と異常系

- 起動の中で走査しない（cache-miss でも）
- cache-miss の起動で、背景構築は必ず起きる（`start_initial_build` の `Deferred` の腕が `Rescan` を渡す。`cache_in_sync = false` でもある）
- first-run の経路は変えない（`setup_first_run` と settings 監視）
- 構築中は first-run と同じガードが効く（`/s`・設定を開く・plain 検索の抑止・トレイの「設定」「終了」のグレーアウト）。cache-miss の起動で 22〜85 秒そうなるのは、決定どおり受容する
- 背景構築が保存する前にプロセスが終わったら、次の起動も cache-miss になり、もう一度背景で構築する（同期で待っていた頃と同じ結果になる）

## テスト方針と検証コマンド

- core（`snotra-core/src/indexer/cache/tests.rs`・dir 注入）
  - `load_cache_with_stats_in`: `index.bin` が無ければ空の材料を返し、走査せず（`stats.scan == 0`・材料 0 件）、`index.bin` を作らない。`cache_hit` / `cache_in_sync` は偽
  - 当たれば材料を返し、`cache_hit` / `cache_in_sync` は真
  - 走査対象のハッシュが違えば外れる（空）
  - 旧版は昇格して書き戻す（既存の `load_cache_upgrades_a_legacy_format_in_place` と同じ形を、新しい入口で 1 本）
  - `load_or_scan_with_stats_in` の既存テスト（`a_cache_hit_startup_does_not_scan` ほか）は緑のまま（挙動を変えない）
- src-tauri: `InitialIndex::of` の表（first-run なら `FirstRun`、非 first-run の hit なら `Loaded`、miss なら `Deferred`）と、`initial_indexing()` が `FirstRun` / `Deferred` で真・`Loaded` で偽。`rescan_callers_request_a_rescan` が `main.rs` の新しい呼び出しも `Rescan` であることを縛る（既存の検査がそのまま母集団に含む）
- E2E（一時スクリプト・scratchpad）: `SNOTRA_CONFIG_DIR` を temp へ向けて実 `config.toml` のコピーだけを置き、基準（main）とこの変更で `startup:ready` までの時間と `cache_hit` を測る。この変更では、構築が終わると `index.bin` ができ、検索が戻る（`egui_search:settled` の `index_entries`）ことも見る
- 検証: カテゴリ A・C・E・F（C は smoke 群が最初の起動で cache-miss を踏むのでローカルで回す）。`test:powershell`（`scripts/` を変えるので）

## 作業項目

### Phase 1: core
- [ ] `cache.rs`: `try_load_with_stats_in` を切り出し、`load_cache_with_stats(_in)` を足す。テスト

### Phase 2: src-tauri とスクリプト
- [ ] `main.rs`: 起動の読み・`InitialIndex` とそのテスト・`start_initial_build`
- [ ] `measure-memory-stages.ps1`: 背景構築の完了を待つ

### Phase 3: 文書と計測
- [ ] `SPEC.md` 3.3・`cache.rs` の doc・prose:sweep の残り
- [ ] E2E で受け入れ 1〜4 を確かめ、`PERFORMANCE.md` に記録する
- [ ] `/state-check`・`/race-check`・`/symmetric-check` を当てる
- [ ] 検証カテゴリ A・C・E・F

## 未確定（実装前に潰す）

（なし）

## 利用者から見える帰結（PR で名指す）

- cache-miss の起動では、構築が終わるまで（22〜85 秒）、トレイの「設定」「終了」がグレーアウトし、`/o` と `/s` も拒まれる（first-run と同じ）
- 構築中に終了すると、次の起動もまた cache-miss になり、もう一度背景で構築する
- 背景構築は走査するので、`icons.bin` を捨てる（SPEC 3.4 の契機）

## 人間レビュー

- [x] 承認済み — 2026-11-17 / 問い: "#1448 をこの計画で実装に進めてよいですか？" / 回答: "承認する"

## セルフレビュー

- リスク: 高（起動の状態遷移・初期化順序・複数モジュールのインターフェース）
- plan-review: 独立レビュー1体（Step 2b・独立導出）
- エージェント数: 2（3b の敵対的調査 1・独立導出 1）
- 要対処: 3 件を反映した。(1) 起動の分岐を `InitialIndex` の値にし、純粋な導出をテストで縛る。(2) 偽になる散文の追加（main.rs の `//!` と `setup_first_run` の doc・state.rs・indexing.rs・cache.rs の `//!` / `compute_config_hash` / `LoadOrScanStats`・architecture・build-commands・SPEC 3.4 / 13.4・PERFORMANCE の比較の断り）。(3) 利用者から見える帰結を PR で名指す
- 独立導出との不一致（採らなかったもの）: 構築完了の trace を足して `measure-memory-stages.ps1` が待つ案 — 計画は `index.bin` の更新時刻を待つ。製品へ計測用の trace を足さずに済み、待つ対象（保存）そのものを見る
- 未検証: 背景の走査が `startup:ready` までの時間を奪う額（E2E で測る）・smoke 群の `*.bin` 検査と results 検査が競合に変わる余裕（カテゴリ C で確かめる）
- 散文の全件列挙: 語 4 / 総件数 168 行 / ★未掲載 11（計画へ足した 1: `startup.rs`。独立導出が別に挙げた 8 件も足した。残りは変更後も真か凍結された記録） / ☆行未掲載 0
- 試行（#1300・高リスクのみ・独立レビュー 1 回ごとに 1 行）: 方式 Step 2b / 設計変更 1（`InitialIndex`） / 主の見落とし 8（散文）
