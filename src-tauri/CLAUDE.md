# src-tauri

Tauri v2 バイナリ crate。Win32 API 統合とフロントエンドとの IPC を担当。

## モジュール構成

- `main.rs`: エントリポイント、Tauri セットアップ、イベントリスナー登録
- `state.rs`: `AppState` 定義（`Mutex<Engine>` + `AtomicBool` × 3: `indexing` / `index_build_started` / `main_visible`）。`Engine` は `snotra-core` の facade で、検索・履歴・設定を単一ロックに統合。`main_visible` は Win32 `is_visible()` の 35ms レイテンシを回避するためのキャッシュ
- `icon.rs`: アイコンのオンデマンド抽出（`SHGetFileInfoW` → PNG → base64）、検索時に遅延ロードしキャッシュ永続化
- `indexing.rs`: バックグラウンドインデックス構築
- `config_watcher.rs`: `notify` クレートで `config.toml` 変更を監視（100ms debounce）し、差分検出後にホットキー・トレイ・インデックス・テーマ・ウィンドウ幅・言語を反映する `apply_config_change()` を実行。**不変条件: 言語変更とホットキー変更が同時に発生した場合、`language-changed` イベントをホットキー失敗通知より先に発火する**（フロントエンドが正しい言語でエラー文字列を組み立てるため）。発火するイベント: `language-changed` / `hotkey-registration-failed` / `visual-config-changed` / `show-icons-changed` / `max-results-changed` / `instant-prefix-changed` / `indexing-complete`（indexing.rs から）
- `ime.rs`: IME オフ操作（`ImmSetOpenStatus(false)`）。Win32 IMM API の薄いラッパー
- `monitor.rs`: マルチモニター対応の Win32 ヘルパー（`GetCursorPos` / `MonitorFromPoint` / `MonitorFromWindow` / `GetMonitorInfoW`）。物理座標ベースで作業領域を取得し、ウィンドウ位置のクランプ・中央配置を提供
- `commands/`: ディレクトリモジュール（`mod.rs` + `search.rs` / `launch.rs` / `config.rs` / `icon.rs` / `window.rs` / `system.rs` / `instant.rs`）。`#[tauri::command]` を責務別に分割。`launch.rs` は `launch_item_core`（`pub(crate)`、`instant.rs` から再利用）に加え、トレイメニューからの起動用に `launch_item_with_state` / `launch_with_tool_with_state` / `launch_default_with_state` / `resolve_all_openers` を `pub(crate)` で公開
- `platform/`: ディレクトリモジュール（`mod.rs` + `hotkey.rs` / `tray.rs` / `wndproc.rs`）。Win32 メッセージループスレッド + トレイアイコン + ホットキー + ウィンドウプロシージャ
  - `wndproc.rs`: `SendMessage` 経由で届く `WM_TRAY_ICON` および `WM_CONTEXTMENU` を `PostThreadMessageW` でスレッドキューに再投入し、メッセージループでの統一処理を保証

## 実装パターン

- ホットキーは `RegisterHotKey` を `platform/` の Win32 メッセージループスレッドで処理し、`AppHandle.emit()` で Tauri イベントとして通知
- 設定ウィンドウは `WebviewWindowBuilder` で同一プロセス内の第2ウィンドウとして生成
- `commands/` は薄いラッパーに保ち、実処理は `snotra-core` に寄せる（KISS）
- `AppState` は `Mutex<Engine>` で検索エンジン・履歴・設定を一括管理。Phase 2.3 以前の 3重ロック（`Mutex<SearchEngine>` / `Mutex<HistoryStore>` / `Mutex<Config>`）は Engine facade に統合済み
- Managed state として `IconCacheState`（`Mutex<Option<IconCache>>`、初回アイコン要求で遅延初期化）と `SettingsProcessState`（`Mutex<Option<Child>>`、設定プロセスのハンドル管理）を保持
- **`show_main_and_emit` の操作順序制約**: 高さリセット（52px）→ `position_on_target_monitor` → `show()` の順。位置計算はウィンドウサイズ（`outer_size()`）でクランプするため、高さリセット前に位置を決めると展開時の高さでクランプされ、折りたたみ時に位置がずれる

## Win32 メッセージ配送の注意

Shell のトレイコールバック (`uCallbackMessage`) は `SendMessage` で配送される場合があり、`GetMessageW` ループに到達しない。カスタムメッセージ (`WM_APP + N`) をウィンドウプロシージャ (`DefWindowProcW`) だけで処理すると消滅するため、`platform_default_wnd_proc` で検出して `PostThreadMessageW` でスレッドキューに再投入する設計にしている。

NOTIFYICON_VERSION_4 では、キーボード操作（Shift+F10 / Application キー）によるコンテキストメニュー要求は `uCallbackMessage` を経由せずウィンドウプロシージャに直接 `WM_CONTEXTMENU` として届く。`platform_default_wnd_proc` で同様に再投入することで `handle_tray_message` に統一している。

**Win32 メッセージハンドラを削除・変更する前に「そのメッセージが届く全経路」を列挙すること。** 同一メッセージでも発火源が複数ある場合がある（例: `WM_CONTEXTMENU` はマウス右クリック環境とキーボード操作の両経路で届く）。「問題の原因になっている経路」だけを削除しようとすると、問題でない別の経路も同時に消える。

## WebView2 ウィンドウ生成の制約

`WebviewWindowBuilder::build()` は WebView2 初期化のために Win32 メッセージポンプの進行を必要とする。**「メインスレッドにいる」と「メッセージポンプが自由に回る」は別物**であり、以下の制約がある:

- **setup フェーズ（イベントループ開始前）**: `build()` が自前でメッセージを処理できるため正常動作する
- **イベントループ中のコールバック（`run_on_main_thread` / `listen` / `RunEvent` 等）**: メッセージポンプが1イテレーション内で停止しているため、`build()` がポンプ進行を待ってデッドロックする
- **IPC ハンドラスレッド**: メインスレッドではないため同様にデッドロックする

このため、ウィンドウの生成は必ず setup フェーズで行い、ランタイムでは show/hide のみで制御する。`CloseRequested` では `api.prevent_close()` で破棄を防ぎ hide に変換する（WebView2 はイベントループ開始後に再生成できない）。

**事前チェック**: ある操作が「内部でメッセージポンプの進行を必要とするか」を確認する。ウィンドウ生成・COM STA 初期化・モーダルダイアログ等は該当し、イベントループコールバック内から呼べない。

## Win32 / Tauri 注意事項

- Win32 関連の不具合では、まず `config.toml`（テーマ含む）を確認し、次にウィンドウライフサイクル順序、最後に API 呼び出しを調査する（白画面バグの真因がテーマ設定だった事例あり）
- Rust クレートをバージョン昇格する際は、対象バージョンが crates.io に実在・正当であることを確認する。大版ジャンプを前提にしない（例: `bincode 3.0.0` は `compile_error!` のみを含むジョークパッケージでコンパイル不能）
- `windows` クレート（現在 v0.62）はバージョンごとに API シグネチャが変わる（`Result` 型の有無、ハンドル型の変更など）。コードを書く前に、使用中のバージョンで対象 API が利用可能か・型が一致するかを確認する
- 必要な feature フラグ（`Win32_UI_WindowsAndMessaging` 等）が `Cargo.toml` に宣言されているか確認してから実装する
- `UpdateWindow` など一部 API は windows クレートのバージョンによっては未提供。代替 API（`RedrawWindow` 等）の存在を事前に調べる
- Windows パスの正規化では `C:` と `C:\` の違いに注意する（ドライブルートは末尾 `\` が必須）
- ファイルメタデータ取得時、シンボリックリンクを考慮する場合は `symlink_metadata` を使う（`metadata()` はリンク先を辿る）
- Tauri プラグインの新機能を使う際は `capabilities/*.json` の権限宣言を確認する
- `tauri.conf.json` の CSP で特定ディレクティブ（`connect-src` 等）を明示すると、そのディレクティブは `default-src` を継承しなくなる。`'self'` が必要な場合は明示的に含めること。また `tauri dev` では CSP が適用されないため、CSP 起因の問題はリリースビルドでしか再現しない
- `ShellExecuteW` でフォルダ・画像・文書ファイルを開く場合は COM STA が必要。Tauri コマンドハンドラスレッドは COM 状態が保証されないため、`std::thread::spawn` + `CoInitializeEx(None, COINIT_APARTMENTTHREADED)` + `ShellExecuteW` + `if com_ok { CoUninitialize() }` パターンで新規スレッドに COM 環境を用意する。`is_ok()` は S_OK(0) と S_FALSE(1) を両方 true とし、どちらも CoUninitialize が必要。EXE ファイルは COM 不要なため同問題を起こさない
- `with_webview()` → `PlatformWebview::controller()` で `ICoreWebView2Controller` にアクセスし、WebView2 COM API を直接呼べる。**setup フェーズでのみ安全**（イベントループ中はデッドロック）。`webview2_com` クレートが必要。現在 `AcceleratorKeyPressed` ハンドラを登録し、`WM_SYSKEYDOWN`（Alt+char）を `SetHandled(true)` で消費してビープ音を防止している
- `SendInput` はシステム入力キューに注入し、ルーティングはキュー取り出し時に決定される。**フォーカス移行直後の `SendInput` は対象ウィンドウに届かない場合がある**（`SetForegroundWindow` は部分的に非同期）。`SendMessageTimeoutW(hwnd, WM_NULL, ..., SMTO_NORMAL, 100, ...)` でフォーカス完了を同期待ちしてから `SendInput` を呼ぶ（Raymond Chen 推奨パターン）
- JS の `preventDefault()` は Chromium レンダラプロセスの IPC 経由で動作するため、**ネイティブ HWND レベルの `DefWindowProc` 呼び出し（`MessageBeep` 等）を阻止できない**。ネイティブ側で阻止する必要がある場合は `AcceleratorKeyPressed` や HWND サブクラスを使う
