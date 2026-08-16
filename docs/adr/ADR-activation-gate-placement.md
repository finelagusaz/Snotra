# ADR-activation-gate-placement: 表示ゲートを起動へ効かせる位置と、`indexing` を配る型

#1077 で「画面に出ていない通常結果を Enter / クリック / Shift+Enter が起動する」を塞いだ。ゲートを置ける場所も、`indexing` を配る形も複数あった。ここに残すのは、**採らなかった案とその理由**である（採った形はコードと `SPEC.md` §4.7 が持つ）。

## 文脈

インデックス再構築中は `SPEC.md` §4.7 の表示ゲート（`search_state::plain_results_hidden`）が通常結果を results 窓から隠すが、**行データと選択は保持する**（「クリア案」は SU6 で却下済み——SolidJS 非 parity・instant carve-out 破壊・bool エッジのパルス見逃し。正本は `docs/superpowers/specs/2026-07-21-phase2-softbuffer-migration-roadmap.md`）。

一方、起動側のガードは `if !self.state.results().is_empty()` だけで表示ゲートを参照していなかった。`SearchState::is_unsettled` が偽（打鍵が落ち着いた状態）の Enter は `on_enter` の flush 枝を通らないため、隠れたままの行が起動しうる。#1072 が塞いだのは同じ族の unsettled 側の切片だけだった。**競合を要さない**——2026-08-16 に使い捨てプロファイルで再現した（`egui_results:hide` の 1.4 秒後に `egui_launch`）。

## 決定

- ゲートは **`activate_or_execute` と `shift_activate` の冒頭**に置く
- 判定は**表示側と同じ述語**（`plain_results_hidden`）を呼ぶ。同義の別式を作らない
- `indexing` は `view.rs` が 1 フレーム 1 回だけ読み、**`FrameIndexing`**（`window_coordinator.rs`・フィールド private・構築子は `read_indexing` ただ 1 つ）として配る
- 止めるのは**不可逆な起動だけ**である。行は消さず、フォルダ展開（→ / ←）も止めない

## 検討した代替案と却下理由

### 1. `start_launch`（起動の合流点）へ置く

`start_launch` は 3 経路（`activate` / `execute_instant_selected` / `execute_tool_selected`）の合流点で、そこへ置けば**将来足される起動経路も自動で覆える**。構造としては最も強い。

**それでも却下した。** ガードの意味は「選んだ行が、いま画面に出ている行ではない」であり、**行の選択の性質**である。`start_launch` へ届く時点で引数は `LaunchWork`（path / query / tools）へ解決済みで、行そのものはもう無い。判定に要る `view_kind` / `instant_rows` を `&self` から読み直すことになり、「解決済みの起動要求」の層で行の話を蒸し返す形になる。

**却下しても数は減らない**という事実も効いた——`shift_activate` の `tools >= 2` 枝は `start_launch` を通らず `SearchState::enter_tool` を直接呼ぶ（LSP の findReferences で確認）ため、どちらの案でも `shift_activate` には個別のガードが要る。

### 2. `activate`（`folder_load_pending` の隣）へ置く

**一度この案を計画に書き、独立導出レビューの指摘で覆した。** 同型の既存ガード `folder_load_pending`（「列挙未着の間は前ビューの行を起動しない」）が `activate` に在るので、隣に並べれば 2 つが常に一緒に読まれる。

**却下の理由は射程である。** `activate` は `activate_or_execute` の plain 枝でしかなく、**行 index を受け取る層の合流点ではない**。クリック逆流（`view.rs` の `take_clicked_for`）と `shift_activate` の `tools <= 1` 委譲は `activate_or_execute` を通るので、そこへ置けば 1 か所で覆える。`plain_results_hidden` は tool ビュー・instant 行では**構造的に偽**（`Results ∧ !instant_rows` を条件に持つ）なので、上の層へ移しても他経路を阻害しない。

### 3. 「ゲート ∧ 行が空でない」の合成を純粋関数へ切り出す

#1072 は `is_unsettled(armed, pending_seq)` として合成を名前のある純粋関数へ出し、受け入れ条件を測れる単位にした。同じ手を提案されたが**却下した——今回は合成が存在しない**。ガードは `plain_results_hidden` 単体で、「行が空でない」の判定は別の層にある（`on_enter` の `!self.state.results().is_empty()` と `activate` の `results().get(index)`）。無い合成に名前を付ければ、真実が 2 か所になる。

呼び出し点の脱落は代わりに**ソーステキスト検査**が捕まえる（`activation_entry_points_consult_the_display_gate`）。`launcher_controller.rs` にテスト席が無い（`LauncherController` の構築が `AppHandle` と engine lock を要求する）ため、述語のテストは「述語が何を返すか」しか測れない。先例は `src-tauri/src/indexing.rs` の `start_index_build_invalidates_the_icon_cache`。

### 4. `indexing` を素の `bool` で配る

`on_enter` は `shift_held: bool` を先に取るため、`on_enter(post.shift, indexing_raw, ..)` の 2 引数は**入れ替えてもコンパイルもテストも通る**。この型にはテスト席が無く、取り違えを区別できる観測が無い。

### 5. `FrameIndexing(pub bool)` のタプル構築子を公開したままにする

**一度この形で実装し、`/symmetric-check` Step 2c の指摘で覆した。** newtype を被せれば引数順の取り違えは塞がるが、**構築子が公開されていれば呼び出し点で任意の `bool` を包める**（`FrameIndexing(post.shift)` がコンパイルを通る）。同スキルが名指しする「**起点が同型なら型は守っていない**」形そのものだった。

型を `read_indexing` の隣（`window_coordinator.rs`）へ移してフィールドを private にし、構築子を読み点ただ 1 つへ閉じた。**残る一手は「本物をもう 1 回読む」ことだけ**で、そちらは `view.rs` の `indexing_is_read_exactly_once_per_frame` が固定する（変異注入で発火を実測）。

### 6. `SPEC.md` §8.6 の遷移ルール要約を規範の置き場所にする

`/state-check` Step 5 が最初にここを指した——§8.6 の図が `NormalMode --> ToolSelectionMode: Shift+Enter [tools >= 2]` を持ち、この変更はその**文書化された遷移にガードを足す**からである。同節は既に `indexing` 由来のガード（`/o` は `!indexing` のときのみ有効）を列挙してもいる。

**規範の正本は §4.7 に置いた**——述語の正本がそこに在り、`AGENTS.md`「正本を 1 か所に定め他は参照へ」に従うためである。§8.6 へは**参照の 1 行だけ**を足した（写しを置かない）。§8.6 単独に置く案を却下したのは、そこが個々の辺に overlay 条件を注記しない書き方を採っており（`launching` も同様）、規則の本体を書くと §4.7 の写しになるためである。

### 7. `run_search_with` の `indexing` 読みも凍結する

フレーム内の `indexing` 読みを 1 つに統一する案。**却下した——用途が違う**。あちらの判定は「行をクリアするか」で、到達経路（`consume_external_pending` / `on_input_changed` / `poll_search_debounce` / `poll_async`）ごとに**その時点で**判断するのが正しい。とくに `consume_external_pending` 経由の読みは順序不変条件を持ち（完了フレームをフリッカーなしで新結果にする）、前へ寄せると壊れる。

**受容する残余**: 凍結値と食い違うと「Enter が 1 フレーム飲まれる」か「行が空で何も起きない」になる。どちらも次フレームの再検索が回復する。`view.rs` の該当箇所に明記した。
