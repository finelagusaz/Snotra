# plan-review — issue #1178（`workspace/plan.md`）独立レビュー（観点1・観点2 限定）

対象: `chore/stats-duration-remaining` ブランチ、`workspace/plan.md`（コード変更 0 行時点）。
検証したのは依頼された 2 観点のみ（公開 API 変更の漏れ／`memory_footprint.rs` の残余算式）。

## 要対処

### [観点1] `src-tauri/src/startup.rs:416-417` のコメントが PR 後に偽になる。計画の対象ファイルに無い

- 根拠: `src-tauri/src/startup.rs:416-417`
  ```
  // **主張はこの式に限る**: `LoadOrScanStats` の他の `*_ms` は今も `snotra-core` の中で
  // 生成時に丸めている。
  ```
  この一文は「`total` 以外の `*_ms`（`hash_ms`/`cache_load_ms`/…）は今も生成時に丸めている」という
  事実主張である。本 issue はまさにこの丸めを表示境界へ移すので、実装後この一文は偽になる。
- 計画の「変更ファイルと対象シンボル」表（plan.md:79-89）に `src-tauri/src/startup.rs` は無く、
  「触らない」節（plan.md:87-89）は `startup.rs` を名指しで除外しているが、その除外根拠
  （research.md:40-41「`set_index_load_stats_total(Duration)` 以降は端から端まで `Duration`」）は
  `total` の話であって、この一文が語る「他の `*_ms`」には当てはまらない。
- 受け入れ条件 5（plan.md:18-21）の旧識別子 grep は `hash_ms` 等の**具体的な識別子**を対象とし、
  この一文は `*_ms`（総称）としか書いていないため、Phase 5 の終端 grep（除外句なし）でもこの
  記述の陳腐化は検出されない——`git grep -n "\bhash_ms\b" src-tauri/` は 0 件のまま緑になる
  （このコメントには `hash_ms` などの具体的トークンが 1 つも無いため）。
- **計画のどこをどう直すか**: 「変更ファイルと対象シンボル」表に
  `src-tauri/src/startup.rs`（416-417 行、コメントのみ）を追加し、Phase 3 か 4 で
  「`LoadOrScanStats` の他の `*_ms` は…」を「この PR で `Duration` 化された」旨へ書き直す
  作業項目を明記する。

### [観点1] 改名表 `Scanned::scan_ms → scan` / `sort_ms → sort` が、同一スコープの `scan: &[ScanPath]` パラメータをシャドウする

- 根拠: `scan_and_sort_timed` 自身のシグネチャが `scan: &[ScanPath]`（`snotra-core/src/indexer.rs:748`）
  であり、返す `Scanned` の呼び出し元 3 箇所すべてが同じ関数スコープ内に `scan: &[ScanPath]` を
  持つ:
  - `snotra-core/src/indexer.rs:814-819`（`load_or_scan_with_stats_in` のパラメータ `scan`。
    `with_index_write_lock` クロージャが `scan` をキャプチャした直後に `Scanned { .. }` を分解）
  - `snotra-core/src/indexer.rs:875-879`（`load_or_scan_with_stats` 自身のパラメータ `scan`。
    `Config::config_dir()` が `None` の枝）
  - `grep -n "scan_and_sort_timed\|Scanned {" snotra-core/src/indexer.rs` の出力（3 呼び出し点が
    いずれも `scan:` パラメータを持つ関数内にあることを確認済み）
  改名表どおり `Scanned::scan_ms → scan` にすると、この 3 箇所の分解パターンは
  `let Scanned { entries, scan, sort } = scan_and_sort_timed(scan, show_hidden_system);` という
  形になり、新しく束縛される `scan`（`Duration` または `u128`）が、直前まで生きていた
  `scan: &[ScanPath]` を同名でシャドウする。
- **これは Rust の shadowing 規則上コンパイルは通る**（RHS `scan_and_sort_timed(scan, ..)` が
  評価されてからパターンが束縛されるため、いずれの 3 箇所も分解後に元の `scan` を再度読んでいない
  ——実測で確認済み）。ただし `&[ScanPath]` を表す識別子が同じスコープで `Duration` に化けるのは
  可読性上の事故りやすい形であり、将来この関数へ処理を足す・順序を入れ替える変更で
  誤読しやすい落とし穴になる。計画にはこの衝突（および回避方針）への言及が無い。
  ちょうどこの点はレビュー依頼の「Scanned の scan / sort」の懸念そのものである。
- **計画のどこをどう直すか**: Phase 1 のチェックリストへ「`scan_and_sort_timed` およびその
  呼び出し元 2 箇所（`indexer.rs:814-819` / `875-879`）では、フィールド初期化に shorthand を
  使わず `scan: scan_dur`（非衝突のローカル変数名）のような明示形にし、`scan: &[ScanPath]`
  パラメータをシャドウしないこと」という 1 項を追加する。

## 軽微

- [観点1] `LoadCacheResult::read_ms → read`（`upgrade_legacy_cache_in` のパラメータ
  `read_ms: u128` を含む・`indexer.rs:1191`）は実際の識別子衝突を起こさない——同スコープに
  `.read()` を呼ぶ他の値・型が無く、`result.read` は単なるフィールドアクセスとして曖昧さが無い。
- [観点1] `LoadOrScanStats::hash` は `Hash` トレイトの derive と衝突しない——struct は
  `#[derive(Debug, Clone, Copy)]`（`indexer.rs:422`）のみで `Hash` を持たず、`config_hash` /
  `current_hash` は別名のローカル変数であり `LoadOrScanStats` のフィールドではない。
- [観点1] `#[derive(Debug, Clone, Copy)]`（`indexer.rs:422`）は型変更後も壊れない——
  `std::time::Duration` と `Option<Duration>` はいずれも `Copy` / `Clone` / `Debug` を実装する。
- [観点2] `memory_footprint.rs:317-330` の println で、`cache_save` は
  「cache-hit＋旧版昇格時に `cache_load` の内数として括弧内に現れる」ことと
  「常に文末の独立項 `+ cache_save {}ms` としても印字される」ことが**同時に**起きる
  （`cache_read` は括弧内にしか現れず独立項を持たない、という非対称と対照的）。これは
  **D3 適用前から存在する挙動**であり、D2（format 文字列を変えない）の下では D3 を適用しても
  そのまま温存される——新たに生む非対称ではない。計画に一言（「既存の非対称は保存されるだけで
  悪化しない」）を添えると、この後 D3 のレビューをする者が同じ疑問を再導出せずに済む。

## 未検証

- ⚠️ [観点1] `docs/superpowers/` 配下（凍結扱い・plan.md/research.md が母集団外と明記）には
  `LoadOrScanStats` / `load_or_scan_with_stats` を参照する設計文書が多数残っている
  （`git grep` で `docs/superpowers/plans/2026-08-09-...` 等 5+ ファイル）。凍結文書を対象外と
  する判断そのものは本レビューの 2 観点（公開 API 漏れ／残余算式）の範囲外だが、「crate 外の
  消費者を洗い出す」という観点1 の広い意味では触れておく。
- ⚠️ [観点2] D4 が「`delta_mib` と同じ『符号つきの差分』の作りに揃える」（plan.md:53-55）と
  主張する点について、`delta_mib`（`memory_footprint.rs:134-136`）は `f64` キャストしての浮動小数
  減算、D4 は `i128`（ns）での整数減算を想定しており、**「符号を保持する」という設計思想は
  一致するが、型機構（float vs 整数）は異なる**。実装が正しく動く分には支障は無いはずだが、
  「同じ作り」という表現がその型機構の違いまで含めてしまうとやや過大——実装時に文言を
  「同じ *設計思想*（符号つき）」程度に留めるかは実装者の裁量で良い（要対処ではない）。

## 結論（3 分類の要約）

- **要対処 2 件**: (1) `startup.rs:416-417` のコメントが計画の射程漏れで陳腐化する、
  (2) `Scanned::scan`/`sort` への改名が `scan: &[ScanPath]` パラメータをシャドウする
  （3 箇所・compile は通るが可読性の事故りやすい形）。
- **軽微 4 件**: `read`/`hash` の識別子衝突は実測上いずれも無害。derive の型互換は壊れない。
  `cache_save` の印字非対称は既存挙動であり D3 で悪化しない。
- **未検証 2 件（⚠️）**: 凍結文書の母集団の扱いは 2 観点の範囲外気味。D4 と `delta_mib` の
  「作り」の一致度はやや誇張の可能性があるが実害は無さそう。
