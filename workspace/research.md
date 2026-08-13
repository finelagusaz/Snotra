# research — #1039 行の差し替えと in-flight の失効を同じ型の内側へ入れる

対象 issue: #1039（`refactor(egui)` / rust / size:M）
ブランチ: `chore/apply-rows-invalidates-in-flight`
基準: main = 8ca9950（2026-08-13）

## 1. issue の要約

同じ出来事（行が総入れ替わる）に対する 2 つの義務が別々の型に住んでいる。

- **(A)** `rows_generation` を進める（クリック逆流の照合・#699）— `SearchState` の内側にあり、機構で守られている
- **(B)** `dispatch.invalidate()` で in-flight を失効させる（worker の遅着を捨てる）— `SearchDispatch` は `LauncherController` が持つので、`SearchState` のメソッドを**呼んだ側**が手で撃つ規約になっている

(B) が規約であるがゆえに漏れる。行を差し替える唯一の関数を作り、in-flight の照合と失効をその内側に入れる。

## 2. 母集団の再列挙（現 HEAD・LSP findReferences）

**issue 本文の「10 箇所」は現 HEAD では 11 箇所である。** issue は main = 23cba3c で ripgrep で数えており、`run_search_with` の **worker 送信失敗枝**（`search_tx.send(..).is_err()`）を列挙に含めていない。結論（漏れ 3 箇所）は変わらないが、以下を今回の事実とする。

### `SearchState::set_results` の production 呼び出し — 11 箇所

`LSP findReferences`（`search_state.rs:201`）が返した 44 件のうち、`search_state.rs` 内の 33 件はテスト。production は `launcher_controller.rs` の 11 件。

| # | 行 | 出所 | 直前の `dispatch.invalidate()` |
|---|---|---|---|
| 1 | 274 | `start_launch`（起動突入） | あり（273） |
| 2 | 434 | `clear_search` | あり（433） |
| 3 | 770 | Folder / error 行 | あり（769） |
| 4 | 774 | Folder / cache フィルタ | あり（773） |
| 5 | 787 | Results/Plain（空クエリ・indexing） | あり（786） |
| 6 | 806 | Results/Plain（**worker 送信失敗**） | あり（805） |
| 7 | 838 | Results/Instant | あり（837） |
| 8 | 856 | Results/Command（`/r` 履歴注入） | あり（855） |
| 9 | 859 | Results/Command（その他 → クリア） | あり（858） |
| 10 | 886 | `drain_search`（worker 採り込み） | **なし**（`accept` が pending を take するため正しい） |
| 11 | 1341 | `on_enter` の flush | あり（1340） |

`dispatch.invalidate()` の呼び出しは 11 箇所（273 / 433 / 769 / 773 / 786 / 805 / 837 / 855 / 858 / **983** / 1340）。983 は `consume_reset_pending` で、`set_results` ではなく `state.reset()` と対になっている。

### `SearchState.results` を `set_results` を通さず直接書き換える経路 — 4 箇所

| 経路 | 行 | (A) `rows_generation` | (B) in-flight 失効 |
|---|---|---|---|
| `enter_tool` | search_state.rs:339–341 | 進める | **なし**（呼び出し側 `shift_activate` は `search_debounce.cancel()` だけ撃ち `dispatch` を触らない） |
| `on_escape`（tool 復帰） | search_state.rs:363–366 | 進める | **なし** |
| `on_escape`（folder 復帰） | search_state.rs:371–375 | 進める | **なし** |
| `reset` | search_state.rs:391–397 | 進める | 呼び出し側が撃つ。**呼び出し点は 2 つある**（`rg "state\.reset\(\)"` 実測）——`consume_reset_pending`（982）は直後の 983 で自分で撃ち、`finish_launch` の `LaunchTag::Tool` 成功枝（354）は**直前の `clear_search()`（353）が 433 で撃つ**。ゆえに (B) は 2 経路とも守られているが、**成立根拠が 2 か所に分かれている**（Step 2b の B-1・自分で再照合して成立） |

**(A) は 15 箇所すべてで守られ、(B) は 3 箇所で漏れている。**

### 漏れの帰結（潜在バグ・未再現）

`drain_search`（868–898）に **view 種別のガードは無い**。`accept` が seq 一致を返せば `self.state.set_results(results)` を無条件に撃つ。ゆえに:

- **`enter_tool` の後に遅着した plain 結果がツール行を上書きしうる**。`SPEC.md` §18.5「ツール選択中は検索結果を上書きしない」に反する。窓は `shift_activate` → `drain_search` の間で、Shift+Enter が `on_enter` の flush を通れば `invalidate` が撃たれるので、開くのは flush が発火しない条件（＝ #1038 の後は armed でも in-flight でもないとき）に限る
- **`on_escape` の復帰行を遅着結果が上書きしうる**（tool 復帰 / folder 復帰の両方）

`docs/architecture.md`「検索フロー」補足（230 行）が**この 2 つを「未再現の観察」として記録済み**である。本 issue はその観察を機構で閉じる。

## 3. 関連ファイル・モジュール・シンボル

| ファイル | 触る対象 |
|---|---|
| `src-tauri/src/egui_shell/search_state.rs` | `SearchState`（フィールド・`set_results` / `enter_tool` / `on_escape` / `reset`）、`rows_generation` の doc、`should_flush_on_enter`（417） |
| `src-tauri/src/egui_shell/search_dispatch.rs` | `SearchDispatch` / `Settled` / `is_unsettled`（79）とその doc・テスト |
| `src-tauri/src/egui_shell/launcher_controller.rs` | `dispatch` フィールド（134）、11 箇所の `invalidate` + `set_results`、`drain_search`（868）、`on_enter`（1314）、`consume_reset_pending`（974） |
| `src-tauri/src/egui_shell/mod.rs` | 88–89 行の re-export（`SearchDispatch`, `is_unsettled`） |
| `src-tauri/src/egui_shell/results_view.rs` | `RowsSnapshot::input_idle` の doc（36 行・`is_unsettled` の式からの導出 1 行） |
| `docs/architecture.md` | 「検索フロー（入力 → 結果表示）」の mermaid（`Disp` participant・invalidate 矢印）と補足 229–230 行 |
| `src-tauri/CLAUDE.md` | `egui_shell/` のファイル索引（**ファイルの増減が無ければ変更不要**） |

### 消費者の列挙（LSP findReferences・grep ではない）

- `is_unsettled`（`search_dispatch.rs:79`）: production は **`launcher_controller.rs:1322` の 1 箇所だけ**。他は `mod.rs:89` の re-export と自ファイルのテスト 9 件
- `SearchDispatch`（`search_dispatch.rs:23`）: production は **`launcher_controller.rs:134`（フィールド宣言）の 1 箇所だけ** + `mod.rs:89`
- `set_results`（`search_state.rs:201`）: 上表の 11 箇所 + テスト 33 件

## 4. 既存パターン（再利用できるもの）

- **`SearchDispatch` は既に「純粋核」を名乗り、`now` を引数で受ける形で `Instant` を扱っている**（`search_dispatch.rs` の `//!` と `issue`（30 行）/ `accept`（41 行）のシグネチャ、`Pending { key_at, dispatched_at }`）。**ゆえに issue の未決 1「`Instant` ベースの計時が純粋核に馴染むか」は既に解けている**——`SearchState` へ丸ごと内包しても、**保たれるのは「時計を内部で読まない」性質**であり、テストは合成 `Instant` で全遷移を駆動できる。同じ流儀は `rescan-in-situ-instrument-design.md`「純粋核 — 時計を持たない」（「時計と I/O は呼び出し側が持つ」）が別モジュールで宣言している。**ただし「異物が入らない」とは書かない**——`search_state.rs` に `std::time::Instant` は現状ゼロで、時刻を保持するフィールドはこの型に**初めて生まれる**（§8 (2)）。計時を seq から引き剥がす必要が無いので issue の未決 2（`search_dispatch.rs` の移設・吸収コスト）も同時に消える（モジュールも dispatch のテストもそのまま残る）
- **`clamp_selected`** — `set_results` / `on_escape` が既に共有する選択クランプ
- **`BlurGrace::reset` の先例**（#745）— show を跨ぐ状態を `*self = Self::default()` で丸ごと畳み、`consume_reset_pending` から呼ぶ形。**今回 `armed` を移送しないならこの先例は要らない**（下の決定 2）
- **`#[must_use]` の配置規約**（`src-tauri/CLAUDE.md`「処置を返す純粋核の強制」）— `Option<T>` を返すものは型段が効かないので**メソッド段必須**

## 5. 技術的制約と、それに基づく設計の決定

### 決定 1: `SearchDispatch` を `SearchState` の private フィールドとして内包する

`search_dispatch.rs` は残す（モジュール・型・テストとも）。`SearchState` が `dispatch: SearchDispatch` を private に持ち、外へは `SearchState` のメソッドだけを見せる。`LauncherController` から `dispatch` フィールドを削除する。

- 行の差し替えを担う private ヘルパ `put_rows(rows, next_selected)` が **results 代入・`clamp_selected`・`rows_generation += 1`・in-flight 失効**を 1 か所で行う。`set_results` / `enter_tool` / `on_escape`（2 枝）/ `reset` の 5 経路をここへ合流させる
- **選択の方針は経路ごとに違う**（`set_results` は現 `selected` を clamp、`enter_tool` と `reset` は 0、`on_escape` は `restore_selected` を clamp）ため、`put_rows` は「次の selected」を引数で受ける。issue のスケッチにある `Origin` enum は採らない（下の「issue のスケッチとの差異」）

### 決定 2: `armed` の disjunct は引数で受ける（#1038 コメントの選択肢 1）

`SearchState::is_unsettled(&self, armed: bool) -> bool { armed || self.dispatch.pending_seq() != 0 }`。

- **`Debouncer` を `SearchState` へ移送しない。** 移送すると #1038 コメント項目 4 の罠（`consume_reset_pending` が `Debouncer` を**丸ごと作り直す**ことで `armed` と `last_input_at` の取り残しを同時に救っている構造）を一緒に運ぶことになり、`last_input_at` を忘れると show 直後の初フレームで `poll` が「隠れていた時間」を経過として読み trailing を撃つ
- **極性は否定形を維持する**（issue 本文の `is_settled()` ではない）。消費者は `on_enter` の `should_flush_on_enter` 第 3 引数 1 つだけで、肯定形にすると呼び出し点に `!` が出る。#1038 が既に下した判断であり、同 comment が「決めるのはこの issue 側でよい」と委ねている。**さらに Step 2b が肯定形を採った結果として副作用を 1 つ明らかにした**——肯定形にすると `results_view.rs:36` の導出行「食い違うのは `armed == false ∧ pending != 0` のとき」の**前置き（「`is_unsettled` から導いた」）が読めなくなり、式ごと書き直しになる**。否定形を保てば #1074 の申し送り 2 は「リンク先を張り替えて読み直す」だけで済む。**採否と理由をここに残す**
- issue の「副次的に閉じるもの」（`is_settled()` が「反映済みか」の持ち主になり `RowsSnapshot.settled` の出所も生まれる）は **#1074 が別解で閉じ済み**。`input_idle` は正しさの述語ではなく perf ヒューリスティック（`!armed`）で、`is_unsettled` とは否定の関係にも無い。**この副次効果の主張は失効している**——`input_idle` に本件の修正を当ててはならない

### 決定 3: 採り込み点に view 種別のガードを置く

`accept_worker_rows(seq, rows, now) -> Option<Settled>` は **seq 一致 ∧ `view_kind() == ViewKind::Results`** のときだけ行を差し替える。

- **`put_rows` の失効だけでは `enter_folder` / `navigate_folder` を覆えない**——この 2 つは行を差し替えず（`restore_results` へ現在行を退避して `self.results` はそのまま）、in-flight を残す。folder 突入後〜列挙到着の窓で遅着 plain 結果が seq 一致で採られ、folder view に別の plain 行が 1 窓ぶん出る並びが**現行でも新設計でも通る**
- ガードを採り込み点に置けば、この窓も `enter_tool` の窓も**手書きの `invalidate` 無しで**閉じる。Folder / Tool ビュー中は `run_search_with` が plain 検索を発行しないので、そこで in-flight にいる結果は必ず遷移前のものであり、常に不要である
- 起動をブロックする既存のガード（`folder_load_pending`）とは別軸。**`folder_gen` / token による folder の識別には触らない**（issue の「folder は触らない」を守る）

### issue のスケッチとの差異（採否と理由）

| issue の提案 | 本計画 | 理由 |
|---|---|---|
| `apply_rows(origin, rows)` + `enum Origin { Sync, Worker(seq) }` | private `put_rows(rows, next_selected)` + public `set_results` / `accept_worker_rows` | 選択の方針が経路ごとに違い、`Origin` だけでは決まらない。private ヘルパでも「型の内側の単一チョークポイント」という構造的効果は同じで、`Origin::Sync` を全同期呼び出し点に書かせる公開面が増えない |
| `in_flight: Option<u64>` を直に持つ | `SearchDispatch` を丸ごと内包 | 計時（`key_at` / `dispatched_at`）と seq が分かれると `Settled` の導出が 2 型に跨がる。`SearchDispatch` は既に純粋核なので内包に追加コストが無い |
| `fn is_settled(&self) -> bool` | `fn is_unsettled(&self, armed: bool) -> bool` | 上の決定 2 |
| 「`is_settled()` が `RowsSnapshot.settled` の出所にもなる」 | 採らない | #1074 で `input_idle` として別概念に確定済み（決定 2 末尾） |

## 6. 検証・トリガー（`AGENTS.md` 条件別チェック）

| トリガー | 参照先 | 該当 |
|---|---|---|
| worker・channel・フレーム drain・共有状態を変更 | `/race-check` | 該当するが**計画段階では起動しない**（スキル本文が #784 を根拠に自ら禁じている）。`/implement` で実施する |
| 対称ペア（生成/破棄・issue/失効） | `/symmetric-check` | 計画段階で実施済み（結果は plan.md の不変条件 7・8 と `accept_worker_rows` の順序） |
| 関数・型を新規定義／改名／導入 | LSP findReferences ＋ `/dry-check` | findReferences は §3 で実施済み。`/dry-check` は実装後（新関数の実体が要る） |
| 重複した読み・冗長に見える状態を束ねる | 各箇所について「後で読まれることに依存していないか」を 1 行ずつ | 11 箇所の `invalidate` を消す作業そのもの |
| ガバナンス文書（`*.md`）を変更 | `npm run governance:check` | `docs/architecture.md` を変更する |
| doc コメント（`///` / `//!`）を変更 | `cargo doc --workspace --no-deps --document-private-items`（**hook は沈黙する**） | intra-doc link を多数張り替える |
| 機能削除・trace イベント名の変更 | `scripts/smoke-egui.ps1` の前提 | **該当しない**——`egui_search:settled` / `egui_search:dropped` の名と payload は保つ |

**`SPEC.md` の更新は不要**。挙動差分（tool / folder ビュー中に遅着結果を採らない）は §18.5「ツール選択中は検索結果を上書きしない」に**合わせる**方向であり、記述を変えない（`AGENTS.md` 開発ワークフロー 1 の判定）。

## 7. 未解決の疑問（plan.md の未確定欄へ引き継ぐ）

1. ~~`LauncherController` から `dispatch` を消したとき、`drain_search` の trace（`pending_seq`）が読む値をどう供給するか~~ → **解決**: `SearchState::pending_seq()` を `pub` で出す（消費者は trace の payload だけ）。`Settled` へ含める案は採らない——`accept` 後は必ず 0 であり `Settled` に載せると同じ値の出所が 2 つになる。**なお敵対枠が確認したとおり、供給し忘れても smoke は SKIP へ落ちて偽の PASS にはならない**（`SnotraTraceInvariants.psm1` の冒頭原則）——ゆえにこれは合否の問題ではなく検知力の問題である
2. `accept_worker_rows` の view ガードで捨てたとき、`egui_search:dropped` の payload を seq 不一致と区別するか（`dropped` を読む検査はリポジトリ内に 1 つも無いので後方互換の制約が無い）
3. ~~自由関数 `is_unsettled` を残すか削除するか~~ → **解決**: 削除して `SearchState::is_unsettled` へ一本化する（§8 の決定 2 追記）
4. 変異注入（`armed` の disjunct を落とす / `put_rows` の失効を落とす / view ガードを落とす）で実際にテストが落ちることの実測

## 8. 敵対的調査（3b）の結果

全文は `workspace/adversarial-1039.txt`。**7 争点のうち壊せたのは 0 件**、⚠️ が 2 件。以下は主エージェントによる裁定（**採るのは所見であって、添えられた機序の説明ではない**）。

### 壊せなかった項目（主張が持ちこたえた）

| 争点 | 検算の道具 | 結果 |
|---|---|---|
| `drain_search` に view ガードが無い | `drain_search` 全文 + 呼び出し元 `view.rs:1077` 前後の読了 | 持ちこたえた |
| production の `set_results` は 11 箇所 | **ripgrep**（LSP とは別の道具）+ **`git log 23cba3c..8ca9950`** | 持ちこたえた。差分の理由が `aba1356`（#1053・worker 送信失敗検知）の介在であることまで独立に裏付いた |
| `is_unsettled` の production 消費者は 1 箇所 | grep 15 件の全数分類（呼び出し 1・re-export 1・doc 名指し 3・テスト 9・その他 1） | 持ちこたえた |
| `input_idle` と `is_unsettled` の分離は意図的／式は腐らない | **自分で式を立てた検算**（doc の転記ではない） | 持ちこたえた |
| `enter_folder` の窓は現行と等価・view ガードで閉じる | `on_nav_keys` と `run_search_with` の全文読了 | 持ちこたえた。**`search_tx.send` を呼ぶのは Results/Plain 分岐だけ**であることが独立に確認された |
| `SPEC.md` の更新は不要 | §18.5（820 行）と §6.3 の読了、`accept_folder_result` の先例 | 持ちこたえた |
| smoke の H7 は壊れない | `SnotraTraceInvariants.psm1` 378-433 行の読了 | 持ちこたえた。**さらに強い事実**——`pending_seq` を供給し忘れても null は SKIP へ落ち、偽の PASS にはならない設計（同 psm1 の冒頭原則） |

### 採った所見

**(1) 漏れは「理論上の窓」ではなく表示まで貫通する**（争点 1 の付随証拠・**採用**）

`plain_results_hidden`（`search_state.rs:476-478`）は `indexing && Results && !instant_rows` であり、**Tool ビューでは常に false**。ゆえに Tool ビュー中に `drain_search` が行を上書きしても、表示ゲートは隠さない。**自分で読んで確認した。**

- 機序の訂正: 敵対枠は「`show_results` 側にも隠す仕組みが無い」と書いたが、`plain_results_hidden` はそもそも **indexing 中に plain 結果を隠すゲート**であって view 種別ごとの上書き防止ではない。「隠さない」のは責務外だからであって欠落ではない。**所見（描画まで届く）は正しいので採り、機序の言い回しは採らない**

**(2) 「純粋核へ新しい異物は入らない」という言い回しは弱める**（争点 2 ⚠️・**採用**）

`search_state.rs` に `std::time::Instant` は現状ゼロ（`Instant` の文字列一致は全て `QueryIntent::Instant`）。内包すると**時刻を保持するフィールドがこの型に初めて生まれる**のは事実。§4 と §5 決定 1 の表現を次へ改める。

> **保たれるのは「時計を内部で読まない」性質である。** `SearchDispatch::issue` / `accept` は `now: Instant` を引数で受けており（`search_dispatch.rs:30, 41`）、`SearchState` の新メソッドも同じ形で受ける。ゆえにテストは合成 `Instant` で全遷移を駆動でき、時計への依存は増えない。

- 機序の訂正: 敵対枠は `rescan-in-situ-instrument-design.md` §2.4「純粋核 — 時計を持たない」を**対抗する先例**として挙げたが、**一次証拠を読むとこれは同じ流儀である**——同節は「**時計と I/O は呼び出し側が持つ**」「区間は呼び出し側が測って渡す」と書いており、`SearchDispatch` が `now` を引数で受ける形とまったく同じ性質を指している。**「定義がモジュールごとに違う」は誤りであり、採らない**

**(3) 自由関数 `is_unsettled` の去就を research.md にも明記する**（争点 5 ⚠️・**一部採用**）

`plan.md` には既に「自由関数とそのテスト 2 件を削除し `SearchState` 版として移設する」と書いてあるが、`research.md`（敵対枠へ渡した資料）の決定 2 は去就を書いていなかった。**§5 決定 2 へ明記する**（下の追記）。

- 機序の訂正: 敵対枠は「自由関数を消すと `unsettled_is_grounded_on_real_dispatch` の接地が書けなくなり、残すと 2 入口が並立するトレードオフ」と書いたが、**これは誤りである**。`SearchDispatch` は型として残る（決定 1）ので、`SearchState` は `issue_search` / `accept_worker_rows` / `pending_seq` で実物の dispatch を駆動できる。接地は「実物の `SearchState` を遷移させ、`is_unsettled` の値を測る」形で保たれ、2 入口は生じない。**トレードオフは存在しない**
- ただし**採る所見**が 1 つある: intra-doc link `[crate::egui_shell::search_dispatch::is_unsettled]`（`results_view.rs:36`）は自由関数の削除で**解決先を失い `cargo doc` が落ちる**。plan.md の「文書の同期」3 が既に張り替えを指示しているが、**`cargo doc` は hook が走らせない**ので Phase 3 の作業項目として明示的に走らせる

### 決定 2 への追記（去就の明記）

**自由関数 `search_dispatch::is_unsettled` は削除する。** `SearchState::is_unsettled(&self, armed: bool)` へ一本化し、テスト 2 件（`unsettled_covers_in_flight_after_trailing_fired` / `unsettled_is_grounded_on_real_dispatch`）も `search_state.rs` へ移設する。`search_dispatch.rs` には `SearchDispatch` / `Settled` と dispatch 自体のテスト 4 件が残る。**同じロジックの入口は 1 つだけになる。**
