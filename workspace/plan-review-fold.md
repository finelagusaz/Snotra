# plan-review-fold: #1395 plan.md の独立レビュー（観点 1: 写像と冪等性・照合器 / 観点 2: 永続形式）

測定は probe `C:\Users\Eoh\AppData\Local\Temp\p1395\src\bin\rev.rs`（`rev3`〜`rev6` は派生）。probe は計画の `fold_latin`（規則 1〜4）と `query.rs` の `normalize_for_match` / `fold_once` / `folds_to_itself` / 参照実装を写したもの。リポジトリのコードと plan.md は変更していない。

## 要対処

1. 受け入れ 2 の検知器は、規則 3 の範囲では循環していて不動点にならない（観点 1 の指定の論点）
   - 規則 3 は 0.3.1 の `nucleo_normalize` を呼ぶ。検知器の参照側も「小文字化 → 0.3.1 → 小文字化」で同じ関数を呼ぶ。依存が動いて 0.3.1 の写像が変わると、製品側と参照側が一緒に動き、検知器は緑のままである。計画 L68 の「0.3.1 の写像が変わっても（依存を上げても）落ちて気づける」は偽
   - `snotra-core/Cargo.toml:13` は `nucleo-matcher = "0.3"` で、0.3.2 以降を許す。`cargo update` で写像が静かに変わり、永続化した派生の列（`lower_names` とマスク）の意味が版を上げずに変わる経路が開いている
   - 検知器が実際に検査するのは、規則 1 と 0.3.1 の両方が働く字（`é` など）で規則 1 が 0.3.1 と食い違わないことだけである。probe では、U+0000〜U+1CFF で「0.3.1 が字を変え、新しい fold が違う結果を返す」字は 0 件（食い違いなし）。この部分は有効
   - 直し方: 規則 3 の母集団を、リテラルの表で固定する（字 → 期待値の対）。事実の対で、表のデータの写しではない。少なくとも「規則 1 が働かず 0.3.1 が字を変える字」の件数と、出力が全部 ASCII であること（probe: 出力の非 ASCII 0 件）を固定する。`ł`→`l` や `ß`→`s` の対は受け入れ 2 の代表として書く。変異 (b) は今の設計でも落ちるが、依存更新には効かない
2. 規則 3 の「65 字」は再現しない（計画 L28・research.md:26・38）
   - probe（規則 1 が働かず、U+0000〜U+1CFF で 0.3.1 が字を変える字）は 138 字。小文字化後に重複を除くと 111 字（`rev4`・`rev3`）。ただし research の「分解できない字」の定義が違う可能性がある。定義を確かめるか、数を書くのをやめる（AGENTS.md「数え上げは古くなり始める。数ではなく正本を指す」）。「保つ」と宣言する文の隣に、再現しない数が置かれている

## 軽微

1. `normalize = false` の影響は、全コードポイントで測って、計画の受け入れ条件・テストで足りる（観点 1 の確認結果）
   - 根拠: nucleo-matcher 0.3.1 の `chars.rs:111-130`。`normalize = true` では非 ASCII の全字に case folding が掛かる。`false` では `char_class_non_ascii` が Upper と見た字だけに掛かる。ただし `Char::normalize`（chars.rs:134-143。prefilter・greedy・exact が使う）は `ignore_case` で常に `to_lower_case` を当てるので、`false` でも前段は畳む。`exact.rs`・`lib.rs` の `matches` 系は `Config.normalize` が針にも効くが、`scoring.rs` の呼び出しは `fuzzy_match` だけ
   - probe（`rev.rs`・`rev3.rs`）: 全コードポイントの 1 字で、自前の fold を通した haystack `ab{c}cd` と針 `{c}` を `fuzzy_match`（`Utf32String::from` で両辺）。`normalize = true` でだけ一致する字 0 件、`false` でだけ一致する字 37 件（`ᴀ`〜`ᴢ`・`ᵢ`〜`ᵥ`・`ẚ`・`Ↄ`/`ↄ`）、両方一致でスコアが違う字 0 件。自己一致の取りこぼし（`µ`・`ς`・`ϐ` など）は `true` でも同じで、前から在る
   - ASCII の針 × 自前 fold 済みの非 ASCII 1 字の haystack でも、`true` でだけ当たるのは 38 件、`false` でだけ当たる・スコアが違うのは 0 件。38 件は「0.3.1 が今も ASCII へ畳むが、自前の fold が畳まない字」で、受け入れ 3・5 が意図する字と一致する。`ẚ`・`ẛ`（U+1E9A/B。0.3.1 は `o`）も含まれるが、計画は恒等にするだけで足りる
   - 計画 L62 の `µ` と `μ` の順位は、この単字の測定では出ない。名前の `µ` × クエリの `μ` の組の効果で、組は測っていない（未検証 2）
   - `Config` は `#[non_exhaustive]` だが、フィールドへの代入は許される。`thread_local!` の初期化子はブロック式（`{ let mut c = Config::DEFAULT; c.normalize = false; Matcher::new(c) }`）で書ける。probe で同じ形が通る
2. 規則 1 は NFC 済み入力以外でも想定外の字を返さない（観点 1 の確認結果）
   - 全コードポイントで、規則 1 が働く字（canonical 分解の先頭を辿って ASCII の英字になる字）は 488 字。256 字ごとのブロックは U+00xx に 53・U+01xx に 147・U+02xx に 44・U+1Exx に 244 で、Latin-1 補助から U+024F と U+1E00〜U+1EFF の外は 0 件（`rev5`）。`Decomposed::Singleton` で ASCII の英字になるのは U+212A（K）だけで、`Singleton` は辿らない（小文字化が先に `k` にする）ので影響しない。先頭が ASCII の英字で、2 字目の canonical 結合クラスが 0 の分解は 0 件
   - 規則 1 が U+1D00 未満で 0.3.1 の写像と食い違う字は 0 件（計画の「食い違い 0 件」は成立）。0.3.1 が字を変えない 22 字（`ǖ` 系列など）だけが新しく畳まれる
3. 冪等性・借用と所有の一致は、計画の設計で崩れない（観点 1 の確認結果）
   - `rev.rs`: 全コードポイント × 5 文脈（既存の `all_codepoint_inputs` と同じ）556 万 3 千 320 件で、`normalize_for_match` の非冪等 0 件・借用に戻らない 0 件・素朴な参照実装との不一致 0 件。2 字の列（既存の母集団の作り方。`firsts` は新しい `folds_to_itself` から作る）281 万 5,575 件で非冪等 0 件。`MAX_COMPOSE_ROUNDS` の到達 0 件
   - 2 字の列の母集団は、`firsts` が `!folds_to_itself` で作られるので、新しい写像に自動で追従する。母集団の手直しは要らない
   - `reference_fold_once` は `fold_latin` を呼ぶ形でよい（借用と所有の一致の検知器は、写像の中身ではなく借用の判定を見る）。写像の中身は受け入れ 1〜3 のテストが持つ
4. 規則 3 の範囲で U+1D00 以降を恒等にして失うものは無い
   - U+1D00〜U+FFFF で 0.3.1 が字を変える字のうち、自前の fold が 0.3.1 と一致するのは 19 字、違うのは 113 字（小文字化後・`rev6`）。113 字は全部、0.3.1 の写像が誤り（`ḃ`→`e`・`ḋ`→`e`・`ⁱ`→`A` など）で、自前の fold は正しい字へ畳むか、恒等にする。補助面は 0.3.1 が字を変える字 0 件
5. 履歴の移行（観点 2 の確認結果）
   - `history.rs:127-128` で、`from_loaded` は読み込みのたびに `migrate_normalize_keys` を通す。`history.bin` の版を上げない判断は正しい。パスのキーは `normalize_entry_key`（`indexer/keys.rs:53` の `to_lowercase` だけ）で、折りたたみの影響を受けない
   - ただし、旧写像が誤って畳んで記録したクエリキー（`ẗ` → `n` など）は、ASCII の別のキーとの区別がつかず、移行で直せない。新しい照合では `t` になるので、記録していたブーストは宙に浮く。対象は誤って畳まれていた字を打ったクエリだけで、受容してよいが、計画の「クエリキーは読み込み時の移行で収束」は、旧写像で畳み損ねていた 22 字については正しいが、誤って畳んでいた字については正しくない。1 行書き添える
   - #1285 は `history.rs` に移行のテスト（`migrate_normalize_keys_merges_nfd_and_nfc_query_keys`）を足した。計画の「触る」表に `history.rs` が無い。`ǘ` と `u` の 2 つのキーが 1 つに合わさるテスト（#1285 と同型）を足すと、移行の主張に裏付けが付く
6. 索引の版上げ 9→10 の計画は、`snotra-core/CLAUDE.md`「IndexCache バージョン変更チェックリスト」と #1285 の差分（`git show 5f50f72e`）に照らして、実質漏れていない
   - 欄 2・8（a)(b)(c)・9 が必要で、欄 1・3〜7（フィールド追加）は意味だけの版上げなので不要（CLAUDE.md の「データの意味だけを変えて版を上げるときも、8 と 9 は要る」）。`INDEX_WIRE_SINCE` は上げない（線上表現は不変）。計画の Phase 3 と一致する
   - 計画に書かれていない追従（#1285 の差分にある）: (a) `tests.rs:268-269`・`332` の doc が `GOLDEN_V9` を指す。現行を `GOLDEN_V10` に改名すると、この指しが古い意味になる。(b) `tests.rs:196-227` のアサーションのメッセージ「凍結 v9 バイト」。(c) `GOLDEN_V8` / `GOLDEN_V7` の doc が「GOLDEN_V9 との差は版のバイトだけ」と書いている（`tests.rs:291`・`312`。連鎖の先頭が `GOLDEN_V10` になる）。(d) `cache.rs:51` の「v8・#1309、v9・#1285」へ「v10・#1395」。(e) `cache.rs` の `load_cache_in` の注記（`v7・v8 は読まない` を `v7・v8・v9` へ）。(f) `tests.rs:1471` の `load_cache_in_reports_the_version_it_actually_read` の v9 の分岐は、v8 の分岐（`tests.rs:1504`）と同型で足す。実装時に `git grep -n "GOLDEN_V9\|v9" snotra-core/src/indexer` で列挙する
   - `frozen_v7_*`・`frozen_v8_*`・`breakdown_still_reads_the_frozen_v7/v8` は変更不要（どちらも今も cache-miss）
7. 版を上げる必要性の補足
   - 最新のタグ `v0.19.4` の `INDEX_CACHE_VERSION` は 7（`git show v0.19.4:snotra-core/src/indexer/cache.rs`）。v8・v9 は未リリースで、リリース済みの利用者は v7 のため、すでに全走査になる。9→10 は、v9 を持つ開発機・先行ビルドだけに 1 回の全走査を払わせる。コストは小さい
   - 版を上げずに v9 へ畳む案は、v9 を持つ開発機が旧い意味の列を使い続けるので、規則（「データの意味を変えるときは版を上げる」・ADR-index-v7-cache-miss の代替案 E の却下）に反する。#1285 も v8 が未リリースのまま 8→9 へ上げた。計画のとおり上げてよい
8. 規則 2 が呼ぶ NFKD の組み立て
   - probe は関数内で `DecomposingNormalizerBorrowed::new_nfkd()` を呼ぶ。製品では `const`（`NFC` の定義と同じ形。`query.rs:169`）にするか、`properties::CompatibilityDecompositionBorrowed` を使う。実装時に決める。計画は形を決めていない
9. 規則 2 の字数を数えて「ASCII の英字 1 字だけ」を固定するテストは、計画にある `⁹`→`⁹`（数字は畳まない）で足りる

## 未検証

1. ⚠️ バイナリサイズ: 規則 2 は `compiled_data` の NFKD の表を製品に引き込む。増分は測っていない。NFC・canonical 分解は既に引いている（`query.rs:169` と `#1285`）。小さな表なので大事ではないはずだが、`cargo build --release` の exe サイズ差は測っていない
2. ⚠️ 名前 `µ` × クエリ `μ`（および `ς`/`σ` など）の組の順位（計画 L62）は、組で測っていない。単字の自己一致と ASCII の針との組は上のとおり差が無い。research.md の F2 で `Some(36)`→`Some(0)` と測っているので、そちらを信用している
3. ⚠️ 計画の他の項目（テストの実装・`cargo test` の実行）は、リポジトリを変更しない取り決めなので実行していない。`reference_fold_once` の書き換えと `folds_to_itself` の ASCII の近道のコメントは、probe では、写しの上でしか確かめていない。近道（`!c.is_ascii_uppercase()`）は、規則 1・2・3 のどれも ASCII に働かない（`decompose` は ASCII で `Default`）ので、写像を足しても成り立つ
4. ⚠️ アロケーションと性能: 計画の +4%（63,328 字）は信用するが、`new_nfkd()` を毎回組み立てる形と、`const` にした形の差は測っていない（規則 2 は U+2070〜U+209F の 32 字にしか働かない）
5. ⚠️ icu_normalizer の更新（`Cargo.toml` は `2.4`）で、新しい Latin の合成済み字が入ると、規則 1 の出力が静かに増え、永続化した列の意味が版を上げずに変わる。`to_lowercase` の Unicode 版の更新にも同じ性質が前からある。新しい検知器は要らないが、受け入れ 2 のリテラルの表に「規則 1 が働く字の件数」を足すと、動いたときに気づける（488 字が現在値。数を書くなら AGENTS.md の「数は正本を指す」に注意）
