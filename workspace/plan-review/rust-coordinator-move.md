# L1 レビュー: window_coordinator.rs 新設（#749 段1）

対象: `window_coordinator.rs`（新規）/ `mod.rs`（8関数削除+re-export）/ `view.rs`（drive_results_window/max_results/デルタガード2フィールド撤去）

## 問題なし

1. **8関数の呼び出し元差分ゼロの主張（観点1）**: 自分で grep して数え上げた結果、8関数はすべて `crate::egui_shell::<名前>` 形の完全修飾パスで呼ばれている。
   - `main.rs:267,432,446,570`（show_egui_main）/ `main.rs:429`（hide_egui_main）/ `main.rs:311`（register_hide_listener）
   - `view.rs:838`（position_results_below_main）/ `view.rs:853`（results_available_height）/ `view.rs:875,1072`（wake_results, wake_main）
   - `results_view.rs:575`（wake_main）
   - `mod.rs:203,638,679`（wake_main。自モジュール内呼び出し）/ `mod.rs:287`（position_results_below_main）
   - `commands/window.rs:96,99,142,145` は `ResultsWindow::set_topmost` の呼び出しで、移設対象の8関数とは無関係（`ResultsWindow` 自体は移設しない）
   
   全て `pub(crate) use window_coordinator::{...}` の re-export で解決でき、plan.md/research.md の「差分ゼロ」主張は成立する。mod.rs 自身の呼び出し（`create` / `spawn_update_check` / `register_initial_hotkey_failure_listener` から）も、re-export された名前が `use` によって mod.rs のスコープへ入るため無修飾のまま動く（Rust の可視性規則上健全）。

2. **drive_results_window / max_results の呼び出し元（観点2）**: 実際に grep すると、`drive_results_window` の呼び出しは `view.rs:1838` の1か所のみ、`max_results()` の呼び出しは `view.rs:818`（`drive_results_window` 内部）の1か所のみ。plan.md/research.md の「1か所だけ」という主張と一致する。doc コメント上の言及（mod.rs:457,471,481,483,489,556、results_window.rs:5,50、results_view.rs:5、layout.rs:102,118、visual.rs:5）はコード呼び出しではないため、この観点には影響しない（ただし文書同期の話は下記「要対処」参照）。

3. **対称ペア（観点3）**: `show_egui_main`/`hide_egui_main` は両方とも移設対象8関数に含まれ、片側だけが取り残されることはない。`wake_main`/`wake_results` も同様に両方移設される。`ResultsWindow::show`/`hide`（results_window.rs 側）は今回変更されず対称性は維持されている。

4. **mod.rs 残留関数から移設関数への参照（観点4）**: `create`（mod.rs:227-297）は `position_results_below_main` を `Moved` リスナー内（mod.rs:287）で呼ぶ。`spawn_update_check`（mod.rs:129）は `wake_main` を呼ぶ（mod.rs:203）。`register_initial_hotkey_failure_listener`（mod.rs:669）は `show_egui_main` と `wake_main` を呼ぶ（mod.rs:678-679）。いずれも re-export された名前が `pub(crate) use window_coordinator::{...}` により mod.rs 自身のスコープに入るため、モジュール分割後も無修飾呼び出しのまま成立する（`super::` を要するのは window_coordinator.rs 側から mod.rs 側の `EguiShellState`/`read_metrics`/`ResultsWindow` を参照する向きだけで、plan.md がこれを正しく明記している）。

5. **不変条件 I3〜I5・I7・I8（観点5）**: ソースを実読して現行の成立を確認した。
   - I3: `hide_egui_main` 内で `state.main_visible.store(false, ...)`（mod.rs:476-478）が `results.hide()`（mod.rs:484-491）より**前**にあることを確認。doc コメント（mod.rs:470-475）も明記。
   - I4: `show_egui_main` 内で `window.show()`（mod.rs:397）の**後**に `state.main_visible.store(true, ...)`（mod.rs:400-402）があることを確認。
   - I5: `drive_results_window` 末尾（view.rs:870-875）の `wake_results` が条件分岐の外にあり無条件であることを確認。
   - I7/I8: 本 PR はこれらの機構（raw操作の3点固定・wakeがprimitiveであること）を変更しておらず、移設のみであるため崩れない。
   移設は「本文を一字も変えない」前提のため、これらは移設後もそのまま成立する。

6. **読み点の非対称の保持（観点6）**: `plain_hidden` は view.rs:1770 で算出され `take_clicked_for` の消費（view.rs:1809）より前。`drive_results_window` の呼び出し（view.rs:1838）は消費より後にある。plan.md が提案する「`DriveResultsInputs` を `update()` 末尾（呼び出し箇所そのもの）で構築する」形は、構築位置が現行の呼び出し位置（1838）と同一である限り、現行と同じ読み点順序を保つ。**ただしこれは実装者が plan.md の警告（「この構造体を作る式を `plain_hidden` の算出の隣へ動かしてはならない」）を守った場合に限る規範的な保証であり、コンパイラも自動検出器も持たない**（issue自身も認める受容残余=I1と同種）。

7. **`&mut self` → 自由関数化での self 参照の完全性（観点7）**: `drive_results_window`（view.rs:788-876）本体の `self.` 参照をすべて列挙した。
   - `self.app_handle`（複数箇所、794/808/818/838/853/861/867/875 相当）→ 引数 `app: &tauri::AppHandle` へ
   - `self.state.results().len()`（803）→ `i.result_count` へ（plan Phase4 の差し替え表に記載済み）
   - `self.max_results()`（818）→ `max_results(app)`（coordinator 内の自由関数呼び出し）へ
   - `self.last_results_height` / `self.last_results_width`（858-863）→ `ResultsWindow.last_size`（Phase2）へ
   - `metrics.row_height`（819相当）は `self.` ではなく引数 `metrics: &Metrics` 経由だが、これも `i.row_height` へ差し替え対象として plan に明記されている
   これ以外に `self.` を参照する箇所は本体中に見当たらず、plan の移行先の割り当てに漏れは無い。

8. **スコープ（観点8・9）**: issue #749 本文を読んだ。「設計上の制約（再導出しないこと）」節が明記する禁止事項——main show だけ raw化しない・窓ごとの層混在禁止・`Deref` 実装禁止・wake は primitive のまま・`drive_results_window` 末尾 wake の edge 化禁止・`hide_egui_main` の順序維持——のいずれも plan.md は変更しておらず、犯していない。re-export 様式（`pub(crate) use window_coordinator::{...}`）は `results_window`/`layout`/`visual`/`notify` 等の既存パターンと同型であり、新規実装ではない（research.md §3 で確認済み、mod.rs 冒頭の既存 re-export 群と型が一致することを実読でも確認）。

## 軽微な懸念

- **research.md の `wake_results` 呼び出し元列挙が不完全**: research.md「外部呼び出し元」表は `view.rs:838,853,875,1072` を挙げるが、`wake_results` の実際の呼び出しは `view.rs:875`（`drive_results_window` 内）と `view.rs:1801`（`update()` 内のスナップショット差分検知ブロック、`drive_results_window` の外）の**2か所**である（`grep 'wake_results('` で実測）。mod.rs 自身の `wake_results` の doc コメント（mod.rs:554-556）も「呼び出しは main の update() 内 2 箇所」と明記しており、研究メモの列挙より一次資料の方が正確だった。
  実害は無い——`view.rs:1801` も既に `crate::egui_shell::wake_results` の完全修飾パスで呼んでおり、re-export 後も無修正で動く。ただし「すべて grep で実在確認済み」（research.md §2 冒頭）という完全性の主張はこの1件で外れており、`/plan-review`「Step 2b」の精神（列挙は SSOT のツール自身に問う）に照らすと、他の列挙（mod.rs 側 3箇所等）も念のため実装前に再 grep する価値はある。

## 要対処

1. **`DriveResultsInputs` の `max_results` フィールドを巡る plan.md 内部の矛盾**: Phase4 のコード例（plan.md 116-132行目のstruct定義、145-161行目の呼び出し例）はいずれも `max_results: u32` をフィールドとして含む。しかしその直後の段落（plan.md 163行目）は「`max_results` を呼び出し側で読むか coordinator の内側で読むかは内側に倒す……ゆえに `DriveResultsInputs` から `max_results` を落とし、`drive_results_window` の中で `max_results(app)` を呼ぶ」と明記しており、構造体からフィールドを除く決定を下している。コード例と決定文が矛盾したまま plan.md に同居しており、このまま実装に渡すと「どちらが正か」を実装者が推測することになる（呼び出し例の該当行 `max_results: crate::egui_shell::...,   // coordinator 側の自由関数を使う` はプレースホルダのまま値として成立しない疑似コードでもある）。実装着手前に plan.md 側のコード例を決定文に合わせて修正するか、決定文を明示的に「上のコード例を訂正する」形に直すべきである。

2. **`mod.rs` 内の「`view.rs` の `drive_results_window`」参照が移設後に不正確化する**: `hide_egui_main` と `wake_results` の doc/インラインコメント（mod.rs:457, 481, 489, 494）は `drive_results_window` の所在を明示的に `` `view.rs` `` と名指ししている。plan.md は「移設する8関数の本文は一字も変えない」と明記し（plan.md:88）、かつ Phase4 で `drive_results_window` 自体も `window_coordinator.rs` へ移設する。結果として、両方の移設が完了すると、`hide_egui_main`（window_coordinator.rs に移設済み）や `wake_results`（同）の doc コメントが「`view.rs` の `drive_results_window`」と書いたまま、実際には**同じファイル内の隣接関数**を指す自己言及的な誤記になる。plan.md の Phase5「文書同期」リスト（`src-tauri/CLAUDE.md`・`docs/architecture.md`・layout.rs:102,118・visual.rs:5）にこの4箇所は含まれておらず、「一字も変えない」という指示と「文書の正確性を保つ」という一般原則が衝突している。実装時にこの4箇所（および mod.rs 全体で `grep 'view\.rs' mod.rs` を再実行して他の見落としがないか）を明示的な例外として更新対象に加えるべきである。

## 未検証（理由）

- **cargo check / clippy / test の実行**: 対象コード（window_coordinator.rs 等）はまだ存在せず、実装前のためビルド健全性そのものは確認できない。特に「観点4」で述べた `super::` 経由のモジュール間参照が実際にコンパイルを通るかは、Rust の可視性規則から推論した判断であり、コンパイラでの最終確認はしていない。
- **段3（#666）への影響**: research.md が引用する issue #666 の本文（「責務に応じて分割して、見通しをよくする」の1行）は読んだが、#666 側で本 PR の設計判断（`window_coordinator.rs` というファイル名・managed state 構成を変えない方針）を前提にした後続作業が既に走っていないか（他ブランチ・他 PR の有無）までは確認していない。
- **カテゴリ D 目視7項目の実施**: これは実装後にしか実施できない検証であり、スカウト（静的レビュー）の範囲外のため未実施。plan.md 自体がこれを「受容残余」として明記している点は妥当と判断した。
- **`results_view.rs` 側から見た `drive_results_window`/`wake_main` 等への依存**: `results_view.rs:5` の doc コメント（「hide は外部（`hide_egui_main` / main の `drive_results_window`）が所有する」）は具体的なファイルパスを名指ししていないため今回は問題なしと判断したが、`results_view.rs` 全文を通読した上での判断ではなく、grep でヒットした周辺のみを確認した。同ファイル内に他の file-path 名指し参照が無いかは网羅的に確認していない。

## チェックリスト（観点1〜10）

- [x] 1. 8関数の呼び出し元の数え上げと re-export 差分ゼロの検証
- [x] 2. drive_results_window / max_results の呼び出し元・参照の数え上げ
- [x] 3. 対称ペアの片側変更チェック
- [x] 4. mod.rs 残留関数から移設関数への参照の成立確認
- [x] 5. 不変条件 I1〜I8 の現行コードでの成立確認（I1・I2・I6 は観点6と合わせて確認、I3〜I5・I7・I8 は本節で直接確認）
- [x] 6. 読み点の非対称の保持確認（`DriveResultsInputs` 構築位置への依存を含む）
- [x] 7. `&mut self` → 自由関数化に伴う self 参照の完全列挙
- [x] 8. issue #749 の要求・禁止事項との整合確認
- [x] 9. 既存 re-export パターンとの整合確認（新規実装の要否）
- [x] 10. 未検証項目の明記（本ファイル最終節）
