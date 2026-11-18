# research — #1479 パスクエリの全件走査が #1463 の後 5〜13% 遅い

## issue の要約

#1463（#1450 の修正）の前後（A = `56805e3c` / B = `63839351`）を GPDWINMini の実運用点（実 `index.bin` 337,323 件・実 `history.bin`・migemo OFF）で A → B を 3 往復して測ると、実索引のパスクエリ 18 行中 17 行で B の中央値が A を上回った（比 0.99〜1.13）。Fuzzy・PATH マージありの `c:\` `c:\users\` `\bin\` はレンジが重ならない。結果（件数・ダイジェスト）は全行一致。受け入れは「原因を 1 つに絞る。直すなら名前クエリの利得（`c`）を保ったままパスクエリを A のレンジへ戻す。切り離せないならトレードオフを `PERFORMANCE.md` に記録して閉じる」。

## 関連するファイル・シンボル

- `snotra-core/src/search/scoring.rs`
  - `score_one_entry`（`#[inline]`）— #1463 で引数 `history: &HistoryStore` → `&HistoryLookup`、`finish` クロージャ（`ScoredEntry` の構築を 2 出口で共有）、`if let Some(base_score) = score && !history.may_hit(..)` の早期 return を追加
  - `HistoryLookup { store, tails: FxHashSet<&str> }`・`HistoryLookup::new`・`may_hit`・`TAIL_BUF`
  - `with_normalized_key` → `path_store::with_cursor`
- `snotra-core/src/search/path_store.rs`
  - `PathStore::push_own_segment`（#1463 で新設・根なら `trim` の分岐）、`key_tail_into`、`key_tail`、`normalized_into`（参照実装・製品は呼ばない）
  - `PathCursor::normalized` — #1463 で根を書く 2 か所（`i` 自身が根のとき・チェーンが外れて根から作り直すとき）を `push_own_segment` 経由に変更。`PathCursor::append` は直書きのまま
- `snotra-core/src/search.rs` `search_with_options` — `HistoryLookup::new(history)` を検索ごとに 1 回（360 行付近）、`score_one_entry` の唯一の製品呼び出し点（398 行付近）
- `snotra-core/src/history.rs` `HistoryStore::path_keys`
- 計測コード: `snotra-core/tests/match_cost.rs`（`REAL_QUERIES` にパス 3 種 `c:\` `c:\users\` `\bin\`、`SYNTHETIC_QUERIES` に `c:\data\`。`normal_mode` は 3 つとも掃く）。`tests/path_query_cost.rs` はパスクエリ専用の計測コード
- 記録: `PERFORMANCE.md`「採用: 履歴に当たりえない候補で正規化キーを組み立てない（#1450）」— HAL9000 でも合成 `c:\data\` で B が 5% 遅い系列と、重なった系列の両方がある。「B が遅く出るブロック」（一致の少ない対照のクエリまでそろって遅い）も記録されている

## パスクエリが #1463 の差分のどこを通るか（コードを読んだ結果）

- パスクエリ（区切りを含む）では、名前・file name に区切りが無い限り名前照合は成立しない（Fuzzy は `skip_name` で照合自体を飛ばし、Substring/Prefix は `find`/`starts_with` が失敗する）。よって `score` は `None` で、`may_hit` の分岐（`score` が `Some` のときだけ評価）に入らない
- `score.is_none() && plan.path_query.is_none()` は偽なので `with_normalized_key` へ進む。カーソルが組み立てる順序（どのエントリでカーソルを通るか）は A と B で同じ——#1463 が省くのは名前で一致した候補だけで、パスクエリではそれがほぼ無い
- B でパスクエリが新たに払うもの:
  1. `HistoryLookup::new`（検索ごと 1 回・履歴キー数に比例。実 `history.bin` は 4,040 B）
  2. `PathCursor::normalized` の根の書き方が `push_own_segment`（`parent == NO_PARENT` の分岐 + `entries[i]` の読み 1 回）。走るのは「`i` が根」か「チェーンが外れた」ときだけで、索引順に走査する限り大半は `append` の経路
  3. `score_one_entry` の codegen の変化（引数型・クロージャ・早期 return の追加でインライン化・レジスタ割り付けが変わりうる）。パスマッチが成立した候補（上限 200 件ではなく一致全件）では `history.store.*` で間接参照が 1 段増える
- 差の大きさ: 実索引 Fuzzy `\bin\` で中央値 ~0.5〜0.7 ms／検索。337k 件で割ると 1 件あたり 1.5〜2 ns。検索 1 回 1 度の `HistoryLookup::new` がこれを生むには履歴が数万キー要るので、1 単独では説明しにくい（未測定）

## 再利用できる既存のパターン

- A/B の作法: `PERFORMANCE.md` の各節（交互に回してレンジで読む）。ドリフト対照として A を取り直す（メモリ ab-baseline-needs-drift-control）
- 計測専用の worktree + 独立 `CARGO_TARGET_DIR` でビルドし、テストバイナリを直接実行（#1450 の測り直しで使った）
- `PathCursor::append` を共有から外した前例（#1463 の `/simplify` で「共有を入れると実索引のパスクエリが遅く出た」ので戻した）。同じ処置を根の 2 か所に当てる案がある

## 技術的な制約

- 結果を 1 バイトも変えない（`match_cost` の全行のダイジェスト一致）
- 正規化キーの規則の写しを増やさない（`snotra-core/CLAUDE.md`「`normalize_entry_key` の冪等性契約」・一致は `path_store_cursor_matches_*` と `key_tail_into_matches_cursor_key_tail*` が守る）
- `HistoryLookup` の論証（末尾セグメントが集合に無ければ 3 表すべて 0）を崩さない
- 現在の main は B の後に #1477（索引の走査を専用プールで fork-join）が入っている。直す差分は main に当てるので、A/B の基準も main に取り直す必要がある
- 計測は GPD WIN MINI（ノート機・ブースト/熱の影響あり）。#1450 の測り直しは常に A → B の順だった（B が常に A の直後）

## 敵対的調査（3b）の採否 — `workspace/adversarial-1479.txt`

| 所見 | 判定 | 採否と理由 |
|---|---|---|
| F3 命題「B の新費用は 3 つに尽きる」 | 壊せた | **採る。** 全称を撤回する。漏れとして `push_segment` の呼び出し点の増加による inline 判断の変化（`append` への波及）と、バイナリ配置の揺れを足す。配置の揺れが ±5% 級であることは下の V の測定で裏付けた |
| F6 1A の汚染（v10 再構築の直後） | 壊せた | **採る。** 汚染の向きは B に有利だった。「A→B 固定順だから B が不利」という含意を撤回する |
| F6 ABAB 補正後も B の退行は残る | 壊せた（前提を） | **採る。** 下の逆順込みの測定でも、B は R 比 +7.2% で再現した |
| F7 #1477 は検索の走査ではない | 壊せた | **採る。** `git diff 63839351 HEAD -- snotra-core/src/search/` はコメント 1 行だけ。「main で取り直す必要」は、比較の相手を「main から #1463 を戻した変種」とする意味で残す |
| F5 1 件あたり ns の算術 | 壊せた | **採る。** 壁時計 ÷ 件数で、16 スレッドの並列を無視していた。結論（分岐 1 つでは説明しにくい）はむしろ強まる |
| F8 レンジが重ならないのは Fuzzy だけ／合成は履歴が空 | 壊せた | **採る。** 実運用の `normal_mode = "substring"` の行は重なる。合成で出る差は `HistoryLookup::new` と無関係 |
| F1・F2・F4 前半・F5 本論・F9 の事実 | 壊せなかった | そのまま残す |
| ⚠️ F4「チェーンの外れは稀」 | 確信なし | **採る（主張を弱める）。** 外れ率は測っていない。ただし外れても (b) の追加は分岐 1 つと読み 1 回 |
| ⚠️ F6 原因の特定 | 確信なし | 下の測定で決着させた |
| ⚠️ F9 worktree の記述に一次資料が無い | 確信なし | このセッションの #1450 測り直しの手順であり、記録は #1450 のコメントには無い。事実としては残すが、根拠は本ファイルの測定手順の節とする |

## 測定（2026-11-18・GPD WIN MINI・release・rustc 1.99.1・`tests/match_cost.rs`・実 `index.bin` 337,323 件・実 `history.bin`）

変種（それぞれ独立の worktree と `CARGO_TARGET_DIR`）:

- A = `56805e3c`（#1463 の直前）、B = `63839351`（#1463）
- M = main `367d9716`、R = M から `git revert 63839351`（衝突なし）
- V = M から `PathCursor::normalized` の根の 2 か所だけを `push_own_segment` 以前の直書きに戻した変種（候補 1 の検証）

**系列 1**（A,B,M,R,V を順・逆・順・逆の 4 巡＝各 4 回）。値は行ごとの p50 の中央値を R で割った比の幾何平均:

| 群 | A | B | M | V |
|---|---:|---:|---:|---:|
| 実・パス 18 行 | 0.997 | **1.072** | 1.013 | **1.058** |
| 実・パス Fuzzy 6 行 | 1.010 | 1.095 | 1.012 | 1.107 |
| 実・`c` 6 行 | 1.013 | 0.867 | 0.891 | 0.856 |
| 合成・名前 30 行 | 1.021 | 0.899 | 0.870 | 0.996 |

**系列 2**（M,R を 20 秒の間隔を空けて MR/RM を交互に 6 往復＝各 6 回）:

| 群 | M / R |
|---|---:|
| 実・パス 18 行 | 1.017 |
| 実・パス Fuzzy 6 行 | 1.022 |
| 実・パス Substring 6 行 | 1.007 |
| 実・`c` 6 行 | **0.869** |
| 合成・パス 6 行 | 1.012 |
| 合成・名前 30 行 | **0.903** |

- 系列 2 の実・パス 18 行は、**すべての行で M と R のレンジが重なった**（行ごとの比 0.98〜1.05）
- 全系列・全変種・全行で、件数とダイジェストが一致した

**読み**:

1. B（#1463 単体のビルド）の退行（+7%）は、逆順を混ぜても再現した。実在する
2. 現 main（M）では +1〜2% で、行ごとのレンジは全部重なる。名前クエリの利得（`c` で −13%）は保たれている
3. 候補 1 を戻した V は、M よりパスが**遅く**（1.058）、合成の名前クエリの利得まで消えた（0.996）。まれにしか走らない分岐を直書きに戻すだけで、無関係の行が ±5〜10% 動いた。B の退行も、仕組みではなくビルドごとのコード配置・インライン化の揺れとみるのが最も素直である
4. 以上から、現 main に対して直すべき退行は測れる大きさで残っていない。2% 前後の残差は、V が示した配置の揺れより小さく、変種を作って帰属させることもできない

## 未解決の疑問

1. ~~退行は実在するか~~ → B では実在（測定の読み 1）
2. ~~現在の main でも同じ差が出るか~~ → 出ない（+1〜2%・全行でレンジが重なる。読み 2）
3. ~~3 候補のどれが効いているか~~ → 候補 1 は否定（V は改善しない）。候補 3 は合成（履歴が空）でも差が出たことから主因ではない。候補 2 と配置の揺れは切り分けられない（読み 3・4）
4. ~~名前クエリの利得と切り離せるか~~ → 現 main では切り離す対象が残っていない
