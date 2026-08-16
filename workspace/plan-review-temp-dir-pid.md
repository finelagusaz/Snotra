# 独立導出レビュー: テスト用一時ディレクトリのプロセス一意性（issue #985）

対象 issue: **#985**（`test: temp_dir ヘルパーがプロセス間で一意でない残り 5 件（history/binfmt/window_data/config/folder）`）
導出日: 2026-08-16 / 対象ブランチ: `main` @ `28bd4001`
本レビューは `workspace/plan.md` / `workspace/research.md` / `workspace/adversarial-985.txt` を**読まずに**、リポジトリの走査だけから母集団を導出した。

---

## 1. 導出の方法（母集団をどう閉じたか）

「`env::temp_dir` の grep 1 本」で済ませていない。次の 6 経路を独立に当てた。

| # | 経路 | コマンド | 結果 |
|---|---|---|---|
| 1 | API 直呼び | `git grep -n "env::temp_dir" -- snotra-core` | 7 ファイル・9 ヒット（下表の全件） |
| 2 | 外部 crate 型 | `git grep -n "TempDir\|tempfile" -- snotra-core` ＋ `git grep -n tempfile -- Cargo.toml */Cargo.toml` | **0 件**。`tempfile` は依存にすら入っていない（Cargo.toml 側 0 ヒット） |
| 3 | cargo 提供の一時領域 | `git grep -n "CARGO_TARGET_TMPDIR" -- snotra-core` | **0 件** |
| 4 | env 経由の自前解決 | `git grep -n 'std::env::var' -- snotra-core` | `opener.rs` の `PATH` / `LOCALAPPDATA` のみ。`TEMP` / `TMP` / `TMPDIR` を直読する箇所は**無い** |
| 5 | ヘルパーを介さない直書き | `git grep -n "join(format!" -- snotra-core` ＋ `git grep -n "C:/tmp\|%TEMP%\|/tmp/" -- snotra-core` ＋ `git grep -n "create_dir_all\|remove_dir_all\|create_dir(" -- snotra-core` | 固定名 1 件（`folder.rs`）と純粋関数テストの文字列リテラル 2 件を追加で発見 |
| 6 | ヘルパー定義そのもの | `git grep -n "fn temp" -- snotra-core` | ヘルパー 6 件（うち 1 件は検知器テスト） |

**経路 1 と経路 6 の結果は一致する**（`env::temp_dir` を呼ぶ 9 行のうち、6 行がヘルパー定義内、3 行が呼び出し側の直書き）。異なる 2 経路が同じ母集団を指したことを閉包の根拠とする。

動的タグ（ループで生成される `tag`）も別に走査した → §4。

---

## 2. 導出したファイル一覧

### snotra-core（本 issue の射程）

| ファイル | 一時ディレクトリを作る箇所の数 | 判定 |
|---|---|---|
| `snotra-core/src/history.rs` | ヘルパー 1（呼び出し 8） | 修正が必要 |
| `snotra-core/src/binfmt.rs` | ヘルパー 1（呼び出し 14） | 修正が必要 |
| `snotra-core/src/window_data.rs` | ヘルパー 1（呼び出し 2） | 修正が必要 |
| `snotra-core/src/config.rs` | ヘルパー 1（呼び出し 10・うち 1 つはループ）＋ 直書き 1（pid あり）＋ 純粋関数テストの文字列 2 | ヘルパーは修正が必要 / 直書きは既に一意 / 文字列 2 は対象外 |
| `snotra-core/src/folder.rs` | ヘルパー 1（呼び出し 13・うち 2 つはループ）＋ 固定名 直書き 1 | ヘルパーは修正が必要 / 固定名は軽微 |
| `snotra-core/src/indexer.rs` | ヘルパー 1（呼び出し多数）＋ 検知器 1 | 既に一意（#982 で修正済み・先例） |
| `snotra-core/tests/search_frame_cost.rs` | 直書き 1 | 既に一意（先例） |
| `snotra-core/src/opener.rs` | 0（`env::var("PATH")` / `LOCALAPPDATA` のみ） | 対象外 |
| `snotra-core/tests/dir_stat_cost.rs` / `memory_footprint.rs` / `path_query_cost.rs` | 0（散文の "temp" のみ） | 対象外 |
| `snotra-core/src/engine.rs` / `search.rs` / `search/` / `query.rs` / `str_arena.rs` / `index_tree.rs` / `hotkey.rs` / `instant.rs` / `error.rs` / `ui_types.rs` / `lib.rs` | 0 | 対象外 |

呼び出し数は `git grep -c "= temp_dir\|= temp_dir_with_contents" -- <file>` で実測した（`config.rs` は `temp_dir(` の総数 12 から `std::env::temp_dir()` の 2 行を差し引いて 10 と検算）。**この数は情報であって判定材料ではない**——ヘルパーのシグネチャが不変なので、呼び出し側は 1 行も変わらない。

### 他 crate（本 issue の射程外・参考）

| ファイル | 箇所 | 判定 |
|---|---|---|
| `src-tauri/src/icon.rs` | 直書き 2 | 対象外（別 crate）・既に一意 |
| `src-tauri/src/egui_shell/window_coordinator.rs` | `crate::working_set::trim_idle_working_set(std::process::id())`（`window_coordinator.rs:501`） | **対象外・無関係**（一時ディレクトリではなく working set trim の引数。広域 grep のノイズであることを明示的に潰した） |
| `snotra-egui-runtime` / `snotra-settings` / `scripts` | 0 | 対象外 |

---

## 3. 導出したシンボル一覧（行番号ではなくシンボルで同定）

### 3-A. 修正が必要（プロセス間で一意でない）— 5 件

| # | ファイル | シンボル | 現在のディレクトリ名の形 | 根拠 |
|---|---|---|---|---|
| 1 | `snotra-core/src/history.rs` | `tests::temp_dir(tag: &str)` | `snotra_hist_test_{tag}` | `history.rs:427` `std::env::temp_dir().join(format!("snotra_hist_test_{}", tag))` |
| 2 | `snotra-core/src/binfmt.rs` | `tests::temp_dir(tag: &str)` | `snotra_binfmt_test_{tag}` | `binfmt.rs:223` `... format!("snotra_binfmt_test_{}", tag)` |
| 3 | `snotra-core/src/window_data.rs` | `tests::temp_dir(tag: &str)` | `snotra_window_test_{tag}` | `window_data.rs:105` `... format!("snotra_window_test_{}", tag)` |
| 4 | `snotra-core/src/config.rs` | `tests::temp_dir(tag: &str)` | `snotra_config_test_{tag}` | `config.rs:3222` `... format!("snotra_config_test_{}", tag)` |
| 5 | `snotra-core/src/folder.rs` | `tests::temp_dir_with_contents(tag: &str)` | `snotra_test_{tag}` | `folder.rs:243` `... format!("snotra_test_{}", tag)` |

5 件すべてが同じ 4 行の形をしている（`join(format!(...))` → `let _ = remove_dir_all(&dir)` → `create_dir_all(&dir).expect("create temp dir")` → `dir`）。`folder.rs` だけ戻り値型が `PathBuf`（`use std::path::PathBuf` 済み）で他 4 件は `std::path::PathBuf` のフルパス表記。**シグネチャは 5 件とも `fn(&str) -> PathBuf`** ゆえ、#982 と同じく**呼び出し元は 1 行も変わらない**。

### 3-B. 既に一意（先例）— 3 件（in-crate）

| # | ファイル | シンボル | 現在の名前の形 | 区切りの形 |
|---|---|---|---|---|
| P1 | `snotra-core/src/indexer.rs` | `tests::temp_dir(tag: &str)` | `snotra_idx_test_{tag}-{pid}` | 本体 `_` / **pid の前だけ `-`** / pid は**末尾** |
| P2 | `snotra-core/src/config.rs` | `tests::dedup_load_does_not_rewrite_config_file`（テスト内の直書き） | `snotra-dedup-{pid}` | **全て `-`** / pid は末尾 |
| P3 | `snotra-core/tests/search_frame_cost.rs` | `build_engine(entry_count: usize)`（`measure_search_frame_cost` から呼ばれる) 内の直書き | `snotra-search-frame-cost-{pid}-unused` | **全て `-`** / pid が**中間**（末尾は `-unused`） |

検知器: `snotra-core/src/indexer.rs` の `tests::temp_dir_name_contains_process_id`（`indexer.rs:2580`）。`file_name()` の**完全一致**で `format!("snotra_idx_test_process_unique-{}", std::process::id())` を pin している。

参考（別 crate・対象外だが命名の形の標本になる）:

| # | ファイル | シンボル | 現在の名前の形 | 区切りの形 |
|---|---|---|---|---|
| P4 | `src-tauri/src/icon.rs` | `tests::invalidate_is_atomic_with_concurrent_load` | `snotra_icon_522_{pid}` | **全て `_`**（pid の前も `_`） |
| P5 | `src-tauri/src/icon.rs` | `tests::invalidate_removes_file_and_clears_memory` | `snotra_icon_522_det_{pid}` | **全て `_`** |

### 3-C. 命名の形の所見（要求 2 への回答）

**先例は「既にこの形である」が、形は 3 種類に割れている。** issue #985 本文は先例 3 つを「いずれも既にこの形」と書くが、**区切り文字と pid の位置は一致していない**。

| 軸 | P1 (indexer) | P2 (config dedup) | P3 (search_frame_cost) | P4/P5 (icon.rs) |
|---|---|---|---|---|
| prefix の区切り | `_` | `-` | `-` | `_` |
| pid 直前の区切り | `-` | `-` | `-` | `_` |
| pid の位置 | 末尾 | 末尾 | **中間**（末尾は `-unused`） | 末尾 |
| pid 以外の可変部 | `tag` あり | なし | なし | なし |

- **共通しているのは「pid を `{}` で素の 10 進として名前に含める」ことだけ**であり、区切り文字は共通していない。
- 未修正 5 件は**全て `_` 系**（`snotra_hist_test_` / `snotra_binfmt_test_` / `snotra_window_test_` / `snotra_config_test_` / `snotra_test_`）。
- したがって `-` へ倒すと 5 件の prefix と不整合な混成（`snotra_hist_test_{tag}-{pid}`）になり、`_` へ倒すと**唯一の検知器を持つ先例 P1 と食い違う**（P1 は `-{pid}`）。**この二択は issue が「どちらでもよいが 5 件で揃えること」と明記した判断点であり、本レビューは data を渡すだけで裁定しない。**
- ただし選択の副作用は 1 つ指摘しておく: `_` へ倒す案を採ると、整合のために P1 も直すことになり、その瞬間 `temp_dir_name_contains_process_id` の完全一致アサーション（`indexer.rs:2588`）も同時に直す必要が生じる。**片方だけ直すと検知器が赤くなる**——これは検知器が意図どおり働く証拠であって欠陥ではないが、`_` 案の作業量に 1 件加算される。

---

## 4. 動的に生成されるタグ（ヘルパー経由なので個別修正は不要）

grep が拾いにくい「ループ内で `format!` したタグ」を別に走査した。**いずれもヘルパーを通るため、ヘルパーに pid を足せば同時にカバーされる**（独立の修正対象として数えない）。

| ファイル | 呼び出し元シンボル | タグの生成式 | 展開後のディレクトリ名 |
|---|---|---|---|
| `folder.rs` | `tests::bench_folder_search(label, n, ...)`（`folder.rs:461` が `temp_dir_with_contents(&tag)`） | `format!("bench_folder_{}_{}", label, n)` | `snotra_test_bench_folder_folder_narrow_1000` 等（label ∈ {`folder_narrow`, `folder_hidden_all`}・n ∈ {1000, 5000, 10000}） |
| `folder.rs` | `tests::bench_folder_topk_sort`（`folder.rs:533`） | `format!("bench_topk_{n}")` | `snotra_test_bench_topk_1000` 等 |
| `config.rs` | `tests::load_from_dir_repairs_and_saves_invalid_hotkey`（`config.rs:3355` が `temp_dir(case)`） | ループ変数 `case` | `snotra_config_test_unknown_modifier` / `_unsupported_key` / `_semantic_conflict` |
| `indexer.rs` | 多数（既に pid あり） | 文字列リテラル | 対象外 |

補足: `bench_*` は `#[ignore]` なので通常の `cargo test` では走らないが、`-- --ignored` で走らせた 2 プロセスは同名を狙う。**ヘルパー修正でカバーされるため追加作業は無い。**

---

## 5. ヘルパーの形をしていない同欠陥クラス（別枠・要求どおり）

| ファイル | シンボル | 名前 | 判定 |
|---|---|---|---|
| `snotra-core/src/folder.rs` | `tests::list_folder_nonexistent_dir_returns_empty`（`folder.rs:356`） | `snotra_test_nonexistent_zzz`（**固定名・pid なし**） | **軽微**（下記） |

**軽微とする根拠**: この箇所は `create_dir_all` も `remove_dir_all` も**呼ばない**。`list_folder` に「存在しないパス」を渡してエラー結果 1 件が返ることだけを見る**読み取り専用**の使い方であり、他プロセスへの書き込み経路を持たない。したがって #985 が名指しする「片方の `remove_dir_all` が他方の `create_dir_all` に割り込む」欠陥クラスには**該当しない**。

ただし 2 点、残余として記録する。

1. この名前は `folder.rs` のヘルパーと**同じ prefix 名前空間**（`snotra_test_`）に居る。ヘルパー側を `snotra_test_{tag}-{pid}` にしても、この固定名は取り残される。名前空間の一貫性を求めるなら同時に触る対象になる（機能上の必要は無い）。
2. 逆向きの脆さが 1 つある: 「存在しない」ことをアサートしているので、**誰かがこの名前のディレクトリを作れば落ちる**。現在このリポジトリで `snotra_test_nonexistent_zzz` を作るコードは無い（`git grep "nonexistent_zzz"` の結果は当該 1 行のみ）が、pid を含めれば「他人が作れない名前」になり構造的に不可能化できる。**これは #985 の射程外の設計改善であり、やる／やらないの判断が要る。**

---

## 6. 対象外（同じ grep に掛かるが欠陥クラスに属さない）

| 所在 | 内容 | 対象外の理由 |
|---|---|---|
| `snotra-core/src/config.rs` `tests::config_dir_from_*` 群（`config.rs:1214` / `1217` / `1264` / `1265`） | `C:\tmp\snotra-profile` / `%TEMP%\Snotra` の**文字列リテラル** | 純粋関数 `config_dir_from` の入出力を比較するだけ。ファイルシステムに一切触れない |
| `snotra-core/src/binfmt.rs:99` | `fs::create_dir_all(dir)` | **製品コード**（`BinFile::save` の保存先作成）。temp ではない |
| `snotra-core/src/config.rs:1046` | `fs::create_dir_all(dir)` | **製品コード**（config 保存先作成）。temp ではない |
| `snotra-core/src/opener.rs:263` / `:294` | `env::var("PATH")` / `env::var("LOCALAPPDATA")` | 製品コードの env 読み取り。temp 解決ではない |
| `src-tauri/src/egui_shell/window_coordinator.rs:501` | `trim_idle_working_set(std::process::id())` | pid を使うが**一時ディレクトリと無関係**（working set trim の対象プロセス指定）。広域 grep のノイズ |
| `src-tauri/src/icon.rs:501` / `:562` | pid 入り temp dir | 別 crate。#985 の表に無い。**既に一意なので作業も不要** |

---

## 7. 更新が必要な文書（調査結果: **無い**）

### 探し方（根拠）

`workspace/` を除外したうえで、次の母集団に対して `temp_dir` / `snotra_idx_test` / `snotra_config_test` / `snotra_hist_test` / `snotra_binfmt_test` / `snotra_window_test` / `snotra_test_` / `process::id` / 「プロセス一意」「プロセス間で一意」「一時ディレクトリ」を当てた。

- `SPEC.md` → **0 ヒット**
- `AGENTS.md` → **0 ヒット**
- ルート `CLAUDE.md` → **0 ヒット**
- `snotra-core/CLAUDE.md` / `snotra-egui-runtime/CLAUDE.md` / `src-tauri/CLAUDE.md` / `snotra-settings/CLAUDE.md` → **0 ヒット**
- `docs/` 配下 → ヒットは `docs/superpowers/plans/*.md` **のみ**（下記）
- `.claude/`（rules / skills / hooks / settings） → `safety-nets.md:28` の「hook を一時ディレクトリへコピー」1 件のみ＝**別文脈**（githooks テストの作法）
- `scripts/`（`governance/checks/G-*.mjs` 全 19 検査を含む） → **0 ヒット**
- `RETROSPECTIVE.md` / `PERFORMANCE.md` / `CONTRIBUTING.md` / `README.md` / `README.en.md` / `.github/` → **0 ヒット**

### `docs/superpowers/plans/*.md` のヒットの扱い

`2026-08-09-rescan-in-situ-instrument.md` / `2026-08-09-scan-all-seen-conditional.md` / `2026-08-10-explicit-scan-only.md` / `2026-08-10-rescan-applies-its-result.md` / `2026-08-11-entry-name-derivation-ssot.md` / `2026-07-23-su3.5-tool-selection.md` / `2026-07-24-su6.5-flip-hardening.md` に `temp_dir(...)` を含むコード片がある。

**これらは実施済みの計画書（凍結された歴史記録）であり、更新対象ではない。** 理由: 計画書は「その時点で何を決めたか」を残す文書であって現在のコードの SSOT ではない。遡って書き換えると、記録としての価値（当時の判断の再現性）が失われる。同種の判断は `AGENTS.md`「文書に事実の写しを増やす変更」の「正本を 1 か所に定め他は参照へ」に整合する——正本はコードである。

### 直接の裏付け（先例が実際にどう振る舞ったか）

`git show --stat f5bf9755`（PR #982 = 同じ欠陥を `indexer.rs` に対して直したコミット）の変更ファイルは **`snotra-core/src/indexer.rs` の 1 ファイルのみ**（25 insertions / 1 deletion）。**同型の修正で文書は 1 枚も動いていない。** これが「今回も文書更新は不要」の最も強い一次証拠である。

### ただし 1 点だけ判断が要る（下記 §8「軽微」へ）

`snotra-core/CLAUDE.md` には「開発ルール」節に**テスト fixture の作法**（`HistoryStore::load()` を使わない・#963）が既にあり、`index.bin 書き込みの排他（INDEX_WRITE_LOCK）` 節には「**プロセスをまたぐ同時起動は世代機構でも守れていなかった**——世代は `INDEX_WRITE_LOCK` と同じくプロセス大域の `static` であり、射程が同じ」という**同じ射程の議論**が既に書かれている。ここへ「テスト用 temp dir は pid を含める」を足す**余地はある**が、**必須ではない**——足すと、正本（コードの doc コメント）の写しが 1 枚増える（`AGENTS.md`「同じ事実を 2 か所以上へ書こうとしている時点で、それが写しである」）。**足さない方に倒すのが既定**と考える。

---

## 8. 所見の 3 分類

### 要対処

| # | 所見 | 所在 |
|---|---|---|
| A1 | `temp_dir(tag)` が pid を含まない | `snotra-core/src/history.rs::tests::temp_dir` |
| A2 | 同上 | `snotra-core/src/binfmt.rs::tests::temp_dir` |
| A3 | 同上 | `snotra-core/src/window_data.rs::tests::temp_dir` |
| A4 | 同上 | `snotra-core/src/config.rs::tests::temp_dir` |
| A5 | `temp_dir_with_contents(tag)` が pid を含まない | `snotra-core/src/folder.rs::tests::temp_dir_with_contents` |
| A6 | **命名の形が先例間で割れている**（`_` 系 3 種 vs `-` 系 2 種、pid 位置も末尾/中間）。「5 件で揃える」だけでは足りず、**先例 P1（唯一の検知器つき）と揃えるのか、未修正 5 件の既存 prefix に揃えるのか**を決める必要がある。`_` を選ぶと P1 と `temp_dir_name_contains_process_id` も同時修正になる | 命名規約（横断） |
| A7 | `binfmt.rs` は `BinFile::save` の **tmp→rename（固定 tmp 名）** を直接叩くテスト群を持つ（`binfile_atomic` / `binfile_overwrite` / `binfile_mkdir` 等 14 本）。`indexer.rs` で `index.bin.tmp` の食い合いが問題になったのと**同じ構造**が、より近い距離にある。5 件の中で優先度が最も高い | `snotra-core/src/binfmt.rs` |

### 軽微

| # | 所見 | 所在 |
|---|---|---|
| B1 | `snotra_test_nonexistent_zzz` が固定名（pid なし）。ただし create/remove を伴わない読み取り専用のため欠陥クラスには属さない。名前空間の一貫性と「他人に作られない名前にする」という構造的堅牢化のために触る余地はある | `folder.rs::tests::list_folder_nonexistent_dir_returns_empty` |
| B2 | `folder.rs` の prefix `snotra_test_` だけが**モジュール名を含まない**（他 4 件は `hist` / `binfmt` / `window` / `config` / `idx` を含む）。pid を足せばプロセス間衝突は消えるが、**同一プロセス内で他モジュールと tag が衝突する余地**はこの prefix だけが持つ。pid を足すついでに `snotra_folder_test_` へ寄せる案がありうる（issue 射程外） | `folder.rs::tests::temp_dir_with_contents` |
| B3 | 検知器を 5 件すべてに置くと `temp_dir_name_contains_process_id` 型のテストが 6 本になる。#982 は 1 本だけ置いた。**5 本増やすか 0 本かは判断**（issue も「判断でよい」と明記）。1 本も置かずに済ませると、将来の誰かが pid を落としても機構は黙る | 検知器の配置 |
| B4 | `snotra-core/CLAUDE.md` へ「テスト temp dir は pid を含める」を書き足す余地はあるが、正本はコード側の doc コメント（`indexer.rs::tests::temp_dir` の rustdoc が既に機序を全文で持つ）。書くと写しが増える。**既定は書かない** | `snotra-core/CLAUDE.md` |
| B5 | 5 件を共有ヘルパーへ寄せる案（issue の判断点）。各テストの片付け作法が実際には**均一**（5 件とも `remove_dir_all` → `create_dir_all` → 返す、の同一 4 行）なので技術的には寄せられる。ただし `#[cfg(test)]` の共有ヘルパーは `lib.rs` 等に置き場所を作る必要があり、**issue は「寄せずに 5 か所へ pid を足すだけでも閉じる」と明記**している。増える構造に対して得るものが小さい | 設計判断 |

### 未検証

| # | 主張 | なぜ未検証か |
|---|---|---|
| C1 | 「同名の temp dir を 2 プロセスが狙うと `remove_dir_all` が `create_dir_all` に割り込んで panic する」という**機序そのもの** | 本レビューでは**再現実験をしていない**。この機序は `indexer.rs::tests::temp_dir` の rustdoc と issue #978/#985 本文が主張しているものであり、本レビューはそれを**引用しているだけ**である。5 件それぞれについて実際に赤が出ることは測っていない |
| C2 | 「テストバイナリが複数プロセスに分かれる状況が実際に起きる」（`cargo test` と `cargo test --release` の重なり・別 worktree での並行実行） | 同上。`indexer.rs` の rustdoc の主張の引用であり、本レビューは CI 設定・worktree 運用の実際の並行度を測っていない |
| C3 | 「`folder.rs` の bench（`#[ignore]`）が実際に並行実行される運用がある」 | `-- --ignored` を 2 プロセスで同時に回す運用が実在するかは未確認。ヘルパー修正で自動的にカバーされるため実害の有無は作業量に影響しない |
| C4 | 「`snotra_test_nonexistent_zzz` を作るコードがリポジトリ内に無い」 | `git grep "nonexistent_zzz"` は当該 1 行のみを返したが、**リポジトリ外**（開発者の手作業・他ツール）で同名が作られる可能性は原理的に否定できない。全称否定として書かず下限主張に留める |
| C5 | 「区切りを `-` に統一しても既存の何も壊れない」 | `temp_dir_name_contains_process_id` 以外に名前を pin する検査は grep では見つからなかったが、**実際に 5 件を変更して `cargo test --workspace` を通す確認はしていない**（本タスクはコード変更禁止） |

---

## 9. 呼び出し側への引き渡しサマリ

- **要対処は 5 件のヘルパー**（issue #985 の表と完全一致）。独立導出でも過不足なし。
- **加えて 1 件の非ヘルパー固定名**（`snotra_test_nonexistent_zzz`）を発見。欠陥クラス外なので軽微。
- **命名の形は先例 3 つで割れている**（issue 本文の「いずれも既にこの形」は pid 混入については正しいが、区切り文字については揃っていない）。`-` / `_` の二択は A6 のトレードオフを見て決めること。
- **文書更新は不要**。根拠は §7（8 母集団で 0 ヒット＋同型 PR #982 が 1 ファイルのみの変更で完結した実績）。
