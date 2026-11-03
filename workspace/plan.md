# plan — #1285 照合用の正規化に NFC を足す（名前の照合のみ）

調査は `workspace/research.md`、反証は `workspace/adversarial-1285.txt`。

## 目的

NFC で打ったクエリが、NFD で保存された名前に一致するようにする（検索・ローマ字検索・フォルダ絞り込み・履歴のクエリキー）。仕様変更として SPEC §4.2 に書き足す。

射程（ユーザー決定 2026-11-03「名前の照合のみ (推奨)」）: `query::normalize_for_match` だけを変える。パスマッチ（`normalize_entry_key` / `path_query` / `search/path_store.rs`）は変えない（永続のパスキーを変えない）。パス側の NFC/NFD 不一致は別 issue に起こす。

## 受け入れ条件

1. NFD の名前 `がいど` が、NFC のクエリ `が` に Prefix / Substring / Fuzzy で一致する（`SearchEngine`）
2. フォルダ絞り込み（`folder::filter_sorted`）で、NFD の名前が NFC の絞り込み `が` に Substring / Fuzzy で一致する
3. ローマ字検索（migemo 有効）で、クエリ `ga` が NFD のカタカナ名 `ガ…` に一致する
4. NFD の Latin 名 `Café` の照合用の名前が `cafe`（ASCII）になり、`name_char_mask` が実マスクになる
5. `normalize_for_match` が冪等である——単一コードポイント全件（既存）と、結合の対の母集団（下記）で
6. v8 の `index.bin` を `load_cache_in` が cache-miss（`None`）として扱い、ファイルを書き換えない（その後の全走査と v9 の保存は、既存の cache-miss 分岐がそのまま受け持つ——v7 と同じ水準で守る）
7. 履歴の NFD のクエリキーと NFC のクエリキーが、load 時の移行で 1 つに合わさる
8. 振る舞いの変化を明示的に固定する: NFD の名前 `が` は、もはやクエリ `か` に Fuzzy で一致しない（今は nucleo が U+3099 を落として一致している。NFC 名と同じ扱いになる）

## 設計

`normalize_for_match` を「NFC → 既存の 1 回分の正規化」の**固定点**にする。

- 既存の本体（trim・空白の統一と圧縮・`fold_char`・借用の判定）を private `fold_once(&str) -> Cow<str>` に移す（中身は変えない）
- `normalize_for_match(s)`:
  1. `trimmed.is_ascii()` なら `fold_once(trimmed)` を返す（ASCII は NFC で不変・折りたたみの出力も ASCII）
  2. それ以外は `cur = fold_once(NFC(trimmed))`。`NFC.is_normalized(cur)` なら返す。そうでなければ `cur = fold_once(NFC(cur))` を繰り返す。上限 `MAX_COMPOSE_ROUNDS = 8`（母集団の実測の最大 3 回に余裕を持たせる）。上限に達したら `debug_assert!` で落とし、release ではその値を返す。doc に帰結を書く: その値は冪等でないので、履歴のクエリキーが記録時と load 時（`migrate_normalize_keys`）で割れうる
- 停止規則の根拠: `fold_once` の出力が NFC なら、次の 1 回は `fold_once(cur)` で、`fold_once` は冪等（既存の検知器）なので不動。よって返す値は `fold_once ∘ NFC` の不動点であり、冪等性は構成で閉じる。実測で停止規則と素の固定点の不一致 0（research.md F3）
- NFC は `icu_normalizer::ComposingNormalizerBorrowed::new_nfc()`（`const fn`）を `const` で 1 つ持つ
- 借用: NFC が `Borrowed` を返し `fold_once` も `Borrowed` なら借用のまま返す（非 ASCII で変える字の無い名前は確保しない）

依存: `snotra-core/Cargo.toml` に `icu_normalizer = { version = "2.4", default-features = false, features = ["compiled_data"] }`。実測で `Cargo.lock` の差分は依存辺 1 行・新規 crate 0（research.md F4）。`unicode-normalization` は lockfile に無い新規 crate なので採らない。

## 変更するファイルとシンボル

| ファイル | シンボル | 変更 |
|---|---|---|
| `snotra-core/Cargo.toml` | `[dependencies]` | `icu_normalizer` を追加 |
| `Cargo.lock` | — | 依存辺 1 行 |
| `snotra-core/src/query.rs` | `normalize_for_match`・新 `fold_once`・新 `NFC` 定数・`MAX_COMPOSE_ROUNDS`・`//!`・テスト `reference_normalize` / 新しい検知器 / 回帰テスト | 上の設計 |
| `snotra-core/src/indexer/cache.rs` | `INDEX_CACHE_VERSION` 8→9、`load_cache_in` の「v7 は読まない」の注記を「v7・v8 は読まない」へ（根拠は ADR-index-v7-cache-miss の帰結を指す）、`INDEX_WIRE_SINCE` の doc の「意味だけを変えた版上げ」に v9・#1285 を足す | `INDEX_WIRE_SINCE` は 7 のまま |
| `snotra-core/src/indexer/cache/breakdown.rs` | 読む範囲 | `INDEX_WIRE_SINCE..=INDEX_CACHE_VERSION` で v8 も読めることを確かめ、版のリテラルがあれば直す |
| `snotra-core/src/indexer/cache/tests.rs` | `GOLDEN_V8` を凍結の v8 として残し、現行 golden を `GOLDEN_V9` に。`index_cache_on_disk_format_is_stable`・`load_cache_in_reports_the_version_it_actually_read`（v8 の分岐）・新 `frozen_v8_is_a_cache_miss_and_is_not_rewritten`・新 `breakdown_still_reads_the_frozen_v8` | チェックリスト 8・9 |
| `snotra-core/src/history.rs` | テスト `migrate_normalize_keys_merges_query_keys_recorded_by_the_old_normalizer` と同型の NFD/NFC テスト | 製品コードは変えない（load 時移行が無条件に走る） |
| `snotra-core/src/search/tests/basic.rs`（と migemo 用に `search/tests/migemo.rs`） | 回帰テスト（受け入れ 1・3・4・8） | — |
| `snotra-core/src/folder.rs` のテスト | 回帰テスト（受け入れ 2） | 製品コードは変えない |
| `SPEC.md` §4.2 | 正規化の箇条に「Unicode 正規化（NFC）: 分解済み（NFD）の表記を合成済みと同じに扱う（例: `か`+U+3099 = `が`）」を足し、「パスマッチ（§4.1）はこの正規化を行わない」を 1 文 | 仕様変更 |
| `snotra-core/CLAUDE.md` | 「`query.rs` の正規化を変えたら…」に結合の対の検知器を足す | — |

触らない: `indexer/keys.rs`・`search/path_store.rs`・`search/query_plan.rs` の `path_query`・`history.bin` の版・`INDEX_WIRE_SINCE` の値・ADR（凍結。新しい却下は生じない——v8 の扱いは ADR-index-v7-cache-miss の「帰結」が予告した同じ問いで、同じ決定を当てる）。

## 実装の順序

1. Red: 受け入れ 1〜4・7・8 の回帰テストと、結合の対の検知器を先に書き、落ちることを確かめる
2. `icu_normalizer` を足し、`query.rs` を設計どおりに変える。`reference_normalize` も同じ固定点に揃える
3. `INDEX_CACHE_VERSION` 9 と cache のテスト群
4. SPEC・`snotra-core/CLAUDE.md`・`//!`
5. 検証と性能の測定

## 不変条件と異常系

- 冪等性（`normalize_for_match(normalize_for_match(x)) == normalize_for_match(x)`）: 構成で閉じ、検知器で固定する。崩れると履歴キーが記録時と load 時で割れる（research.md F3c）
- 借用と所有の一致（既存の検知器）
- 記録側と照合側が同じ関数を通る（既存の規約。変更は関数の内側だけ）
- 上限 `MAX_COMPOSE_ROUNDS` に達する入力: 母集団で 0 件を検知器が保証。到達すれば debug で落ち、release では最後の値を返す（落ちないが冪等でなくなり、履歴のクエリキーが割れうる——doc に書く）
- 版: v8 の `index.bin` は読まない・書き換えない。全走査は呼び出し側の cache-miss 分岐（既存）
- incremental（SPEC §4.2.1 条件 2）: 正規化済みクエリの prefix 判定のまま健全（research.md F5）。変えない

## テスト方針

結合の対の検知器（CI 常駐・`query.rs` の `mod tests`）`normalize_for_match_is_idempotent_on_composing_pairs`:
- 母集団 A: 「`fold_char` で変わる字」×「2 字目候補（ccc ≠ 0 の字 ∪ canonical 分解の 2 字目）」（実測 1,977,955 対）
- 母集団 B: 全コードポイントの canonical 分解（NFD）列（12,236 列）
- 各要素で冪等であることと、上限内で止まることを確かめる
- 母集団の根拠を doc に書く: 折りたたみも NFC も変えない対は、1 回目で不動点になる。「全基底字 × 折りたたみで変わる 2 字目（U+0363–U+036F の 13 字）」は一度きりの実測で非冪等 0（常駐させない・PR 本文に記録）。三つ組以上は常駐させない（一度きりの測定を PR に記録）

検証コマンド: `docs/build-commands.md`「変更後の検証チェックリスト」のカテゴリから該当するもの（Rust の fmt / clippy / test / doc、`npm run governance:check`）。

変異注入（`/implement`「3b」の委譲先が行う）: (a) NFC を外す → 受け入れ 1〜4 が落ちる、(b) 固定点の反復を 1 回にする → 結合の対の検知器が落ちる、(c) `INDEX_CACHE_VERSION` を 8 に戻す → `frozen_v8_*` が落ちる、(d) ASCII の近道で NFC を飛ばす条件を `is_ascii` 以外へ広げる → 既存の全コードポイント検知器か対の検知器が落ちる。

## SPEC・文書

- `SPEC.md` §4.2: 上表のとおり（仕様変更）
- `snotra-core/CLAUDE.md`: 検知器の追加
- 別 issue を起こす: パスマッチの NFC/NFD 不一致（射程外の記録）

用語: 新しく導入する語は無い（NFC / NFD / 合成済み / 分解済みは Unicode の標準の語・`docs/comment-guidelines.md`「言語（日英）」(A)）。「不動点」は数学の標準の語で、初出のコメントで「（もう一度通しても変わらない値）」と添える。

## 作業項目

### Phase 1 — Red

- [ ] 受け入れ 1・4・8 の回帰テスト（`search/tests/basic.rs`）
- [ ] 受け入れ 3 の回帰テスト（`search/tests/migemo.rs`）
- [ ] 受け入れ 2 の回帰テスト（`folder.rs` の `mod tests`）
- [ ] 受け入れ 7 の回帰テスト（`history.rs`）
- [ ] `normalize_for_match_is_idempotent_on_composing_pairs`（`ł`+U+0301 の固定例を含む）
- [ ] 上記が現行コードで落ちることを確認（受け入れ 8 は現行で落ちる＝`Some` が返る）

### Phase 2 — 正規化

- [ ] `snotra-core/Cargo.toml` に `icu_normalizer` を追加
- [ ] `fold_once` へ既存の本体を移し、`normalize_for_match` を固定点にする（`NFC` 定数・`MAX_COMPOSE_ROUNDS`）
- [ ] `reference_normalize` を**素の不動点**（`fold_once(NFC(x)) == x` になるまで反復）で書く。停止規則と同じ構造にしない——同じにすると、停止規則の誤りを借用/所有の検知器が拾えない
- [ ] `query.rs` の `//!` と `normalize_for_match` の doc に NFC と不動点を書く

### Phase 3 — 索引の版

- [ ] `INDEX_CACHE_VERSION` を 9、`load_cache_in` の注記、`INDEX_WIRE_SINCE` の doc
- [ ] `GOLDEN_V9`・凍結 `GOLDEN_V8`・`index_cache_on_disk_format_is_stable`
- [ ] `frozen_v8_is_a_cache_miss_and_is_not_rewritten`・`breakdown_still_reads_the_frozen_v8`・`load_cache_in_reports_the_version_it_actually_read`
- [ ] `breakdown.rs` の読む範囲に版のリテラルが無いことを確認（レビューで `breakdown.rs:267` は `INDEX_WIRE_SINCE..=INDEX_CACHE_VERSION` と確認済み。実装時に再確認）
- [ ] テストの doc の追従: `GOLDEN_V8` の doc（「現行版ゆえ forward-stability」）を `GOLDEN_V9` へ移し、`GOLDEN_V8` の doc を `GOLDEN_V7` と同型にする。`tests.rs` の「凍結 v8」等の言及（レビューの指摘 L180・196・217・227）と、#1309 から古い「現行版（v7）」（L1059 付近）を直す
- [ ] `cache.rs` の v7 cache-miss の注記（L897–902 付近）を、v7（#1309）と v8（#1285）の両方を説明する機序の書き方へ直す

### Phase 4 — 文書

- [ ] `SPEC.md` §4.2
- [ ] `snotra-core/CLAUDE.md` の検知器の行
- [ ] パスマッチの NFC/NFD 不一致の別 issue を起こす

### Phase 5 — 検証

- [ ] 検証チェックリストの該当カテゴリをすべて実行
- [ ] 実データ: 開発機の実 `index.bin`（v8・U+3099 を含む名前 10 件・照合用の名前が変わる 7 件）の名前で、NFC のクエリが当たることを確かめる（`match_cost` の実データ経路か一時テスト。実 `index.bin` は書き換えない・コピーして使う）
- [ ] 一度きりの測定（全基底字 × 13 字・三つ組）の結果を PR 本文の材料として残す

## 未確定（実装前に潰す）

- [x] 性能 — 一時テスト（撤去済み）で、開発機の実 `index.bin`（`load_cached_entries`・書き換えなし）の名前 16,196 件（非 ASCII 120・照合用の名前が変わるもの 7）に対し、設計どおりの試作を release・逐次で 3 標本: 変更前 377–382 µs → 変更後 448–454 µs（+約 70 µs）。`to_kana` 込みで 3.7–3.8 ms（`to_kana` が支配）。PERFORMANCE.md の実運用点（312k 件・非 ASCII 1.7%）へ比例で延ばしても +約 1.4 ms（逐次。`kana_for_cached` は並列）で、キャッシュヒットの組み立て約 210 ms の 1% 未満。判定: 近道は `is_ascii` のままで足りる。quick check の追加はしない
- [x] 結合の対の検知器の所要時間 — 同じ一時テストで母集団 A（基底 1,848 × 2 字目 1,085）を試作に当てた: 非冪等 0、debug 17.3 秒・release 0.9 秒。既存の全コードポイント検知器（debug 約 10 秒/本）の 2 倍弱。判定: 母集団は絞らない（絞る根拠の論証を増やすより、CI の十数秒のほうが安い）。注記: 基底の数が研究時の 1,823 から 1,848 に変わったのは、試作の判定が `normalize_for_match`（空白の扱いを含む）で、probe が素の `fold_char` だったため

## PR 本文に書くこと（作業項目ではない）

- v8 の利用者は更新後の初回起動で 1 回だけ全走査（22〜30 秒）を払う。ADR は触らない（ADR-index-v7-cache-miss の「帰結」が予告した同じ問いに、同じ決定を当てた）
- 一度きりの測定（全基底字 × U+0363–U+036F の 13 字・三つ組・性能）の数字

## セルフレビュー

- リスク: 高（永続形式 `index.bin` の意味の変更）
- plan-review: 独立レビュー1体（Step 2。射程が `AskUserQuestion` の裁定に依存するため）・結果は `workspace/plan-review-nfc.md`
- エージェント数: 1（ほかに 3b の敵対的調査 1 体）
- 要対処: 0 件。軽微 5 件をすべて反映（`reference_normalize` を素の不動点で書く・上限 8 と帰結の doc・受け入れ 6 を守れる水準へ・テストと `cache.rs` の注記の追従・PR 本文の記載）
- 主の照合: issue の要件 → 受け入れ 1〜3／境界（ASCII・借用・上限到達・空白・incremental）→ 不変条件とテスト／新しい状態なし／より単純な既存パターン（#1309 と同型）／冪等性の検知器あり。`/persistence-check`: 版上げ要（意味の変更）・旧形式は凍結 v8 の `load_cache_in` 経由テスト・デコード失敗の保全は既存の cache-miss（`index.bin` は派生データ）・history は load 時移行で収束、すべて満たす
- 未検証: 結合文字 3 個以上の列の反復回数（レビューの ⚠️。三つ組は Phase 5 の一度きりの測定で測る）
- 散文の全件列挙: 語 4 / 総件数 134 行 / ★未掲載 20（計画へ足した 0。`search.rs:124`・`search/build.rs:151`・`query_plan.rs:39` 等の記述は NFC 化後も真、残りは import・版の定数を読む箇所・凍結された歴史）
- 試行（#1300・高リスクのみ・独立レビュー 1 回ごとに 1 行）: 方式 Step 2 / 設計変更 0 / 主の見落とし 4（`reference_normalize` の独立性・上限到達の帰結・受け入れ 6 の水準・テストの doc の追従）

## 人間レビュー

- [x] 承認済み — 2026-11-03 / 問い: "`C:\workspace\Snotra\workspace\plan.md` をご覧のうえ、ファイルへ注釈を書き込むか、この場で承認してくださいまし。" / 回答: "ok"
