# plan.md 内部一貫性レビュー（#749 段1: WindowCoordinator）

対象: `C:/workspace/Snotra/workspace/plan.md`（HEAD `ed6d68a`、`chore/window-coordinator`）。
設計の当否は評価しない。文書内の記述同士の食い違いのみを対象とする。

## 矛盾（要修正）

### M1. 「移設する関数」の内訳数が本文内で食い違う

- `plan.md:13`（変更ファイル一覧・`window_coordinator.rs` 行）:
  > 窓の可視性・位置・サイズ・wake の driver 群（**mod.rs から 9 関数** + main.rs から 1 関数 + view.rs から 2 関数）
- `plan.md:104-106`（Phase 3）:
  > 移設する 9 関数（`research.md` §2 の表・本文は変えない）:
  > `show_egui_main` / `hide_egui_main` / `save_placement_relative` / `register_hide_listener` / `wake_main` / `wake_results` / `position_results_below_main` / `results_available_height` / **`position_on_target_monitor`**（`main.rs:150-193`）

Phase 3 が列挙する「9 関数」は **mod.rs 由来 8 個 + main.rs 由来 1 個（`position_on_target_monitor`）の合計** であり、これは research.md §2 の表（mod.rs の「する」列挙も 8 個）および実際のソースと一致する（下記「確認して問題なかったもの」参照）。
ところが `plan.md:13` は同じ「9」を **mod.rs だけの内訳** として書き、その上で main.rs 由来の 1 個を**別枠でもう一度**加算している。結果として line 13 が主張する合計は 9+1+2=12 個になるが、実際に移設される関数は 8（mod.rs）+1（main.rs）+2（view.rs: `drive_results_window`, `max_results`）= **11 個**である。

実測（`git show a98312c:src-tauri/src/egui_shell/mod.rs`）:
```
129:pub(crate) fn spawn_update_check   → 移設しない
227:pub(crate) fn create               → 移設しない
303:fn apply_rounded_corners           → 移設しない
326:pub(crate) fn read_metrics         → 移設しない
348:pub(crate) fn read_visual          → 移設しない
366:pub(crate) fn show_egui_main       → 移設① 
460:pub(crate) fn hide_egui_main       → 移設②
505:pub(crate) fn save_placement_relative → 移設③
531:pub(crate) fn register_hide_listener  → 移設④
548:pub(crate) fn wake_main            → 移設⑤
563:pub(crate) fn wake_results         → 移設⑥
580:pub(crate) fn position_results_below_main → 移設⑦
612:pub(crate) fn results_available_height (windows)     → 移設⑧
622:pub(crate) fn results_available_height (not windows) → 移設⑧の双子（同一名）
630/644/669: register_config_wake_listeners 等 → 移設しない
```
mod.rs から移る**名前付き関数**はちょうど 8 個。`line 13` の「mod.rs から 9 関数」は実測と食い違う。

**要修正**: `line 13` を「mod.rs から 8 関数 + main.rs から 1 関数 + view.rs から 2 関数」（合計 11）に訂正するか、Phase 3 の「9」の内訳説明と整合する形に書き直す。

---

### M2. カテゴリ D の目視項目数「7 項目」と実際の列挙「9 項目」が食い違う

- `plan.md:260`（テスト方針表・カテゴリ D 行）:
  > `cargo run -p snotra` で下の **7 項目**を目視。issue が「カテゴリ D の目視を必須とし、見るべき項目を PR 本文に列挙する」と要求している
- `plan.md:271-281`（「カテゴリ D 目視項目（PR 本文へそのまま転記する）」）:
  実際に番号付きで列挙されているのは **1〜9 の 9 項目**（ホットキー show／hide 同時消灯／クリック起動／ドラッグ追従／visual 変更再描画／設定画面 topmost 対称／下端クランプ／位置復元／マルチモニター追従）。

`gh issue view 749` を確認したが、issue 本文はカテゴリ D の目視を要求しているだけで具体的な項目数（7 でも 9 でも）は指定していない。したがって「7」は issue からの引用でもなく、plan.md 自身の後段の列挙（9 項目）とも一致しない、**文書内でのみ生じた数え間違い**である。

**要修正**: `line 260` の「7 項目」を「9 項目」に訂正する。

---

### M3. `src-tauri/CLAUDE.md`「モジュール構成」の編集対象を「5 行」と「段落」で違う単位を使っている

- `plan.md:20`（変更ファイル一覧・`src-tauri/CLAUDE.md` 行）:
  > 「モジュール構成」の **5 行**（新規 `window_coordinator.rs` / `mod.rs` / `view.rs` / `results_window.rs` / `layout.rs`）
- `plan.md:219`（Phase 5）:
  > `src-tauri/CLAUDE.md`「モジュール構成」の `egui_shell/` **段落**を直す。…この段落は「ファイル名 + 一言の責務要約」を添える書式である

実測（`src-tauri/CLAUDE.md:34`）— `egui_shell/` の記述は **1 本の長い箇条書き行**であり、`mod.rs` / `layout.rs` / `view.rs` / `results_window.rs` などの責務要約はすべて同じ 1 行の中に読点区切りで同居している（5 つの別々の行ではない）:
```
34:- `egui_shell/`: ディレクトリモジュール（`mod.rs` + `lifecycle.rs` / … / `results_window.rs` / `visual.rs`）。…、`layout.rs` は高さ算出 + results 可視性の導出 + 幾何 + debounce の純粋核（`Metrics` / `results_window_height` / `present_results` / `results_top_y` / `available_below` / `Debouncer`。…）、…`view.rs` は検索 view（…・results 窓 driver・…）、…`mod.rs` は窓生成（main/results 両窓）・show/hide（両窓同期）・位置永続・…
```
`line 219` の「段落」という表現は実測と一致するが、`line 20` の「5 行」は実測と食い違い、かつ同じ文書内の `line 219` の記述とも矛盾する。5 つの編集対象（ファイル名ごとの要約）は実在するが、それは「5 行」ではなく「1 行の中の 5 箇所」である。

**要修正**: `line 20` を「5 箇所（1 段落内）」等、`line 219` と整合する表現に訂正する。実装者が文字どおり「5 行」を探すと見つからず、Edit の対象特定を誤りうる。

---

## 不整合の疑い（要確認）

### D1. mod.rs 内の「view.rs の drive_results_window」名指しが本当に 4 か所で全件か

`plan.md:119` は次のように「全称」を主張する:
> `mod.rs` の 4 か所が「`view.rs` の `drive_results_window`」と名指ししているが…そのままでは誤記になる（457 / 481 / 489 / 494 行）。…**「本文は一字も変えない」の例外はこの 4 か所と、下の `//!` だけである**

実測（`git show a98312c:src-tauri/src/egui_shell/mod.rs | grep -n "view\.rs.*drive\|drive.*view\.rs"`）:
```
28:  main.rs（managed state 化）・view.rs（drive）・commands/window.rs（topmost）が消費する
457: **results の hide はここを通らない経路がある**（`view.rs` の `drive_results_window`）ため、
481: （対称は main update 内の show）。`view.rs` の `drive_results_window` は update **内**
489: （ここと view.rs の drive_results_window）、trace は要求レベルゆえ
494: results 単独 hide（view.rs の drive）では main が可視のままゆえ trim しないのが正しい。
```
457/481/489/494 に加えて **line 28** にも「`view.rs`（drive）」という同種の記述がある（`ResultsWindow` 再エクスポートの消費元コメント）。`drive_results_window` が `window_coordinator.rs` へ移ると、この記述も「view.rs」ではなく「window_coordinator.rs」を指すべきになる点は 457/481/489/494 と同型の問題である。

ただし line 28 は「`drive_results_window`」という関数名を直接持たず「（drive）」という略記であり、かつ文脈は「`ResultsWindow` 型を誰が使うか」の列挙であって「`drive_results_window` の所在」そのものではない。497 行の「自己言及になる」性質（同一モジュール内での名指しが不自然になる）と完全に同型かは読み方に依る。加えて `line 494` 自体も文字どおりには「`drive_results_window`」ではなく「`view.rs` の drive」という略記であり、plan.md の「4 か所が『view.rs の drive_results_window』と名指し」という要約はこの 1 件について厳密ではない。

**要確認**: line 28 を「本文を変えない」の対象外（=そのまま放置してよい）と判断したのか、単に見落としたのかが plan.md からは読み取れない。見落としであれば、Phase 3 の「本文は一字も変えない、例外はこの 4 か所と `//!` だけ」という**全称的な確定**（4 か所 = 全件）が崩れる。

### D2. 「責務が変わるファイルは 6 つ」に main.rs を含めるが、判定は 2 列とも「不要」

`plan.md:206` は次のように述べる:
> **責務記述の drift はサイトではなくクラスとして潰す。** 責務が変わるファイルは **6 つ**（`mod.rs` / `view.rs` / `results_window.rs` / `layout.rs` / `main.rs` / 新規 `window_coordinator.rs`）である。

続く判定表（`plan.md:210-215`）では `main.rs` の行が `//!`＝不要・`CLAUDE.md`＝不要 と、**両方とも「変更不要」**という結論になっている。

「責務が変わる」という見出しの主張（＝文書側の記述を直す必要がある、という含意）と、実際の判定（記述はどちらも真のまま＝直す必要がない）が字面上噛み合っていない。ファイル自体（`main.rs` から `position_on_target_monitor` が抜ける）は変わるが、「責務の**記述**（`//!`・CLAUDE.md）」は変わらない、という区別だと読めば矛盾ではないが、見出しの「責務が変わるファイル」という表現がその区別を明示していないため、字面だけを見た実装者が「main.rs にも記述更新が要る」と誤読しうる。

**要確認**: 見出しを「ファイルの中身が変わる 6 ファイル（記述更新の要否は個別判定）」のような表現に寄せるべきか、現状で実装者に誤解が生じないと判断してよいか。

### D3. Phase 1 の「同じコミットで移行する」がフェーズ境界と整合するか

`plan.md:50-55`（Phase 1）:
> **呼び出し点は 2 つあり、どちらも同じコミットで移行する**（`-D warnings` 下で未使用の新 API は `dead_code` で落ち…）:
> 1. Phase 2 の `ResultsWindow::set_size`（results 窓）
> 2. `view.rs:1830` の main 窓デルタガード

「実装順序」は Phase 1〜5 の見出しで区切られており（本ドキュメントは他所で「各 Phase の検証 green 後にコミット」という一般則を引く運用がある — `CLAUDE.md`「サブエージェント委譲と worktree」）、字面どおり読むと Phase 1＝1 コミット、Phase 2＝別コミットに見える。その場合、呼び出し点 1（Phase 2 内）と呼び出し点 2（Phase 1 の文中で言及されるが所属 Phase 番号の明記なし）を「同じコミット」に収めるには、実質的に Phase 1 と Phase 2 を 1 コミットに束ねるか、呼び出し点 2 の移行を Phase 2 側へ倣わせる必要があるが、plan.md はどちらであるかを明示していない。

なお、`size_delta_exceeds` は Phase 1 自身が追加する 2 本のユニットテストから直接呼ばれるため、Phase 1 単独でコミットしても `cargo clippy --all-targets -D warnings` の `dead_code` 検査は（テストが使用実績になるため）落ちない可能性が高く、この点で「同じコミットで移行する」という要求の技術的な必然性はやや薄い。とはいえ plan.md 自身がこの必然性を dead_code 回避の根拠として明記している以上、**Phase 番号と commit 粒度の対応関係**を明示しないと、実装者が「呼び出し点 2 をどのコミットに含めるか」で迷う。

**要確認**: Phase の番号立てが commit 粒度と 1:1 対応する前提かどうか。対応するなら呼び出し点 2 の所属 Phase を明記すべきで、対応しないなら「同じコミットで」という表現自体の粒度（Phase 単位か PR 単位か）を明確にすべき。

---

## 確認して問題なかったもの（数え直した数値を含む）

| # | 項目 | 確認方法 | 結果 |
|---|---|---|---|
| 1 | `cargo test -p snotra` のベースライン「174 passed / 0 failed / 2 ignored」 | `cd src-tauri && cargo test -p snotra` を実機実行 | **一致**（`test result: ok. 174 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 1.61s`）。「176 passed」目標も 174+2（Phase 1 の新規テスト 2 本）で算術的に整合 |
| 2 | `cfg(not(windows))` 件数ベースライン「7」（mod.rs 2 + results_window.rs 3 + main.rs 2） | `grep -c "cfg(not(windows))"` を該当 3 ファイルへ実行 | **一致**（mod.rs=2 [521,621], results_window.rs=3 [110,154,169], main.rs=2 [61,137]。合計 7） |
| 3 | 移設対象 mod.rs:521 / 621 が `save_placement_relative` 内の非 Windows ブロック / `results_available_height` の非 Windows 実装であること | 実ファイル該当行を確認 | **一致** |
| 4 | `main.rs:150-193` が `position_on_target_monitor` の関数本体であること | `git show a98312c:src-tauri/src/main.rs` の該当範囲を確認（149 行目が `#[cfg(windows)]`、150 行が `fn`、193 行が閉じ `}`） | **一致** |
| 5 | `position_on_target_monitor` の呼び出し元が `mod.rs:396`（`show_egui_main` 内）の 1 か所のみ | `git grep -n "position_on_target_monitor"` をリポジトリ全体へ実行 | **一致**（呼び出しは mod.rs:396 の 1 件のみ。他は doc コメント上の言及） |
| 6 | `view.rs` のフィールド `last_results_height` / `last_results_width`（287/291 行）、初期化（317-318 行）、ガード本体（858-864 行）、reset-on-show（1194-1195 行）、main 窓ガード（1830 行） | `grep -n` で該当識別子の行番号を実測 | **すべて一致**。main 窓ガードの式 `(height - self.last_set_height).abs() > 0.5 \|\| (width - self.last_set_width).abs() > 0.5` も plan.md の引用と文字どおり一致 |
| 7 | `ResultsWindow::set_size` の呼び出し元が `view.rs:861` の 1 か所のみ | `grep -n "results.set_size"` | **一致**（861 行のみ） |
| 8 | `drive_results_window` の関数範囲「788–876」 | `grep -n "fn drive_results_window"` と閉じ括弧の行を確認 | **一致**（788 行開始、876 行で閉じる） |
| 9 | `max_results` の関数範囲「753–760」、利用点が 818 の 1 か所 | 実ファイルを確認 | **一致** |
| 10 | `mod.rs::` 名指しの doc コメントが layout.rs:102,118 / visual.rs:5 の 3 件で全件 | `git grep -n "mod\.rs::"` をリポジトリ全体へ実行 | **一致**（3 件でこれが全件） |
| 11 | `EguiShellState.main_waker` / `results_waker` が同じ `WindowWaker` 型（mod.rs:88,90） | 実ファイルを確認 | **一致** |
| 12 | `results_available_height` の作業領域取得が main の HWND から、換算は results の scale から、という記述 | mod.rs:600-609 の doc コメントを確認 | **一致** |
| 13 | ADR-0007「却下 1 の第 3 理由」＝main の高さの意図的な 2 導出（`bar_height` collapse と `main_window_height`） | `docs/adr/0007-results-presentation-two-stage.md` を確認 | **一致**（却下案 1 の 3 番目の箇条書きと文字どおり一致） |
| 14 | `SPEC.md:430` が `drive_results_window` を名指し、`SPEC.md:412-415` 相当（`follow_cursor_monitor`／中央フォールバック）は関数名を持たない | `SPEC.md` 該当箇所を確認 | **一致** |
| 15 | `docs/architecture.md:83` / `:172` が `drive_results_window` を含む | `grep -n "drive_results_window" docs/architecture.md` | **一致**（両行とも該当） |
| 16 | plan-review 台帳 4 件の成果物と行数（70/92/64/389） | `wc -l` を実行 | **一致**（`rust-coordinator-move.md`=70, `rust-guard-and-layout.md`=92, `docs-sync.md`=64, `independent-derivation.md`=389） |
| 17 | 不変条件表が I1〜I12 の 12 件で欠番なし | 表の行を数え直し | **一致**（12 行） |
| 18 | `commands/window.rs` / `results_view.rs` が `crate::egui_shell::<名前>` 形でのみ参照し re-export で差分ゼロになるという主張 | `results_view.rs:575`（`crate::egui_shell::wake_main`）・`commands/window.rs:96,143`（`crate::egui_shell::ResultsWindow`）を確認 | **一致** |
| 19 | `mod.rs` の `//!`「window 生成・show/hide・blur 自動非表示・位置永続」、`view.rs` の `//!` が「results 窓 driver」を含まないこと（1-8 行）、`main.rs` の `//!` が位置復元を名指ししないこと | 各ファイルの `//!` を実読 | **一致** |
| 20 | `src-tauri/CLAUDE.md:34` の `view.rs` 要約に「results 窓 driver」という語が実在すること | `grep -n` で確認 | **一致**（`「view.rs は検索 view（…・results 窓 driver・…）」` と実在） |

---

## 未検証（理由）

- **`npm run governance:check` の実行結果**: Phase 5 の完了条件だが、対象ファイルが未作成のため実行しても現状の差分と無関係な結果しか得られない。実装後に実施すべき項目であり、計画時点では検証不能
- **`cargo run -p snotra` によるカテゴリ D の目視 9 項目**: 実機 GUI 操作が要り、本レビュー（内部一貫性のみを見るテキストレビュー）の範囲外
- **`pwsh -File scripts/smoke-egui.ps1 -ResultsQuery <1 文字>` の実測**: 開発機の索引内容に依存し、GUI 起動を伴うため未実施
- **`/race-check` が主張する「`last_size` の書き手はイベントループスレッドの 2 経路のみ」という実行時の排他性**: 静的な grep（呼び出し元の列挙）レベルでは plan.md の記述と実コードが一致することを確認したが、実行時のスレッド境界・lock 順序の妥当性そのもの（デッドロックが実際に起きないか等）は今回のレンズ（文書内の記述矛盾検出）の範囲外であり未検証
- **`docs/superpowers/` 配下「20 件超のヒット」**: `grep -rn` で該当識別子を検索したところ 19 ファイル・193 行がヒットし、「20 件超」という下限見積もりの解釈（ファイル数か行数か）によって厳密な一致/不一致の判定が変わる。文書内矛盾ではなく見積もりの粒度の問題と判断し、指摘対象からは除外した
