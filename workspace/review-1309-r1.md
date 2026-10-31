# review-1309（code-reviewer・変異注入）

- align が名乗った sha: fefe7d8580c5f25777d70bf85fa5440e5e323c32（HEAD 一致・作業ツリー clean）
- Critical: 0 件 / High: 1 件（H1 = 検知器が製品経路を外している）

## 所見

### High
- H1 名前側の空白正規化は、製品の索引経路（derive_entry_lowers・木の経路のかな build.rs:498・Wave1 のかな build.rs:139・lower_file_name）では検知器が無い。
  変異「その 1 か所だけを旧 to_lower_folded（小文字化+nucleo・空白は触らない）へ戻す」は 4 か所とも lib 全テスト緑で生き残った（下表 M5b2/M5c2/M5e/M5g）。
  受け入れ条件 6 のテスト entry_name_whitespace_is_normalized_like_the_query は SearchEngine::new（compute_wave1）しか通らず、キャッシュヒット/ミスの製品経路（save_cache_sorted_in -> derive_columns -> derive_entry_lowers・new_with_cached_masks）を通らない。
  A/B 突き合わせ save_side_collapse_and_assemble_measurement_agree_at_entry_view の fixture は空白類を含む名前を持たないので、空白の差を見ない（アクセントの差だけ見る: to_lowercase だけへ戻す変異は café で落ちた）。
  修正例: 同 fixture（search/tests/build.rs）に U+3000 入り・連続空白・先頭空白の名前を足す、または entry_name_whitespace_is_normalized_like_the_query を derive_columns -> from_material 経路でも回す。かなは migemo=true で同じ名前、file name は has_dot クエリ（例 名前 "a  b.exe" にクエリ "a b.exe"）を足す。
  根拠の補足: compute_wave1 の lower_names 枝は製品では from_material の材料が派生データを持たないとき（config_dir 不在・v2 の Skip）か cached lower が None のときしか走らない（build.rs:404-422・548-552）。cache-miss は save_cache_sorted_in -> derive_columns（cache.rs:467-468）、cache-hit はディスクの列か昇格の derive_columns。受け入れ条件 6 の検知器は本件の主題を製品がほぼ通らない経路でだけ固定している。

### Low
- L1 snotra-core/src/history.rs:869,876 — 新テストの内側キー文字列に U+000C（form feed）が入っている（c: のあとに FF、続いて akeapp.lnk）。意図は他テストと同じ c:(BS)fake(BS)app.lnk と推定（バックスラッシュ+f がエスケープとして解釈された）。成否には影響しないが制御文字のリテラルは読めない。
- L2 snotra-core/src/query.rs:323-333 — requires_alloc_for_nbsp と whitespace_only_normalizes_to_empty が NBSP・タブ・U+3000 を生のまま書いており目視で区別できない。修正: Rust の u{a0} / t / u{3000} エスケープで書く。
- L3 snotra-core/src/query.rs:196-198 のコメント「写した結果が空白になる文字も…そうしないと 2 回目の適用で結果が変わる」— 全コードポイントを測ると、空白でない文字で fold_char の結果に空白を含むものは 0 件（今の nucleo の表）。ゆえに変異 M4 は等価変異で生き残る。コードは将来の表の変化への防御として妥当だが、文は現に起きることとして読める。修正例:「nucleo の表が空白へ写す文字を持つ版では…（今の表には無い。全コードポイントの検知器が見張る）」。
- L4 散文の残存（v7 を現行と書く）: snotra-core/src/indexer/cache/tests.rs:946「現行版（v7）」・:970 expect("v7 が読めること")（実体は INDEX_CACHE_VERSION=8 で書いたバイト列）・:324-326「v6 は全ユーザーの index.bin が今まさに置かれている版」（v8 で偽。今そこに居るのは v7）。
- L5 snotra-core/tests/memory_footprint.rs:296-303 — on-disk が v7 のとき「フォールバック枝の数字…ロードの旧版枝が現行版へ書き戻す」と出るが、v7 は cache-miss（全走査の枝）であり旧版枝は通らない。計器の出力が測った枝を偽って名乗る。修正: v7（読まない版）を別腕にして「全走査の枝である」と出す。
- L6 folder.rs:110 の並べ替えキーが trim・空白圧縮を受けることで、空白類を含む名前の並び順が変わる（意図・計画で裁定済み）。旧へ戻す変異 M5f は検知されない——意図の固定が無い。
- ⚠ L7 GOLDEN_V7 と GOLDEN_V8 は版バイト以外同一（fixture の名前 Projects/app/Firefox/docs は旧新どちらの正規化でも同じ派生）。「v7 の列を信じると結果がずれる」検体は無く、v7 の扱いを守るのは版ゲートのテスト（M6 で実測）だけ。GOLDEN_V7 の doc（派生の列だけが前の意味を持つ）は、この fixture ではその差が現れないと一言添えるとよい。

### 確認して問題なしとした点
- 借用路と所有路の一致: 借用は「trim 済み・非空白の全文字が fold_char で自分 1 文字へ写る・空白は単一の U+0020 で連続しない」ときだけ。このとき所有路は同じ文字列を組む（fold_char が恒等、空白は 1 つ、trim 済みゆえ前後空白なし）。逆向き（借用できるのに所有）は値を壊さない。参照実装は split_whitespace（char::is_whitespace）で同じ規則を独立に持つ。
- v2〜v6 の Write 昇格は upgrade_legacy_cache_in -> save_cache_sorted_in -> derive_columns で新しい意味を導き直す（旧版の lower_names を持ち越さない）。Skip 腕は load_cached_entries だけが使い、木しか読まない。
- SPEC §4.2 の 2 点は実装と一致（正規化後が空になるのは空白類だけ・trim と同じ char::is_whitespace。prepare_query_plan は None、folder は全件）。
- git grep: normalize_query / to_lower_folded は PERFORMANCE.md の過去の記録 2 行以外 0 件。
- 照合位置を表示名へ写す処理（強調表示）は存在しない（git grep）ので、空白圧縮による位置ずれは無い。
- ⚠ tests/search_allocations.rs は件数に比例する確保だけを見るので、クエリ側の借用の退行（M3b）では落ちない。借用は ascii_only_borrows・history_query_key_borrows_when_no_change・冪等検知器の Borrowed 判定が捕まえる。

## 変異注入（cargo test -p snotra-core --lib --no-fail-fast。M3 は --test search_allocations も）

| 変異 | 結果 | 落ちたテスト | 巻き戻し後 hash（=アンカー） |
|---|---|---|---|
| M1 folds_to_itself を旧条件 !(is_uppercase or nucleo!=c) | 検知 | agrees_every_codepoint（93 件 ǅ ǈ ǋ）, idempotent_every_codepoint（5490 件）, migrate_normalize_keys_merges_query_keys_recorded_by_the_old_normalizer | query.rs d440108a OK |
| M2 fold_char 最後の flat_map(to_lowercase) を外す | 検知 | agrees（255 件 Ʀ ɢ）, idempotent（510 件 Ʀ->R->r）, migrate_..._old_normalizer | d440108a OK |
| M3a ASCII 近道を外す（if false） | 生存（等価変異・最適化のみ） | なし（search_allocations も緑） | d440108a OK |
| M3b 近道を !is_ascii_alphabetic | 検知 | ascii_only_borrows, history_query_key_borrows_when_no_change, idempotent（Borrowed 判定）。agrees と search_allocations は緑 | d440108a OK |
| M4 所有路の空白判定を q == 空白1字 に（写した空白を扱わない） | 生存（等価変異: 空白へ写る非空白文字は全コードポイントで 0 件を実測） | なし | d440108a OK |
| M5a compute_wave1 lower_names を to_lowercase | 検知 | prefix_matches_accented_entry, substring_matches_accented_entry, entry_name_whitespace_is_normalized_like_the_query, save_side_collapse_..._agree_at_entry_view | build.rs f1d7f681 OK |
| M5b derive_entry_lowers を to_lowercase | 検知 | save_side_collapse_..._agree_at_entry_view（index 2 café/cafe） | columns.rs c3c8f7a3 OK |
| M5b2 derive_entry_lowers を旧 to_lower_folded | 生存 | なし | c3c8f7a3 OK |
| M5c build.rs:498 木の経路のかなを to_lowercase | 検知 | save_side_collapse_..._agree_at_entry_view（migemo=true かふぇ/café） | f1d7f681 OK |
| M5c2 build.rs:498 を旧 to_lower_folded | 生存 | なし | f1d7f681 OK |
| M5d folder.rs:231 matches_filter 名前側を to_lowercase | 検知 | filter_normalizes_whitespace_on_both_sides_in_every_mode | folder.rs d7ed7f3a OK |
| M5e build.rs:139 Wave1 のかなを旧 to_lower_folded | 生存 | なし | f1d7f681 OK |
| M5f folder.rs:110 並べ替えキーを旧 to_lower_folded | 生存（L6） | なし | d7ed7f3a OK |
| M5g lower_file_name を旧 to_lower_folded | 生存 | なし | d440108a OK |
| M6 load_cache_in が現行枝で v7 も受理 | 検知 | frozen_v7_is_a_cache_miss_and_is_not_rewritten, load_cache_in_reports_the_version_it_actually_read | cache.rs 488246e6 OK |
| M7 cache_byte_breakdown_in から 7 を外す | 検知 | breakdown_still_reads_the_frozen_v7 | breakdown.rs d260dee0 OK |

全変異の後 git status は clean。ベースライン: 637 passed / 0 failed / 12 ignored。
