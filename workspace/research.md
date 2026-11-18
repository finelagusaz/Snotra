# 調査: #1455 索引の再構築でディレクトリの走査が 1 スレッドに閉じている

## issue の要約

`snotra-core/src/indexer/scan.rs` の `scan_all` は、根ごとに `scan_directory_with_extensions` を再帰で呼び、
`std::fs::read_dir` を 1 スレッドで順に開く。実運用点の再構築は温かい状態で 22〜30 秒かかる。
issue はまずスパイクで並列の効果を測り、効くなら本実装へ進むよう求める。受け入れ条件は次の 2 つ。

- スパイクの結果（並列度ごとの所要）を `PERFORMANCE.md` に記録し、採否を決める
- 採るなら、同じ走査対象・温かい状態の `scan-log.jsonl` で `scan_ms / dirs` を A-B-A で比べ、`index.bin` のバイト列が変わらないことを確かめる

## スパイクの結果（2026-11-17〜18・HAL9000・16 論理コア・release）

道具: scratchpad の `scanspike`（独立 workspace・rayon =1.12.0）。走査の操作は製品と同じ
（`read_dir` → 各エントリの `DirEntry::metadata` → ディレクトリなら再帰）。根は実 config と同じ 5 つ・同じ順
（`C:\Toolbox`・`C:\KISS`・Start Menu・`C:\workspace`・`C:\`）。`show_hidden_system = true` に合わせ、隠し属性で枝を刈らない。

1 回目（`rayon::Scope::spawn` で再帰・数えるだけ）:

| 並列度 | 所要（2 巡） | 1 ディレクトリあたり |
|---|---|---|
| 逐次 | 27.1 / 27.1 / 27.0 / 26.6 s | 39.0〜39.8 µs |
| 1 | 26.7 s | 39.2 µs |
| 2 | 15.3 / 15.3 s | 22.4 µs |
| 4 | 8.8 / 8.7 s | 12.8 µs |
| 8 | 6.25 / 6.20 s | 9.1 µs |
| 16 | 8.0 / 7.75 s | 11.4〜11.8 µs |

ディレクトリ数は 680,896〜680,900（実行の間にファイルが動いた差のみ）。冷えた状態の逐次は 116.9 s（温めの 1 回）。
冷えた状態の並列は測っていない（再起動が要る）。

2 回目（2026-11-18・fork-join 形・下の「設計の候補」の B）: 逐次と同じく、フォルダと `.exe` / `.lnk` のパスを列挙順に集め、
集めた列のバイト列の指紋（FNV-1a）を比べた。

| 形 | 所要 | 出力 | 順序の指紋 |
|---|---|---|---|
| 逐次 | 27.4 / 27.5 / 27.9 s | 696,226 | `84caed6171d7b0b0` |
| fork-join 8 | 6.11 / 6.26 s | 696,226 | `84caed6171d7b0b0`（両方） |
| Scope 8（数えるだけ） | 6.06 s | — | — |
| fork-join 4 | 9.06 s | 696,226 | `84caed6171d7b0b0` |
| fork-join 16 | 7.85 s | 696,226 | `84caed6171d7b0b0` |

fork-join は Scope 形と同じ速さで、列挙順を逐次と同じに保つ（温めの回の指紋が違うのは、その回の間にファイルが 1 件増えたため。
出力件数が 696,225 → 696,226）。

## 関連するファイル・シンボル

- `snotra-core/src/indexer/scan.rs`
  - `scan_all(scan_paths, show_hidden_system, dirs) -> Vec<AppEntry>`: 根ごとに `Dedup.role` を差し替えて逐次に再帰する
  - `scan_directory_with_extensions(dir, extensions, include_folders, show_hidden_system, entries, dedup, dirs)`: 再帰の本体。
    フォルダは `file_name()`、ファイルは `file_stem()` で名付け、`dedup.accept(path)` が真なら `entries.push`
  - `root_roles` / `RootRole { check, record }` / `Dedup { set, buf, role }` / `Dedup::accept`
  - `sort_entries_canonical`: `target_path` → `name` → `is_folder` の全順序
- `snotra-core/src/indexer/scan/tests.rs`: `scan_with_extensions_*` / `scan_keeps_same_name_different_paths` の 4〜5 本は
  `scan_directory_with_extensions` と `Dedup` を直接組んで呼ぶ（シグネチャを変えると直す）
- `snotra-core/src/indexer/cache.rs`: `scan_and_sort_timed` が `scan_all` → `sort_entries_canonical` を通す唯一の製品経路
  （`rebuild_and_save` と cache-miss の `load_or_scan_with_stats_in` の両方）
- `scan_all` の呼び手: `cache.rs::scan_and_sort_timed`・`search/tests/common.rs`・`tests/memory_footprint.rs::report_scan_all_cost`・
  `src-tauri/src/icon.rs::icon_pipeline_cost_probe`（`#[ignore]`）
- `src-tauri/src/indexing.rs:231`: `rebuild_and_save` を専用スレッドから呼ぶ（rayon のワーカーの中ではない）

## 再利用できる既存のパターン

- `snotra-core` は rayon 1 に依存済み（`Cargo.toml`）。検索・kana・`index_tree` はグローバルプールを使う
- 上限を決めたプールの組み方は `search/tests/common.rs::pool_of`（テスト用）にある。製品コードで専用プールを建てる前例は無い

## 技術的な制約

1. **出力の中身は今と同じでなければならない。** `sort_entries_canonical` が全順序で並べ直すので、出力の順序そのものは問わない。
   問われるのは重複排除の勝者である:
   - 根をまたぐ重複: 先行の根が勝つ（`root_roles` の doc。根ごとに `extensions` / `include_folders` / `target_path` の字面が違う）
   - 同じ根の中の重複（正規化キーが同じ兄弟。大文字小文字だけ違う名前が大文字小文字を区別するディレクトリにある場合など）:
     `record` の根では「先に列挙した方」が勝つ。`record` が偽の根では落とさない（集合へ積まないので）
2. **`Dedup::accept` の照合分岐は確保しない**（`normalize_entry_key_into` で 1 本のバッファを使い回す）。この性質は CI が守らず、
   `tests/memory_footprint.rs` の確保回数だけが見る
3. `dirs` は `read_dir` に成功したディレクトリごとに `Relaxed` で 1 回足す。読み手は別スレッドの進捗表示。並列でもそのまま使える
4. `DirEntry::metadata` は Windows ではリンクを辿らない。ジャンクションとシンボリックリンクは `is_dir()` が偽で、再帰しない
   （循環しない）。並列にしても変わらない
5. 検索・kana が使うグローバルプールを走査で塞がない（issue の指定）。走査中も設定画面のフォルダ展開などは動きうる

## 設計の候補

- A. `Scope::spawn` で再帰し、結果を `Mutex<Vec>` かワーカーごとの束へ集める。根の中の重複の勝者は列挙順で決まらなくなるので、
  決定的な規則（例: パスが小さい方）へ替える必要がある（issue の案）
- B. fork-join: 各ディレクトリで列挙を 1 スレッドで終え、子ディレクトリだけを `par_iter` で再帰し、結果を列挙順に連結して返す。
  根ごとの結果は今の逐次と**同じ順序**になるので、根が終わってから今の `Dedup::accept` を同じ順に当てれば、勝者の規則も
  確保の性質も変わらない。代価は連結のコピー（エントリ 1 件あたり深さ回の move）と、ディレクトリごとの小さな `Vec`

B は速度で A に並んだ（上の 2 回目）。B を採る（挙動を変えないことを構成で示せる）。

## 敵対的調査（3b）の所見と採否（`workspace/adversarial-1455.txt`）

- 壊せた: `scan_and_sort_timed` の呼び手は 3 つ（`load_or_scan_with_stats` の `config_dir()` が `None` の分岐、`cache.rs:621`）。**採る**（上の記述は「唯一の製品経路」としか書いておらず、呼び手を数えた箇所は無いが、3 つと明記する）。結論には影響しない
- 壊せなかった: 争点 1（fork-join の前順と後段の `accept` で出力が同じ）・2（根の中の重複の扱いの読み）・3（ジャンクション/シンボリックリンクは `is_dir` が偽・実測）・5（呼び手はどれも rayon の外。製品の入口は `indexing.rs` の `std::thread::Builder`）・6（上限 8 の根拠は 1 台のみ）
- 採る指摘:
  - 収集側の前段の絞り込み（名前が空・拡張子・`include_folders`・隠し属性）を逐次と同じ位置に置く → 計画の設計に明記した
  - 根の中の大文字小文字だけ違う重複を直接固定するテストが無い → 計画に足した（`scan_all_dedups_in_enumeration_order_within_a_root`）
  - 指紋が見たのはパスだけで、`AppEntry` の組み立てと `accept` 後は見ていない → 受け入れ条件 3 の `index.bin` のバイト比較が見る
  - スパイクと製品の µs/dir の差（製品 44.5〜45.9 µs/dir @ 11-15・`dirs` 613,341、スパイク 39〜42 µs/dir @ 11-17〜18・`dirs` 約 68 万）は未説明 → 判断は製品の A-B-A（同日・同じ状態）で行う。スパイクの数は採否の目安にとどめる
  - 「冷えた」が 2 つの意味で使われている（scan-log の 85.2 s は起動直後の製品、スパイクの 116.9 s は温め前の 1 回） → PERFORMANCE.md で書き分ける
  - 8 は物理コア数（16 論理 = 8 物理 × SMT）かもしれない → `available_parallelism()` は論理数を返すが、上限 8 で切るので本機では同じ。他の機械での最適は未知と記録する
- 採らない指摘:
  - ⚠️ rayon ワーカーの既定スタック（2 MiB）で、待機中の steal が積み重なると逐次より深くなりうる → スタックの大きさを変えない。今の逐次も既定のスタック（`snotra-index-build` は `stack_size` を指定しない）で同じ再帰を回しており、スパイクは実運用点（最大深さ 22・68 万ディレクトリ）を既定のスタックで完走した。測っていない大きさの定数を足さない。PERFORMANCE.md の「測っていないもの」に残す
  - ⚠️ ワーカーの優先度・冷えた状態の並列・HDD での逆効果 → 測っていないものとして記録する。優先度を下げる変更はこの issue の範囲外
  - issue 本文の「約 31 万件」は旧 config の数（現 config の `scanned` は 23,869 件） → 基準に使わない（計画は使っていない）

## 未解決の疑問

- 並列度の上限（8 が最良・16 で悪化）は CPU の数に依るのか、ファイルシステムのフィルタ（Defender）の直列化に依るのか。
  他の機械では未知。`available_parallelism().min(8)` とする根拠は、この 1 台の測定だけ
- 冷えた状態の並列の効果は未測定
