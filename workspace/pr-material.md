# PR 本文の材料（#1336）

## 数

- 字数（`LC_ALL=C.UTF-8 wc -m`）: 29,849 → 5,512
- 太字（`**` の個数）: 232 → 30
- `##` 見出し: 8 → 7（`###` は 0 へ）
- 分類（95 条項・混在は主たる側で数える）: 1 = 10 / 2 = 0 / 3 = 45（うち 3+ = 19）/ **4 = 18（本書に残る）** / 5 = 22
  - 1: M4・M10・W1・V5・V6・R1・R3・I5（本 PR で private 化）・F1・X8（変異 10/10 CAUGHT。I5 は委譲先が E0616 を確かめる）
  - 4: M0・M17（索引）・P1〜P5・P7・E1・E2・D1・D3・V7・T1・N2・G1・G3・G4

## 削除台帳（本書から消えた条項 → 移した先 / 消してよい理由）

### 冒頭
- A1 `[lib]` 無し → 5: 失敗は即時で自己説明的。正しい形は `docs/build-commands.md` カテゴリ A
- A2 読み方の説明 → 5: 書式の宣言で、絞った本書には不要

### モジュール構成
- M0 → 4（1 文へ縮約）
- M1 コヒーレンシを 2 フラグから導かない → `AppState` の `indexing` フィールド doc
- M2 mark_stale → CAS → spawn・drain → `indexing.rs` の `//!`・`start_index_build` doc
- M3 索引の手順は `build_index_from_material` 1 か所 → 同関数 doc（「PATH マージの規則を変えるときは起動経路も」を足した）・`main.rs` の PATH マージ箇所から相互に指す
- M4 木とマスクは組 → 1（`IndexMaterial` の可視性・E0616）
- M5 icon 破棄は `update_config` の後 / M6 `IndexInputs` 差分・`!indexing` ゲート無し → `apply_config_change` 本体コメント（`config_watcher.rs` の `//!` の指しを本体へ直した）
- M7 発火イベントの列挙 → 5: 数え上げ。正本は `apply_config_change` の `emit`
- M8 `commands/`・`platform/` の責務 → 新設の `commands/mod.rs`・`platform/mod.rs` の `//!`。公開関数の列挙は 5
- M9 hotkey は `ParsedHotkey` だけを変換 → 新設の `platform/hotkey.rs` の `//!`
- M10 子モジュールの可視性 → 1（コンパイラ）
- M11 ソーステキスト検査の置き場 → `activation/tests.rs` の `//!`
- M12 z-order は含まない → `window_coordinator.rs` の `//!`
- M13 main のサイズは 2 か所で同じ高さ → `window_coordinator.rs` の `//!`（「正本は CLAUDE.md」の指しを内容で置き換えた）
- M14 基準モニター判断 3 箇所はバー高で共通 → `window_coordinator.rs` の `//!`（3 箇所の一覧）・`layout::bar_rect_height_phys` doc。末尾（`read_frame_geom` / `logical_to_phys`・読み戻し経路の消滅）は `ADR-show-path-derives-bar-rect` と `show_egui_main` 本体コメント → 5
- M15 基準モニターは 2 対 1 → `read_bar_anchor` doc
- M16 `MonitorFromWindow` を使わない → `monitor::point_monitor_work_area` doc
- M17 → 4（索引として残す）

### 処置を返す純粋核の強制
- P1〜P5・P7 → 4（縮約して残す。関数名の列挙と probe の経緯・診断文の引用は削った）
- P6 「落とせなくなった」と書かない（メタ規範）→ 5。`let _ =` が通る事実は本節に残し、`UpdaterUi::try_begin_install` の残余は同関数 doc（`notify.rs`）が持つ。`clippy::let_underscore_must_use` を有効化しない理由は消える

### 外部からの wake
- W1 `wake_main` / `wake_results` を通す → 1（`main_waker` が private・E0616）。射程（handles 側は `pub(crate)`）は `EguiShellState::main_waker` フィールド doc へ
- W2 managed state に `egui::Context` を置かない → `snotra-egui-runtime/CLAUDE.md`「不変条件」
- W3 自ウィンドウでは `request_repaint` → 5: 自明

### イベント駆動 wake / armed
- E1・E2・D1・D3 → 4（「イベント駆動 wake の不変条件」節に統合）
- D2 armed 期限は 4 つ → 5: 数え上げ

### テーマの読み
- V1・V2 → `visual.rs` の `//!`
- V3 色パーサは 1 本 → `visual.rs` の `//!`・`native_brush_color` doc（本書の `visual::native_brush_color` は偽だった。`config_watcher.rs` の同じ誤りも直した）
- V4 → `search_input_ui` doc
- V5 → 1（`src-tauri/clippy.toml` の `disallowed-methods`。`options_mut` 経路の残余も同ファイル）
- V6 → 1（`search_input_ui_applies_theme_values_to_child_ui_in_the_first_pass`）
- V7 → 4（独立した節として残す）
- V8 却下案の ADR 名 → 5: `search_input_ui` doc が ADR を名指す

### config の読み
- R1・R3 → 1（E0616 / E0451 / E0599）
- R2 純 CPU だけ → `AppState::read_config` doc（「射程は読みだけ・書き込みは `update_config` 1 本」を足した）
- R4 規範は機構より広い → 5（裁定）。迂回（`config_handle().read()`）は `read_config` doc に残る。**`/history` 枝（`recent_history`）と `record_folder_expansion` が engine 錠越しに待つ事実は文書から消える**
- R5 前史の ADR 3 本（`ADR-config-read-exception-discriminator` / `ADR-config-read-without-exception` / `ADR-appstate-config-visibility`）→ 5: ADR 自身が経緯の正本。生きた層からの指しは無くなる

### trace
- T1 → 4（そのまま）

### 実装パターン
- I1 ホットキーの経路・I3 KISS・I4 3 重ロックの経緯・I6 managed state の型説明 → 5: 構造の説明・一般則・型定義が正本
- I2 設定画面の子プロセス → `SettingsProcessState` 型 doc・`launch_settings_process` doc
- I5 フラグは `try_begin` / `finish` 経由 → 1（本 PR で `indexing` / `index_build_started` を private 化。読み口 `is_indexing` / `is_index_build_started`）
- I7 show の操作順序 → `show_egui_main` 本体コメント

### 共有 core 関数の返り値契約
- C1〜C3 → 新設の `commands/mod.rs` の `//!`

### Win32 メッセージ配送
- N1・N3 → `platform_default_wnd_proc` 本体コメント
- N2 → 4（独立した節として残す）
- N4 全経路を列挙 → `platform_default_wnd_proc` doc

### ウィンドウ生成
- G1・G3・G4 → 4（残す）。帰結 1（manage 順）は `main.rs` の setup のコメント
- G2 `CloseRequested` 不要 → 5: 自明

### working set
- S1・S2・S3 → `working_set.rs` の `//!`（「commit ではない・HANDLE は RAII」を足した。「任意スレッドから呼べる」は呼び出し点が 1 つゆえ 5）

### フォント登録
- F1 → 1（`font_definitions_*`）
- F2・F4 → `font_stack.rs` の `//!`
- F3 再発経路 → テストのメッセージと本体コメント

### Win32 / Tauri 注意事項
- X1 config を先に見る・X2 crates.io 実在・X3 `windows` crate のバージョン・X13 feature フラグ・X14 未提供 API・X15 `C:` と `C:\`・X16 `symlink_metadata`・X18 capabilities → 5: 一般知識かコンパイラ/実行時に即時に分かる
- X4 宣言的属性の全分岐 → `results_window.rs` が機序（`MARKER_DONT_FOCUS`）を持つ。**一般則「宣言的属性で挙動を代替する前に全分岐を確かめる」は消える**
- X5 show/hide/topmost の層 → `results_window.rs` の `//!`（main=tao / results=raw・「main の show だけ raw」禁止を足した）
- X6 → `results_window.rs`（フラグ差分の判定）
- X7・X10 → `layout::present_results` doc・`results_window.rs` の `//!`
- X8 → 1（`EventLoopProof`・E0277）。「新しい可視性操作にも証人を要求する」は `window_coordinator.rs` の `//!` へ
- X9 5 関数の閉包は表現不能化ではない → `hide_egui_main` doc
- X11 race を lock で囲まない → `ResultsWindow::raw_show` doc（指しを外した）
- X12 presence 検査では捕まらない → `SnotraTraceInvariants.psm1` 先頭コメント
- X17 2 回呼び出しの clamp → `expand_env` のコメント
- X19 `ShellExecuteW` は COM STA → `launch_item_core` のコメント
- X20 `SendMessageTimeoutW(WM_NULL)` は他スレッド所有の宛先だけ → `window_coordinator.rs`・`platform/mod.rs` の撤去跡コメント
- X21 `SnotraPlatformWindow` には有効 → `platform/mod.rs` の生成箇所

## 直さなかった参照

- `docs/adr/`・`docs/superpowers/`・`docs/design/` が消えた見出しを指していても直さない（凍結された歴史・`ADR-adr-frozen-history`）

## 付け替えた参照

- 消えた見出しを指していた `.rs`・SPEC・PERFORMANCE・architecture・rules・code-reviewer・psm1 の指しを移設先へ。`main.rs` の `(src-tauri/CLAUDE.md)` 2 本は本書に該当記述が無かったので `docs/architecture.md`「起動シーケンスと初期化順序」へ
- G-heading-refs が bullet のリード文を指す参照も照合することを、消えた見出しの参照を 2 本注入して確かめた（2 件とも赤）
