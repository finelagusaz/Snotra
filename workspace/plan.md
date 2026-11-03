# plan — #1395 折りたたみを自前の写像にし、照合器に 0.3.1 の表を使わせない

調査は `workspace/research.md`、反証は `workspace/adversarial-1395.txt`。

## 目的

照合用の正規化の 1 文字の写像（`query.rs` の `fold_char`）が、nucleo-matcher 0.3.1 の壊れた表で誤った ASCII 字へ畳む（`ẗ`→`n`・`ḋ`→`e`・`ⁱ`→`a`）。また、結合文字を 2 つ持つ 22 字（`ǘ` など）を畳まない。自前の写像に置き換える。照合器の内部も同じ表を haystack にだけ当てているので、照合器の `normalize` を切る。

方針と射程（ユーザー決定・2026-11-03）: 照合器は 0.3.1 のまま（#1395 のコメントの案 E）。射程はラテン文字全体（訂正後の再確認「全体でいこう。」）。

## 受け入れ条件

1. `normalize_for_match` で `ẗ`→`t`・`ḋ`→`d`・`ⁱ`→`i`・`ₕ`→`h`・`ǘ`→`u`・`ȫ`→`o`（大文字も同じ）
2. 0.3.1 の写像で畳めていた U+1D00 未満の字は、1 字も写像が変わらない（`ł`→`l`・`ß`→`s`・`Č`→`c` など）
3. 0.3.1 が誤って畳んでいた字のうち、自前の写像が対応しない字は恒等になる（`ᴀ`・`ↄ`）
4. 検索で、名前 `ḋata` がクエリ `data` に Prefix / Substring / Fuzzy で当たり、名前 `ẗest` がクエリ `nest` に当たらない
5. フォルダ絞り込みの Fuzzy で、名前 `ᴀbc` が絞り込み `ᴀ` に当たる（照合器が haystack にだけ 0.3.1 の表を当てていた頃は外れた）
6. `normalize_for_match` は冪等のまま（既存の全コードポイントと 2 字の列の検知器）
7. v9 の `index.bin` を `load_cache_in` が cache-miss として扱い、書き換えない

## 設計

`fold_char` の `.map(nucleo_normalize)` を、新しい private `fold_latin(c: char) -> char` に替える。小文字化 → `fold_latin` → 小文字化の順は変えない。

`fold_latin(c)`:
1. canonical 分解を先頭の字だけ辿る（`CanonicalDecompositionBorrowed::decompose` が `Expansion(a, _)` を返す間 `a` へ進む）。辿った先が `c` と違い ASCII の英字なら、その字を返す（`Č`→`C`・`ǘ`→`u`・`ẗ`→`t`）
2. U+2070〜U+209F（上付き・下付き）: 互換分解（NFKD）が ASCII の英字 1 字なら、その字（`ⁱ`→`i`・`ₕ`→`h`）。数字は畳まない（Latin-1 の `²` と扱いを揃える。0.3.1 の表の意図も英字だけ）
3. U+1D00 未満: 0.3.1 の `nucleo_matcher::chars::normalize`（分解できない `ł`・`ø`・`ß` などの写像を保つ。この範囲で 0.3.1 の写像は分解規則と食い違い 0 件。表のデータは写さない——nucleo-matcher は MPL-2.0）

規則 2 の互換分解の照会器は、`NFC` と同じく `const` で 1 つ持つ（関数の中で毎回組まない）。
4. それ以外: 恒等

`search/scoring.rs` の照合器の生成を `Matcher::new(MatcherConfig { normalize: false, ..})` 相当にする（`Config` は `#[non_exhaustive]` なので、`DEFAULT` を写してフィールドへ代入する）。名前も needle も照合の入口で畳み済みなので、照合器の折りたたみは要らない。照合器を生成するのはここ 1 か所（research.md 争点 4）。

却下した設計（ADR は要らない——一本道の選択）: 上流の修正版の表を写す（MPL-2.0、かつ上流の表にも `ḋ`→`e` などの誤りがある）、範囲で区切って U+1D00 以降だけ自前にする（同じ規則に条件が 1 つ増える）。

## 変更するファイルとシンボル

| ファイル | シンボル | 変更 |
|---|---|---|
| `snotra-core/src/query.rs` | `fold_char`・新 `fold_latin`・`folds_to_itself` の ASCII の近道のコメント・`//!`・テストの `reference_fold_once`・新しいテスト | 設計どおり |
| `snotra-core/src/search/scoring.rs` | `MATCHER` の `Matcher::new` | `normalize = false` と、その理由の 1 行 |
| `snotra-core/src/search/tests/ranking.rs` | `bitmask_filter_does_not_skip_accented_entries` のコメント（L156「nucleo はアクセント正規化（é→e）でマッチする」） | 「照合の入口が畳む」へ |
| `snotra-core/src/search/tests/basic.rs` | 回帰テスト（受け入れ 4） | — |
| `snotra-core/src/folder.rs` のテスト | 回帰テスト（受け入れ 5） | — |
| `snotra-core/src/indexer/cache.rs` | `INDEX_CACHE_VERSION` 9→10、`INDEX_WIRE_SINCE` の doc に v10・#1395、`load_cache_in` の注記に v9 | — |
| `snotra-core/src/indexer/cache/tests.rs` | `GOLDEN_V10`（現行）・凍結 `GOLDEN_V9`・`frozen_v9_is_a_cache_miss_and_is_not_rewritten`・`breakdown_still_reads_the_frozen_v9`・`load_cache_in_reports_the_version_it_actually_read` の v9 の分岐 | #1285 と同型 |
| `SPEC.md` §4.2 | 「ラテン文字のアクセント折りたたみ（é→e, ü→u 等）」に、上付き・下付きの英字（`ⁱ`→`i`）も畳むことを足す | 仕様の追記 |

触らない: `Cargo.toml`（依存は増えない）・パスマッチ（#1394）・`history.bin`（クエリキーは読み込み時の移行で収束する。ただし旧い写像が誤って畳んで記録したキー（`ẗ` を `n` として記録したもの）は ASCII の別のキーと区別できず、移行では直らない——該当は U+1D00 以降の字を打ったクエリだけで、受容する）・ADR。

## 実装の順序

1. Red: 受け入れ 1〜5 のテストと、受け入れ 2 の検知器を書いて落とす（受け入れ 2 は現行では緑——変異用の検知器）
2. `fold_latin` と `fold_char` の差し替え、`reference_fold_once` の追従
3. 照合器の `normalize = false`
4. 版を 10 へ
5. 文書・コメント

## 不変条件と異常系

- 冪等性: 1・2 の出力は ASCII、3 は 0.3.1 の写像（既存の検知器で冪等と固定済み）、4 は恒等。全コードポイントと 2 字の列の検知器で確かめる
- 記録側と照合側が同じ関数を通る（変更は `fold_char` の内側だけ）
- 照合器の `normalize = false` で、フォルダ絞り込みの Fuzzy の順位がわずかに動く字がある（`µ` と `μ`・小型大文字）。research.md F2 で受容
- 版: v9 は読まない・書き換えない

## テスト方針

- 受け入れ 1・3: `query.rs` の `mod tests` に写像の表のテスト
- 受け入れ 2: 2 本に分ける（plan-review 要対処 1——規則 3 は 0.3.1 の写像そのものを呼ぶので、0.3.1 と比べる検知器はその範囲で循環し、依存を上げても緑のまま推移する）
  - `fold_char_agrees_with_0_3_1_where_both_fold`（仮名）: U+0000〜U+1CFF で、「小文字化 → 0.3.1 → 小文字化」が字を変え、かつ規則 1 も働く字は、両者が一致する（規則 1 が 0.3.1 の正しい写像を変えていないこと）
  - `fold_char_below_1d00_is_pinned`（仮名）: U+0000〜U+1CFF の全字の `fold_char` の出力を順に畳んだダイジェスト（64 bit）をリテラルで固定し、字を変える出力がすべて ASCII であることも確かめる。0.3.1 の表を写さずに、依存の更新や規則の変更で写像が動いたことに気づける（動いたら、意図した変更なら版を上げてダイジェストを更新する）
- 履歴: `history.rs` に、`ǘ` と `u` のクエリキーが読み込み時の移行で 1 つに合わさるテスト
- 受け入れ 4: `search/tests/basic.rs`、受け入れ 5: `folder.rs` のテスト
- 受け入れ 6: 既存の検知器
- 受け入れ 7: `indexer/cache/tests.rs`

検証コマンド: `docs/build-commands.md`「変更後の検証チェックリスト」の該当カテゴリ（A。E・F はツールが常に回す）。

変異注入（`/implement`「3b」の委譲先）: (a) `fold_latin` の規則 1 を外す → 受け入れ 1 の `ẗ`・`ǘ` と受け入れ 4 が落ちる、(b) 規則 3 を外す（U+1D00 未満も恒等）→ 受け入れ 2 のダイジェストの検知器が落ちる、(b') 規則 1 の結果を大文字で返す等で 0.3.1 と食い違わせる → 受け入れ 2 の一致の検知器が落ちる、(c) 照合器の `normalize` を真に戻す → 受け入れ 5 が落ちる、(d) 版を 9 に戻す → `frozen_v9_*` が落ちる、(e) 規則 2 で数字も畳む → 受け入れ 1 の表（`⁹` は恒等）が落ちる。

## SPEC・文書

- `SPEC.md` §4.2: 上表のとおり
- `snotra-core/CLAUDE.md`: 変更なし（検知器の行は #1285 で整っている。受け入れ 2 の検知器は `query.rs` の正規化の検査の一部として doc から名指す）

用語: 新しく導入する語は無い（「互換分解」「canonical 分解」は Unicode の標準の語）。

## 作業項目

### Phase 1 — Red

- [x] 受け入れ 1・3 のテスト（`query.rs`）
- [x] 受け入れ 2 の検知器 2 本（`query.rs`・ダイジェストは現行の写像で一度測って固定し、Phase 2 の後に 22 字ぶん変わった値へ更新する）
- [x] 履歴の `ǘ` / `u` のテスト（`history.rs`）
- [x] 受け入れ 4 のテスト（`search/tests/basic.rs`）
- [x] 受け入れ 5 のテスト（`folder.rs`）
- [x] 受け入れ 1・3・4・5 が現行で落ち、受け入れ 2 が現行で緑であることを確かめる → 実測: 受け入れ 1・2（2 本）・4・履歴が現行で落ちた。受け入れ 5（`ᴀbc` × `ᴀ`）は現行で緑——現行の写像が両側の `ᴀ` を `a` へ畳むため。照合器の `normalize` を戻す変異 (c) の検知器としては、新しい写像の下で働く（計画の想定の訂正）

### Phase 2 — 写像と照合器

- [x] `fold_latin` を足し、`fold_char` を差し替える。`folds_to_itself` の ASCII の近道のコメントを直す
- [x] テストの `reference_fold_once` を `fold_latin` に追従させる
- [x] `scoring.rs` の照合器を `normalize = false` にする
- [x] `search/tests/ranking.rs` L156 のコメント

### Phase 3 — 索引の版

- [x] `INDEX_CACHE_VERSION` を 10、`INDEX_WIRE_SINCE` の doc、`load_cache_in` の注記
- [x] `GOLDEN_V10`・凍結 `GOLDEN_V9`・`frozen_v9_*`・`breakdown_still_reads_the_frozen_v9`・`load_cache_in_reports_the_version_it_actually_read`
- [x] 版の言及の追従を `git grep -n "GOLDEN_V9\|凍結 v9\|v9・#1285\|v7・v8"` で数えて直す（plan-review 軽微 6: `tests.rs` の `GOLDEN_V9` を指す doc とアサーションのメッセージ、「`GOLDEN_V9` との差は版のバイトだけ」、`cache.rs` の `INDEX_WIRE_SINCE` の doc、`load_cache_in` の注記）

### Phase 4 — 文書

- [x] `SPEC.md` §4.2
- [x] `query.rs` の `//!` と `fold_char` の doc

### Phase 5 — 検証

- [x] 検証チェックリストの該当カテゴリをすべて実行
- ラウンド 1: sha f352b6a1 / ツール green（A・E・F・本体のチェックアウトから。worktree からはツールが linked worktree を拒んだ） / Critical・High 0、Medium 1・Low 4（⚠️ 3） / 変異 7 種すべて検知、ただし一致の検知器が (b'1) を生存（M1）。対処: M1 一致の検知器が `fold_char` の出力を 0.3.1 の製品の経路（小文字化 → 表 → 小文字化）と比べる形へ（表を直に引くと `İ` を誤って数えた。直した後に同じ変異で 24 件検知を自分で確認）、L1 `query.rs` の nucleo を主語にしたコメント、L2 SPEC の上付き・下付きの範囲を明記。L3（`ẛ` は恒等）・L4（前提は今も真）は見送り
- ラウンド 2: sha 6c920a11 / ツール green（A・E・F） / 指摘 0 / 変異 5 種すべて検知・生存 0。Step 4 で `docs/adr/ADR-latin-fold-own-mapping.md` を起こし（照合器の乗り換え 4 案と上流の表を写す案・範囲で区切る案の却下）、`fold_latin` の doc から指した
- [x] 互換分解（NFKD）の表が増やすバイナリサイズを、`snotra` の release ビルドの exe サイズの前後で測り、PR 本文の材料として残す → 規則 2 の互換分解を外した版 10,779,136 B、入れた版 10,836,480 B（+57,344 B・+0.5%）。許容

## 未確定（実装前に潰す）

- [x] 写像の妥当性 — probe（`%TEMP%\p1395`・撤去は PR の後）で試作を測った: U+00A0〜U+FFFF の全字で冪等（非冪等 0）、U+1D00 未満で変わるのは意図した 22 字だけ、0.3.1 で畳めていた字の写像の変化 0。代表: `ẗ`→`t`・`ḋ`→`d`・`ⁱ`→`i`・`ₕ`→`h`・`ǘ`→`u`・`ȫ`→`o`・`ł`→`l`・`ß`→`s`・`Č`→`c`・`ᴀ`→`ᴀ`・`⁹`→`⁹`（数字は畳まない）・`ↄ`→`ↄ`
- [x] 性能 — 同じ probe で U+00A0〜U+FFFF の 63,328 字の「小文字化 → 写像 → 小文字化」: 0.3.1 70.5 ns/字 → 試作 73.6 ns/字（+約 4%、5 回の最良値）。名前の大半は ASCII の近道を通る（#1285 の実測で非 ASCII 1.7%）ので、起動・索引構築への影響は測定誤差の範囲。判定: 前計算の表は作らない

## セルフレビュー

- リスク: 高（永続形式 `index.bin` の意味の変更）
- plan-review: 独立レビュー1体（Step 2。射程が `AskUserQuestion` の裁定に依存するため）・結果は `workspace/plan-review-fold.md`
- エージェント数: 1（ほかに 3b の敵対的調査 1 体）
- 要対処: 2 件を反映（受け入れ 2 の検知器の循環 → 一致の検知器とダイジェストの検知器の 2 本へ、「65 字」の数を削除）。軽微は 5（履歴の例外と `ǘ`/`u` のテスト）・6（版の言及の追従を作業項目へ）・8（互換分解の照会器を `const`）を反映、1〜4・7 は確認の報告で変更なし
- 主の照合: issue の要件 → 受け入れ 1〜5／境界（ASCII・U+1D00 の境・上付きの数字・小型大文字・分解の単一字）→ テスト／新しい状態なし／より単純な既存パターン（#1285 と同型）／冪等性と写像の固定に検知器あり。`/persistence-check`: 版上げ要（意味の変更）・凍結 v9 の `load_cache_in` 経由テスト・`index.bin` は派生データで cache-miss が保全・history は読み込み時移行（例外を明記）
- 未検証: 互換分解の表のバイナリサイズ（レビューの ⚠️。Phase 5 の作業項目で測る）
- 散文の全件列挙: 未実施（`prose:sweep` の代わりに、版の言及は作業項目の `git grep` で数える）
- 試行（#1300・高リスクのみ・独立レビュー 1 回ごとに 1 行）: 方式 Step 2 / 設計変更 1（受け入れ 2 の検知器の構成） / 主の見落とし 2

## 人間レビュー

- [x] 承認済み — 2026-11-04 / 問い: "`C:\workspace\Snotra\.claude\worktrees\fix-fold-table-1395\workspace\plan.md` へ注釈を書き込むか、この場で承認してくださいまし。" / 回答: "ok"
