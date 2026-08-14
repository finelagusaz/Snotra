# 調査: #1079 folder 往復で `is_unsettled` が偽になる

## issue の要約

`SearchState::is_unsettled` は「最終クエリの結果がまだ行へ反映されていないか」を返す述語である（#1038）。folder を `→` / `←` で往復すると、その意味に**反して偽**を返す状態が作れる。結果として復帰直後の Enter に #631 の flush-on-Enter が掛からず、復元した query の最終結果ではない行で起動しうる。

issue は #1039（`693c3e7`）の code-reviewer が挙げた受容残余を記録したもので、**「現時点では直さない」と決めて doc へ書いた判断**そのものが本体である。2026-08-14 にユーザーが「直す」を選択した（本調査の Step 1）。

## 並び（一次証拠つき・全段が `SearchState` のメソッドである）

| # | 出来事 | 一次証拠 |
|---|---|---|
| 1 | 打鍵 → worker へ seq S を発行。`pending_seq == S` | `search_state.rs:477` `issue_search` / `launcher_controller.rs:784-799` |
| 2 | `→` / `←` で folder 突入。`enter_folder` は行を退避するだけで `put_rows` を通らない → in-flight は残る | `search_state.rs:285-299`（doc が「行は差し替えない」と明記）/ 呼び出しは `launcher_controller.rs:1192, 1224` の 2 箇所 |
| 3 | Escape → folder 枝が `put_rows(f.restore_results, ..)` で**展開前の行**へ復帰。**`put_rows` が `dispatch.invalidate()` を無条件に呼ぶので、ここで in-flight S が消える** | `search_state.rs:438-444`, `226-231` / `search_dispatch.rs:68`（`self.pending = None;`） |
| 4 | `armed == false ∧ pending == 0` ゆえ `is_unsettled` は偽。`should_flush_on_enter` が偽を返し flush が走らない | `search_state.rs:568-569`, `589-591` / `launcher_controller.rs:1325-1329` |

**この 3 段はすべて `SearchState` の公開メソッドである。** issue が「検知手段は無い／controller 側の配線をまたぐ」と書いたのは**修正を案 A / 案 B に置いた場合の話**であり、**欠陥そのものは純粋核の中で表現できる**（下記「検知器」）。

### issue の並びは過剰仕様である（敵対的調査 3b の所見・採用）

issue と `on_escape` の doc は段 3 に「S の結果が folder 中に届き、`accept_worker_rows` の view ガードが捨て、`accept` が pending を take するので `pending_seq` は 0 になる」を置く。**この段は要らない。**

`on_escape` の folder 枝が呼ぶ `put_rows` は `self.dispatch.invalidate()` を無条件に呼び（`search_state.rs:230`）、`invalidate` は `self.pending = None;` を無条件代入する（`search_dispatch.rs:66-68`。**自分で読んで裁定した**——所見は採るが機序は独立に確かめる）。**worker の結果が届いたかどうかは結果に影響しない。**

**帰結は再現性である。** issue の受容理由は「稀で低害な並び」を根拠の一つに置くが、**worker との時間的な競合は要件ではない**——in-flight のまま `→` を押して Escape で戻れば、遅着の有無によらず毎回成立する。到達には依然として可視の一覧で `→` / `←` を押す必要があるが、**「タイミングが噛み合ったときだけ」ではない。**

## 関連ファイル・シンボル

- `src-tauri/src/egui_shell/search_state.rs`
  - `SearchState::is_unsettled`（:568）— 述語の正本。doc が :524-567
  - `SearchState::on_escape`（:430）— 受容理由の正本 doc が :411-429
  - `SearchState::put_rows`（:226）— **行の差し替えの単一チョークポイント**（#1039）。`results` 代入・`selected` クランプ・`rows_generation` 前進・`dispatch.invalidate()` を必ず同時に行う
  - `SearchState::set_results`（:235）/ `reset`（:457）/ `enter_folder`（:288）/ `navigate_folder`（:305）/ `enter_tool`（:363）/ `accept_worker_rows`（:498）
  - `should_flush_on_enter`（:589）— `view_kind == Results && is_plain && unsettled`
- `src-tauri/src/egui_shell/launcher_controller.rs`
  - `on_nav_keys`（:1166）— `enter_folder` / `navigate_folder` の唯一の呼び出し点（`→`:1192 / `←`:1224）
  - `on_escape_pressed`（:1106）— `RestoredSearch` 枝は cache / error / `instant_rows_query` のクリアと repaint だけ
  - `on_enter`（:1320）— flush の実装。`engine.lock()` + `engine.search` を**同期で**撃つ（:1337-1341）
  - `run_search` / `run_search_with`（:749 / :754）
  - `instant_prefix`（:660）— `engine.lock()` を取る
- `src-tauri/src/egui_shell/mod.rs`
  - `read_config`（:407）— **UI が config を読む唯一の口**（#1032）。`engine.lock()` を経ない

## 一次証拠で裁定した事実（issue の記述と食い違うもの）

### (1) 案 A の却下理由は現在のコードに当たらない ★

issue と `on_escape` の doc は案 A の費用を「Escape のたびに同期 `engine.search` をフレームへ乗せる」と書く。**`run_search()` はそれをしない。**

`run_search_with` の `QueryIntent::Plain` 腕（`launcher_controller.rs:775-799`）は #1004 の worker 化以降、`self.search_tx.send(SearchRequest { seq, query })` を撃つだけである。同期 `engine.search` が残っているのは `on_enter` の flush（:1337-1341）であって `run_search` ではない。

ゆえに案 A は 2 通りに読める。**却下理由が当たるのは後者だけである。**

- **A-async**（`run_search()` を呼ぶ＝案 A の字面）: worker への `send` 1 回。しかも `issue_search` が `pending_seq != 0` にするので、**Escape のフレームから `is_unsettled` が真になり**、直後の Enter は `on_enter` 自身の同期経路で flush される。欠陥は治る
- **A-sync**（同期 `engine.search` をフレームへ乗せる＝却下理由が想定した実装）: doc の費用記述どおり

### (2) 案 A の実際の残余費用は `instant_prefix` の `engine.lock()` である（ただし新規ではない）

`run_search()` は入口で `self.instant_prefix()`（:660）を呼び、そこが `engine.lock()` を取る。worker は `engine.search` の間じゅう同じ錠を握る（実運用点で 40〜95 ms・#1032）ため、Escape のフレームがそこで待たされうる。**これは案 A が持ち込む費用ではなく、既に存在する #1032 残余である**（`run_search()` は trailing poll / folder drain / `on_enter` から既に呼ばれている）。`read_config`（`mod.rs:407`）へ寄せれば錠を経ずに読める。

### (3) 案 A / A-async は indexing 中に復帰行を消す

`run_search_with` の Plain 腕は `self.indexing()` が真なら `set_results(Vec::new())` して早期 return する（:778-781）。folder を開いている最中に config 変更で再構築が kick されると（`config_watcher` 経由）、Escape が復帰行を**空にする**。案 A を採るならガードが要る。

### (4) tool 経路には同じ欠陥が無い

`enter_tool` は `put_rows` を通る（:391）ので in-flight を失効させるが、**唯一の呼び出し点 `shift_activate` は `on_enter` からしか呼ばれない**（`launcher_controller.rs:1351`・grep 実測で他に呼び出し点なし）。`on_enter` は flush を**先に**済ませる（:1325-1348 → :1349-1354）ため、退避される行は最終クエリの結果である。issue の「既知の状態はこれ 1 つ」はこの点で保たれている。

## 再利用できる既存パターン

- **`put_rows` チョークポイント**（#1039・`1fac4e6`）: 「行が差し替わった」ことに伴う義務をこの 1 メソッドへ集める設計。行と結び付く新しい状態を足すなら、その clear は原理的にここへ入る（`reset()` も `put_rows` を通るので show を跨ぐクリアが構造で付いてくる）
- **`SearchState` のユニットテスト**: `search_state.rs` の `#[cfg(test)]` に `is_unsettled` の合成を固定するテスト群が既にある（:1005-1048、`pending_seq_separates_the_two_drop_reasons`:1084 ほか）。`Instant` を渡すだけで構築でき、`AppHandle` を要さない
- **`read_config`**（`mod.rs:407`）: engine lock を経ない config live-read

## 技術的制約

- **`launcher_controller.rs` に `#[cfg(test)]` は 1 つも無い**（grep 実測）。`LauncherController` は `tauri::AppHandle` を持つためユニットテストのハーネスが無い。**controller に置いた修正は、ユニットテストでは検知器を書けない**
- smoke の trace 不変条件は H1 / H4 / H5 / H7 の 4 つ（`scripts/lib/SnotraTraceInvariants.psm1:32`）。H7 は `egui_search:settled` の `dispatch_seq < pending_seq` しか見ず、この並びを表現できない
- **`armed` は `SearchState` の外にある**（`layout::Debouncer` が持つ）。`is_unsettled` は引数で受ける規約であり、機構ではない（:540-545）
- `is_unsettled` の doc は「`armed` 側の状態をこの型へ移すなら reset 経路で落とす責務も一緒に移ること」と警告する（:546-558）。**新しいフィールドを足すときも同じ懸念が当たる**——ただし `put_rows` で clear すれば `reset()` が構造的に覆う
- **doc の訂正は結論によらず必須である**: `on_escape` の doc（:423-424）が述べる案 A の費用は現在のコードに無い。この repo の規範（`AGENTS.md`「主張は代理ではなく対象そのもので測ってから書く」）に照らして、どの案を採っても訂正が要る

## 敵対的調査（Step 3b・sonnet 1 体）の採否

出力: `workspace/adversarial-1079.txt`。争点 5 件のうち **1 件が壊れ、4 件は壊れなかった**。

| 争点 | 結果 | 採否 |
|---|---|---|
| 1. `run_search()` は同期 `engine.search` をしない | 壊せず（全腕を通読、該当 0 本） | 維持 |
| 2. 並びは `SearchState` 内で再現できる | **部分的に壊れた**——命題自体は真だが、5 段は過剰仕様で **3 段**で足りる | **採用**（上表を 3 段へ書き換え。機序は `search_dispatch.rs:66-68` を自分で読んで裁定した） |
| 3. tool 経路に同じ欠陥は無い | 壊せず（`enter_tool` ← `shift_activate` ← `on_enter` の 1 本鎖。クリックは `view.rs:1146` が `activate_or_execute` を直呼びし `enter_tool` を経ない） | 維持 |
| 4. controller にテストハーネスが無い | 壊せず（`#[cfg(test)]` 0 件・`tests/` 無し・tauri の test feature 未宣言） | 維持 |
| 5. 案 A は indexing 中に復帰行を空にする | 壊せず（`config_watcher` の kick は view_kind を見ない＝構造的に到達可能） | 維持 |

**⚠️ として返った 2 件の扱い:**

- 「案 A の `instant_prefix` 費用を『既存の #1032 残余』と位置づけるのは、folder-Escape という**新しい呼び出し点**での頻度増を過小評価しうる（実測なし）」——**採用**。案 A を採らないので実測は不要だが、却下理由の記述に反映する
- 「クリック経路が flush 対象外なのは意図的設計か射程外かの一次資料が見つからない」——**保留**。#1079 の射程外（本 issue は `→`/`←`→Escape の並びに限る）。ここで裁定しない

**research.md に無い指摘として返った 1 件:** 「案 C のフラグは『folder から復帰した』ではなく『`put_rows` 直前に `pending_seq() != 0` だったか』を捕まえる方が単純で正確かもしれない」——**却下**。`put_rows` は同期検索の結果を入れる経路（`on_enter` の flush・instant・command）も通り、そこでは `pending_seq() != 0` でも**入る行は現クエリの結果である**。この条件では flush 結果を stale と誤判定する。ただし「捕まえるべきは folder という場所ではなく未反映という状態である」という含意は正しく、下の設計（`enter_folder` 時点の `is_unsettled` を frame へ控える）がそれを実現する

## 検知器（issue が「修正前に落ちることを測れ」と課したもの）

上の並び 3 段が全部 `SearchState` のメソッドである以上、**`search_state.rs` のユニットテスト 1 本で再現できる**:

```
issue_search → enter_folder → on_escape → assert!(s.is_unsettled(false))  // 現状は偽 = 落ちる
```

**この検知器が有効なのは、修正が `SearchState` の内側にある場合（案 C）だけである。** 案 A / 案 B は controller が補償する形なので、修正後も `SearchState` は偽を返し続け、この assert は緑にならない。案 A / B を採るなら検知器は smoke の trace 不変条件を新設する必要があり、費用が跳ね上がる。

## 未解決の疑問

1. 案 C のフラグを**どの条件で立てるか**——無条件（folder から復帰したら常に）か、`enter_folder` 時点の `pending_seq != 0` を `FolderFrame` へ控えるか。前者は「folder を開いて何も走っていなかった」場合まで Enter に同期 search を乗せる
2. 案 C のフラグを **clear する場所**が `put_rows` で足りるか——`on_escape` は `put_rows` を呼んだ**後**にフラグを立てる順序になる
3. `navigate_folder`（folder 内の深掘り・親移動）は `enter_folder` と違い frame を作り直さない。往復の途中で `→` を重ねた場合にフラグの根拠が保たれるか
4. `is_unsettled` の doc（:563-567）が「式ごと書き直す必要が生じる」と書く箇所に、フラグの合成が触れる
