# 計画 — #1309（＋#1310・冪等性）照合用の正規化の入口を 1 つにする

## 目的

SPEC §4.2 が「クエリおよびエントリ名」に課す 1 つの正規化を、コードでも 1 つの関数にする。あわせて、その関数の 2 つの既存の欠陥（借用判定のずれ #1310・51 字の非冪等）を直し、全コードポイントの検知器で固定する。ユーザー裁定は #1309 のコメント（2026-10-31 の 2 件）にある。

## 受け入れ条件

1. 照合用の正規化の入口は `query::normalize_for_match`（`Cow<str>`）の 1 つになる。規則は SPEC §4.2 どおり（前後空白除去・空白類〔タブ・NBSP・U+3000 などの `char::is_whitespace`〕を `' '` へ統一・連続空白の圧縮・小文字化・ラテン文字の折りたたみ）
2. クエリ側（`prepare_query_plan`・`normalize_history_query_key`・`folder.rs` の絞り込み）と名前側（`compute_wave1` の `lower_names`・`lower_file_names`・`kana_lower_names`、木の経路 `kana_for_cached`、`indexer/columns.rs` の `derive_entry_lowers`、`query::lower_file_name`、`folder.rs` の `build_sort_keys`）が、すべてこの関数を通る
3. 旧 API `normalize_query` と `to_lower_folded` は消す（呼び出し点の移行と 1 つの変更に束ね、移行漏れはコンパイルエラーで捕まえる・`AGENTS.md`「条件別チェック」の関数の行）
4. **#1310**: 借用で返す条件と所有で組み立てる結果が、全コードポイントで一致する。ASCII の小文字・数字・記号と単一の空白だけの入力は、確保なしで借用を返す
5. **冪等性**: 全コードポイントで `normalize_for_match(normalize_for_match(c)) == normalize_for_match(c)`。折りたたんだあとにもう一度小文字化して閉じる
6. 本体検索で、名前 `資料　2024`（U+3000）・`a  b`（連続空白）・` app`（前の空白）が、クエリ `資料 2024`・`a b`・`app` に、Prefix / Substring で一致する
7. `index.bin` の版を 8 へ上げる（`lower_names` / `lower_file_names` の意味が変わるため）。v7 を読んだときに旧い意味の派生の列を使わない（cache-miss・「未確定」の裁定）
8. SPEC §4.2 に 2 点を足す。(a) 空白類（タブ・NBSP・全角空白など）は半角空白に揃える（クエリ側の既存の挙動を明文化し、名前側にも及ぶことを示す）。(b) 正規化後に空なら、その面の空の扱いに従う（挙動は変えない。ユーザー裁定）
9. パスマッチの系統（`path_query`・`path_store.rs`・`indexer/keys.rs` の `normalize_entry_key`）は**範囲外**で変えない

## 変更ファイル

| ファイル | 変更 |
|---|---|
| `snotra-core/src/query.rs` | `normalize_for_match` を定義（借用判定は文字ごとの折りたたみが恒等かで決め、組み立てと同じ関数を使う）。`normalize_query` / `to_lower_folded` を消す。`normalize_history_query_key`・`lower_file_name` を新しい入口へ。`//!` の「正規化は冪等」を、検知器で固定した主張として書き直す。全コードポイントの検知器 2 本（冪等性・借用と所有の一致）とタブ・全角空白・NBSP のテスト |
| `snotra-core/src/search/build.rs` | `compute_wave1`・`kana_for_cached` を新しい入口へ |
| `snotra-core/src/indexer/columns.rs` | `derive_entry_lowers` を新しい入口へ |
| `snotra-core/src/folder.rs` | `normalize_query` と `build_sort_keys` の `to_lower_folded` を新しい入口へ |
| `snotra-core/src/search/query_plan.rs`・`history.rs` | `normalize_query` の呼び出しを新しい入口へ |
| `snotra-core/src/indexer/cache.rs` | `INDEX_CACHE_VERSION` を 8 へ。v7 は cache-miss（「未確定」の裁定）。`snotra-core/CLAUDE.md`「IndexCache バージョン変更チェックリスト」の全項目と、直前の版を受け取る側の 3 項（breakdown の鎖・版の帰属のテスト・v7 の凍結バイト列を `load_cache_in` 経由で読むテスト）。golden bytes の更新 |
| テスト（`search/tests/basic.rs` など） | 受け入れ条件 6 の名前の一致。既存テストの `normalize_query` / `to_lower_folded` の参照を移す |
| `SPEC.md` | §4.2 に受け入れ条件 8 の 2 点 |
| `snotra-core/CLAUDE.md` | 「`query.rs` の正規化と `Cow<str>` 遅延アロケーション」を新しい入口の名前へ。`to_lower_folded` を名指す散文（`search.rs` の doc・`cache.rs` の doc など）を直す（G-stale-identifiers が見る） |
| `snotra-core/src/indexer/cache/breakdown.rs` | 版の鎖に v7 を旧版として足す（直前の版を受け取る側の (a)） |
| `snotra-core/src/indexer/cache/tests.rs`・`search/tests/build.rs` | 版の帰属のテスト（(b)）・v7 の凍結バイト列が cache-miss になるテスト（(c)）・`to_lower_folded` の参照の移行 |
| `snotra-core/src/search.rs`（:124 の doc）・`search/scoring.rs`（:520 のコメント） | 旧 API を名指す散文を直す |

**prose:sweep の裁き**（語 5 / 186 行 / ★未掲載 41）: 上の 4 行を計画へ足した。`src-tauri` の 3 件・`autostart.rs` などは無関係な「冪等」。`PERFORMANCE.md` の `to_lower_folded`・`INDEX_CACHE_VERSION` は過去の測定の記録で、偽にならない。`tests/memory_footprint.rs`・`engine.rs` は定数を読むだけで、版を上げても真のまま。`indexer/keys.rs`・`path_store.rs`・`search/tests/path.rs` はパスマッチの系統（範囲外）

### 散文の直し（**ガバナンス検査は捕まえない**——`snotra-core/CLAUDE.md`・`PERFORMANCE.md`・`.rs` の doc は G-stale-identifiers の母集団の外で、母集団の中に旧 API の名指しは 0 件・独立導出）

手で直す一覧（独立導出の 5 節。行番号は main の版）:
- 今すでに偽: `query.rs` の `//!`「正規化は冪等」・`history.rs:384`（直したあと真になる）・`to_lower_folded` の doc の折りたたみの順の主張・`indexer/cache/tests.rs:1380`
- 変更で偽になる: `search.rs:124`・`search/build.rs:151`・`indexer/cache.rs:135-137` と `:783`・`GOLDEN_V7` の doc・`indexer/cache/breakdown.rs:291`・`indexer/columns.rs` の `CachedLower` の doc・`folder.rs:224` と `:866`・`tests/memory_footprint.rs:266`
- `snotra-core/CLAUDE.md`: `:125`（所有に切り替わる条件の列挙が今すでに足りない。titlecase・単独のタブ・U+3000）・`:156`（`normalize_query` を名指す）
- 実装を終えたら、旧 API の名前（`normalize_query`・`to_lower_folded`）をリポジトリ全体で `git grep` し、凍結層（`docs/adr/`・`docs/superpowers/`）と過去の測定の記録（`PERFORMANCE.md` の該当行）以外に残らないことを確かめる

## 実装順序

1. `query.rs`: 新しい入口をシグネチャだけ置き、全コードポイントの検知器 2 本と空白類のテストを書いて Red を確かめる。実装して Green
2. 呼び出し点を移し、旧 API を消す（`cargo build -p snotra-core` と下流の `cargo build -p snotra` を移行漏れの検知器にする）
3. 本体検索のテスト（受け入れ条件 6）
4. `index.bin` の版の変更と、チェックリストの全項目
5. SPEC・`snotra-core/CLAUDE.md`・doc の散文

## 不変条件と異常系

- 借用判定と組み立ては同じ文字単位の関数を使い、別の条件を書かない（#1310 の型をコードの形で断つ）
- 検索のホットパス: ASCII の小文字だけのクエリで確保が増えないことを、`tests/search_allocations.rs` の検知器で確かめる
- 版を上げても、検索結果が変わるのは空白類を含む名前・前後に空白を持つ名前・51 字を含む名前だけ
- `history.bin`: 既存のクエリキーは読み込み時の `migrate_normalize_keys` で新しい規則へ収束する（版は上げない。移行は既存の仕組みが担う）

## テスト方針と検証

- 全コードポイント（サロゲートを除く `char`）の検知器。文脈は `c`・`ac`・`Acx`・`x  c`・`c c` の 5 つ（#1310 の表と、空白をはさむ形）:
  - **一致**: 出力が、テスト専用の参照実装（常に所有で組み立てる素朴な版）と一致する。借用で返るのは、参照の出力が trim 済みの入力と等しいときに限る
  - **冪等**: `f(f(s)) == f(s)`、かつ `f(s)` を入れ直すと借用で返る。`normalize_history_query_key` も同じ
  - どちらも現行の実装で落ちることを Red として確かめる（31 字・51 字）。折りたたみの順は、この検知器を緑にする形で決める（nucleo が空白でない文字を空白へ写すかも、この掃引で分かる）
- `index.bin`: `GOLDEN_V7` は本物の旧版の凍結バイト列として残し、`load_cache_in` 経由で cache-miss になることを確かめるテストを足す。v8 の golden を別に置く。v7 の線上表現は v8 と同じなので、旧版の構造体を新しく作らず `IndexCache<'static>` を版 7 として扱えばよい（独立導出）
- 本体検索: 受け入れ条件 6 の 3 例
- 永続化: `/persistence-check` を実装後に当てる（版の判定・旧形式の凍結バイト列・デコード失敗時の保全）
- 検証カテゴリ: A（Rust）と E・F。C は該当しない（ウィンドウ・ホットキーに触れない）
- 変異注入は委譲先が行う（借用判定を旧条件へ戻す・折りたたみ後の小文字化を外す・名前側の 1 か所だけを旧関数へ戻す、など）

## フェーズ 1 — 入口と検知器

- [x] 新しい入口の Red（全コードポイントの 2 本・空白類）→ Green — Red の実測: 参照実装との不一致 348 件（`ǅ` など #1310 を含む）・冪等でない 6000 件（`Ʀ`→`R`→`r`。5 文脈 × 2 関数）。折りたたみの順は「小文字化 → 折りたたみ → 小文字化」で両方 Green
- [x] 呼び出し点の移行と旧 API の削除
- [x] 本体検索の受け入れ条件 6 のテスト — `compute_wave1` の名前側を `to_lowercase` へ戻す変異で落ちることを実測

## フェーズ 2 — 永続化

- [x] `INDEX_CACHE_VERSION` 8 と、チェックリストの全項目（v7 は cache-miss） — フィールドは増えないので 1・3・5〜7 は該当なし。(a) breakdown は v7 も同じ構造体で読む・(b) 版の帰属のテストに v7 = cache-miss・(c) `GOLDEN_V7` を本物の凍結バイト列として残し `frozen_v7_is_a_cache_miss_and_is_not_rewritten`（`Write` でバイト列不変）と `breakdown_still_reads_the_frozen_v7`・golden は `GOLDEN_V8` を別に置いた
- [x] v2〜v6 の `Skip` の腕が旧い意味の列を製品の経路へ返すかを確かめ、返すなら直す — **返さない**。`Skip` を渡す入口は `load_cached_entries` だけで、それは `.material.tree().materialize()` しか使わず派生の列を捨てる（`git grep LegacyUpgrade::Skip`）。直さない
- [x] 却下した v7 の扱い（木から導き直す A・木専用の導出 C・版を上げない D/E）を ADR に残す — `docs/adr/ADR-index-v7-cache-miss.md`。コードの説明から参照するので、Step 4 を待たず実装のコミットに含めた

## フェーズ 3 — 文書

- [x] SPEC §4.2 の 2 点 — 空の扱いは実装で確かめた（検索は UI の `trim().is_empty()` が空クエリへ倒し、正規化で消えるのは空白類だけなので一致する。フォルダは `filter_norm.is_empty()` で絞り込みなし）
- [x] `snotra-core/CLAUDE.md` と doc の散文 — 旧 API の名前は `git grep` で `PERFORMANCE.md` の過去の記録 2 行（凍結の扱い）以外 0 件。v7 を「木の形式」として描く doc（`index_tree.rs`・`search/tests/*`）は v7 以降で真のまま置いた

## 検証とレビュー（/implement Step 3）

- [x] 1 巡目（fefe7d85）: 道具 green（A・E・F）。委譲は Critical 0・High 1（名前側の記録・かな・ファイル名の空白の正規化を守る検知器が無い。変異 4 つが生存）・Low 7。全件対処——A/B の fixture に空白類の名前とファイル名を足し、受け入れ条件 6 にファイル名の例、並べ替えキーのテスト、散文と計器の名乗り、テストの form feed を直した。`derive_entry_lowers` を旧い写像へ戻す変異が落ちることを主が自分で測った。報告は `workspace/review-1309-r1.md`・`verify-1309-r1.json`
- [x] 2 巡目（d9815054）: 道具 green（A・E・F）。前巡で生存した 5 変異はすべて検知。Critical / High 0・Low 2（計器の文言が条件より狭い原因を断定・2 つの腕の指示の食い違い）・⚠ 1（かな 2 か所を同時に戻す変異が生存）。全件対処——文言を直し、`kana_search_matches_entry_with_fullwidth_space` を足して同時の変異で落ちることを主が測った。報告は `workspace/review-1309-r2.md`・`verify-1309-r2.json`
- [x] 3 巡目（a3d85154）: 道具 green（A・E・F）。M5ce は新しいテストの assert で検知。新しいテストはかなの経路で成り立つ（直接一致では不成立）ことを委譲が実測。Critical / High / Low 0。残る ⚠ 2 件（`search_allocations` はクエリ側の借用の退行を見ない——`query.rs` の単体テストが捕まえる・計器が現行より新しい版も「旧版」と名乗る）はどちらも変更前からの性質で製品の挙動に関わらないため対処しない。報告は `workspace/review-1309-r3.md`・`verify-1309-r3.json`

## 未確定（実装前に潰す）

- [x] **v7 の `index.bin` を読んだときの扱い** — **B: v7 を cache-miss として扱い、1 回だけ全走査して v8 で書く**に決めた（ユーザー裁定）。木から導き直す案（A・C）は `derive_columns(entries: Vec<AppEntry>)` が `AppEntry` の列を要るため、木から作り直すと `snotra-core/CLAUDE.md`「indexer.rs の索引更新の契機」が戒める複製（v7 の枝の昇格）が復活するので却下。派生の列だけ導き直して書き戻さない案は、明示的な再構築をしない利用者は毎起動 Wave 1 を払い続け、同節が挙げる「新形式の削減を永久に受け取らない」の形になるので却下。v2〜v6 の枝は `upgrade_legacy_cache_in` が `entries` から導き直して現行版で書くので、新しい規則が自然に当たる。**費用は、更新後の初回起動で 1 回だけの全走査**（大きな環境で 22〜30 秒）。直前の版を受け取る側の 3 項は「v7 は cache-miss になる」を確かめる形で置く
- [x] **かなの派生に空白の正規化を当てるか** — 当てる。クエリ側は `query_plan.rs` で `to_kana(norm_query)`（正規化済みのクエリからかなを作る）なので、名前側も `to_kana(normalize_for_match(name))` にして両側を揃える

## plan-review 結果

- リスク: 高（永続形式の版・複数モジュールのインターフェース・正規化の契約）
- レビュー方式: 独立導出 1 体（Step 2b。ユーザー裁定は #1309 のコメント 2 件に書いてから渡した）
- エージェント数: 1（全文は `workspace/plan-review-1309-derive.md`）

### 要対処（計画を修正した）
- 導出 ∖ plan: 散文の直しの一覧（今すでに偽のもの 4・変更で偽になるもの 10 前後・`snotra-core/CLAUDE.md` 2 か所）。**ガバナンス検査は 1 件も捕まえない**——手の一覧と、終わったあとの `git grep` を計画へ足した
- 導出 ∖ plan: 検知器の形（参照実装と一致・借用で返る条件・冪等の 2 段・履歴のキー）。テスト方針へ反映した
- 導出 ∖ plan: `GOLDEN_V7` を本物の凍結バイト列として残し、v8 の golden を別に置く。v7 の線上表現は同じなので旧版の構造体は要らない
- 判断の不一致: v7 の扱い——導出は「木から導き直して v8 で書く（全走査なし・1 回だけのメモリの山）」を推奨した。**ユーザーに諮り、B（cache-miss・1 回の全走査）に決まった**（2026-10-31）。規範（`snotra-core/CLAUDE.md` の v7 の昇格の戒め）を変えず、置き換えは 1 回きりなので単純さを取った

### 軽微（採らなかった・理由つき）
- 導出: フォルダの並べ替えキーは折りたたみだけに留め、trim・空白の圧縮は当てない（今の並び順を保つため）——**採らない**。並べ替えキーだけに別の関数を残すと、入口が 2 つに戻る。並び順が変わるのは空白類を含む名前だけで、整列の意味（名前順）は保たれる
- ⚠ 導出: v2〜v6 の `Skip` の腕は旧い意味の列を返し続ける（コーパスのテストだけが使う）——実装のときに確かめ、製品の経路に届くなら直す。作業項目へ足した

### 判断
- 実装着手: 人間の承認待ち

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー1体
- エージェント数: 1（ほかに敵対的調査 1 体）
- 要対処: 4 件を計画へ反映
- 未検証: 折りたたみのどの順で冪等になるか（検知器の Red→Green で決める）・U+3000 を含む名前の実データでの件数（実機の索引には 0 件）
- 散文の全件列挙: 語 5 / 総件数 186 行 / ★未掲載 41（計画へ足した 4）
- 試行（#1300・高リスクのみ・独立レビュー 1 回ごとに 1 行）: 方式 Step 2b / 設計変更 1（v7 の扱いの選択肢 A を示した。ユーザー裁定で採らず） / 主の見落とし 3（散文の直しの一覧・検知器の参照実装の形・v7 の凍結バイト列の扱い）

## 人間レビュー

- [x] 承認済み — 2026-10-31 / 問い: "この計画を承認いただけますか？" / 回答: "Ok"
