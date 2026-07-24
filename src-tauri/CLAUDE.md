# src-tauri

Tauri v2 バイナリ crate。検索 UI（`egui_shell/`・egui + softbuffer）と Win32 API 統合を担当（WebView2/フロントエンドは #532 SU7 で撤去）。

各ルールは「**太字 = 守る指示**、後続 = 理由・経緯」の形式。迷ったら太字部分に従えば安全。

## モジュール構成

責務を持つ個別モジュールの責務宣言は各ファイルの `//!`（module doc）を正本とする（薄いラッパーを集約記述する `commands/`・`platform/` は責務を本節に直接記す例外）。本節はファイル一覧と、`//!` に収まらない**横断不変条件・チェックリスト**を記す（#562）。

- `main.rs` — エントリポイント・Tauri セットアップ・イベントリスナー登録（責務は `//!`）
- `state.rs` — Tauri managed state `AppState`（責務・構成は `//!`）。以下はビルドフラグの規律:
  - **インデックスビルドの開始/終了は `try_begin_index_build()` / `finish_index_build()` メソッド経由で行う** — `indexing`・`index_build_started` を coherent に更新する
  - **config 変更→index 再構築のコヒーレンシ判断は engine の `index_stale` ledger（軸1）に閉じており、この 2 AtomicBool は二重ビルド防止（CAS）と UI 表示専用に純化されている**（#347/#348-A）
- `icon.rs` — アイコンのオンデマンド抽出とキャッシュ永続化（責務は `//!`）。**`invalidate_icon_cache` はメモリ内 `IconCacheState` と `icons.bin` を単一 lock 内で両方無効化する** — lock 外でファイル削除すると、並行ロード（None 検知 → `icons.bin` 再ロード）が削除直前の旧ファイルをメモリへ戻す TOCTOU が起きる（#522、実測 17/2000 回）。片方だけだと終了時 `save_if_dirty` で古いアイコンが復活する
- `indexing.rs` — バックグラウンドインデックス構築（責務は `//!`）。以下は drain / panic 戦略の不変条件:
  - **`start_index_build` は `mark_index_stale`（CAS の前）→ CAS → spawn の順**で、**drain ループ**（`begin_index_drain` で現在 config の `IndexInputs` snapshot → ロック外で `rebuild_and_save` / `PrebuiltIndex::new` → `complete_index_drain` で swap + re-diff）を stale が消えるまで回す
  - **ビルド本体は `catch_unwind` で包む（panic 戦略依存）**: unwind ビルド=debug/test では panic を捕捉し `finish_index_build` で flag 固着 wedge を防ぐ。release は Cargo.toml で `panic="abort"` のため build panic はプロセス abort＝ここに来ないが silent wedge にもならず、再起動で fresh build される。どちらでも UI 永久構築中は起きない
  - **finish 後に `is_index_stale` を再チェック**し、finish 窓で刺さった変更を再 kick で拾う。**unwind の panic 経路では再 kick しない**（決定論 panic の無限リトライ回避）
  - config 変更→index 再構築のコヒーレンシは engine の `index_stale` ledger に一元化（#347/#348-A）
- `config_watcher.rs` — `config.toml` 監視（100ms debounce）と `apply_config_change()` による反映（責務は `//!`）。以下は適用の不変条件と発火イベント:
    - **不変条件: `LoadOutcome::ReadFailed`（一時的・環境的な read 失敗）では `apply_config_change` は何も適用せず早期 return する**（`should_apply_config_change()` で判定）。fallback-default を実行中エンジンへ適用すると、live-read 化した履歴剪定が `history.bin` をデータ損失させ、index 再構築判定（`IndexInputs` 差分）が default scan で誤再構築を起こすため。`Config::load` の「一時的失敗は退避も上書きもしない」保全を適用側にも揃える（#348）
    - ただし早期 return の前に短いバウンドリトライ（`load_with_read_failed_retry`、既定 3 回 × 150ms）で一時的ロック解除を待ち、解ければ正規の変更を取りこぼさず適用する（予算超過時のみ skip。リトライ中は適用しないのでデータ損失安全は不変）
  - **不変条件: index 再構築の要否は `IndexInputs::from_config(old) != IndexInputs::from_config(new)` で判定し、ビルド進行中（`indexing`）でも `!indexing` ゲートなしで常に `start_index_build` を kick する**（`start_index_build` が `mark_index_stale` で stale を立て、in-flight ビルドの drain / finish 後再チェックが取りこぼしを拾う。CAS が二重起動を防ぐ。#347/#348-A）
  - 発火するイベント: `hotkey-registration-failed` / `indexing-started`（indexing.rs から）/ `indexing-complete`（indexing.rs から）/ `config-applied`（egui wake・値なし・SU6）。旧フロント向けの値運搬 emit 群（language-changed 等 7 本）は #532 SU7 で削除——egui は config-applied wake + 毎フレーム live-read で値を拾う
- `ime.rs`: IME オフ操作（`ImmSetOpenStatus(false)`）。Win32 IMM API の薄いラッパー
- `trace.rs`: `SNOTRA_TRACE` 環境変数ゲートの構造化トレースログ（`trace_enabled` + `trace`）。`main.rs` の `trace_main` と `commands::trace_command` が薄いラッパーとしてここへ委譲する（#433 で重複を集約）。seq カウンタは単一の `AtomicU64` を共有するため、両者のトレース行は1本の単調増加列で interleave する（旧実装は 2 カウンタに分裂していた。トレースはデバッグ出力専用のため許容される挙動変更）
- `monitor.rs`: マルチモニター対応の Win32 ヘルパー（`GetCursorPos` / `MonitorFromPoint` / `MonitorFromWindow` / `GetMonitorInfoW`）。物理座標ベースで作業領域を取得し、ウィンドウ位置のクランプ・中央配置を提供
- `working_set.rs`: 非表示アイドル時に Win32 `EmptyWorkingSet` でプロセスツリー全体（自プロセス + WebView2 子孫）の物理 working set を回収（Windows のみ、非 Windows は no-op）。`collect_descendant_pids()` は Toolhelp の (pid,ppid) 上を BFS する純関数（Win32 非依存・ユニットテスト対象）。`trim_idle_working_set()` は hide 経路（`egui_shell::hide_egui_main` 合流点）から呼ばれる best-effort 操作で、HANDLE は RAII ガードで解放し、全 Win32 失敗を握りつぶす（#532 SU6.5・egui-hidden の PrivWS を可視時同値 43MiB → ~1MiB へ落とす。旧 WebView2 suspend は SU7 で消滅）
- `commands/`: ディレクトリモジュール（`mod.rs` + `launch.rs` / `icon.rs` / `window.rs` / `system.rs` / `instant.rs`）。egui view・トレイが共有する core 関数群（旧 `#[tauri::command]` ラッパーと `search.rs` / `config.rs` は #532 SU7 のフロント撤去で消滅）。`launch.rs` は `launch_item_core` / `launch_with_tool_core`（いずれも `pub(crate)`、`instant.rs`・`egui_shell/view.rs` から再利用）に加え、トレイメニューからの起動用に `launch_item_with_state` / `launch_with_tool_with_state` / `launch_default_with_state` / `resolve_all_openers` を `pub` で公開
- `platform/`: ディレクトリモジュール（`mod.rs` + `hotkey.rs` / `tray.rs` / `wndproc.rs`）。Win32 メッセージループスレッド + トレイアイコン + ホットキー + ウィンドウプロシージャ
- `egui_shell/`: ディレクトリモジュール（`mod.rs` + `lifecycle.rs` / `search_state.rs` / `layout.rs` / `icon_textures.rs` / `notify.rs` / `strings.rs` / `view.rs`）。製品メインウィンドウ（egui/softbuffer）の外殻 + 検索体験（#532 SU2〜SU7・flip 済みで唯一の UI 経路）。`lifecycle.rs` は純粋核（`plan_hotkey` / `blur_should_hide`）、`search_state.rs` は検索状態の純粋核（`SearchState` / `interpret` / `QueryIntent`）、`layout.rs` は高さ算出 + debounce の純粋核（`compute_window_height` / `Debouncer`）、`icon_textures.rs` はアイコン・テクスチャ層の純粋核（PNG→ColorImage decode・抽出要否述語・可視集合 retain。worker spawn / load_texture の driver は `view.rs` が持つ・#532 SU4）、`notify.rs` は通知 primitive の純粋核（一時通知 NoticeSlot + updater toast 状態機械 UpdaterUi・#532 SU5）、`strings.rs` は UI 文言テーブル（言語は view.rs lang() の毎フレーム live-read）、`view.rs` は検索 view（TextEdit・結果リスト・キーボードナビ・起動・動的高さ・indexing 案内）、`mod.rs` は窓生成・show/hide・位置永続・reset-on-show・hide/config-wake listener（責務は各 `//!`）
  - **イベント駆動 wake の不変条件（#532 SU5）**: runtime はイベント駆動（`RedrawRequested` 待ち）で通常フレームは勝手に回らない。**フレームの paint より後（遅延 dispatch・クリックハンドラ）や worker スレッドで UI 状態を変えたら、必ず `ctx.request_repaint()` で次フレームを起こす**——欠くと次の無関係な入力まで stale 表示が残る（toast dismiss で実測・PR #647 の e746826 で修正。folder/icon worker の送信毎 repaint と同根）。また **hidden 中は `update()` が走らない**（実測・SU5 要石）——時限処理（timeout・通知期限）の `request_repaint_after` は可視中しか効かず、hide を跨ぐ in-flight 状態は reset-on-show の backstop（クリア）とセットで設計する
  - `wndproc.rs`: `SendMessage` 経由で届く `WM_TRAY_ICON` および `WM_CONTEXTMENU` を `PostThreadMessageW` でスレッドキューに再投入し、メッセージループでの統一処理を保証

## 実装パターン

- ホットキーは `RegisterHotKey` を `platform/` の Win32 メッセージループスレッドで処理し、`AppHandle.emit()` で Tauri イベントとして通知
- 設定画面は `snotra-settings.exe`（egui 別バイナリ）を子プロセスとして起動。`SettingsProcessState` で重複起動を防止し、子プロセス存命中はメインウィンドウの `alwaysOnTop` を一時解除する
- `commands/` は薄い共有関数に保ち、実処理は `snotra-core` に寄せる（KISS）
- `AppState` は `Mutex<Engine>` で検索エンジン・履歴・設定を一括管理。Phase 2.3 以前の 3重ロック（`Mutex<SearchEngine>` / `Mutex<HistoryStore>` / `Mutex<Config>`）は Engine facade に統合済み
- **インデックスビルドのフラグは `AppState` のメソッド経由で更新する**: `try_begin_index_build()`（`index_build_started` を CAS 取得 → `indexing` を立てる）と `finish_index_build()`（両方を戻す）が唯一の正しい経路。`indexing` / `index_build_started` を直接 `store()` しない——外部からの force-reset は走行中ビルドのガードを踏み倒す競合の原因になる。2フラグは別物（`index_build_started` は CAS 専用ガード、`indexing` は first-run 時にビルドスレッド不在でも true になる UI 表示用）
- Managed state として `IconCacheState`（`Mutex<Option<IconCache>>`、初回アイコン要求で遅延初期化）と `SettingsProcessState`（`Mutex<Option<Child>>`、設定プロセスのハンドル管理）を保持
- **show の操作順序制約（`egui_shell::show_egui_main`）**: 高さリセット（52px）→ `position_on_target_monitor` → `show()` の順。位置計算はウィンドウサイズでクランプするため、高さリセット前に位置を決めると展開時の高さでクランプされ、折りたたみ時に位置がずれる

## 共有 core 関数の返り値契約

旧 IPC コマンドの3系統規約（#434）のうち、フロント撤去（#532 SU7）後も残る契約:

1. **読み取り・検索系**: 素の `T` を返す。エラーは DTO 内の `is_error` フラグ + UI 層で表示文字列を決定する（`snotra-core` の設計と整合）
2. **起動系**: `LaunchResult { status, code, message }` 契約（`launch_item_core` / `launch_with_tool_core` / `execute_instant_action_core`）
3. **失敗しうる操作系**: `Result<T, String>`。「実行できない状態」（インデックス構築中など）も `Err(定数)` で表現する。例: `open_settings` / `rebuild_index`

`bool` 返しは新規関数で使用しない（「成功/失敗」と「実行できない状態」を混同しやすい）。「インデックス構築中で実行できない」の定数は `ERR_INDEXING_IN_PROGRESS`（`commands/window.rs`）を `open_settings` / `rebuild_index` で共有する。新たに「実行できない状態」を追加する場合もこの定数を再利用するか、命名パターン（`ERR_<状態>`）を揃える。

## Win32 メッセージ配送の注意

Shell のトレイコールバック (`uCallbackMessage`) は `SendMessage` で配送される場合があり、`GetMessageW` ループに到達しない。カスタムメッセージ (`WM_APP + N`) をウィンドウプロシージャ (`DefWindowProcW`) だけで処理すると消滅するため、`platform_default_wnd_proc` で検出して `PostThreadMessageW` でスレッドキューに再投入する設計にしている。

NOTIFYICON_VERSION_4 では、キーボード操作（Shift+F10 / Application キー）によるコンテキストメニュー要求は `uCallbackMessage` を経由せずウィンドウプロシージャに直接 `WM_CONTEXTMENU` として届く。`platform_default_wnd_proc` で同様に再投入することで `handle_tray_message` に統一している。

**Win32 メッセージハンドラを削除・変更する前に「そのメッセージが届く全経路」を列挙すること。** 同一メッセージでも発火源が複数ある場合がある（例: `WM_CONTEXTMENU` はマウス右クリック環境とキーボード操作の両経路で届く）。「問題の原因になっている経路」だけを削除しようとすると、問題でない別の経路も同時に消える。

## ウィンドウ生成の制約

ウィンドウの生成は必ず setup フェーズで行い、ランタイムでは show/hide のみで制御する（メイン窓は `egui_shell::create`・setup 限定）。イベントループ中のコールバック（`run_on_main_thread` / `listen` / `RunEvent` 等）はメッセージポンプが 1 イテレーション内で停止しており、ポンプ進行を要する操作（ウィンドウ生成・COM STA 初期化・モーダルダイアログ等）はデッドロックする——「メインスレッドにいる」と「メッセージポンプが自由に回る」は別物（旧 WebView2 期に実測した不変条件・egui 窓でも生成は setup 限定を維持）。メインウィンドウは `decorations: false` で閉じるボタンを持たないため `CloseRequested` ハンドラは不要。

## working set の能動回収（EmptyWorkingSet）

`working_set::trim_idle_working_set()` が hide 経路（`egui_shell::hide_egui_main` 合流点）で Win32 `EmptyWorkingSet` をプロセスツリー（自プロセス + 子孫。設定サイドカー存命中はそれも含む）へ能動適用し、hide 直後の物理 RSS を即時に落とす（egui-hidden の PrivWS ~1MiB・#532 SU6.5 実測。旧 WebView2 suspend 層は SU7 で消滅）。

- **show 側に逆操作は不要**: trim されたページは show 時に OS が透過的に re-fault する。明示 untrim API は存在しない。trim が hide 前後どちらで走っても無害（再 fault するだけ）
- **best-effort・物理 RAM のみ**: Toolhelp / `OpenProcess` / `EmptyWorkingSet` の全失敗は黙ってスキップ（機能影響ゼロ）。HANDLE は RAII ガードで解放。削減対象は working set であって commit ではない。`EmptyWorkingSet` はスレッド非依存で任意スレッドから呼べる

## Win32 / Tauri 注意事項

- Win32 関連の不具合では、まず `config.toml`（テーマ含む）を確認し、次にウィンドウライフサイクル順序、最後に API 呼び出しを調査する（白画面バグの真因がテーマ設定だった事例あり）
- Rust クレートをバージョン昇格する際は、対象バージョンが crates.io に実在・正当であることを確認する。大版ジャンプを前提にしない（例: `bincode 3.0.0` は `compile_error!` のみを含むジョークパッケージでコンパイル不能）
- `windows` クレート（現在 v0.62）はバージョンごとに API シグネチャが変わる（`Result` 型の有無、ハンドル型の変更など）。コードを書く前に、使用中のバージョンで対象 API が利用可能か・型が一致するかを確認する
- `webview2-com` は `windows-core 0.61` に依存するが、プロジェクトの `windows` クレート（v0.62）は `windows-core 0.62` を使う。`Interface::cast()` 等を呼ぶ際は `windows-core_0_61 = { package = "windows-core", version = "0.61" }` のエイリアス依存を使い、`use windows_core_0_61::Interface` とする
- 必要な feature フラグ（`Win32_UI_WindowsAndMessaging` 等）が `Cargo.toml` に宣言されているか確認してから実装する
- `UpdateWindow` など一部 API は windows クレートのバージョンによっては未提供。代替 API（`RedrawWindow` 等）の存在を事前に調べる
- Windows パスの正規化では `C:` と `C:\` の違いに注意する（ドライブルートは末尾 `\` が必須）
- ファイルメタデータ取得時、シンボリックリンクを考慮する場合は `symlink_metadata` を使う（`metadata()` はリンク先を辿る）
- **Win32 の「サイズ取得 → バッファ充填」2回呼び出しパターン**（`ExpandEnvironmentStringsW` 等）では、2回目の戻り値（書込長）を必ず**バッファ長で clamp してからスライス**する（`written.min(buf.len())`）。値が2呼び出し間で伸びると戻り値 > バッファ長になり `buf[..written-1]` が境界外 panic、release は `Cargo.toml` で `panic="abort"` のためプロセス abort に化ける（#394）
- Tauri プラグインの新機能を使う際は `capabilities/*.json` の権限宣言を確認する
- `ShellExecuteW` でフォルダ・画像・文書ファイルを開く場合は COM STA が必要。Tauri コマンドハンドラスレッドは COM 状態が保証されないため、`std::thread::spawn` + `CoInitializeEx(None, COINIT_APARTMENTTHREADED)` + `ShellExecuteW` + `if com_ok { CoUninitialize() }` パターンで新規スレッドに COM 環境を用意する。`is_ok()` は S_OK(0) と S_FALSE(1) を両方 true とし、どちらも CoUninitialize が必要。EXE ファイルは COM 不要なため同問題を起こさない
- `SendInput` はシステム入力キューに注入し、ルーティングはキュー取り出し時に決定される。**フォーカス移行直後の `SendInput` は対象ウィンドウに届かない場合がある**（`SetForegroundWindow` は部分的に非同期）。`SendMessageTimeoutW(hwnd, WM_NULL, ..., SMTO_NORMAL, 100, ...)` でフォーカス完了を同期待ちしてから `SendInput` を呼ぶ（Raymond Chen 推奨パターン）
