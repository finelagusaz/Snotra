# research — #749 段1: WindowCoordinator

対象 issue: #749「段1: WindowCoordinator — 窓の可視性・位置・サイズ・z-order・wake を 1 つの責務へ集める」
ブランチ: `chore/window-coordinator`（基点 `a98312c`）

## 1. issue の要約

`egui_shell` を 4 責務へ分ける再編の**段 1**。窓の可視性・位置・サイズ・z-order・wake を 1 つの責務（WindowCoordinator）へ集める。

- **段 2（#752・PR #756）はマージ済み**（`a98312c`）。着手順は入れ替わっており、本 issue が最後発ではなく段 3（#666）が残る
- issue 自身が「8 割は既に `mod.rs` に集まっている」「欠けている一片は `drive_results_window`。これを引き取るのが本 issue の実質」と書いている
- **挙動は変えない。** issue は「いま壊れている挙動があるわけではない。規約とコメントだけが不変条件を支えている」と明記する

### 切り出し範囲（ユーザー判断・2026-07-27）

「WindowCoordinator を新ファイルとして実体化するか / mod.rs へ引き取るだけか」を問い、**新ファイルへ集約**が選ばれた。`window_coordinator.rs` を新設し、mod.rs の窓操作群と view.rs の `drive_results_window` を移す。

### 段 3（#666）からの制約

`gh issue view 666` の本文は 1 行（「責務に応じて分割して、見通しをよくする」）のみで、**モジュール割り・ファイル名の指定は無い**。ゆえに段 1 の形は段 3 から拘束されない。逆に、段 1 が `main.rs` の managed state 構成を組み替えると段 3 の前提が動くため、**managed state の構成は変えない**方針を採る（下記 4-A）。

## 2. 関連コード（すべて grep で実在確認済み）

### 移設元 A: `src-tauri/src/egui_shell/mod.rs`（681 行）

| 関数 | 行 | 責務 | 移設 |
|---|---|---|---|
| `show_egui_main(app, t0)` | 366 | main の show 列（高さ collapse → 位置 → show → main_visible → focus → IME） | する |
| `hide_egui_main(app)` | 460 | main の hide 列（世代 bump → 位置保存 → hide → main_visible=false → results.hide → trim） | する |
| `save_placement_relative(window)` | 505 | 作業領域原点相対で `window.bin` へ保存 | する |
| `register_hide_listener(app)` | 531 | `EGUI_HIDE_REQUESTED` → `hide_egui_main` | する |
| `wake_main(app)` | 548 | main 窓の wake primitive | する |
| `wake_results(app)` | 563 | results 窓の wake primitive | する |
| `position_results_below_main(app) -> Option<i32>` | 580 | main 直下へ results を配置し**上端物理 y を返す** | する |
| `results_available_height(app, top_y) -> Option<f64>` | 612 / 622 | 上端から作業領域下端までの論理高さ（windows / not(windows) の 2 実装） | する |
| `create(app, w, bg)` | 227 | 両窓生成・`Moved` リスナー登録・attach | **しない** |
| `EguiShellState` / `EguiShellHandles` | 78 / 212 | 共有状態・引き渡し型 | **しない** |
| `read_metrics` / `read_visual` | 326 / 348 | config 由来の 1 フレーム読み | **しない** |
| `spawn_update_check` | 129 | updater | **しない** |
| `register_config_wake_listeners` / `register_hotkey_failure_listener` / `register_initial_hotkey_failure_listener` | 630 / 644 / 669 | listener 登録 | **しない** |
| `apply_rounded_corners` | 303 | DWM 角丸（`create` 専用の private） | **しない** |

### 移設元 B: `src-tauri/src/egui_shell/view.rs`（1948 行）

| 要素 | 行 | 内容 |
|---|---|---|
| `drive_results_window(&mut self, plain_hidden, width, metrics)` | 788–876 | results の可視性・位置・サイズ・wake の毎フレーム driver |
| 呼び出し点 | 1838 | `update()` 末尾（`take_clicked_for` は 1809・`plain_hidden` 算出は 1770） |
| `last_results_height` / `last_results_width` | 287 / 291 | set_size のデルタガード（`&mut self` を要求する唯一の理由） |
| 生成時初期値 | 317–318 | いずれも `0.0` |
| reset-on-show での 0 復帰 | 1194–1195 | `reset_pending` 消費ブロック内 |
| `max_results(&self) -> u32` | 755 | config live-read。**利用点は 818 の 1 か所のみ**（grep で確認） |

### 依存先（変更しない純粋核）

- `layout::present_results(ResultsInputs) -> ResultsPresentation`（186）— SPEC §8.6 の 4 連言。ユニットテスト済み（`present_results_truth_table_distinguishes_all_four_conjuncts` 他）
- `layout::clamp_results_height`（94）/ `results_window_height`（72）/ `results_top_y`（113）/ `available_below`（128）
- `results_window::ResultsWindow`（`show` / `hide` / `set_topmost` / `set_size` / `set_position` / `scale_factor`）

### 外部呼び出し元（移設で壊れうる）

| 呼び出し元 | 参照 |
|---|---|
| `main.rs:267,432,446,570` | `egui_shell::show_egui_main` |
| `main.rs:429` | `egui_shell::hide_egui_main` |
| `main.rs:311` | `egui_shell::register_hide_listener` |
| `view.rs:838,853,875,1072,1801` | `position_results_below_main` / `results_available_height` / `wake_results`（**875 の drive 末尾と 1801 の snapshot 差分検知の 2 か所**）/ `wake_main` |
| `results_view.rs:575` | `wake_main` |
| `mod.rs:203,288,679` | `wake_main`（updater 完了）/ `position_results_below_main`（`Moved` リスナー）/ `wake_main`（初回 hotkey 失敗） |
| `commands/window.rs:96,99,142,145` | `ResultsWindow::set_topmost`（**設定サイドカー監視のポーリングスレッド**から） |

**すべて `crate::egui_shell::<名前>` 形の参照である。** mod.rs で `pub(crate) use window_coordinator::{...}` と re-export すれば呼び出し元の差分はゼロになる（mod.rs が全サブモジュールを re-export する既存様式と一致）。

### 文書側の参照（同期対象）

| 位置 | 記述 |
|---|---|
| `SPEC.md:430`（§8.5） | 「可視性・サイズ・位置は `main` の毎フレーム更新（`drive_results_window`）が駆動する」 |
| `docs/architecture.md:83` | 同旨 |
| `docs/architecture.md:172` | シーケンス図 `View->>View: ... drive_results_window` |
| `src-tauri/CLAUDE.md`「モジュール構成」 | `egui_shell/` のファイル一覧（新規ファイル名行の追加が要る） |
| `layout.rs:102,118` / `visual.rs:5` | doc コメント中の `mod.rs::position_results_below_main` / `mod.rs::results_available_height` |

`docs/superpowers/` 配下の plan / spec は**歴史記録**ゆえ更新しない。

## 3. 既存パターン（再利用できるもの）

- **サブモジュール + mod.rs での re-export**: `results_window` / `layout` / `visual` / `notify` などすべてこの形。`window_coordinator` も同型で足せる
- **`ResultsWindow` の「遷移したときだけ true を返す」idiom**: `show()` / `hide()` は内部フラグを `swap` し、遷移時のみ raw 操作を撃って `true` を返す。デルタガードを同型で `set_size` へ入れられる
- **純粋核 + driver の分離（`EscapeOutcome` / `present_results`）**: 判定は値で返し副作用は driver が実行する。段 2 で results の判定は既にこの形になっており、段 1 は driver 側の置き場を決めるだけ
- **`//!` を責務の正本にする**（#562）: 新規ファイルの責務は `//!` に書き、`src-tauri/CLAUDE.md` にはファイル名行だけ足す

## 4. 技術的制約（再導出しないこと）

### A. 窓ごとの層を混在させない（`src-tauri/CLAUDE.md`「Win32 / Tauri 注意事項」）

- main = show / hide / topmost の 3 操作すべて tao 経由。**main の show だけ raw にして統一するのは禁止**（tao の `VISIBLE` が stale 化し `set_always_on_top` が main を消す）
- results = 3 操作すべて raw（`SW_SHOWNOACTIVATE` / `SW_HIDE` / `SetWindowPos`）
- raw へ寄せるかの判定基準は「`apply_diff` を通るか」ではなく「**フラグ差分が生じるか**」。`set_size` / `set_position` は差分が空ゆえ tao 経由のままでよい
- `ResultsWindow` に `Deref<Target = tauri::Window>` を実装しない
- **表現不能化は達成できない**（`results_window.rs` の `//!`）。`Manager` から生ハンドルを引く書き方はコンパイルが通り黙って no-op する。本再編の目的は「正しい経路を 1 つにする」ことであって表現不能化ではない

### B. 読み点の非対称（ADR-0007 / `present_results` の doc / #752 F2）

同一フレーム内で:
- 連言③ `plain_hidden` は**クリック逆流の消費（`take_clicked_for`・view.rs:1809）より前**に 1 回だけ読む（`indexing` は `AtomicBool` の live-read ゆえ 2 回読むとフレーム内で食い違う）
- 連言②の材料 `result_count` は**消費より後**に読む（間の `start_launch` が `set_results(Vec::new())` を撃つ）

**読み点を前へ寄せると、行クリック起動フレームで古い行が 1 フレーム描かれる。`cargo test` では落ちない。** 移設で `result_count` を引数へ切り出すと、呼び出し側で `plain_hidden` の算出（view.rs:1770）と並べて書きたくなる——これが本 PR で唯一「黙って壊せる」不変条件である。

### C. wake は primitive のまま（#711）

`wake_main` / `wake_results` は「呼ばれたら起こすだけ」で公開する。armed 期限（検索 debounce・一時通知・起動タイムアウト・blur 猶予の 4 本）は**保持者が毎フレーム再要求する**契約であり、Coordinator が「いつ起こすか」を決めた瞬間にこの契約が壊れる。`request_repaint_after` 系は `SearchWindowView` に残す。

### D. `drive_results_window` 末尾の無条件 `wake_results` を edge 化しない（#673 決定 5 / #697）

results は config 系イベントを一切 listen せず、visual だけの変更では `RowsSnapshot` が不変ゆえ差分 wake も出ない。この level-triggered wake が新しい色を描く唯一の保証である。移設で**そのまま運ぶ**。

### E. `hide_egui_main` の順序不変条件（#671 PR A′）

`main_visible = false` は `results.hide()` の**前**。show 側は `window.show()` の**後**に `true`。どちらも「main が可視でない期間に `visible=true` と読ませない」向き。移設で行の並びを変えない。

### F. 可視判定にはクランプ**前**の高さを渡す（#675 / ADR-0007）

クランプには上端が要り、上端は位置決めが決めるが、位置決めは可視判定の**後**にある（不可視なら早期 return し `SetWindowPos` を撃たない）。`desired_height`（判定用）と `applied_height`（`set_size` 用・デルタガードが覚える値）の別名は #752 F5 の成果ゆえ保つ。

### G. Win32 依存モジュールはユニットテスト前提にしない（`.claude/rules/src-tauri.md`）

`ResultsWindow` は `tauri::Window` を持つためテストできない。デルタガードをこの型へ入れると判定式もテスト外になる——**判定式だけを `layout.rs` の純粋関数へ出せば、0.5 の許容境界にテストを置ける**（本 PR で唯一得られる自動カバレッジ）。

### H. `commands/window.rs` のポーリングスレッド

`set_topmost` の呼び出し元は設定サイドカー監視の `std::thread::spawn`（104 行）である。`ResultsWindow` は managed state のまま据え置くので**到達経路は変わらない**が、`ResultsWindow` へ内部可変フィールド（デルタガード）を足すため、`Send + Sync` 要件と lock 順序は `/race-check` の対象になる。

### I. 検証手段（issue「検証」節）

- trace の presence を見るスモークは、このクラスの回帰を**緑のまま通した実例がある**（`egui_results:hide` は出るのに窓は残った・#671 PR A′）
- PostToolUse hook（clippy + crate テスト）の沈黙はここのカバレッジにならない
- 実際の検出器は `docs/build-commands.md` **カテゴリ D の目視**であり、人間の目が要る。**検出器の無い不変条件は受容残余として PR 本文に明記する**

## 5. 未解決の疑問

なし。設計上の分岐（新ファイル化の是非）はユーザー判断で解決済み、デルタガードの行き先は下記 plan.md で決定した。

- 参考: `max_results()` は view.rs での利用点が `drive_results_window` の 1 か所だけであることを grep で確認済み。ゆえに view に残す理由がなく、coordinator へ一緒に移す（残すと view→coordinator の引数経路と view-local ヘルパーが二重に残る）
