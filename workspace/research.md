# research — #1395 nucleo-matcher 0.3.1 の折りたたみ表の誤り

## issue の要約

照合用の正規化 `query::normalize_for_match` の 1 文字の写像 `fold_char` は、`nucleo_matcher::chars::normalize`（0.3.1）を呼ぶ。この表が壊れており、`ẗ`（U+1E97）を `n` へ写すなど、誤った ASCII 字へ畳む。

方針（ユーザー決定・2026-11-03・#1395 のコメント）: 照合器は 0.3.1 のまま使い、折りたたみだけを自前の写像にする（案 E）。上流の git 依存・`ncp-matcher`・自前フォーク・別の照合器は却下（理由は #1395 のコメント）。

**射程（ユーザー決定・2026-11-03）: ラテン文字全体。** 自前の写像を全範囲に当て、今正しく畳めている字の写像は変えない。

**訂正（敵対的調査 F1・自分で再測）**: 下の表の「105 字が畳まれない」は、0.3.1 の表を大文字に直に当てた測定の産物だった。製品の `fold_char` は先に小文字化するので、Latin Extended-A は今も畳めている（`Dvořák`→`dvorak`・`Čapek`→`capek`・`Łódź`→`lodz`）。製品の経路で U+1D00 未満の食い違いは **22 字**（結合文字を 2 つ持つ `ǖ` `ǘ` `ǚ` `ǜ` `ǟ` `ǡ` `ǭ` `ǻ` `ȫ` `ȭ` `ȱ` とその大文字）だけ。ユーザーには訂正を伝え、射程「全体」の再確認を得た（2026-11-03「全体でいこう。」）。製品の経路で壊れているのは: U+1D00 以降の誤写像（`ẗ`→`n`・`ḋ`→`e`・`ⁱ`→`a`）と、この 22 字の未折りたたみ。

## 実測（一時 probe・`%TEMP%\p1395`・nucleo-matcher =0.3.1・icu_normalizer =2.4.0・上流の表は helix-editor/nucleo@8c16d47c の `matcher/src/chars/normalize.rs` を include した。リポジトリには入れない）

0.3.1 の表の壊れ方（`src/chars/normalize.rs`）:

- `generate_table` が `sparse_data[j].1` ではなく常に `DATA1[j].1` を読む → `TABLE2`（`DATA2`・U+1D00〜）と `TABLE3`（`DATA3`・U+2071〜U+209C）が `DATA1` の値で埋まる
- `DATA2` は整列しておらず（末尾に U+1EA4〜U+1EF1 が後から並ぶ）、`generate_table` は整列を前提に進むので、途中から照合が外れて恒等になる。L390 では `('\u{1EF9}', 'y')` がコメントに呑まれている
- U+1D00〜U+209F で、0.3.1 の写像と上流の修正版が食い違う字: 322 字（`ᴀ`→`A`・`ᴃ`→`A`・`ᴉ`→`C` など）

Latin-1 / Extended-A / Extended-B（U+00A0〜U+024F）で、0.3.1 の写像と「canonical 分解の基底文字が ASCII の英字ならその字」（以下「分解規則」）を比べた:

| 区分 | 件数 | 例 |
|---|---|---|
| 両方が畳み、一致 | 139 | `é`→`e`・`ü`→`u` |
| 0.3.1 だけが畳む（分解できない字） | 65（U+00A0〜U+024F・大文字に直に当てた数。plan-review は U+0000〜U+1CFF で別の定義により 138 字と数えた） | `ł`→`l`・`ø`→`o`・`đ`→`d`・`ß`→`s`・`ı`→`i`・`ƒ`→`f` |
| 分解規則だけが畳む（0.3.1 は恒等） | 105 | `Ā`・`Č`・`Ě`・`Ğ`・`Ĺ`・`Ń`（probe の出力の先頭 30 件から） |
| 両方が畳み、食い違う | 0 | — |

→ 0.3.1 の `DATA1` も途中から照合が外れており、Latin Extended-A の大半を畳まない。SPEC §4.2「ラテン文字のアクセント折りたたみ（é→e, ü→u 等）」に反する（「Dvořák」がクエリ `dvorak` に当たらない）。

上流の修正版（8c16d47c）も誤りを持つ: `ḋ`(U+1E0B)→`e`・`ḣ`→`g`・`ẉ`→`j`・`⁹`→`0` など（分解規則と比べて 25 字の食い違いのうち、`ḋ`〜`ẉ`・`⁹` は上流の誤り）。**上流の表を正解の基準にしない。**

## 自前の写像の案（1 文字・NFC 済みの字に当てる）

1. canonical 分解（再帰）の先頭が ASCII の英字で、後ろが結合文字なら、その英字（`Č`→`C`・`ẗ`→`t`・`ḋ`→`d`）
2. U+2070〜U+209F（上付き・下付き）: 互換分解（NFKD）が ASCII の英数字 1 字ならその字（`ⁱ`→`i`・`ₕ`→`h`・`⁹`→`9`）
3. U+1D00 未満で 1 に当たらない字: 0.3.1 の写像（`ł`→`l` など。この範囲では 0.3.1 の写像は分解規則と食い違い 0 件）
4. それ以外: 恒等（U+1D00〜U+1DFF の小型大文字 `ᴀ` などは上流の修正版でも恒等）

1・2 は `icu_normalizer`（#1285 で既に依存）で求まる。3 だけ nucleo の `normalize` を残す（表のデータを写さない。nucleo-matcher は MPL-2.0、Snotra は MIT）。

## 関連するファイル・シンボル

- `snotra-core/src/query.rs`: `fold_char`（L148 付近・`.map(nucleo_normalize)`）、`folds_to_itself`、`normalize_for_match`、テストの `reference_fold_once`（L538 付近で `nucleo_normalize` を直に呼ぶ）、検知器 `normalize_for_match_*_on_every_codepoint`・`normalize_for_match_is_idempotent_on_composing_pairs`
- `snotra-core/src/search/scoring.rs`: `Matcher::new(MatcherConfig::DEFAULT)`（L73・スレッドごとの照合器）、`fuzzy_match_score`（検索とフォルダ絞り込みの Fuzzy が共有）
- nucleo-matcher 0.3.1 `src/chars.rs` L111-131: `Config.normalize` が真なら、照合器は haystack の非 ASCII 字ごとに同じ壊れた表を当て、さらに大文字小文字を畳む。needle は `fuzzy_match` の呼び出し側が渡したまま（`Pattern` を使わない Snotra では畳まれない）
- `nucleo_matcher::Config` は `#[non_exhaustive]` だがフィールドは `pub`（`DEFAULT` を写してから `normalize = false` を代入できる）
- `snotra-core/src/indexer/cache.rs`: `INDEX_CACHE_VERSION = 9`（#1285 で上げた）・`INDEX_WIRE_SINCE = 7`・`load_cache_in` の「v7・v8 は読まない」
- `SPEC.md` §4.2

## 再利用できる既存のパターン

- #1285（PR #1396）と #1309: 照合用の正規化の意味を変え、`INDEX_CACHE_VERSION` を上げ、直前の版を cache-miss にし、凍結バイト列のテストを足す。`docs/adr/ADR-index-v7-cache-miss.md` の「帰結」の同じ問い
- 履歴のクエリキーは `migrate_normalize_keys` が読み込みのたびに新規則へ収束させる（版上げ不要）

## 技術的な制約

- `Config.normalize` を真のままにすると、自前の写像で畳んだ後の haystack に照合器が 0.3.1 の表をもう一度当てる。自前の写像が恒等で残す字（U+1D00〜U+1DFF など）が、照合器の中だけで誤って写る。needle は畳まれないので、haystack と needle が食い違う → `normalize = false` にする（両側とも照合の入口で畳み済み）
- `normalize = false` は照合器の大文字小文字の畳みの条件も変える（`case_fold` が「大文字のときだけ」になる）。両側とも照合の入口で小文字化済み
- 冪等性: 1・2 の結果は ASCII なので、もう一度通しても変わらない。全コードポイントと 2 字の列の検知器で確かめる
- 性能: `fold_char` は非 ASCII の字ごとに呼ばれる（毎起動の `kana_for_cached` を含む）。分解の照会の単価を測る
- `index.bin` の派生の列の意味が変わる → 9→10、v9 は cache-miss

## 敵対的調査（3b）の所見と採否（`workspace/adversarial-1395.txt`）

| 所見 | 判定 | 採否 |
|---|---|---|
| F1 105 字は測定の産物。製品の経路の食い違いは 22 字 | 壊せた | 採る（自分で再測・上の訂正） |
| 争点 1 後半: U+00A0〜U+024F で 0.3.1 が畳む字は分解規則と食い違い 0 | 壊せなかった | 採る |
| 争点 2: U+1D00 未満で今畳めている字の写像は変わらない | 壊せなかった | 採る。CI の検知器にする |
| 争点 3 前半: `normalize = true` だと照合器が haystack にだけ壊れた表を当てる（`ᴀbc` × `ᴀ` が `None`） | 壊せなかった | 採る。`normalize = false` にする |
| F2: `normalize = false` で変わるのは大文字小文字の扱いだけ、は偽。`char_class_non_ascii` が Lower と見る字（`µ`・`ς` など）は最適化段で畳まれず、前段と食い違う（`µm` × `μ` が `Some(36)`→`Some(0)`）。`ᴀbc` × `ab` は `Some(62)`→`None` | 壊せた | 採る。影響はフォルダ絞り込みの Fuzzy だけ（実 config の通常検索は Substring）。名前の側も needle の側も照合の入口で同じ写像を通っており、`µ`/`μ` の一致は照合の入口が畳まない字どうしの話なので受容する。`ᴀ` を畳まないのは上流の修正版と同じ |
| 争点 4: 0.3.1 の表を使う経路は `fold_char`（とテストの参照実装）と `scoring.rs` の `Matcher::new` だけ | 壊せなかった | 採る。`search/tests/ranking.rs:156` のコメントが古くなる → 計画に入れる |
| 争点 5: 自前の写像で全コードポイントが冪等 | 壊せなかった | 採る |
| 規則 1 の後続はすべて U+0300〜U+036F・規則 1 が発火する字は NFC 済み入力で到達する | 壊せなかった | 採る。「後ろが結合文字」の判定は不要（基底文字が ASCII の英字かだけを見る） |
| W1 U+1D00〜U+1DFF の 0.3.1 の写像はすべて誤り | ⚠️ | 採る（恒等にして失うものは無い） |
| W2 修飾文字（`ᵃ` など）・`ª`・全角英字は恒等のまま | ⚠️ | 射程外（0.3.1 も畳まない現状維持）。規則 2 も英字だけにして `²` と `⁴` の扱いを揃える |
| W3 `ↄ`(U+2184)→`E` は誤り、自前案で恒等 | ⚠️ | 採る |
| W4 `µ`/`μ` の順位が動く | ⚠️ | 受容（F2） |

## 未解決の疑問

- ~~「後ろが結合文字」の判定~~ → 不要（上表）
- `ß`→`s` は保つ（0.3.1 の写像を変えない）
- 性能の増分 → plan の未確定で測る
