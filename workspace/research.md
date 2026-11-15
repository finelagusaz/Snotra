# 調査: issue #1041 明示スキャンの所要を測る計器と進捗表示

## issue の要約

- 明示スキャン（`/s` = `SlashCmd::RebuildIndex`）の所要が、製品のどこにも記録されない。#580 の検証中に同じバイナリ・同じ索引で 29.3 秒と 82.8 秒（2.8 倍）が観測されたが、`SNOTRA_TRACE` にも残らず、trace 時刻と `index.bin` の mtime で外から挟んで採った。
- `rescan-log.jsonl` の書き手は背景再スキャンと一緒に撤去された（2e1b8c8b・#1023・`ADR-rescan-explicit-only`）。明示経路には計器が無い。
- 所要は FS キャッシュの温度で ±13 秒級に揺れる（#977）。1 標本で「遅い」と判定しない設計にする。
- 計器を置くなら、発火しうるかを走る場所で先に測る。

### 要求の判断（ユーザー回答・2026-11-15・このセッション）

1. 記録の置き場所: **永続 JSONL ファイル**（`%APPDATA%\Snotra` に置く。`SNOTRA_TRACE` だけにはしない）
2. 進捗の提示: **本 issue に含める**
3. 進捗の形: **件数 / 前回の件数**（例「インデックス構築中... 128,000 / 約 312,000 件」。分母はいまメモリにある索引の件数。分母が 0 なら件数だけ。残り時間は出さない）

## 関連するファイル・モジュール・関数

### 構築の経路（src-tauri）

- `src-tauri/src/indexing.rs`
  - `start_index_build(app) -> bool`: ビルド要求の単一入口。`mark_index_stale` → `try_begin_index_build`（CAS）→ アイコン無効化 → `notify_indexing_started` → スレッド `snotra-index-build` で `drain_index` を `catch_unwind` で包んで走らせ、`finish_index_build` → `notify_indexing_complete` → stale なら自己再 kick。
  - `drain_index(app)`: `begin_index_drain` が `Some(inputs)` を返す間ループ。1 反復 = `indexer::rebuild_and_save`（走査・整列・保存）→ `build_index_from_material`（PATH マージ + `PrebuiltIndex::from_material`）→ `complete_index_drain`（swap）。
  - 呼び出し元（grep）: `commands/system.rs` の `rebuild_index`（`/s` とトレイ）、`config_watcher.rs`（設定変更）、`commands/window.rs`（初回起動の monitor 経路）、`main.rs` の `setup_first_run`（設定アプリ起動失敗時）、`indexing.rs` 自身（自己再 kick）。
- `src-tauri/src/egui_shell/launcher_controller/activation.rs` の `execute_slash`: `SlashCmd::RebuildIndex` は `emit_hide()` してから `commands::rebuild_index` を呼ぶ。**`/s` の直後、ランチャーは隠れている**。
- `src-tauri/src/state.rs`: `indexing` / `index_build_started` の `AtomicBool`、`try_begin_index_build` / `finish_index_build` / `is_indexing` / `is_index_build_started`。初回起動では `is_indexing()` が真でもビルド未開始の期間がある（`commands/window.rs` の `!is_index_build_started()`）。

### 構築の中身（snotra-core）

- `snotra-core/src/indexer/cache.rs`
  - `rebuild_and_save(scan, show_hidden_system) -> IndexMaterial`: `with_index_write_lock` の中で `scan_all` → `sort_entries_canonical` → `compute_config_hash` → `save_cache_sorted`。区間を測っていない。
  - `scan_and_sort_timed(scan, show_hidden_system) -> Scanned { entries, scan, sort }`: cache-miss 分岐（`load_or_scan_with_stats` 系）用に走査と整列を測る private 関数。doc は「保存する分岐と保存しない分岐が同じここを通る。書き起こすと計測コードが 2 部出荷になる」。`rebuild_and_save` はこれを通っていない（`scan_all` を直接呼ぶ）。
  - `LoadOrScanStats`: 起動時ロード経路の区間（`scan` / `sort` / `cache_save` ほか）。起動計器 `src-tauri/src/startup.rs` へ運ばれる（`main.rs` の `load_or_scan_with_stats` 呼び出し）。
- `snotra-core/src/indexer/scan.rs` の `scan_all(scan_paths, show_hidden_system) -> Vec<AppEntry>`: 根ごとに `scan_directory_with_extensions`（再帰・`read_dir`）。進捗を外へ出す口は無い。
  - 呼び出し元（grep）: `cache.rs` の `scan_and_sort_timed` と `rebuild_and_save`、`icon.rs`（`Err(_) =>` の分岐）、テスト（`indexer/scan/tests.rs` 3 件・`search/tests/common.rs`）、計測コード `tests/memory_footprint.rs`。
- `snotra-core/src/engine.rs`: `begin_index_drain()`（stale なら `IndexInputs` を返す）、`entry_count()`（現在の索引件数）、`complete_index_drain`。

### 表示（src-tauri/egui_shell）

- `view.rs`: status 行。`indexing_raw = self.controller.indexing().get()` を **フレームで 1 回だけ読む**（#904・#1077 の規約）。`overlay_kind(indexing_raw && is_results, launching_now, has_notice_now)` が `Indexing` のとき `ui_strings::indexing_hint(lang)` を出す。
- `strings.rs` の `indexing_hint(l) -> &'static str`: 「インデックス構築中...」/「Building index...」。引数に件数を取らない。
- `mod.rs` の `register_config_wake_listeners`: `INDEXING_STARTED` / `INDEXING_COMPLETE` を wake の合図として消費。「値を運ばない」が benign 性の前提（doc に明記）。
- 走査中に status 行を描き直す wake は無い（開始と完了の 2 回だけ）。
- `src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」: hidden 中は `update()` が走らない。`request_repaint_after` は可視中しか効かない。再要求してよいのは時間経過で解消する不成立だけ。
- トレイ: `platform/mod.rs` の `SetIndexing(bool)` は `indexing_in_progress` を立てるだけ（メニューのグレーアウトと文言）。

### 撤去された計器（参考・再利用の候補）

- `git show 2e1b8c8b^:snotra-core/src/rescan_log.rs`: 純粋核（`start_line` / `end_line` / `unattributed`）と best-effort の書き手（`append_in` / `prune_in`・200 行上限・末尾を残す）・`new_sid`（pid + ミリ秒）。serde_json と chrono を使っていた。
- 設計 `docs/superpowers/specs/2026-08-09-rescan-in-situ-instrument-design.md`（凍結層・直さない）: 2 行 1 組（start を走査の前に書く → end が無ければ未完走）、`null` と `0` の区別、`total` は終端で 1 回読み `unattributed` で恒等式を閉じる、丸めは表示境界だけ、best-effort、剪定は start の前に 1 回。§8.1（trace だけにする案の却下）と §8.2（計器を振る舞いの入力に兼ねさせる案の却下・冒頭追記で「今も有効」）。
- `SPEC.md` から §13.4 は撤去で消えている（現行の §13 は 13.1〜13.3）。

## 再利用できる既存のパターン

- 起動計器 `src-tauri/src/startup.rs`: 時計を持たない純粋核、区間は `Duration` で持ち出力時だけ ms へ切り捨て、通らなかった区間は `null`、残余は名前付きの項目。
- `scan_and_sort_timed`: 走査と整列の区間を測る唯一の場所。`rebuild_and_save` もここを通せば、区間の意味が経路ごとにずれない。
- `Config::config_dir()`: 保存先を導く唯一の経路（`SNOTRA_CONFIG_DIR` も効く）。
- `*_in(dir)` の dir 注入（`load_cache_in` / `save_cache_sorted_in`）: テストが実 `%APPDATA%\Snotra` に書かない形。
- フレーム内の値の凍結（`indexing_raw` / `visible_rows`）。

## 技術的な制約

- **serde_json は snotra-core から撤去済み**（2e1b8c8b で `Cargo.toml` から削除）。src-tauri には在る。chrono は snotra-core に在り、src-tauri には無い。
- release は `panic = "abort"`: 計器の失敗で製品を落とさない（書き込み失敗は握り潰す）。
- 走査のホットループ（`scan_directory_with_extensions`）へ何かを足すと、確保や所要の退行は CI に映らない（`Dedup::accept` の doc）。足すなら `tests/memory_footprint.rs` の手動計測で確かめる。
- `/s` は hide してから走るので、走査中に status 行が見えるのはユーザーがランチャーを開き直したときだけ。hidden 中はフレームが走らない。
- 走査の後（整列・保存・PATH マージ・`PrebuiltIndex` 構築）の数秒は件数が進まない。
- drain ループは 1 回の要求で複数反復しうる（ビルド中の設定変更）。自己再 kick もある。CAS に負けた要求は走行中のビルドに吸収される → **契機（`/s`・設定変更・初回）を記録へ帰属させる規則が自明でない**。
- 起動時の cache-miss 経路（`load_or_scan_with_stats`）は `AppHandle` 以前で、起動計器が既に区間を持つ。
- 新しい永続ファイルは SPEC の変更（§13 に節を足す）。ただし振る舞いに影響しない使い捨ての物体であり、`binfmt` の仲間ではない（§13.2 に混ぜない）。
- テストは実 `%APPDATA%\Snotra` に触れない（`snotra-core/CLAUDE.md`「開発ルール」）。`rebuild_and_save` は `Config::config_dir()` を内部で解決するので、テストから通さない。

## 未解決の疑問

1. 1 行の単位: 「ビルド要求（`start_index_build` の受理〜`notify_indexing_complete`）」と「drain の 1 反復」のどちらか。前者がユーザーの待ちに一致し、後者が区間の内訳に一致する。
2. 契機を記録するか（帰属の規則が要る）。
3. 起動時 cache-miss 経路を対象に入れるか。
4. 純粋核と書き手を snotra-core と src-tauri のどちらに置くか（serde_json の所在・書き手の呼び出し元は drain ループ）。
5. 区間の内訳: `scan` / `sort` / `save` を core から返す形（`rebuild_and_save` の返り値を広げる／`_with_stats` を足す）。
6. 2.8 倍の変動の説明変数を記録に持たせるか（例: ビルドスレッドの CPU 時間。壁時計との差が I/O 待ち・競合を示す）。
7. 進捗の件数を `scan_all` から出す口の形（`&AtomicUsize` を引数に足す／ディレクトリごとに `entries.len()` を store する）と、他の呼び出し元への影響。
8. 分母（前回の件数）をどこで読むか（`begin_index_drain` と同じ錠の中で `entry_count()`）。索引件数は PATH エントリを含み、走査件数は含まない。設定変更で根が変わると分母は意味を失う。
9. 走査中の status 行の描き直し: 可視中だけ一定間隔で `request_repaint_after` を撃つ形が、「時間経過で解消する不成立だけ再要求する」規約に当たるか。
10. 進捗の値をフレームで 1 回だけ読む規約（`indexing_raw` と同じ凍結）と、件数のリセットの時機（前回のビルドの件数が一瞬見える窓）。

## 敵対的調査（3b）の所見と採否

全文は `workspace/adversarial-1041.txt`。sonnet 1 体。

### 壊せた項目

| 所見 | 採否 | 理由（一次証拠で裁定） |
|---|---|---|
| **登録件数は仕事量の代理にならない。** 実 `config.toml` の `[[paths.scan]]` は 5 本（調査の前提「4 本」は 8 月の構成）で、最後の `C:\` は `extensions = []`・`include_folders = false`。`matches_extension` は空配列で必ず偽なので、`C:\` の全再帰は 1 件も登録しない。`index.bin` は 574,743 B（8 月の 31 万件規模ではない） | **採る** | `config.toml` を自分で読み（mtime 2026-11-07）、`scan.rs` の `matches_extension`（`binary_search_by` が空配列で `Err`）を確かめた。**進捗の単位を「走査したフォルダ数」へ替えた。** 分母の取り方はユーザーに再度問い、「記録の直近の完走行から読む」と決まった（下の「要求の判断」4） |
| 走査中の再描画は開始・完了の 2 回だけ、は偽。egui の caret 点滅が、可視・フォーカス中は約 0.5 秒ごとにフレームを起こす | **一部採る** | 機序（egui の `paint_text_cursor`）は委譲先がソースを読んだだけで、実測していない。フォーカスの無い可視状態（`auto_hide_on_focus_lost = false`）では点滅は止まる。**計画では明示の `request_repaint_after` を残す**——点滅への依存は egui の既定値（`text_cursor.blink`）という、こちらが持たない値への依存になるため |
| issue の 29.3 / 82.8 秒の終端は `index.bin` の mtime、つまり保存の完了。ユーザーの待ちには PATH マージ・`PrebuiltIndex` の構築・swap が加わる | **採る** | `end` 行に `scan_ms` / `sort_ms` / `save_ms` / `build_ms` を分けて持たせるので、`scan + sort + save` が issue の標本と同じ区間になる。比べる区間を `docs/build-commands.md` の読み方に書く |
| status 行が出るのは Results ビューだけ（`overlay_kind` の入力が `indexing_raw && is_results`） | **採る** | `view.rs` で確かめた。受け入れ条件 6 は「status 行が構築中案内を出しているあいだ」と書いてあるので、この条件を含む。SPEC の追記も同じ書き方にする |
| `/s` は `ensure_not_indexing` で構築中なら Err で捨てられる（CAS に吸収されるのではない）。設定変更は吸収される | **採る** | 契機を記録しない判断（計画の決定 2）は変わらない。理由の文面だけ直した |
| 分母は `include_path_env` の切り替え直後も PATH 分ずれる | **採る（論点ごと消えた）** | 単位をフォルダ数に替えたので、索引件数は分母に使わない |

### 壊せなかった項目

- `scan_all` と `start_index_build` の呼び出し元の列挙（`icon.rs:835` を含む。re-export は `indexer.rs` の 1 つ）。`rebuild_and_save` の製品の呼び出し元は `drain_index` の 1 つだけ
- serde_json は core に無く src-tauri にある。chrono は core の直接依存で、src-tauri の直接依存には無い（間接には在る）
- `scan_and_sort_timed` はロックを取らず、`rebuild_and_save` の `with_index_write_lock` の内側から呼べる。呼び出しは cache-miss の 2 箇所だけ
- `entry_count` は `SearchEngine.entries.len()`。PATH マージは swap の前に入る
- 設定アプリは構築中は開けない。トレイは文言とグレーアウトだけ。`/s` は `emit_hide()` してから再構築する
- `SNOTRA_CONFIG_DIR` は未設定。`%APPDATA%\Snotra` に `rescan-log.jsonl` の残骸は無い

### ⚠️（確信が持てない）

- 索引の件数は `strings` での概算。→ 単位をフォルダ数に替えたので結論に効かない
- Folder / Tool ビューで status 行が出ないことは、コードを読んだだけ（実機の確認は計画の作業項目に含める）
- caret 点滅の再描画は未実測 → 明示の再描画を残すので結論に効かない
- カウンタを CAS 成功の直後・notify の前に 0 へ戻せば窓が閉じる（D3）→ 計画は `finish_index_build` で 0 に戻す形を採る（次のビルドの開始時点で既に 0 であり、CAS 前後の順序に依存しない）

## 要求の判断（追加・2026-11-15・このセッション）

4. 進捗の単位: **走査したフォルダ数**。分母は **`scan-log.jsonl` の直近の完走行（`end` 行）から読む**。記録が無い・壊れている・走査済みが分母を超えたときは、フォルダ数だけを出す。

## 調査の追加

- `snotra-core/src/indexer/cache.rs` の `scan_identity_hash(scan, show_hidden_system) -> u64`: `compute_config_hash` の公開の入口。doc は「呼び出し元は #1001 で撤去済みで、現在どこからも呼ばれない」。走査対象の同一性だけを表す（`include_path_env` / `migemo_enabled` を含まない）。分母を「同じ走査対象で完走した行」に限る照合に使える。使えば doc の「どこからも呼ばれない」は偽になる。
