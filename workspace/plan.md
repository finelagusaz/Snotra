# 計画: #1394 パスマッチの照合に Unicode の合成（NFC）を足す

## 判定

**仕様変更**である。`SPEC.md` §4.2 は「パスマッチング（§4.1）は、パス側もクエリ側もこの正規化を通らない。そのため NFC と NFD の違いは揃わない」と、いまの不一致を仕様として書いている。順序は SPEC → コード → 文書。

## 目的と受け入れ条件

パスマッチで、NFC のクエリが NFD で保存されたパスに一致する（逆も）。識別用のパスキー（`normalize_entry_key`）と永続形式は変えない。

- AC1: NFD のパス `C:\tools\か\u{3099}ーむ\app.exe` に、NFC のクエリ `が\` / `がーむ\app` がパスマッチで一致する
- AC2: NFC のパスに NFD のクエリ（`か\u{3099}ーむ\`）が一致する
- AC3（意図した挙動変化のピン止め）: NFD のパス `…\か\u{3099}ーむ\…` に、クエリ `\か` で始まり `か` の直後で終わる部分（例 `tools\か`）が**一致しなくなる**（名前の照合 #1285 と同じ扱い）
- AC4: 履歴ブーストは NFD のパスでも従来どおり効く（識別用キーで照合し続ける）
- AC5: `normalize_entry_key` の出力・`index.bin` / `history.bin` の形式は 1 バイトも変わらない（既存のバイト一致の検知器が緑のまま）
- AC6: パスクエリの全件走査で、非 ASCII・NFD のエントリが居てもエントリ数に比例する確保をしない
- AC7: パスクエリ 1 打鍵の費用の増分が、ベースラインから許容幅に収まる（下の「性能の測り方」）

## 設計

照合用の合成は 1 関数に閉じ、パスとクエリの両辺がそれを通す（「記録側と照合側は同じ関数を通す」の照合版）。

- `snotra-core/src/query.rs` に `pub(crate) fn compose_for_path_match<'a>(buf: &'a mut String, s: &'a str) -> &'a str` を足す
  - `s.is_ascii()` なら `s` を返す（NFC は ASCII で恒等）
  - `NFC.is_normalized(s)` なら `s` を返す
  - それ以外は `buf.clear()` → `NFC.normalize_to(s, buf)` → `buf` を返す（確保済みバッファへ書き、ウォームアップ後は確保しない）
  - **「クエリが ASCII だから合成を飛ばす」は書かない**: パス側の判定はパス側のバイト列で行う（NFC の singleton 分解で、非 ASCII が ASCII になる字がある。例 U+037E → `;`。パスに U+037E が居れば、ASCII の `;` を含むクエリと一致させるにはパス側を合成する必要がある）
- クエリ側: `search/query_plan.rs` の `path_query` を、いまの組み立て（trim → `/` `¥` → `\` → 小文字化）の後に `compose_for_path_match` へ通した値にする（所有の `String` に収める）
- パス側: `search/scoring.rs` の `score_one_entry` のパスマッチ（`plan.path_finder.find(key)`）の直前でだけ、`key` を `compose_for_path_match` へ通す。合成先は**新しい thread_local のバッファ**（`with_cursor` の `f` の中で `CURSOR` を借り直すと panic するため、別のセル）。**履歴照合 3 種は合成前の `key`（識別用）をそのまま使う**
- `path_store.rs`・`indexer/keys.rs`・`history.rs` は触らない（それらの契約は `normalize_entry_key` とのバイト一致であり、そこへ合成を入れると識別用キーが変わる）

両辺の写像は f = NFC ∘（小文字化・区切り統一）である。canonically equivalent な入力で f が一致することは、全コードポイントの検知器で測る（記憶で断定しない）。

`/persistence-check` に該当しない理由: `with_normalized_key` の出力（識別用キー）と、その記録・移行の経路を変えない。永続化されるのは識別用キーだけである。

## 変更するファイルと対象シンボル

| ファイル | シンボル | 変更 |
|---|---|---|
| `SPEC.md` | §4.1 パスマッチングの箇条・§4.2 の 126 行目 | パスマッチが通す正規化（trim・小文字化・`/` `¥` → `\`・NFC）と通さないもの（アクセントの折りたたみ・空白の統一と圧縮）を書き分ける |
| `snotra-core/src/query.rs` | `compose_for_path_match`（新設）・`//!` | 関数を足す。`//!` に一言 |
| `snotra-core/src/search/query_plan.rs` | `prepare_query_plan` の `path_query`・`QueryPlan::path_query` の doc・112〜116 行目のコメント | 合成を足し、コメントを直す |
| `snotra-core/src/search/scoring.rs` | `score_one_entry` のパスマッチ・511 行目と 528〜530 行目のコメント・thread_local の新設 | 照合の直前だけ合成。「パスマッチと履歴照合 3 種が同じ結果を見る」を直す |
| `snotra-core/src/search/path_store.rs` | `//!` 26 行目 | 「履歴照合とパスマッチが使う」に、パスマッチは照合の直前で合成することを添える |
| `snotra-core/CLAUDE.md` | 「`normalize_entry_key` の冪等性契約」末尾の `path_query` の文 | NFC だけは照合用の関数を共有する形へ |
| `docs/architecture.md` | パスクエリの段と単価の表（277 行目付近「パスマッチ（部分文字列探索）」） | 「照合用の合成」の段（非 ASCII で NFC でないキーだけ・O(フルパス長)）を足す |
| `PERFORMANCE.md` | 「パスクエリ全走査のコスト — `normalized_keys` を保持するか導出するか」 | 合成の費用（2 面の実測）を足す |
| `snotra-core/src/query.rs` の `mod tests` / `snotra-core/src/search/tests/path.rs` / `snotra-core/tests/search_allocations.rs` | テスト | 下の「テスト方針」 |

## 実装の順序

1. SPEC.md を直す
2. 失敗するテストを書く（AC1〜AC3・全コードポイント検知器）→ Red を確かめる
3. `compose_for_path_match` を足し、クエリ側・パス側へ当てる → Green
4. 確保のテスト（AC6）と性能の計測（AC7）
5. コメント・`snotra-core/CLAUDE.md` を直す

## 不変条件と異常系

- 識別用キー（`with_normalized_key` が貸す `key`）は変えない。履歴照合はそれだけを使う — 検知: 既存の `path_store_cursor_matches_*`・`derives_same_bytes_as_normalize_entry_key`・`migrate_normalize_keys_is_idempotent` と、新しい AC4 のテスト
- 両辺は同じ関数を通す — 検知: 全コードポイントの検知器（f(NFD(c)) == f(c)）
- `with_cursor` の `f` の中で `CURSOR` を借り直さない（panic）— 新しいバッファは別の thread_local
- 合成が失敗する経路は無い（`normalize_to` の `fmt::Write` は `String` で失敗しない。戻り値は捨てずに `debug_assert` か `expect` にせず、`let _ =` で icu 自身と同じ扱いにする）

## テスト方針と検証コマンド

- `query.rs` の `mod tests`
  - 全コードポイントの検知器: canonical 分解を持つすべての `c` について、f(c) と f(NFD(c)) が一致する（f = 小文字化 → `compose_for_path_match`）。2 字の列（基底 + 結合文字）も、既存の `normalize_for_match_is_idempotent_on_composing_pairs` の母集団の作り方を流用して当てる
  - ASCII と NFC 済みの入力で、入力をそのまま返す（バッファを使わない）
- `search/tests/path.rs`: AC1・AC2・AC3・AC4（NFD のパスに履歴を積み、ブーストが乗ること）
- `tests/search_allocations.rs`: `path_query_full_scan_does_not_allocate_per_entry` の兄弟として、パスに NFD の成分を持つ fixture と非 ASCII のクエリで、エントリ数に比例する確保をしないこと（AC6）
- 変異注入（`/implement` の委譲先が行う）: パス側の合成を外す → AC1 が落ちる／クエリ側の合成を外す → AC2 が落ちる／履歴照合に合成後のキーを渡す → AC4 は落ちない可能性があるので、NFD のパスで履歴キーを識別用キーで記録したテストが落ちることを確かめる
- 検証: `docs/build-commands.md`「変更後の検証チェックリスト」の該当カテゴリ（Rust・doc コメントを触るので `cargo doc` を含む）、`npm run governance:check`
### 性能の測り方

開発機の実 `index.bin` は v7（現行は v10）で、`path_query_cost.rs` の運用点の計測は走らない（research.md F2）。実ファイルを書き換える rescan はしない。合成の索引で 2 面を測る。

1. **ASCII の面（増える費用の主項 = 全キーへの `is_ascii` の 1 パス）**: 既存の `search/tests/performance.rs` の `bench_fuzzy_path_query_scaling`（`#[ignore]`）。ベースライン（main・2026-11-05・同機で 3 回）の 300,000 件: 1969 / 1780 / 1714 µs（中央値 1780 µs）、100,000 件: 824 / 758 / 743 µs。合格: 実装後 3 回の中央値が 300,000 件で 1960 µs（+10%）以下
2. **非 ASCII の面（最悪ケース = 全キーが非 ASCII で `is_normalized` を払う）**: 既存のベンチはどれも `target_path` が ASCII（`make_bench_entries_katakana` も `C:\Program Files\App{i}\…`）で、この分岐を通らない。そこで、`target_path` に NFC の日本語の成分を持つ 300,000 件（うち 0.1% を NFD）でパスクエリを測る**一時的なベンチ**を `performance.rs` に足して実装前後で測る。合格: 増分を記録し、+50% 以下（実データの非 ASCII は 2.62%・F4 なので、運用点への寄与は 2% 未満に収まる）。**撤去**: 測り終えたらコミットの前に消す（main に残さない。数値は PR 本文に残す）

逃げ道（1 が不合格のとき）: `PathCursor` の `push_segment` がすでにセグメントごとに `is_ascii` を判定しているので、カーソルに「非 ASCII を書いたか」のフラグを持たせ、キー全体の再走査を消す。

## SPEC.md・関連文書の更新

- `SPEC.md` §4.1・§4.2（上表）
- `snotra-core/CLAUDE.md`「`normalize_entry_key` の冪等性契約」
- `PERFORMANCE.md`: パスクエリ全走査の節（「パスクエリ全走査のコスト — `normalized_keys` を保持するか導出するか」）へ、照合用の合成の費用（2 面の実測）を足す

## 作業項目

### Phase 0 — ベースライン

- [x] 一時的な非 ASCII のベンチを足し、実装前の値を 3 回測って記録する — `temp_bench_non_ascii_path_query`（300,000 件・全キー非 ASCII・0.1% NFD）: 8101 / 7753 / 8190 µs（中央値 8101 µs）

### Phase 1 — 仕様

- [x] `SPEC.md` §4.1・§4.2 を直す

### Phase 2 — Red

- [x] 全コードポイントの検知器と 2 字の列の検知器を書く
- [x] `search/tests/path.rs` に AC1〜AC4 のテストを書き、AC1・AC2・AC3 が Red であることを確かめる

### Phase 3 — Green

- [x] `query.rs` に `compose_for_path_match` を足す
- [x] `query_plan.rs` の `path_query` に当てる
- [x] `scoring.rs` のパスマッチの直前に当てる（新しい thread_local のバッファ）
- [x] 確保のテスト（AC6）を足す

### Phase 4 — 検証と文書

- [x] 性能を実装後に 2 面とも測り、ベースラインと比べる（AC7）。値を `PERFORMANCE.md` へ書く
- [x] 一時的な非 ASCII のベンチを消す
- [x] コメント（`query_plan.rs`・`scoring.rs`・`path_store.rs` の `//!`）と `snotra-core/CLAUDE.md`・`docs/architecture.md` を直す
- [x] 変更後の検証チェックリストの該当カテゴリと `governance:check` を通す
  - ラウンド 1: sha 50f48af5 / verify:tree green（A・E・F）/ 指摘 8 件（Medium 2・Low 6、Critical/High 0）/ 変異 9 個中 生存 1（M7: クエリが ASCII ならパス側の合成を飛ばす）
  - ラウンド 2: sha 454aa9de / verify:tree green（A・E・F）/ 前回指摘 6 件解消・新規指摘 0 / 追加変異 2 個中 生存 1（M10: NFC を NFKC に替える）→ `path_match_does_not_apply_compatibility_decomposition` を追加（主ツリーで M10 を注入し assert で赤・ハッシュ照合で巻き戻し）
  - ラウンド 3: sha 0289cdbf / verify:tree green（A・E・F）/ 新規指摘 0（⚠️ Low 1: ループの早期停止で `game` の行単独の検知は未測定。全角英字の互換分解は Unicode の仕様どおりなので採らない）/ M10 を assert で検知・生存 0
- [x] レビュー指摘への対処: M7 の検知器 `path_match_composes_the_path_even_for_an_ascii_query` を追加（M7 を主ツリーで注入して assert で赤・ハッシュ照合で巻き戻し）、SPEC §4.1 の区切り統一をクエリとパスで書き分け、§4.2 に前後空白の除去を足す、`compose_for_path_match` の doc の言い過ぎ・検知器の doc の「製品の経路そのもの」を直す、`query.rs` の use 群をまとめる。採らない: `path_query` の `.to_owned()`（検索 1 回 1 度で費用は無視できる）、glossary への追加（「識別用のキー」「照合用の合成」は標準の語の組み合わせで、`docs/comment-guidelines.md`「言語（日英）」の対象になる造語ではない）

## 未確定（実装前に潰す）

- [x] 実データの NFD を含むパスの件数と、非 ASCII パスの割合 — 敵対的調査が stale な v7 の `index.bin` をリポジトリ外のコピーで展開して測った: 16,841 件中、非 ASCII 441 件（2.62%）、NFD 7 件（research.md F4）。運用点の規模は測れない（F2）
- [x] UI がパスマッチの一致位置（`pos`）を使っていないか — 使わない（`SearchResult` に載らない・research.md 壊せなかった項目）
- [x] 性能のベースラインと許容幅 — 実 `index.bin` が v7 で運用点を測れないため、合成の索引の 2 面に移した（上の「性能の測り方」）。ASCII の面のベースラインは測定済み。非 ASCII の面は一時ベンチが要るので Phase 0 の作業項目にした

## 射程外（報告のみ）

- `opener.rs` の `find_matching_tools` も、ルールのパス条件と対象パスを NFC を通さずに `starts_with` で比べる（research.md F1）。別 issue の候補
- 開発機の実 `index.bin` が v7 のため、実インデックスの corpus 検知器がこの機で黙ってスキップしている（research.md F2）。検知器の健全性の問題であり、本 issue では扱わない

## 人間レビュー

- [x] 承認済み — 2026-11-05 / 問い: "`workspace/plan.md` に注釈を書き込むか、このまま承認するかをお選びくださいませ。" / 回答: "ok /implement"

## セルフレビュー

- リスク: 通常（永続形式・識別用キー・公開 API・状態遷移・並行性・ガバナンス文書に触れない。新しい関数は `pub(crate)` の純関数で、既存の `query.rs` の正規化群に 1 本足すだけ。thread_local は worker ごとの作業バッファで共有状態ではない）
- plan-review: 未実施（通常リスク）
- エージェント数: 0（3b の敵対的調査 1 体は別）
- 要対処: 2 件を反映 — (1) 5a の照合で「クエリが ASCII なら合成を飛ばす」近道が誤りになりうること（U+037E → `;`）を設計に禁止として明記 (2) `prose:sweep` の ★未掲載 から `docs/architecture.md` の段と単価の表を変更ファイルへ追加
- 未検証: 運用点（実 `index.bin`）での費用。開発機の索引が v7 のため測れない（research.md F2）。合成の索引の 2 面で代える
- 散文の全件列挙: 語 3（`path_query`・パスマッチ・NFC）/ 総件数 115 行 / ★未掲載 17（計画へ足した 1: `docs/architecture.md`。残り 16 は名前の照合の NFC〈#1285〉・`normalize_entry_key` の写し・「norm_query と path_query で正規化が異なる」〈変更後も真: `path_query` はアクセントと空白を保つ〉・凍結された ADR / spec で、変更で偽にならない）
