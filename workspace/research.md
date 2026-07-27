# research: #666 段 3 — LauncherController / MainView（view.rs の分割）

## issue の要約

`src-tauri/src/egui_shell/view.rs`（1869 行）を責務で分割し見通しを良くする。`egui_shell` 責務分離の 3 段のうち**段 3**（最大）。段 1（#749 `window_coordinator.rs`）・段 2（#752 `WindowPresentation` = `layout::present_results`）はともに CLOSED・main にマージ済み（`5ef346f` / `a98312c`）。

**挙動を変えない移設である。** issue が「着手前に織り込むべき事実」として 7 件を確定済み（再調査不要）。要約は本ファイル「技術的制約」節、逐一の対応は `plan.md`。

issue が示す目標の分離（たたき台）:

| LauncherController | MainView |
|---|---|
| セッション状態 / 検索状態 / 非同期世代 / deadline / 通知 | egui 入力 → UiEvent 変換 / TextEdit / drag / main 窓描画 |

**issue はモジュール割りを指定していない**（ADR-0008「帰結」が「#666（段 3）はモジュール割りを一切指定していない」と明記）。

## 関連コード

### 分割対象 `src-tauri/src/egui_shell/view.rs` の全項目（実測・grep 済み）

**フィールド 19**（`SearchWindowView`・L244-289）:
`app_handle` / `was_focused` / `unfocus_at` / `state` / `search_debounce` / `last_input_at` / `last_set_height` / `applied_font_family` / `applied_background_hex` / `last_set_width` / `folder_tx` / `folder_rx` / `folder_cache` / `folder_error` / `instant_rows_query` / `launching` / `last_seen_index_generation` / `notice` / `notice_base`

**inherent メソッド 25**（`impl SearchWindowView`・L291-970）:
`new` `emit_hide` `activate` `start_launch` `finish_launch` `drain_launch` `clear_search` `execute_slash` `execute_instant_selected` `activate_or_execute` `shift_activate` `execute_tool_selected` `record_folder_expansion` `resolve_tools` `auto_hide_enabled` `settings_running` `instant_prefix` `indexing` `lang` `window_width` `spawn_folder_load` `run_search` `run_search_with` `handle_toast_action` `spawn_install`

**フォント群**（L27-202・176 行）: `JP_FONT_BYTES`(static) / `CJK_PROBE`(const) / `font_covers_cjk` / `font_definitions` / `ResolvedFont`(type) / `USER_FONTS`(static) / `resolve_font_family` / `jp_font_bytes` / `configure_japanese_font`

**その他の自由項目**: `FolderMsg` `LaunchWork` `LaunchTag` `LaunchInFlight`（L206-242）/ `ToastAction` `draw_toast_button`（L972-1016）

**`impl EguiView for SearchWindowView`**: `setup`（L1019-1031）/ `update`（L1033-1762・**730 行**）

**テスト 7**（L1765-1868）: `font_definitions_*` 4 件 + `font_covers_cjk_*` 3 件。**全件がフォント群のテストであり、それ以外の view.rs 項目にユニットテストは 1 件も無い**

### `update()` の文の実行順序（34 段・実測。**この順序の保存が本タスクの中核不変条件**）

1 drag interact → `frame.drag_window()` ／ 2 `read_visual` → `visual`/`metrics` ／ 3 `reset_pending` 消費（`state.reset` / cache / error / `instant_rows_query` / debounce 再構築 / `launching=None` / `notice.clear` / `ResultsWindow::reset_size_guard`）／ 4 `ctx = ui.ctx().clone()` ／ 5 index 世代検知 → `run_search` ／ 6 `pending_hotkey_failure` 消費 → `notice.set` ／ 7 `ctx.set_visuals` ／ 8 `font_family_changed` → `configure_japanese_font` ／ 9 `background_hex_changed` → `window.set_background_color` ／ 10 `drain_launch` ／ 11 `notice.poll` / `notice.remaining` ／ 12 folder drain → cache/error 適用 → `run_search` ／ 13 `focused` / `escape` 読み ／ 14 focused → `unfocus_at=None` ／ 15 Escape ラダー ／ 16 blur 検知 → `unfocus_at=Some` ／ 17 `blur_grace_action` ／ 18 **↑↓ の消費**（`input_mut` + `events.retain`）→ `move_selection` ／ 19 → キー（folder 展開 + `record_folder_expansion` + `spawn_folder_load`）／ 20 ← キー ／ 21 **TextEdit 構築** ／ 22 `response.changed()` → interp 分岐（debounce / `run_search_with` / `execute_slash`）／ 23 `request_focus` ／ 24 status 行描画（`overlay_kind`）／ 25 updater toast 描画 ／ 26 `handle_toast_action` ／ 27 trailing debounce poll + 再 arm ／ 28 **Enter 判定（後置）** → flush → `shift_activate` / `activate_or_execute` ／ 29 `plain_results_hidden` → `show_results` ／ 30 snapshot publish + `wake_results` ／ 31 `take_clicked_for` → `activate_or_execute` ／ 32 `main_window_height` → `size_delta_exceeds` → `set_size` ／ 33 `drive_results_window`（`result_count` はここで読む）／ 34 `was_focused = focused`

`frame: &mut RuntimeFrame` の用途は **1 の `drag_window()` のみ**（実測）。

### view.rs 外の消費者（grep 実測）

| 参照元 | 参照先 | 種別 |
|---|---|---|
| `egui_shell/mod.rs:77,308` | `view::SearchWindowView`（`new` + `runtime.attach`） | コード |
| `egui_shell/results_view.rs:442,478` | `view::configure_japanese_font`（**2 箇所**） | コード |
| `commands/launch.rs:50` | `egui_shell::view::SearchWindowView::activate` / `execute_tool_selected` | doc コメント |
| `commands/instant.rs:19` | `egui_shell::view::execute_instant_selected` | doc コメント |
| `egui_shell/window_coordinator.rs:9,405` | `SearchWindowView` / `SearchWindowView::update()` | doc コメント |
| `egui_shell/results_window.rs:7,54` | `SearchWindowView` | doc コメント |
| `egui_shell/results_view.rs:1` | `SearchWindowView` | doc コメント |
| `egui_shell/visual.rs:86` | `configure_japanese_font` | doc コメント |

**フォント群は消費者が 2 モジュール**（main の view と `results_view`）である。ADR-0008 の規則 R は「複数のモジュールから消費されるものは残す」と定めており、**この 1 事実がフォント群の行き先を決める**（どちらの view にも入れられない）。

### 文書側の参照（`*.md`・実測。`.superpowers/sdd/**` は歴史記録ゆえ対象外）

- `src-tauri/CLAUDE.md`「モジュール構成」— `egui_shell/` のファイル一覧行と `view.rs` の責務散文、「フォント登録」節（「規則の正本はテストと `view.rs` の `//!`」）
- `docs/architecture.md:80,125,147,156` — `egui_shell/view.rs` を 4 箇所で名指し
- `.claude/skills/state-check/SKILL.md:40` — 「主な置き場所」に `view.rs`（キー入力分岐・driver）

## 既存パターン

- **ADR-0008 規則 R（段 1 の先例）**: 「移設する関数がその中でしか使わないヘルパーは一緒に運ぶ。複数のモジュールから消費されるものは残す」。**例外ゼロで説明できることが規則の要件**とされ、例外が 1 つ出た候補規則（却下 2）はそれを理由に落ちている
- **`//!` に「不在」を明記する様式**: `window_coordinator.rs` の `//!` は「z-order は本モジュールに無い」「main 窓のサイズは 2 か所に分かれたまま」を明記する（ADR-0008 決定 3・却下 5）。全称表現を前提条件なしで書かないため
- **状態を持たない従属 view の先例**: `results_view::ResultsView` は `RowsSnapshot` を描くだけで検索状態を持たない（一方向データフロー・#646 PR2 決定 5）
- **段 1 の移設規模との対比**: `window_coordinator.rs` は 479 行・11 関数の移設で ADR 1 本を要した

## 技術的制約

### issue が確定済みの 7 件（再調査不要・原文は issue #666 コメント）

1. **入力変換は 1 段にできない**: Escape/↑↓/→← は TextEdit の**前**で**消費**（`events.retain`）が要る（#700 の実バグ）。Enter/Shift+Enter は `response.changed()` に依存するため TextEdit の**後**。→ pre-widget / post-widget の 2 段
2. **外部イベントの消費は 7 箇所**: `take_clicked_for` は他 6 つと同カテゴリの破壊的 take でありながら **snapshot publish の後**という別フェーズに固定（#699）。包括的な名前を付けると例外が不可視化する
3. **順序制約は `update()` の全長にわたる**: `SearchState::reset()` が `rows_generation += 1` を撃つため、冒頭の reset と末尾のクリック照合が結ばれる
4. **load-bearing な「弱い」順序が 2 つ**: (a) `drain_launch` の timeout/Failed/Disconnected 分岐は `notice.set` するが自前の `request_repaint` を持たず、deadline を張る唯一の主体が `notice.remaining()` ブロック。(b) folder drain は後ろへ #699 の世代照合、前へ `reset()` の `folder_gen` bump（`accept_folder_result` の stale 棄却）と結ばれる
5. **反映境界は 4 つ**: `ui.visuals_mut()` / `ctx.set_visuals`（次 pass から効く）/ `ctx.set_fonts`（次 pass 冒頭で消費）/ `window.set_background_color`（OS 窓）。**egui 0.35.0 では `ctx.set_visuals` は現在の pass の `Ui` に届かない**（root `Ui` が pass 冒頭で `ctx.global_style()` を `Arc` snapshot する）。view.rs L1139-1140 の「`set_visuals` はウィジェット描画より前である必要がある」というコメントは**このバージョンでは成立していない**。潜在バグは #751（OPEN・**本タスクの対象外**）
6. **モードは導出のまま残す**: `ViewKind` は `tool.is_some()`/`folder.is_some()`、`QueryIntent` は query 文字列からの導出。明示 enum フィールドを重複させない。ADT 置換（`enum ViewState`）は可読化目的には費用対効果が低いと評価済み
7. **全域 `Effect` enum は作らない**: 順序制約が `Vec<Effect>` の並びへ移るだけ。型を導入する基準は「呼び出し側が処理を忘れると不変条件が破れること」（`EscapeOutcome` が成功例）

### 検証の制約（issue「検証の制約」節）

**フレーム順序の不変条件に自動検出器は無い。** trace の presence を見るスモークはこのクラスの回帰を緑のまま通した実例がある（#671 PR A′）。実際の検出器は `docs/build-commands.md` カテゴリ D の**目視**である。

### ビルド・検証の制約

- `-D warnings`（`cargo clippy --workspace --all-targets`）: **未使用の新 API は `dead_code` で落ちる**。型・関数の定義と呼び出し点の移行を同一フェーズに束ねる必要がある（`AGENTS.md` 条件別チェック表「関数・型を新規定義／改名／導入」）
- 発火する検証カテゴリ（`docs/build-commands.md`）: **A**（`cargo check/clippy/test -p snotra` + `cargo doc --workspace --no-deps --document-private-items`——`//!`/`///` を大量に触るため intra-doc link 切れが実リスク・hook 非発火）／ **C**（`npm test` + `smoke:startup` + `smoke:egui`。スラッシュコマンド経路 `execute_slash` を移設するため）／ **D**（`cargo run -p snotra` 目視。フォント登録に触る変更では `src-tauri/CLAUDE.md`「フォント登録」が省略を禁じている）／ **F**（`npm run governance:check`——`.md` とモジュール索引を触るため。`.md` の沈黙は合格ではない）
- `.rs` の追加/削除は `src-tauri/CLAUDE.md` モジュール構成のファイル名行の追加/削除が必須（`AGENTS.md` 条件別チェック表）

### 型・所有の制約（実測）

- `SearchWindowView` は `runtime.attach(window, view)` へ move される（`mod.rs:308`）。`EguiView` トレイトの `setup(&mut self, &egui::Context)` / `update(&mut self, &mut egui::Ui, &mut RuntimeFrame)` を実装する型は 1 つでなければならない
- `app_handle: tauri::AppHandle` は clone 可能（`Manager` ハンドル）。config live-read ヘルパー 6 件（`lang` / `indexing` / `instant_prefix` / `auto_hide_enabled` / `settings_running` / `window_width`）はすべて `app_handle` のみに依存し、他フィールドを読まない
- `read_visual` の戻り値 `VisualSnapshot` は **`self.` へ保持してはならない**（寿命は 1 フレーム・`visual.rs` の `//!`）

## 未解決の疑問（いずれも解決済み・解決先を記す）

- ~~**分割の粒度（モジュール数と境界）は issue も ADR-0008 も指定していない。**~~ → 設計書 §1 の**規則 R(段 3)**（4 条項・例外ゼロ）で決着。3 モジュール（`launcher_controller.rs` / `view.rs` / `font_stack.rs`）
- ~~`.claude/skills/state-check/SKILL.md` の更新水準~~ → 本 PR で L40 のファイル名のみ更新（判定ロジックは無変更）。`CLAUDE.md` 最重要ルール 2 に従い差分を提示して**合意を得た**（2026-07-27）。`safety-nets.md` が要求する `/norm-review` は「上限 1 巡・塞ぎ 1 件 1 文まで・条項を足さない」の停止条件で `plan.md` Phase 4 に組み込み済み

## 本調査の取りこぼし（記録）

上の「文書側の参照」を作るとき `grep -rn "view\.rs" --include=*.md . | head -40` を使い、**アルファベット順で後ろの `PERFORMANCE.md`（3 箇所）が `head` に切られて落ちた**。`AGENTS.md`「列挙も SSOT のツール自身に問う」に反した形で、`/plan-review` のスカウトが検出した。**参照の全件は `plan.md`「変更ファイル一覧」が正本である**（`.rs` 内 47 件・`driver` 45 件の母集団を含む）。
