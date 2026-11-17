# 計画: #1456 migemo_enabled / include_path_env の切り替えで全走査し直さない

## 目的と受け入れ条件

`include_path_env` / `migemo_enabled` だけを切り替えたときは、ファイルシステムを走査せず、`index.bin` を読み直して PATH のマージと `PrebuiltIndex` の構築だけを行う。

- [受け入れ 1] 2 つの切り替えで全走査が走らない: `scan-log.jsonl` の start 行が増えない（行数ではなく start 行の増分で数える・孤児の start 行が実在するため）
- [受け入れ 2] 検索結果は、全走査した場合と同じになる
- [受け入れ 3] `/s`・初回構築・走査対象（`scan` / `show_hidden_system`）の変更は、今までどおり走査する
- [受け入れ 4] `/state-check` と `/race-check` を通す

## 設計

### 要求の種類を分ける

`indexing::start_index_build` に要求の種類を渡す。

```rust
pub enum BuildRequest {
    /// 必ず走査する（`/s`・初回構築）。
    Rescan,
    /// 索引を建て直す。走査するかは drain がキャッシュの照合で決める（設定変更）。
    Rebuild,
}
```

| 呼び出し元 | 種類 |
|---|---|
| `commands/system.rs` の `rebuild_index`（`/s`） | `Rescan` |
| `main.rs` の `setup_first_run` | `Rescan` |
| `commands/window.rs` の settings 監視（first-run） | `Rescan` |
| `config_watcher.rs`（`IndexInputs` の差分） | `Rebuild` |
| `start_index_build` 自身の finish 後の自己再 kick | `Rebuild`（旗に触れない。下の ledger が要求を持ち越す） |

### engine の ledger に走査の要求を載せる（`snotra-core/src/engine.rs`）

- フィールドを足す: `rescan_requested: u64`（`Rescan` の要求の世代）・`rescan_done: u64`（走査で満たした世代）・`cache_in_sync: bool`（`index.bin` がいまの索引と同じ走査の結果だと言えるか）
- `mark_index_stale()` は今のまま（`Rebuild`）。`request_rescan()` を足す: `index_stale = true` にし、`rescan_requested += 1`
- `begin_index_drain(&self) -> Option<IndexDrain>`。`IndexDrain { inputs: IndexInputs, must_scan: Option<u64> }`。`must_scan` は `rescan_requested > rescan_done` なら `Some(rescan_requested)`、そうでなく `!cache_in_sync` なら `Some(rescan_done)`（世代は進めない）、それ以外は `None`
- `complete_index_drain(&mut self, index, drain: &IndexDrain, outcome: DrainOutcome)`。`DrainOutcome` は `Loaded`（キャッシュから建てた）か `Scanned { saved: bool }`
  - `Scanned` なら、`drain.must_scan` が `Some(g)` のとき `rescan_done = max(rescan_done, g)`。`cache_in_sync = saved`
  - `Loaded` なら `cache_in_sync` は変えない（読んだものが engine の索引になったので一致は保たれる）
  - stale を落とすのは、`IndexInputs` が `drain.inputs` と一致し、かつ `rescan_done >= rescan_requested` のときだけ
- 起動経路が `cache_in_sync` の初期値を決める: `Engine::from_material` の後に `set_cache_in_sync(bool)` を呼ぶ。値は `LoadOrScanResult` に足す `cache_in_sync`（cache-hit なら `true`、cache-miss で保存したならその成否、保存先が無ければ `false`）。first-run は起動のロードを通らないので `false`（直後の初回構築は `Rescan` で走査し、`saved` で真になる）

lost-update の扱い（`/state-check`・`/race-check` で確かめる点）:
- `Rebuild` の drain の最中に `/s` が来た: `rescan_requested` が増え、complete で stale が残る → 次の反復が走査する
- `Rescan` の drain の最中（`begin` の後）に `/s` が来た: complete が書く `rescan_done` は `begin` 時点の世代なので、新しい要求は満たされない → 次の反復が走査する
- finish ウィンドウ（complete が stale を落とした後〜finish の前）に `/s` が来た: CAS に負けるが、stale と世代は立っている → 既存の finish 後の再チェックが `Rebuild` で再 kick し、`begin` が世代を見て走査する

### drain の分岐（`src-tauri/src/indexing.rs` の `drain_index`）

- `must_scan` が `Some` → 今の手順（scan-log の start → 進捗 → **アイコンキャッシュの無効化** → `rebuild_and_save` → 建てる → complete（`Scanned { saved }`）→ scan-log の end）
- `None` → `indexer::load_cached_material(&inputs.scan, inputs.show_hidden_system)` を呼ぶ
  - `Some(material)` → `build_index_from_material` → complete（`Loaded`）。scan-log は書かない。進捗も置かない。アイコンキャッシュに触らない
  - `None`（`index.bin` が無い・壊れている・ハッシュが合わない）→ 走査の手順へ落ちる
- アイコンキャッシュの無効化は `start_index_build` から外し、drain の走査の分岐（`rebuild_and_save` の直前）へ移す。反復ごとに走査するたび撃つ

### キャッシュを読む入口（`snotra-core/src/indexer/cache.rs`）

- `pub fn load_cached_material(scan, show_hidden_system) -> Option<IndexMaterial>` を足す。`load_cache(hash, LegacyUpgrade::Skip)` を通し、走査も書き込みもしない。旧版でも読める（書き戻さない）
- 保存の成否を返す: `save_cache_sorted_in` が `bf.save` の結果を返し、`rebuild_and_save` の `RebuildStats` に `saved: bool` を足す。`config_dir` が無い分岐（保存しない）は `false`

### 文書

- `SPEC.md`「7.5 設定反映タイミング」: インデックス条件のうち、走査対象（スキャンパス・隠しファイル表示）の変更は走査して再構築し、`include_path_env`・migemo の変更は保存済みの索引から建て直す（走査しない）と書く。ずれている「ステータスに『インデックスを再構築中…』を表示」の行を、インデックス条件の下へ戻す
- `SPEC.md`「3.4」相当のアイコンの節（「索引の再構築が始まったとき（初回構築・`/s`・設定変更のいずれの契機でも）」）: 「索引の再構築で走査したとき（初回構築・`/s`・走査対象の変更）」へ直す
- `docs/architecture.md` の「config↔派生状態のコヒーレンシ」の段落: ledger が走査の要求（世代）と `index.bin` との一致も持つことを 1 文足す
- `PERFORMANCE.md`: 切り替えの所要を前後で 1 行ずつ記録する（下の E2E で測る）
- `indexing.rs` の `//!` と `start_index_build` / `drain_index` の doc を、要求の種類と分岐に合わせて直す。`engine.rs` の `//!`（ledger の説明）と `IndexInputs` の doc も同じく

## 変更するファイルとシンボル

1. `snotra-core/src/indexer/cache.rs`: `save_cache_sorted_in`（保存の成否を返す）・`save_cache_sorted`・`rebuild_and_save`・`RebuildStats.saved`・`LoadOrScanResult.cache_in_sync`・`load_cached_material`（新規）。`load_or_scan_with_stats_in` と `upgrade_legacy_cache_in` は `save_cache_sorted_in` の返り値の形が変わるので追従する（昇格の保存に失敗しても、読んだ旧版は engine の索引と同じ走査の結果なので `cache_in_sync` は真のまま）
2. `snotra-core/src/indexer.rs`: `load_cached_material` の re-export
3. `snotra-core/src/engine.rs`: フィールド 3 つ・`request_rescan`・`set_cache_in_sync`・`IndexDrain`・`DrainOutcome`・`begin_index_drain`・`complete_index_drain`
4. `src-tauri/src/indexing.rs`: `BuildRequest`・`start_index_build`・`drain_index`・ソーステキスト検査 2 本
5. `src-tauri/src/config_watcher.rs`・`src-tauri/src/commands/system.rs`・`src-tauri/src/commands/window.rs`・`src-tauri/src/main.rs`: 呼び出しに種類を渡す。`main.rs` は `set_cache_in_sync` も呼ぶ
6. `src-tauri/src/icon.rs`: `invalidate_icon_cache` の doc（「ビルド要求の受理時（`indexing::start_index_build`）に呼ぶ」）を、drain の走査の分岐で呼ぶ形へ直す
7. ソーステキスト検査の改名 `start_index_build_invalidates_the_icon_cache` → `drain_index_invalidates_the_icon_cache_before_the_scan` に合わせて、名指している 2 か所を直す: `src-tauri/src/egui_shell/launcher_controller/activation/tests.rs`（757 行の doc）・`scripts/lib/SnotraTraceInvariants.Tests.ps1`（54 行のコメント）
8. `SPEC.md`（7.5・アイコンの節・13.4 の scan-log の記述 776〜778 行）・`docs/architecture.md`（100 行）・`docs/build-commands.md`（285 行）・`src-tauri/CLAUDE.md`（19 行の `indexing.rs` の説明）・`PERFORMANCE.md`（1750 行「今は `start_index_build` が判定なしで撃つ」と、採用の記録）
9. `src-tauri/src/scan_log.rs`（20 行の、構築ごとに記録するという `//!`）・`snotra-core/src/indexer/cache.rs` の `LegacyUpgrade` / `INDEX_WRITE_LOCK` / `load_cache_in` の doc（`Skip` の使い手と、実行時の書き手の記述）

prose:sweep で見つかった残りの名指しは直さない: `docs/adr/` と `docs/superpowers/plans/` と `docs/design/` は凍結された記録。`snotra-settings/CLAUDE.md` 180 行（「`start_index_build` を開始」）・`src-tauri/src/state.rs` 90 行・`snotra-core/src/search/tests/build.rs` 502 行は、変更の後も真のまま

## 不変条件と異常系

- `/s` と初回構築は、キャッシュの状態によらず必ず走査する
- 実行時に `index.bin` を書くのは drain の走査の分岐だけ（非走査の分岐は `Skip` で読み、書かない）
- 非走査の分岐で読んだ `index.bin` が engine の索引と別の走査の結果であってはならない → `cache_in_sync` が偽なら走査へ落ちる。保存に失敗した走査の後は偽になる
- engine のロックの内側で I/O をしない（`load_cached_material` は drain スレッドのロック外で呼ぶ）
- 読み込みに失敗しても索引を失わない（走査へ落ちる）
- drain が panic で complete に届かなかったときは、stale も世代も残る（次の要求で回復する。今と同じ）

受容する残余:
- **「切り替えでは走査しない」を CI で守る検知器は置かない。** 判定の材料（ledger の `must_scan`・`load_cached_material`）は core のテストで守るが、それを使う drain の分岐（`indexing.rs`）は `AppHandle` を要するので単体テストにできない。ここが退行して毎回走査に戻っても、CI は緑のままである（結果は正しく、遅くなるだけ）。今回は E2E の一時スクリプトで 1 度だけ確かめる
- 起動時の旧版の昇格で保存に失敗し、旧版の `index.bin` が残っている場合、`Skip` の旧版の分岐は整列し直さないので、切り替えの後の並び（同点の順）が全走査と違いうる。頻度は低い（昇格の保存失敗が要る）

## テスト方針と検証コマンド

- engine（`snotra-core/src/engine.rs` の `mod tests`）
  - `request_rescan` の後の `begin` が `must_scan: Some` を返し、`Scanned` の complete で stale が落ちる
  - `Rebuild` の drain の最中に `request_rescan` → complete 後も stale が残り、次の `begin` が `must_scan: Some`
  - `Rescan` の drain の最中（`begin` の後）に `request_rescan` → complete 後も stale が残る
  - `cache_in_sync = false` なら、`Rescan` の要求が無くても `must_scan: Some`。`Scanned { saved: true }` で真に戻り、`saved: false` で偽のまま
  - `Loaded` の complete は `cache_in_sync` を変えない
  - 既存の ledger の 4 本は新しいシグネチャへ追従させ、意味を変えない
- cache（`snotra-core/src/indexer/cache/tests.rs`。dir 注入の `load_cache_in` 経由で、実 `%APPDATA%` に触れない）
  - 走査対象のハッシュが一致すれば材料を返し、`index.bin` の mtime もバイト列も変わらない（書かない）
  - ハッシュが違えば `None`
  - [受け入れ 2] 同じ entries について、保存してから読んだ材料と、保存が返した材料（全走査の経路）から建てた `SearchEngine` が、代表クエリ（Fuzzy / Substring / Prefix・パスクエリ）で同じ結果を返す。PATH（合成した PATH エントリを `extend_with_path_entries` で足す / 足さない）× migemo（ON / OFF）の 4 組で比べる
  - 保存に失敗したとき `saved: false` を返す（書けない dir を渡す）
- src-tauri のソーステキスト検査
  - `drain_index_appends_start_before_the_scan` は走査の分岐で成り立つまま
  - `start_index_build_invalidates_the_icon_cache` を、drain の走査の分岐が `rebuild_and_save` の前にアイコンキャッシュを無効化することの検査へ直す
- E2E（一時スクリプト・scratchpad・測った後に消す）: 検証用プロファイルで起動し、`config.toml` の `migemo_enabled` と `include_path_env` をそれぞれ切り替える → `indexing-complete` を待つ → `scan-log.jsonl` の start 行が増えていないこと。`/s` の後は 1 行増えること。走査対象を変えると 1 行増えること。所要（切り替えから `indexing-complete` まで）を前後で測る
- 検証: `docs/build-commands.md`「変更後の検証チェックリスト」のカテゴリ A・C・E・F（`verify:tree` で A・E・F、C は `smoke:startup` / `smoke:egui`）

## 作業項目

### Phase 1: core
- [x] `cache.rs`: 保存の成否を返す・`RebuildStats.saved`・`load_cached_material` とテスト
- [x] `engine.rs`: ledger の拡張とテスト

### Phase 2: src-tauri
- [x] `indexing.rs`: `BuildRequest`・drain の分岐・アイコン無効化の移設・ソーステキスト検査
- [x] 呼び出し元 4 か所へ種類を渡す・`main.rs` で `set_cache_in_sync`

### Phase 3: 文書と計測
- [x] `SPEC.md` 7.5 とアイコンの節・`docs/architecture.md`・doc コメント
- [x] E2E で受け入れ 1・3 を確かめ、所要を `PERFORMANCE.md` に記録する
  - E2E（2026-11-17）: 3 つの切り替えで走査 0・index.bin 不変（基準は各 1 回走査）、走査対象の変更と /s は各 1 回走査。PERFORMANCE.md に記録
- [x] `/state-check`・`/race-check`・`/symmetric-check` を当てる（3 つとも指摘なし・f7e33cb2 の後）
- [ ] 検証カテゴリ A・C・E・F

## 未確定（実装前に潰す）

- [x] `cache_in_sync` の起動時の値 — 当初は cache-miss で `false` に倒す案だったが、cache-miss の分岐も `save_cache_sorted_in` を通るので、保存の成否をそのまま `LoadOrScanResult.cache_in_sync` で返せる。保守的に倒す必要が無くなったので、成否を運ぶ形にした（本文の ledger の節と変更ファイル 1 に反映）

## 人間レビュー

- [x] 承認済み — 2026-11-17 / 問い: "#1456 をこの計画で実装に進めてよいですか？" / 回答: "承認する"

## セルフレビュー

- リスク: 高（状態遷移・共有状態の ledger・複数モジュールのインターフェース）
- plan-review: 独立レビュー1体（Step 2b・独立導出）
- エージェント数: 2（3b の敵対的調査 1・独立導出 1）
- 要対処: 4 件を反映した。(1) 偽になる散文の追加（SPEC 13.4・`docs/build-commands.md`・`src-tauri/CLAUDE.md`・`PERFORMANCE.md` 1750 行・`scan_log.rs`・`cache.rs` の doc）。(2) 受け入れ 2 の比較を PATH × migemo の 4 組へ。(3) CI に検知器が無いことを受容する残余として明記。(4) 旧版の `Skip` の並びの残余を明記
- 独立導出との不一致（採らなかったもの）: (a) 走査対象の変更を `config_watcher` で `Rescan` に分類する案 — 計画はキャッシュのハッシュ照合で走査へ落とす。A→B→A でも、B の走査が保存されていれば A は照合に外れて走査する。保存に失敗していれば `cache_in_sync` が偽で走査する。ゆえに分類は要らない。(b) first-run で ledger に `Rescan` を仕込む案 — 計画では first-run の `cache_in_sync` が偽なので、設定の保存が先に `Rebuild` を kick しても走査になる。同じ効果を既存の旗で得る。(c) 保存失敗を受容する残余にする案 — 計画は `saved` を運んで塞ぐ
- 未検証: E2E の所要（実装後に測る）
- 散文の全件列挙: 語 6 / 総件数 160 行 / ★未掲載 11（計画へ足した 4: `src-tauri/src/icon.rs`・`activation/tests.rs`・`SnotraTraceInvariants.Tests.ps1`、および呼び出し元のパス表記の修正。残り 7 は凍結された記録か、変更後も真） / ☆行未掲載 0
- 試行（#1300・高リスクのみ・独立レビュー 1 回ごとに 1 行）: 方式 Step 2b / 設計変更 0 / 主の見落とし 3（SPEC 13.4・`docs/build-commands.md`・`src-tauri/CLAUDE.md` の散文）
