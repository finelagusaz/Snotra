# src-tauri

Tauri v2 バイナリ crate。Win32 API 統合とフロントエンドとの IPC を担当。

## モジュール構成

- `main.rs`: エントリポイント、Tauri セットアップ、イベントリスナー登録。起動時の背景再スキャン（`indexer::load_or_scan_with_stats` が返す `BackgroundRescanTask`）を `setup` フェーズで低優先度スレッドに spawn し、`RescanOutcome::Changed` なら `icon::invalidate_icon_cache` を呼ぶ
- `state.rs`: `AppState` 定義（`Mutex<Engine>` + `AtomicBool` × 3: `indexing` / `index_build_started` / `main_visible`）。`Engine` は `snotra-core` の facade で、検索・履歴・設定を単一ロックに統合。`main_visible` は Win32 `is_visible()` の 35ms レイテンシを回避するためのキャッシュ。インデックスビルドの開始/終了は `try_begin_index_build()` / `finish_index_build()` メソッド経由で行い、`indexing`・`index_build_started` を coherent に更新する。**config 変更→index 再構築のコヒーレンシ判断は engine の `index_stale` ledger（軸1）に閉じており、この 2 AtomicBool は二重ビルド防止（CAS）と UI 表示専用に純化されている**（#347/#348-A）
- `icon.rs`: アイコンのオンデマンド抽出（`SHGetFileInfoW` → PNG バイト列）、検索時に遅延ロードしキャッシュ永続化。`invalidate_icon_cache` はメモリ内 `IconCacheState` と `icons.bin` を**両方**無効化する（片方だけだと終了時の `save_if_dirty` で古いアイコンが復活する）
- `indexing.rs`: バックグラウンドインデックス構築。`start_index_build` は `mark_index_stale`（**CAS の前**）→ CAS → spawn で **drain ループ**（`begin_index_drain` で現在 config の `IndexInputs` snapshot → ロック外で `rebuild_and_save` / `PrebuiltIndex::new` → `complete_index_drain` で swap + re-diff）を stale が消えるまで回す。ビルド本体は `catch_unwind` で包む（**panic 戦略依存**: unwind ビルド=debug/test では panic を捕捉し `finish_index_build` で flag 固着 wedge を防ぐ。release は Cargo.toml で `panic="abort"` のため build panic はプロセス abort＝ここに来ないが silent wedge にもならず、再起動で fresh build される。どちらでも UI 永久構築中は起きない）。finish 後に `is_index_stale` を再チェックし、finish 窓で刺さった変更を再 kick で拾う（**unwind の panic 経路では再 kick しない**＝決定論 panic の無限リトライ回避）。config 変更→index 再構築のコヒーレンシは engine の `index_stale` ledger に一元化（#347/#348-A）
- `config_watcher.rs`: `notify` クレートで `config.toml` 変更を監視（100ms debounce）し、差分検出後にホットキー・トレイ・インデックス・テーマ・ウィンドウ幅・言語を反映する `apply_config_change()` を実行。**不変条件: 言語変更とホットキー変更が同時に発生した場合、`language-changed` イベントをホットキー失敗通知より先に発火する**（フロントエンドが正しい言語でエラー文字列を組み立てるため）。**不変条件: `LoadOutcome::ReadFailed`（一時的・環境的な read 失敗）では `apply_config_change` は何も適用せず早期 return する**（`should_apply_config_change()` で判定。fallback-default を実行中エンジンへ適用すると、live-read 化した履歴剪定が `history.bin` をデータ損失させ、index 再構築判定（`IndexInputs` 差分）が default scan で誤再構築を起こすため。`Config::load` の「一時的失敗は退避も上書きもしない」保全を適用側にも揃える、#348）。ただし早期 return の前に短いバウンドリトライ（`load_with_read_failed_retry`、既定 3 回 × 150ms）で一時的ロック解除を待ち、解ければ正規の変更を取りこぼさず適用する（予算超過時のみ skip。リトライ中は適用しないのでデータ損失安全は不変）。**不変条件: index 再構築の要否は `IndexInputs::from_config(old) != IndexInputs::from_config(new)` で判定し、ビルド進行中（`indexing`）でも `!indexing` ゲートなしで常に `start_index_build` を kick する**（`start_index_build` が `mark_index_stale` で stale を立て、in-flight ビルドの drain / finish 後再チェックが取りこぼしを拾う。CAS が二重起動を防ぐ。#347/#348-A）。発火するイベント: `language-changed` / `hotkey-registration-failed` / `visual-config-changed` / `show-icons-changed` / `max-results-changed` / `instant-prefix-changed` / `top-n-history-changed` / `indexing-started`（indexing.rs から）/ `indexing-complete`（indexing.rs から）
- `ime.rs`: IME オフ操作（`ImmSetOpenStatus(false)`）。Win32 IMM API の薄いラッパー
- `monitor.rs`: マルチモニター対応の Win32 ヘルパー（`GetCursorPos` / `MonitorFromPoint` / `MonitorFromWindow` / `GetMonitorInfoW`）。物理座標ベースで作業領域を取得し、ウィンドウ位置のクランプ・中央配置を提供
- `working_set.rs`: 非表示アイドル時に Win32 `EmptyWorkingSet` でプロセスツリー全体（自プロセス + WebView2 子孫）の物理 working set を回収（Windows のみ、非 Windows は no-op）。`collect_descendant_pids()` は Toolhelp の (pid,ppid) 上を BFS する純関数（Win32 非依存・ユニットテスト対象）。`trim_idle_working_set()` は hide 経路（hotkey + `notify_main_hidden`）から呼ばれる best-effort 操作で、HANDLE は RAII ガードで解放し、全 Win32 失敗を握りつぶす
- `commands/`: ディレクトリモジュール（`mod.rs` + `search.rs` / `launch.rs` / `config.rs` / `icon.rs` / `window.rs` / `system.rs` / `instant.rs`）。`#[tauri::command]` を責務別に分割。`launch.rs` は `launch_item_core`（`pub(crate)`、`instant.rs` から再利用）に加え、トレイメニューからの起動用に `launch_item_with_state` / `launch_with_tool_with_state` / `launch_default_with_state` / `resolve_all_openers` を `pub` で公開
- `platform/`: ディレクトリモジュール（`mod.rs` + `hotkey.rs` / `tray.rs` / `wndproc.rs`）。Win32 メッセージループスレッド + トレイアイコン + ホットキー + ウィンドウプロシージャ
  - `wndproc.rs`: `SendMessage` 経由で届く `WM_TRAY_ICON` および `WM_CONTEXTMENU` を `PostThreadMessageW` でスレッドキューに再投入し、メッセージループでの統一処理を保証

## 実装パターン

- ホットキーは `RegisterHotKey` を `platform/` の Win32 メッセージループスレッドで処理し、`AppHandle.emit()` で Tauri イベントとして通知
- 設定画面は `snotra-settings.exe`（egui 別バイナリ）を子プロセスとして起動。`SettingsProcessState` で重複起動を防止し、子プロセス存命中はメインウィンドウの `alwaysOnTop` を一時解除する
- `commands/` は薄いラッパーに保ち、実処理は `snotra-core` に寄せる（KISS）
- `AppState` は `Mutex<Engine>` で検索エンジン・履歴・設定を一括管理。Phase 2.3 以前の 3重ロック（`Mutex<SearchEngine>` / `Mutex<HistoryStore>` / `Mutex<Config>`）は Engine facade に統合済み
- **インデックスビルドのフラグは `AppState` のメソッド経由で更新する**: `try_begin_index_build()`（`index_build_started` を CAS 取得 → `indexing` を立てる）と `finish_index_build()`（両方を戻す）が唯一の正しい経路。`indexing` / `index_build_started` を直接 `store()` しない——外部からの force-reset は走行中ビルドのガードを踏み倒す競合の原因になる。2フラグは別物（`index_build_started` は CAS 専用ガード、`indexing` は first-run 時にビルドスレッド不在でも true になる UI 表示用）
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

このため、ウィンドウの生成は必ず setup フェーズで行い、ランタイムでは show/hide のみで制御する。メインウィンドウは `decorations: false` で閉じるボタンを持たないため `CloseRequested` ハンドラは不要。非表示化はフロントエンド側の `win.hide()`（フォーカス喪失時・Escape 時等）で行う。

**事前チェック**: ある操作が「内部でメッセージポンプの進行を必要とするか」を確認する。ウィンドウ生成・COM STA 初期化・モーダルダイアログ等は該当し、イベントループコールバック内から呼べない。

## WebView2 TrySuspend / Resume パターン

非表示中に WebView2 レンダラーを中断してメモリ・CPU を削減する。`ICoreWebView2_3::TrySuspend` / `Resume`（Edge 88+）を使用。

- **hide 時（ホットキートグルのみ）**: `w.hide()` → `emit("window-hidden")` → `suspend_webview(&w)`。emit を suspend より先に送ることで、JS 側のクリーンアップ（Blob URL 解放等）がレンダラー中断前にキューイングされる。`TrySuspend` は `IsVisible=false` を要求するため hide が先
- **show 時**: `resume_webview(&main)` → `set_size` → `show` → `emit`。Resume は同期 API で即座に復帰
- **フロントエンド起因の hide（Escape / クリック起動 / フォーカス喪失）では suspend しない**: `notifyMainHidden` IPC は tokio スレッドで実行されるため `with_webview(TrySuspend)` は非同期ディスパッチになり、`win.hide()` より先にメインスレッドに到達すると IsVisible=true で失敗する。ホットキートグル（メインスレッドで同期実行）に限定することで順序を保証
- **`with_webview()` の同期性はコンテキスト依存**: setup フェーズ / `app.listen` コールバック → 同期。IPC ハンドラ / `std::thread::spawn` → 非同期（fire-and-forget）
- **TrySuspend と MemoryUsageTargetLevel は混用禁止**: TrySuspend が自動で MemoryUsageTargetLevel を Low に設定し、Resume が Normal に戻す

## WebView2 working set の能動回収（EmptyWorkingSet）

TrySuspend / MemoryUsageTargetLevel.Low は**論理目標**を下げるだけで、メモリ圧迫のない環境では OS が**物理 working set を回収しない**（実測: 非表示アイドル ~110MB が表示↔非表示・120 秒放置でも不変）。`working_set::trim_idle_working_set()` が hide 経路で Win32 `EmptyWorkingSet` をプロセスツリー全体（自プロセス + WebView2 子孫）へ能動適用し、アイドル物理 RSS を数MB まで落とす（再表示は OS の透過 re-fault で ~44ms 維持、UI 正常）。

- **TrySuspend とは別レイヤーで補完的**: TrySuspend=論理目標（圧迫待ち・CPU 中断）、EmptyWorkingSet=物理 working set の即時トリミング。競合しない
- **全 hide 経路に適用**: hotkey トグル（`main.rs`、`suspend_webview` の後）と `notify_main_hidden`（`commands/system.rs`、全フロントエンド hide の IPC チョークポイント）の両方から呼ぶ。**`EmptyWorkingSet` はスレッド非依存**（`with_webview` のような非同期制約がない）ため、tokio IPC スレッドの `notify_main_hidden` からも安全
- **show 側に逆操作は不要**: trim されたページは show 時に OS が透過的に re-fault する。明示 untrim API は存在しない。trim が hide 前後どちらで走っても無害（再 fault するだけ）
- **best-effort・物理 RAM のみ**: Toolhelp / `OpenProcess` / `EmptyWorkingSet` の全失敗は黙ってスキップ（機能影響ゼロ）。HANDLE は RAII ガードで解放。削減対象は working set であって commit ではない

## Win32 / Tauri 注意事項

- Win32 関連の不具合では、まず `config.toml`（テーマ含む）を確認し、次にウィンドウライフサイクル順序、最後に API 呼び出しを調査する（白画面バグの真因がテーマ設定だった事例あり）
- Rust クレートをバージョン昇格する際は、対象バージョンが crates.io に実在・正当であることを確認する。大版ジャンプを前提にしない（例: `bincode 3.0.0` は `compile_error!` のみを含むジョークパッケージでコンパイル不能）
- `windows` クレート（現在 v0.62）はバージョンごとに API シグネチャが変わる（`Result` 型の有無、ハンドル型の変更など）。コードを書く前に、使用中のバージョンで対象 API が利用可能か・型が一致するかを確認する
- `webview2-com` は `windows-core 0.61` に依存するが、プロジェクトの `windows` クレート（v0.62）は `windows-core 0.62` を使う。`Interface::cast()` 等を呼ぶ際は `windows-core_0_61 = { package = "windows-core", version = "0.61" }` のエイリアス依存を使い、`use windows_core_0_61::Interface` とする
- WebView2 COM インターフェース（`ICoreWebView2_3` 等）は `!Send + !Sync`（内部が `NonNull<c_void>`）。Tauri managed state（`Send + Sync` 必須）に保持できない。`with_webview()` コールバック内でインラインにキャストして使う
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
