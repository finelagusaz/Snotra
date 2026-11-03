# research — #1285 NFC の絞り込み文字列が NFD で保存された名前に一致しない

## issue の要約

NFC で打ったクエリ（`が` = U+304C）が、NFD で保存された名前（`か` + U+3099）に一致しない。照合用の正規化は小文字化と nucleo の折りたたみだけで、Unicode 正規化（NFC/NFD）をしていない。macOS 由来・一部アーカイブ由来のファイル名は NFD でありうる。

**射程（ユーザー決定 2026-11-03）**: 名前の照合のみ。`normalize_for_match` に NFC 化を足し、SPEC §4.2 に追記する。パスマッチ（`normalize_entry_key` / `path_query`）の NFC/NFD 不一致は別 issue に切り出し、永続のパスキーは変えない。

issue 本文の `to_lower_folded` / `normalize_query` は #1309（PR #1326）で `query::normalize_for_match` へ統合・削除済み（`git grep` 0 件）。issue の前提の関数名は古いが、主張（Unicode 正規化をしていない）は現行コードでも真。

## 実測（一時 probe テスト・撤去済み、2026-11-03、main 5f8cff4e 相当）

| 入力 | `normalize_for_match` | `Utf32String::from` 後 | `to_kana` |
|---|---|---|---|
| `がいど`（NFD） | `がいど`（そのまま） | `か い ど`（U+3099 を落とす） | `がいど` |
| `がいど`（NFC） | `がいど` | `が い ど` | `がいど` |
| `café`（NFD） | `café` | `cafe` | — |
| `café`（NFC） | `cafe` | `cafe` | — |
| `ガ`（NFD カタカナ） | `ガ` | `カ` | `が` |

- Fuzzy: haystack NFD `かいど` / needle `が` → `None`。needle `か` → `Some(36)`（NFD 名は濁点なしのクエリに一致してしまう）
- Substring: NFD 名 `.contains("が")` → false
- ローマ字検索: `to_kana("ga")` = `が`。NFD 名のかな名は `が` のままなので一致しない
- Latin の NFD（`café`）は Substring/Prefix では `cafe` に当たるが、正規化結果が非 ASCII のまま残るので `name_char_mask` は `u64::MAX`、また完全一致系のスコアは NFC 名と揃わない

## 関連するファイル・シンボル

- `snotra-core/src/query.rs`: `normalize_for_match`（照合用の正規化の唯一の入口・SPEC §4.2）、`fold_char`、`folds_to_itself`、`normalize_history_query_key`、`lower_file_name`。テスト: `normalize_for_match_agrees_with_the_owned_reference_on_every_codepoint` / `normalize_for_match_is_idempotent_on_every_codepoint`（単一コードポイントのみ）、`reference_normalize`、`all_codepoint_inputs`
- 名前側の呼び出し元（すべて `normalize_for_match` 経由）:
  - `snotra-core/src/search/build.rs`（Wave 1 の `lower_names`・かな名 `to_kana(&normalize_for_match(..))` 2 箇所）
  - `snotra-core/src/indexer/columns.rs`（`derive_entry_lowers` 付近の `lower_name`）、`query::lower_file_name`
  - `snotra-core/src/folder.rs`（`filter_sorted`・`matches_filter`・並べ替えキー L97）
- クエリ側: `snotra-core/src/search/query_plan.rs` `prepare_query_plan`（`norm_query`・`needle_u32`・`kana_query`・`path_history_key`）
- 履歴: `snotra-core/src/history.rs` `migrate_normalize_keys` が load のたびにクエリキーへ `normalize_history_query_key` を無条件に当てる → 版を上げずに新規則へ収束する（#1309 で同じ扱い・`migrate_normalize_keys_merges_query_keys_recorded_by_the_old_normalizer`）
- 永続: `snotra-core/src/indexer/cache.rs` の `IndexCache` が `lower_names` / `lower_file_names` / `char_masks` / `file_name_char_masks` を保存する。`INDEX_CACHE_VERSION = 8`、`INDEX_WIRE_SINCE = 7`。v7 は cache-miss（`docs/adr/ADR-index-v7-cache-miss.md`）。テスト: `index_cache_on_disk_format_is_stable`・`frozen_v7_is_a_cache_miss_and_is_not_rewritten`・`breakdown_still_reads_the_frozen_v7`・`load_cache_in_reports_the_version_it_actually_read`（`indexer/cache/tests.rs`）
- 仕様: `SPEC.md` §4.2「クエリ正規化」に NFC/NFD の記述なし

## 再利用できる既存のパターン

- **#1309（PR #1326）がほぼ同型**: 照合用の正規化の意味を変え、`INDEX_CACHE_VERSION` 7→8、旧現行版 v7 を cache-miss にし、履歴は load 時移行に任せ、SPEC §4.2 に追記、全コードポイント検知器で固定。今回は 8→9 で v8 を cache-miss にする
- ADR-index-v7-cache-miss「帰結」: 「次に派生の列の意味を変えるときも、その時点の現行版は同じ問いに当たる」——v8 も `entries` を持たないので同じ扱いが既定
- `snotra-core/CLAUDE.md`「IndexCache バージョン変更チェックリスト」: 意味だけの版上げでも 8（直前の版の受け手を数え直す）と 9（golden）が要る

## 技術的な制約

- **NFC の実装**: `icu_normalizer` 2.4.0 は既に `Cargo.lock` にある（`url` → `idna` → `idna_adapter` 経由・src-tauri 側）。snotra-core には未依存。API: `ComposingNormalizerBorrowed::new_nfc()`（`const fn`・compiled_data は default feature）、`normalize(&str) -> Cow<str>`、`is_normalized(&str) -> bool`、`split_normalized`。`unicode-normalization` は lockfile に無い（足すと新規 crate）
- **冪等性**: 「NFC → 折りたたみ」だけでは冪等にならない可能性がある。折りたたみ（小文字化・nucleo 表）が基底字を変えた結果、後続の結合文字と合成可能になる列がありうる（例: 単独で合成しない字 X が `e` へ畳まれ、`X́` → `é` → 次の適用の NFC で `é` → `e`）。既存の検知器は単一コードポイントしか回さないので、この 2 文字列を捕まえない。構成と検知器の両方で手当てが要る
- **借用の高速路**: ASCII は常に NFC。非 ASCII だけ NFC の判定を通せば ASCII 名（実運用点で 98% 超）の費用は変わらない
- **incremental（SPEC §4.2.1 条件 2）**: 正規化済みクエリのバイト prefix 拡張で判定しているので、NFC で合成が起きた打鍵（`か` → `が` = `が`）は prefix 拡張にならず full scan へ落ちる＝正しい側へ倒れる
- **`char_bitmask` / `name_char_mask`**: NFD Latin が ASCII へ畳まれるようになり、マスクが `u64::MAX` から実マスクへ変わる——マスクも意味が変わる列であり、版上げの対象に含まれる
- **corpus テスト**（`load_cached_entries`・`LegacyUpgrade::Skip`）: 開発機の実 `index.bin` が v8 のままだと v9 では読めず、そのテストはスキップ側へ倒れる（#1309 の版上げと同じ）
- v6 の `Skip` 分岐は旧い意味の lower 列をそのまま渡す（corpus テスト専用・製品は `Write` で導き直す）。今回の変更で新たに生じる問題ではない

## 敵対的調査（3b）の所見と採否（`workspace/adversarial-1285.txt`）

| 所見 | 判定 | 採否と理由（機序は自分で測り直した） |
|---|---|---|
| F1 名前照合の入口は `normalize_for_match` だけ | 壊せなかった | 採る。パス照合は別規則で残る非対称（射程どおり・別 issue） |
| F2 8→9・v8 は cache-miss | 壊せなかった | 採る |
| F3 「NFC→折りたたみ(→NFC)」は非冪等 | 壊せた | 採る。**自分で再測**（scratchpad probe・icu_normalizer 2.4.0 / nucleo 0.3.1）: `ł`+U+0301 → `l`+U+0301 → 再適用で `l`。`g = fold∘NFC` は「折りたたみで変わる字 1,823 × 2 字目候補 1,085」の 1,977,955 対で **7,734 対が非冪等**、固定点までの反復は **最大 3 回**（1 回 1,970,221・2 回 7,483・3 回 251）。停止規則「fold 出力が NFC なら止める」は固定点と 0 件不一致。canonical 分解 12,236 列は非冪等 0・反復 1。折りたたみで変わる 2 字目は U+0363–U+036F の 13 字（ccc 230・ラテン小字形の結合文字）で、全基底字 × この 13 字は非冪等 0 |
| F3c/F6 非冪等だと履歴キーが割れる | 壊せた | 採る。固定点構成で冪等性を構成的に閉じれば前提が戻る |
| F4 依存 | 一部壊せた | 採る。**自分で再測**: `default-features = false, features = ["compiled_data"]` で `Cargo.lock` の差分は依存辺 1 行だけ（新規 crate 0）。snotra-core の依存木には icu 系など 21 crate が入る（全て既に lockfile にあり src-tauri がビルド済み）。snotra-settings の依存木は 20 crate 増える |
| F5 incremental 条件 2 は健全 | 壊せなかった ⚠️（論理のみ） | 採る。正規化済みクエリの `starts_with` は、名前とクエリが同じ写像を通る限り部分文字列の含意を保つ |
| F9 実 `index.bin` は v8、NFD 名が実在（U+3099 10 件・U+309A 4 件） | 壊せた（環境の事実） | 採る。開発機で再現する＝受け入れの実データに使える。出所は未特定 ⚠️ |
| F9/F10 cache hit では lower 列はキャッシュから、kana 列は毎起動 `kana_for_cached`（build.rs:498）で再計算 | 壊せた（記述の欠落） | 採る。版上げが必須であることの根拠、かつ性能の測定点は `kana_for_cached`。実 config は 通常 substring / フォルダ fuzzy / migemo 有効 |
| nucleo 表の U+1E97 `ẗ` → `n`・U+1E96 `ẖ` → `l` | 自分で確認（`nucleo_normalize` 直呼び） | 既存の NFC 名でも同じ写像なので、今回の変更で NFD の `t`+U+0308 が NFC 名と同じ扱いになるだけ。表の誤りは射程外（記録のみ） |

## 未解決の疑問

- ~~冪等性を閉じる構成~~ → 解決: 固定点（上表 F3）。plan に反映
- 性能: 毎起動の `kana_for_cached`（全名・並列）と cache-miss の Wave 1 での費用。ASCII 高速路で大半は不変の見込み → plan の未確定で測る
- ~~依存~~ → 解決: 上表 F4。`icu_normalizer` を `default-features = false, features = ["compiled_data"]` で足す
