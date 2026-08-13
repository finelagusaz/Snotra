# plan — #1039 行の差し替えと in-flight の失効を同じ型の内側へ入れる

ブランチ: `chore/apply-rows-invalidates-in-flight` / 基準 main = 8ca9950
調査: `workspace/research.md`（母集団の再列挙・設計の決定 1〜3・issue スケッチとの差異はそちらが正本）

## 目的

同じ出来事（行が総入れ替わる）に対する 2 つの義務——(A) `rows_generation` の前進と (B) in-flight の失効——を `SearchState` の内側の**単一のチョークポイント**へ合流させる。(B) を規約から機構へ移し、11 箇所の手書き `dispatch.invalidate()` と 3 箇所の漏れを同時に消す。

**機構になるのは (B) だけである**（Step 2b の A-5・**この限定を落とすと実装より強い主張になる**）。`is_unsettled` が持つもう 1 つの合成——`armed || in_flight`——は、決定 2 で `armed` を引数で受ける以上、**呼び出し側が正しい `armed` を渡す規約に依存し続ける**。型が構造的に所有するわけではない。「規約が機構になった」と書くときは (B) に限る。

## 受け入れ条件

1. `LauncherController` は `dispatch` フィールドを持たない。`SearchDispatch` / `is_unsettled` は `egui_shell::mod.rs` から re-export されない（＝ controller から名前で届かない）
2. `SearchState` の内側で `self.results` へ代入・`clear` する経路が private ヘルパ 1 つに収束し、そこが `rows_generation` の前進と in-flight の失効を**両方**行う
3. **`enter_tool` / `on_escape`（tool 復帰・folder 復帰）の後に遅着した worker 結果が行を上書きしない**（現行の漏れ 3 箇所が閉じる）。**この漏れは表示まで貫通する**——`plain_results_hidden`（`search_state.rs:476-478`）は `indexing && Results && !instant_rows` ゆえ Tool ビューでは常に false で、上書きされた行を隠すものが無い（3b の付随証拠・裁定は `research.md` §8 (1)）
4. **Folder / Tool ビュー中は遅着 worker 結果を採らない**（`enter_folder` / `navigate_folder` の窓も閉じる。決定 3）
5. `on_enter` の flush 判定が `SearchState::is_unsettled(armed)` を通り、**`armed` の disjunct を落とす変異でテストが落ちる**（#1038 コメントの明示要求）
6. **`put_rows` から in-flight 失効を落とす変異で、受け入れ 3 のテストが落ちる**
7. trace イベント名と payload キーが不変（`egui_search:settled` の `dispatch_seq` / `pending_seq` / `index_entries` / `since_key_us` / `since_dispatch_us`）——smoke の H7 が読む
8. `cargo test -p snotra` / `cargo clippy -- -D warnings` / `cargo fmt --check` / `cargo doc --workspace --no-deps --document-private-items` / `npm run governance:check` がすべて green

## 変更ファイルと対象シンボル

| ファイル | 対象 |
|---|---|
| `src-tauri/src/egui_shell/search_state.rs` | `SearchState`（`dispatch` フィールド追加）／private `put_rows`／`set_results`／`enter_tool`／`on_escape`／`reset`／新 `issue_search`・`accept_worker_rows`・`pending_seq`・`is_unsettled`／`rows_generation` の doc |
| `src-tauri/src/egui_shell/search_dispatch.rs` | 自由関数 `is_unsettled` とそのテスト 2 件を `search_state.rs` へ移設。`SearchDispatch` / `Settled` / 残る 4 テストはそのまま。`//!` と `is_unsettled` doc の「#1039 への申し送り」を畳む |
| `src-tauri/src/egui_shell/launcher_controller.rs` | `dispatch` フィールド削除／`invalidate` 11 箇所削除／`run_search_with`・`clear_search`・`start_launch`・`on_enter` の呼び出し／`drain_search` の採り込み／`consume_reset_pending` |
| `src-tauri/src/egui_shell/mod.rs` | 88–89 行の re-export から `SearchDispatch` / `is_unsettled` を外す |
| `src-tauri/src/egui_shell/results_view.rs` | `RowsSnapshot::input_idle` の doc（`is_unsettled` への intra-doc link と導出 1 行の張り替え） |
| `docs/architecture.md` | 「検索フロー」mermaid の `Disp` participant と invalidate 矢印群／補足 229–230 行 |

`SPEC.md` は変更しない（挙動差分は §18.5 の記述に**合わせる**方向・`AGENTS.md` 開発ワークフロー 1 の判定）。ファイルの増減が無いので `src-tauri/CLAUDE.md` のモジュール索引も変更しない。

## 設計（実装者が追加判断せず書ける形）

```rust
// search_state.rs
pub struct SearchState {
    // ... 既存
    rows_generation: u64,
    /// 検索要求の同一性（#1004）。**行の差し替えと同じ型に住まわせる**（#1039）。
    dispatch: SearchDispatch,
}

impl SearchState {
    /// 行の差し替えの単一チョークポイント（private）。
    /// results 代入・selected クランプ・世代前進・in-flight 失効を必ず同時に行う。
    fn put_rows(&mut self, rows: Vec<SearchResult>, next_selected: usize) {
        self.results = rows;
        self.selected = clamp_selected(self.results.len(), next_selected);
        self.rows_generation += 1;
        self.dispatch.invalidate();
    }

    pub fn set_results(&mut self, results: Vec<SearchResult>) {
        let keep = self.selected;
        self.put_rows(results, keep);
    }

    /// 新しい検索要求へ seq を振る（controller が worker へ送る）。
    pub fn issue_search(&mut self, key_at: Instant, now: Instant) -> u64 {
        self.dispatch.issue(key_at, now)
    }

    /// worker の結果を採り込む。**seq が一致し、かつ Results ビューにいるときだけ**行を差し替える。
    #[must_use = "…（Settled は計時 trace の唯一の材料であり、落とすと settled が観測できなくなる）"]
    pub fn accept_worker_rows(
        &mut self,
        seq: u64,
        rows: Vec<SearchResult>,
        now: Instant,
    ) -> Option<Settled> {
        // **順序は `accept` が先である**（`/symmetric-check` Step 2b）。view ガードを前に置くと
        // Folder/Tool ビューで届いた結果が pending を残したまま捨てられ、生成 1 : 破棄 0 の終端が
        // できる。seq 不一致のときだけ pending を保つ（より新しい要求がまだ飛んでいる）。
        let settled = self.dispatch.accept(seq, now)?;
        if self.view_kind() != ViewKind::Results {
            return None;      // 遷移前の要求である（accept が take 済み＝ここで失効している）
        }
        let keep = self.selected;
        self.put_rows(rows, keep);   // ここで invalidate は no-op（accept が take 済み）
        Some(settled)
    }

    /// in-flight の seq（無ければ 0）。**消費者は trace の payload だけである。**
    pub fn pending_seq(&self) -> u64 { self.dispatch.pending_seq() }

    /// 最終クエリの結果がまだ行へ反映されていないか（#1038 → #1039 で移設）。
    pub fn is_unsettled(&self, armed: bool) -> bool {
        armed || self.dispatch.pending_seq() != 0
    }
}
```

`enter_tool` / `on_escape`（2 枝）/ `reset` は末尾の 3 行（`results` 代入・`selected` 代入・`rows_generation += 1`）を `put_rows(rows, next_selected)` の 1 行へ置き換える。

- `enter_tool` → `self.put_rows(rows, 0)`
- `on_escape`（tool 復帰）→ `self.put_rows(t.restore_results, t.restore_selected)`
- `on_escape`（folder 復帰）→ `self.put_rows(f.restore_results, f.restore_selected)`
- `reset` → `self.put_rows(Vec::new(), 0)`（`self.results.clear()` + `selected = 0` の置換）

実装上の注意 3 点:

- `view_kind()` は `tool` / `folder` を読むので、`enter_tool` は `put_rows` を呼ぶ**前に** `self.tool` を `Some` にしている。**`put_rows` に `view_kind` を読ませてはならない**（読ませると経路ごとに意味が変わる）
- `enter_tool` の `std::mem::take(&mut self.results)`（`ToolFrame.restore_results` への退避）は `put_rows` の**前**に来る。take が空 Vec を残すので `put_rows` の代入と衝突しない
- `reset` は現行 `self.results.clear()`（確保を保つ）だが `put_rows(Vec::new(), 0)` は確保を解放する。**hide のたびに行の確保を返すのはむしろ望ましい**（`working_set::trim_idle_working_set` と同じ向き）。挙動差は無い

controller 側:

- `run_search_with` の Plain 枝: `let seq = self.state.issue_search(self.last_input_at, Instant::now());`
- 同期で差し替える 10 箇所: `self.dispatch.invalidate();` の行を**削除**（`set_results` が内側で撃つ）
- `drain_search`: `let Some(settled) = self.state.accept_worker_rows(seq, results, now) else { …dropped trace… continue };` へ。**`egui_search:dropped` の payload に `"reason"` を足す**（決定）——値は `"seq"`（追い越された）と `"view"`（Folder/Tool へ遷移していた）。`dropped` を読む検査はリポジトリ内に 1 つも無い（grep 実測）ため後方互換の制約が無く、**2 つの捨て方は診断上まったく別の事象である**（前者は正常な追い越し、後者は本 issue が新設したガードの発火）。**ガードが実際に発火しているかを実機で観測する唯一の手段になる**——区別しないと、新設したガードが一度も効いていなくても `dropped` の件数からは分からない
- `consume_reset_pending`: `self.dispatch.invalidate();`（983）を削除（`state.reset()` が内側で撃つ）
- `on_enter`: `crate::egui_shell::is_unsettled(armed, self.dispatch.pending_seq())` → `self.state.is_unsettled(self.search_debounce.is_armed())`

## 不変条件と異常系

1. **`rows_generation` は行が差し替わったときだけ進む**（空撃ち禁止・#1004 / #699）。`put_rows` を呼ばない経路（`enter_folder` / `navigate_folder` / `move_selection` / `reset_selection` / `set_folder_filter` / `EscapeOutcome::Hide` 枝）では進めない——**現行と同じ集合であることを既存テストが固定する**（`rows_generation_is_stable_on_enter_folder` / `_on_escape_to_hide` / `_on_selection_change`）
2. **seq と generation は別の量である**（#1004）。統合しない
3. **`folder_gen` / token による folder の識別には触らない**（#1039「folder は触らない」）
4. **`RowsSnapshot::input_idle` は `!armed` のまま**。perf ヒューリスティックであり `is_unsettled` の修正を当ててはならない（#1074）
5. 異常系: worker 送信失敗（`search_tx.send` の `Err`）枝は現行どおり行をクリアする。`set_results` 経由になるので失効も自動で撃たれる
6. 異常系: `accept_worker_rows` が `None` を返したとき、**`rows` は drop される**（保留キューは作らない）。現行の `continue` と同じ
7. **`pending` の生成 1 に対し破棄 1 が全終端にある**（`/symmetric-check` Step 2b）。終端は 5 つ——採り込み成功（`accept`）／追い越し（次の `issue` が上書き）／同期差し替え（`put_rows` の `invalidate`）／reset（同）／**view 不一致で捨てた場合（`accept` が take 済み）**。`start_launch` の single-flight 早期 return だけは `issue` の後に来ないので対象外（行も変わらない）
8. **受容する残余（`/symmetric-check` Step 2c）**: `SearchState` が `u64` を返す生成メソッドを 2 系統持つ（`enter_folder`/`navigate_folder` の folder token と `issue_search` の dispatch seq）。取り違えても型・テスト・smoke が通る。newtype は採らない——区別できる観測が「folder の行が永久に出ない」（`accept_folder_result` が一致しない）で目に見えるため。**両メソッドの doc に「これは folder token であって dispatch seq ではない」旨を相互に名指しする**

## テスト方針と検証コマンド

### 新規テスト（`search_state.rs` の `mod tests`）

- `late_worker_rows_do_not_overwrite_tool_rows` — `issue_search` → `enter_tool` → `accept_worker_rows(seq, …)` が `None` を返し、行がツール行のままであること（受け入れ 3）
- `late_worker_rows_do_not_overwrite_restored_rows_after_escape` — tool 復帰・folder 復帰の 2 枝（受け入れ 3）
- `late_worker_rows_are_dropped_in_folder_view` — `issue_search` → `enter_folder` → `accept_worker_rows` が `None`（受け入れ 4・view ガード）
- `late_worker_rows_do_not_survive_reset` — `issue_search` → `reset` → `accept_worker_rows` が `None`
- `sync_replacement_invalidates_in_flight` — `issue_search` → `set_results` → 同 seq の `accept_worker_rows` が `None`（`stale_result_is_dropped_after_synchronous_replacement` の `SearchState` 版）。**`None` を返すことに加え「行が変わっていないこと」も assert する**（Step 2b の U-5: 不一致でも `rows` を代入する実装ミスは `None` の assert だけでは通る）。この assert は上の 4 件すべてに置く
- `unsettled_covers_in_flight_after_trailing_fired`（移設）— 真理値表 4 件 + `should_flush_on_enter` との合成。**`SearchState` を実際に遷移させて作る**（リテラルの `pending_seq` を渡さない）
- `settled_timing_survives_the_move` — `issue_search(key_at, dispatched_at)` → `accept_worker_rows(…, now)` の `since_key` / `since_dispatch` が現行の `accepts_only_the_latest_seq` と同じ値を返すこと

### 変異注入（コミット前に実際に当てて落ちることを測る）

| # | 変異 | 落ちるべきテスト |
|---|---|---|
| M1 | `is_unsettled` から `armed ||` を落とす | `unsettled_covers_in_flight_after_trailing_fired` |
| M2 | `put_rows` から `self.dispatch.invalidate()` を落とす | `sync_replacement_invalidates_in_flight` ほか受け入れ 3 のテスト |
| M3 | `accept_worker_rows` の view ガードを落とす | `late_worker_rows_are_dropped_in_folder_view` |
| M4 | **`enter_folder` を `put_rows` へ通す**（＝進めすぎの側） | `rows_generation_is_stable_on_enter_folder`（既存・`search_state.rs:638`） |

**M4 は「漏れ」ではなく「やりすぎ」を殺す枠である**（Step 2b の T9）。`put_rows` を「行に触る全経路」へ機械的に当てると `enter_folder`（行を差し替えず退避するだけ）まで通しかねず、そのとき #699 の照合が正当なクリックを全部捨てる。**既存の `rows_generation` テスト 8 本（`search_state.rs:591-679`）は 1 本も落ちてはならない。**

### 検知力の空白（受容する残余・Step 2b の A-6）

- **新規テスト 7 件が測るのは `SearchState` の内側だけである。** `drain_search` が `accept_worker_rows` を呼ぶ配線（`launcher_controller.rs:886`）が消えても、これらは緑のままである。同種の限界は既存コードが自認している（`search_state.rs` の #743 / #838 ブロック冒頭）
- **smoke の H7 は `invalidate` の呼び忘れを検知しない**（`SnotraTraceInvariants.psm1:402-405` が自ら明記——古い行が生えても `pending_seq` は 0 で PASS になる）。**ゆえに (B) の検知力はユニットテストが単独で担う**。ハーネスは支えにならない

### 検証コマンド（`docs/build-commands.md` カテゴリ A・F）

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p snotra                      # --lib は付けない（[lib] が無く常に失敗する）
cargo doc --workspace --no-deps --document-private-items
npm run governance:check
```

PostToolUse hook は A の一部を自動で走らせる（沈黙 = 合格）。**`cargo doc` は hook が走らせないので手で実行する**（`.claude/rules/comments.md`）。

## 文書の同期（実装と同格の作業）

1. `search_state.rs` の `rows_generation` doc 156–158 行——「`set_results` / `enter_tool` / `on_escape` / `reset` だけが触る」を `put_rows` の単一チョークポイントへ書き換え、**(B) も同じ関数が持つ**ことを書く
2. `search_dispatch.rs` の 3 か所（Step 2b の B-7）:
   - `is_unsettled` doc の「**#1039 への申し送り**」節を消費して畳む。`armed` の意味論（バースト継続中・invalidate 直後の窓）と `consume_reset_pending` の reset 罠は**移設先の `SearchState::is_unsettled` の doc へ移す**
   - `//!` 3 行目——「`rows_generation` とは別の量である」は**残す**が、続く「**#699 の世代は `set_results` が持ったままにする**」は `put_rows` へ移るので書き換える
   - `invalidate` の doc（54 行）「**同期で `set_results` を呼ぶ出所は必ずここを通す**（spec §4.5）」は**規約の宣言そのもの**であり、機構化された事実へ書き換える。なお `spec §4.5` の指す先は `SPEC.md` ではなく `docs/superpowers/specs/2026-08-10-search-worker-design.md`「in-flight の失効 — 同期で行を差し替える経路はすべて `pending_seq` を進める」である（実測。**その spec は意思決定の記録なので編集しない**）
3. `results_view.rs:36` の `input_idle` doc——`is_unsettled` への intra-doc link を [`SearchState::is_unsettled`] へ張り替える。**導出 1 行（「食い違うのは `armed == false ∧ pending != 0` のときである」）は腐らない**（#1074 申し送り 2 が名指しで再検査を求めた箇所・裁定済み）: 決定 2 が式を `armed || pending_seq != 0` のまま保つため、`input_idle`（`!armed`）との差集合は `!armed ∧ pending != 0` で不変である。**文言はそのまま、リンク先だけ張り替える。** 式を変える判断へ転んだ場合はこの行も同時に直す
4. `docs/architecture.md` の mermaid——**`Disp` participant を消し、`State` の矢印へ畳む**（決定）。`SearchDispatch` は `SearchState` の private フィールドになり、外から見える相手ではなくなるため、別の participant として描くと図が実装より 1 段細かい嘘をつく。`View->>Disp: invalidate()` の 5 本は消え、`View->>State: set_results(…)` に「（in-flight は内側で失効する）」の注記が付く。採り込みは `State: accept_worker_rows(seq)` の alt（採る / 追い越された / view 不一致）になる。補足 230 行（「射程は `set_results` の呼び出し点である」「その 2 つには in-flight が残りうる」「未再現の観察である」）は**本 issue が閉じた事実**へ書き換える
5. **インラインコメント「同期で差し替える＝in-flight は古い（spec §4.5）」× 9**（`launcher_controller.rs` の 272 / 433 / 769 / 773 / 785 / 805 近傍 / 837 / 855 / 858）は、`invalidate` の行と一緒に**消す**。残すと「規約がまだ在る」と読める（Step 2b の B-6）
6. `launcher_controller.rs:867` の `drain_search` doc——照合の主語が `SearchState` へ移り、「世代は `set_results` が進める」の名前も変わる（Step 2b の B-4）
7. **リンクにできる名指しはリンク形（intra-doc link）で書く**（#1074 の経験）。ただし `//` のインラインコメントは rustdoc が読まないのでリンク形を使わない

## 実装順序と作業項目

### Phase 1 — 純粋核と呼び出し点（`search_state.rs` / `search_dispatch.rs` / `launcher_controller.rs` / `mod.rs`）

**Phase 1 と Phase 2 は 1 コミットに束ねる**（`AGENTS.md`「条件別チェック」の「新 API の導入と呼び出し点の移行は 1 タスクに束ねる」）。分けると Phase 1 終了時点で `issue_search` / `accept_worker_rows` / `is_unsettled` が未使用になり、`-D warnings` 下の `dead_code` で落ちる。以下は**作業の順序**であってコミットの境界ではない。

- [ ] `SearchState` に `dispatch: SearchDispatch` を足し、private `put_rows` を実装する
- [ ] `set_results` / `enter_tool` / `on_escape`（2 枝）/ `reset` を `put_rows` へ合流させる
- [ ] `issue_search` / `accept_worker_rows` / `pending_seq` / `is_unsettled` を実装する（`#[must_use]` は `Option` を返す `accept_worker_rows` に付ける——`Option` は型段が効かないためメソッド段必須・`src-tauri/CLAUDE.md`。`issue_search` の `u64` は `enter_folder` の token 前例に倣い付けない）
- [ ] 自由関数 `is_unsettled` とそのテスト 2 件を `search_dispatch.rs` から削除し、`search_state.rs` へ `SearchState` 版として移設する
- [ ] 新規テスト 7 件を書く（上表）。Red → Green を確認する
- [ ] `dispatch` フィールドと 11 箇所の `dispatch.invalidate()` を削除する
- [ ] `run_search_with` / `drain_search` / `on_enter` / `consume_reset_pending` を新 API へ移行する
- [ ] `mod.rs` の re-export から `SearchDispatch` / `is_unsettled` を外す（**compile-fail を移行漏れ検出器にする**）
- [ ] trace の payload（`dispatch_seq` / `pending_seq` / `index_entries` / `since_*_us`）が不変であることをコードで確かめる
- [ ] `cargo test -p snotra` と `cargo clippy --workspace --all-targets -- -D warnings` が green

### Phase 2 — 文書同期

- [ ] 上の「文書の同期」1〜7 を適用する
- [ ] **`cargo doc --workspace --no-deps --document-private-items` を手で走らせる**——自由関数 `is_unsettled` の削除で `results_view.rs:36` の intra-doc link が確実に落ちる（3b が指摘・PostToolUse hook は沈黙する）
- [ ] `npm run governance:check` が green

### Phase 3 — 検知器の実測

- [ ] 変異 M1 / M2 / M3 / M4 を 1 つずつ当て、指定のテストが**実際に落ちる**ことを測る（測定後に変異を戻す）
- [ ] 全検証コマンド（カテゴリ A・F）を通し、実装差分を確定させる

## 未確定（実装前に潰す）

- [x] **`egui_search:dropped` の payload に理由を足すか** — **足す**（`"reason": "seq" | "view"`）。判断と理由は「設計」節の `drain_search` の項へ書いた。`dropped` を読む検査はリポジトリ内に 1 つも無い（grep 実測）ため後方互換の制約が無い
- [x] **`docs/architecture.md` の mermaid で `Disp` participant を残すか畳むか** — **畳む**。判断と理由は「文書の同期」4 へ書いた
- [x] **敵対的調査（3b）の所見の採否** — 7 争点中 0 件が崩れ、⚠️ 2 件。採否と機序の訂正は `research.md` §8 が正本。**採った所見 3 件**を本計画へ反映済み: (1) 漏れは表示まで貫通する（`plain_results_hidden` は Tool ビューで false・受け入れ 3 の重みが増す）、(2)「純粋核へ異物が入らない」の言い回しを「時計を内部で読まない性質は保たれる」へ訂正、(3) 自由関数削除で `results_view.rs:36` の intra-doc link が **`cargo doc` で落ちる**——Phase 3 の作業項目で確実に走らせる
- [x] **check スキルの要対処の反映** — `/symmetric-check` を計画段階で実施し 1 件を反映（`accept_worker_rows` の順序・不変条件 7・8）。`/race-check` はスキル本文が「計画段階では起動しない」（#784）と定めるため `/implement` へ、`/dry-check` は新関数の実体が要るため実装後へ回す
- [x] **`/plan-review` の要対処の反映** — 高リスク判定で Step 2b（独立導出 1 体）を実行。要対処 6 件のうち 5 件を反映、1 件は既に計画が満たしていた。詳細は下の「plan-review 結果」

## plan-review 結果

- リスク: **高**（状態遷移の変更・worker/drain の変更・複数モジュール間インターフェースの新設）
- レビュー方式: **独立導出 1 体**（Step 2b。`workspace/plan-review-1039-derivation.md`）＋ 主エージェントの自己照合（Step 1）＋ `/symmetric-check`
- エージェント数: **2**（3b の敵対枠 1 + Step 2b の独立導出 1）

### 自己照合（Step 1）で見つけた不一致 — 2 件、両方修正済み

- **項目 6**（タスク分割が既存トリガーを跨ぐ）— Phase 1（新 API）と Phase 2（呼び出し点の移行）を分けていた。`AGENTS.md`「新 API の導入と呼び出し点の移行は 1 タスクに束ねる」に抵触し、分けると `-D warnings` 下で新 API が `dead_code` で落ちる。**1 コミットへ統合した**
- **項目 5**（未確定欄に「実装時に決める」が残る）— `dropped` の reason と mermaid の `Disp` を**この場で決めた**

項目 7（変更で偽になる散文）の概念ラベル grep は追加ファイルを出さなかった: `PERFORMANCE.md:2576`（`SearchDispatch::issue` は型として残る）と `layout.rs:387`（`set_results` は残る）はどちらも記述が真のまま。

### 要対処（Step 2b・再照合した根拠つき）

- **A-1 母集団は 11 箇所** — 計画へ反映済み（`research.md` §2）。**計画は数ではなく `rg -n "state\.set_results"` の全件を指す**という指摘を採り、実装時に数え直す
- **A-2 `docs/architecture.md:230` の bullet が偽になる** — 「文書の同期」4 に既にあった。mermaid の矢印 7 組（`Disp` 宛 5 本 + `accept` 1 本 + 名前 2 か所）まで具体化した
- **A-3 `results_view.rs:36` の導出行** — 既に反映済み。**さらに Step 2b が肯定形 `is_settled` を採った結果、極性反転で式ごと書き直しになると判明**——否定形を保つ決定 2 の根拠が 1 つ増えた（`research.md` §5 決定 2 へ追記）
- **A-4 trace 2 本を殺さない形** — 計画の設計（`pending_seq()` を pub・`Option<Settled>` を返す）が既に満たす。**加えて「H7 は `invalidate` 忘れを検知しない」（psm1:402-405 が自認）を受容残余として明記した**
- **A-5 `armed` の合成は機構にならない** — **採用**。「目的」節へ限定を追記した（実装より強い主張を書かない）
- **A-6 検知力の空白（新規テストは配線を測らない）** — **採用**。「検知力の空白」節を新設。M4（進めすぎの変異）も追加した

### 軽微

- **B-1 `state.reset()` の呼び出し点は 2 つ** — **採用**（`rg` で自分で再照合し成立）。`research.md` の母集団表を修正
- **B-2 `layout.rs:387` の平文名指しを intra-doc link 形へ** — **見送り**。`set_results` は名前も動作も残るので腐らない。リンク化は #1074 の一般的な推奨だが本 issue の射程外で、「やりすぎ」の側
- **B-4 / B-5 / B-6 / B-7 の doc とコメント** — 全件「文書の同期」へ反映
- **B-3 `search_state.rs:156-158` の呼び出し元列挙** — 既に「文書の同期」1 にあった
- **B-8 `src-tauri/CLAUDE.md` の索引行** — `search_dispatch.rs` を残す設計なので不変。計画の記述どおり

### 未検証（Step 2b が挙げたもののうち、裁定した項目と残した項目）

- **U-1 計時を核へ持ち込むか** — 裁定済み（`research.md` §4・§8 (2)）。`now` を引数で受ける形なので時計への依存は増えない
- **U-2 `PERFORMANCE.md` の 2 か所** — **書き換えない**。過去に実測した値の記録であり、当時の名前で正しい。書き換えると「新しい名前で測り直した」と読める（`fixing-instrument-invalidates-ab-comparison` の型）
- **U-3 spec `2026-08-10-search-worker-design.md` §4.5** — **編集しない**。spec は意思決定の記録であり、そのとき何を決めたかは真のまま
- **U-4 `#[must_use]` の当否** — **付ける**。`&mut self` で行を差し替えてから `Option<Settled>` を返す形は規約の対象で、`Option` は型段が効かないためメソッド段必須。ただし**メッセージは正確に書く**——落として消えるのは状態遷移ではなく計時 trace である
- **U-5 不一致時に行を触らないことの固定** — **採用**。テストへ「行が変わらないこと」の assert を追加
- **U-6 `SearchState::new()` の 35 件** — 全件テスト内で `dispatch` は `Default` から始まるため影響なし。**全件を目で確かめてはいない**（Step 2b と同じ残余をそのまま引き継ぐ）

### 判断の不一致（導出 ∖ 計画）

- **Step 2b は issue のスケッチどおり公開 `apply_rows(RowOrigin, rows, now)` を採り、計画は private `put_rows` + `set_results` / `accept_worker_rows` を採る。** 計画を維持する——導出案は Sync 経路でも `now` を要求し（10 箇所で未使用の引数）、`enter_tool` の `selected = 0` と `set_results` の clamp-keep という**選択方針の差**を `RowOrigin` だけでは表せない。構造的効果（型の内側の単一チョークポイント）は同じである
- **Step 2b は view 種別ガード（決定 3）を導出していない。** issue の WHAT に無いため——これはスコープの判断であり、下の「人間レビュー」で名指しして確認する

## セルフレビュー

- リスク: **高**
- plan-review: **独立レビュー 1 体**（Step 2b・独立導出）
- エージェント数: **2**（3b 敵対枠 1 + Step 2b 1）
- 要対処: **6 件**（うち 5 件を計画へ反映、1 件は計画が既に満たしていた）。軽微 8 件は 7 件反映・1 件見送り（B-2・理由記載）。未検証 6 件はすべて裁定
- 未検証: `SearchState::new()` の 35 件を目で全件確認していない（Step 2b の U-6 をそのまま引き継ぐ。`Default` 由来ゆえ影響は無いはずだが未実測）
- 判断: **実装着手 = 人間の裁定待ち**（決定 3 のスコープ確認）

## 人間レビュー

- [x] 承認済み — 2026-08-13 / 問い: "この計画で実装へ進んでよいでしょうか。" / 回答: "承認する"
- [x] スコープの裁定 — 2026-08-13 / 問い: "採り込み点（accept_worker_rows）に「Results ビューのときだけ行を差し替える」ガードを置きますか。issue の WHAT には無い追加です。" / 回答: "置く（推奨）"

**裁定の帰結**: 決定 3（view 種別ガード）は計画どおり実装する。`enter_folder` / `navigate_folder` の窓も閉じ、`egui_search:dropped` の payload に `"reason": "seq" | "view"` を足す（ガードの発火を実機で観測する唯一の手段になる）。
