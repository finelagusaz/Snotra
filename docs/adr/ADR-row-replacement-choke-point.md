# ADR-row-replacement-choke-point: 行の差し替えを private なチョークポイントへ畳み、由来は呼び出し側のメソッド選択で表す

## 文脈

#1039 で、同じ出来事（`SearchState` の行が総入れ替わる）に対する 2 つの義務——(A) `rows_generation` の前進（#699 のクリック照合が読む）と (B) 検索 worker の in-flight の失効（#1004 の seq が読む）——を 1 か所へ合流させることになった。それまで (A) は `SearchState` の内側に、(B) は呼び出し側（`LauncherController`）が手で撃つ規約として散っており、11 箇所の手書き `dispatch.invalidate()` と 3 箇所の漏れ（`enter_tool` / `on_escape` の 2 枝）が生じていた。

issue 本文のスケッチと、`/plan-review` Step 2b の独立導出は、どちらも**公開メソッド `apply_rows(origin: RowOrigin, rows, now)`** ——行の由来を enum で受ける単一の入口——を提案していた。

## 決定

**private な `put_rows(rows, next_selected)` を単一のチョークポイントに置き、由来は呼び出し側が選ぶメソッド（`set_results` / `accept_worker_rows` / `enter_tool` / `on_escape` / `reset`）で表す。**

view 種別ガード（Folder/Tool ビュー中は遅着 worker 結果を採らない）は `put_rows` ではなく採り込み点 `accept_worker_rows` に置く。

## 検討した代替案と却下理由

- **公開 `apply_rows(RowOrigin, rows, now)`（issue のスケッチ・Step 2b の導出）**: 却下。理由は 2 つある。(1) **`now: Instant` を同期経路の 10 箇所が未使用のまま渡すことになる**——時刻を要求するのは worker の採り込みだけである。(2) **`RowOrigin` は選択方針の差を表せない**——`enter_tool` は `selected = 0`、`set_results` は現在の選択を保ってクランプ、`on_escape` は frame から復元した値を使う。由来の enum に選択方針まで載せると、enum が「由来」ではなく「由来 × 選択方針」の直積になり、呼び出し側が結局どちらも指定する。構造的効果（型の内側の単一チョークポイント）は private 案と同じである。

- **`SearchDispatch` を消して `SearchState` が `in_flight: Option<u64>` を直に持つ**: 却下。計時（`key_at` / `dispatched_at`）と seq が別の型へ分かれ、`Settled`（`since_key` / `since_dispatch`）の導出が 2 型に跨がる。`SearchDispatch` は既に「時計を内部で読まない」純粋核（`now` を引数で受ける）なので、丸ごと private フィールドとして内包する追加コストが無い。

- **`is_unsettled` を肯定形 `is_settled` にする**（issue 本文が想定した極性）: 却下。呼び出し点に `!` が出るだけでなく、`results_view.rs` の `RowsSnapshot::input_idle` の doc が持つ導出——「食い違うのは `armed == false ∧ pending != 0` のときである」——が極性反転で式ごと書き直しになる。否定形を保てばこの行は腐らず、リンク先の張り替えだけで済む。

- **view 種別ガードを `put_rows` の内側へ置く**: 却下。`put_rows` に `view_kind()` を読ませると経路ごとに意味が変わる——`enter_tool` は `self.tool` を `Some` にした**後**に呼ぶので、ガードが自分自身を弾く。呼ぶ側は既に自分がどの遷移の途中かを知っており、判定を共有する理由が無い。

- **view ガードを `accept` より**前**に置く**: 却下。Folder ビューで届いた結果が pending を残したまま捨てられ、**生成 1 : 破棄 0 の終端**ができる（`/symmetric-check` Step 2b）。seq 不一致のときだけ pending を保つのが正しい——そのときはより新しい要求がまだ飛んでいる。**ただし逆順で user-visible な実害が出る並びは構成できていない**——Folder ビューから出る経路（`on_escape` の folder 枝 / `reset` / `run_search_with` の Folder 枝）はすべて `put_rows` を通って失効させ、残っている間の唯一の消費者 `is_unsettled` も `should_flush_on_enter` が `ViewKind::Results` を要求するため Folder 中は読まれないためである。**却下の理由は実害ではなく不変条件の局所性である**——逆順にすると「生成 1 : 破棄 1」の成立根拠が呼び出し点の外（Folder の全出口）へ散る。

- **folder token と dispatch seq を newtype で分ける**: 却下。どちらも `u64` を返す生成メソッドで、取り違えても型・テスト・smoke が通る（**受容する残余**）。newtype を採らないのは、取り違えが「folder の行が永久に出ない」という目に見える形で現れるためである。代わりに両メソッドの doc で相互に名指しした。

- **`egui_search:dropped` の理由を返り値の型へ載せる**（`Result<Settled, DropReason>` 等）: 却下。承認済みのシグネチャ（`Option<Settled>`）から逸脱する割に、消費者は trace の payload 1 つだけである。呼ぶ前に `pending_seq() == seq` を控える形で全域に分割でき、その分割の正しさはテストで固定できる。

## 帰結

- (B) は規約から機構になった——行の差し替えを書いて失効を書き忘れる形が表現できない。`put_rows` から `invalidate` を落とす変異は、テスト 5 件に加えて **clippy の `dead_code`** でも落ちる（`invalidate` の呼び出し点がそこ 1 つである機械的証拠）。
- **`is_unsettled` のもう 1 つの合成は機構になっていない**——`armed || pending != 0` の `armed` を引数で受ける以上、正しい値を渡す呼び出し側の規約に依存し続ける（`Debouncer` は `LauncherController` のフィールドで純粋核から見えない）。これは #1039 の射程外である。**これは (A) でも (B) でもない第 3 の合成であって、(A) は #1039 以前から機構であり、いまも `put_rows` が持つ**——「世代の前進は規約である」と読んで呼び出し点へ `rows_generation += 1` を手書きすると、`run_search_with` の冒頭コメントが名指しする空撃ち（行は変わらないのに世代だけ進み、#699 のクリック照合が正当なクリックを全部捨てる）が戻る。
- `docs/superpowers/specs/2026-08-10-search-worker-design.md`「4.5 in-flight の失効 — 同期で行を差し替える経路はすべて `pending_seq` を進める」が置いた規約は、この機構へ吸収されたことで役目を終えた（spec は意思決定の記録ゆえ編集しない）。

---

status: Accepted
関連: #1039 ・#1004 ・#699 ・#1038 ・`docs/superpowers/specs/2026-08-10-search-worker-design.md`
