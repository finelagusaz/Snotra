# 調査: #751 — 色だけの config 変更で入力欄の 3 値が同じフレームに届かない

## issue の要約

`[visual]` の**色だけ**を変更した config 適用フレームで、`ctx.set_visuals` 経由の 3 値
（TextEdit 背景 `extreme_bg_color` / 選択色 `selection.bg_fill` / hint 色 `weak_text_color`）が
**そのフレームの描画に反映されない**。

- 起票時（2026-08-03 以前）は「機構追跡済み・症状未観測」だった
- **2026-08-03 のコメントで症状が観測された**（`npm run smoke:manual` 項目 5 の実施中）:
  「設定ウィンドウを**開いたまま**入力欄の背景色を変更して保存 → main の入力欄背景は旧色のまま →
  設定ウィンドウを閉じると反映される」
- `background_color` は影響を受けない（実測済み・`set_clear_color` は style を経由しないため）
- #881 とは無関係（2026-07-30 の release ビルドでも同じ症状を再現済み）

## 機構（egui 0.35.0 の一次ソースで再確認・本調査で実測）

読んだのは `~/.cargo/registry/src/index.crates.io-*/egui-0.35.0/`（`Cargo.lock` は `egui 0.35.0` を pin）。

1. `Context::run_ui` → `run_ui_dyn`（`src/context.rs:780-807`）が **user callback より前に**
   `Ui::new(...)` で root Ui を作る
2. `Ui::new`（`src/ui.rs:108-136`）は `let style = style.unwrap_or_else(|| ctx.global_style());` で
   その時点の global style を `Arc<Style>` として掴む
3. `Context::set_visuals`（`src/context.rs:2212-2214`）は `style_mut_of` で **Context 側の
   `Options::dark_style` を差し替えるだけ**。既に作られた root Ui の `Arc<Style>` は差し替わらない
4. TextEdit が 3 値を読むのは**すべて `ui` 側**である:
   - 背景: `builder.rs:697` `ui.visuals().text_edit_bg_color()` → `style.rs:1147`
     `text_edit_bg_color.unwrap_or(extreme_bg_color)`
   - hint: `builder.rs:591` `ui.style().visuals.weak_text_color()`
   - 選択帯: `text_selection/visuals.rs:39` `visuals.selection.bg_fill`（`ui.visuals()` から渡る）
5. `Ui::visuals_mut`（`src/ui.rs:433`）は `Arc::make_mut(&mut self.style)` の copy-on-write で
   **その Ui と、以後に作られる子 Ui**（`new_child` は `Arc::clone(&self.style)`・`src/ui.rs:236`）に効く

**ゆえに `ui.visuals_mut()` へ移せば同じ pass に届く。**

## 関連ファイル・シンボル

| ファイル | 位置 | 役割 |
|---|---|---|
| `src-tauri/src/egui_shell/view.rs` | `SearchWindowView::update` の `:353-361` | 現行の適用点（`ctx.style_of(ctx.theme()).visuals.clone()` → 3 値代入 → `ctx.set_visuals`） |
| 同 | `:9-22`（module doc） | 「反映境界は 5 つ」「`ui.visuals_mut()` は全域 grep で 0 件（2026-07-28 実測）」「style 経由 3 値の #751 制約」 |
| 同 | `:340-342` | `set_clear_color` が #751 と無縁である旨 |
| 同 | `:345-352` | 「本段では直さない」（#666 段 3 で意図的に先送りした記述） |
| 同 | `:520-589` | `egui::Frame::new().show(ui, ..)` → `ui.add_sized(TextEdit::singleline(..))`。**適用点より後**に子 Ui が作られる |
| 同 | `:584-585` | 「hint の色は `set_visuals` の `weak_text_color` が正本」 |
| 同 | `:888-947` | 既存の `#[cfg(test)] mod tests`。**`ctx.run_ui` を使う headless テストの先例が既にある**（`restored_search_inserts_next_input_at_query_end`） |
| `src-tauri/src/egui_shell/visual.rs` | `VisualSnapshot` / `visual_snapshot` | 値の導出（純関数）。**本 issue では変えない**——壊れているのは適用であって導出ではない |
| `src-tauri/src/egui_shell/mod.rs` | `register_config_wake_listeners`（`:413-424`） | `config-applied` / `indexing-*` を受けて `wake_main` を無条件で撃つ |
| `src-tauri/src/config_watcher.rs` | `:153` | `update_config` **後**に `CONFIG_APPLIED` を emit |
| `snotra-egui-runtime/src/runtime.rs` | `render()`（`:393-422`） | `!visible` なら早期 return → `run_ui(raw_input, |ui| view.update(ui, &mut frame))` |
| 同 | `:433-445` | `SNOTRA_EGUI_REPAINT_TRACE` の計器（`window=` / `focused=` / `since_prev_ms=` / `causes=`） |

## 再利用できる既存パターン

- **headless egui テスト**: `view.rs` の既存テストが `egui::Context::default()` + `ctx.run_ui(...)` で
  TextEdit を実際に走らせている。同じ形で「同一 pass の子 Ui が新しい visuals を読むか」を測れる
- **カテゴリ D の自動判定**: `scripts/visual-check-colors.ps1`（`npm run check:colors`）が
  `SNOTRA_CONFIG_DIR` で使い捨てプロファイルを作り、窓矩形をキャプチャして exit code で判定する。
  ただし**測っているのは main / results の定常背景だけ**で、入力欄の矩形は測っていない
- **打鍵注入**: `scripts/lib/SnotraSmoke.psm1` の `Send-SnotraKeyChord` / `Wait-SnotraTraceEvent` /
  `Get-SnotraWindowCapture`

## 技術的制約

- **`ctx.set_visuals` の削除は安全である**（本調査で grep 実測・2 段で確かめた）。
  1. **消費者側（このリポジトリ）**: global style から新しい root Ui を作る egui コンテナ
     （`egui::Area` / `Window` / `CentralPanel` / `Modal` / `ComboBox` / `popup_below_widget` /
     `on_hover_text` / `on_hover_ui` / `show_tooltip` / menu 系）は `src-tauri/src/` に **1 件も無い**。
     `ScrollArea`（`results_view.rs:538`）は親 `ui` から子を作るうえ、results 窓は**別 Context** で
     `set_visuals` を一度も呼ばない
  2. **消費者側（egui 内部）**: `global_style()` / `options.style()` の全呼び出し点を egui 0.35.0 の
     `src/` 全域で列挙した（21 件）。うち**この経路で走りうるもの**が読むのは
     `interaction.interact_radius`（`context.rs:473`）・`visuals.text_options`（`:568`）・
     `visuals.error_fg_color`（`:1118` / `painter.rs:284`）・`animation_time`（`:3090` / `:3106`）・
     `scroll_animation`（`response.rs:823`）・`TextStyle::Body`（`:1640`）・`visuals.dark_mode`
     （`pass_state.rs:98`）と `debug.*` の各フラグだけである。**変更する 3 値
     （`extreme_bg_color` / `selection.bg_fill` / `weak_text_color`）を読むものは 1 件も無い**。
     残りは `containers/{area,window,tooltip,resize}.rs` と `color_picker.rs`＝上の 1 で 0 件を確認済み
  - **副産物の注意**: `visuals.text_options`（フォントのテッセレーション設定）だけは
    **global style からしか読まれない**。将来そこを config から変える必要が出たら、
    `ui.visuals_mut()` では届かない
- **新しい順序の不変条件が生まれる**: 現行の `ctx.set_visuals` は「どこで呼んでも当該 pass には
  届かない」ので**位置に意味が無い**。`ui.visuals_mut()` にすると「**最初のウィジェット／子 Ui の
  構築より前**」が correctness の条件になる。コンパイラ・ユニットテスト・`check:colors`・smoke の
  どれもこれを検知しない（受容残余）
- **`update()` の冒頭には既に `ui.interact`（`:298`）がある**が、これは背景ドラッグの
  ヒットテスト登録で描画しない。子 Ui も作らない。ゆえに適用点は現在の `:353` のままでよい
- **費用は増えない**: 現行は `Visuals` の clone + `style_mut_of` の `Arc::make_mut`。
  変更後は `Ui::style` の `Arc::make_mut` 1 回だけ
- **症状の検出器は目視／窓矩形キャプチャだけ**。issue コメント曰く「設定ウィンドウが main と
  重なると `CopyFromScreen` は設定ウィンドウの画素を撮る」——重ならない配置なら測れる

## 未解決の疑問（plan.md の「未確定」へ引き継ぐ）

1. **`config-applied` の wake で main のフレームが実際に 1 枚走るか。**
   issue コメント自身が「その間にフレームが何枚走ったかは測っていない」と認めている。
   コードからは `register_config_wake_listeners` が無条件に `wake_main` を撃ち、main は可視なので
   `render()` の `!visible` 早期 return には掛からない、と読める。だが**観測ではない**。
   - **走る** → `ui.visuals_mut()` への置換だけで症状は消える
   - **走らない** → 置換は必要だが十分ではなく、issue 第 3 案（色変化フレームでの
     `ctx.request_repaint()`）も要る
   - 症状の報告からは推論できない——報告者は `input_background_color` **だけ**を変えているため、
     「入力欄だけ旧色」はフレーム 0 枚とも整合してしまう
2. `panel_fill` / `window_fill` の「消費者ゼロの死んだ書き込み」（issue の「副次的な発見」）は
   **既に決着している**: `view.rs:351-352` が #673 spec 決定 2 で撤去済みと明記し、
   現行コードに代入は無い（grep 実測: `panel_fill` はコメント 1 件のみ）
