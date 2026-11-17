# #1456 独立導出（コードから・workspace/ の他ファイルは未読）

導出者: サブエージェント（2026-11-17）。コード・`SPEC.md`・`docs/` を読んで導出した。`cargo` は走らせていない。

## 0. 現状の機序（根拠）

- `src-tauri/src/config_watcher.rs:103-104` — `index_changed = IndexInputs::from_config(old) != IndexInputs::from_config(new)`。`:170-172` で `indexing::start_index_build(app)`。**4 キーを区別しない。**
- `snotra-core/src/engine.rs:87-103` — `IndexInputs { scan, show_hidden_system, include_path_env, migemo_enabled }`。
- `snotra-core/src/engine.rs:115-121` — ledger は `index_stale: bool` 1 本。`mark_index_stale`(:263) / `begin_index_drain`(:273, `Option<IndexInputs>`) / `complete_index_drain`(:289, re-diff で stale を落とす)。
- `src-tauri/src/indexing.rs:29-110` — `start_index_build`: mark → CAS → **`invalidate_icon_cache`（:51-53, CAS 成功側で無条件）** → spawn(drain) → finish → stale なら自己再 kick(:80-88)。
- `src-tauri/src/indexing.rs:145-218` — `drain_index`: 各反復で `scan_log::begin_in`(:164) → `indexer::rebuild_and_save`(:178, **常に全走査 + `index.bin` 書き直し**) → `build_index_from_material`(:193, PATH マージ + `PrebuiltIndex::from_material`) → `complete_index_drain` → `scan_log::append_in`(:212)。
- `snotra-core/src/indexer/cache.rs:301-325` — `compute_config_hash` / `scan_identity_hash` は `scan` と `show_hidden_system` だけ。`index.bin` は **PATH マージ前**の走査結果で、migemo にも依存しない（`save_cache_sorted_in`:618-650、kana 列は書かない）。⇒ 2 つの切り替えでは `index.bin` の中身は変わらない。
- `snotra-core/src/indexer/cache.rs:897` `load_cache_in(dir, hash, LegacyUpgrade)` — hash 不一致・不在・破損・v7〜v9 で `None`。
- 呼び出し元（`start_index_build`）: `config_watcher.rs:171`（設定変更）/ `commands/system.rs:20`（`rebuild_index` = `/s`、`activation.rs:321`）/ `commands/window.rs:169`（first-run: 設定アプリ終了後）/ `main.rs:473`（first-run: 設定アプリ起動失敗のフォールバック）/ `indexing.rs:87`（自己再 kick）。
- first-run は `main.rs:229-233` で空ツリーから Engine を建て、`initial_indexing=true`。

## 1. 変更が必要なファイルとシンボル

### snotra-core

| 対象 | 変更 | 根拠 |
|---|---|---|
| `engine.rs` 新型 `IndexDemand`（例: `Rebuild` / `Rescan`） | 要求の種類。`Rescan` = 全走査、`Rebuild` = `index.bin` ロード + PATH マージ + 構築 | 新設 |
| `engine.rs` `IndexInputs` に分類関数（例: `fn demand_since(old: &IndexInputs, new: &IndexInputs) -> Option<IndexDemand>`、または `scan_identity_differs`） | `scan` か `show_hidden_system` が変われば `Rescan`、`include_path_env`/`migemo_enabled` だけなら `Rebuild`、同じなら `None`。**判定の正本を 1 か所に**（`scan_identity_hash` の入力と同じ 2 キー） | engine.rs:87、cache.rs:301 |
| `engine.rs` `Engine` に ledger フィールド追加（推奨: `rescan_requested: u64` と `rescanned_through: u64` の世代 2 本） | 走査要求の lost-update を塞ぐ（§3-B） | engine.rs:121 |
| `Engine::mark_index_stale` | `mark_index_stale(demand: IndexDemand)` へ。`Rescan` なら `rescan_requested += 1` | engine.rs:263 |
| `Engine::begin_index_drain` | `Option<IndexInputs>` → `Option<DrainPlan>`（`inputs` + `rescan: bool` + `rescan_gen: u64`） | engine.rs:273 |
| `Engine::complete_index_drain` | 引数を `&DrainPlan` へ。`plan.rescan`（または実際に走査した）なら `rescanned_through = max(.., plan.rescan_gen)`。stale を落とす条件に「`rescan_requested == rescanned_through`」を足す | engine.rs:289 |
| `engine.rs` `//!`(:7-13)、`IndexInputs` doc(:76-86)、`index_stale` doc(:117-120)、`mark_index_stale` doc(:261-262) | ledger の記述を 2 種の要求へ | 同 |
| `engine.rs` tests(:531-) | 既存 4 本は新シグネチャへ。新規は §4 | engine.rs:534-585 |
| `indexer/cache.rs` 新関数（例: `pub fn load_saved_material(scan, show_hidden_system) -> Option<IndexMaterial>` + `_in(dir, ..)`） | **走査しない**ロード。`load_cache_in(dir, compute_config_hash(..), LegacyUpgrade::Skip)` を呼び `material` を返す。`config_dir` 不在なら `None` | cache.rs:897, 770 |
| `indexer.rs` の `pub use cache::{..}`(:23-27) | 新関数を再輸出 | indexer.rs:23 |
| `indexer/cache.rs` `LegacyUpgrade` doc(:757-764)・`load_cache_in` doc(:885-896)・`INDEX_WRITE_LOCK` doc(:1092-1101)・`LoadCacheResult::upgrade_save` doc(:739-744) | 「製品の呼び出し元は `main` の起動段の 1 つだけ」系の文は、`Write` を使えば偽になる。`Skip` を使えば「`Skip` は corpus テスト用」の文(:760-763)が偽になる。どちらかを直す（§3-F） | 同 |

### src-tauri

| 対象 | 変更 | 根拠 |
|---|---|---|
| `indexing.rs` `start_index_build(app)` | `start_index_build(app, demand: IndexDemand)` へ。`mark_index_stale(demand)` | indexing.rs:29-33 |
| `indexing.rs` 自己再 kick(:87) | `IndexDemand::Rebuild`（最弱）を渡す——走査要求は既に ledger に残っている | indexing.rs:80-88 |
| `indexing.rs` アイコン無効化(:39-53) | **drain の走査分岐へ移す**（推奨・§3-E）。`start_index_build` から外す | indexing.rs:51 |
| `indexing.rs` `drain_index`(:145-218) | 反復の先頭で `plan.rescan` を見る。`false` なら `indexer::load_saved_material(..)` を試し、`Some` なら scan-log を書かずに `build_index_from_material` → complete。`None`（不在・hash 不一致・破損・v7〜v9・config_dir 無し）または `plan.rescan` なら、今の手順（`begin_in` → アイコン無効化 → `rebuild_and_save` → `append_in`）。complete には「実際に走査したか」を渡す | indexing.rs:145 |
| `indexing.rs` `//!`(:1-7)・`start_index_build` doc(:20-28)・`drain_index` doc(:137-144)・drain 内コメント(:184-188)・`build_index_from_material` doc(:112-121「この crate ではこの drain ループの 1 つに閉じている」) | 2 分岐を反映 | 同 |
| `indexing.rs` tests `drain_index_appends_start_before_the_scan`(:359-393) | **そのまま通るように書く**: `rebuild_and_save(` をちょうど 1 回、`begin_in(` をその前にちょうど 1 回。drain 本体に `"` を入れない（:368 の assert） | indexing.rs:368-392 |
| `indexing.rs` tests `start_index_build_invalidates_the_icon_cache`(:395-432) | 無効化を移すなら、対象を `drain_index` の本体へ替え「`invalidate_icon_cache(` が `rebuild_and_save(` の直前（同じ分岐）にある」を測る形へ書き直す。doc(:395-418) も | 同 |
| `config_watcher.rs` `apply_config_change`(:103-104, :167-172) | `index_changed: bool` → `IndexInputs::demand_since(old, new)`（`Option<IndexDemand>`）。`Some(d)` なら `start_index_build(app, d)` | config_watcher.rs:103 |
| `commands/system.rs` `rebuild_index`(:20) | `IndexDemand::Rescan` | `/s` |
| `commands/window.rs:169` | `IndexDemand::Rescan`（first-run） | |
| `main.rs:473` `setup_first_run` | `IndexDemand::Rescan` | |
| `main.rs` first-run（:229-233 付近、`Engine::from_material` の後・`AppState::new` の前） | first-run なら ledger に走査要求を**種まき**（例: `engine.mark_index_stale(IndexDemand::Rescan)` 相当、または専用 `request_rescan()`）——設定アプリの保存で config_watcher が先に `Rebuild` を kick しても走査になるように（§3-D） | main.rs:229, 298 |
| `icon.rs` `invalidate_icon_cache` doc(:183-186「`indexing::start_index_build` に呼ぶ」「呼び出し元はここ 1 つ」) | 呼び出し元の名前を更新 | icon.rs:183 |
| `scan_log.rs` `//!`(:18-21「構築 1 回（`drain_index` の 1 反復）につき」) | 「走査を伴う反復につき」へ | scan_log.rs:20 |
| `state.rs` `begin_scan_progress` 呼び出し | ロード分岐でも呼ぶか決める（推奨: ロード分岐は `begin_scan_progress(None)` を呼ぶか、呼ばない。呼ばなければ同じ drain の前反復の数字が status 行に残りうる → **呼ぶ（None）を推奨**） | indexing.rs:174, state.rs |
| `egui_shell/launcher_controller/activation/tests.rs:757` | `start_index_build_invalidates_the_icon_cache` を名指ししているので改名に追随 | grep |

## 2. 変更で偽になる散文

- `SPEC.md:430-431`（§7.5）「インデックス条件（スキャンパス・隠しファイル表示・`include_path_env`・migemo 有効化）: 検知時に変更を判定し、バックグラウンドで自動再構築」→ 2 群に分ける（走査対象〔スキャンパス・隠しファイル〕は再走査、`include_path_env`・migemo は保存済み索引から建て直し・走査しない）。**仕様変更として扱う**（issue の決定）。
- `SPEC.md:432-433` — 「ステータスに『インデックスを再構築中…』を表示」が**アイコン表示の切替の子項目**に置かれている（既存の位置ずれ。本来はインデックス条件の子）。触るついでに直す候補。
- `SPEC.md:81`（§3.3）「索引が更新される契機は 3 つ: 初回構築・`/s`・設定変更による再構築」——真のまま。ただし「走査を伴うのは初回・`/s`・走査対象の変更」を書き添える候補。
- `SPEC.md:99`（§3.4）「**索引の再構築が始まったとき**（初回構築・`/s`・設定変更のいずれの契機でも）: メモリ内キャッシュと `icons.bin` の両方を無効化」→ 無効化を走査時だけにするなら偽。「走査を伴う再構築が始まったとき」へ。
- `SPEC.md:776-778`（§13.4）「索引の構築（初回構築・`/s`・設定変更）の所要」「構築 1 回ごとに、走査の前に開始の行を…」→ 「走査を伴う構築」へ。
- `docs/architecture.md:100` — 「`config_watcher` が `IndexInputs` 差分で `start_index_build` を kick → ロック外ビルド」→ 要求の種類（走査/建て直し）と ledger の走査世代に触れる。
- `docs/build-commands.md:285` — 「索引の構築（`/s`・設定変更・初回構築）ごとの所要は…`scan-log.jsonl`」→ 「走査を伴う構築」へ。
- `src-tauri/CLAUDE.md:19` — 「`scan_log.rs` — 索引の構築ごとの所要」→ 「走査ごと」へ（軽微）。
- `snotra-settings/CLAUDE.md:180` — 「監視スレッドがプロセス終了を検知 → `start_index_build` を開始」——関数名を保てば真のまま。
- `PERFORMANCE.md:1750` — 「今は `start_index_build` が判定なしで撃つ」→ 無効化を移すなら偽（記録文書だが現在形）。加えて「採用: …」の節を 1 つ足す（`show_icons` を外した節:1763 と同種の先例）。
- コード内 doc（§1 の表に含めた）: `engine.rs` `//!`・`IndexInputs`・`index_stale`・`mark_index_stale`、`indexing.rs` `//!`・`start_index_build`・`drain_index`・`build_index_from_material`・drain 内コメント・テスト doc、`icon.rs:183-186`、`scan_log.rs:20`、`cache.rs` の `LegacyUpgrade` / `INDEX_WRITE_LOCK` / `load_cache_in` / `upgrade_save` の「製品の呼び出し元は起動段だけ」系、`cache.rs:290-294`（「索引の更新は明示操作だけ…これが自動で走る唯一の場所」は真のまま）、`state.rs:35`（「判断は engine の `index_stale` ledger」→ 真のまま、ledger の中身が増えるだけ）。
- **UI 文言（概念語で探して該当なし）**: `snotra-settings/src` と `src-tauri/src` の `.rs` を「再構築|再走査|再スキャン|rebuild」で grep し、PATH・migemo のトグル付近を読んだ。トグルのラベルと hint（`snotra-settings/src/i18n.rs` の `CbIncludePathEnv` / `CbMigemoEnabled` / `HintMigemo`、`tabs/search.rs:35-40, 105-111`）は再構築に触れていない。トレイの「インデックス再構築中」（`platform/tray.rs:341`）は `indexing` 旗に従うので、ロード分岐でも旗を立てるなら真のまま。
- `snotra-core/tests/path_query_cost.rs:413`・`:249` など「`migemo_enabled` は索引の構築入力である（`IndexInputs`）」——真のまま。

## 3. 設計上の判断点と推す答え

### A. 走査の要否をどこで決めるか
- 候補 1（推す）: **要求者が種類を宣言し、ledger が世代で持つ**。config_watcher は `IndexInputs::demand_since(old,new)` で分類（scan/show_hidden が違えば `Rescan`）。`/s`・first-run は `Rescan`。drain は `begin_index_drain` が返す `plan.rescan` に従う。
- 候補 2: ledger が「いまの索引の基になった走査の同一性」を持ち、begin で現在 config と比べる（+ `/s` 用の世代）。first-run は同一性 `None` で自然に走査になる。要求の分類を呼び出し側に置かずに済むが、`Engine::from_material` に同一性を渡す口が増える。
- 候補 3: 「とにかく `index.bin` を current hash でロードし、外れたら走査」だけで決める（`/s` だけ強制）。最も小さいが、走査対象を A→B→A と戻したとき B の保存が失敗していると古い A を読む・first-run に同 hash の `index.bin` が残っていると読む、の 2 点で「走査対象の変更・初回起動は走査する」が崩れる。**ただし候補 1 でも「ロードが外れたら走査」のフォールバックは必須**（§3-C）。

### B. 並行する要求の取りこぼし（race）
- `rescan` を `bool` で持って begin で消すと、反復が unwind panic したとき走査要求が消える。complete で無条件に消すと、走査中に届いた走査要求を飲む。⇒ **世代 2 本**（`rescan_requested` / `rescanned_through`）。begin が `rescan = requested > through`、`rescan_gen = requested` を写す。complete は走査した場合だけ `through = max(through, plan.rescan_gen)`。**stale を落とすのは `inputs` 一致かつ `requested == through` のとき**。
- 列挙したウィンドウ:
  1. ロード反復の最中に `/s`——`/s` は `ensure_not_indexing` で拒否される（system.rs:10-14）ので ledger に来ない。来るのは config_watcher の `Rescan`（走査対象の変更）で、そのときは inputs も違うので既存 re-diff でも残る。世代はその上の保険。
  2. config_watcher の `update_config`(config_watcher.rs:152) と `mark`（start_index_build 内）の間に begin が挟まる——新 inputs で `rescan=false` の計画になる。ロードは hash 不一致で外れて走査へ落ちる（正しい）。外れなかった場合も complete で `requested > through` ゆえ stale が残り、次反復で走査する。
  3. finish ウィンドウ（complete 後・finish 前の要求）——既存の自己再 kick が拾う。再 kick は `Rebuild` を渡しても ledger の世代が走査を選ぶ。
  4. CAS 失敗側の要求——mark は CAS の前（indexing.rs:33）なので ledger に載る。種類も載る。
- **簡素化の争点**: 「ロードが外れたら走査」のフォールバックがあるので、世代 2 本でなく `bool` 1 本（begin で写し、走査を完了した complete でだけ落とす）でも足りるかもしれない。`bool` で失うのは「走査中に届いた 2 回目の走査要求」で、これを送るのは config_watcher の走査対象の変更だけであり、そのときは inputs が違うので re-diff が stale を残す（`/s` は `ensure_not_indexing` が拒む）。first-run の種まきも同じ。ただし「走査中に `/s` 以外の経路で同じ inputs の走査要求が来る」経路が将来増えると黙って落ちる。**世代を推すが、`bool` も成立する**——呼び出し元の計画と突き合わせる点。
- 二重起動防止の `AtomicBool` 2 本（state.rs）は変えない。判断を ledger に閉じる方針（engine.rs:7-13）を保つ。

### C. 保存に失敗した後の `index.bin`
- `rebuild_and_save` は `bf.save` の失敗を `eprintln` だけで返す（cache.rs:644-646）。失敗後にロード分岐へ入ると、(i) hash が違えば外れて走査（正しい・費用だけ）、(ii) **同じ hash の古いファイルが残っていれば、いまメモリにある索引より古い走査結果へ巻き戻る**（検索結果はもっともらしいまま＝沈黙）。
- 推す答え: **(ii) を受容する残余として明記する**。理由: 起動経路が既に同じ前提（「`index.bin` が正本・保存失敗後の次回起動は古いファイルを読む」）で動いており、切り替えはそれに揃うだけ。保存失敗は稀（読み取り専用・満杯）。守るなら `rebuild_and_save` に保存成否を返させ、ledger（または drain 側）に「保存済みの基とメモリの基が一致するか」の旗を持ち、不一致なら `Rebuild` を `Rescan` へ格上げする（起動経路 `load_or_scan_with_stats` の cache-miss 保存失敗と `config_dir` 不在も同じ旗へ）。配管 3 か所ぶんの費用。**どちらを採るかは争点としてユーザーに上げる。**
- `config_dir` が引けない環境: ロードは常に `None` → 走査へ落ちる（今と同じ費用、正しい）。

### D. first-run
- first-run は Engine が空ツリーで建ち、`index.bin` は通常不在→ロードは外れて走査になる。ただし config.toml だけ消えて `index.bin` が残る環境で、設定アプリの保存が migemo/PATH だけ変えると config_watcher の `Rebuild` が先に drain を起こし、`window.rs:169` の kick は `is_index_build_started` で落ちる（または `is_indexing=false` で落ちる）→ **走査せずに古い `index.bin` を読む**。
- 推す答え: `main.rs` の first-run 分岐で ledger に `Rescan` を種まきする。kick の経路（window.rs / main.rs:473）も `Rescan` を渡す。
- 種まきで stale が立っても無害である（確定）: 製品で `is_index_stale` を読むのは `indexing.rs:80-85`（finish 後の再チェック）だけ、`begin_index_drain` を呼ぶのは `drain_index`（indexing.rs:150）だけで、どちらも drain スレッドの内側にしか居ない（冒頭の grep 結果）。drain が起きるまで誰も読まない。

### E. アイコンキャッシュの無効化
- 2 つの切り替えはファイルシステム上のアイコンを変えない（PATH OFF で消えたエントリのアイコンは FIFO が掃除する）。無効化は不要。
- `start_index_build` に置いたまま種類で分岐すると、ロード反復が走行中に届いた `Rescan`（CAS 失敗側）で無効化が撃たれない。⇒ **推す答え: 無効化を `drain_index` の走査分岐（`rebuild_and_save` の直前）へ移す**。走査 1 回につき 1 回撃つので、「CAS 成功側だけ」「自己再 kick で 2 回撃つ」の注記(indexing.rs:46-49)も消える。代償: テスト `start_index_build_invalidates_the_icon_cache` と `SPEC.md` §3.4・`icon.rs` doc・`PERFORMANCE.md:1750` の書き換え。drain の本体に `"` を入れない制約に注意（`drain_index_appends_start_before_the_scan` :368）。
- 意味の変化: いまは「走査の**開始前**（要求受理の瞬間）」に捨て、新形は「走査の直前」。間に検索で再抽出されたアイコンを捨てる点は同じ。

### F. `index.bin` の書き手・ロードの `LegacyUpgrade`
- ロード分岐で `LegacyUpgrade::Write` を使うと、`upgrade_legacy_cache_in` の「読みがロックの外」ウィンドウの前提「製品でこの経路を通るのは `main` の起動段だけ」(cache.rs:739-744, 1094-1099) が崩れる（実害は無い: 実行時のもう一方の書き手は同じ drain スレッドで逐次）。
- 推す答え: **`LegacyUpgrade::Skip` を使い、切り替えは `index.bin` を一切書かない**。書き手の集合（`rebuild_and_save` / cache-miss 保存 / 起動時の昇格）が変わらない。旧版が残るのは起動時の昇格が失敗した場合だけで、その場合は毎回読み直すだけ。`LegacyUpgrade` の doc（`Skip` は corpus テスト用、:757-763）に製品の 2 つ目の用途を足す。
- **Skip の残余**: `finish_legacy_read` の `Skip` 分岐（cache.rs:870-886 付近）は `sort_entries_canonical` を通さずに `IndexTree::build` する（`Write` 側の `upgrade_legacy_cache_in` は通す）。実行時に旧版が残るのは起動時の昇格保存が失敗した場合だけだが、そのとき並びと `sorted_by_path` が全走査の場合と一致しないことがある。受容する残余として 1 行書くか、Skip 分岐でも整列するかを決める。
- `INDEX_WRITE_LOCK` はロード分岐では取らない（`load_cache_in` doc :885-896 の「保持したまま呼ぶな」に従う。Skip なら書かないので無関係）。

### G. scan-log・進捗・UI
- ロード分岐では `scan_log::begin_in` / `append_in` を呼ばない（受け入れ「行が増えない」）。`scan_log.rs` の 2 行 1 組の規約は「走査する反復」に限定して保たれる。
- `indexing` 旗・`notify_indexing_started/complete`・`index_generation` は今のまま通す（数十〜数百 ms の間、§4.7 のゲートで結果が隠れるが、索引を差し替える間の一貫性は今と同じ）。ロード分岐の所要を別に測って記録する仕組みは置かない（やりすぎ）。

### H. `build_index_from_material` と起動経路の二重実装
- 変えない。ロード分岐も同じ `build_index_from_material` を通すので、PATH マージの規則は drain 内では 1 か所のまま（起動経路 main.rs:267 の別実装は既存）。

## 4. 必要なテスト

### snotra-core（単体・ledger は純粋に測れる）
1. `IndexInputs::demand_since` の分類: `scan` 変更→`Rescan`、`show_hidden_system`→`Rescan`、`include_path_env` のみ→`Rebuild`、`migemo_enabled` のみ→`Rebuild`、両方→`Rebuild`、scan + migemo→`Rescan`、無関係キー（`visible_rows`・`show_icons`）→`None`。既存 `index_inputs_differ_on_each_index_key` / `_ignore_show_icons_*` と並べる。
2. ledger: `mark(Rebuild)` → begin は `rescan=false`；`mark(Rescan)` → `rescan=true`；`Rescan` 後に `Rebuild` を重ねても `rescan=true`（格下げしない）。
3. ledger の lost-update: begin(rescan=false) の後に `mark(Rescan)` → complete（inputs 同じ）で stale が**残り**、次の begin が `rescan=true`。
4. 走査中に別の `Rescan`: begin(rescan, gen=1) → mark(Rescan, gen=2) → complete → stale 残る。
5. 走査が完了すれば stale も rescan も落ちる；panic を模して complete しなければ次の begin も `rescan=true`。
6. 既存 `complete_index_drain_keeps_stale_when_config_changed_during_build` 等を新シグネチャで維持。
7. `load_saved_material_in(dir, ..)`: 保存済み（`rebuild_and_save` 相当で書いた tmp dir）なら `Some`・**走査しない**（`dirs` カウンタ 0、または scan 対象ディレクトリを消してからでも読める）；hash 違い→`None`；不在→`None`；壊れ→`None`；**`index.bin` を書き換えない**（旧版を置いて mtime/バイト不変）。
8. A/B 同値（受け入れ「検索結果は全走査と同じ」）: 同じ fixture ツリーで (a) 走査→PATH マージ→`from_material(migemo)` と (b) 保存→ロード→同じマージ→`from_material` の索引が一致（件数・`entry_name`・`sorted_by_path`・代表クエリの検索結果）。`include_path_env` × `migemo_enabled` の 4 組。既存 `search/tests/build.rs:422-` の A/B 検知器の形を流用。

### src-tauri（`AppHandle` が要るので drain は単体で回せない）
9. `drain_index_appends_start_before_the_scan` を緑のまま（`rebuild_and_save(` と `begin_in(` が本体に 1 回ずつ・順序）。
10. アイコン無効化の検査を drain の走査分岐へ移す（`invalidate_icon_cache(` が `drain_index` の本体にあり、`rebuild_and_save(` より前）。存在形の assert のまま（`!contains` にしない）。
11. **受け入れ「2 つの切り替えで走査しない」の自動検知器が要る**（ここが最大の穴）。テスト 2〜5 は ledger、7 はローダを測るが、「`plan.rescan == false` なら drain がロード分岐を選ぶ」という接着部は手動スモーク 13 しか測らない。退行して常に走査へ落ちても検索結果は同じなので全テストが緑のまま——`index_inputs_ignore_show_icons_in_both_directions` の doc が警告するのと同じ静かな壊れ方である。推す形: 「ロードを試し、外れたら走査」の選択を `AppHandle` なしで呼べる純関数へ切り出す（例: snotra-core `indexer::material_for(dir, scan, show_hidden, rescan: bool, dirs) -> (IndexMaterial, Option<RebuildStats>)` のような dir 注入版、または src-tauri 側の `fn choose_material(..)`）。テスト: tmp dir に保存済み `index.bin` を置き、`rescan=false` で呼ぶと **`dirs` カウンタが 0 のまま・`index.bin` のバイト列不変・stats が `None`**、`rescan=true` なら走査する、hash 不一致なら走査へ落ちる。**制約**: `drain_index_appends_start_before_the_scan` は `scan_log::begin_in(` と `indexer::rebuild_and_save(` が `drain_index` の本体にちょうど 1 回ずつ、この順で現れることを要求する（indexing.rs:378-392）。走査をヘルパーへ移すと綴りが本体から消えて赤になるので、(a) 判定だけを切り出して `rebuild_and_save(` の呼び出しは drain 本体に残す（判定関数 = 「ロードを試す/走査すべきか」を返すだけ）、または (b) その検査の対象と綴りを同じ変更で書き直す。**推すのは (a)**（`start` 行を走査の前に書く順序の検査を壊さない）。(a) の場合の検知器は「`rescan=false` かつ保存済みなら Some(material) を返し走査しない」をローダ側で測り、drain 本体が `plan.rescan` と判定結果で `rebuild_and_save(` を飛ばすことはソーステキスト検査（`rebuild_and_save(` が判定の `None`/rescan 分岐の内側にある）で補う。どちらも置かないなら「検知器の無い残余」として失うもの（切り替えのたびに全走査へ黙って戻る）を名指しで書き、争点に上げる。
12. config_watcher は分類関数を呼ぶだけにして、分類のテストは core 側（1.）に置く。

### 手動・スモーク（受け入れ）
13. 実機で `include_path_env` / `migemo_enabled` を切り替え、`scan-log.jsonl` の行数が増えない・`index.bin` の mtime が変わらない・`icons.bin` が消えない・PATH のコマンドが出る/消える・ローマ字検索が効く/効かない。
14. `/s`・スキャンパス変更・隠しファイル切替・first-run で従来どおり `scan-log.jsonl` に 2 行増える。
15. `/state-check`・`/race-check`（issue の受け入れ）と、`index.bin` 書き手を触らないことの `/persistence-check`（Skip を採る場合は形式変更なし）。
