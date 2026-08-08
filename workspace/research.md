# 調査: issue #984 — `rebuild_and_save` が保存の返り値を捨てている

## issue の要約

反復 11（#977）は cache-miss の**起動経路だけ**を「保存が返した `CachedMasks` をそのまま索引の表現に使う」形へ繋いだ。`rebuild_and_save` → `drain_index`（設定ダイアログからの再構築・背景ビルドスレッド）の枝は今も `PrebuiltIndex::from_tree` に落ちるので、**保存が計算した派生データをその場で捨てて Wave 1/2 を建て直している**。

この issue はそれを繋ぎ、`indexer.rs` の `rebuild_and_save` の doc から「意図的・受容する残余」を消す。副題として、その doc が受け皿として指す issue 番号（#979・PR #983 で閉じた別作業）を差し替える——`governance:check` は落ちない（参照先の見出し `PERFORMANCE.md`「次の反復の候補」は実在する）ため、**番号の意味だけが静かに腐る**種類のずれである。

額の SSOT は `PERFORMANCE.md`「次の反復の候補」の該当行（`PrebuiltIndex` を `CachedMasks` 込みで建てる）。ここへは写さない。

## 関連ファイル・シンボル（すべて grep で実在を確認済み）

### 変える側

| 位置 | 現状 |
|---|---|
| `snotra-core/src/indexer.rs:900` | `pub fn rebuild_and_save(scan, show_hidden_system) -> IndexTree`。中で `save_cache_sorted(entries, config_hash).0` として**タプルの第 2 要素を捨てている** |
| `snotra-core/src/indexer.rs:897-899` | その doc の「**保存が返す `CachedMasks` をここでは捨てている（意図的・受容する残余）**」3 行（issue #984 を名指し・撤去条件の対象） |
| `snotra-core/src/engine.rs:59` | `PrebuiltIndex::from_tree(tree, migemo_enabled)` → `SearchEngine::new_from_tree`。doc は「**製品はこちらを通る**」 |
| `src-tauri/src/indexing.rs:102` | `let mut tree = indexer::rebuild_and_save(...)` |
| `src-tauri/src/indexing.rs:105-108` | PATH マージ。`scan_path_env` → `tree.extend_with_roots`。**`extend_cached_masks` を呼んでいない** |
| `src-tauri/src/indexing.rs:148` | `PrebuiltIndex::from_tree(tree, inputs.migemo_enabled)` |
| `src-tauri/src/main.rs:193-204` | 起動経路の PATH マージ。`scan_path_env` → `extend_cached_masks`（`Some` のときだけ）→ `extend_with_roots`。**drain 側と同じ仕事の 2 つ目の実装** |

### 手本（変えずに倣う側）

- `snotra-core/src/indexer.rs:859` `save_cache_sorted_in(dir, entries, config_hash) -> (IndexTree, CachedMasks)`——**`Option` ではない**。doc が「書き込みの失敗は返り値に影響しない。返すのは今メモリに在る木と、その木に対して導出した派生データ」と明言している
- `snotra-core/src/indexer.rs:1624` `pub fn extend_cached_masks(masks: &mut CachedMasks, new_entries: &[AppEntry])`。3 枝（`None` / `Raw` / `Collapsed`）を持ち、per-entry 導出は `derive_entry_lowers` / `derive_entry_collapsed` を通す
- `snotra-core/src/index_tree.rs:403` `extend_with_roots(&mut self, entries: Vec<AppEntry>)`。**`entries.is_empty()` で早期 return する**（main.rs 側の `!is_empty()` ガードは木に対しては冗長）。末尾で `sorted_by_path = false`
- `snotra-core/src/engine.rs:142` `Engine::new_from_cache(tree, cached_masks, history, config)` → `SearchEngine::new_with_cached_masks`
- `snotra-core/src/search/tests/build.rs:247` `path_merge_after_cache_miss_agrees_with_deriving_over_the_extended_tree`——起動経路の A/B 検知器。doc（`:243-245`）が「**`drain_index` は今も `extend_cached_masks` を呼んでいない**……あちらを `CachedMasks` 経由へ繋ぐ日には、呼び忘れが長さの食い違いとして出る——そのときこの検知器が形の手本になる」と、**この issue の日を予告している**

## 再利用できる既存パターン

1. **`(IndexTree, CachedMasks)` を返す**——`save_cache_sorted_in` が既にその形。`rebuild_and_save` は `.0` を外すだけで第 2 要素が手に入る。
2. **`Option<&mut CachedMasks>` で「マスクを持つ経路／持たない経路」を 1 本にまとめる**——`extend_cached_masks` 自体は `&mut CachedMasks` を取り、`main.rs` が `if let Some(ref mut masks)` で包んでいる。この包みを関数側へ移せば両呼び出し点が同じ形になる。
3. **A/B 一致の検知器**——`assert_engines_agree` / `collapse_census` が `search/tests/build.rs` にあり、「変更前の導出（拡張後の木から Wave 1/2）」と「変更後（追記 → 木へ追加 → `new_with_cached_masks`）」が `entry_view` の水準で一致することを測る枠組みが既にある。

## 技術的制約

- **`assemble` の長さ検証は `debug_assert` ゆえ release では消える。** `extend_cached_masks` の呼び忘れは「マスクだけが短くなる」形で入り、`PathStore::adopt` の連鎖 `zip` が最短で黙って切る（`extend_with_roots` の doc が同じ機序を記す）。**添字 panic か沈黙の食い違い**になる。
- **`CachedMasks` は `Clone` を持たない**（`indexer.rs:66` の derive は `Debug` と test 時 `PartialEq` のみ）。`IndexTree` も `Clone` を持たない（`index_tree.rs:144`）。ゆえに 1 プロセス内で A/B を両方作る計測は、木を 2 度建てるか `Clone` を足すかを要する。
- **`scan_path_env` は実 PATH 環境変数を読む**ので、決定的なユニットテストに乗らない。共通化する単位は「スキャン」ではなく「**併合**（マスクへ追記 ＋ 木へ根として追加）」でなければ検知器が書けない。
- `PrebuiltIndex::from_tree` の呼び出し元は `src-tauri/src/indexing.rs:148` の **1 か所だけ**（grep 実測。`engine.rs` のテストが使うのは `PrebuiltIndex::new`、`snotra-core/tests/` が使うのは `SearchEngine::new_from_tree` / `Engine::new_from_tree` で別物）。移行すると呼び出し元がゼロになる。`pub` なので `dead_code` は鳴らない。
- **`INDEX_CACHE_VERSION` は据え置き**。`index.bin` のバイト形式に触らない（保存側の計算も表現も変えず、**返り値を捨てるのをやめるだけ**）。ゆえに `/persistence-check` のトリガー（永続形式・識別子/キー形式の変更）には当たらない。
- ベースライン: `cargo test -p snotra-core` は worktree 作成直後に **exit 0**（実行済み）。

## この変更が当たる `AGENTS.md`「条件別チェック」のトリガー

- **`Option` / フラグ / enum variant など「どの分岐が選ばれるかを決める値」の出所を変更**——`cached_masks` の有無が `new_with_cached_masks` と `new_from_tree` を分ける。drain 経路でこれが `None` 相当 → `Some` へ変わるので、**`extend_cached_masks` の `Collapsed` 枝と `new_with_cached_masks` の `Collapsed` 枝が drain 経由で初めて走る**。#977 が踏んだのと同型（1 行も変えていない下流が初めて生きる）。→ 検知器を置き、**呼び忘れを再現する変異で落ちることまで**確かめる
- **関数・型を新規定義／改名／導入**——呼び出し元 grep（済）＋ `/dry-check`。旧 API（`PrebuiltIndex::from_tree`）の削除は下流の compile-fail（`cargo build -p snotra`）を移行漏れ検出器にする。新 API 導入と呼び出し点の移行は 1 タスクに束ねる
- **対称ペアを変更**——「マスクへ追記」と「木へ追加」は長さが揃わねばならない対 → `/symmetric-check`
- **ガバナンス文書（`*.md`）・`.rs` のコメントの見出し参照を変更**——`npm run governance:check`
- **当たらないと判断したもの**: `/persistence-check`（上記のとおり `index.bin` に触らない）、`/race-check`（`drain_index` の中身は変えるが並行構造は変えない——`masks` はループ 1 反復に閉じたローカルで、スレッド・channel・共有状態・listener を増やさない）、`/state-check`（UI モード・ガード条件に触らない）。

## 未解決の疑問

`PERFORMANCE.md`「次の反復の候補」の**撤去条件**が要求する行き先が確定していない。同節は「採ったら『採用』節へ移し、測って成立しないと分かったら『試みたが機能しない手法』へ移す。**どちらでもないまま残してはならない**」と書き、かつ「実測と見積もりを分けてある」を節の存在理由に挙げている。既存の「採用」項目はすべて実測表を持つ。

→ `plan.md`「未確定」で扱う。
