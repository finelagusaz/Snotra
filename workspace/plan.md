# plan — #1070 パスクエリで再利用されえない全一致 index 収集をやめる

**根拠となる測定は `workspace/research.md` §1（baseline）と §2（ablation）である。** ここには数字を
最小限しか写さない。

## 目的

パスクエリ（`has_path_sep`）で毎打鍵 312,208 件ぶん組み立てている incremental cache 用の
全一致 index（`local_matches` → `all_match_indices` → `IncrementalCache::update`）は、
**読み手が `IncrementalCache::can_reuse` の 1 つしか無く、その述語が `!has_path_sep` を要求する。**
**現在の正規化器（`nucleo-matcher` 0.3.1）の下では、集めた候補を読む打鍵列を構成できない**
（敵対的調査 命題 1 が反証を試みて到達しなかった）。これを止めて裾を削る。

**ただし安全性はこの事実に立脚させない**——万一将来これが偽になっても、帰結は全件走査へ
倒れることだけで**結果は変わらない**（下の「『結果が変わらない』の論証」）。

**射程は「マッチ件数が索引全件に及ぶクエリ」であり、`c:\` という字面ではない。** 条件は
`plan.has_path_sep` のみで、ドライブレターにも scan 根の形にも依存しない（research.md の設計制約）。

## 受け入れ条件

**#1067 から引き継いだ `max < 16,700` は、本サイクルで検討・実測した手段では到達できず、
しかも結果を破壊する天井（X2）ですら 3 回中 1 回は超えた**（research.md §2）ため、
**2026-08-13 のユーザー決定で p90 へ改める。**

1. **実運用点で `c:\` の p90 が 16,700 µs を下回る**——同一機・同一セッションの **3 回とも**。
   計器は `measure_path_query_frame_cost_at_operating_point`（**1 バイトも変更しない**）
2. **max は 1 フレームを超えたままでよい。** ただし**実測値と、届かなかった理由（検討した 3 手が
   成立しないこと・未測定で残した候補・測定分散そのものの大きさ）を `PERFORMANCE.md` と ADR へ
   残す**（残したことが受け入れ条件である）
3. **対照が退行しない**——`\zzz-no-such-path\`（0 件）/ `\program files\` / `c:\users` /
   `c:\users\` / `users`（区切り無し）。0 件行と区切り無し行は**変わらないことが正しい**
4. **検索結果が集合・順序とも 1 件も変わらない**——PATH マージを通した実 index 全件で
   突き合わせる（型は `search/tests/path.rs` の `sorted_prefix_fast_path_changes_nothing_over_real_index`）
5. **既存の incremental search が退行しない**——区切りを含まないクエリでは今までどおり収集し再利用する

## 変更ファイル一覧と対象シンボル

| ファイル | 対象シンボル | 変更 |
|---|---|---|
| `snotra-core/src/search.rs` | `IncrementalCache`（新: 述語の関連関数）/ `can_reuse` / `search_with_options` の fold | 収集の可否を述語 1 本に一元化し、read と write の両方をそこへ通す |
| `snotra-core/src/search/tests/incremental.rs` | 新規テスト 3 本 | 新しい不変条件の検知器（両方向 + 結果不変） |
| `snotra-core/CLAUDE.md` | 「incremental cache とパスクエリの非互換」節 | 「読まない」だけでなく**「書かない」**になったことを追記（数字は書かない） |
| `PERFORMANCE.md` | 新節 | baseline / ablation / 改修後の実測と、到達不能の論証（**数字の正本**） |
| `docs/adr/ADR-path-query-tail-top-k.md`（新規） | — | 否定の知識: C2 で検討した 3 手が成立しない理由と**未測定で残す候補** / C3・C6 を測って却下 / max ゲートを p90 へ改めた判断 |
| `SPEC.md` | — | **更新不要**（→ 下の「SPEC.md の更新要否」で根拠を示す） |

## 実装順序

### Phase 1 — 収集の停止（述語の一元化）

- [x] `IncrementalCache` に**収集可否の述語を 1 本置く**（例: `fn caches_candidates(plan: &QueryPlan) -> bool`）。
      `can_reuse` の `!plan.has_path_sep` もこの関数を通すよう書き換える
      ——**read と write が同じ関数を通ることだけが、片側だけ変わるドリフトを防ぐ根拠**である
      （`normalize_entry_key_into` / `measure_derived_sharing` と同じ理屈）
- [x] **`plan.has_path_sep` と `plan.norm_query_has_path_sep` の取り違えを、構造で表現不能にする**
      （`/symmetric-check` Step 2c）。2 つは同じ型の隣接フィールドで、取り違えても
      **収集をやめるクエリ集合が変わるだけで結果は変わらない**ため、**現在の正規化器の下では
      挙動テストで検出できない**。述語を関数 1 本にしてフィールド参照を 1 か所へ寄せれば、
      この誤りは書けなくなる。**この理由を当該関数の doc に書く**
  - **「原理的に検出できない」とは書かない**（plan-review 要対処 2）。前提は
    **`nucleo-matcher` 0.3.1 が `\` `/` `¥` を恒等で通すこと**であり、その前提は
    `search/scoring.rs` の当該コメント（「今の nucleo では〜検知器を置けない」）が
    **既に条件つきで記録している**。**同じ事実を 2 か所へ書かず、そこを参照する**
    （AGENTS.md「文書に事実の写しを増やす変更」）。一次証拠は当該クレートの
    `src/chars/normalize.rs`（`DATA1` の先頭が `U+00C0`・`00A5` のエントリは無い）
- [x] 述語の doc に、**write 側（今回のクエリを評価して収集するか）と read 側（今回のクエリを
      評価して再利用するか）で、同じ関数が別の時制で 2 回呼ばれる**ことを 1 行書く
      （plan-review 軽微 4）
- [x] `search_with_options` の fold で、`local_matches.push(i)` をその述語で条件づける
- [x] `can_reuse` / `update` / 当該述語の doc に、**「書かない」側の不変条件**を書く。
      安全性の論証は**正規化の詳細に依存させない形**で書く: 収集を止めると `prev_candidates` が
      空になり `can_reuse` の `!self.prev_candidates.is_empty()` が落ちて**全件走査へ倒れるだけ**であり、
      **全件走査は母集団そのもの**ゆえ結果は不変。**`¥` や `nucleo_normalize` の挙動に
      依存させてはならない**（依存させると、その挙動を永久に固定する義務が生じる）

### Phase 2 — 検知器

- [x] `path_query_leaves_the_incremental_candidates_empty`: パス区切りを含むクエリの後、
      `prev_candidates` が空である（新しい不変条件そのもの）
- [x] `non_path_query_still_populates_the_incremental_candidates`: 区切りを含まないクエリでは
      今までどおり積む（**述語の反転・無条件 off を捕まえる逆向き**）
- [x] `path_query_results_are_identical_to_a_fresh_engine`: **fixture と打鍵列を次の形に固定する**
      （plan-review 要対処 1。任意の打鍵列では変異 (c) を捕まえられない）:
  - fixture 2 件 —— (i) クエリ `c` に**名前で**マッチするエントリ、
    (ii) 名前にも file_name にも `c` を含まず、`target_path` が `c:\` 配下にあるエントリ
  - 打鍵列 —— `"c"`（区切り無し・収集される）→ `"c:\"`（パスクエリ。
    **`norm_query` が前回の `norm_query` を文字どおり prefix として含む**）
  - 期待 —— 2 打鍵目の結果が、新品の engine へ `"c:\"` を単発で打った結果と**順序込みで一致**する
    （エントリ (ii) が欠落しない）
  - **この形でなければならない理由**: `can_reuse` の他条件（`prev_mode` 一致・
    `!prev_candidates.is_empty()`・`starts_with`・dot/kana 単調性）を**全部満たしたうえで
    `has_path_sep` だけが分岐点になる**遷移でしか、read 側の変異は観測できない。
    既存の `path_match_incremental_cache_monotonic` は 2 打鍵とも パスクエリなので、
    2 回目は自分自身の `has_path_sep` で必ず落ちて**この差を見ない**
- [x] **変異注入でこの 3 本が落ちることを実測する**（AGENTS.md「検知器を置き、呼び忘れを
      再現する変異で落ちることまで確かめる」）。当てる変異と、落ちるはずの本数:
      **(a) 述語を反転する → #1 と #2** / **(b) 収集を無条件に止める → #2 のみ**
      （#1 は元から空を期待するので検知できない）/ **(c) `can_reuse` 側だけ元へ戻す → #3 のみ**

### Phase 3 — 実運用点の再測定（**交互測定**）

- [x] `docs/build-commands.md` のコマンド（`at_operating_point`）を、**同一セッションで
      A→B→A→B→A→B の順に 6 回**走らせる。**切り替えは 1 コマンドで固定する**——
      **A 側 = `git checkout main -- snotra-core/src/search.rs`** /
      **B 側 = `git checkout HEAD -- snotra-core/src/search.rs`**（Phase 1〜2 をコミット済みの前提。
      `git stash` は使わない——コミット後は戻せない）。切り替えるのは `search.rs` だけでよい
      ——計測は `--test path_query_cost` で lib を `cfg(test)` 無しで組むため、
      `search/tests/` に足したテストはバイナリに載らない。**各切替で release の再ビルド
      約 40 秒が挟まる**
- [x] **baseline を別ブロックで先に取ってはならない**——research.md §1 はそれをやった結果、
      機体の暖まりが A 側だけ有利に働き、対照行（`\program files\`）の判定ができなくなった
      （敵対的調査 命題 6）
- [x] 受け入れ条件 1（`c:\` の p90 < 16,700 が B の 3 回とも）と条件 3（対照行が退行しない）を
      **自分で読んで**判定する。**対照は同一セッションの A と比べる**（別ブロックの baseline とは比べない）。
      計器は合否を言わない（`docs/development-principles.md`「判定を持たない道具」）
- [x] **境界ケースの判定規則を先に決めておく**（X1 の p90 15,588〜15,946 はゲートまで余裕
      約 800 µs ＝ 5% しかなく、A′ ではセッション全体が同程度 drift した）:
  - **B の p90 が超えた実行があり、同一セッションの A 側 p90 も baseline の帯（17,772〜18,921）を
    超えて膨れている** → **セッション不良**。別セッションで 6 本取り直す
  - **A が帯の内側で B だけ超えた** → **不合格**。ユーザーへ差し戻す
  - **max のゲートで踏んだ「最も不安定な統計量に条件が乗る」問題の縮小版を、先回りで潰す形である**
- [x] 実 index 全件の結果不変テスト（`#[ignore]` を含む path 系）を手元で走らせる（受け入れ条件 4）

### Phase 4 — 文書

- [x] `PERFORMANCE.md` に新節（数字の正本）。**表は「同一セッション・同一機・3 回」の形で載せ、
      max の揺れ幅（A/A′ 4 回で ±20%）を併記する**——1 回の実行を運用点の記述として読ませない
- [x] `docs/adr/ADR-path-query-tail-top-k.md`（否定の知識）。**含めるもの**:
  - **max のゲートを p90 へ改めた判断**（2026-08-13 のユーザー決定）と、そう決めた根拠
    ——C1 で届かず、**結果を破壊する天井（X2）ですら 3 回中 1 回は 16,700 を超えた**
  - **却下 — C2 の 3 手**（score 下限の枝刈り / `k` の縮小 / tie-break 規則の変更）。
    **「実装形が無い」とは書かない**（全称否定・敵対的調査 命題 3）
  - **未測定で残す候補（再訪条件つき）**: (a) heap への逐次 push を**バッチ選択**
    （`select_nth_unstable_by`）へ替える、(b) `(score, last_launched)` と `lower_name` の
    先頭数バイトを 1 語へ詰めた**多段比較キー**、(c) rayon を**より細かく**分割する方向
    （C3 は粗くする方向しか測っていない）。**どれも `ScoredEntry::Ord` を変えないので
    結果は保てる形である**。再訪条件は「max を再びゲートに載せる決定が出たとき」
  - **却下 — C3（粗い分割）と C6（candidate index の `Vec` を作らない）**。C3 は
    **対照行が 1 フレームのゲート自体を超えた**（`\program files\` max 18,590）
- [x] `snotra-core/CLAUDE.md` の当該節を更新（**数字は写さず `PERFORMANCE.md` を指す**）
- [ ] **PR 本文に「ゲートは 2026-08-13 のユーザー決定で max → p90 へ改訂（根拠は ADR）」を 1 行残す。**
      issue #1070 の本文には `max < 16,700` が逐語で残るため、書かないと `/merge-pr` の時点で
      受け入れ条件が食い違って見える（PR 本文は squash で main の commit message になる）
- [x] `npm run governance:check`（`*.md` と新規 ADR を触るため・AGENTS.md 条件別チェック）

## 不変条件と異常系

| 不変条件 | 壊れたときの症状 | 検知器 |
|---|---|---|
| 収集の write と再利用の read が同じ述語を通る | 片側だけ変わると、パスクエリ後に古い候補で再利用され**結果が欠ける**（false negative） | Phase 2 の 3 本（変異注入で落ちることを実測） |
| 区切り無しクエリの incremental は不変 | 全打鍵が全件走査になり `users` が 100 µs → 4,300 µs へ転落 | `non_path_query_still_populates_...` + 計測の `users` 行 |
| 検索結果は集合・順序とも不変 | 順位だけが静かにずれる（クラッシュしない） | 実 index 全件の突き合わせ（受け入れ条件 4） |

**異常系**: `prev_candidates` が空のまま `can_reuse` が真を返す経路は無い（`!is_empty()` が述語に
入っている）。空 Vec を `update` へ渡すのは既存の「マッチ 0 件」経路と同じ状態であり、
`incremental_search_empty_prev_candidates_falls_back` が既に固定している。

**`update` が書く残り 3 フィールド（`prev_query` / `prev_mode` / `prev_kana_query`）は
条件づけない。** 止めると「`can_reuse` が read する全フィールドを `update` が書く」という
**既存の対称不変条件のほうが壊れる**（`search.rs` の `update` の doc）。候補を空にするだけで
再利用は落ちる。

**緑のままであるべき既存の検知器**（`/symmetric-check` で特定）:
`search/tests/path.rs` の `path_match_incremental_cache_monotonic` と
`path_match_incremental_disabled_avoids_accent_false_negative`。後者は「前回の空結果を
再利用すると false negative になる」という**今回触る依存そのもの**を突いている。

**副次的な利得（受け入れ条件ではない）**: 今日は `c:\` を打った後、312,208 要素の `Vec`
（約 2.4 MiB）が次の検索まで `prev_candidates` に**常駐し続ける**。収集をやめると常駐も消える
（`search/footprint.rs` の `incremental_cache` 行が 0 になる。値を assert している箇所は無い）。

### 「後で読まれることに依存していないか」の 1 行ずつの書き出し（AGENTS.md 条件別チェック）

消すのは `local_matches.push(i)` の write 1 か所である。その値の読み手を全部列挙する:

1. `reduce` の `a_matches.extend(b_matches)` — 同じ値の併合。**消える側と同じ寿命**
2. `IncrementalCache::update(_, candidates, _, _)` の第 2 引数 → `prev_candidates`
3. `prev_candidates` の読み手は **`can_reuse` の `!is_empty()` と `take_candidates()` の 2 つだけ**
   （`footprint.rs` の常駐計測は容量を数えるだけで意味を読まない）
4. `take_candidates()` は `can_reuse` が真のときにしか呼ばれない
5. ゆえに**「後で読まれる」経路は `can_reuse` 1 本に閉じており、そこは `!has_path_sep` を要求する**

**diff に現れない下流で新しく生きる行**（AGENTS.md「どの分岐が選ばれるかを決める値の出所を変更」）:
`can_reuse` が偽になる**理由**が `!has_path_sep` から `!is_empty()` へ移る。**新しく生きる分岐は
今日は 1 つも無い**——理由が移っても偽であることは変わらず、両者が食い違う入力
（生クエリに区切りがあるのに正規化後には無い＝`¥` 系）は**現在の `nucleo-matcher` 0.3.1 では
存在しない**（敵対的調査が当該クレートの `src/chars/normalize.rs` を読んで確認。
`DATA1` の先頭が `U+00C0` で `00A5` のエントリは無い）。
**この事実に安全性を立脚させない**——将来クレートが上がって食い違う入力が現れても、
そのときの帰結は「再利用が成立しうる場面で全件走査へ倒れる」だけであり、**結果は変わらない**
（下の論証）。

**「結果が変わらない」の論証**（敵対的調査 命題 2 で補強された形）:

1. **同一クエリ内**——`top_k.push(scored)` は `local_matches.push(i)` と独立に、
   スコアが `Some` の全候補へ無条件で走る。収集を止めても top-k は 1 ビットも変わらない
2. **次回呼び出し**——全件走査は候補集合の「上位集合」ではなく**母集団そのもの**であり、
   `score_one_entry` は呼び出し間状態を一切読まない純粋なエントリ単位の述語である
3. **向きが一方通行**——C1 は `can_reuse` を**真→偽の向きにしか動かさない**（候補を空にするだけ）。
   ゆえに「再利用してはならないのに再利用する」は**構造的に生じない**

**`prev_candidates` が非空のまま古い状態を運ぶ経路は 1 つだけ在る**（plan-review 軽微 3・
**既存の挙動で本変更は触れない**）: `norm_query` が空になるクエリは `search_with_options` の
早期 return（`search.rs:278-280`）で `update` を通らないので、「有効 → 空 → 有効」の列では
2 回目が 2 つ前の状態と照合される。間に索引は変わらないので上の論証は保たれる。

## テスト方針と検証コマンド

コマンド文字列の正本は `docs/build-commands.md`。ここでは**どれを走らせるか**だけを書く。

- カテゴリ A（Rust 変更）: `cargo test -p snotra-core` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo fmt --all -- --check`
- doc コメントを触るため `cargo doc --workspace --no-deps --document-private-items`（**hook は沈黙する**）
- カテゴリ F（`*.md` と新規 ADR）: `npm run governance:check`
- 手元 release の計測: `at_operating_point` を **交互に 6 回**（A→B→A→B→A→B・Phase 3）
- 実 index を使う `#[ignore]` のテスト（`search/tests/path.rs` 系）を手元で明示的に走らせる
  ——**CI では走らない**（scan パスが空なら自己スキップする）

## SPEC.md・関連文書の更新要否

- **`SPEC.md` は更新しない。** 判定は AGENTS.md の 2 参照で行う: (a) `SPEC.md` に incremental
  cache の候補収集の記述は無い（`SPEC.md` が持つのは検索の意味論と結果の並び）、(b) 本変更は
  その記述に**合わせる/変える**のどちらでもなく、**結果を 1 件も変えない**内部最適化である
- `PERFORMANCE.md` が数字の正本、ADR が否定の知識の正本、`snotra-core/CLAUDE.md` は**参照**を持つ
  （AGENTS.md「文書に事実の写しを増やす変更」——同じ数字を 2 か所へ書かない）

## 未確定（実装前に潰す）

- [x] **裾の帰属**（どの成分が max を作っているか）— research.md §2 で ablation 実測。
      C1 が −3,000〜−6,600（max）、残る支配項は top-k 挿入
- [x] **C1 だけで max のゲートに届くか** — **届かない**（18,290〜19,992）。実測で確定
- [x] **C2 に結果を保つ実装形があるか** — **検討した 3 手（score 下限の枝刈り / `k` の縮小 /
      tie-break 規則の変更）はどれも成立しない**（research.md §2）。**未探索の 2 案
      （バッチ選択・多段比較キー）は結果を保てる形でありうるが未測定**——本サイクルでは
      踏み込まず ADR へ再訪条件つきで残す（ユーザー決定で受け入れ条件が p90 へ移ったため）
- [x] **C3（task 粒度）・C6（candidate index の Vec）は効くか** — **どちらも却下**。C6 は
      X1 と区別できず、C3 は対照行が退行した（実測）
- [x] **既存テストに反転する不変条件があるか** — 無い（`search/tests/incremental.rs` の 6 本は
      パス区切りを含むクエリを 1 つも使っていない）
- [x] **max のゲートをどう扱うか（要求判断）** — 2026-08-13 のユーザー決定で **p90 へ改める**。
      追加 issue は立てない
- [x] **敵対的調査（Step 3b）の所見の採否** — research.md「敵対的調査」節へ反映済み
      （壊せた 2 件は本計画と research.md を訂正・壊せなかった 4 件は論証を補強）
- [x] **plan-review の要対処 2 件・軽微 2 件** — 本計画へ反映済み（上の「plan-review 結果」）

## plan-review 結果

- **リスク: 高**（`AGENTS.md` / `/plan-review`「リスク判定」の「状態遷移を変更する」「共有状態を変更する」に該当）
- **レビュー方式: 計画準拠レビュー 1 体**（Step 2。全文は `workspace/plan-review-incremental-collect.md`）
- **エージェント数: 2**（Step 3b の敵対的調査 1 体 + plan-review 1 体。どちらも `sonnet`）
- **追加で実行した check スキル**: `/symmetric-check`（AGENTS.md 条件別チェック「対称ペア」）。
  `/race-check` は**実行しない**——新しい共有状態・worker・channel・listener を 1 つも足さず、
  fold の状態は元から task ごとに閉じているため（判断の根拠を残す）
- **要対処: 2 件・どちらも反映済み**
  1. 変異 (c)（`can_reuse` 側だけ元へ戻す）を捕まえるには、テスト #3 の fixture と打鍵列が
     「非パス → パス」かつ `starts_with` 成立かつ「名前では当たらないがパスでは当たる」
     エントリを持つ形でなければならない → Phase 2 に**具体形を固定**した
  2. 「取り違えは**原理的に**検出できない」は前提を欠いた全称主張
     （`AGENTS.md`「全称表現は前提条件とセットで書く」に抵触）→ **前提つきの記述へ改め**、
     既に条件つきで書かれている `search/scoring.rs` のコメントを**参照**する形にした（写しを増やさない）
- **軽微: 2 件・どちらも反映済み**（空クエリの早期 return で状態が持ち越される既存経路の記録／
  述語が 2 つの時制で呼ばれることの doc）
- **未検証（レビュア側の申告）**: nucleo の恒等写像をレビュアは一次資料で確認していない
  → **敵対的調査の側が当該クレートの `src/chars/normalize.rs` を読んで確認済み**（2 体の所見が補完した）
- **未検証（残余）**: テスト #3 の fixture は実装するまで実測できない。**Phase 2 の変異注入で確かめる**

## 人間レビュー

- [x] 承認済み — 2026-08-13 / 問い: "**`workspace/plan.md` を承認いただけますでしょうか。** 注釈を加えたい箇所があればご指示くださいませ。" / 回答: "p90 へ改めてください。plan.md 承認します"
