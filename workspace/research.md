# 調査 — issue #1442

## 要約

`search/path_store.rs` の `PathCursor` は `thread_local!` の `CURSOR` に住み、`stack: Vec<(u32, u32)>`（index と buf 長）だけを持つ。どの `PathStore` で組み立てたかを持たないので、索引を差し替えた後の最初の走査で、新しい索引の親 index が古いチェーンに「当たり」、古い索引の接頭辞へ新しいセグメントを書き足す。結果、正規化キーが `normalize_entry_key` とずれ、履歴照合（ブースト）が外れる・別エントリに当たる。パスマッチの照合位置もずれうる。

あるべき姿: 索引を差し替えても、カーソルの出力が `normalize_entry_key` と一致する。直し方の候補は issue が示す「`PathStore` に一意な世代（グローバル `AtomicU64`）を持たせ、世代が違えばチェーンを捨てる」。

分類: バグ（`PathCursor::normalized` の doc が「`normalize_entry_key` と 1 バイトも違わない」を契約として書いている。挙動をその契約に合わせる）。`SPEC.md` は変えない。

## 関連するファイル・シンボル

- `snotra-core/src/search/path_store.rs`
  - `PathStore`（フィールド `entries` / `names` / `table` / `sorted_prefix_len`）
  - `PathStore::adopt` — 製品で `PathStore` を作る唯一の入口（`PathStore::build` は `#[cfg(test)]` で `adopt` を呼ぶ）。呼び出しは `search/build.rs:228`
  - `PathStore::footprint_rows` — `let Self { .. }` を `..` 無しで分解している。フィールドを足すとここも直す（コンパイルエラーで気づく）
  - `PathCursor`（`buf` / `stack`）、`PathCursor::new`（`const fn`・`thread_local!` の `const` 初期化に使う）、`PathCursor::normalized`、`PathCursor::append`
  - `with_cursor`、`thread_local! CURSOR`
  - `CMP_BUFS`（`cmp_paths` のバッファ）— 毎回 `raw_into` が `clear` から書くので状態を持ち越さない。同じ欠陥は無い
- `snotra-core/src/search/scoring.rs`
  - `with_normalized_key`（`with_cursor` の唯一の呼び出し元）。呼び出しは `score_one_entry`（`scoring.rs:521`）と `recent_history`（`search.rs:476`）
  - `MATCHER` / `PATH_MATCH_BUF` — 使い回しのスクラッチで、索引に依存する状態を持たない
- `snotra-core/src/search/tests/path.rs`
  - `cursor_fixture`、`path_store_cursor_matches_full_rebuild`（コメント「走査ごとにチェーンを空から始める（カーソルの寿命 = 1 走査）」）、`path_store_cursor_matches_normalize_entry_key_over_real_index`（`#[ignore]` は付いていない。実 `index.bin` が無ければ自動スキップする）
- `snotra-core/src/history.rs:47` — `static NEXT_SAVE_SEQUENCE: AtomicU64` の既存例（グローバル連番）
- 文書: `docs/architecture.md:281`、`PERFORMANCE.md`（`PathCursor` の記述）、`snotra-core/CLAUDE.md`「`normalize_entry_key` の冪等性契約」

## 影響を受ける製品の経路

1. 検索の `score_one_entry` — 履歴照合（`history` のキー）とパスマッチ（`has_path_sep` のクエリ）が正規化キーを使う
2. 空クエリの `recent_history` — 最近起動のキーを索引から引く。全件を組み立てるので 1 スレッドでは index 0（根）で戻るが、複数 worker では 2 番目以降の葉の先頭で誤り、recent の一部を取りこぼしうる（3b で 4/8 スレッドのプールにて 15 回中 4 回・直近 8 件のうち 1〜2 件を取りこぼしを実測。非決定的）

どのスレッドの `CURSOR` を使うかは分割の有無で決まる（rayon 1.12.0 の `bridge_producer_consumer` の `helper` をローカルの registry で読んで確認した）。分割しないとき（`len / 2 >= min` が偽＝`with_min_len(512)` の検索で候補 1024 未満）は**呼び出しスレッドの上で** fold する——製品では search worker（プロセス寿命）、テストでは libtest のスレッド。分割するときは `join_context` を通り、呼び出し元が worker でなければグローバルプールの worker へ渡る。`recent_history` は `with_min_len` を持たないので 2 件以上なら分割する。`pool.install` の中ではそのプールの worker で走る。search worker も rayon の worker もプロセス寿命なので、カーソルの寿命もプロセス寿命になる。`CURSOR` の doc「rayon の worker ごとに 1 本」はすでに不正確である。

**発火の条件**: 新しい索引の走査で、葉（rayon の分割単位）の最初に組み立てるエントリが根でないこと。根（`parent == NO_PARENT`）に当たればチェーンは捨てられる。index 0 は必ず親なしなので、1 スレッドの全件走査（`recent_history` や `has_path_sep` のクエリ）は最初の 1 件で正しく戻る。誤るのは、(a) プレフィルタが根を弾いて葉から先に組み立てるクエリ（issue の `one`）と、(b) 複数 worker で分割された 2 番目以降の葉の先頭である。

## 索引の差し替えの契機

`SearchEngine` は `PathStore` を 1 つ持ち、`PrebuiltIndex` のスワップで丸ごと替わる（`/s`・設定変更・初回構築）。`PathStore` 自身は構築後に親子関係を変えない——`&mut self` のメソッドは `mark_file_name_is_lower_name`（フラグのみ）・`shrink_to_fit`（容量のみ）・`force_unsorted_for_test`（`cfg(test)`・`sorted_prefix_len` のみ）で、どれも正規化キーを変えない。したがって「同じ `PathStore` インスタンスの中ではチェーンが有効」は保たれ、世代は構築時に 1 回振れば足りる。

## 再利用できる既存パターン

- `history.rs` の `NEXT_SAVE_SEQUENCE: AtomicU64`（`fetch_add` によるグローバル連番）
- `PathCursor::normalized` の根の分岐が既に「`buf.clear(); stack.clear()`」を持つ。世代の不一致でも同じ捨て方を使える

## 技術的な制約

- `PathCursor::new` は `const fn` で、`thread_local!` の `const { }` 初期化に使われている。世代のフィールドは `const` で初期化できる値（例: `0`）でなければならない。`PathStore` の世代は 1 から振れば、初期カーソル（世代 0）は必ず不一致になる
- `stack` は `(u32, u32)`。世代は stack の要素ではなく `PathCursor` に 1 つだけ持てば足りる
- アドレス（`*const PathStore`）で識別しない: `SearchEngine` の move と、旧索引の解放後に同じアドレスへ新索引が置かれる ABA がある（issue の指摘どおり）
- 費用: キー 1 本あたり `u64` の比較 1 回。パスクエリの全件走査は感度が高い領域（`PERFORMANCE.md` に 1.6〜1.9 倍の退行の実測がある）ので、見込みでは済ませず `path_query_cost` の実運用点で A/B を測る
- `PathStore` は `Clone` を derive していない。clone で同じ世代の 2 インスタンスが生まれる経路は今は無い（足すなら同内容なので同世代でも正しい）
- 回帰テストは `PathCursor` を単体で叩く形（A の後に B）と、1 スレッドの rayon プールで `SearchEngine` を 2 つ作って検索する形の 2 通りが書ける。rayon は snotra-core の依存にある

## 未解決の疑問（計画で決着）

- 回帰テストの形 → 単体（A の**葉だけ**を組み立てた直後に B を組み立てる）とエンジン級（`pool_of(1).install` で worker を固定し、A を `two`→ B を `one` で検索）の 2 本。A 側を全件走査にすると index 0 の根でチェーンが捨てられ、バグがあっても通る（3b の M2・実測）
- `path_store_cursor_matches_full_rebuild` の「カーソルの寿命 = 1 走査」 → 走査ごとに `new` する理由（3 通りの順序を独立に試す）は世代を入れても残る。コメントは「スレッド寿命の持ち越しは別テストが固定する」旨へ直す

## 敵対的調査（3b）の所見と採否

出力: `workspace/adversarial-1442.txt`。

- 壊せなかった: 争点 1（入口は `adopt` だけ・in-place で鍵を変えるメソッド無し）、2（索引依存の状態を持ち越すのは `CURSOR` だけ。⚠️ `MATCHER` は nucleo の契約に依拠・未実行だが、索引を見ないので本件外）、3（製品経路は 2 つ）、4（世代 1 始まり・カーソル 0 で初期不一致。照合は `normalized` の先頭に置き、根の分岐でも世代を更新する——採用）、5（`Clone` 無し。2 エンジン交互はチェーンを毎回捨てて遅くなるだけ）、6（再現した: 1 スレッドで A→B 後 `[abc\one.exe, one.exe]`・fresh では逆。グローバルプールで 30/30）
- M1 は一度採用したが、Step 2b の独立導出が rayon のソースで反証し、自分でも `plumbing/mod.rs` の `helper`（分割しなければ `producer.fold_with` をその場で呼ぶ）を読んで確かめた。**M1 は却下**。当初の本文「候補が少なければ呼び出しスレッド」が正しく、境界は `MIN_PAR_CANDIDATES` 未満ではなく 2 倍未満である。本文はそれに合わせて直した
- M2 採用: 再現は「プレフィルタが根を読まない」ことに依存。回帰テストの組み方へ反映
- M3 採用: `recent_history` は 1 スレッドでは誤らず、複数 worker で非決定的に誤る。本文を直した。回帰テストには入れない（非決定的。単体テストが機序を固定し、`recent_history` は同じ `with_cursor` を通るので同じ修正で直る）
- M4（未実測）: `has_path_sep` も同じ条件で誤る見込み。M3 と同じ扱い
- 性能の見込みが未測定 → 採用。計画に A/B 計測を入れた
- `#[ignore]` 相当の記述は不正確 → 採用・直した
