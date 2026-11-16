# 計画 — issue #1442 索引の差し替え後に PathCursor が古い索引のチェーンを流用する

分類: バグ（`PathCursor::normalized` の doc の契約「`normalize_entry_key` と 1 バイトも違わない」に挙動を合わせる）。`SPEC.md` は変えない。調査は `workspace/research.md`。

## 目的と受け入れ条件

- 索引（`PathStore`）を差し替えた後も、同じスレッドのカーソルが返す正規化キーが `normalize_entry_key` と一致する
- 受け入れ:
  1. 単体の回帰テスト: 1 本の `PathCursor` で次の 2 系列を組み立て、各出力が `normalize_entry_key` と一致する（系列 1 は修正前に赤）
     - 系列 1: A の**葉だけ** → B の葉 → B の全件
     - 系列 2（交互）: A の葉 → B の根（index 0）→ A の index 1（親 0）。世代照合を根の分岐の後ろへ置く誤りを検知する（その誤りではカーソルの世代が A のまま B の根がチェーンに載り、`c:\bbb\zzz` になる）
  2. エンジン級の回帰テスト: `pool_of(1).install` の中で、索引 A を `two` で検索した後に索引 B（`C:\bbb\one.exe` に起動履歴 3 回）を `one` で検索し、1 位が `C:\bbb\one.exe` になる（修正前は赤）
  3. 既存の `path_store_cursor_matches_*` が通る
  4. パスクエリの全件走査に測れる退行が無い（下の A/B 計測。合否: 交互に取った p50 の差が、同じ側の繰り返しのばらつきの範囲に収まる。超えたら実装を止めて報告する）

## 設計

- `PathStore` にフィールド `generation: u64` を足す。`adopt` の中で、モジュール内の `static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1)` から `fetch_add(1, Ordering::Relaxed)` で振る（一意性だけが要り、他のメモリとの順序は要らない）。`path_store.rs` は `std::cmp::Ordering` を import 済みなので、atomic 側は `std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering}` と別名で import する
- `PathCursor` にフィールド `generation: u64` を足す。`new()` は `0`（`const fn` のまま）。`PathStore` の世代は 1 から振るので、初期カーソルは必ず不一致になる
- `PathCursor::normalized` の**先頭**（根の分岐より前）で `self.generation != store.generation` なら `buf.clear()` / `stack.clear()` を行い `self.generation = store.generation` とする。stack が空ならその後の `rposition` は必ず外れ、既存の「外れ」経路で根から作り直す。根の分岐も同じ先頭の処理を通るので、根の分岐で世代を更新し忘れる形にならない
- アドレスで識別しない（move と ABA。理由を `generation` の doc に一言書く）
- `PathStore::footprint_rows` の分解に `generation: _` を足す（`..` を書かない方針のため、足さないとコンパイルが通らない）
- 世代はヒープを持たないので footprint の行は増えない

## 変更するファイルと対象シンボル

1. `snotra-core/src/search/path_store.rs`
   - `PathStore`（`generation` フィールドと doc）・`PathStore::adopt`・`PathStore::footprint_rows`
   - `PathCursor`（`generation` フィールドと doc）・`PathCursor::new`・`PathCursor::normalized`（先頭の世代照合と doc）
   - `static NEXT_GENERATION`（新設）
   - `thread_local! CURSOR` の doc（「rayon の worker ごとに 1 本」はすでに不正確——分割しない検索は呼び出しスレッド〈製品では search worker〉で走る。「走査するスレッドごとに 1 本・寿命はスレッド寿命で索引をまたぐ・またいでも正しいのは世代による」へ直す）
   - `PathCursor` の struct doc（「全件走査のあいだ…」の寿命の記述と「順序が乱れた入力でも結果は変わらない」の適用範囲に、索引をまたぐ場合を足す）、`stack` の doc（載っている index はすべて `generation` の索引のもの）、`normalized` の doc（`with_cursor` ではなくここで照合する理由——単体テストが直接呼ぶため）
   - `PathStore::generation` の doc に「構築後に親子関係・名前・table を書き換える口を足すなら、世代も進める」を書く（I4: 機械的な検知が無い前提）
2. `snotra-core/src/search/tests/path.rs`
   - 新テスト `path_store_cursor_discards_chain_across_stores`（受け入れ 1）
   - 新テスト `history_boost_survives_index_swap_on_same_worker`（受け入れ 2）
   - `path_store_cursor_matches_full_rebuild` の「カーソルの寿命 = 1 走査」のコメントを、「走査ごとに `new` するのは 3 通りの順序を独立に試すため。索引をまたぐ持ち越しは `path_store_cursor_discards_chain_across_stores` が固定する」へ直す

触らないもの: `CMP_BUFS` / `MATCHER` / `PATH_MATCH_BUF`（毎回 `clear` から書き、索引に依存する状態を持たない・3b で確認）、`search.rs` / `scoring.rs`（呼び出し側の形は変わらない）、`index_tree.rs`、永続形式。

## 実装の順序

1. Red: 2 本のテストを書き、修正前に赤であることを確かめる
2. Green: `path_store.rs` を直す
3. 既存テスト・検証チェックリスト
4. A/B 計測

## 不変条件と異常系

- 同じ `PathStore` インスタンスの中ではチェーンが有効——構築後に親子関係・名前・table を変えるメソッドが無い（`&mut self` は `mark_file_name_is_lower_name` / `shrink_to_fit` / `force_unsorted_for_test` だけ）。検知: 既存の `path_store_cursor_matches_full_rebuild`（1 本のカーソルで順・逆順・乱順）
- 世代は `PathStore` ごとに一意——`fetch_add` は `u64` で、溢れるまでに 2^64 回の構築が要る（溢れは考えない）
- 1 スレッドで 2 つの `SearchEngine` を交互に走査すると、チェーンを毎回捨てる。正しさは保たれ、速さだけが落ちる。製品にその経路は無い（`SearchEngine` は 1 つを差し替える）。正しさの検知: 単体テストの系列 2
- 境界条件と検証:
  - 初期カーソル（世代 0）→ 最初の呼び出しで不一致・作り直し: 既存の全テストが新しいスレッドのカーソルで最初に呼ぶ
  - 新索引の最初の呼び出しが根: 系列 2 の 2 手目（A の葉の直後に B の根）
  - 新索引の最初の呼び出しが葉（親 index が旧チェーンに偶然載る）: 系列 1 の 2 手目
  - 根を挟んで旧索引へ戻る: 系列 2 の 3 手目
  - 同じ索引の連続呼び出し: 既存の `path_store_cursor_matches_full_rebuild`

## テスト方針と検証コマンド

- 単体テスト（受け入れ 1）: fixture は issue の索引 A（`C:\aaa` / `C:\aaa\zzz` / `C:\aaa\zzz\two.exe`）と B（`C:\bbb` / `C:\bbb\abc` / `C:\bbb\abc\one.exe` / `C:\bbb\one.exe`）。系列ごとに新しい `PathCursor` を使う。**A 側を全件走査にしない**（index 0 の根でチェーンが捨てられ、修正前でも通る・3b の M2 実測）
- エンジン級（受け入れ 2）: `pool_of(1).install(|| { engine_a.search("two", ..); engine_b.search("one", .., &history, ..) })` の 1 位が `C:\bbb\one.exe` であることを直接 assert する（期待値は固定値で書く）。`history` は `HistoryStore::empty()` に `record_launch("C:\\bbb\\one.exe", "one")` を 3 回。落とし穴（Step 2b の導出）: (1) A のクエリはどこかに当たらねばならない（当たらなければ `with_normalized_key` が呼ばれずチェーンが空のまま）——`two` は A の葉に当たる。(2) B のクエリは根を読まない名前一致にする（パスクエリや `recent_history` を先に呼ぶと index 0 でリセットされる）。(3) fixture は 1024 件未満に保つ（分割が起きると走査の順とスレッドが変わる）。(4) A と B は別の文字列の索引にする（同じ entries から 2 回 build すると誤ったチェーンでも出力が一致する）。(5) A と B は同じ `install` の中で走らせる（`pool_of(1)` を 2 回呼ぶと別スレッドになる）
- 変異注入（`/implement`「3b. 委譲へ渡すもの」の委譲先が行う）: (a) 世代照合を消す → 系列 1 とエンジン級が赤、(b) 世代照合を根の分岐の後ろへ移す → 系列 2 が赤、(c) 世代の更新を消す → 毎回作り直しになるだけで緑（性能でしか見えない）であることを確認し、検知対象外と記録する
- 検証: `docs/build-commands.md`「変更後の検証チェックリスト」のうち当てはまるカテゴリ（Rust の build/test/clippy/fmt、doc を変えるので `cargo doc`）
- A/B 計測: `docs/build-commands.md` の `path_query_cost` の `at_operating_point` を、修正前（main）と修正後で交互に取る（実 `index.bin` がある開発機）。結果は PR 本文に載せる（差が雑音の範囲なら `PERFORMANCE.md` には書かない）

## 文書の更新

- `SPEC.md`: 不要（挙動を契約へ戻すだけ）
- `docs/architecture.md:281`: `PathCursor` がチェーンを持ち回る記述。索引をまたがないことは書いていない——世代の記述は `path_store.rs` の doc に置き、architecture は変えない（正本を 1 か所に保つ）
- `PERFORMANCE.md`: A/B に差が出たときだけ記録する
- `snotra-core/CLAUDE.md`: モジュール構成は変わらない。変更不要

## 作業項目

### Phase 1 — Red
- [x] `search/tests/path.rs` に `path_store_cursor_discards_chain_across_stores` を足し、修正前に赤を確認
- [x] `search/tests/path.rs` に `history_boost_survives_index_swap_on_same_worker` を足し、修正前に赤を確認

### Phase 2 — Green
- [x] `path_store.rs` に `NEXT_GENERATION` と `PathStore::generation` を足し、`adopt` で振る
- [x] `PathCursor::generation` を足し、`normalized` の先頭で照合・破棄・更新する
- [x] `footprint_rows` の分解に `generation: _` を足す
- [x] `PathCursor` / `CURSOR` / `generation` の doc を直す
- [x] `path_store_cursor_matches_full_rebuild` のコメントを直す

- [x] （計画外・dry-check）`cursor_fixture` を `entries_from_paths`（`swap_fixture` を改名）経由に置き換える（同じ名前導出と整列を手で書いていた。`ranking.rs` の同形は別モジュールゆえ範囲外）

### Phase 3 — 検証
- [ ] 変更後の検証チェックリスト（該当カテゴリ）を通す
  - ラウンド 1: sha 599a91b8 / verify:tree green（A・E・F）/ 指摘 Critical〜Medium 0・Low 6（L1 手目の 0 始まり・L2 跨ぐ/またぐの混在・L3 構造の写し・L4「1 から」の太字が何も守らない・L6 回帰テストのラベルを採用。L5 CLAUDE.md の「2 本」は glob の主張として真なので不採用）/ 変異 (a)(b)(d) 検知・(c) は計画どおり検知対象外 / 生き残り 0
- [x] `path_query_cost` の `at_operating_point` で A/B を取り、結果を記録する — 2026-11-16・開発機の実 index.bin（23,672 件・PATH マージで +248）・main と本ブランチを交互に 3 回ずつ（release・100 標本）。`c:` の p50 中央値（µs）: 4 構成で A 480/461/470/451・B 486/457/457/472。他のパスクエリも同様に差は両方向に散り、同じ側の 3 回の幅（例 `c:` A 451〜507）に収まる。退行なし。PERFORMANCE.md には書かない（計画どおり）

## 未確定（実装前に潰す）

（なし）

## 人間レビュー

- [x] 承認済み — 2026-11-16 / 問い: "計画を承認なさいますか。それとも `workspace/plan.md` に注釈を書き込まれますか。" / 回答: "承認します"

## plan-review 結果

- リスク: 高（`static AtomicU64` の新設と、スレッドをまたいで寿命を持つスレッドローカル状態の変更が「共有状態を変更する」に当たる）
- レビュー方式: 独立導出 1 体（Step 2b・成果物 `workspace/plan-review-derive-1442.md`）
- エージェント数: 1

### 要対処
- rayon の走査スレッドの記述 — 計画と research.md を修正 — 3b の M1（「常に worker」）を Step 2b が rayon 1.12.0 のソースで反証。自分で `plumbing/mod.rs` の `helper` を読み、分割しなければ呼び出しスレッドで fold することを確認した。エンジン級テストの設計（同じ `install` の中で A→B）は変わらない
- エンジン級テストの落とし穴 5 点 — 計画のテスト方針へ追記 — 導出 ∖ plan
- `CURSOR` / `PathCursor` / `stack` / `normalized` / `generation` の doc の具体的な直し先 — 変更ファイル一覧へ追記 — 導出 ∖ plan（`CURSOR` の「worker ごとに 1 本」が現状すでに不正確なことを含む）

### 軽微
- `snotra-core/CLAUDE.md` の「`path_store_cursor_matches_*` の 2 本が守る」: 新テスト名 `path_store_cursor_discards_chain_across_stores` は glob に当たらないので偽にならない。変更しない
- 世代の一意性を固定する 1 行テスト（導出の I3）: 系列 1 が「世代を定数にする」変異でも赤になるので足さない
- `buf.clear()` の要否: 導出は不要とする（根と外れの分岐が clear する）。計画は不一致時に `stack` と併せて clear する——害が無く、`stack` と `buf` の対応を 1 か所で保てるので計画のままにする
- 範囲外: `search/scoring.rs` の `EntryView` の doc が存在しない `KEY_BUF` を名指している（実体は `CURSOR`）。本件の変更が偽にしたものではないので触らない。直すなら別 issue
- 既存テストのフレーキーの懸念（導出の ⚠️）: グローバルプールを共有するテスト間でチェーンを持ち越しうるのは本件のバグそのもので、修正で消える

### 未検証
- 性能の増分（A/B は Phase 3 で測る）
- ABA でアドレスが再利用されるケースは決定的なテストにできない（アドレス識別を採らないので本件の検知対象ではない）

### 判断
- 実装着手: 可（人間の承認待ち）

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー1体（Step 2b）
- エージェント数: 1（3b の敵対的調査を含めると 2）
- 要対処: 3 件（上の plan-review 結果のとおり反映済み）
- 未検証: 性能の増分（Phase 3 で測る）
- 散文の全件列挙: 語 4 / 総件数 144 行 / ★未掲載 22（計画へ足した 0：別義の「チェーン」〈hook のコマンド連鎖・キャッシュのフォールバック・派生文字列の共有〉か、変更後も真のまま〈`index_tree.rs` の `PathCursor` への言及〉） / ☆行未掲載 0（計画へ足した 0）
- 試行（#1300・高リスクのみ・独立レビュー 1 回ごとに 1 行）: 方式 Step 2b / 設計変更 0 / 主の見落とし 3
