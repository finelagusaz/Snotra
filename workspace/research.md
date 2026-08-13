# research — #1067 全件マッチのパスクエリに残る約 11.4 ms（マッチ後）

対象 issue: #1067（`#1059` から切り出し・`#1059` は PR #1066 で却下により閉じた）
ブランチ: `chore/path-query-post-match-cost` / 起点 `ee8dcb1`

## 1. issue の要約

パスクエリ `c:\`（実 `index.bin` 312,108 件が**全件マッチ**する唯一のクエリ）の p50 は
15,525 µs で、1 フレーム 16,700 µs の**内側**にある。#1057（name の Fuzzy スコアリングを飛ばす）
と #1059（走査の前向き 1 パス化を却下）を経て残った額のうち、**マッチが成立した後に走る 3 成分**
——履歴照合 3 種 / `TopK::push` / tie-break（`ScoredEntry::cmp` → `PathStore::cmp_paths`）——が
約 11.4 ms を占める、というのが issue の推定である。

本 issue は「落ちているものを直す」ではなく「余裕 1.2 ms を広げる」issue である。
**max（18,642〜22,420 µs）は 1 フレームを超え続けており、2026-08-13 のユーザー決定で
受け入れ条件に入れる**（p50 と max の両方で判定する）。

## 2. 事実の確認（一次証拠つき）

### 2.1 11.4 ms は**引き算で作った数字**である（測定値ではない）

出典は #1059 本文の 4 成分分解で、**`\zzz-no-such-path\` の行で測ったもの**である:

| 成分 | 額 (µs) | 出典 |
|---|---:|---|
| ループ素のオーバーヘッド | 888 | #1059 本文の表 |
| name の Fuzzy スコアリング | 3,824 | 同（#1057 が消した） |
| 正規化キーの組み立て（`with_normalized_key`） | 3,204 | 同 |
| `find(pq)` | 3,740 | 同 |
| 合計 | 11,656 | #1057 の A 列 `zzz` と一致 |

#1059 自身が「**4 測定値 4 未知数のちょうど決定系ゆえ代数的恒等式であり、過剰決定の関係式が
1 本も無い。個々の値は ±数百 µs**」と書いている。#1067 の 11.4 ms は、この 888 と 3,204 を
**別の行（`c:\`）へ移して**引いた値であり、独立検算は 1 つも無い。

自己整合性の検算（本調査で実施）: #1067 の `zzz` p50 実測 7,762〜8,731 は
11,656 − 3,824 = 7,832 とよく合う。**成分の帰属ではなく合計の連続性だけが確かめられている。**

### 2.2 計測環境は実運用点と **2 方向** に食い違っている（どちらも実測で確認）

`%APPDATA%\Snotra\config.toml`（本調査で直接読んだ）:

```
normal_mode = "substring"      # ← 計測は "fuzzy" に差し替えている
include_path_env = true
history_normalization = "disabled"
result_limit = 200 / recent_limit = 8 / migemo_enabled = false
[[paths.scan]] は 4 本すべて C:\ 配下（最大の根が 'C:\' + extensions=[".txt"] + include_folders）
```

**(a) `sorted_by_path` — 計測は速い側、実運用は遅い側**

- `PathStore::cmp_paths`（`snotra-core/src/search/path_store.rs:256`）は
  `sorted_by_path` が真なら `a.cmp(&b)` で即返し、偽なら `CMP_BUFS` へ**両辺のフルパスを
  組み立てて**比較する
- `IndexTree::extend_with_roots`（`snotra-core/src/index_tree.rs:732`）は末尾
  `*sorted_by_path = false;`（`index_tree.rs:761`）で旗を下ろす。
  **ただし「無条件」ではない**（敵対的調査で訂正・§7-1）: 冒頭に
  `if entries.is_empty() { return; }`（`index_tree.rs:733-735`）が在り、`scan_path_env` が
  空 vec を返す（PATH が空、または `reject_existing` が全件を既存と判定）ときは旗が立ったまま残る。
  **正しい言い方は「PATH スキャンが 1 件以上返したときに限り下ろす」である**
- PATH マージの呼び出し点は製品に 2 つあり、どちらも `include_path_env` で条件づく:
  `src-tauri/src/main.rs:207`（起動）と `src-tauri/src/indexing.rs:120`（背景の再構築）。
  実 config は `include_path_env = true` である。**この開発機では旗は下りる側にあると強く見込まれる**
  ——`HKCU\Environment\Path` は非空（**722 文字・16 ディレクトリ・全て `C:\` 配下**・本調査で
  PowerShell により直接読んだ）、かつ scan 根の `C:\` は `extensions = [".txt"]` ゆえ
  PATH 上の `.exe` / `.bat` / `.cmd` / `.com` は索引に居らず `reject_existing` を通り抜ける。
  **ただしこれは導出であって実測ではない。** マージ後の件数は Phase 1 の計器が出す
- `measure_path_query_frame_cost`（`snotra-core/tests/path_query_cost.rs:184`）は
  `load_or_scan_with_stats` を呼ぶだけで `IndexMaterial::extend_with_path_entries`
  （`snotra-core/src/indexer.rs:1933`）を通さない。**ゆえに計測は `sorted_by_path = true` 側**
- → **tie-break が支配的なら、実運用の `c:\` は表より重い。どれだけ重いかは未測定である**

**(b) `normal_mode` — 計測は `fuzzy`、実運用は `substring`**

`Engine::search` は `SearchMode::from(cfg.search.normal_mode)`（`snotra-core/src/engine.rs:149`）
で決める。#1057・#1059・#1067 の全表は `normal_mode = "fuzzy"` の temp config で測っている。
`substring` では `score_one_entry` の分岐が 2 つ変わる:

- `skip_name`（`scoring.rs:376`）は `mode == SearchMode::Fuzzy` を要求するので**発火しない**
  → 全 312,108 件が `lower_name.find("c:\\")` を通る（`Utf32String` + nucleo ではなく `find` ゆえ
  安いが 0 ではない）。**#1057 の -4.3 ms はそもそも substring では効いていない**
- `adjusted_history_boost`（`scoring.rs:193`）の cap は `Fuzzy` かつ
  `normalization == FuzzyRelativeCap` のときだけ効く。実 config は
  `history_normalization = "disabled"` ゆえ**どちらの mode でも素通り**（この 1 点は食い違わない）

→ **(a) と (b) は逆向きに効く。** 実運用点を再現すると tie-break で重くなり name スコアリングで
軽くなる。**符号すら予測できないので、実運用点は測るしかない。**

### 2.3 `c:\` が全件マッチする前提は成立する

scan の 4 根はすべて `C:\` 配下（うち 1 本が `C:\` そのもの）。正規化キーは小文字化 + `/`→`\`
なので全キーがバイト 0 から `c:\` に一致する。`find` は 3 バイト比較で `Some(0)` を返す。
**ただし PATH マージ後は他ドライブの実行ファイルが根として足されうる**ため、実運用点では
「全件」ではなくなる可能性がある（PATH の中身は未確認・実運用点の計器で件数を出せば分かる）。

### 2.4 履歴照合 3 種は「マップが大きい」からではなく「119 B を 3 回ハッシュする」から高い（**要検証**）

`HistoryStore::prune`（`snotra-core/src/history.rs:344`）は `top_n`（= `effective_result_limit()`
= 実 config で 200）で `global` / `query` を剪定する。ゆえに引く相手のマップは高々 200 件で、
`c:\` の 312,108 件はほぼ全部が**ミス**である。3 種の実体（`history.rs:267 / 290 / 330`）は
どれも `FxHashMap::get` 1〜2 段:

- `get_global_stats_normalized(key)` — 119 B の key を 1 回ハッシュ
- `query_count_pre_normalized(hq, key)` — まず**短いクエリキー**を引き、当たったときだけ key を引く。
  `c:\` が履歴に無ければ 2 段目に到達しない
- `folder_expansion_count_normalized(key)` — `is_folder` のときだけ（実データの 81.9% が folder）

**一次証拠（本調査で実測）**: `%APPDATA%\Snotra\history.bin` は **4,040 バイト**である
（同ディレクトリの `index.bin` は 17,323,918 バイト）。剪定容量 200 に届くどころではなく、
マップは実際に数十件の規模である。**引く相手は L1 に収まる。**

→ 概算では単スレッド 8 ms 前後・16 コアなら数百 µs〜3 ms。**11.4 ms の支配項が履歴照合だと
いう issue の並びは仮説にすぎない。** これは ablation で決める。

### 2.5 tie-break が「総当たりで発火する」という機序の記述は**過大である疑いが濃い**

`TopK::push`（`scoring.rs:253`）は満杯後、候補を**ヒープの worst 1 件とだけ**比べる
（`peek_mut` → `scored.cmp(&worst)`）。`ScoredEntry::cmp`（`scoring.rs:138`）は
score → last_launched → `lower_name` → `cmp_paths` の順で、`cmp_paths` へ落ちるのは
**候補と worst の `lower_name` がバイト一致したときだけ**である。`c:\` は全件が score 3000 で
並ぶが、`lower_name` は大半が早期に決着する。

**ただし「1 候補につき 1 比較」ではない**（敵対的調査で訂正・§7-2）。`std` の
`BinaryHeap::peek_mut` は `DerefMut` を経た場合にかぎり drop 時に `sift_down(0)` を走らせるので、
**置換が起きた候補は追加で O(log 200) 段の `ScoredEntry::cmp` を払い、その各段が再び
`cmp_paths` へ落ちうる**。置換の回数は「上位 k に入る記録更新」の回数で、ランダム順なら
`k·ln(n/k) ≈ 200 × ln(312108/200) ≈ 1,500` 回程度と概算できるが、**索引はパス順に並んでおり
`lower_name` と相関するので、この概算は当てにならない**。加えて実 config は `C:\` を
`include_folders = true` で走査するため同名フォルダ（`bin` / `src` / `node_modules` 配下など）が
多く、`lower_name` の一致は「稀」と断定できない。

→ **機序としては「総当たり」でも「1 候補 1 比較」でもない。実呼び出し回数は未測定である。**
Phase 3 の `M_cmp`（第 4 キーを `Ordering::Equal` にする変異）がこの額を直接切り出す。

`path_store.rs:261` の既存コメントは「`c:\` のような全件が同スコアになるクエリでは tie-break が
総当たりで発火し」と書いており、**issue 本文はこれを引き継いでいる。** ただし「総当たり」が
指すのは第 3 キー（`lower_name` の比較）までで、第 4 キー（`cmp_paths`）ではない可能性が高い。
**`sorted_by_path = false` の実運用点でここが効くかどうかが、本 issue の分岐点である。**

### 2.6 issue が名指ししていない**第 4 の候補**が在る（`TopK::merge` の非対称）

`c:\` と `zzz` の差に効く成分で、issue の 3 つに入っていないものが少なくとも 1 つある:

- rayon の fold は task ごとに `TopK::new(limit)` を作る。`c:\` は**どの task もヒープを 200 件
  まで満たす**が、`zzz` は 1 件も入らない
- reduce の `TopK::merge`（`scoring.rs:265`）は相手の全要素を `push` へ通す。task が N 個なら
  `c:\` は 200 × (N−1) 回の `push`（＋ sift）を払い、**`zzz` はゼロである**
- `heap_into_results`（`scoring.rs:215`）の `into_sorted_vec` と 200 件のフルパス組み立ても同様に
  `c:\` だけが払う

**さらに 5 つ目がある——incremental cache 用の全一致 index の収集。** `search.rs:307-334` の
fold は `TopK` と**並んで `Vec<usize>`** を育てており、`score_one_entry` が `Some` を返した
エントリすべてで `local_matches.push(i)` を**無条件に**行う（top-k から落ちた一致も残すため。
コメントは `search.rs:315-316`）。

- `c:\` は **312,108 個の `usize` を push する**（2.5 MB。倍々確保ゆえ書き込み総量はその約 2 倍）
- reduce の `a_matches.extend(b_matches)`（`search.rs:331`）が task 間でさらに連結する
- **`zzz` はゼロである**（`Some` を 1 件も返さない）

**これも「マッチ後コスト」だが、`score_one_entry` の中には無い。** ablation の切り口を
`score_one_entry` の 3 成分だけに限ると、第 4・第 5 の成分が「残差」へ紛れて機序を取り違える。

**しかも変異の設計に直接効く**: `score_one_entry` を `None` にする変異は
`local_matches.push(i)` **も**巻き添えにするので、top-k だけを切り出したことにならない。
変異は `score_one_entry` の中ではなく **fold の呼び出し点**（`search.rs:317-322`）へ当てて、
2 つの `push` を独立に殺せる形にする必要がある。

### 2.7 過去の全表は**履歴が空**で測られた疑いが濃い（第 3 の未統制要因）

`SNOTRA_CONFIG_DIR` を temp へ向けると、`Config::config_dir()` から派生する **`history.bin` も
そちらを向く**（`snotra-core/CLAUDE.md`「保存先の導出は `Config::config_dir()` の 1 点だけ」）。
#1057 / #1059 / #1067 の計測は「実 `index.bin` のコピー」と書いており、**`history.bin` を
コピーしたとは書いていない。**

不在なら空が返ることは確認済み: `HistoryStore::load_in` は
`load_with_fallback(...).unwrap_or((HistoryData::default(), _))`（`history.rs:116-118`）で、
`history.rs:104` の doc も「CI のランナーにはそのファイルが無く `load()` も空を返す」と書く。

**空のマップはハッシュを計算しない。** ローカルツールチェーンの hashbrown 0.17.1 の実物
（`.rustup/.../vendor/hashbrown-0.17.1/src/map.rs:1283-1287`）を読んだ:

```rust
if self.table.is_empty() {
    None
} else {
    let hash = make_hash::<Q, S>(&self.hash_builder, k);
    ...
}
```

→ **過去の表では履歴照合 3 種のコストは実質ゼロだった可能性が高い。** そうであれば、
**issue が第 1 に挙げた成分は、測定された構成については読むだけで否定される**——
11.4 ms の中身ではありえない。そして実運用点（4,040 バイトの実 `history.bin`）では
**逆にその額が新たに乗る**。

**これは推論であって実測ではない**（temp に `history.bin` が在ったかを直接は確かめられない）。
Phase 1 の計器が履歴のエントリ数を出力に添えることで、以後この要因は統制下に入る。

### 2.8 `#1059` の spike 足場が撤去されずに残っている（**incidental**）

`snotra-core/src/search/tests/performance.rs:228` の撤去条件は
「本 issue の判定を記録した PR がマージされたら 6 つを消す」であり、**PR #1066 は
`2758340` でマージ済み**である。しかし `kmp_failure` / `advance_over` / `forward_pass` /
`parallel_sweep` / `sequential_sweep` / `spike_forward_pass_vs_parallel_sweep_over_real_index`
（`performance.rs:357`）は現存する。**撤去条件が発火済みなのに実行されていない。**

## 3. 関連ファイル・シンボル（すべて grep で実在を確認）

| パス | シンボル | 役割 |
|---|---|---|
| `snotra-core/src/search/scoring.rs` | `score_one_entry`（320） | マッチ後 3 成分がすべてここに在る |
| 〃 | `with_normalized_key`（79）/ `ScoredEntry::cmp`（138）/ `TopK::push`（253）/ `adjusted_history_boost`（193）/ `heap_into_results`（215） | ablation の切り口 |
| `snotra-core/src/search/path_store.rs` | `PathStore::cmp_paths`（256）/ `sorted_by_path`（119）/ `raw_into` | tie-break の高速路と遅い路 |
| `snotra-core/src/index_tree.rs` | `extend_with_roots`（732・761 で旗を下ろす） | 実運用点で旗が偽になる唯一の理由 |
| `snotra-core/src/indexer.rs` | `IndexMaterial::extend_with_path_entries`（1933）/ `load_or_scan_with_stats` | PATH マージの唯一の入口 |
| `snotra-core/src/history.rs` | `get_global_stats_normalized`（267）/ `query_count_pre_normalized`（290）/ `folder_expansion_count_normalized`（330）/ `prune`（344） | 履歴照合 3 種と剪定容量 |
| `snotra-core/src/engine.rs` | `Engine::search`（149 で mode 決定）/ `IndexInputs` | 実運用点の mode の出所 |
| `snotra-core/tests/path_query_cost.rs` | `measure_path_query_frame_cost`（184） | **1 バイトも変えない**既存計器 |
| `snotra-core/src/search/tests/path.rs` | `skipping_name_scoring_changes_nothing_over_real_index`（494） | 実 index 全件で集合・順序を突き合わせる**既存の型** |
| `snotra-core/src/search/tests/common.rs` | `real_index_entries`（32）/ `real_scanned_entries`（55） | 実 index の入力（取り違え注意・doc に警告あり） |
| `snotra-core/src/search/tests/performance.rs` | spike 6 点（228 の撤去条件・357） | 撤去条件が発火済み（§2.8） |
| `src-tauri/src/main.rs` / `src-tauri/src/indexing.rs` | 207 / 120 | PATH マージの製品側呼び出し点 |

## 4. 再利用できる既存パターン

- **実運用点の A/B を「同じエンジンの片方だけ最適化を殺す」形で作る**:
  `skipping_name_scoring_changes_nothing_over_real_index` が `any_name_has_path_sep` を
  強制的に立てて baseline を作る。**受け入れ条件「集合・順序とも 1 件も変わらない」は
  この型をそのまま複製すればよい**（`(name, path)` の列を順序ごと比較する。件数一致では足りない）
- **drift 対照列**（#1059 / #1032 の教訓）: A → B → A' を同一セッション同一機で測り、
  A 側の幅を明示する。`#1059` は A' を後から足したせいで A が +27% 動いたことまで記録している
- **足場は製品コードに 1 行も入れず、撤去条件を成果物自身の doc に持たせる**（#1059 の spike の型）。
  ただし撤去の合図は **issue 番号ではなくマージ済みの事象**（`scaffold-removal-condition-self-reference`）
- **計器の並存**: `measure_path_query_frame_cost` を据え置き、実運用点を再現する**別の計器**を
  足す。`RETROSPECTIVE.md`「計器の実運用点との乖離を、2 サイクル続けて『引き継ぐ』で送った」が
  「**3 サイクル目に入るなら、送るのではなく別計器を足す側へ倒す**」と明記している

## 5. 技術的制約

- `score_one_entry` / `TopK` / `PathStore` は `pub(super)` / private ゆえ、**crate 外の
  `tests/` から内部を分解した ablation は書けない**。`snotra-core/src/search/tests/performance.rs`
  は crate 内なので届く。一方 `Engine::search`（製品レベルの入口）は crate 外の
  `tests/path_query_cost.rs` にしか無い。**層をまたぐので、どちらに何を置くかは計画で決める**
- ablation は「製品コードの一部を殺して測る」ので、**変異を当てて測って戻す**形になる。
  `#1059` の 4 成分分解と同型で、**ちょうど決定系にすると独立検算が 1 本も無い**。
  過剰決定にする（単独 + 組合せを測って和の一致を検算する）ことが本 issue の改善点である
- `load_or_scan_with_stats` は**旧版の `index.bin` をその場で現行版へ書き換えうる**
  （`path_query_cost.rs` の `//!` が警告）。現行は v7 なので昇格は起きないが、
  実 `%APPDATA%` を直接読む計器を足すなら注意書きが要る
- `search/tests/` は `#[cfg(test)]` の crate 内テストであり、`HistoryStore::load()` の使用は
  計測ハーネスに限る（`snotra-core/CLAUDE.md`「開発ルール」・#963）
- 最終スコアに履歴ブーストが乗るので、**履歴照合を top-k の後へ遅延すると順位が変わりうる**
  （issue の「着手前に潰すこと 3」）。実 config は `history_normalization = "disabled"` ゆえ
  `adjusted_history_boost` は素通しだが、**boost 自体は消えない**

## 6. 未解決の疑問（計画の未確定欄へ送るもの）

1. **11.4 ms の内訳**。履歴照合 / `TopK::push` / tie-break のどれが支配的か。
   §2.4・§2.5 の概算は「どちらも小さい」と言っており、**残差の主が 3 成分の外に在る可能性**
   （§2.6 の `TopK::merge` / `into_sorted_vec`・`ScoredEntry` の構築）を潰していない。
   ablation の切り口に**「3 成分をすべて殺した残り」を必ず 1 列置く**
2. **実運用点（`substring` + PATH マージ ＝ `sorted_by_path = false`）の実額**。
   §2.2 のとおり 2 つの差が逆向きに効くので、符号すら予測できない
3. **max（18,642〜22,420 µs）の帰属**。ユーザー決定で受け入れ条件に入った。
   ばらつきの出所（rayon の task 分割・他プロセスの割り込み・cmp_paths の発火揺れ）を
   p50 と同じ ablation で分解できるか、標本数をいくつにするか
4. **PATH マージ後の件数**。`c:\` が「全件」でなくなるなら、その行の性格が変わる
5. **spike 足場の撤去**（§2.8）を本 PR に含めるか

---

## 7. 敵対的調査（Step 3b）の所見と採否

> サブエージェント 1 体（sonnet）の出力は `workspace/adversarial-1067.txt`。
> **採るのは所見であって、添えられた機序の説明ではない。**

サブエージェント 1 体（general-purpose / sonnet）。**壊せた 2 件・壊せなかった 4 件・⚠️ 3 件**を
両方宣言させた。以下は主エージェントによる裁定であり、**機序と数値は一次証拠で測り直している。**

### 採用した所見

**§7-1（争点 A・壊せた）— 「実運用は必ず旗が下りている」は前提条件を欠いた全称表現だった**

`IndexTree::extend_with_roots` の冒頭に `if entries.is_empty() { return; }` が在る
（`index_tree.rs:733-735`・**主エージェントが自分で読んで確認**）。`scan_path_env` は PATH が
空か `reject_existing` が全件を弾いたとき空 vec を返す（`indexer.rs:1772-1777`）。
**`AGENTS.md`「全称表現は前提条件とセットで書く」に自分で違反していた。** §2.2(a) を訂正済み。

- **添えられた数値は採らなかった**: 敵対枠は `HKCU\Environment\Path` を
  「非空 64,101 バイト・14 ディレクトリ」と報告したが、**主エージェントが PowerShell で
  読み直したところ 722 文字・16 ディレクトリ**だった。所見（非空である）は正しく、
  数値だけが誤っている。**`CLAUDE.md`「所見が正しくても、そこに添えられた機序の説明は
  独立に誤りうる」の実例**——採ったのは「早期 return が在る」であって報告値ではない
- **実害**: この開発機では旗は下りる側にあると見込まれる（§2.2(a) の導出）。
  だが**旗の実効値は Phase 1 の計器が出す**（未確定 1）

**§7-2（争点 D・壊せた）— 「worst 1 件としか比べない」は置換時の `sift_down` を数え落としていた**

`PeekMut` は `DerefMut` を経た場合に drop で `sift_down(0)` を走らせるため、**置換が起きた候補は
追加で O(log 200) 段の比較を払う。** §2.5 を訂正済み。

- **ただし「ゆえに `cmp_paths` が支配的」とは採らない。** 敵対枠自身が実害を ⚠️ に置いており、
  置換回数も `lower_name` 一致率も未測定である。**採ったのは「機序の記述が不完全だった」ことで
  あって、「tie-break が支配項である」ではない。** 決着は Phase 3 の `M_cmp` に委ねる

**§7-3（見落とし）— rayon の task 分割数は動的である**

`into_par_iter().fold` は work-stealing ゆえ task 数は固定 16 ではない。**§2.6 の
`TopK::merge` の額を「N × 200」の固定見積もりで語ってはならない。** 計画の Phase 3 に
「task ごとのコストを固定値で見積もらない」を反映した。

### 採用しなかった所見・壊れなかった項目

- **争点 B（履歴照合）— 壊れなかった。** `history.rs` の各行と `top_n = 200` は正確で、
  実 `history.bin` 4,040 バイトは**主張を補強する向き**だった。「16 コアで 1 ms 未満」の桁は
  ビルド無しでは検算できない旨の ⚠️ が付いたが、**それは Phase 3 が測る対象そのものである**
- **争点 C（引き算）— 壊れなかった。** むしろ支持が 1 つ増えた: `scoring.rs:461-463` のガードは
  `path_query = Some` の両行で素通りするので、**組み立て 3,204 µs の成分に限れば「両行で同額」に
  構造的根拠がある**。ループ素 888 の側は依然として独立検算不能——だからこそ Phase 3 の
  過剰決定の検算 (ii) を置く
- **争点 E（全件マッチ）— 壊れなかった。** PATH の 16 ディレクトリも全て `C:\` 配下
- **§2.7（spike 撤去漏れ）・§3 の行番号 15 か所 — 壊れなかった**（全件 grep 突合でずれ 0）
- **`TopK::new(201)` の混同疑い — 該当しない。** 現行の §2.6 は `TopK::new(limit)` と書いており、
  `BinaryHeap::with_capacity(limit + 1)`（`scoring.rs:247`）とは別の話である。
  敵対枠は編集前の版を読んだと見られる
- **`index.bin` 上の `sorted_by_path` 実値が未確認 — ⚠️ として受け入れる。** バイナリデコードで
  確かめる価値は低く、**Phase 1 の計器が実効値を出すことで解消する**（未確定 1）
