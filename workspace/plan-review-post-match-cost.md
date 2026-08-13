# plan-review — #1067

対象計画: `workspace/plan.md`（ブランチ `chore/path-query-post-match-cost`）
観点: (1) 新計器の実運用点再現度、(2) ablation 変異の選択性

**第 2 版**: 主エージェントが `M_topk` を `score_one_entry` 内から `search.rs:317-322` の fold 呼び出し点へ
移し、`M_idx`（第 5 の成分）を追加した後の再判定。観点 1（アクセサの鎖）は第 1 版から変更されていない
ため、そのまま持ち越す。観点 2 は変更点を中心に判定し直した。

## 要対処

### 1.（持ち越し・未対応）`sorted_by_path` アクセサは「1つ」では `Engine` から辿れない

`workspace/plan.md:236-245` の未確定 1 は今回のラウンドで変更されていない。第 1 版の指摘がそのまま
有効である:

- `snotra-core/src/search/path_store.rs:119` の `sorted_by_path` は非公開フィールド。既存の
  `pub(super)` アクセサ（`cmp_paths` `path_store.rs:256`、`raw_into` `path_store.rs:216`、
  `to_path` `path_store.rs:276` 等）と同型の getter がこのフィールドには無い
- `snotra-core/src/search.rs:118` の `SearchEngine.entries: PathStore` も非公開。crate 外への
  公開には `footprint_rows`（`search/footprint.rs:106-107`）と同型の `#[doc(hidden)] pub` が要る
- `snotra-core/src/engine.rs:99` の `Engine.search_engine: SearchEngine` も非公開。`entry_count`/
  `entry_name`（`engine.rs:309-316`）が示す通り、`Engine` 経由の読み出しには passthrough が要る

`tests/memory_footprint.rs:375,397` の `footprint_rows` 先例は `SearchEngine` を直接構築して呼んで
おり `Engine` を経由しない（history/mode/boost 不要なため）。今回は `Engine::search()` を必ず経由する
設計（`workspace/plan.md:51-52`）なので、**3 ファイルに 1 つずつ、計 3 メソッドの新設**が要る。
「アクセサを 1 つ足す」という記述のままでは実装時に不足に気づいて追加判断が生じる。

### 2.（部分的に解消・残り 2 件）`-D warnings`: `M_topk` は対応済みだが `M_all` / `M_hist` は未対応

`M_topk`（`workspace/plan.md:128-133`）は変異位置を fold 呼び出し点へ移し、`let _ = scored;` で
`-D warnings` を明示的に手当てしている——**これは正しい対応であり、第 1 版の指摘は解消した。**

しかし同じ手当てが `M_all` と `M_hist` には無いまま残っている:

- **M_all**（`scoring.rs:478` の `let base_score = score?;` 直後に `return None`）: それ以降の
  全文（`scoring.rs:480-505`、履歴照合〜`Some(ScoredEntry{..})`）が到達不能になり
  `unreachable_code` が発火し、`base_score` 自体も未読のため `unused_variables` も発火する
- **M_hist**（履歴照合 3 種を定数化）: `score_one_entry` の引数 `history: &HistoryStore`
  （`scoring.rs:326`）の唯一の使用箇所が消えるため、関数引数が `unused_variables` になる

`-D warnings` は `Cargo.toml`/`.cargo/config.toml` に焼き込まれていないため `cargo test --release`
自体は妨げられないが、`.claude/hooks/post-edit.mjs`（PostToolUse）は `.rs` 編集のたびに `-D warnings`
相当を自動実行するため、この 2 変異を当てるたびに会話へ失敗が届く。`M_topk` と同じ水準の手当て
（`#[allow(unreachable_code, unused_variables)]` または `_` 接頭辞）をこの 2 つにも広げること。

### 3.（新規）`M_topk` と `M_idx` は `mut` バインディングを未使用にし `unused_mut` を出す

`search.rs:312` の fold クロージャは `|(mut top_k, mut local_matches), i| { ... }` と両方を `mut` で
束縛する。

- `M_topk` は `top_k.push(scored)`（`search.rs:321`）だけを止める。クロージャ本体には `top_k` への
  `&mut self` 呼び出しがこれ以外に無いため、`top_k` の `mut` が不要になり `unused_mut` が発火する
- `M_idx` は `local_matches.push(i)`（`search.rs:320`）だけを止める。同様に `local_matches` の
  `mut` が不要になり `unused_mut` が発火する

`workspace/plan.md:133` の「`-D warnings` を通すため未使用になる束縛は `let _ = scored;` で受ける」は
`scored`（新しく生まれる未使用**値**）の手当てであり、`unused_mut`（既存の**バインディング修飾子**が
不要になる件）は別種の警告で、この記述ではカバーされない。⚠️ 実際にビルドして確認していないが、
`push` が唯一の `&mut self` 呼び出しである（`search.rs` を読む限り他に無い）ことから、rustc の
`unused_mut` は素直に発火すると判断する。**計画をどう直すか**: `M_topk`/`M_idx` それぞれで、止めた
側のパターンから `mut` を外す（例: `M_topk` なら `|(top_k, mut local_matches), i|` へ）よう明記する。

### 4.（新規）`M_all` と `M_idx` は path クエリ以外の行（`users` 等）で incremental reuse を巻き添えにする

`local_matches.push(i)` は `IncrementalCache::update`（`search.rs:343` 付近）を経て
`prev_candidates` へ渡る。`can_reuse`（`search.rs:204-217`）の必須条件の 1 つが
`!self.prev_candidates.is_empty()`（`search.rs:211`）である。

`M_all` は `score_one_entry` 自体を全クエリで `None` 化するため、`M_idx` は fold 呼び出し点で
`local_matches.push(i)` を無条件に止めるため、**どちらも `c:\` に限らず全クエリで
`prev_candidates` が恒久的に空になる**。これは `has_path_sep` を持つクエリ（`c:\` 等）には
影響しない——そちらは `can_reuse` 自身が `!plan.has_path_sep` で無条件に incremental を無効化
しているため、ベースライン A でも元々 full scan である。**しかし `users`
（区切り無し = bitmask pre-filter が効く「比較の基準」、既存ハーネスのコメント
`tests/path_query_cost.rs:200`）のような通常クエリは、ベースライン A では 2 回目以降の反復で
`take_candidates()`（前回のマッチ集合を `mem::take` で使い回す・O(1)）を通るのに対し、
`M_all` / `M_idx` ではこの経路が恒久的に塞がれ、毎反復 `(0..self.entries.len()).collect()`
（312,108 要素の Vec を毎回組み立てる・O(N)）へ落ちる。**

`workspace/plan.md:152-155` の内部対照は `zzz`（0 件）だけを見ており、**`zzz` はそもそも
incremental を一度も使わないので、この巻き添えを検出できない。** `users` 行を `M_all` /
`M_idx` の表に含めて `A` と比較すると、意図した「post-match コストの分離」とは無関係な理由で
大きく重くなる可能性がある。

⚠️ 実害の大きさは Phase 3 が `users` 行を各変異ごとに測るかどうかに依存し、計画の Phase 3 の
文面（`workspace/plan.md:116-161`）はこれを明示していない（「各構成を実運用点で測る」とだけ書か
れ、対象クエリの絞り込みが `c:\`/`zzz` に限定されるとは書かれていない）。**計画をどう直すか**:
(a) Phase 3 の ablation 測定を path クエリ（`c:\`・`\program files\`・`zzz`）に限定すると明記する
——これらは元々 incremental が無効なのでこの巻き添えを構造的に避けられる。(b) `users` 等も比較の
ために残すなら、`M_all`/`M_idx` の該当行に「incremental reuse が巻き添えで無効化されるため、この
行の増分は post-match コストではない」という注記を Phase 4 の記録に必須で添える。

## 軽微

1.（持ち越し）`workspace/plan.md:55-57` の「どちらも Config の値をコード側で差し替えるだけで」が
   `normal_mode`（live-read）と `include_path_env`（Engine を作り直す必要あり）を対称に読める
   書き方のまま。直前の記述で手順は示されているため実害は小さい。
2.（新規）`workspace/plan.md:155` の内部対照の注記が「4 変異はいずれも zzz の経路に…」のままだが、
   `M_idx` の追加で変異は `M_all`/`M_hist`/`M_topk`/`M_idx`/`M_cmp` の**5 つ**になっている。
   `AGENTS.md`「数え上げも同じ強さである——版・経路・分岐を数えた散文は足すたびに腐る」に該当する
   軽微な陳腐化——「4 変異」を「5 変異」に直すか、数を書かない形（「いずれの変異も」）へ言い換える。

## 未検証

- L（`local_matches.push(i)` のコスト）は `M_idx` によって直接測定可能になった（第 1 版では
  理論的な推測しかできなかった点が改善）。ただし実測はまだ行われていないため、Sc
  （`ScoredEntry` 構築そのもの）が過剰決定の検算 (i) の残差としてどの程度出るかは Phase 3 の
  実行結果を見るまで分からない——**この点は計画自身が残差の候補として明記済み**
  （`workspace/plan.md:146-147`）であり、追加の指摘は不要と判断した。
- `要対処 4` の実害の大きさ（`users` 行が実際に Phase 3 で各変異ごとに測られるか）。
