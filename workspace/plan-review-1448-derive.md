# #1448 の変更の独立導出

- 対象: worktree `issues-1445-1457-c19d64`（ブランチ `perf/1448-cache-miss-background-build`・HEAD 51516e34）
- `workspace/` の他ファイル（plan.md・research.md・adversarial-*.txt）は読んでいない
- コードは編集していない。`cargo test` も Snotra の起動もしていない（以下の「⚠️」は未実測）

## 0. 現状の機序（根拠）

- 同期の全走査は `src-tauri/src/main.rs:237-266` の else 分岐。`indexer::load_or_scan_with_stats` を `main()` の中で呼ぶ。cache-miss 分岐は `snotra-core/src/indexer/cache.rs:466-510`（`load_or_scan_with_stats_in`）。`with_index_write_lock` の中で、走査・整列・保存（`save_cache_sorted_in`）まで済ませてから返る
- 背景構築の入口は `src-tauri/src/indexing.rs:43` の `start_index_build(app, BuildRequest)`。`AppHandle` が要るので、呼べるのは setup 以降に限られる。drain が走査するかどうかは `engine.rs:333` の `must_scan = rescan_requested > rescan_done || !cache_in_sync` で決まる
- first-run は `main.rs:231-236` を通る。空の索引で、`initial_indexing=true`・`cache_in_sync=false` にする。setup の `setup_first_run`（`main.rs:475-479`）が `--first-run` で設定アプリを起動し、起動に失敗したときだけ `start_index_build(Rescan)` を呼ぶ。設定アプリが終わった後の kick は `commands/window.rs:163-170`（`is_indexing() && !is_index_build_started()`）にある
- 構築中の表示は、どれも `AppState.indexing` から導いている。通常結果の非表示と status 行の「インデックス構築中… N / 約 M フォルダ」（`egui_shell/window_coordinator.rs:161` の `read_indexing`、SPEC §4.7・§3.3）、`/o`・`/s` の `ERR_INDEXING_IN_PROGRESS`（`commands/window.rs:178`・`commands/system.rs:11`）、トレイのグレーアウト（`PlatformCommand::SetIndexing` → `platform/mod.rs:318`・`tray.rs:353`）がこれに当たる
- 形式昇格（旧版の `index.bin` を書き戻す処理）は `load_cache_in(.., LegacyUpgrade::Write)` の内側で起き、読みはロックの外にある。このウィンドウを閉じているのは起動の順序だけである（`cache.rs` の `INDEX_WRITE_LOCK` の doc と、`LoadCacheResult::upgrade_save` の doc の「`INDEX_WRITE_LOCK` の取得待ちを含む」の段落）

## 1. 変更が必要なファイルとシンボル

### 推す形（要約）

**キャッシュの読みは `main()` に残し、cache-miss のときに走査だけを背景へ回す。**

1. `main()` で `index.bin` を読む（cache-hit なら、旧版の昇格も含めて今と同じ）
2. 読めなければ空の索引で engine を建てる。`initial_indexing=true`、`cache_in_sync=false` にする
3. setup の、今 `setup_first_run` を呼んでいる位置で `start_index_build(.., BuildRequest::Rescan)` を呼ぶ。設定アプリは起動しない

ロード全体（cache-hit も含む）を背景へ移す案は採らない。理由は §4 の判断点 A。

### snotra-core

| ファイル / シンボル | 変更 | 根拠 |
|---|---|---|
| `snotra-core/src/indexer/cache.rs` に新しい公開関数を足す（名前は仮に `load_cached_with_stats` / `_in`） | 読むだけで、cache-miss でも走査しない。返り値は「hit: `LoadOrScanResult` 相当（material・`cache_in_sync=true`・stats）／miss: stats だけ」。`LegacyUpgrade::Write` で読む（昇格は今と同じく `main` の起動段で起きる） | `load_or_scan_with_stats_in`（`cache.rs:429`）の hit 側が `cache.rs:434-465`、miss 側が `466-510` |
| `load_or_scan_with_stats_in`（`cache.rs:429`） | 新しい関数の hit 側を呼び、miss のときだけ今の走査と保存を続ける形に組み直す（hit の半分を写さない。`LoadOrScanStats` の生成を 2 部に分けない） | 製品以外の呼び手が実在する: `tests/memory_footprint.rs:277`・`match_cost.rs:136`・`path_query_cost.rs:153/286/398/526`・`search_allocations.rs:509`（`#[ignore]` の計測コード）と `cache/tests.rs:704/716/755/1126/1163` |
| `load_or_scan_with_stats`（`cache.rs:525`） | 残す（上の計測コードが使う）。`config_dir()` が `None` の分岐（`528-560`）も残す | 同上 |
| `snotra-core/src/indexer.rs:24-26` の re-export | 新しい関数を足す | `pub use` の一覧 |
| `LoadOrScanStats` | 型は再利用してよい。miss のとき `scan` / `sort` / `cache_save` は `ZERO`。doc の「cache-miss 分岐（scan して save する）」（`cache.rs:84-92`）は「`load_or_scan_with_stats` の cache-miss 分岐」と限定し、新しい関数の miss には scan も save も無いことを書く | `cache.rs:84-104` |

代わりの形: `load_or_scan_with_stats` に `OnMiss::{Scan, Defer}` を足す案もある。製品の呼び手が 1 か所で、計測コードはすべて `Scan` を渡すので、引数で分けるより入口を分けて合成するほうが、呼び手の意図が名前に出る。

### src-tauri

| ファイル / シンボル | 変更 | 根拠 |
|---|---|---|
| `main.rs:231-284`（`main()` の索引の段） | 3 分岐を 1 つの値で表す。たとえば `enum InitialIndex { Loaded, FirstRun, Deferred }`。`Deferred` は空の索引・`initial_indexing=true`・`cache_in_sync=false`。`startup::set_branch` は今のとおり stats から取る（`cache_hit=false`）。`set_index_load_stats_total` も呼ぶ（新しい関数が `total` を返す） | `main.rs:257-264` |
| `main.rs:305` `AppState::new(engine, initial_indexing)` | `initial_indexing` を `InitialIndex` から導く（FirstRun と Deferred が true） | `state.rs:63` |
| `main.rs:413`・`setup_first_run`（`main.rs:469-479`） | 名前を変え、`InitialIndex` を受け取る形にする。たとえば `setup_initial_index`。FirstRun は今のまま（設定アプリ。失敗したら Rescan）、Deferred は `indexing::start_index_build(app, indexing::BuildRequest::Rescan)` だけ、Loaded は何もしない。**`match` を網羅にし、Deferred の腕に `launch_settings_process` を書かない** | `main.rs:475-478` |
| `main.rs:300-302` の `set_cache_in_sync` とそのコメント | 値は `InitialIndex` から導く（Deferred は false）。コメントの「保存に失敗した cache-miss」は「cache-miss（空の索引で起動する）」へ直す | `main.rs:300-301` |
| `main.rs:268-276`（PATH のマージ） | Deferred でも `include_path_env` なら空のツリーへ PATH エントリを足すことになる（first-run も今そうしている）。背景の構築が `build_index_from_material` で PATH をマージし直すので、結果は正しい。手当ては要らない（判断点 E） | `indexing.rs:129-142` |
| `indexing.rs` のテスト `rescan_callers_request_a_rescan`（`indexing.rs:587-624`） | 変えなくてよい。`main.rs` の `start_index_build(` を含む行が、すべて `BuildRequest::Rescan` を含めば通る。**Deferred の呼び出しは 1 行で書く**（折り返すと赤になる） | `indexing.rs:608-621` |
| `indexing.rs:36` の doc（「ビルド要求の全経路（config 変更 reindex / first-run / 手動 rebuild / 自己再 kick）」） | 起動時の cache-miss を足す | 散文が偽になる |
| （任意）`indexing.rs` の `notify_indexing_complete` などに trace を足す | 構築の完了を外から待てる口になる。§3 の `measure-memory-stages.ps1` と受け入れの計測が使う（判断点 D） | 今は構築の完了を表す trace が無い（`git grep` の結果は 0 件） |

`startup.rs` は、コードを変えなくても動く。`Branch` の値は `first_run=false`・`cache_hit=false` のままで、`index_load` は「ハッシュ + 読みの失敗」だけの短い区間になる。変える散文は §2。

## 2. 変更で偽になる散文

| 場所 | 今の主張 | 直し方 |
|---|---|---|
| `SPEC.md` §3.3「通常起動時はキャッシュを即時ロードして終わり」（79-82 行） | 起動は読むだけで終わる。索引が更新される契機は 3 つ（初回構築・`/s`・設定変更） | 「`index.bin` が使えない（無い・壊れている・形式が古すぎる・走査対象が違う）ときは、空の索引で起動し、背景で構築する。構築中の表示は初回起動と同じ。設定画面は開かない」を足す。契機の列に「起動時に `index.bin` が使えないとき」を足す（「3 つ」という数は書かず、列挙だけにする） |
| `SPEC.md` §3.3 の first-run の子項目（73-78 行） | 構築中の表示・トレイの規則が first-run の下にだけ置かれている | 構築中の規則を「構築中は（契機を問わず）」の形で 1 か所に上げるか、cache-miss の項から参照する |
| `SPEC.md` §3.4（99 行）の「索引の再構築で走査したとき（初回構築・`/s`・走査対象の変更）」 | アイコンを無効化する契機の列挙 | cache-miss の起動を足す（`drain_index` は走査した反復で必ず無効化する。`indexing.rs:225-227`） |
| `SPEC.md` §13.4（777 行）の「走査を伴う索引の構築（初回構築・`/s`・走査対象の変更）」 | `scan-log.jsonl` に記録する契機 | 同上（`drain_index` の走査する反復は必ず記録する） |
| `docs/build-commands.md:285`（「走査を伴う索引の構築（`/s`・走査対象の変更・初回構築）ごとの所要は…」） | 同上 | 同上 |
| `snotra-core/src/indexer/cache.rs` の `//!` 6-9 行（「走査の契機は明示操作だけである——初回構築・`/s`・設定変更の 3 つで…」） | | 起動時の cache-miss（背景）を足す。cache-hit の起動は読むだけで終わる、は残す |
| `cache.rs` の `compute_config_hash` の doc（294-297 行。「次の起動は cache-miss 分岐へ落ちて 22〜30 秒の全走査を払う（…これが自動で走る唯一の場所である）」） | 起動が走査で止まる、と読める | 「起動の後、背景で全走査を 1 回払う」に直す |
| `cache.rs` の `load_cached_entries` の doc（349-352 行） | `load_or_scan_with_stats` の cache-miss は走査して書く | 真のまま（関数は残る）。直さない |
| `cache.rs` の `INDEX_WRITE_LOCK` の doc（1137 行「書き手（`rebuild_and_save` / cache-miss save / ロードの旧版の分岐の昇格）」） | cache-miss save を製品の書き手に数えている | 「cache-miss save（計測コードが呼ぶ `load_or_scan_with_stats` だけ）」と限定する。「製品でこの経路を通る呼び出し元は `main` の起動段の 1 つだけ」は、新しい関数も `main` が呼ぶので真のまま（§3） |
| `cache.rs` の `LoadOrScanStats::cache_save` の doc（84-92 行）と `total` の doc（106-110 行。`startup.rs` の消費を述べる） | 起動の cache-miss は scan と save を含む | 新しい関数の miss には scan も save も無い、と書く |
| `src-tauri/src/main.rs` の `//!`（4-5 行「索引の更新は明示操作（`start_index_build` の kick）だけを経路とする」） | | 「明示操作と、起動時の cache-miss」にする |
| `main.rs:254-258` のコメント（「`load_or_scan_with_stats` の中にある未命名の処理」「非 first-run の cache-miss が cache_hit=true になる」） | 関数名と分岐の説明 | 新しい関数名へ直す。`initial_indexing` は cache-miss でも立つようになるので、「first-run でしか立たない」は偽になる。そこから導かない理由を「Deferred と FirstRun を区別できない」へ書き直す |
| `main.rs:301` のコメント（「first-run・保存に失敗した cache-miss」） | | §1 のとおり直す |
| `main.rs:411-412`・`setup_first_run` の doc（469-474 行） | first-run だけを述べる | 関数の改名に合わせて書き直す |
| `src-tauri/src/startup.rs:417-428`（`index_load_unattributed_ms` の段落。「`load_or_scan_with_stats` の中にある未命名の処理」「外側の区間（…`load_or_scan_with_stats` の呼び出し後）」） | 関数名 | 新しい関数名へ直す。非負性の前提（外側の区間が内側の `total` を包む）は、`main` のマークの位置を変えなければ保たれる |
| `startup.rs:897` 付近のテストコメント（「first-run 分岐では `LoadOrScanStats` 自体が存在しない」） | | 真のまま |
| `src-tauri/src/state.rs:26-27`（「first-run ではビルドスレッドが居なくても `AppState::new` が true で建てる」） | | cache-miss の起動でも true で建てる、を足す（cache-miss ではすぐ setup がスレッドを起こす） |
| `snotra-core/src/engine.rs:724` のテストコメント（「起動で index.bin と同期していない（first-run・保存失敗）」） | | cache-miss を足す（任意） |
| `docs/architecture.md`「起動シーケンスと初期化順序」（86-91 行） | 索引についての記述が無い | 1 行足すのを推す（`main()` は読むだけで、読めなければ setup が背景構築を kick する）。「設定管理」の段（100 行）の「`/s`・初回構築の走査要求を世代で持ち」にも、起動時の cache-miss を足す |
| `PERFORMANCE.md`「計測と受け入れ基準」（3238 行） | 「cache-miss を測るには `SNOTRA_CONFIG_DIR` を temp へ…初回起動を再現できる」 | 計測コード（`memory_footprint`）については真のまま。製品の起動では、cache-miss でも `index_load` に走査が乗らなくなる。`cache_hit=false` の起動時間を、#1448 の前後で同じ物差しで比べないこと、を 1 行足す。受け入れの計測値は新しい「採用:」の節に書く |
| `scripts/bench-startup.ps1:89-90`（「ループ内で作り直すと毎回 first-run + cache-miss になり」） | | 真のまま。ただし §3 の注意を足す |
| `scripts/measure-memory-stages.ps1:21-22`・`150-163`（「外れると起動の中で全走査が走り…1 度だけ再起動する」） | **偽になり、スクリプトが壊れる**（§3） | 直す |
| `scripts/smoke-startup.ps1:176-179`・`scripts/smoke-egui.ps1:974-978`・`scripts/visual-check-colors.ps1:314-315`（「実測（#803）: 出るのは `index.bin` である（索引 0 件でも書かれる）」） | `index.bin` は起動の中で同期に書かれる、が暗黙の前提 | 判定はおそらく緑のまま（§3）。「`index.bin` は背景の構築が書く」を添えるか、#803 の実測を取り直す |
| `docs/adr/ADR-index-v7-cache-miss.md`・`ADR-rescan-explicit-only.md`・`ADR-scan-progress-unit-and-log-fields.md` | 凍結された記録 | 触らない（依頼による除外） |

## 3. 頼っている／壊れうる周辺と、その扱い

### 起動の計測（`startup.rs` とその契約）

- **payload のキー集合は変わらない。** `Phase` の全区間・`first_run`・`cache_hit`・`include_path_env`・`index_load_unattributed_ms` はそのまま出る。cache-miss の起動では `index_load` が非 null の短い値になり、`index_load_unattributed_ms` も非 null になる（新しい関数が `LoadOrScanStats.total` を返すため）。`scripts/lib/SnotraStartupContract.psm1:118` の null 免除は `first_run` だけなので、契約の検査（`Test-SnotraStartupPayload`）は通る
- 非負性（`index_load ≥ LoadOrScanStats.total`）: `startup::mark(IndexLoad)` を新しい関数の呼び出しの後に置く限り保たれる。`bench-startup.ps1` の `>= 0` が見張っている
- 区間への影響: `start_index_build` を今の `setup_first_run` の位置で呼ぶと、その費用（engine の錠 1 回・`try_begin` の CAS・`notify_indexing_started` の bridge 送信と emit・スレッドの spawn）は `windows_create` のマークから `setup_rest` のマークまでの区間に入る。1 ms に届かない見込み（⚠️未実測）。走査スレッドは `hotkey_register`（platform スレッドの登録待ち）と並行に走るので、I/O や CPU を奪い合う可能性はある。受け入れ（同じ桁）は満たす見込み（⚠️）
- **意味の変化**: #1448 の後、`first_run=false && cache_hit=false` は「走査を背景へ回した起動」を表す。payload にフィールドを足さなくても読み分けられる（判断点 C）。`PERFORMANCE.md` の過去の `cache_hit=false` の値（走査を含む）とは比べられない

### 計測と smoke のスクリプト

- **`scripts/measure-memory-stages.ps1`（壊れる）**: 起動が `cache_hit=false` なら、`Stop-Process` して 2 秒後に再起動し、キャッシュに当てる作りになっている（150-163 行）。今は `startup:ready` が走査と保存の後に来るので、再起動したときには `index.bin` がある。変更後は `startup:ready` がすぐ来るので、背景の保存の前に kill される。再起動しても miss のままで、`throw "再起動しても索引キャッシュに当たらない"` になる。**直す**: 構築の完了を待ってから再起動する。待ち方は、構築完了の trace（判断点 D）か、`scan-log.jsonl` の `end` 行を見る（`SNOTRA_CONFIG_DIR` / `%APPDATA%\Snotra` を解決する必要がある）
- **`scripts/bench-startup.ps1`**: `-UseVerificationProfile` の seed では、走査対象は空（索引は 0 件）。1 回目は miss で、背景構築は数 ms で終わるので、2 回目からは hit になる見込み。実 config で cache-miss のときは事情が違う。毎回 `SettleMs`（1.5 s）の後に kill され、保存まで届かないので、**全回が miss になる**。そのうえ、メモリは走査の途中を測る。cache-miss の起動時間を繰り返し測るにはこの性質がむしろ都合よいが、メモリの列は意味が変わる。スクリプトのコメントか出力で「miss の回のメモリは走査の途中」と断るのを推す
- **`scripts/smoke-startup.ps1`**: プロファイルは 5 起動で共有し、`index.bin` は初回だけ無い（`New-SnotraVerificationProfile` は `*.bin` を消す。`SnotraSmoke.psm1:245`）。1 回目は Deferred を通る。first-run を踏まないこと（`cmd:launch_settings_process:*` が 0 件）は、Deferred の腕に設定アプリの起動を書かない限り保たれる。ループの後の「`*.bin` ≥ 1」の判定は、背景構築（走査対象は空）が `$WaitMs` の間に `index.bin` を書けば通る。**同期から競合へ変わる**ので、⚠️で残す
- **`scripts/smoke-egui.ps1`**: 毎回 `*.bin` を消すので、シナリオ 1・2 とも Deferred を通る。`"z"` を注入した時点で構築中なら通常結果は隠れる。それでも構築が終わると `index_generation` が進み、view が検索し直すので（`state.rs` の `index_generation` の doc・`view.rs:964` 付近）、`egui_results:show` は遅れて出る見込み。走査対象は 1 ファイルなので、構築は打鍵より先に終わる見込みが高い（⚠️。CI の runner で確かめる。カテゴリ C）。`*.bin` の判定は smoke-startup と同じ
- **`scripts/visual-check-colors.ps1`**: 同じ型。results ウィンドウを 10 s 待つので、構築の完了は待てる見込み
- **CI の workflow**: `.github/workflows/smoke.yml`（smoke-egui・smoke-startup・bench-startup `-UseVerificationProfile`）と `release.yml:82-83`（release 本体での smoke-startup）。workflow のファイルは変えなくてよい。ただし上の 2 つの smoke の 1 回目が Deferred を通るようになるので、**CI の smoke が新しい経路の唯一の実機の観測点になる**。PR では smoke.yml が走る（`paths` に `src-tauri/**` が含まれることを確認した）

### `INDEX_WRITE_LOCK` の順序の論証

- 論証の要点は、昇格（読みがロックの外にある書き手）は `main` の起動段でしか起きず、そのとき索引ビルドのスレッドはまだ存在しない、ということである（`cache.rs` の `INDEX_WRITE_LOCK` の doc）
- 推す形では、昇格は今と同じく `main` の起動段（新しい関数の hit の側）でしか起きない。背景構築は setup の中で `start_index_build` が起こすので、`main` の読みより必ず後になる。しかも cache-miss のとき昇格は起きていない。**論証は崩れない。** 背景の書き手は `rebuild_and_save`（`cache.rs:703-732`）で、走査から保存までを 1 回のロック取得で覆っている
- **崩れるのは、ロード全体（hit も含む）を背景スレッドへ移す形である。** そうすると昇格が背景で走り、設定変更の `start_index_build` が別の drain を起こす経路と並び、ウィンドウが開く。判断点 A でこの形を退ける理由の 1 つである
- `LoadCacheResult::upgrade_save` の doc（「今のところ待ちは立たない…」）も、同じ理由で真のまま
- `snotra-core/CLAUDE.md`「index.bin 書き込みの排他（INDEX_WRITE_LOCK）」の「ロード後に走りうる書き手を足すときは見直す」: 足すのは既存の書き手の新しい呼び出し点であり、上の見直しで閉じていると言える。PR 本文にこの見直しを書く

### first-run の経路

- 変えない。`Config::is_first_run()` が真なら、今のとおり空の索引で `--first-run` の設定アプリを起動する。cache-miss（非 first-run）では設定アプリを起動しない（決定どおり）
- `commands/window.rs:163-170`（設定アプリの終了時に、`is_indexing() && !is_index_build_started()` なら kick する）: Deferred では setup がすでにビルドを起こしているので `is_index_build_started()` が真になり、重ねて kick しない。spawn に失敗した場合は `finish_index_build` が `indexing=false` に戻す（`indexing.rs:108-113`）。その後に設定アプリを閉じても kick は起きないが、それは今の first-run の失敗経路と同じである

### 構築中の振る舞い（issue のコメントが確かめよと言う点）

- 通常結果の非表示（SPEC §4.7）・status 行の進捗・`/o` と `/s` の拒否・トレイのグレーアウトは、どれも `AppState.indexing` と `SetIndexing` から導かれる。`start_index_build` を通る限り、この経路でも同じに働く。進捗の分母は `scan-log.jsonl` から引くので、同じ走査対象で完走した記録があれば出る（版を上げた後の miss では前回の記録が使える）
- `SetIndexing(true)` は bridge が managed になってからでないと届かない（`indexing.rs:268-272` は `try_state`）。今の `setup_first_run` の位置は `finish_platform_thread`（`main.rs:366`）より後なので届く。トレイの生成（`setup_tray`）より前に届くが、platform 側は旗を変数で持つ（`platform/mod.rs:239/318`）ので問題ない。first-run の失敗経路が今すでにこの順で動いている
- `INDEXING_STARTED` の emit は setup のスレッドで同期に走る。wake の listener（`register_config_wake_listeners`・`main.rs:396`）はそれより前に登録されている
- **利用者から見える帰結**（決定の「待たせてよい」の範囲内だが、PR 本文で名指しするのを推す）:
  - 構築中（22〜85 秒）は、トレイの「設定」と「終了」がグレーアウトし、`/o` も拒まれる（SPEC §3.3・§8.6）。`/q` とホットキーは効く。版を上げた後の最初の起動で、全員が 1 回これを経験する
  - 構築中に `exit` すると保存の途中で止まりうる。次の起動もまた miss になる（`/s` の最中に終了するのと同じ）
  - 走査した反復はアイコンキャッシュ（`icons.bin`）を捨てる（`indexing.rs:225`）。今の同期の miss は捨てていない。版を上げただけの miss でもアイコンを取り直すことになる（無害だが、§2 の SPEC §3.4 へ反映する）
  - `scan-log.jsonl` に `start`/`end` の 2 行が増える（今の同期の miss は記録しない）。進捗の分母が育つので、むしろ望ましい

### 「履歴は実データ、索引は空」という新しい組（first-run では起きなかった）

- first-run では履歴も空なので、今の経路はこの組を通ったことがない。確かめた結果、**履歴が索引に依存して書き換わる経路は無い**:
  - `HistoryStore::prune`（`history.rs:358-375`）は起動回数の上位 `top_n` で間引くだけで、索引を見ない
  - `Engine::prepare_history_flush`（`engine.rs:248-251`）は `top_n` を config から取り、`history.prepare_flush` へ渡すだけである。構築中に `/q` で終わっても、履歴は失われない
  - 空クエリの recent（`Engine::recent_history` → `SearchEngine::recent_history`・`search.rs:459-`）は、履歴のキーを索引と突き合わせて表示する結合にすぎない。索引が空なら表示が欠けるだけで、永続化には及ばない。しかも構築中は §4.7 のゲートで通常結果ごと隠れる
- 構築の完了後の再検索は、`launcher_controller/frame_stages.rs:87-92` が `index_generation` の進みを見て `run_search()` を呼ぶ（`finish_index_build` が世代を進める・`state.rs` の `finish_index_build`）。smoke-egui の見立てはこれに乗っている（確認済み）

### 本体を起動するスクリプトの母集団（`Start-SnotraProcess` / `Start-Process` / `New-SnotraVerificationProfile` の呼び手で数えた）

`git grep -l -E "Start-SnotraProcess|Start-Process|New-SnotraVerificationProfile" -- scripts` の結果は 11 ファイル。分類は「毎回 `*.bin` を消すか（＝1 回目が Deferred か）」と「起動直後の検索結果や `index.bin` の同期の書き込みに頼るか」の 2 軸で行った:

| スクリプト | Deferred を通るか | 影響 |
|---|---|---|
| `measure-memory-stages.ps1` | 実 config で miss のとき | **壊れる**（上記） |
| `bench-startup.ps1` | `-UseVerificationProfile` の 1 回目／実 config で miss のときは全回 | 130 行以降に `cache_hit` の assert は無い（`$branch` は表示だけ）。契約の検査と終端だけを見るので緑のまま。miss の回のメモリの意味は上記 |
| `smoke-startup.ps1` | 1 回目（`*.bin` が無い限り毎回） | `*.bin` の判定。5 回とも Deferred を通りうるので、どれか 1 回で `$WaitMs`（既定 1800 ms・最初の trace 以降）の間に空の走査が保存を終えれば通る。⚠️は「競合だが余裕は大きい」 |
| `smoke-egui.ps1` | 毎回 | 上記。results は構築完了の再検索で拾える |
| `visual-check-colors.ps1` | 毎回 | results を 10 s 待つ。`*.bin` の判定は smoke-startup と同型 |
| `lib/SnotraSmoke.Tests.ps1`（統合 Pester） | 848 行の It: 毎回 | `index.bin` の生成を 10 s まで待つループを持つ（873-878 行）。非同期でも通る |
| 同 963 行の It（分離プロファイル・#1306） | B は壊れた config の復旧（非 first-run）で、`index.bin` が無いので Deferred。**既定の走査対象で背景構築が走る** | 今は B の起動が既定の走査対象を同期で走査してから `hotkey:registered` を出していた。変更後は走査が背景へ回るので、待ちは短くなる。転送されて終わる C は setup に届かず、走査しない見込み（single-instance の初期化順が前提・下の⚠️） |
| 同 890 行の It（キャレット） | 毎回 | 打鍵は注入しない（`docs/build-commands.md:308`）。影響なし |
| `visual-input-metrics.ps1` | 検証用プロファイル | 入力の計測。結果に頼るかは読んでいない（⚠️） |
| `manual-smoke.ps1` | 実 config | 人が操作する。miss なら構築中の表示が出るだけ |
| `occupy-hotkey.ps1` | 本体を起動しない（ホットキーを握るだけ。grep には `Start-Process` で当たった） | なし |

- `.github/workflows/smoke.yml` の `pull_request.paths` に `src-tauri/**` と `scripts/lib/**` が入っているので、この PR で smoke は自動で走る（確認済み）

### 二重起動（⚠️未確認）

- SPEC §13（732 行）は「単一インスタンスの判定は、設定・索引・履歴の読み込みより後に走る」と書いている。今は、2 つ目のプロセスが miss すると、転送して終わる前に全走査と保存を払う（しかも、プロセスをまたぐ排他は無い）。推す形では、走査は setup で起きる。single-instance プラグインがアプリの setup より前に 2 つ目のプロセスを落とすなら、2 つ目は走査しなくなる。プラグインの初期化順は確かめていない。良い方向の変化だが、SPEC の文言は真のまま

## 4. 設計上の判断点と、推す答え

- **A. 何を背景へ移すか。** 推すのは、読みは `main` に残し、miss のときの走査だけを背景へ回す形。理由: (1) cache-hit の起動は今と同じ（受け入れの比較相手を動かさない）、(2) 昇格を `main` に留めるので、`INDEX_WRITE_LOCK` の順序の論証が崩れない（§3）、(3) 背景構築は既存の `start_index_build` と drain をそのまま使い、first-run と同じ表示・ガード・記録が付いてくる。ロード全体を背景へ移す案は、(2) を壊し、cache-hit の起動にも「構築中」の一瞬を足すので退ける
- **B. 新しい入口を足すか、`load_or_scan_with_stats` に引数を足すか。** 新しい入口（読むだけ）を足し、`load_or_scan_with_stats_in` をその上に組む形を推す。`#[ignore]` の計測コード群（§1 の表）は、走査と保存まで含む今の振る舞いに頼っているので、触らない
- **C. startup の payload に分岐のフィールドを足すか。** 足さないのを推す。`first_run=false && cache_hit=false` が、そのまま「背景へ回した起動」を表すからである。足すと、`SnotraStartupContract.psm1:98` のキー一覧・Pester（`SnotraStartupContract.Tests.ps1`）・`bench-startup.ps1` の表示がすべて動く。読み手のために足すとしたら「cache_hit=false の起動は #1448 以降、走査を含まない」の 1 行（`startup.rs` の `Branch` の doc と `PERFORMANCE.md`）で足りる
- **D. 構築の完了を外から観測する口を足すか。** 足すのを推す（`SNOTRA_TRACE` の下の trace を 1 つ。たとえば `index_build:finished`、`ok`／panic を添える）。今は、構築の完了を外から知る手段が `scan-log.jsonl` か `index.bin` の mtime しかない。`measure-memory-stages.ps1` の修理と、受け入れの「構築が終わると検索できる」を実測するのに要る。ただし `src-tauri/CLAUDE.md`「trace の presence 検査は状態の検査ではない」に従い、この trace を「検索できる」ことの証拠には使わない（検索できることは results の観測で測る）。足さずに `scan-log.jsonl` の `end` 行で待つ案もある。その場合、製品コードは変わらないが、スクリプトがプロファイルのディレクトリを解決する必要がある
- **E. Deferred の起動で PATH エントリを先にマージするか。** **既定は「触らない」**（今の first-run と同じく、空のツリーへ `scan_path_env` を当てる）。背景の構築が `build_index_from_material` でマージし直すので、結果は正しい。構築中は通常結果が隠れるので、先にマージしても見えず、費用（数 ms〜）は無駄である。それでも、FirstRun まで含めてマージを飛ばすのは #1448 の範囲外の挙動変更になるので、**やるなら別 issue** にする
- **F. `BuildRequest` は Rescan か Rebuild か。** Rescan を推す。`cache_in_sync=false` なので、どちらでも走査はする（`engine.rs:333`）。Rebuild は `must_scan` が偽なら `index.bin` を読みに行く道を持つ。Rescan のほうが意図（走査が要る）を名前で表す。テスト `rescan_callers_request_a_rescan` も Rescan を要求している
- **G. `initial_indexing` を Deferred でも true にするか。** true を推す。setup の kick までの間（フレームはまだ走らないが）に「索引は空、構築中でもない」状態を作らないためである。`try_begin_index_build` は `index_build_started` の CAS だけを見るので、`indexing=true` から始めても成功する（`state.rs:337-344` のテストが first-run の形で固定している）
- **H. kick の位置。** 今の `setup_first_run` の位置（`finish_platform_thread` と listener の登録の後、`setup_hotkey_listener` の前）を推す。bridge と wake の listener が揃っている最初の地点だからである。`setup_startup_display` の後ろ（setup の末尾）へ置いても正しく動き、`setup_rest` の区間から spawn の費用が外れる。ただし初期の索引の処置が 2 か所に分かれる。1 か所に保つほうを取る

## 5. 必要なテスト

### snotra-core（`cache/tests.rs`。dir を注入する入口を使う。実 `%APPDATA%` に触れない）

1. 新しい関数（`_in`）: `index.bin` が無い dir では miss を返し、**`index.bin` を書かない**。走査もしない（走査対象に 1 ファイルを置いた dir を渡し、返り値が材料を持たないことと、dir に `index.bin` が生じないことを見る）
2. 新しい関数: 現行版の `index.bin` が読めれば hit を返し、material・`cache_in_sync=true`・`stats.cache_hit=true` を返す
3. 新しい関数: 旧版を読んだら昇格の書き戻しが起き、`stats.cache_save` に昇格の時間が入る。`load_or_scan_with_stats_reports_upgrade_save_in_cache_save`（`cache/tests.rs:1074`）と同じ形を、製品が通る入口で測る（製品の入口が変わるので、配線を落とす退行を製品側で拾うため）
4. 新しい関数: `config_hash` が違う `index.bin` では miss になる（走査対象を変えて止めていた場合）
5. 既存の `load_or_scan_with_stats_in` のテスト（`cache/tests.rs:704/716/755`）は無修正で通ること。組み直しの退行検知になる

### src-tauri

6. 初期の索引の決め方を純粋関数へ切り出し（例: `InitialIndex::from(is_first_run, hit_or_miss)` と、そこから導く `initial_indexing` / `cache_in_sync` / 「setup で何をするか」）、3 分岐の表をテストする。とくに「Deferred は設定アプリを起動しない」「Deferred は `initial_indexing=true`・`cache_in_sync=false`」「Loaded は何も kick しない」
7. `rescan_callers_request_a_rescan`（`indexing.rs:588`）が、`main.rs` に増えた呼び出しでも通ること（既存。Deferred の呼び出しを 1 行で書く）
8. （任意）engine: 空の索引・`cache_in_sync=false` から `request_rescan` した drain が `must_scan=true` になること。既存の `engine.rs:724` 付近のテストが同じ形を持つか確かめ、無ければ足す

### PowerShell / 実機（カテゴリ C ほか）

9. `scripts/measure-memory-stages.ps1` の修理を実機で 1 回通す（実 config で miss → 構築完了を待つ → 再起動して hit）
10. `smoke:startup` と `smoke:egui` を手元と CI で通す（1 回目が Deferred を通る。first-run 不発・`*.bin` の生成・results の観測が緑であること）
11. 受け入れの計測: `SNOTRA_CONFIG_DIR` を temp へ向け、実 `config.toml` だけを置いて起動する（`PERFORMANCE.md`「計測と受け入れ基準」）。`startup:ready` の `post_main_ms` を、cache-hit（2 回目の起動）と比べる。あわせて、構築の完了後に実際のクエリで結果が出ることを確かめる（D の trace か `scan-log.jsonl` の `end` で完了を待つ）。結果は `PERFORMANCE.md` に「採用:」の節として残す
12. `SnotraStartupContract.Tests.ps1` は、判断点 C で payload を変えないなら変更不要。変えるならキー一覧のテストを直す

### 検査カテゴリ

- `/race-check`（worker の spawn の位置が増える）・`/state-check`（`indexing` の初期値の出所が変わる）・`/symmetric-check`（`indexing=true` を立てる新しい経路 → 戻すのは `finish_index_build`）・`/persistence-check` は形式を変えないので不要
- `docs/build-commands.md` のカテゴリ A（fmt・clippy・両 crate のテスト・`cargo doc`）・C（smoke）・`governance:check`（`SPEC.md`・docs を触る）

## 6. 残した ⚠️（未実測・未確認）

- `start_index_build` を setup で呼ぶ費用（`setup_rest` の区間に乗る）と、背景の走査が `hotkey_register` と CPU や I/O を奪い合う額。受け入れの「同じ桁」は満たす見込み
- smoke-startup・smoke-egui・visual-check-colors の `*.bin` の判定と results の観測が、同期から競合へ変わる点。余裕は大きい（空か 1 ファイルの走査）。CI で確かめる
- single-instance プラグインが、アプリの setup より前に 2 つ目のプロセスを落とすか（落とすなら、2 つ目は走査しなくなる。現状からの退行ではない）
- `visual-input-metrics.ps1` が起動直後の検索結果に頼るか（読んでいない）
