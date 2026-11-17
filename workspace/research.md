# 調査: #1456 migemo_enabled / include_path_env の切り替えで全走査し直さない

## issue の要約

`config_watcher` は `IndexInputs`（`scan` / `show_hidden_system` / `include_path_env` / `migemo_enabled`）の差分で `indexing::start_index_build` を呼ぶ。`drain_index` はどの差分でも `indexer::rebuild_and_save`（全走査 + `index.bin` の書き直し）を呼び、`start_index_build` はアイコンキャッシュも捨てる。`include_path_env` と `migemo_enabled` は走査の結果を変えないので、この 2 つだけの差分ではロード・PATH のマージ・`PrebuiltIndex::from_material` で足りる。

方針（2026-11-17 決定・issue のコメント）: 全走査し直すのは SPEC の意図ではない。切り替えで本当に変わるものだけを作り直す。

## 関連するファイル・シンボル

- `src-tauri/src/indexing.rs`
  - `start_index_build`: `mark_index_stale` → CAS（`try_begin_index_build`）→ アイコンキャッシュの無効化（`icon::invalidate_icon_cache`。CAS に勝った側だけ）→ `drain_index` を別スレッドで回す
  - `drain_index`: `begin_index_drain` で `IndexInputs` を取り、`scan_log::begin_in`（start 行）→ `begin_scan_progress` → `rebuild_and_save` → `build_index_from_material`（PATH マージ + `PrebuiltIndex::from_material`）→ `complete_index_drain` → `scan_log::append_in`（end 行）
  - `build_index_from_material`: PATH マージと `PrebuiltIndex::from_material(material, inputs.migemo_enabled)`
  - テスト: `drain_index_appends_start_before_the_scan`（ソーステキスト検査）・`start_index_build_invalidates_the_icon_cache`（ソーステキスト検査）
- `snotra-core/src/engine.rs`
  - `IndexInputs`・`Engine::{mark_index_stale, is_index_stale, begin_index_drain, complete_index_drain}`・`index_stale` フィールド
  - テスト: `fresh_engine_is_not_index_stale` ほか ledger の 4 本、`index_inputs_*` の 3 本
- `snotra-core/src/indexer/cache.rs`
  - `rebuild_and_save`（`with_index_write_lock` の中で走査・整列・保存）
  - `load_or_scan_with_stats`（キャッシュが読めればそれ、無ければ走査して保存）
  - `load_cache`（private・`Config::config_dir()` を解決）→ `load_cache_in(dir, config_hash, LegacyUpgrade)`
  - `compute_config_hash` / `scan_identity_hash`: `scan` と `show_hidden_system` だけから作る。`include_path_env` / `migemo_enabled` は入らない
  - `load_cached_entries`（`LegacyUpgrade::Skip`・corpus テスト専用・`#[doc(hidden)]`）
- `start_index_build` の呼び出し元（4 か所）
  - `config_watcher.rs`（`IndexInputs` の差分）
  - `commands/system.rs` の `rebuild_index`（`/s`。`activation.rs` から呼ばれる）
  - `main.rs` の `setup_first_run`（first-run で設定アプリの起動に失敗したとき）
  - `commands/window.rs` の settings 監視（first-run で索引が未構築のとき）
- `src-tauri/src/main.rs` の起動経路: `load_or_scan_with_stats` → PATH マージ → `Engine::from_material`
- `SPEC.md`「7.5 設定反映タイミング」: インデックス条件（スキャンパス・隠しファイル表示・`include_path_env`・migemo 有効化）は「検知時に変更を判定し、バックグラウンドで自動再構築」。走査するかは書いていない。なお「ステータスに『インデックスを再構築中…』を表示」の行が、アイコン表示の切替の行の下へずれて入っている
- `docs/adr/ADR-rescan-explicit-only.md`: 索引の更新は明示操作（`/s`・設定変更・初回構築）だけ。凍結された記録で、今回の変更で「設定変更」のうち 2 つは走査しなくなる

## 再利用できる既存のパターン

- キャッシュの照合は `config_hash`（= 走査の同一性）で行う。PATH と migemo を変えてもハッシュは変わらないので、切り替えの後も `index.bin` はそのまま「いまの走査対象の結果」として読める
- 起動経路（`main.rs`）が、まさに「キャッシュを読む → PATH マージ → 建てる」を行っている。drain の非走査の分岐はこれと同じ手順になる
- engine の ledger（stale + スナップショット + re-diff）が lost-update を塞いでいる。走査の要否も同じ ledger に載せれば、並行する要求の扱いが 1 か所に閉じる

## 技術的な制約

- `/s` は、キャッシュが読めても必ず走査しなければならない（ファイルシステムの最新を取り込む唯一の明示操作）。走査対象の変更は、ハッシュが変わるのでキャッシュの照合で自然に走査へ落ちる。first-run は `index.bin` が無いので同じく走査へ落ちる
- 非走査の分岐で `index.bin` を書かない。`LegacyUpgrade::Write` で読むと旧版の昇格で書きうるので、`Skip` で読む（起動経路が既に昇格させている。実行時に `index.bin` を書く経路は drain だけに保つ）
- `scan-log.jsonl` は走査ごとに start / end の 2 行。非走査の分岐では書かない（受け入れ条件「`scan-log.jsonl` に行が増えない」）
- アイコンキャッシュの無効化は「走査した」ときだけでよい（アイコンはパスで引くので、PATH / migemo の切り替えで古くならない）。いまは `start_index_build` が要求の受理時に撃っている
- engine のロックの内側で I/O をしない（ロードは drain スレッドのロック外で行う）

## 敵対的調査（`workspace/adversarial-1456.txt`）の採否

実機の前提: `config.toml` は `include_path_env = true`・`migemo_enabled = true`・`show_hidden_system = true`。`index.bin` は版 10（現行）。PATH にしか無い名前 7 件は `index.bin` に 0 件で、PATH エントリは保存されていない。

- **採る: `start_index_build` の呼び出しは 5 か所。** `start_index_build` 自身の finish 後の自己再 kick がある。これは要求の種類を持たないので、走査要求の旗を ledger に持たせ、再 kick は旗に触れない形にする
- **採る: first-run は「必ず走査」の側。** `is_first_run` は `config.toml` の有無だけを見るので、`index.bin` が残っているとハッシュが一致して古い索引を採りうる。`setup_first_run` と settings 監視の 2 か所は `/s` と同じ要求にする
- **採る: 実行時の `index.bin` が現行版とは限らない。** first-run は昇格しないし、昇格の保存失敗でも旧版が残る。非走査の分岐は `LegacyUpgrade::Skip` で読むので、旧版でも読めて書かない。「常に現行版」を前提にしない
- **採る: 「実行時に書くのは drain だけ」は「起動後は」に限る。** 起動経路（`load_or_scan_with_stats`）も cache-miss と昇格で書く
- **採る: SPEC.md「アイコン」の節が、索引の再構築が始まったら（初回構築・`/s`・設定変更のいずれでも）アイコンキャッシュを無効化すると定めている。** 走査したときだけ無効化するなら SPEC の変更が要る。無効化の位置は、要求の受理時（走査するか未定）から drain の走査の分岐へ移す。`start_index_build_invalidates_the_icon_cache` も直す
- **採る: 保存に失敗していると、`index.bin` が engine の索引と別の走査の結果になりうる**（`save_cache_sorted_in` は `bf.save` の失敗を `eprintln!` するだけで返さない）。A→B で保存に失敗した後 B→A へ戻すと、古い A を読みうる。対策は計画で決める
- **採る: `scan-log.jsonl` には end の無い start 行が実在する。** 受け入れの検査は行数ではなく、試験の前後の start 行の増分で数える
- **壊せなかった**: `compute_config_hash` の入力（命題 1）、`index.bin` が PATH エントリと kana 列を持たないこと（命題 2）、走査対象の変更がハッシュ不一致で走査へ落ちること、PATH / migemo の切り替えでアイコンが古くなる理由が無いこと、関連シンボルとテスト名
- ⚠️ キャッシュから読んだ材料と全走査した材料で検索結果が同じになるかは、調査役は実測していない → 計画のテストで測る

## 未解決の疑問

- `index.bin` が、いま engine が持っている索引と同じ走査の結果だと言えるか。前回の drain で保存に失敗していた場合（書けない環境・ディスク満杯）、`index.bin` は古いか、無い。無ければ走査に落ちる。古い（同じハッシュで前の走査）ときは、切り替えで一つ前の走査結果へ戻る
- 走査の要否を、要求の種類（`/s` か否か）で持つか、キャッシュの照合だけで決めるか。後者だけでは `/s` を表せない
- `/s` の要求が、走査しない drain の実行中に届いたとき、取りこぼさないか（ledger の lost-update 対策と同じ問題）
- 非走査の drain でも status 行に「構築中」を出すか（所要は数十 ms）
