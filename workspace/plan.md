# plan: #666 段 3 — `view.rs` を `LauncherController` / `view` / `font_stack` へ分ける

**設計 SSOT**: `docs/superpowers/specs/2026-07-27-666-launcher-controller-main-view-design.md`（規則 R(段 3)・分類結果・却下案・34 段の照合表）。本計画はそれを実行手順へ落とす。
**調査**: `workspace/research.md`（全項目の実測 inventory・7 確定事実・外部参照の一覧）。

**挙動を変えない。** 動かすのは所有であって `update()` の副作用の順序ではない。

## 変更ファイル一覧

| ファイル | 種別 | 何を変えるか |
|---|---|---|
| `src-tauri/src/egui_shell/font_stack.rs` | 新規 | フォント解決・登録（関数 5・静的/型/定数 4）+ テスト 7。**`font.rs` という名前は使わない**——`snotra-settings/src/font.rs` が既に在り（`git ls-files` 実測）、`governance:check` の G1 は **basename 包含方式で wrong-directory 検出を意図的に放棄している**（`scripts/governance-check.mjs:77-78`）。同 basename を 2 crate に置くと、この変更に対する**唯一の機構的ゲートが両 crate で盲になる** |
| `src-tauri/src/egui_shell/launcher_controller.rs` | 新規 | `LauncherController`（フィールド 15・メソッド 23・型 5） |
| `src-tauri/src/egui_shell/view.rs` | 大幅縮小 | `SearchWindowView`（フィールド 5・メソッド 1・自由関数 1・`EguiView` impl 2）。`//!` を更新 |
| `src-tauri/src/egui_shell/mod.rs` | 修正 | `mod font_stack;` / `mod launcher_controller;` 追加。re-export コメントの消費者名を更新 |
| `src-tauri/src/egui_shell/results_view.rs` | 修正 | `view::configure_japanese_font` → `font_stack::configure_japanese_font`（2 箇所）・`//!` の `SearchWindowView` 言及 |
| `src-tauri/src/egui_shell/window_coordinator.rs` | doc のみ | 「期限の所有者（`SearchWindowView`）」→ `LauncherController`（L9）・L405 の呼び出し元表記 |
| `src-tauri/src/egui_shell/results_window.rs` | doc のみ | `SearchWindowView` 言及 2 箇所（L7・L54） |
| `src-tauri/src/egui_shell/visual.rs` | doc のみ | `configure_japanese_font` の指し先（L86） |
| `src-tauri/src/egui_shell/search_state.rs` / `notify.rs` / `layout.rs` / `icon_textures.rs` | doc のみ | **概念ラベル**（「driver（view.rs）」等）の指し先。母集団は下記 Phase 4 の grep 全件 |
| `src-tauri/src/commands/launch.rs` | doc のみ | `egui_shell::view::SearchWindowView::activate` / `execute_tool_selected`（L50） |
| `src-tauri/src/commands/instant.rs` | doc のみ | `egui_shell::view::execute_instant_selected`（L19） |
| `src-tauri/CLAUDE.md` | 修正 | 「モジュール構成」の `egui_shell/` ファイル一覧に 2 件追加 + 責務の一行。**`commands/` 節 L32 の「`egui_shell/view.rs` から再利用」**（`launch_item_core` / `launch_with_tool_core` の呼び出し元は `launcher_controller.rs` へ移る）。「フォント登録」節 L89 の指し先 |
| `PERFORMANCE.md` | 修正 | **L157-158・L179 が `egui_shell/view.rs` を `font_covers_cjk` / `configure_japanese_font` / `font_definitions` の所在として名指ししている**（実測）。Phase 1 で全て `font_stack.rs` へ移るため必ず腐る。`governance:check` は検知しない（正規形の参照を使っていないため） |
| `docs/architecture.md` | 修正 | `egui_shell/view.rs` の 4 箇所（L80・L125・L147・L156）を実態へ。**L156 は mermaid の fenced code 内の `participant View as egui_shell/view.rs (main)`** で CI の参照実在検査が届かない。L174 の「#646 PR2 で view.rs から移管」は**当時の事実ゆえ触らない**（歴史記述） |
| `.claude/skills/state-check/SKILL.md` | 修正 | 「主な置き場所」の `view.rs` 行を `view.rs`（キー入力の読みと描画）+ `launcher_controller.rs`（状態遷移の driver）へ。**判定ロジック・チェック項目は無変更**。`CLAUDE.md` 最重要ルール 2（エージェント設定の変更は合意してから）に当たるため差分を提示して確認し、**2026-07-27 に本 PR で更新する合意を得た** |
| `docs/superpowers/specs/2026-07-27-666-...-design.md` | 新規（作成済み） | 設計書 |
| `workspace/research.md` / `workspace/plan.md` | 新規（作成済み） | 調査・計画 |

**触らない**: `snotra-core/**`・`snotra-egui-runtime/**`・`snotra-settings/**`・`SPEC.md`（§SPEC.md 更新要否）・`search_state.rs` / `layout.rs` / `lifecycle.rs` / `notify.rs` / `visual.rs` の**実装**（純粋核は段 1・段 2 で確定済み）。

## 実装順序（フェーズ）

**各フェーズは単独で `cargo clippy --workspace --all-targets -- -D warnings` が緑になること。** 新 API の定義と呼び出し点の移行を別フェーズに割ると、未使用の新 API が `dead_code` で落ちる（`AGENTS.md`「関数・型を新規定義／改名／導入」）。ゆえに「型を作る」フェーズと「使う」フェーズを分けない。

### Phase 0 — baseline を取る（着手前・1 コマンド）

- [x] `cargo test -p snotra -- --list > <scratch>/tests-baseline.txt` を実行し、テスト名の一覧を退避する（Phase 1・Phase 5 で突き合わせる。**挙動不変の refactor では Red が取れないため、これが「テストが消えていない」唯一の機械的証拠である**）

### Phase 1 — `font_stack.rs` を切り出す

- [x] `src-tauri/src/egui_shell/font_stack.rs` を新規作成し、`view.rs` L27–202 の 9 項目（`JP_FONT_BYTES` / `CJK_PROBE` / `font_covers_cjk` / `font_definitions` / `ResolvedFont` / `USER_FONTS` / `resolve_font_family` / `jp_font_bytes` / `configure_japanese_font`）を**中身を変えずに**移す
- [x] `view.rs` L1765–1868 のテスト 7 件（`font_definitions_*` 4 + `font_covers_cjk_*` 3）を `font_stack.rs` の `mod tests` へ移す。**アサーション文言も含めて 1 字も変えない**（移設の正しさをテスト名の一致で示すため）
- [x] `font_stack.rs` に `//!` を書く: 責務（フォント解決と `set_fonts` 登録）・**`OnceLock` は set-once / never-clear**（`transmute` の健全性がそれに依存）・**消費者は `view.rs` と `results_view.rs` の両方**（だから独立モジュールである）
- [x] `mod.rs` に `mod font_stack;` を追加する。**既存の `mod` 宣言（`mod.rs:8-25`）はアルファベット順ではない**（`icon_textures` / `lifecycle` / `search_state` / `layout` / `notify` / `strings` / `results_view` / `results_window` / `view` / `visual` / `window_coordinator`・実測）。並べ替えず末尾寄りへ足す
- [x] `view.rs` の **2 箇所**の `configure_japanese_font` **呼び出し**（`setup`・L1025 / `update` の `font_family_changed` 分岐・L1157）を `super::font_stack::configure_japanese_font` 経由へ差し替える（L187 は**定義**であり呼び出しではない——移設対象そのもの）
- [x] `results_view.rs` L442・L478 の `crate::egui_shell::view::configure_japanese_font` を `crate::egui_shell::font_stack::configure_japanese_font` へ差し替える
- [x] `visual.rs` L86 の doc 内の `configure_japanese_font` 参照が指し先として正しいままか確認し、必要なら `font_stack::` を明示する
- [x] **[dry-check]** `font_stack.rs` に `pub(super) fn font_family_from_config(app: &tauri::AppHandle) -> String` を置き、`view.rs:1020-1024` と `results_view.rs:437-441` の**完全に同一な** 4 行（`try_state::<AppState>` → `engine.lock()` → `config().visual.font_family.clone()` → `unwrap_or_else("Segoe UI")`。フォールバック文字列まで一致することを実測済み）を両方これへ置き換える。**`font_stack.rs` を新設する本段が、この重複を寄せる唯一の自然な機会である**（`view.rs` に置くと `results_view` が main の view に依存し続ける）
- [x] **フォールバック文字列を `const DEFAULT_FONT_FAMILY: &str = "Segoe UI";` として `font_stack.rs` に 1 度だけ書く。** `font_family_from_config` は Phase 1 唯一の**移設ではない新規コード**であり、`cargo test -- --list` の突き合わせも `AppHandle` 依存ゆえのユニットテスト不能も、この 1 行のドリフトを検出できない。**文字列を打ち間違えるとフォント解決が黙って jp_font 単一へ退化する**（失敗もクラッシュもしない）。置換した 2 箇所が同じリテラルだったことは実測済みで、その証跡を Phase 1 のコミットメッセージへ残す
- [x] `view.rs` の `//!` からフォント登録 3 枝の記述を `font_stack.rs` へ移す（`view.rs` 側は残さない——**同じ事実の写しを 2 か所に置かない**）
- [x] 検証: `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test -p snotra` / `cargo doc --workspace --no-deps --document-private-items`
- [x] 検証: `cargo test -p snotra -- --list` の出力を Phase 0 の baseline と突き合わせ、**テスト名の集合が完全一致**することを確認する（消失・改名が無い証拠）
- [x] Phase 1 をコミットする（`refactor: フォント解決・登録を egui_shell/font_stack.rs へ切り出す (#666)`）

### Phase 2 — `launcher_controller.rs` を切り出す（最大・分割不能）

- [x] `src-tauri/src/egui_shell/launcher_controller.rs` を新規作成し、`pub(super) struct LauncherController` に**フィールド 15**（`app_handle` / `was_focused` / `unfocus_at` / `state` / `search_debounce` / `last_input_at` / `folder_tx` / `folder_rx` / `folder_cache` / `folder_error` / `instant_rows_query` / `launching` / `last_seen_index_generation` / `notice` / `notice_base`）を移す。**フィールド doc は 1 字も落とさず運ぶ**
- [x] 型 5（`FolderMsg` / `LaunchWork` / `LaunchTag` / `LaunchInFlight` / `ToastAction`）を `launcher_controller.rs` へ移す
- [x] メソッド 23 を移す（`emit_hide` `activate` `start_launch` `finish_launch` `drain_launch` `clear_search` `execute_slash` `execute_instant_selected` `activate_or_execute` `shift_activate` `execute_tool_selected` `record_folder_expansion` `resolve_tools` `auto_hide_enabled` `settings_running` `instant_prefix` `indexing` `lang` `spawn_folder_load` `run_search` `run_search_with` `handle_toast_action` `spawn_install`）。**本文を書き換えない**——`self.` の指す先が変わるだけである
- [x] **[race-check R4・借用]** `update()` / `setup()` の冒頭で `let app = self.controller.app().clone();` を **1 回だけ**取り、以降の `try_state::<...>()` はすべてこのローカル変数から引く。**`self.controller.app()` の戻り値を保持したまま `&mut self.controller` のメソッドを呼ぶ形を作らない**——`tauri::State<'_, T>` は借用元に紐付くため E0502 になる。**リポジトリに先例がある**（`results_view.rs:451` が同じ理由で `let app_handle = self.app_handle.clone();` を置き、コメントで経緯を明記している）
- [x] **[race-check R1]** `consume_reset_pending(&mut self) -> bool` は **`bool` を返す**。現行の `view.rs:1062-1097` は `reset_pending.swap(false)` の `if` ブロック**の中**で `ResultsWindow::reset_size_guard()` を呼んでおり、guard の呼び出しは規則 R により view 側に残るため、view が「今フレームが reset フレームか」を知る手段が返り値以外に無い。返り値を落とすと**同一フレームでの reset が黙って片肺になる**
- [x] `view` へ公開する読み口を `pub(super)` で定義する: `app(&self) -> &tauri::AppHandle` / `state(&self) -> &SearchState`（**`&` 1 本で読みを全て通し、mutator は `&mut self` ゆえ view から届かない**・設計 §3.7）/ `notice_message(&self) -> Option<&str>` / `is_launching(&self) -> bool` / `is_search_armed(&self) -> bool` / `instant_rows_query(&self) -> Option<&str>` / `lang` `indexing`（既存メソッドを `pub(super)` へ昇格）。
      **`instant_prefix` は `pub(super)` にしない**——呼び出し 3 件はすべて controller 内部である（`view.rs:814` = `run_search` / `:1448` = 段 22 / `:1641` = 段 28。後 2 者は `on_input_changed` / `on_enter` として controller へ移る・grep 実測）。view 側の呼び出し元が無い `pub(super)` は `-D warnings` 下で `dead_code` に落ちる
- [x] **各アクセサに view 側の呼び出し元を 1 つ名指しできることを、昇格前に確認する**（上と同じ罠。実測での対応: `lang`→段 21/24/25、`indexing`→段 24/29、`notice_message`→段 24、`is_launching`→段 21/24、`is_search_armed`→段 30、`instant_rows_query`→段 29、`state`/`app`→全域）
- [x] `update()` の各段から controller へ落ちる遷移を `pub(super)` メソッドとして切る。**連続する文の塊にだけ名前を付け、塊の並べ替え・分割位置の変更をしない**（設計 §2）:
      `consume_reset_pending`（段 3）/ `consume_external_pending`（段 5–6）/ `poll_async`（段 10–12）/ `on_escape_pressed` `on_focus_changed`（段 14–17）/ `on_nav_keys`（段 18–20）/ `on_input_changed`（段 22）/ `poll_search_debounce`（段 27）/ `on_enter`（段 28）/ `set_focused`（段 34）
- [x] `view.rs` の `SearchWindowView` を `{ controller: LauncherController, applied_font_family, applied_background_hex, last_set_width, last_set_height }` の 5 フィールドへ縮小し、`window_width` メソッド 1 本と `draw_toast_button` を残す
- [x] **借用の早期検証**: `LauncherController` の骨格（フィールド + `app()` / `state()` の 2 アクセサ）と `SearchWindowView` の新しい形が置けた**時点で**`cargo check -p snotra` を 1 回走らせる。**フェーズ末まで待たない**——借用エラーは 23 メソッドを移し終えてからだと原因の切り分けが高くつく。手作業の NLL 解析では snapshot publish / `take_clicked_for` / `shift_activate` 周辺に違反を見つけられなかったが、**コンパイルするまで確認したことにならない**
- [x] `SearchWindowView::new(app_handle)` は `LauncherController::new(app_handle)` を包む形にする。**初期値を 1 つも変えない**（`last_set_height: 52.0` / `Debouncer::new(50ms, true)` / `last_set_width: 0.0` 等）
- [x] `mod.rs` に `mod launcher_controller;` を追加し、re-export のコメントの消費者名を実態へ更新する。**範囲は L13-19・L27-32・L38-43・L45-68 の全域**（「view.rs（driver）が消費する」型の記述は L13・L15・L17-19・L27・L52・L57・L62・L64・L67 に散在する・実測。一部だけ直すと残りが古いまま残る）
- [x] `launcher_controller.rs` の `//!` に**不在の明記**を書く（設計 §4 の 5 項目——フレームを所有しない / `take_clicked_for` はここに無い / `reset()` の世代 bump が末尾と結ばれる / `drain_launch` は自前の repaint を持たない / folder drain の前後関係）
- [x] 検証: `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test -p snotra` / `cargo doc --workspace --no-deps --document-private-items`
- [x] 検証: 設計 §5 の 34 段照合表を開き、`git diff` 上で `update()` の文の並べ替えが **0 件**であることを 1 段ずつ確認する。**Phase 2 は所有を移すだけで位置を動かさないため、判定は二値である**（1 件でも動いていたら誤り）
- [x] Phase 2 をコミットする（`refactor: 検索セッションの状態と遷移を LauncherController へ集約する (#666)`）

### Phase 3 — `view.rs` の入力変換を 2 段にし、`//!` と誤コメントを直す

- [x] `view.rs` に私的な入力読み 2 本を作る: `read_pre_widget_input(ctx) -> PreWidgetInput`（`focused` / `escape` / `nav_down` / `nav_up`（**`events.retain` による消費もここ**）/ `right` / `left`）と `read_post_widget_input(ctx) -> PostWidgetInput`（`enter` / `shift`）
- [x] 両関数の doc に **1 段にできない理由**を書く（Escape/↑↓/→← は TextEdit の前で**消費**が要る〔#700〕・Enter は `response.changed()` に依存するため後〔codex 発見 4〕）
- [x] `read_pre_widget_input` の doc に「**この関数より後で `key_pressed(ArrowUp/ArrowDown)` を読んでも常に `false` である**」を書く。`InputState::key_pressed()` は `self.events` を走査するため（`egui-0.35.0/src/input_state/mod.rs:743,750-760`）、`retain` 後の読みは沈黙して `false` を返す。**将来 ↑↓ を読む文を段 14〜20 に足した編集者が落ちる罠であり、構造では塞げない**
- [x] 読みを前へ寄せる安全性を doc に明記する: egui の入力はフレーム内で不変・消費（`events.retain`）は TextEdit より前という制約だけを持ち、寄せた先と現行位置の間の文（段 14–17）は ↑↓ イベントを読まない
- [x] **処置を 1 つも動かさない**（`move_selection` / folder 展開 / blur 判定は現行の段に残す）
- [x] `view.rs` L1139–1140 の誤ったコメント（「`set_visuals` はウィジェット描画より前である必要がある」）を確定事実 5 の内容へ差し替える——**egui 0.35.0 では `ctx.set_visuals` は現在の pass の `Ui` に届かない**（root `Ui` が pass 冒頭で `ctx.global_style()` を `Arc` snapshot する）・潜在バグは #751・**本段では直さない**
- [x] `view.rs` の `//!` を書き直す: 責務（main 窓の 1 フレーム——入力の読みと描画・OS 窓への適用）・**反映境界は 4 つあり 1 つの名前に畳んでいない**・**入力変換は pre/post の 2 段である**・検索セッションの状態は `launcher_controller` にある
- [x] 段 31 の `take_clicked_for` 周りに、**snapshot publish の後という位置が不変条件である**旨の既存コメント（#699）が残っていることを確認する（移設で落ちていないか）
- [x] 検証: `git diff` 上で動いた文が**「重複した読みを束ねる判定」節に列挙した 4 件の読みだけ**であることを確認する（`nav_down`/`nav_up`〔消費込み〕・`ArrowRight`・`ArrowLeft` の 4 行が段 18/19/20 → 段 13 へ寄る）。**5 件目が動いていたら欠陥である。** Phase 2 の「並べ替え 0 件」と役割が違う——**位置を動かすのは本フェーズだけであり、ここが順序回帰の主戦場である**
- [x] 検証: `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test -p snotra` / `cargo doc --workspace --no-deps --document-private-items`
- [x] Phase 3 をコミットする（`refactor: main 窓の入力変換を pre/post の 2 段へ分け、view.rs の //! を実態へ揃える (#666)`）

### Phase 4 — 文書・doc コメントの同期

- [x] `src-tauri/CLAUDE.md`「モジュール構成」の `egui_shell/` ファイル一覧へ `font_stack.rs` / `launcher_controller.rs` を追加し、`view.rs` の責務散文を実態へ直す（**責務の正本は各ファイルの `//!`**・#562。ここはファイル名の索引を保つ）
- [x] `src-tauri/CLAUDE.md`「フォント登録」節の「規則の正本はテストと `view.rs` の `//!`」を `font_stack.rs` へ差し替える
- [x] `docs/architecture.md` の 4 箇所を**一括で直さず 1 件ずつ裁定する**（`.rs` の 47 件と同じ規律）。実測での見込み: **L80**（「検索バーは `main`…（`egui_shell/view.rs` / `results_view.rs`）に分離して描画」）と **L147**（「テーマ・行視覚は…毎フレーム live-read で描画（`egui_shell/view.rs`）」）は**描画の所在ゆえ真のまま据え置き**、**L125**（「view.rs が直呼び実行」）は `execute_slash` / `execute_instant_selected` が controller へ移るため**要修正**、L156 は下記
- [x] `PERFORMANCE.md` L157-158・L179 の `egui_shell/view.rs` を `egui_shell/font_stack.rs` へ直す
- [x] `src-tauri/CLAUDE.md` L32（`commands/` 節の「`egui_shell/view.rs` から再利用」）を `launcher_controller.rs` へ直す
- [x] `src-tauri/src/commands/launch.rs` L50 / `commands/instant.rs` L19 / `egui_shell/window_coordinator.rs` L9・L405 / `egui_shell/results_window.rs` L7・L54 / `egui_shell/results_view.rs` L1 の doc コメント参照を更新する
- [x] **所在を語る散文の数え上げ（`docs/development-principles.md:42` が命じる手順）**: `grep -rn "view\.rs" --include=*.rs src-tauri/src`（**47 件**・`view.rs` 自身を除く。うち `results_view.rs` を指すものを除くと **37 件**）と `grep -rn "driver" --include=*.rs src-tauri/src`（**45 件**）を母集団とし、1 件ずつ「指し先が `launcher_controller.rs` / `font_stack.rs` / `view.rs` のどれになったか」を判定する。**この 2 つの grep が doc 更新対象の SSOT であり、上の個別項目はその部分集合である**（個別に挙げた行だけを直して sweep を省くと、`notify.rs:3`「時刻は driver（view.rs）が注入する」のような**個別列挙に無い件が落ちる**）。**シンボル名の grep では届かない「概念ラベル」（`driver（view.rs）`・「main 側」・「view.rs の drive」）が主戦場である**——#749 は関数名で grep して 4 箇所を直したが実際は 6 箇所あった
- [x] **一括置換を使ってはならない**（`sed` / `Edit` の `replace_all`）。数え上げた各件は 3 つの理由で個別判定が要る:
      - **`snotra-egui-runtime/src/repaint.rs:301,307` の `"view.rs"` はテストの fixture リテラルである**（`file: "view.rs"` / `assert_eq!(..., "… ; view.rs:439 state changed")`・実測）。一括置換すると**assert は緑のまま fixture だけが壊れる**
      - `mod.rs` の re-export コメントのうち **5 件（`ViewKind` / `strings` / `OverlayKind` / `plain_results_hidden` / `ToastKind`・`UpdaterPhase`）は消費者が新 2 モジュールへ割れる**。「view.rs」を 1 語で置換すると 5 件とも誤りになる
      - 後述の「既存の腐り」は**直さない**判定である
- [x] **`snotra-core/src/engine.rs:158` と `folder.rs:140` の「driver がキャッシュし」は据え置く**（検算済み）——ファイル名を含まない**役割名**であり、driver が `LauncherController` になっても記述は真のまま。`engine.rs:32` の「src-tauri driver」も同様。**この 2 crate をまたぐ参照は `view.rs` / `SearchWindowView` / `src-tauri/` のどの grep にも掛からない**ため、据え置きの判定をここに記録しておく（次に読む人が「見落とし」と読まないため）
- [x] 上の数え上げで見つかる**既存の腐り 3 件**は**本段のスコープ外**とし、直さずに PR 本文へ「発見したが別件」として残す。ついでに直すと差分の意味が「移設」から外れる:
      - `icon_textures.rs:3` ほか計 4 箇所 + `mod.rs:52` の「icon texture の driver は view.rs」——#646 PR2 で `results_view.rs` へ移管済み（実測）
      - `layout.rs:7` の「view.rs `RowTheme::path_size`」——`RowTheme` は #673 で `visual.rs` へ移設済み（実測）
- [x] `docs/architecture.md` L156 の mermaid `participant View as egui_shell/view.rs (main)` を実態へ（**fenced code 内ゆえ CI の参照実在検査が届かない**）
- [x] `.claude/skills/state-check/SKILL.md` L40 の「主な置き場所」を更新する（`view.rs` = 入力の読みと描画 / `launcher_controller.rs` = 状態遷移の driver）。**判定ロジック・チェック項目は 1 文字も変えない**
- [x] `/norm-review` を SKILL.md の変更に対して起動する（`.claude/rules/safety-nets.md` の要求）。**停止条件を先に書く**: 合格条件＝「読者が状態遷移の置き場所を取り違えない」/ 上限 1 巡（変更は事実の索引であって判定を足さないため）/ 残余は受容として明記 / 塞ぎ 1 件あたり 1 文まで
- [x] **ADR は本 PR では書かない。** 理由と回収先を PR 本文へ明記する: 段 1 の ADR-0008 は実装 PR（#759）ではなく**サイクル末の #762 で書かれた**先例があり、本段の否定の知識（設計書 §3 の却下 8 件）は**本サイクルの `/retrospective` で ADR へ回収する**。回収先を名指ししない deferred は脱落である（`/plan-review`「スコープ」）
- [x] 検証: `npm run governance:check`（`.md` の沈黙は合格ではない・カテゴリ F）
- [x] 検証: `cargo doc --workspace --no-deps --document-private-items`。**ただしこれは上の doc コメント参照 6 箇所を検知しない**——`launch.rs:50` / `instant.rs:19` はいずれも素のバッククォート code span（`` `egui_shell::view::SearchWindowView::activate` ``）であり、rustdoc の `broken_intra_doc_links` が見るのは `[...]` 記法だけである（実測）。**この 6 箇所は「機構が守らない残余」であり、Phase 4 のチェックボックスを 1 件ずつ目で潰すことが唯一の検知手段である**
- [x] Phase 4 をコミットする（`docs: view.rs 分割に伴うモジュール索引・参照・skill の指し先を同期する (#666)`）

### Phase 5 — 全体検証（PR 作成前）

- [x] カテゴリ A: `cargo check --workspace` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test -p snotra` / `cargo doc --workspace --no-deps --document-private-items`
- [ ] カテゴリ C: `npm test` / `npm run smoke:startup` / `npm run smoke:egui`（スラッシュコマンド経路 `execute_slash` を移設したため発火）
- [x] カテゴリ D: `cargo run -p snotra` で実機目視（項目は下の「破壊不変条件と検知手段」の 5 件 + `npm run smoke:manual` の既定項目）。**エージェントは実行できない**——人間が実施し `-PostToPr` か貼り付けで PR に残す
- [ ] カテゴリ D 追加: `$env:SNOTRA_EGUI_FAKE_UPDATE_FAILED = "1"; cargo run -p snotra`（**PowerShell 形式**——`docs/build-commands.md` の記載が正本。bash の `VAR=1 cmd` 形式は動かない）で toast の描画（ボタン + 末尾省略）と `[閉じる]` の動作を見る（`handle_toast_action` を controller へ移したため）
- [x] `cargo test -p snotra -- --list` を最終確認し、Phase 0 baseline とテスト名集合が一致することを再確認する
- [x] `scripts/smoke-egui.ps1` が前提とする trace イベント名（`egui_launch` / `egui_slash` / `egui_results:*` 等）と hotkey が 1 つも変わっていないことを grep で確認する（`AGENTS.md`「機能削除・IPC ルート変更」行。移設で `crate::trace_main` の第 1 引数を書き換えないこと）

## 不変条件

1. **`update()` の副作用文の実行順序は保存される。** 抽出は連続する塊への命名に限る。入力の**読み**だけは前へ寄るが、**処置は 1 つも動かない**（設計 §2.2）
2. **↑↓ の消費（`events.retain`）は TextEdit の構築より前にある。** 破れると #700 が再発する（focus を保持した TextEdit が同じ ↑↓ でキャレットを飛ばす）
3. **Enter / Shift+Enter の判定は TextEdit の `changed()` 処理より後にある。** 破れると同一フレームの IME 確定・paste が旧 state で起動される
4. **`take_clicked_for`（7 番目の破壊的 take）は snapshot publish の後にある**（#699）。照合に使う `rows_generation` は、そのフレームで行を差し替えうる全ハンドラより後の値でなければならない
5. **`reset_size_guard` の呼び出しは同一フレームの `drive_results_window` より前にある**（#749）。イベントループスレッド上という前提を移動で壊さない
6. **`drain_launch` は `reset_pending` 消費の後にある**（spec C 節 不変条件 2）。前だと show 直後フレームで stale `Ok` が再 show 窓を hide で撃つ
7. **通知（notice）の期限を張る唯一の主体は `notice.remaining()` ブロックである**（確定事実 4a）。`drain_launch` の **`notice.set` を撃つ 3 分岐**（timeout / Failed / Disconnected）は自前の `request_repaint` を持たない。**`drain_launch` 全体が repaint を持たないという意味ではない**——`view.rs:497`（Empty 腕の未 timeout 側）には `ctx.request_repaint_after(LAUNCH_TIMEOUT - elapsed)` があるが、これは**起動タイムアウトの期限**であって通知の期限ではない（別の armed 期限・#711 の 4 種のうち 2 つ）。両者を別モジュールへ分けず、`poll_async` の中で**同じフレームに呼ばれる**ことを崩さない
8. **`folder_gen` の bump（`reset()`）は `accept_folder_result` の stale 棄却より前にある**（確定事実 4b）
9. **フォント登録は index 0 への `insert` である**（`push` = 末尾ではない・#399/#579）。3 枝（user 単独 / user + jp fallback / jp 単独）の判定は `font_covers_cjk`
10. **`OnceLock`（`JP_FONT_BYTES`）は set-once・never-clear。** `transmute` による `'static` 化の健全性がこの不変性だけを根拠にしている。再 set・クリアの経路を足さない
11. **`read_visual` の戻り値を `self.` へ保持しない**（寿命は 1 フレーム）。`applied_font_family` / `applied_background_hex` は snapshot ではなく「適用済みマーカー」であり、これは保持してよい
12. **`LauncherController` は `view` に依存しない**（依存は一方向）。破れると規則 R(段 3) の規則 2 が成立しなくなる
13. **`app_handle` を**フィールドとして**2 本持たない**（controller が単独所有する）。view は `update()` / `setup()` の冒頭で `controller.app().clone()` を**ローカル変数へ 1 回**取って使う——借用を跨いで `&mut controller` を呼べないため（race-check R4・先例は `results_view.rs:451`）
14. **`LauncherController` は `egui::Context` をフィールドとして持たない。** ctx は毎回引数で受け取る（現行の全 15 箇所がこの形・grep 実測）。`egui_shell` のどの構造体も Context を保持しておらず、`mod.rs:102-104` が旧「`Mutex<Option<egui::Context>>` スロット」方式を廃した経緯を残している（Context の clone は repaint callback ごと複製し、`RepaintScheduler` の `Arc` が窓の `Destroyed` を越えて worker の停止・join を止める・#671 PR D）。**移設中に「ctx を毎回渡すのが面倒だから controller に持たせる」誘惑が生じるが、これは禁止である**
15. **updater toast の読み（段 25・view の描画）と書き（段 26・`handle_toast_action`）は同一フレーム・この順序である。** 分割で別モジュールへ割れるため、両側の doc にこの順序を明記する（`handle_toast_action` は `ctx` を受け取り、状態を変えたら `request_repaint()` する——欠くと旧 toast が次の無関係な入力まで残る・SU5 e746826 の同型バグ）
16. **異常時**: 本段は新しい状態フラグ・プロセス・ウィンドウ・チャネルを 1 つも導入しない。既存の失敗経路（worker panic → `Disconnected` で失敗扱い復帰・`launching = None` による Receiver drop・`try_state` 不在時の早期 return）はすべて移設先へそのまま運ぶ。**予期しない順序で呼ばれたときの振る舞いも現行と同一である**——`update()` の呼び出し元（`snotra-egui-runtime`）を触らないため

## 破壊不変条件と検知手段

**フレーム順序の不変条件に自動検出器は無い**（issue「検証の制約」）。trace の presence を見るスモークは同クラスの回帰を緑のまま通した実例がある（#671 PR A′）。ゆえに検知手段は以下に限られる。

**この表は `scripts/manual-smoke.ps1` の `$items` の SSOT である**（スクリプト冒頭が「項目の SSOT は PR 本文の目視表であり、`$items` はその写し」と定める）。**行を増減したらスクリプトと PR 本文の両方を直すこと。** 右端に対応する smoke 項目番号を持つ。

| # | 壊れたら即アウト | 検知手段 | smoke 項目 |
|---|---|---|---|
| 1 | ↑↓ の消費が TextEdit より後へ落ちる | **実機**: 検索 → ↑ で選択を動かす → 文字を打つ。文字が**キャレット位置（末尾）**に入ること。先頭に入ったら #700 の再発 | **11**（新設） |
| 2 | 結果行クリックが 1 フレーム古い行を起動する | **実機**: 打鍵直後に結果行をクリックして起動。正しい項目が開くこと（#699） | **3**（#749 で既存） |
| 3 | フォント登録が末尾 fallback へ戻る | **font テスト 7 件**（index 0 を固定）+ **実機**: Latin と CJK が混在する行のベースラインに段差が出ないこと（`raster.rs` は AA を持たないため段差が可視化する・#399/#579） | **12**（新設） |
| 4 | reset-on-show と results driver の順序が壊れる | **実機**: Alt+Q で hide → 再 show。前回の結果が一瞬出ない・results 窓のサイズが 1 フレーム古くないこと。加えて `npm run smoke:egui` | **10**（#749 で既存） |
| 5 | 起動 timeout / 通知の期限が張られなくなる | **実機**: 存在しないパスを起動して失敗通知を出し、**放置して数秒で自然に消える**こと（`notice.remaining()` が deadline を張っている証拠。消えなければ #7 の不変条件が破れている） | **13**（新設） |

## テスト方針

- **新規ユニットテストは追加しない。** 純粋核（`search_state` / `layout` / `lifecycle` / `notify` / `visual` / `icon_textures`）は段 1・段 2 までにテスト済みで、本段が触る `LauncherController` / `SearchWindowView` は `tauri::AppHandle` と `egui::Ui` に依存し**ユニットテスト前提にしない**層である（`.claude/rules/src-tauri.md`）。ここでテストを足せないことは見落としではなく層の性質である
- **既存テスト 7 件は 1 字も変えずに `font_stack.rs` へ移す。** 移設の正しさは「テスト名の集合が変わらないこと」で示す
- **挙動不変ゆえ Red は取れない。** 代わりに **`cargo test -p snotra -- --list` の baseline 突き合わせ**（Phase 0 で取得 → Phase 1・Phase 5 で照合）を、テストが消えていない証拠に使う
- 順序の回帰は上表の実機目視が唯一の検出器である（自動化しない——できないことを「した」形にしないため）

## 確定事実 7 件への対応（issue が「再調査不要」とした一覧＝壊れうる箇所の一覧）

| 事実 | 本計画のどこで守るか |
|---|---|
| 1. 入力変換は 2 段 | Phase 3 の `read_pre_widget_input` / `read_post_widget_input`。不変条件 2・3 |
| 2. 外部イベント消費は 7 箇所（`take_clicked_for` が例外） | 包括名を付けない——段 3 / 5–6 / 10–12 を**別々の名前**（`consume_reset_pending` / `consume_external_pending` / `poll_async`）で切り、7 番目は `view.rs` に残す。不変条件 4・設計 §3.6 |
| 3. 順序制約は `update()` の全長 | 不変条件 1・8。`launcher_controller.rs` の `//!` に明記（設計 §4） |
| 4. load-bearing な弱い順序 2 件 | 不変条件 7（deadline）・8（folder drain）。`//!` に明記 |
| 5. 反映境界 4 つ | Phase 3 で 1 つの名前に畳まず個別に残す + 誤コメントの差し替え。#751 は対象外（設計 §3.8） |
| 6. モードは導出のまま | 明示 enum フィールドも ADT 置換も行わない（設計 §3.5・スコープ外） |
| 7. 全域 `Effect` enum は作らない | 設計 §3.4。`EscapeOutcome` の既存パターンは維持する |

## 「重複した読みを束ねる」判定（`AGENTS.md` 条件別チェック表の該当行）

本段が束ねるのは **`ctx` からの入力の読み 6 件だけ**である（config live-read の重複は束ねない・設計 §6）。各件について「**後で**読まれる/立つことに依存していないか」を 1 行ずつ書き出す:

- `focused`（段 13）— 現行位置のまま。移動しない
- `escape`（段 13）— 現行位置のまま。移動しない
- `nav_down` / `nav_up`（段 18 → 13）— 読みと**消費**（`events.retain`）が同時に前へ寄る。間の段 14–17（focus / Escape ラダー / blur 猶予）は ↑↓ イベントを 1 度も読まない（実測）。消費の唯一の制約は「TextEdit より前」であり保たれる。**`retain` は後続の `key_pressed()` に効く**——`InputState::key_pressed()` は `num_presses()` 経由で `self.events` を走査する（`egui-0.35.0/src/input_state/mod.rs:743,750-760`・一次資料で確認）。「入力はフレーム内で不変だから読む順序は関係ない」は**偽**であり、根拠にしてはならない（設計 §2.2）
- `ArrowRight`（段 19 → 13）— 読みのみ前へ寄る。処置（folder 展開）は段 19 に残す。`ctx.input` は非破壊で、段 13–18 の誰も ArrowRight を消費しない
- `ArrowLeft`（段 20 → 13）— 同上
- `enter` / `shift`（段 28）— **前へ寄せない。** `response.changed()` の後でなければ同一フレームの IME 確定・paste を見落とす（確定事実 1）

**config live-read（`lang()` / `indexing()` / `instant_prefix()`）の重複した呼び出しは 1 件も束ねない**——確定事実 4 が示すとおり「弱く見える」読みが load-bearing でありうるため（設計 §6）。

## SPEC.md 更新要否

**不要。** 挙動を変えないため。加えて `SPEC.md` は `view.rs` も `SearchWindowView` も 1 度も名指ししていない（grep 実測。`egui_shell` の言及は L92〔`icon_textures.rs`〕と L420〔`create`〕の 2 箇所のみで、どちらも本段が触らない）。

## セルフレビュー

### Step 5a — 起動した check スキルと反映結果

`AGENTS.md`「条件別チェック」表を計画が記述する**設計上の操作**に当てた結果、`/plan-review`（常時）・`/race-check`・`/state-check`・`/dry-check` が発火した。`/cache-check` は非該当（`folder_cache` の**述語**を変えず所有だけを動かすため）、`/persistence-check` は非該当（永続形式・キー形式に触れないため）。`/symmetric-check` の観点（対称ペア・リソース生成/破棄）は `/plan-review` Step 2 が扱う（`/start-issue` 5b の免除）。

**`/race-check`（計画レビュー）** — 新設される並行境界は 0 件。全境界が「移設」であり、問われたのは「分割が境界とその相棒を引き離さないか」。

- **R1〔反映済み・Phase 2〕** `reset_pending` の消費（controller）と `ResultsWindow::reset_size_guard()`（view）が `view.rs:1062-1097` の同一 `if` ブロックから割れる → `consume_reset_pending(&mut self) -> bool` にした。返り値を落とすと同一フレームの reset が黙って片肺になる
- **R2〔反映済み・不変条件 14〕** `LauncherController` に `egui::Context` をフィールドとして持たせない（`egui_shell` の全 15 箇所が引数渡し・grep 実測。`mod.rs:102-104` が旧スロット方式を廃した経緯を持つ）
- **R4〔反映済み・Phase 2 + 不変条件 13〕** `self.controller.app()` の借用を跨いで `&mut self.controller` を呼ぶと E0502。`update()` 冒頭で `app` をローカルへ clone する（先例: `results_view.rs:451` が同じ理由で同じ形を採り、コメントに経緯を残している）
- **R3〔反映済み・不変条件 15〕** updater toast の読み（view）と書き（controller）が割れる → 同一フレーム・この順序であることを両側の doc に明記
- 残る境界（`spawn_folder_load` / `start_launch` / `spawn_install` / `poll_async` の 2 drain + 通知期限 / `emit` 2 本 / config live-read）は**安全**。特に確定事実 4a の「deadline を張る唯一の主体は `notice.remaining()`」は、`drain_launch` と同じ `poll_async` に同居するため分離されない

**`/state-check`** — モードもガード条件も新設・変更しないため直交性マトリクスの新規行は 0。リセット経路 7 件は全て `consume_reset_pending` の中に留まる（R1 の 1 件だけが view 側）。入力分岐 6 経路は分岐条件を 1 つも変えず、読みの位置だけが動く（「重複した読みを束ねる判定」節で 6 件を 1 行ずつ判定済み）。`SPEC.md` §8.6（`SPEC.md:437`）は `view.rs` も `SearchWindowView` も名指ししていない（grep 実測）→ **状態モデルとの不整合なし**。

**`/dry-check`** — grep パターン: `ctx.input\|input_mut\|\.input\(` / `engine.lock().unwrap().config()` / `configure_japanese_font`。

- `read_pre_widget_input` / `read_post_widget_input`: `ctx.input` 系の全ヒットは `view.rs` の 7 箇所と `runtime.rs:419`（ランタイム自身の別レイヤー）のみ。`results_view.rs` は 0 件 → 手書き重複なし
- `pub(super)` accessor 群: `state()` 1 本へ集約する設計ゆえ重複を作らない
- **〔反映済み・Phase 1〕** `view.rs:1020-1024` と `results_view.rs:437-441` の font_family 読みが完全同一（フォールバック `"Segoe UI"` まで一致・実測）→ `font_stack::font_family_from_config` へ寄せる

**`/plan-review`** — 台帳 4 件（`egui-shell-impl` / `src-tauri-periphery` / `governance-docs` / `independent-derivation`）。結果は下の「Step 5a-2」へ。

### Step 5a-2 — plan-review の統合結果

**配送（台帳 4 件中 4 件が実在）**

- A. `egui_shell` 実装 → `workspace/plan-review/egui-shell-impl.md`: 実在（問題なし 8・軽微 2・要対処 1・未検証 3）
- B. `src-tauri` 外周と検証 → `workspace/plan-review/src-tauri-periphery.md`: 実在（問題なし 7・軽微 4・要対処 1・未検証 4）
- C. ガバナンス文書・スキル → `workspace/plan-review/governance-docs.md`: 実在（問題なし 8・軽微 3・要対処 3・未検証 3）
- D. 独立導出（Step 2b） → `workspace/plan-review/independent-derivation.md`: 実在（512 行・§1〜8）。**ただし本人が汚染を自己申告している**——初期の `git grep` 1 回が `workspace/` を除外せず、`research.md` / `plan.md` の断片が文脈へ入った。以降は除外したが**完全な独立性は主張されていない**。ゆえに「本計画との一致」を独立確認として数えず、**相違点（下記）だけを証拠として採る**

不着はゼロ。再起動は行っていない。

**要対処の再照合（スカウト 6 件 → 全件成立・降格 0 件）**

すべて自分で根拠を開き直した。

1. `PERFORMANCE.md:157-158,179` が font 3 関数の所在として `view.rs` を名指し（再照合済み: `grep -rn "font_covers_cjk\|font_definitions\|configure_japanese_font" --include=*.md .`）→ **変更ファイル一覧と Phase 4 へ追加**
2. 規則 R(段 3) にヘルパー条項が無く font 内部 7 項目の行き先が決まらない（再照合済み: 設計書 §1 の条項 1〜3 を font 群へ当てて確認）→ **設計書へ条項 4（ADR-0008 の継承）を追加**
3. 設計書 §1.1 のメソッド数が 24 で §0 の 25 と不一致（`new` が未割当）→ **§1.1 に「`new` は分類ではなく分割」を明記**
4. ADR への言及が無い（否定の知識 8 件が非規範な `specs/` にのみ在る）→ **Phase 4 に「本 PR では書かず、本サイクルの `/retrospective` で回収する」を明記**（先例: ADR-0008 は #759 ではなく #762 で書かれた）
5. `cargo doc` は doc コメント参照 6 箇所を検知しない（再照合済み: `launch.rs:45-55` / `instant.rs:15-25` を開き、素のバッククォート code span であることを確認）→ **Phase 4 に「機構が守らない残余」と明記**
6. `font.rs` は名前が衝突する（再照合済み: `git ls-files | grep font` → `snotra-settings/src/font.rs`。`scripts/governance-check.mjs:77-78` が「basename 包含方式・wrong-directory 検出は放棄」と明記）→ **`font_stack.rs` へ改名**

**独立導出との差分（Step 2b）**

- **漏れ（導出 ∖ plan）— 4 件、すべて反映済み**: `font.rs` の basename 衝突 ／ `repaint.rs:301,307` の `"view.rs"` が**テスト fixture リテラル**であり一括置換で assert 緑のまま壊れること ／ `mod.rs` の re-export コメント 5 件は消費者が両モジュールへ割れるため 1 語置換で 5 件とも誤りになること ／ `.claude/skills/` の変更が最重要ルール 2（要合意）に当たること
- **据え置き（導出が挙げ、検算して不要と判定）— 1 件**: `snotra-core/src/{engine,folder}.rs` の「driver がキャッシュし」3 行。**ファイル名を含まない役割名**ゆえ driver が `LauncherController` になっても真のまま（判定を Phase 4 に記録し、次の読み手が「見落とし」と読まないようにした）
- **スコープ過剰（plan ∖ 導出）— 0 件**
- **不一致 — 1 件**: 導出は判別規則を「窓へ最後に適用した値の memo とその適用・描画 = view / それ以外 = controller」（規則 S）とし、`AppHandle` を「状態ではなく capability」として規則の射程外に置く。本計画の規則 R は同じ 19 フィールドを同じ側へ振り分けており**結論は完全に一致**する。**規則 R を採る**——規則 S は `font_stack` の行き先（条項 3・4 が担う部分）を説明できず、段 1 の ADR-0008 との連続性も持たないため
- **一致（完全性の証拠）**: 新規ファイル 2 という結論、`update()` を 1 関数のまま残す判断（確定事実 3）、入力の pre/post 2 段化（確定事実 1）、`smoke:egui` がこの変更を検出しないこと（trace 名が交わらない）——4 点が独立に再一致した

**総評** — completeness: **高**（台帳 4/4 実在・不着ゼロ・要対処 6 件すべて反映済み）。ただし D の汚染自己申告により、一致 4 点の証拠力は「完全な独立確認」ではなく「弱い独立確認」として扱う。実装着手可否: **可**。

### Step 5a-3 — codex exec による敵対的裏どり（同意ではなく反証を求めた）

`codex exec --sandbox read-only` に、計画の中核 3 主張の**反例を探させた**（プロンプトは「褒めるな・反証できないときだけそう明記しろ」）。

| 主張 | 結果 |
|---|---|
| ① 入力の読みを前へ寄せても安全（設計 §2.2） | **根拠の一方が偽と判明**（下記）。結論（安全であること）自体は反証されず |
| ② 規則 R(段 3) は例外ゼロで 68 項目を分類できる | 反証できず（条項衝突・行き先不定の具体例は挙がらなかった） |
| ③ 各フェーズが `-D warnings` 下で単独 green になる | 反証できず（E0502 になる現行行は特定できなかった。**実コンパイルは未実施**と明記された） |

**①の反証は成立し、一次資料で自ら確認した。** `InputState::key_pressed()` は `num_presses()` を経て **`self.events` を走査する**（`egui-0.35.0/src/input_state/mod.rs:743,750-760`）。ゆえに「egui の入力はフレーム内で不変だから読む順序は消費に影響しない」は**一般命題として偽**であり、設計 §2.2 の根拠 (a) をこれに置いていたのは誤りだった。`view.rs` の Enter 後置ブロックのコメントも同じ一般化を書いているが、それが成り立つのは **Enter を retain する箇所が無い**からである。

→ 設計 §2.2 を「局所的な 2 事実」（retain が除くのは ↑↓ だけ／段 13〜21 の間に ↑↓ を読む箇所が無い）へ書き直し、**2 つ目が構造保証ではないこと**と、`read_pre_widget_input` の doc へ「以後の `key_pressed(ArrowUp/Down)` は常に `false`」を書く項目を追加した。

**他に指摘 3 件**: (a) 設計 §2 の見出し「文の実行順序を 1 行も動かさない」が §2.2 と自己矛盾する全称表現 → 「**副作用**の実行順序」へ限定（`AGENTS.md`「書けないなら書かない」）。(b) Phase 1 の「`view.rs` の 3 箇所の呼び出し」は実際は 2 箇所（L187 は定義）→ 訂正。(c) doc 更新の個別列挙と grep sweep の関係が不明瞭 → **sweep を SSOT と明記**（`notify.rs:3` のような個別列挙に無い件が落ちるため）。

### Step 5b — plan-review が扱わない 3 観点

1. **境界条件** — 移設対象に新しい入力は無いため、境界は「既存の境界条件テストが移設後も同じ対象を測るか」に帰着する。フォント判定の境界（パース不能バイト列・Latin-only フォント・和文フォント不在時の skip）はテスト 7 件がそのまま `font_stack.rs` へ移り、対象も判定も変わらない。**新たに境界を持つのは `font_family_from_config` の統合だけ**で、その境界（`AppState` 不在 → `"Segoe UI"`）は統合前の 2 箇所と同一である（実測で文字列一致を確認済み）
2. **シンプル化の挑戦** — 「この複雑さは要るか」を 3 点で問い直した。(a) **3 モジュールは 2 で足りないか** → 足りない。`configure_japanese_font` の消費者が `results_view.rs` にあり（実測 2 箇所）、2 モジュールだとどちらかの view が他方に依存し続ける。(b) **アクセサを増やさず `pub(super)` フィールドで済まないか** → 済むが「遷移は controller だけが起こす」が規約に落ちる。`state()` 1 本で読みを全部通し変更を型で不能にできるので、複雑さは増えていない（設計 §3.7）。(c) **`update()` を分割しないという選択は逃げでないか** → 逃げではない。確定事実 3 の「最長の順序制約が関数の全長にわたる」がそれを禁じており、独立導出も同じ結論に達した。**新しい状態は 0 個・新しい失敗経路は 0 本**であり、「この操作が失敗したらどうなるか」は不変条件 16 が現行と同一であることを述べている
3. **破壊不変条件 + 検知手段** — 「破壊不変条件と検知手段」節の表 5 件がこれに当たる。**5 件すべてに検知手段が紐付いており、そのうち 4 件は実機目視である**。自動化されているのはフォント登録（テスト 7 件）だけで、これは受容ではなく**この領域に自動検出器が存在しない**という issue 確定の帰結である（trace presence を見る smoke が同クラスを緑で通した #671 PR A′ の実例）。`smoke:egui` の trace 名 5 件と `view.rs` が出す 14 件は交わらない（B・D が独立に実測）——**「smoke が緑だから移設は安全」と読める余地を残さないため、この事実を計画に明記した**
