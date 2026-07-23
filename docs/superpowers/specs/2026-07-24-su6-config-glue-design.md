# SU6: 統合 glue（config 反映 + #633 + §12 IME parity）設計

日付: 2026-07-24 / 対象: #532 Phase 2 SU6 / 先行: SU5（PR #647・main マージ済）
ロードマップ: `2026-07-21-phase2-softbuffer-migration-roadmap.md` の SU6 行

## 背景と言語化

ロードマップの SU6 は 5 項目（config_watcher 反映・#633・§12 IME parity・終了保存整合・設定サイドカー共存）だが、SU3〜SU5 が egui 側を「毎フレーム live-read」で作ってきたため、実態は以下に圧縮される:

- **値の反映は大半が済んでいる**: テーマ 5 色/背景/選択色（`view.rs` update() 冒頭で毎フレーム `set_visuals`）・visible_rows・instant_prefix・show_icons・result_limit・language（`lang()` 都度読み）・ime_off_on_show（フラグ自体）・hotkey/hotkey_toggle/tray（platform 層・窓非依存）は live-read か窓非依存で追従する
- **欠けているのは wake**: runtime はイベント駆動（RedrawRequested 待ち）のため、可視アイドル中に config が変わっても repaint が来ず stale 表示が残る。SU5 で確立した横断不変条件「UI 状態を変えたら `request_repaint` で起こす」の config 変更への適用が SU6 の本丸
- **live-read でない例外は 2 つ**: `font_family`（`setup()` で一度きり）・`window_width`（config_watcher の set_size が `get_webview_window("main")` 直書きで flag ON では何も起きない gap を確認済み）
- **サイドカー共存はほぼ済み**: `/s`→`open_settings` は view.rs から呼出済み、settings 存命中の alwaysOnTop 一時解除も SU2 で flag ON 分岐（`get_window`）対応済み。残余は end-to-end スモークのみ
- **終了保存は窓非依存の見込み**: `setup_exit_listener` は `exit-requested` イベント listen。flag ON での実測確認が残余

## 決定事項

### 決定 1: wake 機構は「単一合図 + live-read 徹底」（案 A）

`apply_config_change` の末尾で `app.emit("config-applied", ())` を**無条件** emit（WebView2 側に listener が無く flag OFF では無害）。`egui_shell/mod.rs` が `app.listen("config-applied", ...)` で受け、`EguiShellState.egui_ctx`（SU5 実装済み）経由で `request_repaint` を積む。**値は運ばない**——次フレームの live-read が全て拾う。

- 却下 B（per-event listener 群・WebView2 同型）: egui は値を config から読む設計のため二重配送（イベント値 vs live-read 値の二正本化）。listener 10 本の肥大。SU7 で WebView2 イベント自体を削る流れと逆行
- 却下 C（可視中ポーリング）: アイドル CPU/電力に直結し #628/SU6.5（hidden・アイドルで再描画停止の確認）と逆行
- hidden 中の wake 空振りは無害（hidden 中 update() は走らない・SU5 実測）。次回 show 時の live-read が最新値を描く

### 決定 2: live-read 例外 2 つは個別処置（dirty flag 配管は作らない）

- **font_family**: view が `applied_font_family: String` を保持し、フレーム冒頭で config 値と比較、差分時に `configure_japanese_font` を再実行（`ctx.set_fonts` は次フレーム適用でフレーム間安全）。「live-read + エッジ検出」で統一し、listener→view の dirty flag 配管を作らない
- **window_width**: config_watcher の `width_changed` 分岐に flag ON → `get_window("main")` の分岐を追加（`launch_settings_process` が SU2 で確立した同型パターン。flag OFF の既存コードは不変）

### 決定 3: #633 は indexing エッジ検出で解く（§4.7）

view に `prev_indexing: bool` を持ち、update() 冒頭でエッジ検出:

- **false→true**（セッション中の再インデックス開始）: plain 結果と選択をクリア → 既存の indexing hint が live-read で表示される。folder 面は fs 由来（index 非依存）のため保持
- **true→false**（完了）: 現クエリで `run_search` を再実行（SolidJS `search.ts` の `indexing-complete`→`runRefresh()` parity）

wake は `indexing-started` / `indexing-complete`（indexing.rs が emit 済み）を mod.rs の listener 群に足して `request_repaint`（合図のみ・payload 不使用）。エッジ判定の決定ロジックは純粋核（`search_state.rs` 近傍）に述語として置きユニットテスト対象にする。

hidden 中に flip した場合は wake 空振り + reset-on-show でクエリごと消えるため自然に整合する。

### 決定 4: §12 IME parity は show 経路への 1 呼び出し

egui の show 経路（`egui_shell/mod.rs`）に WebView2 `show_main_and_emit` と同型の処置を追加: `ime_off_on_show` を実行中 config から都度読み（キャッシュしない・#576 同型）、有効なら egui 窓 HWND へ `ime.rs`（`ImmSetOpenStatus(false)`）を適用。復元なし（§12 どおり）。`apply_ime_control` が `&WebviewWindow` 前提なら `&Window` へ一般化する（SU2 の `position_on_target_monitor` 一般化と同じ手筋）。

### 決定 5: 再変換（IMR_RECONVERTSTRING）は defer・別 issue（否定の知識）

コード確認の結果、**再変換は egui 経路に実装が存在せず動きようがない**: runtime の IMM32 subclass（`snotra-egui-runtime/src/windows_ime.rs`）は `WM_IME_STARTCOMPOSITION`/`WM_IME_COMPOSITION`/`WM_IME_ENDCOMPOSITION` のみ処理し、`WM_IME_REQUEST`（`IMR_RECONVERTSTRING`）応答はリポジトリ全体に無い。**「製品 IMM32 コードを移植」という #582 メモの表現は不正確**——WebView2 経路の再変換は WebView2 のエディットコントロールが無償提供していたもので、移植元の製品コードは存在しない。やるなら新規実装。

ロードマップの既存判断（再変換は WANT・切替をブロックしない・困難なら defer）に従い、SU6 では issue 起票のみ行い flip 後に判断する。

## 確認項目（コード変更なしの見込み・スモークで裏取り）

- 終了保存: トレイ Exit → `exit-requested` → history/icon 保存を flag ON + `SNOTRA_TRACE=1` で実測
- サイドカー: `/s` → settings 起動 → alwaysOnTop 解除/復帰 → 保存 → watcher → glue → egui 反映の end-to-end
- hotkey / tray: 設定変更の反映（platform 層・窓非依存）をスモークで確認のみ

## テストと受け入れ

- 純粋核ユニット: indexing エッジ決定（クリア/再検索の判定）・font 変化述語
- driver: 実機スモーク `SNOTRA_EGUI_MAIN=1; cargo run -p snotra`（テーマ色変更が可視中に反映 / 幅変更 / font 変更 / 再インデックス中の結果クリア→完了後再表示 / IME オフ / 終了保存 / サイドカー end-to-end）
- 受け入れ（ロードマップ SU6 行）: config 変更の反映（テーマ/ホットキー/index）・終了保存・サイドカーが egui ウィンドウで動く。セッション中の再インデックスで stale 結果が消える（#633・§4.7）。§12 IME 制御が egui 経路で機能する

## 付随作業

- 再変換 defer の issue 起票（WANT・flip 非ブロック・上記決定 5 の根拠を転記）
- SPEC §7.5 へ egui 経路の反映機構（wake 合図 + live-read）を as-built 追記
- #633 は SU6 PR で close（PR 本文の closing 手順は CLAUDE.md の squash 手順に従う）
