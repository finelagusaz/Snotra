# plan — #1067 全件マッチのパスクエリに残る約 11.4 ms（マッチ後）

ブランチ: `chore/path-query-post-match-cost` / 起点 `ee8dcb1`
調査: `workspace/research.md`

## 目的

`c:\`（実 index 312,108 件が全件マッチする唯一のクエリ）のマッチ後コスト約 11.4 ms を
**引き算の推定から実測へ替え**、支配項を名指しできる状態にする。そのうえで、削る手段が
在るかを判定する。

**#1059 と同じ形で閉じうる issue である。** 勝てる成分が無ければ却下を記録してクローズする。
計画は**勝敗のどちらでも実行する作業だけ**で構成し、分岐の中身は未確定欄へ置く。

## 受け入れ条件

1. **`c:\` のマッチ後コストが、同一行・同一セッションの直接測定として得られている**
   （`c:\` と `zzz` の**行をまたぐ引き算をしない**。#1059 の 11.4 ms はそれだった）
2. **3 成分（履歴照合 / top-k / tie-break）の分解に過剰決定の検算が 1 本以上ある**
   （#1059 の 4 成分分解はちょうど決定系で、独立検算が 1 本も無かった）
3. **実運用点（`normal_mode = "substring"` + `include_path_env = true` ＝ PATH マージ後で
   `sorted_by_path = false`）の実額が測れている。** 2 要因それぞれの寄与が分離されている
4. **p50 と max の両方で判定できている**（2026-08-13 のユーザー決定）。標本数と percentile を
   出力に持つ。**数値ゲートは「max が 16,700 µs（60fps の 1 フレーム）を下回ること」**であり、
   実装へ進む判定が出た場合はこの条件を受け入れ条件へ**逐語で引き継ぐ**
5. 既存 `measure_path_query_frame_cost` は **1 バイトも変わっていない**（過去の全表との比較可能性）
6. 判定と測定値が `PERFORMANCE.md` に記録されている（採否のどちらでも）
7. **製品コードに計測用の変異が 1 行も残っていない**（ablation は当てて測って戻す）

## 変更ファイル一覧と対象シンボル

| ファイル | 変更 | 対象シンボル |
|---|---|---|
| `snotra-core/tests/path_query_cost.rs` | **追加**（既存関数は不変） | 新規 `measure_path_query_frame_cost_at_operating_point`・`//!` へ層の区別を追記 |
| `snotra-core/src/search.rs` | **変異のみ・コミットしない** | fold の呼び出し点（`search.rs:317-322`）— `M_topk` / `M_idx` |
| `snotra-core/src/search/tests/performance.rs` | **削除** | #1059 spike 6 点（`kmp_failure` / `advance_over` / `forward_pass` / `parallel_sweep` / `sequential_sweep` / `spike_forward_pass_vs_parallel_sweep_over_real_index`） |
| `docs/build-commands.md` | 追記 | 新計器の実行コマンド（既存 188 行の隣） |
| `PERFORMANCE.md` | 追記 | 実運用点の実額・2 要因の分離・ablation の分解・過剰決定の検算・判定 |
| `snotra-core/CLAUDE.md` | 追記 | `tests/path_query_cost.rs` の行に「実運用点を再現する計器」を足す |
| `docs/adr/ADR-*.md` | **判定次第**（未確定欄で決める） | 却下になった場合のみ新設 |

**ablation の変異（`snotra-core/src/search/scoring.rs` の `score_one_entry` / `ScoredEntry::cmp`）は
測定のために当てて戻す。コミットしない。**

## 実装順序

### Phase 1 — 実運用点を再現する計器を足す

- [x] `snotra-core/tests/path_query_cost.rs` に `measure_path_query_frame_cost_at_operating_point`
      を追加する（`#[test] #[ignore]`）。**既存 `measure_path_query_frame_cost` は触らない**
  - `Config::load()` の値をそのまま使う（`normal_mode` を差し替えない）
  - PATH マージは**製品の手順をそのまま写す**:
    `indexer::scan_path_env(material.tree(), show_hidden_system)` →
    `IndexMaterial::extend_with_path_entries` → `Engine::from_material`
    （出典 `src-tauri/src/main.rs:207`。製品側に共有関数が無いことは grep で確認済み・
    `indexing.rs` の `build_index_from_material` は `pub(crate)` かつ `PrebuiltIndex` を返す）
  - **2×2 の構成を 1 回の実行で回す**: `include_path_env` を {真, 偽} × `normal_mode` を
    {`substring`, `fuzzy`} に振り、4 表を出す（`SNOTRA_CONFIG_DIR` も temp コピーも使わない）。
    **2 軸は対称ではない**——`normal_mode` は `Engine::search` が毎回 config から読む
    live-read（`engine.rs:149`）ゆえ同じ `Engine` のまま振れるが、`include_path_env` は
    材料の作り方が変わるので **`Engine` を 2 つ建てる**。ゆえに構造は
    「Engine 2 個 × その各々で mode 2 通り」である
  - クエリは既存ハーネスと同じ 6 本を同じ順で並べる（比較可能性）
  - **2 回暖機 + 100 標本**、min / p50 / p90 / p99 / max を出す（max が受け入れ条件に入ったため）
  - 出力に**帰属のための諸元**を添える: 木の件数（PATH マージ前後）・`c:\` の実マッチ件数・
    `sorted_by_path` の実効値・`normal_mode`・`history_normalization`・`result_limit`・
    `migemo_enabled`・`history.bin` のエントリ数
  - **実 index が無ければ自己スキップする**（既存 2 関数と同じ `config.paths.scan.is_empty()`
    ガード）。CI では `#[ignore]` ゆえ走らない
  - **これは足場ではなく恒久の計器である**（撤去条件を持たない）。`RETROSPECTIVE.md`
    「計器の実運用点との乖離を、2 サイクル続けて『引き継ぐ』で送った」が
    「3 サイクル目に入るなら、送るのではなく別計器を足す側へ倒す」と指示している。
    **その旨を関数 doc に書く**（足場と読まれて次サイクルで消されないため）
  - **既存ハーネスを `SNOTRA_CONFIG_DIR` + temp コピーで実運用点へ寄せる案は採らない**:
    `normal_mode` は config で振れるが、**PATH マージは呼び出し側の手順であって config では
    再現できない**（`measure_path_query_frame_cost` は `extend_with_path_entries` を呼ばない）。
    加えて既存ハーネスを触ると過去の全表との比較可能性が失われる
  - **PATH マージの 3 行は `src-tauri/src/main.rs:207` の写しになる**（DRY の残余）。
    製品側に共有関数が無く（`indexing.rs::build_index_from_material` は `pub(crate)` かつ
    `PrebuiltIndex` を返す）、製品自身が既に 2 か所へ書いている。**受容し、doc に出典行を書く**
- [x] `sorted_by_path` は `PathStore` の private ゆえ crate 外から読めない。**読めるものへ
      置き換える**か、`#[doc(hidden)] pub` な既存の口が無いか調べて決める（未確定 1）
- [x] 関数 doc に次を書く: 既存ハーネスと**判定を混ぜない**理由 / 実 `%APPDATA%` を直接読むこと /
      `load_or_scan_with_stats` が旧版を書き換えうること（現行は v7 ゆえ起きない）
- [x] **この計器が単独で捕まえる経路を doc に数え上げる**（`docs/development-principles.md`
      「検知器を足す前に、その検知器が単独で捕まえる経路を数える」）。現時点で 2 経路:
      (1) PATH マージ後の `sorted_by_path = false` での tie-break、(2) `normal_mode = "substring"`
      での name スコアリング。**どちらも既存のどの計器も見ていない**
- [x] **この計器が見ないものも doc に書く**（同「検証の層と、層と層の隙間」）:
      検索結果の正しさ（挙動テストの担当）/ 実 UI スレッドが本当にこの額を払うか
      （`smoke:egui` の担当）/ **計器自身が実運用点を再現できているかは、出力に添える諸元を
      人間が読んで初めて確かめられる**（判定を持たない層である・`check:input-metrics` と同じ形）
- [x] **アクセサの追加と、それを読む計器の追加は 1 タスクに束ねる**（`AGENTS.md` 条件別チェック
      「新 API の導入と呼び出し点の移行は 1 タスクに束ねる」）——`-D warnings` 下で
      未使用の `#[doc(hidden)] pub` は `dead_code` で落ちる
- [x] **`path_query_cost.rs` の `//!` の見出し「# 2 つの層を混ぜない」から数を外す。**
      製品レベルの関数が 2 → 3 本になるので、数を書いた見出しはこの変更で偽になる。
      **数ではなく分類（製品レベル / 走査だけを切り出した写し）を見出しにする**
      （`AGENTS.md`「数え上げも同じ強さである——版・経路・分岐を数えた散文は足すたびに腐る」）
- [x] `docs/build-commands.md` に実行コマンドを足す
- [x] `cargo test --release -p snotra-core --test path_query_cost -- --ignored --nocapture --test-threads=1`
      で新計器が走ることを確かめる

### Phase 2 — 実運用点と旧計測点の差を測る（変異なし）

- [x] Phase 1 の 2×2 を**同一セッション・同一機（GPD WIN MINI）**で 3 回走らせる
- [x] 2 要因（`sorted_by_path` の反転 / `normal_mode`）の寄与を分離して表にする。
      **符号が予測できない**（`research.md` §2.2 のとおり逆向きに効く）ので、両方向を記録する
- [x] 旧計測点（`fuzzy` + PATH マージ無し）の行が、既存ハーネスの過去表（#1067 issue 本文の
      15,525 / 15,869 / 15,506）と重なることを確かめる
- [x] **重ならなかったときの第一容疑者は「計器が別物」ではなく `history.bin` である**
      （`research.md` §2.7）。過去の表は `SNOTRA_CONFIG_DIR` を temp へ向けており、
      **`history.bin` もそちらを向く**ので履歴が空だった疑いが濃い。空のマップは
      hashbrown が**ハッシュを計算する前に短絡する**（`vendor/hashbrown-0.17.1/src/map.rs:1283-1287`・
      本計画作成時にローカルツールチェーンの実物で確認）。
      **ゆえにブリッジ行には履歴のエントリ数を必ず併記する**——併記しなければ、この要因は
      次の反復でも未統制のまま残る
- [x] **PATH マージ後の `c:\` のマッチ件数を確かめ、記録の文言を確定させる。**
      「全件」でなくなるならその行の性格が変わる（`research.md` §2.3）
- [x] **2 要因の寄与を分離した表を確定させる**（本フェーズの成果物）

### Phase 1・2 の測定結果（2026-08-13・GPD WIN MINI・release・`0969483`）

実 `index.bin` v7 312,108 件 + PATH マージ 100 件 = 312,208 件 / 実 `history.bin` **17 件**（非空）/
`SNOTRA_CONFIG_DIR` 未設定（実 `%APPDATA%` を直読）/ 暖機 2 + **100 標本** / **同一セッション 3 回**。

**`c:\`（µs・run 1 / 2 / 3）**

| 構成 | `sorted_by_path` | p50 | max |
|---|---|---|---|
| **実運用点**（substring + PATH マージ） | false | **23,126 / 23,228 / 24,339** | **33,520 / 29,777 / 29,290** |
| fuzzy + PATH マージ | false | 20,710 / 21,044 / 21,209 | 25,258 / 24,895 / 24,603 |
| substring + マージ無し | true | 14,671 / 14,745 / 15,725 | 19,263 / 20,961 / 18,989 |
| 旧計測点（fuzzy + マージ無し） | true | 12,681 / 12,613 / 13,737 | 16,318 / 16,553 / 16,679 |

**2 要因の寄与（`c:\` p50・符号は予測できないと書いたが、実測では両方とも「重くする」向き）**

| 要因 | 差 |
|---|---|
| `sorted_by_path` 真 → 偽（fuzzy 固定） | **+7,500〜8,000 µs** |
| `normal_mode` fuzzy → substring（マージあり固定） | **+2,400〜3,100 µs** |
| 両方（旧計測点 → 実運用点） | **+10,000〜10,600 µs** |

和 10,000〜11,100 に対し同時適用が 10,000〜10,600 で、**ほぼ加法的**である。

- **`c:\` の実マッチ件数**: PATH マージ後も 200 件返す（`result_limit` 上限）。マージした 100 件は
  すべて `C:\` 配下ゆえ「全件マッチ」の前提は崩れていない（木 312,208 件）
- **ブリッジ**: 既存 `measure_path_query_frame_cost` を**同一セッションで**走らせた
  （計器は 1 バイトも触っていない）。実 config ＝ substring・マージ無しで `c:\` p50 **16,312**、
  `users` の results が **169**（substring の指紋。fuzzy なら 200）。新計器の同構成
  14,671〜15,725 と**同じ帯にある**——2 つの計器は同一構成で一致する
- **issue 本文の 15,525 / 15,869 / 15,506（fuzzy・別日）とは重ならない。** ただし
  **`history` 要因では説明できない**——今日の履歴は 17 件で非空、空なら hashbrown が短絡して
  **今日のほうが速くなる**向きであり、観測（今日のほうが速い）と符号が合わない。
  **日をまたぐ機体の drift と読む**（だから drift 対照を同一セッションで採る）

#### ⚠️ issue の前提が実運用点では成立しない

issue は「p50 15.5 ms は 1 フレーム 16,700 µs の**内側**であり、本 issue は落ちているものを
直すのではなく**余裕 1.2 ms を広げる**issue である」と書いている。**実運用点の p50 は
23.1〜24.3 ms で、1 フレームの 1.4 倍である。** max は 29.3〜33.5 ms＝約 2 フレーム。
`c:\users` / `c:\users\` も p50 15,647〜16,474 µs で 1 フレームに**迫る**（超えてはいない）。

**ただし射程は開発機の実 config に限る**（`substring` / `include_path_env = true`）。
**2 軸とも既定ではない**——既定は `fuzzy` / `false` で、既定構成の実起動が居るのは
表の最終行「旧計測点」であり、そこでは支配項の `cmp_paths` が発火しない。
**この限定を落とすと、「常に落ちている」を根拠に旗を直しに行った次の反復が、
既定構成のユーザーには 1 µs も効かない改修になる。** 正本は
`PERFORMANCE.md`「実運用点のパスクエリのフレームコストと、マッチ後の分解（#1067）」。

**この構成では「余裕を広げる」ではなく「常に落ちているものを直す」issue である。**
判定の基準（受け入れ条件 4 の「max < 16,700 µs」）はそのまま使う。

### Phase 3 — マッチ後コストの ablation（過剰決定つき）

変異は `snotra-core/src/search/scoring.rs` と `snotra-core/src/search.rs` へ 1 つずつ当て、
測ったら `git checkout` で戻す。**当てる場所は成分ごとに違う**——`score_one_entry` の中で
殺せるのは履歴照合と「マッチ後の全部」だけで、top-k と index 収集は fold の呼び出し点でしか
分離できない（`research.md` §2.6）。

**ablation の対象クエリは `c:\` / `\program files\` / `\zzz-no-such-path\` の 3 本に限る。**
`users`（区切り無し）を混ぜてはならない——`M_all` / `M_idx` は `prev_candidates` を恒久的に
空にするので `IncrementalCache::can_reuse` の `!self.prev_candidates.is_empty()`
（`search.rs:211`）が落ち、`users` だけが毎反復 `(0..312,108).collect()` へ転落する。
**マッチ後コストとは無関係な増分が乗る。** パスクエリ 3 本は `can_reuse` が
`!plan.has_path_sep` で元から incremental を無効化しているため**構造的にこの汚染を受けない**
（`users` は Phase 1・2 の変異なしの表にだけ載せる）。

**すべての変異に `-D warnings` の手当てを同時に書く**（PostToolUse hook が `.rs` 編集のたびに
走るため、手当てが無いと変異のたびに失敗が会話へ届く）。

- [x] **A（現行）** を測る
- [x] **M_all**: `score_one_entry` の `let base_score = score?;` の直後に `return None` を置く
      → マッチ後の**全部**（履歴照合・`ScoredEntry` 構築・`TopK::push` / `merge` /
      `into_sorted_vec`・tie-break・`local_matches.push`）が消える。
      **A − M_all が、行をまたがない直接測定である**。
      手当て: 以降が到達不能になり `base_score` も未読になるので
      `#[allow(unreachable_code, unused_variables)]` を関数へ付ける
- [x] **M_hist**: 履歴照合 3 種を定数（`(0, 0)` / `0` / `0`）へ置き換える。
      手当て: 引数 `history: &HistoryStore`（`scoring.rs:326`）の唯一の使用箇所が消えるので
      `_history` へ改名するか `let _ = history;` を置く
- [x] **M_topk**: **`search.rs:317-322` の fold の呼び出し点**で `top_k.push(scored)` だけを
      止める（`local_matches.push(i)` は残す）→ top-k 側（push / merge / into_results /
      tie-break）だけが消える。
      **`score_one_entry` を `None` にする形にしてはならない**——`local_matches.push(i)` を
      巻き添えにして下の `M_idx` と重なる（`research.md` §2.6）。
      手当ては 2 種類が要る: 未使用になる**値** `scored` は `let _ = scored;` で受け、
      **`mut` 束縛**は fold のパターンから外す（`|(top_k, mut local_matches), i|` へ）——
      `top_k` への `&mut self` 呼び出しは `push` 1 か所だけなので `unused_mut` が出る
- [x] **M_idx**（**第 5 の成分**）: 同じ呼び出し点で `local_matches.push(i)` だけを止める
      → incremental cache 用の全一致 index 収集（`c:\` では 312,108 個の `usize` と
      reduce の `extend`）だけが消える。**issue の 3 成分に入っていない**。
      手当ては同じく `|(mut top_k, local_matches), i|` へ（`unused_mut`）
- [x] **M_cmp**: `ScoredEntry::cmp` の第 4 キー `self.paths.cmp_paths(..)` を `Ordering::Equal` へ
      → tie-break の最終段だけが消える（`research.md` §2.5 の機序の検算）。
      **この変異だけは純粋な引き算ではない**——`Equal` を返すと `push` の
      `cmp(&worst) == Ordering::Less` が偽になる回数が増え、置換と `sift_down` が減る。
      **cmp のコストと「ヒープ仕事の減少」が混ざるので、そう注記して記録する。**
      `sorted_by_path = true` 側では差は 0 に近いはずで（`cmp_paths` が `a.cmp(&b)` で即返す）、
      **意味を持つのは実運用点の側だけである**
- [x] **A′（drift 対照）**: 全 revert 後に A を測り直す。**A 側の幅を明示し、削減がその何倍かを書く**
- [x] **過剰決定の検算 (i)**: `(A − M_hist) + (A − M_topk) + (A − M_idx) ≈ (A − M_all)` が
      成り立つか。**成り立たなければ、切り出した成分の外にまだ額が在る**
      （`ScoredEntry` の構築そのもの等）。**この検算が #1059 に無かった関係式である**
- [x] **過剰決定の検算 (ii)**: `M_all(c:\)` が **888 + 3,204 ≈ 4,100 µs** に着地するか。
      #1067 の 11.4 ms は「`zzz` の行で測ったループ素 888 と組み立て 3,204 を `c:\` の行へ移して
      引く」操作で作られており、**両行で同額であることを誰も測っていない**（`research.md` §2.1）。
      `M_all(c:\)` はまさにその 2 項の和であり、**行をまたぐ転記が妥当だったかの直接の検算になる**
- [x] **内部対照**: `\zzz-no-such-path\`（0 件）の行が**どの変異でも動かない**ことを確かめる。
      動いたら変異が意図しない枝に効いている。
      （構造的根拠: `zzz` は `let base_score = score?;`（`scoring.rs:478`）で必ず抜けるので、
      **いずれの変異も** `zzz` の経路に 1 行も掛からない）
- [x] **task ごとのコストを固定値で見積もらない**（`research.md` §7-3）。`into_par_iter().fold`
      は work-stealing ゆえ task 数は動的である。`TopK::merge` の額を「N × 200」の形で
      推定して記録に書かない——**測った差だけを書く**
- [x] 各構成を**実運用点**（Phase 1 の substring + PATH マージ）で測る。旧計測点でも A と M_all
      だけは測り、過去表への橋を残す
- [x] p50 と max の両方で分解する（受け入れ条件 4）
- [x] **分解表を確定させ、支配項を名指しする**（本フェーズの成果物）。検算 (i)(ii) が
      合わなければ、`research.md` §2.6 の第 4・第 5 の候補を切り出す変異を追加してから閉じる

### Phase 3 の測定結果（2026-08-13・GPD WIN MINI・release・同一セッション）

**実運用点**（substring + PATH マージ・`sorted_by_path = false`）の `c:\` p50（µs）。
変異は 1 つずつ当てて `git checkout` で戻した。**A′ は全 revert 後の drift 対照。**

| 構成 | 殺したもの | p50 | **A − X** |
|---|---|---:|---:|
| **A**（現行） | — | 22,535 | — |
| **A′**（drift 対照） | — | 23,530 | **+995（+4.4%）** |
| **M_all** | マッチ後の全部 | 6,702 | **15,833** |
| **M_topk** | top-k（push / merge / into_results / tie-break） | 10,861 | **11,674** |
| **M_cmp** | tie-break の第 4 キー（`cmp_paths`）だけ | 14,821 | **7,714** |
| **M_idx** | incremental 用の全一致 index 収集 | 19,714 | **2,821** |
| **M_hist** | 履歴照合 3 種 | 22,021 | **514** |

**支配項は tie-break の `cmp_paths` である。** top-k の 11,674 µs のうち 66% を占める。

#### 検算 (i) — 過剰決定（#1059 に無かった関係式）

`(A − M_hist) + (A − M_topk) + (A − M_idx) = 514 + 11,674 + 2,821 = **15,009**` に対し
`A − M_all = **15,833**`。**残差 824 µs（5.2%）** で、`ScoredEntry` の構築そのもの等に相当する。
**3 成分の外に大きな額は無い。**

#### 検算 (ii) — 行をまたぐ転記は 28% 過小だった

旧計測点（fuzzy + マージ無し）で `M_all(c:\)` = **5,229**。#1059 の分解からの予測は
`888（ループ素）+ 3,204（組み立て）+ ≒0（find）= 4,092` で、**実測はその 1.28 倍**である。
同じ実行の `A(c:\)` が 12,179 なので、**旧計測点のマッチ後コストは 12,179 − 5,229 = 6,950 µs**
——issue が書いた 11.4 ms は、(a) 別日の A（15,525）と (b) 過小なベースライン（4,092）の
両方で膨らんでいた。**「行をまたぐ引き算をしない」ことが実際に 40% の誤りを消した。**

#### `M_cmp` は `sorted_by_path` が偽のときにしか効かない（独立な裏取り）

同じ実行の旧計測点の腕（`sorted_by_path = true`）では `M_cmp` の `c:\` が 12,313 で、
A の 12,179 と**差が無い**（`cmp_paths` が `a.cmp(&b)` で即返すため）。

**Phase 2 の要因分解（`sorted_by_path` 真→偽で +7,500〜8,000 µs）と、Phase 3 の `M_cmp`
（偽の側で −7,714 µs・真の側で ≒0）は、別々の手順で同じ額を指している。**

#### 内部対照

`\zzz-no-such-path\`（0 件）の p50 は A 10,111 / M_all 10,007 / M_hist 10,170 / M_topk 10,536 /
M_idx 10,160 / M_cmp 10,711 / A′ 10,091 で、**どの変異でも drift の帯（±1,000）を出ない**
——変異はマッチ後の経路にしか掛かっていない。

#### 受容する残余

- **`M_hist` の 514 µs は drift の帯（995 µs）の内側**であり、**0 と区別できない。**
  「履歴照合は 514 µs である」とは書かず「**1 ms を超えない**」と書く
- **`M_cmp` は純粋な引き算ではない**（`Equal` を返すと置換と `sift_down` が減る）。
  cmp のコストと減ったヒープ仕事が混ざっている
- `users` は ablation の表に載せない（`M_idx` で実測 4,309 µs へ転落した——`prev_candidates`
  が空になり incremental が壊れるため。計画どおりの汚染で、パスクエリ 3 本は影響を受けない）

### Phase 4 — 記録

- [x] `PERFORMANCE.md` へ節を足す: 実運用点の実額 / 2 要因の分離 / ablation の分解 /
      過剰決定の検算 / drift 対照 / **判定**
- [x] **数字に「測った対象の限定」を添える**（`RETROSPECTIVE.md`「却下の記録に、測った実装への
      限定を書き落とした」）——機体名（GPD WIN MINI）・日付・commit・config の実値・標本数
- [x] `snotra-core/CLAUDE.md` の `tests/path_query_cost.rs` の行へ、実運用点の計器の存在を足す
- [x] `npm run governance:check` を通す
#### 判定（2026-08-13）— **削る手段は在る。ただし max のゲートは単独では満たさない**

**支配項は `sorted_by_path` が偽であることに由来する tie-break である**（`cmp_paths` = 7,714 µs・
要因分解でも 7,500〜8,000 µs と別経路で一致）。**手段は「マッチ後のコストを削る」ではなく
「速い経路を失わないようにする」側にある**——`IndexTree::extend_with_roots` が PATH エントリを
末尾へ足して旗を無条件に下ろしているのを、**マージ後に測り直す**か**整列を保って併合する**へ替える。

- **正しさは構造で保たれる**（どちらの案も）。旗は「契約ではなく実測」という既存の形をそのまま
  使うので、測り直して真になったときだけ速い経路へ入る。**「整列しているはず」と仮定しない**
- **効果の見積もり**: `M_cmp` の p50 14,821 が下限の目安（実際は `cmp_paths` が `a.cmp(&b)` に
  なるだけなのでもう少し高い）。**p50 は 1 フレーム 16,700 µs の内側へ入る見込み**
- **max のゲート（< 16,700 µs）は満たさない見込み**: `M_cmp` の max は 23,976、`A` は
  27,516〜36,709。**マッチ後を全部消した `M_all` でようやく max 8,618** なので、
  max を満たすには `sorted_by_path` の復元だけでは足りず、残る `M_idx`（2,821）と
  `ScoredEntry` 構築の残差（824）まで踏み込むことになる
- **履歴照合（≤1,000 µs・drift の帯の内側）へは手を入れない。** issue が第 1 に挙げた成分だが、
  実測で支配項ではなかった。`research.md` の第 3 案（index → boost の疎な表）は**採らない**

**実装着手と issue のクローズは 2026-08-13 の承認の射程外である**（承認は「計器を足して測ること」まで）。
実測をお見せして指示を受けたうえで、計画に実装フェーズを足す。

- [x] **判定を下し、この計画を書き換える。** Phase 3 の分解に照らして「削る手段が在るか」を
      決め、結論を計画本文へ反映する。**下すこと自体は勝敗のどちらでも起きる**ので作業項目に
      置いてよい——**枝の中身は作業項目にしない**（`/start-issue` Step 4）。
      枝は 2 つある: 手段が在れば計画へ実装フェーズを足す（候補は「`extend_with_roots` が旗を
      下ろすのをやめる——マージ後に測り直す、または整列を保って併合する」「index → boost の
      疎な表」。受け入れ条件 4 の数値ゲート「max < 16,700 µs」を逐語で引き継ぐ）。
      無ければ `docs/adr/` へ却下を起こす。
      **issue のクローズと実装着手はどちらの枝でも本計画の外である**——外向き・不可逆ゆえ、
      実測をお見せしてから改めて指示を受ける（2026-08-13 の承認の射程）

### Phase 5 — #1059 spike 足場の撤去

- [x] `snotra-core/src/search/tests/performance.rs` の #1059 spike 6 点を消す。
      撤去条件「本 issue の判定を記録した PR がマージされたら消す」は **PR #1066（`2758340`）の
      マージで発火済み**（`research.md` §2.7）
- [x] `PERFORMANCE.md`「却下: パスクエリの走査を前向き 1 パスへ替える」の計器への参照を、
      撤去済みと分かる形へ直す

### Phase 6 — 実装差分を確定させる

- [x] `cargo test --workspace`（`#[ignore]` 以外）が緑
- [x] `cargo clippy --workspace --all-targets -- -D warnings` が緑
- [x] `cargo doc --workspace --no-deps --document-private-items`（intra-doc link・hook は沈黙する）
- [x] `git diff` で `snotra-core/src/search/scoring.rs` と `snotra-core/src/search.rs` に
      変異が 1 行も残っていないことを確かめる（**引数 1 個の `git diff` で作業ツリーを見る**——
      `main...HEAD` の 3 点形は commit 同士の比較ゆえ未コミットの変異を見ない・#922）
- [x] `git diff` で `measure_path_query_frame_cost` が 1 バイトも変わっていないことを確かめる

## 不変条件と異常系

- **既存 `measure_path_query_frame_cost` は不変**。過去の全表（#1057 / #1059 / 反復 4）との
  比較可能性がこれに掛かっている（`fixing-instrument-invalidates-ab-comparison`）
- **製品コードに変異を残さない。** ablation は測って戻す。Phase 6 の `git diff` 検査が検知器
- **`zzz`（0 件）と `users`（区切り無し）は本 issue の対象外であり、変わらないことが正しい**
  （issue の受け入れ案）。Phase 3 の内部対照がこれを兼ねる
- **実 `index.bin` は現行版 v7 ゆえ計器が書き換えない**が、旧版が置かれた環境では
  `load_or_scan_with_stats` が昇格を起こす（ハーネスの `//!` の既存警告と同じ）
- **新計器は実 `%APPDATA%\Snotra` を読む。** `#[ignore]` ゆえ既定のスイートでは走らないが、
  `HistoryStore::load()` の使用は計測ハーネスに限る規律（#963）の内側であることを doc に書く
- **PATH マージ後は `c:\` が「全件」でなくなりうる**（他ドライブの実行ファイルが根として入る）。
  件数を出力に添えるので、前提が崩れたらその場で分かる

## テスト方針と検証コマンド

- 本 issue の Phase 1〜5 は**計測ハーネスと文書**であり、製品の挙動を変えない。
  ゆえに「結果が変わらないこと」の検知器は既存のスイートがそのまま担う
- **実装へ進む判定が出た場合**（未確定 5）は、受け入れ条件「検索結果が集合・順序とも
  1 件も変わらない」に対して
  `snotra-core/src/search/tests/path.rs` の
  `skipping_name_scoring_changes_nothing_over_real_index` と**同じ型**の全件 A/B を新設する
  （`(name, path)` の列を順序ごと比較。件数一致では足りない）。
  **`sorted_by_path = false` 側も覆うこと**——既存の A/B は `real_index_entries` →
  `SearchEngine::new_with_migemo` 経由ゆえ旗が真の側しか通っていない

```
cargo test --release -p snotra-core --test path_query_cost -- --ignored --nocapture --test-threads=1
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --workspace --no-deps --document-private-items
npm run governance:check
```

## SPEC.md・関連文書の更新要否

- **`SPEC.md`: 不要。** 本 issue は挙動を変えない（計測と記録）。実装へ進む判定が出た場合も、
  対象は内部のコストであり `SPEC.md` の記述する挙動・フロー・状態遷移を変えない
  （変える案が候補に上がったら、その時点で仕様変更として扱い直す）
- `PERFORMANCE.md`: **必須**（Phase 4）
- `snotra-core/CLAUDE.md`: 必須（新計器の索引・Phase 4）
- `docs/build-commands.md`: 必須（Phase 1）
- `docs/adr/`: **判定次第**。却下になったら「否定の知識」として新設する（#593 の基準）

## 未確定（実装前に潰す）

- [x] **`sorted_by_path` の実効値を crate 外から読む手段** → **`#[doc(hidden)] pub` の
      アクセサを 3 つ足す（鎖の 3 段すべてが private である）。**
      `PathStore::sorted_by_path`（`path_store.rs:119`）→ `SearchEngine::entries`
      （`search.rs:118`）→ `Engine::search_engine`（`engine.rs:99`）がいずれも非公開なので、
      **`Engine` から辿るには 3 ファイルに 1 つずつ passthrough が要る**（先例は
      `Engine::entry_count` / `entry_name`・`engine.rs:309-316`）。
      `tests/memory_footprint.rs` の `footprint_rows` の先例は `SearchEngine` を直接構築して
      呼んでおり `Engine` を経由しないが、**本計器は `Engine::search` を必ず経由する設計**
      ゆえ同じ形にはできない。`PathStore` の private であり既存の口では読めないことを
      `grep -rn "doc(hidden)"` で確認した（`footprint.rs:106` は `footprint_rows`、
      `indexer.rs:1954` は `IndexMaterial::has_derived`）。先例は `indexer.rs:1951` の doc
      「検知器と計測ハーネスのためだけに在る」であり、**同じ扱いを明示的に先例として書いている**。
      `docs/development-principles.md`「テストのために可視性を広げる弁明を doc に書いたら、
      切り口が対象と合っていない兆候である」に当てた結果: **ここは I/O を持たない `bool` 1 つの
      読みで、切り出すべき純粋な導出が別に無い**ため、当該条項の言う「弁明」には当たらない。
      **代理指標（PATH マージを通したか）では足りない**——`extend_with_roots` は空 vec で
      早期 return するので「通した」と「旗が下りた」は一致しない（`research.md` §7-1）
- [x] **`adjusted_history_boost` の性質**（issue「着手前に潰すこと 3」） → **「boost が 0 に
      なりうる件を先に弾く」形は `c:\` では成立しない。** 重みは `GLOBAL_WEIGHT = 5` /
      `QUERY_WEIGHT = 20` / `FOLDER_EXPANSION_WEIGHT = 5`（`snotra-core/src/search.rs:35-37`）で、
      `c:\` は全件が `base_score = 3000` で並ぶため `base + max_boost > worst` がほぼ常に真になり
      枝刈りが効かない。**`c:\` こそが本 issue の対象なので、この案は候補から外れる。**
      代わりに**第 3 案**を候補へ置く: 履歴ブーストのうち key だけで決まる成分
      （global / folder_expansion）を「エントリ index → boost」の疎な表として先に作り置く
      （履歴が変わるのは起動・実行時だけで、検索のたびに 312,108 回の 119 B ハッシュを払う
      理由が無い）。**Phase 3 で履歴照合が支配項だった場合にだけ意味がある**
- [x] **max の標本数** → **2 回暖機 + 100 標本で始める。** 既存ハーネスの 20 標本では
      max が 18,642〜22,420 µs と 20% 揺れており（issue 本文の 3 回の実行）、帰属に足りない。
      100 標本で p90 / p99 / max を出し、**p99 と max の開きが実行間で安定するかを Phase 1 の
      初回で見る**。安定しなければ Phase 3 の前に上げる（上げた値は表の label に書く）
**測定でしか答えの出ない問いは、この欄から作業項目へ畳んだ**（2026-08-13）。当初は #1059 の
plan.md（`2758340`）に倣ってここへ置いたが、`/implement` の入口ゲート（未確定ゼロ）と
`/start-issue` の引き渡し契約が要求するのは**この欄が空であること**であり、**「実装前に潰す
未確定」と「フェーズの成果物」は別物**である。畳んだ先は次のとおりで、**内容は 1 つも
落としていない**:

| 元の未確定 | 畳んだ先 |
|---|---|
| 11.4 ms の内訳 | Phase 3「分解表を確定させ、支配項を名指しする」 |
| 実運用点の実額と 2 要因の寄与 | Phase 2「2 要因の寄与を分離した表を確定させる」 |
| PATH マージ後の `c:\` のマッチ件数 | Phase 2「PATH マージ後の…件数を確かめ、記録の文言を確定させる」 |
| 判定: 削る手段が在るか | Phase 4「判定を下し、この計画を書き換える」——**下すこと自体は勝敗のどちらでも起きる**ので作業項目に置ける。枝の中身は散文のままにした |
- [x] **Phase 5（spike 撤去）を本 PR に含めてよいか** → **含める（2026-08-13 に逐語で確認済み。
      記録は「人間レビュー」節）。** 撤去条件「本 issue の判定を記録した PR がマージされたら
      消す」は PR #1066（`2758340`）のマージで発火済みである

**issue のクローズは作業項目にも未確定にも置かない。** 外向きの不可逆操作であり、
実測をお見せしてから改めて指示をいただく（#1059 の承認記録と同じ扱い）。

## セルフレビュー

- リスク: **高**（`AGENTS.md` 条件別チェック「ガバナンス文書を変更」——`snotra-core/CLAUDE.md` /
  `docs/build-commands.md` / `PERFORMANCE.md`）
- plan-review: **計画準拠レビュー 1 体**（`workspace/plan-review-post-match-cost.md`）
- エージェント数: **2**（敵対的調査 1 体 + 計画準拠レビュー 1 体）
- 主エージェント自身の照合 5 点:
  1. **issue の全要件に作業項目が対応する** — 「着手前に潰すこと」3 点は Phase 3（ablation）/
     Phase 1・2（計器の実運用点）/ 未確定の `adjusted_history_boost`（解消済み）へ対応。
     「max を射程に入れるか」はユーザー決定（2026-08-13）で受け入れ条件 4 へ
  2. **境界条件と検証** — 全件マッチ（`c:\`）/ 200 件（`\program files\`）/ 0 件（`zzz`）/
     区切り無し（`users`）。**`users` は ablation から除外**（incremental の巻き添え・下記 4 番目）
  3. **新しい状態・リソースの正常/失敗/破棄経路** — 新計器は実 index 不在で自己スキップ。
     ablation の変異は「当てて測って戻す」で、破棄経路は Phase 6 の `git diff` 検査が担う
  4. **より単純な既存パターンで置き換えられないか** — 既存ハーネスを `SNOTRA_CONFIG_DIR` で
     寄せる案を検討し、**PATH マージが config では再現できない**ため却下（Phase 1 に記録）
  5. **壊してはならない不変条件に検知手段がある** — 既存ハーネスの不変性と製品コードの
     無変異は Phase 6 の `git diff`（引数 1 個の形）で見る
- 要対処: **4 件、全件を計画へ反映**（すべて主エージェントが `file:line` で再照合し成立を確認）
  1. `sorted_by_path` アクセサは 1 つでなく **3 段の passthrough** が要る（鎖の 3 段とも private）
  2. `M_all` / `M_hist` の `-D warnings` 手当てが欠けていた（`unreachable_code` /
     `unused_variables`。PostToolUse hook が `.rs` 編集のたびに走るため実害が出る）
  3. `M_topk` / `M_idx` は `unused_mut` を出す（fold のパターンから `mut` を外す）
  4. **`M_all` / `M_idx` は `prev_candidates` を空にするので `users` 行が汚染される**
     （`can_reuse` の `!prev_candidates.is_empty()`・`search.rs:211`）。ablation を
     パスクエリ 3 本に限定して構造的に回避した
- 軽微: 2 件とも反映（2×2 の 2 軸が非対称であること / 「4 変異」の数え上げを外した）
- 未検証: **`M_cmp` の差分は純粋な引き算ではない**（`Equal` を返すと置換と `sift_down` が減り、
  cmp のコストと減ったヒープ仕事が混ざる）。計画に注記して記録へ引き継ぐ形で受容した

## 人間レビュー

- [x] 承認済み — 2026-08-13 / 問い: "**`workspace/plan.md` へ注釈を追加する**、または **計画を明示的に承認する** のどちらかをお願いします / 承認の範囲は **「計器を足して測ること」** に限らせてください。判定後の issue クローズや実装は外向き・不可逆ですので、実測をお見せしてから改めて指示をいただきます / 未確定が 1 件残っています — **Phase 5（#1059 spike 足場の撤去）を本 PR に含めてよいか** / お含みおきください — この計画は #1059 と同型で、**測定でしか解消しない未確定を残したまま workspace をコミット**します" / 回答: "OK"
  - **承認の射程**: 計器の追加と測定まで。**判定後の実装・issue クローズは含まない**（改めて指示を受ける）
  - **Phase 5（spike 足場の撤去）も承認済み** — 2026-08-13 / 問い: "**Phase 5（#1059 spike 足場の撤去）は承認に含まれると解しました**。承認の問いで名指しして提示し異議がなかったためですが、違っていれば落とします" / 回答: "spike 足場の撤去もOK"（推定ではなく逐語の確認である）
