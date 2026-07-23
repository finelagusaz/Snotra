# SU6: 統合 glue（config 反映 + #633 + §12 IME parity）設計

日付: 2026-07-24 / 対象: #532 Phase 2 SU6 / 先行: SU5（PR #647・main マージ済）
ロードマップ: `2026-07-21-phase2-softbuffer-migration-roadmap.md` の SU6 行
改訂: 2026-07-24 マルチパースペクティブレビュー（並行性 / SPEC・SolidJS parity / スコープ / codex 反証の 4 レンズ）反映。主な設計変更は決定 2（width を view 単独 writer 化）・決定 3（クリア → 表示ゲート + 世代カウンタ）・決定 4（PlatformBridge 経由）

## 背景と言語化

ロードマップの SU6 は 5 項目（config_watcher 反映・#633・§12 IME parity・終了保存整合・設定サイドカー共存）だが、SU3〜SU5 が egui 側を「毎フレーム live-read」で作ってきたため、実態は以下に圧縮される:

- **値の反映は大半が済んでいる**: テーマ 5 色/選択色（`view.rs` update() 冒頭で毎フレーム `set_visuals`）・visible_rows・instant_prefix・show_icons・result_limit・language（`lang()` 都度読み）・auto_hide_on_focus_lost・follow_cursor_monitor・ime_off_on_show（フラグ自体）・hotkey/hotkey_toggle/tray（platform 層・窓非依存）は live-read か窓非依存で追従する（§7.5 全行をレビューで総当たり済み・未反映項目なし。auto_update mode は両経路とも起動時一回読みで対象外）
- **欠けているのは wake**: runtime はイベント駆動（RedrawRequested 待ち）のため、可視アイドル中に config が変わっても repaint が来ず stale 表示が残る。SU5 で確立した横断不変条件「UI 状態を変えたら `request_repaint` で起こす」の config 変更への適用が SU6 の本丸
- **live-read でない例外は 3 つ**: `font_family`（`setup()` で一度きり）・`window_width`（config_watcher の set_size が `get_webview_window("main")` 直書きで flag ON では何も起きない）・**native 背景ブラシ**（`create()` の `background_color` が生成時一度きり。painted panel は live だがリサイズ時に露出する native surface の色は追従しない——レビューで発見）
- **サイドカー共存はほぼ済み**: `/o`→`open_settings` は view.rs から呼出済み（**注意: settings を開くのは `/o`。`/s` は RebuildIndex**——初版 spec の事実誤りをレビューで訂正）、settings 存命中の alwaysOnTop 一時解除も SU2 で flag ON 分岐（`get_window`）対応済み。残余は end-to-end スモークのみ
- **終了保存は窓非依存**: `flush_persistent_state` は engine(history) + IconCacheState のみ読む窓非依存ルーチンで、`exit-requested` listener（トレイ Exit・`/q` 共有）と updater の `on_before_exit` が共有する

## 決定事項

### 決定 1: wake 機構は「単一合図 + live-read 徹底」（案 A）

`apply_config_change` の末尾（`update_config` 後）で `app.emit("config-applied", ())` を**無条件** emit（WebView2 側に listener が無く flag OFF では無害）。`egui_shell/mod.rs` が `app.listen("config-applied", ...)` で受け、`EguiShellState.egui_ctx`（SU5 実装済み）経由で `request_repaint` を積む。**値は運ばない**——次フレームの live-read が全て拾う。

- 却下 B（per-event listener 群・WebView2 同型）: egui は値を config から読む設計のため二重配送（イベント値 vs live-read 値の二正本化）。listener 10 本の肥大。SU7 で WebView2 イベント自体を削る流れと逆行
- 却下 C（可視中ポーリング）: アイドル CPU/電力に直結し #628/SU6.5 と逆行
- hidden 中の wake 空振りは無害（hidden 中 update() は走らない・SU5 実測）。次回 show 時の live-read が最新値を描く
- **listener 登録位置は pin する**: main.rs setup の egui block（`egui_shell::create` 近傍）で、**config_watcher 起動・startup_display より前**に登録する（可視窓が合図を取りこぼす窓を作らない・並行性レビュー）
- **「値を運ばない」は load-bearing な安全性前提**: `egui_ctx` が None の期間（setup〜初フレーム）に合図が来ても、初 show フレームの live-read が最新を描くから benign。将来イベントに値を載せる変更はこの前提を壊す
- 既知の受容: `update_config` は hotkey 再登録の同期待ち（`recv_timeout` 2s）より後にあり、hotkey が詰まると反映が最大 2 秒遅れる。これは WebView2 の `visual-config-changed`（同じく末尾 emit）と同一の既存挙動であり、SU6 は同時刻性 parity を保つ（watcher 側の順序改変は非スコープ）

### 決定 2: live-read 例外 3 つの処置（dirty flag 配管は作らない）

- **font_family**: view が `applied_font_family: String` を保持し、フレーム冒頭で config 値と比較、差分時に `configure_japanese_font` を再実行。**`applied_font_family` は解決の成否に依らず config 値へ無条件更新する**（未解決フォント名で毎フレーム `load_system_fonts`（数十 ms）が走る perf cliff の回避・並行性レビュー）。再実行後は `request_repaint`（set_fonts は次フレーム適用のため）。なお WebView2 経路は `--font-family` CSS 変数で即時反映しており、このホットリロードは parity 上**必須**（スコープレビューで裏取り）
- **window_width**: **view を唯一の size writer にする**。動的高さの `set_size`（view.rs）が現在 `inner_size()` から読んでいる幅を config の `window_width` live-read に変え、幅・高さいずれかの差分で `set_size` する。初版の「config_watcher に flag ON 分岐を追加」案は**却下**（否定の知識）——notify スレッドの幅 set_size と egui スレッドの高さ set_size が 2 次元 read-modify-write で潰し合う race（幅巻き戻し / `last_set_height` ガードの誤高さ固着）を並行性レビューが特定。view 単独 writer なら watcher 側変更ゼロで race 自体が消える（flag ON では `get_webview_window` が None を返し既存コードが自然に no-op）
- **native 背景ブラシ**: `background_color` は窓生成時一度きりのため、実行時のテーマ変更後は リサイズ時に露出する native surface が旧色のまま（リサイズフラッシュ対策の要件が壊れる・codex 反証）。テーマ変更のエッジ検出（font と同型）で `Window::set_background_color` を呼ぶ——**API の実在は plan 段階で確認し、無ければ受容残余として SPEC §11 に明記**する

### 決定 3: #633 は「表示ゲート + 世代カウンタ」で解く（§4.7）

初版の「false→true で plain 結果と選択をクリア」は**却下**（否定の知識・2 レンズが独立に反証）:

- **クリアは SolidJS parity ではない**: SolidJS は indexing-started で `setIndexing(true)` するだけで結果も選択も保持し、非表示は派生 memo `shouldShowResults`（`search.ts` の `interpKind()==="instant" || !indexing()`）が担う。SPEC §4.7 は **instant carve-out**（indexing 中もインスタントコマンドは表示継続）を明記しており、無条件クリアは instant 行まで消す
- **bool エッジ検出はパルスを見逃す**: `finish_index_build`（indexing=false）は `notify_indexing_complete` より**先**に走る（indexing.rs 一次確認済み）。速い再構築では started/complete の 2 つの `request_repaint` が 1 フレームに合体し、update() は prev=false/now=false しか見ない——クリアも再検索も走らず旧 index の結果で起動できてしまう（act-on-stale・並行性 Critical + codex が同一指摘）

採用する設計:

- **表示ゲート**: 結果リストの表示判定に indexing の live-read ゲートを足す（`shouldShowResults` の鏡写し: plain 結果のみ非表示・instant/folder/tool は対象外・選択とデータは保持）。indexing 中は既存の検索バー hint が案内する
- **世代カウンタ**: `AppState` に単調増加の `index_generation: AtomicU64` を追加し、index build 完了時に bump。view は last-seen 世代と比較し、差分があれば現クエリで `run_search` を再実行（SolidJS `runRefresh()` 相当）。カウンタは累積するため、フレーム合体で true 窓を丸ごと見逃しても再検索が保証される（folder の drain-latest と同型のアキュムレータ化）
- wake は `indexing-started` / `indexing-complete` を mod.rs の listener 群に足して `request_repaint`（合図のみ・payload 不使用）
- hidden 中に flip した場合は wake 空振り + reset-on-show でクエリごと消えるため自然に整合する

### 決定 4: §12 IME parity は既存 PlatformCommand の再利用

egui の show 経路（`egui_shell/mod.rs` `show_egui_main`）に追加: `ime_off_on_show` を実行中 config から都度読み（キャッシュしない・#576 同型）、有効なら **`PlatformCommand::TurnOffIme(hwnd.0 as usize)` を `send_command`**（WebView2 の `apply_ime_control` と同一の委譲。rule「Win32 API は PlatformBridge 経由」整合）。`TurnOffIme` は生 HWND(usize) を取るため窓型非依存で、初版の「`&Window` 一般化」は**不要**（parity レビュー訂正）。**配置は focus 同期（`SendMessageTimeoutW(WM_NULL)`）の後**——前に置くと IME オフが対象窓に効かない（WebView2 側 doc の警告条件）。復元なし（§12 どおり）。

### 決定 5: 再変換（IMR_RECONVERTSTRING）は defer・別 issue（否定の知識）

コード確認の結果、**再変換は egui 経路に実装が存在せず動きようがない**: runtime の IMM32 subclass（`snotra-egui-runtime/src/windows_ime.rs`）は `WM_IME_STARTCOMPOSITION`/`WM_IME_COMPOSITION`/`WM_IME_ENDCOMPOSITION` のみ処理し、`WM_IME_REQUEST`（`IMR_RECONVERTSTRING`）応答はリポジトリ全体に無い。**「製品 IMM32 コードを移植」という #582 メモの表現は不正確**——WebView2 経路の再変換は WebView2 のエディットコントロールが無償提供していたもので、移植元の製品コードは存在しない。やるなら新規実装。

ロードマップの既存判断（再変換は WANT・切替をブロックしない・困難なら defer）に従い、SU6 では issue 起票のみ行い（ロードマップ決定 2 の記述と紐付ける）、flip 後に判断する。

## 確認項目（コード変更なしの見込み・スモークで裏取り）

- 終了保存: トレイ Exit → `exit-requested` → history/icon 保存を flag ON + `SNOTRA_TRACE=1` で実測。**加えて Alt+F4 / OS close 要求の挙動を両経路（flag ON/OFF）で確認する**——`RunEvent` 経由の flush は両経路とも存在せず（`on_before_exit` は updater 専用）、穴があるなら**対称の既存 gap**。対称なら受容し必要に応じ別 issue（SU6 非スコープ）、egui 側だけ挙動が違うなら SU6 で対処判断
- サイドカー: **`/o`** → settings 起動 → alwaysOnTop 解除/復帰 → 保存 → watcher → glue → egui 反映の end-to-end
- hotkey / tray: 設定変更の反映（platform 層・窓非依存）をスモークで確認のみ

## テストと受け入れ

- 純粋核ユニット: 表示ゲート述語（indexing × interp 種別 → plain のみ非表示）・世代カウンタ比較→再検索判定・font 変化述語（無条件更新を含む）
- driver: 実機スモーク `SNOTRA_EGUI_MAIN=1; cargo run -p snotra`（テーマ色変更が可視中に反映 / 幅変更 / **font_family + 結果行 font_size** 変更 / 再インデックス中の plain 非表示 + instant 表示継続 → 完了後再表示 / 速い再構築でも stale が残らない / IME オフ / 終了保存 / サイドカー end-to-end）
- **スモークの範囲注意**: 検索入力欄（TextEdit）の font_size 追従は #643 の領分（52px バー rework 待ち）で SU6 の合否に含めない
- 受け入れ（ロードマップ SU6 行）: config 変更の反映（テーマ/ホットキー/index）・終了保存・サイドカーが egui ウィンドウで動く。セッション中の再インデックスで stale 結果が消える（#633・§4.7）。§12 IME 制御が egui 経路で機能する

## 付随作業

- 再変換 defer の issue 起票（WANT・flip 非ブロック・決定 5 の根拠を転記・ロードマップ決定 2 と相互参照）
- SPEC §7.5 へ egui 経路の反映機構（wake 合図 + live-read）を additive に as-built 追記（WebView2 イベント名の既存行は両経路併存中のため現状維持）
- **#648(B) の取り込み**: `strings.rs` の `//!` と `egui_shell/mod.rs` の「言語は起動時一回読み」コメントは as-built（`lang()` 都度読み）と矛盾する stale 記述——language 反映と同時に是正（#648 が SU6 同時を明示指定）
- #633 は SU6 PR で close（PR 本文の closing 手順は CLAUDE.md の squash 手順に従う）
- #643 とは関心が別（family vs size）だが双方 view.rs のフォント処理に触るため、実装順が重なる場合は rebase 順を調整
