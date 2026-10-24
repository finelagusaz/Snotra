# 計画: issue #1276 — フォルダ絞り込みの Fuzzy を検索と同じ照合の規則へ揃える

調査は `workspace/research.md`、敵対的調査は `workspace/adversarial-1276.txt`。

## 目的

フォルダ絞り込み（`folder.rs` の `matches_filter` の Fuzzy の腕）を、検索と**同じ関数**で照合器へ渡す。判定は**バグ**である（`SPEC.md` §4.1・§4.2「全検索モード・フォルダ展開共通」・§6.3 は検索とフォルダで照合の規則を分けていない。SPEC の文言は変えない）。あわせて、フォルダの入口が呼び出しごとに払っている `Matcher::new`（133,120 B）と、候補ごとの `Vec::new()` ×2 を消す。

## 受け入れ条件

1. フォルダの Fuzzy の照合は、検索の Fuzzy の腕と**同じ 1 つの関数**（下の `fuzzy_match_score`）を通る。haystack は `utf32_haystack`、needle は `Utf32String::from`（呼び出し 1 回に 1 度）で組む。`folder.rs` に `Utf32Str::new` / `Matcher` / `MatcherConfig` は残らない
2. 非 ASCII の名前 × 結合文字を含む needle の組で、フォルダ絞り込みの真偽が検索と同じ規則になる: `filter_sorted` で `日本語 x.txt` × `x\u{308}` が一致し、`Cafe\u{301}.txt` × `e\u{308}` が一致する（いずれも旧実装では不一致・`research.md` の probe 実測）
3. 純 ASCII の名前 × 結合文字 needle は修正後も一致しない（`resume` × `e\u{301}`）——nucleo 0.3.1 の `(Ascii, Unicode)` 腕は `None` を返す。この境界をテストで固定する
4. 既存のフォルダ絞り込みテスト（Prefix / Substring / Fuzzy・`filter_sorted`）と検索のテストがすべて緑のまま
5. 検索側の挙動は変わらない（Fuzzy の腕は同じ処理を関数へ括り出すだけ）

## 変更ファイルと対象シンボル

- `snotra-core/src/search/scoring.rs`
  - 新設 `pub(crate) fn fuzzy_match_score(haystack: &str, needle: Utf32Str<'_>) -> Option<u16>`: `MATCHER.with` → `utf32_haystack(haystack, &mut scratch.haystack)` → `scratch.matcher.fuzzy_match(haystack, needle)`。`///` に「検索の Fuzzy の腕とフォルダ絞り込みの Fuzzy の腕が共有する。`fuzzy_match` を直接呼ぶ箇所を他に足すと規則が 2 か所になる（検知器は無い）」「`MATCHER` の借用はこの関数の中に閉じる（再入しない）」を書く——「唯一の入口」とは書かない（`Matcher` は外部 crate の pub 型で、他所からの直呼びを止める機構が無い）
  - `match_score_single_cached` の Fuzzy の腕を `fuzzy_match_score(lower_name, needle_u32.slice(..)).map(|s| s as i64)` へ置き換える。同関数の doc の「スレッドローカル（`MATCHER`）の借用は Fuzzy の腕の中に閉じてある」を、借用が `fuzzy_match_score` に閉じる形へ書き換える
  - `FuzzyScratch` の `///`「rayon の worker ごとに 1 組の道具」と `MATCHER` 直上のコメント「D-5: one Matcher per rayon worker thread」を、フォルダ絞り込みを呼ぶスレッド（UI スレッド）にも 1 組置かれる、と事実へ合わせて書き換える
- `snotra-core/src/search/path_store.rs`
  - `CURSOR` の `///`「`MATCHER` と同じ形である」を外す（`CURSOR` が worker ごとに 1 本であることは真のまま。thread_local + `RefCell` という形としては今後も同じだが、直前の「rayon の worker ごとに 1 本」と並ぶので、比喩が `MATCHER` も worker ごとだと含意してしまう）
- `snotra-core/tests/path_query_cost.rs`
  - 164 行のコメント「`search/scoring.rs` の thread-local `MATCHER` と同じ形」を外す（同上）
- `snotra-core/src/search.rs`
  - `pub(crate) use scoring::fuzzy_match_score;`（既存の `use scoring::TopK;` の隣）
- `snotra-core/src/folder.rs`
  - `use` から `Config as MatcherConfig, Matcher, Utf32Str` を外し、`nucleo_matcher::Utf32String` と `crate::search::fuzzy_match_score` を足す
  - `matches_filter(name, filter_lower, needle: &Utf32String, mode)`: `&mut Matcher` 引数を外し、Fuzzy の腕は `fuzzy_match_score(&name_lower, needle.slice(..)).is_some()`
  - `read_dir_entries` / `filter_sorted`: `Matcher::new` を消し、`let needle = Utf32String::from(filter_lower.as_str());` を `to_lower_folded(filter)` の直後に 1 度だけ作る
  - テスト内 `read_dir_entries_for_test`: 同じ形へ追随
  - テスト追加（下の「テスト方針」）

`SPEC.md`・`snotra-core/CLAUDE.md`・`PERFORMANCE.md` は変えない（SPEC は既にこの挙動を述べている。`snotra-core/CLAUDE.md` の folder.rs 行は責務を `//!` へ委ねており、責務は変わらない。額は測らないので `PERFORMANCE.md` へ書く値が無い）。

## 実装順序

1. Red: `folder.rs` のテストに受け入れ条件 2・3 のテストを足し、2 が現行実装で落ちること・3 が通ることを確かめる
2. `scoring.rs` に `fuzzy_match_score` を足し、検索の Fuzzy の腕をそれへ置き換える（検索のテストが緑のまま）
3. `search.rs` で `pub(crate) use`、`folder.rs` を置き換える（Green）
4. コメントの書き換え（`MATCHER` / `match_score_single_cached` の doc）
5. 検証コマンド

## 不変条件と異常系

- **`MATCHER` の `borrow_mut` は再入で panic する**。`fuzzy_match_score` の中から照合以外を呼ばないこと（今の 2 つの呼び出し点はどちらも入れ子にならない: `filter_sorted` / `read_dir_entries` の中で検索は走らず、検索の `score_one_entry` の中でフォルダの絞り込みは走らない）
- 規則の一致（`utf32_haystack` ≡ `Utf32String::from(..).slice(..)`）は既存の `utf32_haystack_matches_utf32_string_from` が守る。フォルダ側が同じ関数を通ることは型（`folder.rs` が `Matcher` を持たない）と受け入れ条件 2 のテストが守る
- 状態・リソースの新設は無い。スレッドローカルの常駐が UI スレッドに 1 組増える（`Matcher` 133,120 B + 展開先の `Vec<char>`。初回のフォルダ Fuzzy 絞り込みで作られ、スレッドの終了まで残る）——索引の常駐 54.92 MiB に比べ 0.23% 程度で、代わりに打鍵ごとの 133,120 B の `alloc_zeroed` が消える。破棄経路はスレッドの終了（既存の `MATCHER` と同じ）
- filter が空の呼び出し（`folder_nav.rs:76` の `read_dir_entries(.., "")`）では needle の `Utf32String::from("")` が作られるが、`matches_filter` は呼ばれない。0.3.1 の `From<&str>` は ASCII なら `value.to_owned().into_boxed_str()`（`src/utf32_str.rs` 307 行）であり、空文字列では確保しない。filter が非空の Prefix / Substring では needle が使われないまま呼び出し 1 回に 1 度（filter の長さぶん）確保される——置き換える `Matcher::new` の 133,120 B に比べて小さいので、モードで分岐させずに受け入れる

## テスト方針と検証コマンド

- 追加テスト（`folder.rs` の `mod tests`・`filter_sorted` 経由・`sr(name)` fixture を使う）
  - `fuzzy_filter_drops_combining_marks_in_needle_like_search`: `[sr("日本語 x.txt"), sr("Cafe\u{301}.txt")]` に対し filter `x\u{308}` で `日本語 x.txt` の 1 件、filter `e\u{308}` で `Cafe\u{301}.txt` の 1 件。doc に「`Utf32Str::new` へ戻すと needle が `Ascii` の生バイトのまま照合され、ここが不一致になる」を書く。**2 行を 1 本にまとめる**——`日本語` の行は needle だけ、`Cafe` の行は haystack と needle の両方に依る。どの行が検知したかは assert のメッセージ（名前と filter を含める）で区別でき、変異注入の委譲先には変異ごとに落ちた assert を報告させる
  - `fuzzy_filter_ascii_name_never_matches_non_ascii_needle`: `[sr("resume")]` × filter `e\u{301}` が 0 件（nucleo 0.3.1 の `(Ascii, Unicode)` 腕）。版を上げて赤になったら上流の側が変わったかを見る、と doc に書く
- 変異注入（`/implement`「3b. 委譲へ渡すもの」の委譲先が行う）: (a) `folder.rs` の needle を `Utf32Str::new` へ戻す → 1 本目が落ちることを期待 / (b) `fuzzy_match_score` の haystack を `Utf32Str::new` へ戻す → `utf32_haystack_matches_utf32_string_from` は関数を直接見るので落ちない。どの検知器が落ちるかを測って報告させる（1 本目の `Cafe\u{301}.txt` の行は haystack が `Ascii` の生バイトになると `(Ascii, Unicode)` で `None` になり落ちる見込み——推論であって実測ではない）
- 追加テスト 3 本目 `fuzzy_filter_keeps_old_byte_level_matches`: `[sr("Re\u{301}sume\u{301}.docx")]` × filter `e\u{301}` が 1 件（旧・両辺修正とも一致し、haystack だけの修正で落ちる組。片側だけ替える退行を捕まえる）
- 検証: `docs/build-commands.md`「変更後の検証チェックリスト」カテゴリ A（fmt・check・clippy・`cargo test -p snotra-core`・doc コメントを触るので `cargo doc` 行）

## フェーズと作業項目

### Phase 1 — Red

- [x] `folder.rs` に受け入れ条件 2・3 のテストと 3 本目（`fuzzy_filter_keeps_old_byte_level_matches`）を足し、1 本目が現行で落ちる・2・3 本目が通ることを確かめる（実測: 旧実装へ 3 本を差し込んで 1 本目だけ落ちた。fixture は `.txt` の `x` が needle `x\u{308}` に一致したため `日本語 x.md` / `Cafe\u{301}.md` へ変えた）

### Phase 2 — 共有関数と置き換え

- [x] `scoring.rs` に `fuzzy_match_score` を足し、`match_score_single_cached` の Fuzzy の腕を置き換える
- [x] `search.rs` で `pub(crate) use scoring::fuzzy_match_score;`
- [x] `folder.rs` の `matches_filter` / `read_dir_entries` / `filter_sorted` / `read_dir_entries_for_test` を置き換え、`Matcher` / `Utf32Str` の import を外す
- [x] `FuzzyScratch` / `MATCHER` / `match_score_single_cached` の doc・コメントと、`CURSOR`・`tests/path_query_cost.rs` の「`MATCHER` と同じ形」を事実へ合わせる

### Phase 3 — 検証

- [x] カテゴリ A のコマンドがすべて緑（委譲先が `7017480` で実測・全 exit 0。修正巡の再実行は下）
- [x] 委譲レビューの ⚠️ L1（テスト 1 が最初に落ちた行しか報告しない）を、2 行の結果を集めて 1 度だけ比べる形へ直す
- [x] 委譲レビューの ⚠️ L2（`MATCHER` のコメントが呼び出し元のスレッドを列挙し、代理条件「UI thread」を含む）を、呼ぶスレッドごとに 1 つ、の形へ直す

## 未確定（実装前に潰す）

（なし）

## 範囲外（記録のみ・別 issue の候補）

- SPEC §4.2 の「前後空白除去・連続空白の 1 文字化」を `folder.rs` の絞り込みが行っていない（`to_lower_folded` だけ）。敵対的調査が指摘。照合器への入力の規則とは別の既存の食い違い
- NFC の needle × NFD の名前（`が` × `か\u{3099}`）は修正の前後とも一致しない。NFC/NFD の正規化の問題で、検索側も同じ

## 人間レビュー

- [x] 承認済み — 2026-10-24 / 問い: "`workspace/plan.md` へ注釈を書き足していただくか、明示的に承認していただけますでしょうか。" / 回答: "OK"

## セルフレビュー

- リスク: 通常（永続形式・識別子・並行境界・UI モードを触らない。スレッドローカルはスレッドをまたいで共有されない）
- plan-review: 未実施（通常リスク）
- エージェント数: 1（3b の敵対的調査のみ）
- 照合（`/start-issue` 5a の 5 点）
  1. issue の要件 → 規則の共有（受け入れ条件 1）・needle/haystack の両辺（条件 2・3 本目のテスト）・`Matcher::new` の撤去（条件 1）・候補ごとの `Vec` の撤去（`fuzzy_match_score` の展開先を使い回す）。`to_lower_folded` の `String` の使い回しは issue が「直すなら」とした任意項目で、新 API が要り額も未測定ゆえ落とした
  2. 境界条件: 純 ASCII 名 × 結合文字 needle（2 本目）・非 ASCII 名 × 結合文字 needle（1 本目）・旧で一致していた生バイトの組（3 本目）・filter 空（既存の `filter_sorted_empty_returns_prefix_up_to_max`）・Prefix / Substring は needle を使わない（既存テスト）
  3. 新しい状態・リソース: UI スレッドのスレッドローカル 1 組。生成は初回の Fuzzy 絞り込み、破棄はスレッドの終了（既存の `MATCHER` と同じ）
  4. より単純な既存パターン: 検索の Fuzzy の腕そのものを関数へ括り出して共有する——新しい仕組みは足さない
  5. 壊してはならない不変条件の検知: 規則の一致は既存の `utf32_haystack_matches_utf32_string_from`、フォルダが同じ関数を通ることは追加テスト 3 本と「`folder.rs` が `Matcher` を持たない」こと
- 条件別チェック: 「関数・型を新規定義」に当たる。`fuzzy_match` / `Matcher::new` / `Utf32Str::new` の呼び出しの全数は grep で列挙済み（製品 `.rs` 全 crate・`folder.rs` の 4 行と `scoring.rs` の 2 行だけ）。変更後は `scoring.rs` だけに残る。呼び出し元の LSP 列挙は実装時に `fuzzy_match_score` へ当てる
- 要対処: 敵対的調査から 3 件（両辺の同時置き換え・書き換えるコメント 4 か所・常駐比の分母）。いずれも反映済み
- 未検証: 変異注入 (b) でどの検知器が落ちるか（推論のみ。委譲先が測る）
