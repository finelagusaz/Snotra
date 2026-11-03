# plan-review-nfc (#1285 plan の独立レビュー・観点: 冪等性 / 永続形式)

測定: scratchpad の p2 (icu_normalizer 2.4.0 + nucleo 0.3.1, release, repo の Cargo.lock 流用)。製品コードと plan.md は未変更。

## 結論
設計の骨子（固定点構成・v8 を cache-miss にする 8->9・history.bin 据え置き）は、今回の測定の範囲では崩れなかった。要対処は無し。

## 要対処
なし。

## 軽微
1. reference_normalize を停止規則と同じ構造（「出力が NFC なら止める」）に揃えると、停止規則の主張（返す値 = 素の不動点）が CI で独立に検証されない。plan は「揃える」としか書いていない。参照は素の不動点（`g(x)==x` まで反復。g = fold_once∘NFC）で書くのが良い。その形なら `normalize_for_match_agrees_with_the_owned_reference...` が停止規則の誤りも拾う。research F3 の「停止規則と不動点の不一致 0」は一度きりの実測で、常駐検知器に入っていない。
2. 上限 MAX_COMPOSE_ROUNDS=4 は release では黙って非冪等な値を返す（debug_assert のみ）。母集団の最大は 3 回なので 4 回は妥当だが、アーカイブ由来の悪意ある名前はテスト母集団の外にありうる。到達時の帰結は plan の記述どおり「照合が少しずれる」だけでなく、履歴キーが記録時と load 時 (`migrate_normalize_keys`, history.rs:400) で割れて重複キーになる形でもある。doc に帰結を書くか、release の上限を大きく（例 16）して「到達しない」の根拠を母集団の最大回数との差で示すこと。
3. 受け入れ 6 の後半（cache-miss 後に全走査して v9 を書く）を直接守るテストが plan に無い。既存の `frozen_v7_is_a_cache_miss_and_is_not_rewritten`（tests.rs:1207-1225）も load_cache_in が None を返し書き換えないことまでしか見ない。v7 の前例と同じ水準なので許容でき、全走査側の分岐は呼び出し側の既存コード。受け入れの文言を「load_cache_in が None」へ弱めるか、既存の load_or_scan 系テストで v8 入力を通すかを選ぶ。
4. 文書・コメントの追従漏れ候補:
   - tests.rs:263 の GOLDEN_V8 doc「version 8 ヘッダー込み」「現行版ゆえ…forward-stability だけ」を GOLDEN_V9 へ移し、GOLDEN_V8 の doc は GOLDEN_V7 (tests.rs:287-296) と同型に「v9 化の前に実際に書かれていた v8。fixture の名前は ASCII で空白類も NFD も持たないので GOLDEN_V9 との差は版のバイトだけ」と書く。
   - tests.rs:1059 のコメント「現行版（v7）」は既に古い（#1309 の取りこぼし）。触るなら直す。
   - tests.rs:180/196/217/227 の `GOLDEN_V8`・「凍結 v8」を GOLDEN_V9 に（plan は `index_cache_on_disk_format_is_stable` としか書いていない）。
   - cache.rs:897-902 のコメントは「v7 は照合用の正規化を 1 つにする前の意味」と版固有に書いてある。v8 の理由（NFC・#1285）を足すか、版を列挙せず機序で書く。
5. ADR を触らない判断は妥当（`ADR-index-v7-cache-miss.md` の「帰結」が同じ問いの再来を予告、ADR は凍結）。ただし ADR のファイル名・決定文が「v7」固定なので、CLAUDE.md:108 の参照先として残るのは正しいが、v8 を同じ扱いにした根拠の参照先は cache.rs のコメントに残す必要がある（plan の「根拠は ADR の帰結を指す」で足りる）。

## 検証した点（問題なし）
- 冪等性の論証（返す r は fold_once の出力かつ NFC → fold_once(NFC(r)) = fold_once(r) = r、n(r) も同値）に穴は見つからなかった。空白: NFC が空白類を非空白へ、またはその逆へ変える単一コードポイントは 0 件（測定 `ws changes=0`）。fold_once は trim・圧縮を毎回かけ直すので、NFC 前後の空白の入れ替わりは吸収される。fold が字数を増やす例 (U+0130 -> i + U+0307) も、前置 {空,空白,全角空白}× 6 基底 (İ I i ẞ K Ω ł 含む 8 字) × 結合文字 U+0300-036F の 2 連（三つ組）で、停止規則の結果 = 素の不動点 = 再適用の結果が全件一致 (`triples bad=0`)。
- 母集団 A の盲点候補だった Hangul の algorithmic 合成（L+V, LV+T は canonical 分解表に無く ccc=0 なので「2 字目候補」から漏れる）: 折りたたみで変わる 1,823 字 × 初声/中声/終声/音節 (U+1100-11FF, AC00-D7A3) を両順序で全件確認、停止規則 = 不動点 = 冪等、不一致 0。
- 全コードポイント × {U+0301, U+0323, U+0363} も不一致 0。
- ASCII 近道 (`trimmed.is_ascii()`): NFC は ASCII 不変・fold の出力も ASCII。再適用の借用判定 (`twice` が Borrowed) は、出力が NFC かつ fold_once の不動点なので NFC.normalize が Borrowed を返し fold_once も Borrowed になる。既存検知器の `matches!(twice, Cow::Borrowed(_))` は維持される見込み（実装時に確認）。
- 呼び出し元 (grep): folder.rs:34,97,160,190、history.rs:154,278,400,874(test)、indexer/columns.rs:185-186、search/build.rs:117,123,139,498、search/query_plan.rs:78,140、query.rs:25,140。すべて `normalize_for_match`/`normalize_history_query_key`/`lower_file_name` の 3 入口経由で、別実装は無い。記録側（columns.rs:185, build.rs:117,139,498, folder.rs:97）と照合側（query_plan.rs:78, folder.rs:34,160）は同一関数なので、関数の内側だけの変更で割れない。`measure_derived_sharing` は `lower_name == name` を値で測る (query.rs:126) ので、NFD 名は共有されず lower 列に保存される（正しい側）。ハイライト用のインデックスを名前→表示名へ写す箇所は無い (grep `indices|highlight` 0 件)ので、NFC による字数の変化が表示を壊さない。
- incremental (SPEC 4.2.1 条件 2): 正規化後の prefix 判定のまま。`か`→`が`のように合成が起きる打鍵は prefix にならず full scan に落ちる。

## 永続形式（8->9）の確認
- `snotra-core/CLAUDE.md`「IndexCache バージョン変更チェックリスト」: 1-7 はフィールド追加の項で、意味だけの版上げは 8 と 9 のみ要る（同書「データの意味だけを変えて版を上げるときも、8 と 9 は要る」）。plan は 8 (a) `breakdown.rs:267` の `INDEX_WIRE_SINCE..=INDEX_CACHE_VERSION` が 9,8,7 を自動で拾う（リテラル無し・確認済み）、(b) `load_cache_in_reports_the_version_it_actually_read` に v8 の分岐を足す、(c) `frozen_v8_is_a_cache_miss_and_is_not_rewritten`（`Write` を渡して load_cache_in 経由）、9 の golden 更新を含む。漏れなし。
- load_cache_in (cache.rs:860-895): 現行版の try_deserialize は `INDEX_CACHE_VERSION` 固定。v8 バイトは版ヘッダー不一致で落ち、続く IndexCacheV6/V5/V4 の各版も不一致で全て Err -> None（cache-miss）になる。v7 と同じ経路で、新しい分岐は不要。
- `INDEX_WIRE_SINCE` は 7 のまま（線上表現は不変）。doc (cache.rs:51) の「v8・#1309」へ v9・#1285 を足す。plan 記載のとおり。
- history.bin: `migrate_normalize_keys` が load のたびにクエリキーへ `normalize_history_query_key` を当てる (history.rs:384-400) ので、#1309 と同じく版を上げずに新規則へ収束する。パスキー (`normalize_entry_key`) は不変で、履歴の内側のキーは変わらない。判断は正しい。ただし「冪等でない」と移行時に割れるので、軽微 2 の上限の扱いがここに効く。
- kana 列: `kana_for_cached` (build.rs:498) は毎起動、名前から `to_kana(&normalize_for_match(..))` で再計算し、キャッシュに載らない。char_masks / file_name_char_masks / lower_names / lower_file_names は載るため版上げ（cache-miss）が手当て。NFD の Latin は ASCII に畳まれてマスクが実マスクへ変わる（u64::MAX は上位集合なので旧値でも誤判定はしないが、lower 列自体が旧い意味）。版上げ以外の手当ては要らない。
- ADR-index-v7-cache-miss の「帰結」「A 案が費用で勝つ」: 連続 2 回目の全走査（22-30 秒）をユーザーに強いる。ユーザーは v7 の前例どおりの判断を #1285 で承認済みという前提で、争点にしない。ただし PR 本文に「v8 のユーザーは初回に全走査 1 回」と明記すること（plan の人間レビュー欄に無い）。

## 未検証
- ⚠️ 母集団 A の「2 字目候補（ccc≠0 ∪ canonical 分解の 2 字目）」を plan の定義どおり再現してはいない。私の測定は独立に (a) 折りたたみで変わる 1,823 字 × Hangul 全体、(b) 6 基底+ł× U+0300-036F の三つ組、(c) 全コードポイント × 3 結合文字、で補った。母集団 A そのもの（1,977,955 対）の再実行と、基底×(13 字の三つ組以上) の反復回数の上限（最大 3 を超える入力の存在）は未確認。
- ⚠️ 非 BMP / ccc≠0 の記号類（U+1D165 など）や CJK 互換・結合文字（U+3099/309A 以外）を 2 字目にした場合は (c) の 3 字と母集団 A に頼っており、私の測定では見ていない。
- ⚠️ 実装後の借用/所有の検知器 (`agrees_with_the_owned_reference`) が、plan の「reference を同じ不動点に揃える」で通るかは、実装がまだ無いため未測定。
- 開発機の実 `index.bin` を使った Phase 5 の実データ検証は未実施（レビューの範囲外）。
- 性能の数字（plan 未確定欄: +約 70 µs）は再測していない。
