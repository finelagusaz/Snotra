# 計画レビュー — #1070 incremental cache 収集停止（観点1: 状態遷移 / 観点2: 検知器）

対象 issue: **#1070**（`c:\` の裾を削るため、`local_matches.push(i)` をパスクエリで止める）
対象計画: `workspace/plan.md`（Phase 1〜2）
製品コードは一切変更していない（読むだけ）。

---

## 要対処

### 1. 変異 (c)「`can_reuse` 側だけ元へ戻す」を捕まえられるのは Phase 2 のテスト #3 だけであり、
   その実装が特定の形を持たない限り捕まえられない

**論証**: `can_reuse` の `!plan.has_path_sep` ガード（`search.rs:215`）は「**今回の**クエリが
パス区切りを含むなら再利用そのものを禁じる」判定であり、直前クエリで収集が止まったかどうか
（write 側）とは独立な、別の安全弁である。この 2 つの安全弁は正常系ではほぼ常に**重複**して
効く——たとえば `path_match_incremental_cache_monotonic`（`search/tests/path.rs:87-114`）は
`"tool\\"` → `"tool\\ed"` という 2 回とも path クエリの遷移で、2 回目の `can_reuse` は
`starts_with` 判定より前に**自分自身の** `has_path_sep` で必ず落ちる。**変異 (c) の効果が
観測できるのは「直前クエリが非パス（収集された）で、今回クエリがパス、かつ今回の
`norm_query` が直前の `norm_query` を文字どおり prefix として持つ」という 1 点に絞られた
遷移だけである**（`search.rs:204-217` の `can_reuse` の他条件——`prev_mode` 一致・
`!prev_candidates.is_empty()`・`starts_with`・dot/kana 単調性——を全部満たしたうえで
`has_path_sep` だけが分岐点になる必要があるため）。

- Phase 2 テスト #1（`path_query_leaves_the_incremental_candidates_empty`）と #2
  （`non_path_query_still_populates_the_incremental_candidates`）は**単発クエリの write 結果**
  しか見ないため、`can_reuse`（read 側）のロジックを一切通らない。変異 (c) はこの 2 本を
  1 本も動かさない
- テスト #3（`path_query_results_are_identical_to_a_fresh_engine`）だけが read 側を通るが、
  plan.md の記述（「打鍵列（区切り無し → パスクエリ → その拡張）」）は上の 3 条件
  （prefix・非空・型一致）を**明示的に要求していない**。たとえば `"notepad"` → `"c:\\"` の
  ような、意味的には自然だが `starts_with` が成立しない列を選ぶと、`can_reuse` は
  `has_path_sep` 変異の有無に関わらず `starts_with` で既に落ちるため、**変異 (c) を当てても
  テストは緑のままになりうる**（実測ではなく構造からの推論——テストが未実装のため）
- 差を生む具体例を示す: fixture に `name="app"`（`c` を含まない）・`target_path` が `c:\...`
  の下にあるエントリを 1 件用意し、クエリ列を `"c"`（非パス・`has_path_sep=false`）→
  `"c:\\"`（パス・`norm_query` が `"c"` を文字どおり prefix として含む）とすれば、
  `can_reuse` の他条件は全部満たされ、`has_path_sep` ガードだけが差を分ける。この形なら
  変異 (c) で `"c:\\"` の結果からこのエントリが**false negative で欠落**し、fresh engine との
  比較が落ちる

**結論**: 計画の「3 変異すべてを 3 本で捕まえる」という主張は、**テスト #3 がこの特定の
prefix 関係とマッチ非対称性を持つ fixture で書かれることに完全に依存する**。plan.md の
現在の記述はそこまで踏み込んでいないので、実装前に明記すべきである
（`/symmetric-check` Step 2c を named path つきで通す形に落とす）。

### 2. 「`has_path_sep` と `norm_query_has_path_sep` の取り違えは挙動テストでは原理的に
   検出できない」は、前提条件を欠いた全称主張であり、`AGENTS.md`「全称表現は前提条件と
   セットで書く」に抵触する

**論証**: この主張が成り立つ理由は、`\` `/` `¥` の 3 文字が `nucleo_matcher::chars::normalize`
（`query.rs:9` で import）の変換対象に入っていない——**現在のバージョンでは**——という
外部クレートの実装依存の事実 1 点だけである。この事実自体は既存コード
（`search/scoring.rs:365-368`）が明記している:

> 「今の nucleo では `¥` は畳まれず 2 述語は外延的に一致するので、この条件だけは変異を
> 当てても落ちる検知器を置けない」

**この既存コメントは「今の nucleo では」と明示的に条件づけている**のに対し、plan.md
（50-54 行目）は「原理的に検出できない」と無条件の全称表現へ強めている。両者は同じ事実を
指しているが、強さが違う。`nucleo_matcher` が将来 `¥`（U+00A5・YEN SIGN）に normalize
テーブルのエントリを持つ版へ上がれば、2 つの述語は現実の入力で乖離しうる——そのとき初めて
「取り違えても結果は変わらない」という安全性の論証全体（Phase 1 の 2 番目の bullet、
plan.md 50-54 行目）が壊れる。**この場合に何が起きるかも軽微ではない**: 収集述語が
`norm_query_has_path_sep` を使うよう取り違えられていると、`has_path_sep=true` かつ
`norm_query_has_path_sep=false` の入力（¥ が畳まれるようになった世界での ¥ 入りクエリ）で
**収集が起きてしまい**、そのクエリは `path_query`（`query_plan.rs:104` で `has_path_sep` 時のみ
`Some`）を伴うパスマッチのフォールバックを経由しているため、`local_matches` には
**名前ベースではマッチしない、パス部分一致だけで拾われたエントリ**が混入しうる。次の
非パスクエリがこれを prefix 拡張として再利用すると、名前ベースのスコアリングでは
本来ヒットするはずのエントリが**候補集合の外**にいることになり、false negative が起こりうる
（全件走査が常に上位集合という安全性の論証は、収集された集合が「名前ベースでのみ汚染されて
いない」という前提に依存しており、その前提は `has_path_sep` を正しく使うときにしか
成り立たない——`query_plan.rs:104-118` で `path_query` が `Some` になるのは `has_path_sep`
のときだけである、という構造上の理由から）。

**裁定**: plan.md の主張は**現在の nucleo_matcher の挙動に限れば正しい**が、「原理的に」という
語は誤り。正しい書き方は「**現在の `nucleo_matcher::chars::normalize` が `\` `/` `¥` を
変化させないことに依存する**」であるべきで、この前提が崩れたときの挙動（上記の false
negative の機序）も当該関数の doc に一言残すことを推奨する。検出できるテストとしては、
「任意の（少なくとも fixture が持つ範囲の）クエリ文字列について `has_path_sep ==
norm_query_has_path_sep` が成り立つ」という**外延一致そのものを pin する**プロパティテストが
書ける——これは取り違えの直接検知器ではないが、安全性の論証が依存する前提の破れを
別途検知できる。

---

## 軽微

### 3. `prev_candidates` が「非空のまま古い候補を運ぶ」経路が 1 つだけ存在する
   （既存コード・本計画は触れない・害はない）

`search_with_options` の早期 return（`search.rs:278-280`）:

```
let Some(plan) = prepare_query_plan(query, mode, &options) else {
    return Vec::new();
};
```

`norm_query` が空になるクエリ（空文字・空白のみ）ではここで即 return し、
`IncrementalCache::update` を含む incremental cache への一切の read/write を通らない。
ゆえにユーザーが「有効なクエリ → 空クエリ → 有効なクエリ」と打った場合、2 回目の有効クエリの
`can_reuse` は**1 回前ではなく 2 回前**の `prev_query` / `prev_candidates` と照合される。

**これは本計画が触れる箇所ではなく、既存の挙動である。** 安全性も既存のまま壊れない
——空クエリを挟んでも、その間に索引側のデータは変わらないので、「スキップされた空クエリが
本来課すべきだった追加の絞り込み」は存在せず、2 回前の候補集合をそのまま基準にしても
上位集合の論証は保たれる。**新しいリスクではないので要対処ではないが**、観点 1 が要求する
「`prev_candidates` が空でないまま古い候補を運ぶ経路」の唯一の答えとして file:line で
記録しておく。

### 4. 共有述語関数は「write 側（直前クエリの評価）」と「read 側（今回クエリの評価）」という
   異なる時制で 2 回呼ばれる設計になる

Phase 1 が導入する `caches_candidates(plan)` は、write 側では「**たった今処理を終えた**
クエリの結果を次回のために保存してよいか」を判定し、read 側（`can_reuse` 経由）では
「**これから処理する**クエリが保存済みの候補を再利用してよいか」を判定する——**異なる
`QueryPlan` インスタンス**（違う検索呼び出しに属する）に対して同じ関数を 2 回呼ぶ形になる。
意味論としては対称（`has_path_sep` なクエリは書く側にも読む側にも参加できない）なので
不具合ではないが、関数の doc に「両方の呼び出し時制で何を評価しているか」を一言書いておくと、
将来この関数を変更する人が read/write の非対称性を見落としにくくなる。

---

## 未検証

- **Phase 2 のテスト #3 の実際のフィクスチャ設計**（要対処 1 で指摘した prefix 関係・
  名前ではマッチしないがパスではマッチするエントリの有無）は、テストがまだ書かれていない
  ため検証できない。実装時にこの設計になっているかを確認すること
- **`nucleo_matcher::chars::normalize` が本当に `\` `/` `¥` の 3 文字すべてに対して
  恒等写像であること**は、`search/scoring.rs:365-368` のコメント（#1057 で確認済みとされる）
  を根拠として採用したが、本レビューでは `nucleo_matcher` のソース・テーブルを一次資料として
  直接確認していない

---

## 観点別の直接回答（要対処・軽微の節と重複する部分は参照のみ）

### 観点 1

- **`prev_candidates` が空でないまま古い候補を運ぶ経路**: `search.rs:278-280`
  （早期 return で `update` を通らない）の 1 つだけ。既存挙動・無害（軽微 3）
- **全件走査が上位集合にならない経路**: 無い。`search.rs:291` の
  `(0..self.entries.len()).collect()` は定義上エントリ全件であり、`score_one_entry` が
  棄却しない限りどんな一致経路（name/file_name/kana/path）で見つかるマッチも取りこぼさない
- **`can_reuse` 以外に `prev_candidates` を読む経路**: `take_candidates`
  （`search.rs:220-222`。呼び出しは `can_reuse` が真のときのみ・`search.rs:289`）と
  `footprint.rs:239,249` の `.capacity()` 読み（容量のみ、意味は読まない）の 2 つだけ。
  実質的な「意味を読む」経路は `can_reuse` に閉じている
- **結論が変わる既存テスト**: 無い。`search/tests/incremental.rs` の 6 本・
  `search/tests/migemo.rs` の全テストはパス区切りを含むクエリを 1 つも使わない（grep で確認）。
  `search/tests/path.rs` の複数回 `search()` を呼ぶテスト
  （`path_match_incremental_cache_monotonic` / `path_match_incremental_disabled_avoids_accent_false_negative` /
  `sorted_prefix_fast_path_changes_nothing_over_real_index` /
  `skipping_name_scoring_changes_nothing_over_real_index`）は手でクエリ列を追跡し、
  いずれも「収集を止めても次のクエリの `can_reuse` が別の条件
  （`has_path_sep` 自身・`starts_with`・`!is_empty()`）で独立にブロックされる」ため
  結果が変わらないことを確認した

### 観点 2

- 変異 (a) 述語の反転: テスト #1・#2 の両方が独立に捕まえる
- 変異 (b) 収集を無条件に止める: テスト #2 だけが捕まえる（#1 は元々 path クエリで
  空を期待するテストなので、無条件停止でも同じ結果になり検知できない）
- 変異 (c) `can_reuse` 側だけ元へ戻す: **テスト #3 だけが捕まえうるが、要対処 1 の条件を
  満たす形で書かれていなければ捕まえない**
- 「取り違えは挙動テストでは原理的に検出できない」の裁定: **条件付きで正しい**
  （現在の `nucleo_matcher::chars::normalize` の挙動に依存）。「原理的に」という無条件の
  強さは誤り。詳細は要対処 2

---

## 参照した一次資料（file:line）

- `snotra-core/src/search.rs:191-238`（`IncrementalCache` 全体）
- `snotra-core/src/search.rs:264-351`（`search_with_options`）
- `snotra-core/src/search/query_plan.rs:24-139`（`QueryPlan` / `prepare_query_plan`）
- `snotra-core/src/search/scoring.rs:320-495`（`score_one_entry`）
- `snotra-core/src/search/footprint.rs:233-256`（`IncrementalCache::footprint_row`）
- `snotra-core/src/search/tests/incremental.rs`（全文）
- `snotra-core/src/search/tests/path.rs:1-615`（該当テスト抜粋）
- `snotra-core/src/search/tests/migemo.rs`（grep で path-sep 不在を確認）
