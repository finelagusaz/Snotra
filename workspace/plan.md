# 計画: #1455 索引の走査を、専用プールで fork-join の並列にする

## 目的

`scan_all` の再帰を、上限を決めた専用の rayon プールで並列に回す。実運用点の温かい状態で 27 秒 → 6 秒前後を見込む
（`workspace/research.md`「スパイクの結果」）。**出力（要素と重複排除の勝者）は今と 1 件も変えない。**

## 受け入れ条件

1. スパイクの結果（並列度ごとの所要・fork-join と Scope 形の比較）を `PERFORMANCE.md` に記録する
2. `scan_all` の出力の列（整列の前）が、プールのスレッド数によらず同じで、今の逐次と同じ前順（ディレクトリのエントリを列挙順に並べ、
   フォルダの直後にその中身を置く順）になる。テストで固定する
3. 実運用点で A-B-A（基準 → 新 → 基準）の `/s` を回し、`scan-log.jsonl` の `scan_ms / dirs` が縮み、`dirs` が揃い、
   `index.bin` のバイト列（先頭の `built_at` を除く）が基準と一致する。一致しなければ、差分のエントリを列挙し、
   実行の間のファイルの増減で説明できることを示す
4. 既存のテスト（`scan_all_*`・`root_roles_*`・`scan_*`）が通る

## 設計（research.md「設計の候補」の B）

- ディレクトリ 1 つの処理（新しい関数 `scan_directory`）:
  1. `read_dir` に成功したら `dirs` を 1 足す（今と同じ位置・同じ `Relaxed`）
  2. エントリを列挙順に 1 スレッドで見て、`Item` の列を作る: フォルダは「`include_folders` なら `AppEntry` を伴う」形で、
     子として再帰する印をつける。ファイルは拡張子が合えば `AppEntry`。前段の絞り込み（`metadata` の失敗・隠し属性・
     名前が空・拡張子・`include_folders`）は今の `scan_directory_with_extensions` と同じ位置・同じ条件に置く。
     `accept` だけを後段（`scan_all`）へ移す
  3. 子フォルダだけを `par_iter` で再帰し、`Vec<Vec<AppEntry>>` を受け取る
  4. 列挙順に連結して返す（フォルダの `AppEntry` → その子の結果、の順）。**今の逐次と同じ前順である**
- `scan_all`:
  - `root_roles` と `Dedup` は今のまま
  - 専用プールを 1 回建てる（スレッド数 `available_parallelism().min(SCAN_THREADS_MAX)`、`SCAN_THREADS_MAX = 8`、
    スレッド名 `snotra-scan-<i>`）。建てられなければ、グローバルプールで同じ関数を回す（結果は同じ）
  - 根を順に、`pool.install(|| scan_directory(..))` で 1 根ぶんの列を受け取り、**その列を順に `dedup.accept(&e.target_path)` へ
    当てて、真のものだけ `entries` へ移す。** 根が終わるまで集合へ触れないので、根をまたぐ勝者も根の中の勝者も今と同じ
  - 集合が無い（`dedup.set == None`）ときも `accept` は真を返すので、分岐を足さない
  - プールは `scan_all` の終わりで drop する（スレッドが終わる）
- 内部の入口 `scan_all_with_threads(scan_paths, show_hidden_system, dirs, threads)` を置き、`scan_all` はスレッド数を決めて呼ぶだけにする
  （テストが 1 と 8 以上を比べるため。公開面は増やさない: `fn` のまま private）

### 変更するファイルとシンボル

- `snotra-core/src/indexer/scan.rs`
  - `scan_all`（本体を `scan_all_with_threads` へ移す）・新規 `scan_all_with_threads`・新規 `SCAN_THREADS_MAX`
  - `scan_directory_with_extensions` を `scan_directory`（`-> Vec<AppEntry>`・`entries` / `dedup` 引数を外す）へ置き換える
  - `Dedup::accept` は変えない。doc の「`scan_all` の結線」の記述を新しい結線（根ごとの後段で当てる）に合わせる
  - `//!` に「走査は専用プールで並列・出力は逐次と同じ前順」を 1 文足す
- `snotra-core/src/indexer/scan/tests.rs`
  - `scan_with_extensions_filters_by_ext` / `scan_with_extensions_includes_folders` / `scan_with_extensions_no_folders_when_disabled` /
    `scan_keeps_same_name_different_paths` / `scan_extensions_case_insensitive` を `scan_directory` の呼び出しへ直す
    （`Dedup` を組んでいた箇所は外す。`scan_keeps_same_name_different_paths` は同名異パスが 2 件残ることを `scan_all` 経由で見る）
  - 新規テスト 2 本（下の「テスト方針」）
- `snotra-core/CLAUDE.md`「`scan_all` の重複排除」: 「重複排除は根ごとの列へ後段で当てる（並列の走査の中では触れない）」を 1 文足す
- `PERFORMANCE.md`: 新しい節「採用: 索引の走査を専用プールで fork-join の並列にする（#1455）」。スパイクの 2 表・A-B-A・測っていないもの
  （冷えた状態の並列・他の機械での上限・8 が物理コア数〔本機は 16 論理〕と FS の飽和のどちらで決まるか・ワーカーの深いスタック・
  ワーカーの優先度・照合で落とす件も `target_path` を確保してから捨てること・走査中のピークのメモリ・`scan_all` ごとにスレッドを建てて捨てる代金）。「冷えた」は、scan-log の起動直後の製品（85.2 s）と、スパイクの温め前の 1 回（116.9 s）を書き分ける。
  スパイクの数は採否の目安で、効果の額は製品の A-B-A で言う
- `snotra-core/tests/memory_footprint.rs`: 変えない（`report_scan_all_cost` は `scan_all` を呼ぶだけ）。確保回数が増えることは
  PERFORMANCE.md の節に書く（ディレクトリごとの `Vec`。照合分岐の確保ゼロは保たれる）

- 走査の所要を「22〜30 秒」と現在形で書いた散文（並列で偽になる）。数を写さず「全走査」と書く（所要の正本は PERFORMANCE.md の新しい節）:
  - `snotra-core/src/indexer/cache.rs` の `config_hash` 周りの doc（「背景の構築で 22〜30 秒の全走査を払う」）
  - `snotra-core/src/index_tree.rs` の wire 形式の doc（「cache-miss ＝ 22〜30 秒の全走査へ落ちる」）
  - `snotra-core/src/str_arena.rs` の `LowerFileName` の線上表現の doc（同上）
  - `snotra-core/src/search/build.rs` の `compute_wave1` の kana 分岐のコメント（「cache-miss（走査 22〜30 秒）の内側だけ」）
  - `PERFORMANCE.md`「`compute_wave1` の kana 分岐は逐次のまま残してある」の括弧（同上）。同じ文書の「額は実運用点では測っていない。
    消えるのは全走査（issue が記録した 22〜30 秒…）」は当時の issue の引用なので残す
  - `snotra-core/src/indexer/cache.rs` の起動時の読み込みの doc と `snotra-core/src/indexer/cache/tests.rs` の対応するテストの doc
    （「22〜85 秒のあいだトレイもホットキーも出ない」）
  - `snotra-core/tests/path_query_cost.rs` の `show_hidden_system` を掃かない理由（「全走査（22〜30 秒）が走り」）
  - `PERFORMANCE.md` の冒頭の規約「計測コードが黙っていることと…」の「`TotalProcessorTime`（走査中は 1 コアを使い切る）」。
    並列では複数コアになるので「走査中は増え続ける」へ直す（信号としての使い方は変わらない）
  - 探し方: `git grep -n -e '22〜30' -e '22〜85' -e '1 コアを使い切'`（`docs/adr/`・`docs/superpowers/` は凍結された歴史なので除く）
  - kana 分岐を逐次のまま残す理由は「走査に比べて小さい」だった。走査が約 6 秒になると比が変わる（kana の逐次は約 1 秒・同節）。
    並列化するかはこの issue の範囲外とし、PR 本文で別 issue の候補として挙げる

`SPEC.md` は変えない（走査のスレッド数は仕様に書かれておらず、進捗のフォルダ数・記録の形式・索引の中身は変わらない）。

## 不変条件と異常系

- 出力の列は前順で、スレッド数に依らない（`par_iter().map().collect()` は添字の順を保つ）
- 重複排除は根ごとに、列の順に当てる。根の中の勝者・根をまたぐ勝者とも今と同じ。`Dedup::accept` の照合分岐は確保しない（変えない）
- `dirs` は `read_dir` 成功ごとに 1 足す。並列でも合計は同じ（既存テスト `scan_all_counts_every_directory_it_reads_even_when_it_registers_nothing`）
- リンクは辿らない（`DirEntry::metadata` は Windows でリンクを辿らず、ジャンクションは `is_dir()` が偽）。循環しない。今と同じ
- 走査中のパニック: `install` が呼び出し側へ伝える。今の逐次と同じく、走査スレッド（`indexing.rs` の専用スレッド）で落ちる
- プールを建てられない: グローバルプールで回す（結果は同じ。検索と競合するだけ）
- `scan_all` を rayon のワーカーの中から呼ぶ経路: 製品には無い（`scan_and_sort_timed` は `indexing.rs` の専用スレッドと起動時の経路から呼ばれる）。
  ワーカーの中から別プールの `install` を呼んでもデッドロックはしない（rayon-core の `Registry::in_worker_cross` は、待つ間に呼び出し側のプールの仕事を処理しながら完了を待つ）
- 深い再帰: 1 段ごとに `par_iter` の join がスタックに積まれ、待機中のワーカーの steal で逐次より深くなりうる。スタックの大きさは
  変えない（今の逐次も既定のスタックで同じ再帰を回す・スパイクは実運用点の最大深さ 22・68 万ディレクトリを既定のスタックで完走した）。
  release は `panic = "abort"` なので、溢れれば即終了する。PERFORMANCE.md の「測っていないもの」に残す

## テスト方針

Red → Green: `scan_directory` を先にシグネチャだけ（中身は `Vec::new()`）で置き、新規テストが赤になることを見てから実装する。

- 新規 `scan_all_returns_the_same_preorder_regardless_of_threads`: `SCAN_THREADS_MAX` を跨ぐ数（例 `SCAN_THREADS_MAX * 4`）の
  子フォルダを持つ fixture（各子に `.exe` とさらに孫フォルダ）を作り、`include_folders = true` で `scan_all_with_threads` を
  1 スレッドと `SCAN_THREADS_MAX` スレッドで呼び、**整列前の列**が一致し、かつ期待する前順（fixture から組んだ列）と一致することを見る
- 新規 `scan_all_dedups_in_enumeration_order_within_a_root`: 2 つの根を重ね（先の根が `record`）、先の根の下に
  大文字小文字を区別するディレクトリ（`fsutil file setCaseSensitiveInfo <dir> enable`）を作り、`Tool.exe` と `tool.exe` を置く。
  勝者（残る 1 件）が、そのディレクトリを `std::fs::read_dir` で列挙したときの先頭と一致することを、1 スレッドと
  `SCAN_THREADS_MAX` スレッドで見る。`fsutil` が失敗する環境（WSL の機能が無い CI ランナーなど）では、理由を出力してスキップする
- 既存の `scan_all_dedups_when_roots_are_nested` / `..._child_root_comes_first` / `scan_all_counts_every_directory_..` が、根をまたぐ重複と
  `dirs` を守る
- 変異注入（委譲先が行う）: 連結の順を逆にする・`dedup.accept` を `pool.install` の中（並列の中）で当てる形に戻す・プールを
  1 スレッド固定にする（速度だけが変わる → 検知器が無いことを報告させる）

検証: カテゴリ A（fmt / clippy / test / doc）と、実運用点の A-B-A（受け入れ条件 3。手順は下）。

### A-B-A の手順（受け入れ条件 3）

1. 基準 = この計画の分岐元（origin/main）の release exe、新 = 実装後の release exe を、scratchpad へ別名で置く
2. 静かな状態（ビルドを走らせない）で、温めの基準 1 回のあと、基準 → 新 → 基準 → 新 → 基準 → 新の順に、`index.bin` を消して各 exe を
   起動する（`/s` の代わりに cache-miss の起動の背景の構築〔#1448〕で走査させる。同じ `scan_and_sort_timed` を通り、`scan-log.jsonl` に
   記録される。GUI への入力の注入が要らない）。終了の行が書かれたら `index.bin` を scratchpad へ写して終了する。実 `index.bin` は最初に
   退避し、最後に戻す（道具は scratchpad の `aba.ps1`）
3. `scan_ms / dirs` を版ごとに並べる。`index.bin` は先頭の varint（`built_at`）を除いてバイト比較する
4. 不一致なら、`load_cached_entries` 相当でエントリの集合を出して差を列挙する（scratchpad の一時の道具）

## 新しく導入する用語

- 「前順」（pre-order）: 各ディレクトリで、エントリを列挙順に並べ、フォルダの直後にその中身を置く順。標準の語で言えるが、
  初出に括弧で原語を添える（(A)）。他のファイルには無い語なので glossary には足さない
- 「fork-join」: 原語のまま書く（(B)・標準の語）

## 作業項目

### Phase 1: 走査の並列化

- [x] `scan.rs` に `SCAN_THREADS_MAX` と `scan_directory`（中身は空）・`scan_all_with_threads` を置き、新規テストを書いて赤を見る
- [x] `scan_directory` を実装し、`scan_all` / `scan_all_with_threads` を専用プールと根ごとの後段の重複排除へ組み替える
- [x] 既存テストを `scan_directory` の呼び出しへ直す
- [x] `scan.rs` の `//!`・`Dedup::accept` の doc・`snotra-core/CLAUDE.md`「`scan_all` の重複排除」を新しい結線に合わせる（`Dedup::accept` の doc は「`scan_all` の結線の退行」が今も真なので変えない）
- [x] 走査の所要と 1 コアを前提にした散文（「`SPEC.md` は変えない」の直前の列挙・cache.rs 2 か所・cache/tests.rs・index_tree.rs・
  str_arena.rs・search/build.rs・tests/path_query_cost.rs・PERFORMANCE.md 2 か所）を、数を写さない形へ直す
- [x] 変更した crate の fmt / clippy / test を手元で通す
- [x] ラウンド 1: d79b7038・verify:tree green（A/E/F）・指摘 High 1 / Medium 2 / Low 5・生き残った変異 1（スレッド数 1 固定・速度だけが変わるので検知器を置かない）。用語集に「前順」・CI ではスキップを失敗に・`Dedup` の doc・グローバルプールの条件・連結の確保と `par_iter` を子フォルダだけへ、を直した。PERFORMANCE.md の節は Phase 2 で足す
- [x] ラウンド 2: a646ad4f・verify:tree green（A/E/F）・指摘 Low 2（`subdirs` と `Dir` の数のずれを `debug_assert!` で止める・測っていないものを計画の PERFORMANCE.md の項へ足す）・生き残った変異 0（スレッド数 1 固定は対象外）。両方直した
- [x] ラウンド 3: 799a9439・verify:tree green（A/E/F）・指摘 Medium 1（新の版の sha）/ Low 6・生き残った変異 0（ただし `debug_assert!` は片方向だけ）。sha・件数の `debug_assert_eq!`・表の µs / dir・冷えた状態の列挙・「退けて」→「消して」・「ツリーの深さ」を直した。節の置き場所（メモリの章の下）は直さない（#1002 の走査の節と隣り合わせるため）
- [x] ラウンド 4: c2a3720a・指摘 Low 1（「差は PERFORMANCE.md だけ」が偽 → 「差に製品のコードは含まれない」）・生き残った変異 0（`debug_assert_eq!` は両方向で赤）。直した

### Phase 2: 実測と記録

- [x] A-B-A を回し、`scan_ms / dirs` と `index.bin` のバイト比較を取る
  - 結果: 基準 24.35〜24.61 s・新 5.48〜5.62 s（約 4.4 倍）・`dirs` 688,043 と `scanned` 30,932 は全 7 回で一致・`index.bin` は `built_at` を除いて全 7 回でバイト一致
- [x] `PERFORMANCE.md` に「採用: 索引の走査を専用プールで fork-join の並列にする（#1455）」を書く（スパイクの 2 表・A-B-A・測っていないもの）

## 未確定（実装前に潰す）

- [x] 根の中の重複の勝者を直接見るテストを置けるか — 2026-11-18 にこの機械で非管理者のまま `fsutil file setCaseSensitiveInfo <dir> enable`
  が成功し（exit 0）、`Tool.exe` と `tool.exe` が同じディレクトリに並んだ。新規テストに足す（「テスト方針」）。CI ランナーで作れる保証は
  無いので、失敗したらスキップする形にした

## 人間レビュー

- [x] 承認済み — 2026-11-18 / 問い: "この計画で実装へ進めてよろしいですか？" / 回答: "承認する"

## plan-review 結果

- リスク: 高（worker・共有状態を変える）
- レビュー方式: 独立導出 1 体（Step 2b・`workspace/plan-review-indep-1455.md`）
- エージェント数: 1

### 要対処

- 並列で偽になる散文が計画の列挙より多かった（cache.rs の起動時の読み込みの doc・cache/tests.rs・tests/path_query_cost.rs・
  PERFORMANCE.md の「1 コアを使い切る」）— 計画の修正（「`SPEC.md` は変えない」の直前の列挙と Phase 1 の作業項目へ足した）— 各行を `git grep` で再照合した

### 軽微

- 導出は Scope 形 + 勝者の規則の変更（パスが小さい方）を選んだ。計画は fork-join で列挙順を保つので、勝者の規則も
  `memory_footprint.rs` の「件数も順序も一致」も `icon_pipeline_cost_probe` の `take(200)` の標本も変わらない。差は設計の違いであり、漏れではない
- 導出はプールを建てられないとき「呼び出し側のスレッドで逐次」を推した。計画はグローバルプールで回す（結果は同じ）。逐次へ落とすには
  並列でない 2 本目の再帰が要り、スレッドを作れないほどの状態のためだけに写しを持つことになるので採らない
- 3 根の入れ子で `(true, true)` の根を通すテスト: 役割の割り当てと `accept` は変えないので足さない（`root_roles_*` が役割を守る）
- `ADR-mtime-differential-scan-ceiling` が指す `snotra-core/tests/dir_stat_cost.rs` が現存しない: ADR は凍結された歴史なので直さない

### 未検証

- 冷えた状態の並列・他の機械の最適な上限（PERFORMANCE.md の「測っていないもの」へ）

### 判断

- 実装着手: 可（人間の承認待ち）

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー1体
- エージェント数: 1（ほかに 3b の敵対的調査 1 体）
- 要対処: 1 件（並列で偽になる散文 4 か所を計画へ足した）
- 未検証: 冷えた状態の並列・他の機械での上限
- 散文の全件列挙: 語 3 / 総件数 195 行 / ★未掲載 14（計画へ足した 0。`scan_all` の再エクスポート・呼び手・無関係の「逐次」で、偽にならない） /
  ☆行未掲載 0（計画へ足した 0）。偽になる散文は「22〜30」「22〜85」「1 コアを使い切」の別の語で拾い、10 行中 9 行を計画へ足した（残る 1 行は当時の issue の引用）
- 試行（#1300・高リスクのみ・独立レビュー 1 回ごとに 1 行）: 方式 Step 2b / 設計変更 0 / 主の見落とし 4
