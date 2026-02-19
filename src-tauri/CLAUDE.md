# src-tauri

Tauri v2 バイナリ crate。Win32 API 統合とフロントエンドとの IPC を担当。

## モジュール構成

- `main.rs`: エントリポイント、Tauri セットアップ、イベントリスナー登録
- `commands.rs`: 15個の `#[tauri::command]`（検索/履歴/設定/アイコン/ウィンドウ位置）
- `state.rs`: `AppState` 定義（`Mutex<SearchEngine>`, `Mutex<HistoryStore>`, `Mutex<Config>`）
- `platform.rs`: Win32 メッセージループスレッド + トレイアイコン（Tauri イベント経由で通信）
- `hotkey.rs`: グローバルホットキー登録/解除
- `ime.rs`: IME 制御
- `icon.rs`: アイコンのオンデマンド抽出（`SHGetFileInfoW` → PNG → base64）、検索時に遅延ロードしキャッシュ永続化

## 実装パターン

- ホットキーは `RegisterHotKey` を `platform.rs` の Win32 メッセージループスレッドで処理し、`AppHandle.emit()` で Tauri イベントとして通知
- 設定ウィンドウは `WebviewWindowBuilder` で同一プロセス内の第2ウィンドウとして生成
- `commands.rs` は薄いラッパーに保ち、実処理は `snotra-core` に寄せる（KISS）

## Win32 メッセージ配送の注意

Shell のトレイコールバック (`uCallbackMessage`) は `SendMessage` で配送される場合があり、`GetMessageW` ループに到達しない。カスタムメッセージ (`WM_APP + N`) をウィンドウプロシージャ (`DefWindowProcW`) だけで処理すると消滅するため、`platform_default_wnd_proc` で検出して `PostThreadMessageW` でスレッドキューに再投入する設計にしている。
