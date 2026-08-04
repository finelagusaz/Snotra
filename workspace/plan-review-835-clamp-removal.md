# plan-review — issue #835 クランプ撤去の網羅性と hide 契約

## 要対処

- `src-tauri/src/egui_shell/mod.rs:315` — `results` 窓生成時のコメント「初期値。実高は main が実件数フィットで設定」が「実件数フィット」という撤去対象の概念を名指ししたまま残る。計画の `mod.rs` 行（変更ファイル一覧）は 38 行（`results_available_height` の名を消す）だけを挙げており、315 行は対象外になっている。固定高へ書き換えた後は「実高は max_results 分の固定高で設定」等へ直す必要がある（grep 実測: `grep -rn "実件数フィット" --include=*.rs` の唯一の未計画ヒット）。

- `src-tauri/src/egui_shell/layout.rs:600-602` — `present_results_truth_table_distinguishes_all_four_conjuncts` のテスト doc が「**16 行のうち 4 行は到達不能である。** 「②false ∧ ④true」は生の入力から構成できない（`result_count = 0` なら高さも 0 になる）」と書いている。D1 で `results_window_height` から `result_count` を落とすと、④（窓高さ>0）は `max_results` だけで決まり `result_count` と無関係になるため、`result_count = 0 ∧ max_results > 0` で「②false ∧ ④true」が**到達可能になる**（従来は不可能だった行が新設計では構成できる——`present_results` 自体は D2 の `result_count > 0` 連言でこの行を正しく Hidden へ倒すので出力は壊れないが、この doc の論証はそのまま偽になる）。計画の変更ファイル一覧は同テストについて「606」（`let h = 3.0 * 37.0 + 8.0;` のリテラル）しか挙げておらず、591-602 の doc 段落は対象外になっている。

- `src-tauri/src/egui_shell/window_coordinator.rs:670-676` — `position_results_below_main` 自身の doc が「**設定した results 上端の物理 y を返す**（#675）。高さのクランプに上端が要る…**計算した値を捨てる関数は、次の利用者に写しを書かせる。**」と、`Option<i32>` を返す理由を明示的に説明している。計画は D3 でこの関数の戻り値を `()` へ落とすと明記している（撤去後は 2 呼び出し点のどちらも値を使わない）が、変更ファイル一覧の `window_coordinator.rs` 行には `results_available_height`（削除）と `drive_results_window` 845-849（`applied_height`→`desired_height` と 842-844 行の doc 削除）しか無く、665-676 のこの doc 段落自体が対象外になっている。戻り値を落とせば、この doc の存在理由（写しを書かせないため返す）がそのまま矛盾した記述として残る。

- `scripts/lib/SnotraTraceInvariants.psm1:16` — H4 不変条件の説明が「`egui_results:show` の `rows` が 0 なら異常 | 「高さ 0 ⇔ hide」の契約違反（`layout::present_results`）」と書かれている。この文言は現行の「`desired_height > 0.0` が 0 件 hide を代行する」設計をそのまま指しており、D2 でその代行を `result_count > 0` という独立連言へ切り出した後は、正本が `present_results`（0 件は連言②が直接効く）と `results_window_height`（D4 で「高さ 0 ⇔ hide」の doc の正本が移る先。ただし移設後の意味は `max_results == 0` という別の edge case に限定される）とで分裂する。**このファイルは計画の変更ファイル一覧に一切現れない**（`results_window_height` / `clamp_results_height` / `present_results` を名指す非コード資産を `grep -rln` した 3 件——`docs/architecture.md`・`src-tauri/CLAUDE.md`・本ファイル——のうち前 2 件だけが計画に載っている）。H4 の判定ロジック自体（rows==0 の show は異常）は変更後も真であり続けるため実害は無いが、契約の名指し（「高さ 0 ⇔ hide」）は撤去対象の概念そのものであり、SPEC.md §8.6 の連言分離を追わずに残ると読者が誤った正本を辿る。

## 軽微

- `src-tauri/src/monitor.rs` — `window_monitor_work_area` を削除すると、そこでしか使われていない `HWND`（`use windows::Win32::Foundation::{HWND, POINT}`）と `MonitorFromWindow`（`use ... MonitorFromPoint, MonitorFromWindow`）の import が未使用になる（`HWND` は同ファイル内で `window_monitor_work_area` の中でしか使われておらず、`POINT` は他 3 関数が使うため残る）。計画の D3 表は 5 シンボルの削除を挙げるが、この 2 import は挙げていない。ただし `cargo build`（`cargo clippy` を待たずに素の `cargo build` でも `unused_imports` は警告になる）が Phase 3 の検証ステップで即座に落とすため、静かに見逃される種類の漏れではない。
- `src-tauri/src/egui_shell/window_coordinator.rs:833-841` — `drive_results_window` 内のクランプ説明コメント（「作業領域の下端でクランプする（#675）。あふれた行は既存の ScrollArea が拾う。」から「**可視判定（上の `present_results`）には `desired_height`（クランプ前）を使う。**」までの段落）は、計画が明示的に挙げる「842-844 行の doc」よりも広い範囲がクランプの存在を前提にしている。`clamp_results_height` 呼び出し（845-849）を消す同じ編集で自然に目に入る位置ではあるが、計画の行番号引用（842-844）はこの段落の一部しか指していない。

## 未検証

- 5 シンボル撤去後に `cargo build -p snotra` / `cargo clippy --workspace --all-targets -- -D warnings` を実際に走らせて dead_code / unused_imports の連鎖がこれ以上ないことを確認してはいない（本レビューは grep と静的読解のみ。撤去対象の呼び出し点はいずれも grep で 1 箇所ずつ実測したが、コンパイラでの裏取りはしていない）
- `.claude/worktrees/round2-findings/` に本リポジトリの並行ワークツリーが存在し、`docs/adr/ADR-main-window-clamp-on-pointer-release.md` 等の同名ファイルを含むことを確認したが、これが本計画・本 issue と関係する作業かは未確認（別軸の #738 関連ドキュメントに見え、本レビューの対象外と判断したが事実関係の裏取りはしていない）
