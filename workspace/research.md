# research: issue #159 — ホットキー登録失敗時のユーザー通知

## issue の要約

`RegisterHotKey` 失敗時にユーザーへ通知が届いていない。2ケースに分けて対処する。

| ケース | 現在 | 目標 |
|--------|------|------|
| 初回登録失敗 | ウィンドウを表示するだけ | + エラー通知をウィンドウ内に表示 |
| 設定変更で失敗 | `eprintln!` のみ | + 一時エラー通知をウィンドウ内に表示 |

## 関連コード

### Rust バックエンド

- `src-tauri/src/platform/mod.rs`
  - `PlatformCommand::RegisterInitialHotkey` ブランチ（line ~263）:
    - `hotkey::register(current_hotkey)` 失敗時に `app_handle.emit("platform-event", "initial-hotkey-failed")` を発火（既存）
    - `current_hotkey: HotkeyConfig` が同スコープで参照可能 → `modifier` / `key` フィールドで hotkey 文字列を構成できる
  - `PlatformCommand::SetHotkey` ブランチ（line ~230）:
    - 失敗時: `hotkey::register(current_hotkey)` で旧ホットキーに復帰し `reply.send(false)` するのみ
    - 通知イベントは発火していない（通知責務は呼び出し側の `config_watcher.rs` に委譲している）

- `src-tauri/src/config_watcher.rs`
  - `apply_config_change()` → `SetHotkey` → `rx.recv_timeout()` → `Ok(false) | Err(_)` ブランチ（line ~93）:
    - `eprintln!` のみで通知なし
    - `new_config.hotkey.modifier` / `new_config.hotkey.key` が同スコープで参照可能
    - `app: &AppHandle` が引数にある → `.emit()` 可能

### フロントエンド

- `ui/src/MainApp.tsx` (line 98-111):
  - `listen<string>("platform-event", ...)` で `initial-hotkey-failed` を受け取り、ウィンドウ表示のみ実施
  - 通知表示ロジックは未実装

- `ui/src/stores/search.ts`:
  - `launchNotice` シグナル（getter export）: SearchWindow の通知オーバーレイに表示される
  - `setLaunchNoticeWithAutoClear(msg, 2400ms)`: 内部関数、未 export
  - `clearLaunchNotice()`: export 済み
  - エラースタイルは `launchNotice() !== null && !launching()` 条件で自動適用（`indexing-message--error` クラス）

- `ui/src/components/SearchWindow.tsx` (line 296-303):
  - `<Show when={launching() || launchNotice()}>` で通知オーバーレイを表示
  - `classList={{ "indexing-message--error": !launching() && launchNotice() !== null }}`

- `ui/src/lib/i18n.ts`:
  - `TranslationKey` union 型 + `JA_JP` レコード
  - `{param}` プレースホルダー対応の `t(key, params?)` 関数

## 既存パターン

- **通知表示**: `setLaunchNoticeWithAutoClear(message)` → `launchNotice` シグナル → SearchWindow オーバーレイ。エラー文字列は i18n で管理。
- **フロントエンドからの通知消費**: `clearLaunchNotice` / `launchNotice` が export 済みで、store 外からも操作可能。
- **Tauri イベント型**: `listen<T>(event, handler)` で型付き受信。

## 技術的制約

- Win32 `RegisterHotKey` 失敗時、競合相手アプリ名の取得は不可能（issue 記載通り）。
- `platform-event` の既存ペイロード型は `string`。変更すると MainApp.tsx のリスナー型定義も合わせる必要がある。変更しないのが安全。
- `setLaunchNoticeWithAutoClear` は 2400ms タイムアウト固定。ホットキーエラーメッセージは長文のため、5000ms 程度が望ましい。
- `search.ts` のモジュールスコープ変数（`launchNotice`, `launchNoticeTimer`）は SearchWindow と同一 WebviewWindow（`main`）でのみ有効。

## 未解決の疑問

なし。調査で設計判断に必要な情報はすべて揃った。
