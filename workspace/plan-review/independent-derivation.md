# #749 段 1（WindowCoordinator）— 独立導出

作成: 2026-07-27 / 基点コミット `a98312c`（段 2 = PR #756 マージ済）
方法: issue #749・#752・#666、ADR-0007、コードベース、規約文書のみから再導出した。`workspace/plan.md` / `workspace/research.md` / `workspace/plan-review/` の他ファイルは読んでいない。

---

## 1. 要件の理解

### 1-1. WHAT（issue から）

`egui_shell` 再編 3 段のうち段 1。**窓の可視性・位置・サイズ・z-order・wake を 1 つの責務（WindowCoordinator）へ集める。** 挙動は変えない。新規ファイル `src-tauri/src/egui_shell/window_coordinator.rs` を作り、そこへ集約する（ユーザー判断・2026-07-27）。

issue の自己評価「8 割は既に `mod.rs` に集まっている／欠けている一片は `drive_results_window`」を実測で確認した。**おおむね正しいが、2 点で不足している**（後述 1-3）。

### 1-2. 集約対象の実測（責務 → 実体 → 現在地）

| 責務 | 実体 | 現在地 | 種別 |
|---|---|---|---|
| main 可視性 | `show_egui_main` / `hide_egui_main` | `mod.rs:366` / `:460` | driver（副作用） |
| main hide の受け口 | `register_hide_listener` | `mod.rs:531` | listener 登録 |
| results 可視性 | `ResultsWindow::show` / `hide` | `results_window.rs:58` / `:76` | 所有型（raw Win32） |
| results 可視性の駆動 | `drive_results_window` | `view.rs:788`（`&mut self`） | driver |
| results 位置 | `position_results_below_main` | `mod.rs:580` | driver（Win32 読み + 適用） |
| results 高さの材料 | `results_available_height`（cfg 2 arm） | `mod.rs:612` / `:622` | Win32 読み |
| results サイズ | `ResultsWindow::set_size` / `scale_factor` / `set_position` | `results_window.rs:126` / `:135` / `:140` | 所有型（tao 経由） |
| results z-order | `ResultsWindow::set_topmost`（cfg 2 arm） | `results_window.rs:90` / `:111` | 所有型 |
| main z-order | `main.set_always_on_top(false/true)` | `commands/window.rs:94` / `:140` | 直呼び |
| wake | `wake_main` / `wake_results` | `mod.rs:548` / `:563` | primitive |
| wake の受け口 | `register_config_wake_listeners` | `mod.rs:630` | listener 登録 |
| 位置の永続 | `save_placement_relative` | `mod.rs:505` | driver |
| main 位置（起動/show 時） | **`position_on_target_monitor`** | **`main.rs:150`** | driver（`#[cfg(windows)]`） |
| main サイズ（毎フレーム） | `set_size` + `last_set_height` / `last_set_width` | `view.rs:1824-1837` | driver |
| main サイズ（show 時 collapse） | `read_metrics(app).bar_height` → `set_size` | `mod.rs:392-393` | driver |

### 1-3. issue の表に無い実体（独立導出で見つけた 2 件）

1. **`position_on_target_monitor`（`main.rs:150-193`）は「main 窓の位置」そのものである。** 呼び出し元は `show_egui_main` 1 箇所だけで（`git grep` = 定義 1 + 呼び 1）、`monitor::{cursor,primary}_monitor_work_area` と `window_data::load_search_placement` を使う。`save_placement_relative`（保存側）が `egui_shell/mod.rs` にあるのに復元側が `main.rs` にある**非対称**であり、「位置を 1 つの責務へ集める」を額面どおり取るなら移設対象に入る。
   → **これは「所見」であって「推奨」ではない。スコープの決定は issue のオーナーに属する**（issue の責務表に載っておらず、確定事項も「新規ファイルを作ってそこへ集約する」までしか言っていない）。選択肢と代償だけ置く:

   | 選択 | 代償 |
   |---|---|
   | 含める | `main.rs` を触る差分が増える（現状 `main.rs` 内の private fn ゆえ可視性の調整が要る）。`monitor.rs` への依存が `egui_shell` へ入る。保存（`save_placement_relative`）/ 復元の対称が回復し、「位置」の集約が完成する |
   | 含めない | 差分最小。ただし**「位置を 1 つの責務へ集めた」は偽になる**——`AGENTS.md`「全称表現は前提条件とセットで書く。書けないなら書かない」に照らし、**PR 本文か `src-tauri/CLAUDE.md` に「main 窓の位置決め（`position_on_target_monitor`）は `main.rs` に残る」と明記する**ことが条件 |

2. **main 窓の毎フレーム `set_size`（`view.rs:1824-1837`）は集約対象**にしない**方がよい。** 理由は ADR-0007 却下 1 の第 3 理由と同型: main の高さは `show_egui_main` の `bar_height` collapse（位置クランプの順序制約のため）と毎フレームの `main_window_height` の**意図的な 2 導出**であり、幅は `window_width()`（config live-read）で `layout.rs` を通らない。ここを動かすと段 2 が明示的に外へ置いた設計判断を段 1 が巻き戻す。

### 1-4. 明示的に含めない（境界の宣言）

- **`AppState.main_visible` の所有権移動**: `main.rs:417`（hotkey トグル判定）・`commands/system.rs:47`（テスト）・`state.rs:18` が読む。coordinator へ引き取るのは移設ではなく**所有権の変更**で、`AtomicBool` の Ordering 契約（`SeqCst`・`ResultsWindow.visible` / `hotkey_generation` と揃えてある）も巻き込む。挙動不変の段 1 の外。
- **`ResultsWindow` 型そのものの `window_coordinator.rs` への吸収**: `results_window.rs` は既に単一責務（raw Win32 の所有点）で、`//!` が「得られないもの」を明記している。吸収すると `Deref` 非実装・raw 3 点セットの根拠が散る。**別ファイルのまま coordinator が使う**のが正しい。
- **`layout.rs` の純粋核**（`present_results` / `clamp_results_height` / `results_top_y` / `available_below` / `main_window_height` / `results_window_height` / `Metrics`）: 段 2 で位置が確定済み（ADR-0007）。段 1 で動かさない。
- **`wake_results` の呼び出し点 `view.rs:1801`**: snapshot 差分の edge wake であり、publish 経路に属する。coordinator へ移すと「wake は primitive として公開し、いつ起こすかは保持者が決める」（#711・issue の制約）に反する。
- **`commands/window.rs` の topmost 対（`main.set_always_on_top` / `results.set_topmost`）**: 監視スレッドから来る非同期経路で、`SettingsProcessState` のライフサイクルに束ねられている。z-order を「集める」と読むなら候補だが、**移すと `commands/window.rs` → `egui_shell` の依存方向が増える**。段 1 では `ResultsWindow::set_topmost` を通す現状を維持し、「z-order の所有点は `ResultsWindow`」と記述で閉じるのが最小。

---

## 2. 変更集合の列挙

### 2-1. 形（shape）の選択 — 計画が明示すべき分岐

`drive_results_window` は `&mut self` で、view-local な 2 フィールド `last_results_height` / `last_results_width`（`view.rs:285-291`）を書く。さらに `self.state.results().len()` と `self.max_results()` を読む。**純粋な関数移設ではない。** 3 つの形と代償:

| 形 | 代償 |
|---|---|
| (a) Coordinator を managed state 化（`ResultsWindow` 同様） | デルタガードがスレッド跨ぎになり内部可変性が要る → 並行性の姿勢が変わる（`/race-check` 領域）。「意味変化ほぼゼロ」ではなくなる |
| (b) Coordinator を `SearchWindowView` のフィールドにする | ガードは単一スレッドのまま。ただし `hide_egui_main` / `wake_*` / `position_results_below_main` は `main.rs` / `commands/window.rs` / `Moved` リスナーから自由関数として到達可能でなければならず、結局 2 系統になる |
| (c) 自由関数を `window_coordinator.rs` へ置き、`mod.rs` が `pub(crate) use` で再エクスポート。デルタガードは小さな struct（例 `ResultsDriveState { last_height, last_width }`）にまとめて `SearchWindowView` が保持し、`&mut` で渡す | 差分最小。`main.rs` / `platform/mod.rs` / `commands/window.rs` の呼び出し点は無変更。ガードの reset（`view.rs:1194-1195`）が `state.reset_guards()` の 1 行になる |

**私の結論: (c)。** 根拠は 3 つ。① `-D warnings` 下で「新 API を作って呼び出し点は後で移す」が `dead_code` で落ちる（`AGENTS.md`「条件別チェック」の「関数・型を新規定義／改名／導入」）ので、移設と呼び出し点移行は 1 コミットに束ねるしかなく、束ねられるのは差分最小の形だけ。② `mod.rs` の既存の `pub(crate) use` は**すべて消費者を名指すコメントを持つ**（`mod.rs:8-51`）。再エクスポートはこの様式に従えばよく、新しい規約を要らない。③ managed state を 1 つ増やすと、`try_state` が `Option` を返す消費点が増え、**manage 前の消費が沈黙して skip される**穴（3-4 参照）も増える。

### 2-2. ファイル → シンボル → 変更内容

#### 新規

| ファイル | 内容 |
|---|---|
| `src-tauri/src/egui_shell/window_coordinator.rs` | `//!` に責務宣言（「窓の可視性・位置・サイズ・z-order・wake の driver。純粋な導出は `layout.rs`、raw Win32 の所有点は `results_window.rs`」）。下表の移設先 |

#### 移設（`mod.rs` → `window_coordinator.rs`）

| シンボル | 現在地 | 備考 |
|---|---|---|
| `show_egui_main(app, t0)` | `mod.rs:366-451` | 内部の `#[cfg(windows)]` ブロック 3 つ（bar_height collapse / focus 同期 / IME オフ）ごと移す。`read_metrics` への依存が `mod.rs` → `window_coordinator` の向きになる |
| `hide_egui_main(app)` | `mod.rs:460-501` | `working_set::trim_idle_working_set` 呼び出しごと。**`main_visible=false` を `results.hide()` の前に置く順序を保つ** |
| `register_hide_listener(app)` | `mod.rs:531-536` | |
| `save_placement_relative(window)` | `mod.rs:505-526` | `#[cfg(windows)]` / `#[cfg(not(windows))]` の 2 arm を両方 |
| `wake_main(app)` | `mod.rs:548-552` | |
| `wake_results(app)` | `mod.rs:563-567` | 「1 関数に束ねない」の doc ごと |
| `register_config_wake_listeners(app)` | `mod.rs:630-641` | |
| `position_results_below_main(app) -> Option<i32>` | `mod.rs:580-598` | `layout::results_top_y` を呼ぶ。`layout` は `mod.rs` の private mod なので **`mod.rs` の `mod layout;` を `pub(crate) mod layout;` にするか、`crate::egui_shell::layout::` 経由にする**（現状 `view.rs` が `crate::egui_shell::layout::` で参照しているので既に到達可能） |
| `results_available_height(app, top_y) -> Option<f64>` | `mod.rs:612-624` | **`#[cfg(windows)]` と `#[cfg(not(windows))]` の 2 arm を両方**（3-3 参照） |

#### 移設（`view.rs` → `window_coordinator.rs`）

| シンボル | 現在地 | 変更内容 |
|---|---|---|
| `SearchWindowView::drive_results_window` | `view.rs:788-876` | 自由関数 `drive_results_window(app, &mut ResultsDriveState, plain_hidden, result_count, width, metrics)` へ。**`result_count` を引数化する**（`self.state` に触れないため）——これが最大の意味変化で、読み点の責務が呼び出し側へ移る（4-1） |
| `last_results_height` / `last_results_width` | `view.rs:287` / `:291` | `ResultsDriveState`（新規小型 struct）へ。`view.rs:1194-1195` の reset は `state.reset()` 相当の 1 メソッドへ |
| `SearchWindowView::max_results` | `view.rs:755-760` | **移さない**——`drive_results_window` 以外の消費点が無いか要確認（`git grep max_results` = `view.rs:755` 定義 + `:818` 呼びの 2 件のみ）。呼び出し側で評価して引数に載せるか、coordinator が `app` から読む。**coordinator が読む方を推す**（`layout::ResultsInputs.max_results` の材料であり view の状態ではない） |

#### 呼び出し側の更新

| ファイル | 箇所 | 変更 |
|---|---|---|
| `view.rs:1838` | `self.drive_results_window(...)` | 自由関数呼び出しへ。**行の位置を動かさない**（クリック逆流の消費 `view.rs:1809` より後という順序が不変条件） |
| `view.rs:1194-1195` | ガード reset | `ResultsDriveState` のメソッドへ |
| `mod.rs:287` | `Moved` リスナー内の `position_results_below_main(&handle)` | パス修飾のみ（再エクスポートなら無変更） |
| `mod.rs:203` | `spawn_update_check` 末尾の `wake_main(&handle)` | 同上 |
| `mod.rs:678-679` | `register_initial_hotkey_failure_listener` 内の `show_egui_main` / `wake_main` | 同上 |
| `main.rs:267, 311, 315, 429, 432, 446, 570` | `egui_shell::{show_egui_main, hide_egui_main, register_hide_listener, register_config_wake_listeners}` | **再エクスポートを保てば無変更**（`egui_shell::` パスのまま）。無変更で済むことを PR 本文で明示する |
| `results_view.rs:575` | `crate::egui_shell::wake_main` | 同上 |
| `view.rs:853, 875, 1072, 1801` | `crate::egui_shell::{results_available_height, wake_results, wake_main}` | 同上 |
| `mod.rs:8-51` の再エクスポート群 | — | `pub(crate) use window_coordinator::{...};` を消費者名指しコメント付きで追加 |
| `mod.rs:1-2` の `//!` | — | 責務から「show/hide・位置永続・hide/config-wake listener」を外し、`window_coordinator.rs` を指す |

#### 文書

| ファイル | 箇所 | 変更 |
|---|---|---|
| `src-tauri/CLAUDE.md:34` | `egui_shell/` のファイル一覧 + 各ファイルの責務散文 | **`window_coordinator.rs` を追加（G1 必須・4-6）**。`mod.rs` の責務散文から show/hide・位置永続・listener を移す |
| `src-tauri/CLAUDE.md:35` | 「外部から窓を起こす経路は `wake_main` / `wake_results` の 2 本だけ」 | 位置が変わるだけで文言は真のまま。パス言及があれば更新 |
| `src-tauri/CLAUDE.md:104, 107` | 「実装は `egui_shell::ResultsWindow` に集約」「show 述語側のゲート（`egui_shell::layout::present_results`）」 | 変更不要（どちらも動かさない） |
| `src-tauri/CLAUDE.md:49` | 「show の操作順序制約（`egui_shell::show_egui_main`）」 | パスは `egui_shell::` のまま真（再エクスポート）。要確認 |
| `src-tauri/CLAUDE.md:82` | working set の節（`egui_shell::hide_egui_main` 合流点） | 同上 |
| `SPEC.md:430` | 「可視性・サイズ・位置は `main` の毎フレーム更新（`drive_results_window`）が駆動する」 | **関数名を名指ししている唯一の SPEC 行。** 名前は保つ想定なので文言は真だが、責務の所在（view → coordinator）を反映するなら更新 |
| `SPEC.md:387, 504, 506-520` | §8.5 表 / §8.6 従属軸 | **変更不要**（概念記述で関数名を出さない。挙動不変ゆえ意図も不変） |
| `docs/architecture.md:83` | 「`results` の位置・可視性は `main` の毎フレーム更新（`drive_results_window`）が駆動する」 | 更新（責務の所在）。**G2 に注意**——ファイル単位のモジュール表行を再導入してはならない |
| `docs/architecture.md:172` | mermaid シーケンス図の `drive_results_window` | 更新 |
| `src-tauri/src/working_set.rs:3` の `//!` | 「hide 経路（`egui_shell::hide_egui_main` 合流点）」 | 再エクスポートなら真のまま |
| `src-tauri/src/events.rs:37` | `EGUI_HIDE_REQUESTED` の doc（`hide_egui_main` の 1 経路） | 同上 |
| `src-tauri/src/events.rs:19, 24` | `register_initial_hotkey_failure_listener` の doc 参照 | 同上（この関数は移さない想定） |
| `layout.rs:102, 118` | 「`mod.rs::position_results_below_main` の算術部」「`mod.rs::results_available_height` の算術部」 | **`mod.rs::` を `window_coordinator.rs::` へ**（3-2 クラス B） |
| `layout.rs:172-173` | 「`wake_main` 自体は main の可視性を見ない」「hide 側の同期（`hide_egui_main`）」 | 名前のみ・真のまま |
| `visual.rs:5` | 「（`mod.rs::position_results_below_main`）」 | **パス修飾を更新**（クラス B） |
| `results_view.rs:4-5` | 「窓の可視性・サイズ・位置の driver は main 側」「hide は外部（`hide_egui_main` / main の `drive_results_window`）が所有」 | **概念ラベル。「main 側」が偽になる**（3-2 クラス C） |
| `results_window.rs:5-6, 17, 50, 64` | 「片方の hide 経路（`drive_results_window`）だけが更新し」「`egui_results:show` は `drive_results_window` が 1 回だけ出す」 | 名前は残るが所在の含意が変わる |
| `mod.rs:457, 471, 481-490` の doc/コメント | 「`view.rs` の `drive_results_window`」「update 外＝ここ／update 内＝`drive_results_window`」 | **「update 外/update 内」は概念ラベルで、両方が同じファイルに来ると読者を誤らせる**（3-2 クラス C） |
| `view.rs:773, 1192-1193, 1769, 1776` のコメント | `drive_results_window` を名指す 4 箇所 | 所在の更新 |

#### テスト

- **移設するテストは 0 件。** `view.rs` の `#[cfg(test)]` は `font_definitions_*` 4 件 + `font_covers_cjk_*` 3 件のみ（`view.rs:1844-1940`）で `drive_results_window` に触れない。`layout.rs` のテスト群（`present_results_*` 3 件 / `clamp_results_height` / `results_top_y_*` / `available_below_*`）は `layout.rs` に残る。
- **ゆえに受け入れ条件は「テスト件数が不変で全緑」。** 実測ベースライン（`a98312c`・本セッションで実行）: `cargo test -p snotra` = **174 passed / 0 failed / 2 ignored**。
- **新規テストを足せる箱は無い**（段 1 の対象はすべて driver で `AppHandle` を要る）。これは受容残余として明記する。段 2 が「唯一カバレッジが手に入る箱」だったのは issue #752 の主張どおり。

---

## 3. 間接参照の分類と洗い出し

列挙の SSOT はツールに問うた。`git grep` を `*.rs` に限定し、`.superpowers/` と `docs/superpowers/`（歴史資料・#589 で非規範化）は規範対象から外した。

### 3-1. クラス A: 当の語をそのまま grep すれば当たる（直接参照）

`*.rs` 内の出現件数（定義行を含む）:

| シンボル | `*.rs` の出現 | 実呼び出し点 |
|---|---|---|
| `show_egui_main` | 12 | `main.rs` 4（single-instance / hotkey ShowNow / alt 解放待ち / startup display）+ `mod.rs:678` |
| `hide_egui_main` | 13 | `main.rs:429`（hotkey HideNow）+ `mod.rs:534`（listener） |
| `position_results_below_main` | 6 | `mod.rs:287`（Moved）+ `view.rs:838`（drive） |
| `results_available_height` | 4 | `view.rs:853` |
| `save_placement_relative` | 2 | `mod.rs:467` |
| `wake_main` | 10 | `mod.rs:203, 638, 679` + `results_view.rs:575` + `view.rs:1072` |
| `wake_results` | 3 | `view.rs:875, 1801` |
| `drive_results_window` | 15 | `view.rs:1838` |
| `ResultsWindow` | 15 | `try_state` 消費 **6 箇所**: `mod.rs:484, 581, 616` / `view.rs:796` / `commands/window.rs:96, 143` |
| `register_hide_listener` | 2 | `main.rs:311` |
| `register_config_wake_listeners` | 3 | `main.rs:315` |
| `EguiShellState` | 15 | — |
| `main_visible` | 30 | — |

### 3-2. クラス B: 同概念・別名 — パス修飾された散文（grep 語では当たるが、直すべきは前置のパス）

`` `mod.rs::<fn>` `` の形で他モジュールから名指されている。**関数名で grep すると当たるが、直すのは関数名ではなく `mod.rs::` の部分**である。移設後も関数名が同じなら「壊れていないように見えて、指す先が嘘になる」。

- `layout.rs:102` — 「`mod.rs::position_results_below_main` の算術部」
- `layout.rs:118` — 「`mod.rs::results_available_height` の算術部」
- `visual.rs:5` — 「results 窓の配置（`mod.rs::position_results_below_main`）」

**この 3 件は `git grep 'mod\.rs::'` で網羅できる**（実測 3 件・すべて上記）。移設時は必ずこのクエリを打つこと。

### 3-3. クラス C: 概念ラベル — シンボル名を含まないため grep で到達しない（最重要）

移設後に**文法的に正しいまま意味だけが偽になる**散文。検出器は無い。

| 場所 | 現在の文 | 移設後に偽になる理由 |
|---|---|---|
| `results_view.rs:4` | 「窓の可視性・サイズ・位置の driver は **main 側**（hidden 窓は update() が走らないため）」 | driver は依然 main のフレームから呼ばれるが「main 側 = `view.rs`」の含意が崩れる |
| `mod.rs:481-483` | 「`update()` の外から results を hide する経路はここだけ／`drive_results_window` は update **内**で動く対の経路」 | 両方が同じファイルに来ると「ここ」「対の経路」が指す先が読者に見えない |
| `mod.rs:457-458` | 「**results の hide はここを通らない経路がある**（`view.rs` の `drive_results_window`）」 | `view.rs` の指定が偽になる |
| `results_window.rs:5-8` | 「可視フラグは `SearchWindowView` 側の view-local な bool であり」（歴史記述） | 歴史として真だが、`SearchWindowView` が driver でなくなった後は読者が現在形と誤読しうる |
| `view.rs:771-773` | 「main（本 view）が両窓の唯一の size writer に一意化されている（results への幅適用は `drive_results_window` 経由で main が担い）」 | 「本 view」が driver でなくなる |
| `SPEC.md:430` | 「可視性・サイズ・位置は `main` の毎フレーム更新（`drive_results_window`）が駆動する」 | 関数名が残れば真だが、`main` の含意（= main 窓の view）が薄れる |
| `docs/architecture.md:83` | 同型 | 同型 |
| `src-tauri/CLAUDE.md:34` | 「`view.rs` は検索 view（… **results 窓 driver** …）」 | **明確に偽になる**——ここは責務列挙なので必ず直す |

**クラス C の網羅方法**: 語で引けないので、`results 窓` / `driver` / `main 側` / `update 内` / `update 外` / `size writer` のような**概念語**で `*.rs` と規範 `*.md` を舐めるしかない。私が上で列挙したのがその結果である。**完全性は保証しない**（`AGENTS.md`「全称表現は前提条件とセットで書く」）。

### 3-4. クラス D: 同名・別概念（誤爆させないための除外リスト）

- **`layout.rs:381, 456-468` の `main_visible`** — ユニットテストのローカル変数・クロージャ引数。`AppState.main_visible` とは別物。
- **`commands/system.rs:47` の `main_visible`** — テスト用 `AppState` 構築。
- **`view.rs:1830-1837` の `set_size`** — main 窓のサイズ。results の `set_size`（`view.rs:861`）と同名だが別窓・別概念。
- **`last_set_width`（main 用）と `last_results_width`（results 用）** — `view.rs:288-291` の doc が明示的に「流用しない」と書いている。統合する誘惑があるが**同一フレーム内で main のブロックが先に `last_set_width` を更新するため、比較すると常に差分 0 になり results が幅の live-reload に追従しなくなる**。
- **`request_repaint` 系 20 箇所超** — 自窓 Context の repaint（正しい経路）であって `wake_*`（外部・別窓からの wake）ではない。`src-tauri/CLAUDE.md:35` が区別を SSOT で定めている。coordinator へ集めない。
- **`scale_factor`** — `ResultsWindow::scale_factor`（results 窓）と `main.scale_factor()`（`mod.rs:589`・`show_egui_main:387`）。`layout.rs:110` / `:122` が「型では区別できない」と明記。移設で行が混ざると取り違えが起きやすい。

### 3-5. `.rs` 以外の参照（`git ls-files` ベース・歴史資料を除く）

`git grep -ln` で `*.md` / `*.ps1` / `*.ts` / `*.mjs` / `*.yml` / `*.json` を舐め、`.superpowers/` を除外した結果 **7 件**:

`SPEC.md` / `docs/adr/0007-results-presentation-two-stage.md`（歴史記録・訂正不要） / `docs/architecture.md` / `docs/build-commands.md`（trace 名の記述） / `docs/development-principles.md:109`（`hide_egui_main` の歴史記述・訂正不要） / `scripts/smoke-egui.ps1`（trace 名） / `src-tauri/CLAUDE.md`。

**`scripts/smoke-egui.ps1` と `docs/build-commands.md` は trace イベント名でのみ結合している**（`egui_show:done` / `egui_hide:done` / `egui_results:show` / `egui_results:hide`）。関数を移しても名前を変えなければ壊れない。**逆に、移設のついでに trace 名や `"from"` フィールド（`"hide_main"` / `"drive"`）を整理すると smoke が黙って skip / 失敗する。触らないこと。**

---

## 4. 黙って壊れうる不変条件と検出器の有無

### 4-1. 読み点の非対称（`plain_hidden` は pre-click / `result_count` は post-click）

**現状**: `drive_results_window` は `plain_hidden` を引数で受け（`view.rs:1770` で pre-click に評価）、`count` を関数本体で `self.state.results().len()` から読む（`view.rs:803`）。関数の呼び出し行（`view.rs:1838`）がクリック逆流の消費（`view.rs:1809`）より後にあることで、後者が post-click になる。ADR-0007 帰結が「恒久的な制約」と呼んでいるもの。

**移設で何が変わるか**: 自由関数化すると `self.state` に触れないので `result_count` は**引数になる**。引数式は呼び出し行で評価されるので post-click のままだが、**不変条件を担う担保が「関数本体の位置」から「呼び出し式の位置」へ移る**。引数が 2 つ（`plain_hidden` = pre-click, `result_count` = post-click）になると、**リストの見た目上は同格なのに読み点が違う**という、極めて壊れやすい形になる。将来「引数評価を呼び出し行の手前へまとめる」リファクタは自然に見え、しかも `cargo test` では落ちない。

**検出器**: **無い。** `layout.rs` の等価グリッドは「pre-click 件数 == post-click 件数」のフレームしか固定しない（ADR-0007 帰結・明記済み）。カテゴリ D の目視でも「行クリック起動時に古い行が 1 フレーム描かれる」は人間の目にほぼ映らない。
**緩和**: **引数名は `layout::ResultsInputs` のフィールド名（`plain_hidden` / `result_count`）と一致させたまま**にし、非対称の記述は**1 か所に閉じる**——不変条件の SSOT は `present_results` の doc（`layout.rs:178-185`）であり、呼び出し行（`view.rs:1838`）にそこを名指すコメントを 1 本置く。**独自の別名（`*_pre_click` / `*_post_click`）を作らない**——同じ概念に 2 つの語彙ができ、SSOT と派生コピーの照合という悪い形になる（`AGENTS.md`「照合は SSOT に対して行う」）。

### 4-2. `hide_egui_main` の順序（`main_visible = false` が `results.hide()` の前）

**壊し方**: 移設時に行を並べ替える、あるいは `if let Some(state)` ブロックを見やすさのために下へまとめる。

**検出器**: **部分的にしか無い。** `scripts/smoke-egui.ps1:428-443` に orphan 検査がある（最後の `egui_hide:done` **より後**の行に `egui_results:show` が無いこと）。ただし:
- **gate されている**: `$resultsChecked`（`-SeedConfig` で config.toml を新規 seed できた、または `-ResultsQuery <letter>` 明示）かつ `$failures.Count -eq 0` のときだけ走る。**ローカルの素の `npm run smoke:egui` では skip されうる。** CI は `-RequireResults`（#686）で skip を失敗に変えている。
- **順序入れ替えに対するカバレッジは非決定的である。** hotkey 経路の `hide_egui_main` は platform メッセージループスレッドで走り（`src-tauri/CLAUDE.md`「`app.listen` のコールバックは emit した呼び出し元スレッド上で同期実行される」）、再表示する側は egui イベントループスレッドのフレームなので、両者は本当に並走する。**その `egui_results:show` が `egui_hide:done` の前に落ちるか後に落ちるかを決めるものは何も無い**——`results.hide()` と `egui_hide:done` の間には `trace_main("egui_results:hide")` と `trim_idle_working_set`（Toolhelp でプロセスツリーを走査する。マイクロ秒では終わらない）が挟まる（`mod.rs:484-500`）。orphan 検査は「最後の `egui_hide:done` **より後ろ**」しか見ないので、**タイミング次第で捕まえたり素通ししたりする**。**確率的な検出器は、計画の受け入れ条件としての検出器ではない。**
- orphan 検査が**決定的に**捉えるのは「`main_visible=false` が**丸ごと消えた**」クラスである（hide 完了後の `config-applied` / `indexing-*` / updater wake が必ず再表示を起こすため、必ず `egui_hide:done` より後ろに出る）。
- **ゆえに issue #749 の「実際の検出器はカテゴリ D の目視」は、#690 以降も**この不変条件については**正しい**。issue 本文は #690 の orphan 検査を知らずに書かれているが、結論は変わらない。正確に言い直すなら「presence を超えた区間検査が 1 本あるが、それは (i) `-SeedConfig` / `-ResultsQuery` で gate され、(ii) 順序入れ替えクラスに対しては非決定的にしか鳴らない」である。

**緩和**: 移設後の `hide_egui_main` に、順序を守る理由のコメント（現行 `mod.rs:470-475`）を**そのまま**運ぶ。行の順序を変えないこと自体を PR 本文のチェック項目にする。

### 4-3. `#[cfg(windows)]` / `#[cfg(not(windows))]` の双子 arm

対象は 2 組:
- `results_available_height`（`mod.rs:612` / `:622`）— **移設対象**
- `ResultsWindow::set_topmost` / `raw_show` / `raw_hide`（`results_window.rs`）— 移設対象外だが隣接

**壊し方**: 移設時に `#[cfg(not(windows))]` の arm を落とす。Windows では**コンパイルが通る**。

**検出器**: **無い。** `docs/build-commands.md` の CI 対応表によれば `ci.yml` の rust-check は **windows** で走る（node-check だけ ubuntu で、cargo は回さない）。したがって非 Windows arm は**誰も一度もコンパイルしない**。clippy も `cargo check --workspace` も Windows 上では通る。落としても永久に気づかない。
**緩和**: 移設の diff で `cfg(not(windows))` の出現数を数える（移設前後で不変であること）。**ベースライン（本セッションで実測・`a98312c`）**: `src-tauri/src` 全体で **10 件**（`commands/launch.rs` 2 / `egui_shell/mod.rs` **2**（`results_available_height` と `save_placement_relative` の非 Windows arm・**どちらも移設対象**） / `egui_shell/results_window.rs` 3 / `main.rs` 2 / `working_set.rs` 1）。移設後も合計 10 件で、`mod.rs` の 2 件が `window_coordinator.rs` へ移っていること。

### 4-4. `wake_results` の無条件（level-triggered）呼び出し

`drive_results_window` 末尾（`view.rs:875`）の無条件 `wake_results`。`#673` 決定 5 / #697 で「edge 化してはならない」と決まっている——results は config 系イベントを listen せず、visual だけの変更では `RowsSnapshot` が不変ゆえ差分 wake も出ない。

**壊し方**: 移設のついでに「毎フレーム wake は無駄」と見えて条件を付ける。あるいは早期 return の位置を変えて到達しなくする。

**検出器**: **無い**（自動）。カテゴリ D の目視でのみ検出できる: 実行中に `config.toml` の `[visual]` の色や `font_size` を変更し、**results 窓が（入力せずに）新しい色で描き直されるか**を見る。これは PR 本文の目視項目に必ず入れる。

### 4-5. `Moved` リスナーと managed state の構築順序

`position_results_below_main` は `try_state::<ResultsWindow>()` を引く。`Moved` リスナーは `create()` の**中**（`mod.rs:283-290`）で、`attach` より前に登録される。`app.manage(handles.results_window)` は `main.rs:309`——**リスナー登録より後**である。

**現状はどうなっているか**: `try_state` は `Option` を返し、`let (Some(main), Some(results)) = ... else { return None }` で**沈黙して skip** する。setup ブロック中はイベントループの 1 イテレーション内なので `Moved` は実際には来ない（`src-tauri/CLAUDE.md`「setup フック自身もイベントループの中で走る」）。ゆえに現状は安全だが、**その安全は型ではなく setup の実行機構が担っている。**

**移設で何が変わるか**: 形 (c)（自由関数）なら不変。形 (a)（Coordinator を managed state 化）を選ぶと、**managed state が 1 つ増え、同じ穴が 1 本増える**。しかも新しい state の manage 位置を間違えても panic せず沈黙して skip する。

**検出器**: **無い**（`if let Some(..)` は沈黙する）。形 (c) を選ぶ理由の 1 つ。

### 4-6. モジュール索引（`src-tauri/CLAUDE.md:34`）の更新漏れ

**検出器: ある。** `npm run governance:check` の **G1**（`scripts/governance-check.mjs:76-127`）が `src-tauri/CLAUDE.md` の「モジュール構成」節と `src-tauri/src/**.rs` を**双方向**で照合する。新ファイルが索引に無ければ「実ファイル … が索引（本文のバッククォート）に見当たらない」で落ちる。
**ベースライン（本セッションで実測・`a98312c`）**: `governance:check — G1..G11 passed（対象文書 40 件 / rules 7 件 / skills 12 件 / 恒久規範 常時ロード 15798/15823 字・rules 8408/8418 字 / 見出し参照 104 件を 51 文書から照合）`。
**注意**: G1 は `snapshot.files`（追跡ファイル）を母集団にする。**`git add` していない新規ファイルは母集団に入らず、緑のまま通る**可能性がある。索引更新の検算はステージ後に行うこと。
**注意 2**: `docs/architecture.md` を直すときは **G2**（ファイル単位モジュール表行の再導入禁止）に触れないこと。散文・mermaid はよいが、表の行にしない。
**注意 3**: `*.md` の編集に PostToolUse hook は付かない（`selectChecks` が空集合）。**沈黙は「何も走らなかった」であり合格ではない**（`docs/build-commands.md` カテゴリ F）。

### 4-7. `-D warnings` / `dead_code` による段階分割の不可能性

`window_coordinator.rs` に関数を作って呼び出し点を後のコミットで移す形は、`cargo clippy --workspace --all-targets -- -D warnings` が `dead_code` で落とす。**移設と呼び出し点移行は同一コミットで完結させる**（`AGENTS.md`「新 API の導入と呼び出し点の移行は 1 タスクに束ねる」）。
**検出器: ある**（clippy / PostToolUse hook）。これは味方の検出器。

### 4-8. `mod.rs` の `pub(crate) use` に消費者コメントを付ける慣習

`mod.rs:8-51` の再エクスポートは**例外なく**「誰が何のために消費するか」のコメントを持つ。新しい `pub(crate) use window_coordinator::{...}` にも同じ形が要る。
**検出器: 無い**（規約のみ）。レビューで見る。

### 4-9. `show_egui_main` の 3 段順序（高さリセット → 位置 → show）

`src-tauri/CLAUDE.md:49` が SSOT。移設時にブロックを並べ替えると、展開時の高さで位置クランプが効き、折りたたみ時に位置がずれる。
**検出器: 無い**（自動）。カテゴリ D の目視（toast を出した状態で hide → 再 show して位置が動かないか）。

### 4-10. 挙動不変のベースライン差分検証

`AGENTS.md`「挙動変更なし前提は代表入力/出力をベースライン化して差分検証する」。段 1 は driver の移設なので、**ベースラインは trace 列**が唯一取れるもの。移設前後で `SNOTRA_TRACE=1` の trace 列（イベント名と順序）が同一であることを見る。
- 対象イベント: `egui_show:done` / `egui_hide:done` / `egui_results:show` / `egui_results:hide`（`from` フィールド込み） / `egui_show:ime_control` / `egui_show:no_window` / `egui_results:click_stale`。
- **これは presence でなく順序の比較**である（`src-tauri/CLAUDE.md:28`「trace の presence 検査は状態の検査ではない」）。

---

## 5. 検証コマンド（`docs/build-commands.md` の分類で）

### カテゴリ A（`*.rs` 変更・必須）

```bash
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p snotra                       # ベースライン: 174 passed / 0 failed / 2 ignored
cargo doc --workspace --no-deps --document-private-items
```

- `cargo doc` は **PostToolUse hook が発火しない**（CI のみ）。本 PR は `//!` と `///` を大量に触るので**ローカル手動実行が必須**（intra-doc link 切れ）。`layout.rs:102` / `:118` / `visual.rs:5` の `mod.rs::` 表記は現状バッククォート散文で intra-doc link ではないため `cargo doc` では捕まらない点に注意（3-2 の grep で拾う）。
- clippy と `cargo test -p snotra` は PostToolUse hook が自動発火する（沈黙 = 合格）。

### カテゴリ C（ウィンドウ生成／表示順に触れる・必須）

```bash
npm test
npm run smoke:startup
npm run smoke:egui        # ← 素で回すと results 検査が skip されうる
```

- **`smoke:egui` は必ず results 検査が走る形で回す。** 素の実行では `-SeedConfig` で config.toml を新規 seed できないと（既存 config があれば seed しない）results 検査が黄色 NOTE で skip され、**本 PR の当の対象が一切検証されない**。開発機では `-ResultsQuery <既存索引に一致する 1 文字>` を明示する:
  ```powershell
  pwsh -File scripts/smoke-egui.ps1 -ResultsQuery z
  ```
  実行後にサマリで results 検査が SKIPPED でないことを目で確認する。
- PR CI では `Smoke` workflow（`e2e.yml`）が `src-tauri/**` の paths で自動起動し `-RequireResults` を渡す。**「通常 PR CI が緑」は smoke 済みを意味しない**（`docs/build-commands.md` カテゴリ C の注記）。

### カテゴリ D（目視・**必須**・issue が明示要求）

```bash
cargo run -p snotra
```

PR 本文に列挙する目視項目（各項目にどの不変条件を見ているかを併記する）:

1. hotkey → main 表示 → 1 文字入力 → results が **main 直下 + gap** に出て、**2 文字目が打てる**（フォーカスを奪わない = `SW_SHOWNOACTIVATE`）
2. Escape → **main と results が同時に消える**。results だけ最前面に残らない（4-2）
3. hotkey で hide（`hotkey_toggle`）→ 同上。**platform スレッド経路**を通るのでこちらも必ず見る（4-2 の並走が起きうるのはこの経路）
4. main を**ドラッグ移動** → results が追従する（`Moved` リスナー経路・4-5）
5. クエリを全消し / ヒット 0 件 → results が消える。再入力で出る（4 連言 ②）
6. インデックス再構築中に打鍵 → plain 結果の results が出ない（連言 ③ carve-out）
7. **アプリ実行中に `config.toml` の `[visual]` の色 / `font_size` を変更** → **入力せずに** results が新しい色・行高で描き直される（4-4 の level-triggered wake。**これが無条件 wake の唯一の観測点**）
8. `window_gap` を変更 → results の間隔が変わる（`position_results_below_main` の別 lock 読み）
9. main を**画面下端付近**へ移動して検索 → results 高さが作業領域下端でクランプされ、1 行 + 8px の床を割らない（`clamp_results_height`）
10. トレイ / `/settings` から設定画面を起動 → **main も results も** topmost が解除され設定の下に潜る。設定を閉じると両方復帰する（`commands/window.rs` の対称）
11. toast を出した状態（`$env:SNOTRA_EGUI_FAKE_UPDATE = "1"`）で hide → 再 show → **位置がずれない**（4-9 の高さリセット順序）
12. マルチモニター環境で hotkey → 保存位置が復元され results も同じモニターに追従（`position_on_target_monitor` を移設する場合は必須）

### カテゴリ F（ガバナンス文書変更・必須）

```bash
npm run governance:check
```

- `src-tauri/CLAUDE.md` / `SPEC.md` / `docs/architecture.md` を触るので必須。**G1 の検算は `git add` 後に行う**（4-6）。
- ベースライン（`a98312c`）: G1..G11 passed / 対象文書 40 件 / 見出し参照 104 件。

### 補助（挙動不変のベースライン差分・4-10）

```powershell
$env:SNOTRA_TRACE = "1"; cargo run -p snotra 2> before.log   # 移設前
$env:SNOTRA_TRACE = "1"; cargo run -p snotra 2> after.log    # 移設後
# 同じ操作列（show → 1 文字 → Escape）を踏み、event 名の列を比較する
```

---

## 6. 判断に迷った点・未検証の観点

### 迷った点

1. **`position_on_target_monitor` を段 1 に含めるか**（1-3）。**独立導出としてはどちらにも倒さない**——スコープの決定は issue オーナーの判断であり、私の仕事は「issue の責務表が 1 件取りこぼしている」を surface することまでである。1-3 に選択肢と代償の表を置いた。どちらを選んでも、選んだ側の条件（含めない場合は全称表現の限定）を満たすこと。

2. **`ResultsWindow` を `window_coordinator.rs` へ吸収するか**。吸収すれば「1 つの責務」に見えるが、`results_window.rs` の `//!` が担っている「raw 3 点セットの所有点・`Deref` 非実装の理由・得られないもの」という別の命題が薄まる。**吸収しない**を推す。ただし「WindowCoordinator という 1 責務」が 2 ファイルに跨ることになるので、`window_coordinator.rs` の `//!` で分担を明記する必要がある。

3. **`max_results()` を coordinator が読むか、引数で渡すか**。`layout::ResultsInputs.max_results` の材料であり view の状態ではないので coordinator が `app` から読む側を推したが、**engine lock を 1 フレームで何回取るかが変わる**（現状 `drive_results_window` 内で `self.max_results()` が 1 回取っている。移しても回数は同じ）。挙動不変は保たれる見込みだが、lock の取得点が `visual` snapshot（`view.rs:1158`）より後である事実は保つこと。

4. **coordinator の形を (c) 自由関数にすると「Coordinator という型」が存在しない。** issue は「1 つの責務へ集める」としか書いておらず型を要求していないので整合するが、「WindowCoordinator」という名前が型を期待させる。`ResultsDriveState` だけが型になる形になる。**名前と実体の乖離を PR 本文で明示すべき**（型を作るなら (a)/(b) の代償を払う）。

### 未検証の観点

- **4-2 の「順序入れ替えに対して orphan 検査が非決定的」は導出であって実測ではない**（並走する 2 スレッドの相対順序に上界を与えるものが無い、という論証まで）。実装者への推奨として**フォールトインジェクション**を挙げる: `hide_egui_main` の 2 行を意図的に入れ替えたビルドで `smoke:egui -ResultsQuery z` を数回回し、**緑で通る回があるか**を見る。1 回でも緑なら「検出器ではない」が確定する。**本レビューでは実行していない**——検査対象（`hide_egui_main`）を変更しながら検査を走らせることになり、`AGENTS.md`「委譲した検査が対象を読む時刻は制御できない＝検査対象を変更しながら検査を走らせない」に抵触するうえ、独立導出の読み取り専用の枠を超えるため。**この検算の価値は高い**——「検出器がある」と「この不変条件に届く検出器がある」は別で、前者を後者と誤読するのが #671 PR A′ の再発形である。
- **クラス C（概念ラベル）の網羅は保証していない**（3-3）。語で引けないため、私の目視走査が漏れている可能性がある。`/plan-review` Step 2b の独立再導出をもう 1 枠かけるか、`egui_shell/` 配下の `//!` と `///` を全文読み直す枠を取るのが確実。
- ~~`layout` モジュールの可視性~~ **解決済み（実測）**: `mod.rs:6` は `mod layout;`（**private**）。それでも `view.rs` が `crate::egui_shell::layout::present_results` で参照できるのは、Rust の private 項目が「定義モジュールとその子孫」から見えるためである。`window_coordinator.rs` も `egui_shell` の子になるので**修飾を変えずにそのまま使える**。`mod layout;` を `pub(crate) mod` へ広げる必要は無い（広げると `egui_shell` 外からも見え、段 2 が閉じた境界を緩める）。
- **`cargo test -p snotra` = 174 件のうち `egui_shell/` 由来が何件かは数えていない。** 「テスト件数不変」の検算には全体件数で足りるが、移設で誤って `#[cfg(test)]` ブロックを巻き込んだ場合の切り分けには内訳が要る。
- **段 3（#666）との衝突予測をしていない。** #666 は `view.rs` の分割で、段 1 が `view.rs` から `drive_results_window` と 2 フィールドを抜く。段 3 が同じファイルを大きく動かすため、**マージ順を決めておく**必要がある（`CLAUDE.md`「並列エージェント委譲はファイル境界で衝突を予測してから行う」）。段 1 → 段 3 の順が自然。
