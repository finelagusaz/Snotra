# 調査: #1394 パスマッチの照合で、NFC のクエリが NFD で保存されたパスに一致しない

## issue の要約

- パスマッチ（クエリに `\` `/` `¥` を含む・`SPEC.md` §4.1）では、パス側もクエリ側も Unicode の合成（NFC）を通らない。NFC で打ったクエリ（IME の既定）が、NFD で保存されたパス成分（`か` + U+3099 など）に一致しない
- #1285 は名前の照合（`query::normalize_for_match`）にだけ NFC を足し、パスマッチを射程外にした。`SPEC.md` §4.2 は「パスマッチング（§4.1）は、パス側もクエリ側もこの正規化を通らない。そのため NFC と NFD の違いは揃わない」と明記している（126 行目）
- issue の「未確認」: (1) 照合だけを NFC 化し、永続キー（`normalize_entry_key`）は変えない分け方が取れるか (2) 実データの NFD パス成分の件数

## 関連するファイル・シンボル

### パス側（照合のたびに導出する正規化キー）

- `snotra-core/src/indexer/keys.rs` の `normalize_entry_key` / `normalize_entry_key_into` — 規則の正本（trim・小文字化・`/` → `\`）。**識別用**: 履歴のパスキー（`history.rs`）・走査の重複排除（`indexer/scan.rs`）・PATH マージ（`indexer/path_env.rs`）・`index_tree.rs` の `file_key_into`
- `snotra-core/src/search/path_store.rs` の `PathCursor::normalized` / `push_segment` / `with_cursor` — 検索時に同じ規則でキーを組み立てる写し（性能のための再実装。一致は `search/tests/path.rs` の `path_store_cursor_matches_full_rebuild` / `path_store_cursor_matches_normalize_entry_key_over_real_index`）
- `snotra-core/src/search/scoring.rs` の `with_normalized_key`（`with_cursor` の唯一の窓口）と `score_one_entry` 内のブロック（507〜533 行目）。**同じ `key` を 2 用途で使う**: パスマッチ（`plan.path_finder.find(key)`）と履歴照合（`get_global_stats_normalized(key)` / `query_count_pre_normalized(.., key)` / `folder_expansion_count_normalized(key)`）

### クエリ側

- `snotra-core/src/search/query_plan.rs` の `prepare_query_plan` 内の `path_query`（122〜136 行目）— 生クエリを trim → `/` `¥` を `\` → `char::to_lowercase`。`path_finder`（`memchr::memmem::Finder`）を 1 回だけ作る
- `path_history_key` は `normalize_history_query_key`（= `normalize_for_match` + 区切り統一）を通るので、**すでに NFC 化されている**。パスマッチの照合とは別物

### NFC の実装（再利用できる部品）

- `snotra-core/src/query.rs` の `const NFC: ComposingNormalizerBorrowed<'static>`（private）。`icu_normalizer` 2.4.0（`Cargo.lock`）
- icu_normalizer 2.4.0 の `ComposingNormalizerBorrowed` には `normalize`（`Cow`）・`normalize_to<W: fmt::Write>`（確保済みバッファへ書ける）・`is_normalized`・`split_normalized` がある（`~/.cargo/registry/src/*/icu_normalizer-2.4.0/src/lib.rs` で確認）

## 分け方の結論（issue の未確認 (1)）

取れる。**照合用の NFC は `score_one_entry` のパスマッチの直前と `path_query` の組み立てにだけ当て、`with_normalized_key` が貸す `key`（= 識別用キー）は変えない。**

- 履歴照合は `key`（識別用）をそのまま使い続ける。履歴に記録されるパスは起動したエントリの `target_path` そのものなので、記録側と照合側は同じバイト列から同じ `normalize_entry_key` を通り、NFC の有無に関係なく一致する。ゆえに `history.bin` の移行・`migrate_normalize_keys`・「history.rs のキー正規化に関するチェックリスト」の 3 者は**触らない**
- `normalize_entry_key` の規則・`path_store.rs` の写し・`tests/path_query_cost.rs` の写しは変わらない（バイト一致の検知器 3 本はそのまま緑のはず）
- `index.bin` の形式も変わらない（正規化キーは索引に保持されず、検索時に導出する）

## 再利用できる既存のパターン

- `query::normalize_for_match` の「ASCII は合成を飛ばす」高速路（`trimmed.is_ascii()`）
- `path_store.rs` / `scoring.rs` の「スレッドローカルのバッファを使い回し、ウォームアップ後に確保しない」形（`CURSOR`・`CMP_BUFS`）。`with_cursor` の `f` の中から `CURSOR` を再び借りると panic するので、合成先のバッファは**別の thread_local**にする
- 「記録側と照合側は同じ関数を通す」（`snotra-core/CLAUDE.md`「`normalize_entry_key` の冪等性契約」）: パスとクエリの両辺に同じ合成関数を当てる

## 技術的な制約

- **性能**: パスクエリは bitmask pre-filter を飛ばし、毎打鍵で全エントリを走査する唯一の経路（`query_plan.rs` の `//!`）。合成は非 ASCII のキーだけに掛かる（実運用点 312,377 パスのうち非 ASCII 混じりは 1.7%・`keys.rs` の doc）。計測コードは `snotra-core/tests/path_query_cost.rs`（`#[ignore]`・開発機のみ）
- **確保**: `tests/search_allocations.rs` の `path_query_full_scan_does_not_allocate_per_entry` はエントリ数に比例する確保を禁じる。`NFC.normalize` は分解済みの入力で `Cow::Owned` を返すので、ホットパスでは `normalize_to` で thread_local のバッファへ書く。なお同テストの fixture は ASCII のみで、合成の経路を通らない
- **合成順**: 両辺とも「小文字化 → NFC」の順で当てる（パス側は `with_normalized_key` の出力が小文字化済み）。canonically equivalent な 2 つの入力でこの順の結果が一致することは、`char::to_lowercase` が正準等価を保つことに依る（全コードポイントで検知器を置ける）
- **意図した挙動の変化**: いままで NFD のパス `…\か゛…`（`か` + U+3099）に、`か` で終わる部分クエリが部分一致していた。合成後は `が` になるので一致しなくなる。名前の照合（#1285）と同じ扱いである
- `incremental` はパスクエリで使わない（`IncrementalCache::caches_candidates`）ので、単調性の議論には触れない
- パスマッチの `pos`（スコアの減点）は合成後のバイト位置になる。NFD の成分を含むパスで数バイト前へずれるだけ

## 未解決の疑問（→ 敵対的調査で決着。下の節）

- 実データの NFD を含むパスの件数 → F4
- 合成の判定の費用 → F2 のため実 `index.bin` では測れない。計画の未確定欄で扱う
- UI が `pos` を使うか → 使わない（壊せなかった項目）

## 敵対的調査（3b）の所見と採否

出力: `workspace/adversarial-1394.txt`（sonnet 1 体）。

### 壊せた項目

- **F1（採る・射程外として記録）** 「パスキーのほかの消費者に同じ NFC 問題は無い」は偽。`snotra-core/src/opener.rs` の `find_matching_tools` が `to_lowercase()` したルールのパス条件と対象パスを `starts_with` で比べ、NFC を通らない（自分で 60〜82 行目を読んで確かめた）。ただしパスマッチ（検索）でも履歴・索引キーでもなく、#1394 の射程外。別 issue の候補としてユーザーへ報告する
- **F2（採る・最重要）** 開発機の実 `index.bin` は version 7（先頭 8 バイト `INDX` + `07 00 00 00` を `od` で自分で確かめた・2026-11-04 21:19 更新）。現行の `INDEX_CACHE_VERSION` は 10（`indexer/cache.rs:47`）で、v7 は cache-miss 扱い。ゆえに実インデックスの corpus 検知器（`derives_same_bytes_as_normalize_entry_key`・`search/tests/path.rs` の実索引テスト）はこの機で**黙ってスキップ**し、`path_query_cost.rs` の運用点の計測も走らない。本メモの「312,377 パス・非 ASCII 1.7%」はこの機の現状から再現できない（`keys.rs` の doc に書かれた過去の測定値）。更新時刻が昨日なので、v7 を書く旧版の Snotra が常駐していると読める。→ 性能の測定を合成の索引へ移す（計画の未確定欄）
- **F3（採る）** `path_query_full_scan_does_not_allocate_per_entry` の fixture は全件 ASCII で、合成の分岐を通らない。→ 計画の AC6 に、非 ASCII・NFD の fixture を持つ兄弟テストと、「合成が毎回確保する」変異で落ちることの確認を置く（計画済み）
- **F4（採る・⚠️）** stale な v7 をリポジトリ外のコピーで展開した 16,841 エントリで、非 ASCII を含むパス 441 件（2.62%）、NFD のパス成分 7 件（すべて `C:\Toolbox\LimeChat\log\#…`・例 `#うかへ゛ん`）。NFD のパスは実在する（issue の未確認 (2) に答える）。運用点の規模での件数は未測定
- **F5（採る・計画済み）** 打鍵の途中で一致が消える挙動変化（`\#へ` が NFD の `#へ゜…` に一致しなくなる）。AC3 として既にピン止めし、SPEC §4.2 を直す作業項目にある。全件走査なので「打鍵途中で一瞬消える」見え方になるのは名前の照合（#1285）と同じ

### 壊せなかった項目

- 正規化キーは `index.bin` に保持されない（`IndexCache` の全フィールドを確認・形式変更は不要）
- f = NFC ∘ 小文字化 は正準等価を保つ: 全スカラー値 × 結合文字 15 種 0〜1 個（17,793,024 件）と、cased または分解を持つ 14,361 字 × 結合文字 2 個の全順列（3,231,225 件）で、f(NFD(s)) = f(NFC(s)) = f(f(s)) = f(s)。反例 0。結合文字 3 個以上は未検査
- `pos` は `SearchResult` に載らず、UI は使わない
- `with_normalized_key` の利用者は `score_one_entry` と `recent_history` の 2 つで、どちらも識別用キーとして使う。`folder.rs` の絞り込みは `normalize_for_match` 経由で NFC 済み
- icu_normalizer 2.4.0 の API・`CURSOR` の再借用で panic・`path_query` / `path_history_key` の記述
