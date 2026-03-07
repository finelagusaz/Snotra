# Research: Issue #155 — 起動直後のホットキー表示で検索フレーズの先頭が飛ぶ

## Issue の要約

アプリ起動直後に Alt+Q でウィンドウを呼び出すと、最初の数文字が入力されない、または Windows の警告音（ビープ）が鳴る。先頭の文字が「飛ぶ」現象。

## 再現条件

1. アプリ起動
2. Alt+Q で呼び出し
3. すぐに文字を入力
4. 先頭文字が欠落 / ビープ音

## 通知音の特定

- **ファイル**: `C:\Windows\media\Windows Background.wav`
- **トリガー**: `MessageBeep(0)` = レジストリ `.Default` サウンド
- **呼び出し経路**: `WM_SYSCHAR` → `DefWindowProc` → `MessageBeep(0)`
- `WM_SYSCHAR` は Alt 修飾が残った状態で文字キーが押されたとき、`TranslateMessage` が `WM_SYSKEYDOWN` から合成する

## イベント処理順序（完全版）

```
時刻  イベント                              備考
-----------------------------------------------------------------------
T0    WM_HOTKEY 受信 (platform thread)      RegisterHotKey が発火
T1    emit("hotkey-pressed")                Tauri イベントで main thread へ
T2    is_alt_pressed() チェック             GetAsyncKeyState(VK_MENU)
T3    wait_alt_release_or_timeout()         10ms ポーリング、最大 350ms (別スレッド)
T4    Alt 物理リリース                      ユーザーが指を離す
T5    main.show()                           ShowWindow(SW_SHOW) + SetIsVisible(true)
      +-- WebView2: SetIsVisible(true) → レンダラのスロットル解除開始
      +-- 最初のフレーム描画: まだ完了していない（コールドスタート時）
T6    main.set_focus()                      SetForegroundWindow + MoveFocus(PROGRAMMATIC)
      +-- OS: WM_SETFOCUS を送信（同一スレッド内は同期）
      +-- WebView2: フォーカス受入れ（レンダラ状態に依存）
      +-- ただし SetForegroundWindow は部分的に非同期（Raymond Chen）
T7    send_alt_key_up()                     SendInput → システム入力キューに注入
      +-- 5ms sleep
      +-- 配送先: システムキューからの取り出し時に決定（非同期）
T8    IME control                           PlatformCommand::TurnOffIme 送信（非同期）
T9    emit("window-shown")                  フロントエンドに通知
T10   focusInputWithRetries()               RAF 2回(~33ms) → input.focus() + 120ms/280ms リトライ
      +-- WebView2 レンダラ: コールドスタート時はまだ初回フレーム処理中の可能性
T11   ユーザーのキー入力到着
      +-- レンダラ準備完了 → 正常処理
      +-- レンダラ未準備 → WM_SYSKEYDOWN → WM_SYSCHAR → DefWindowProc → MessageBeep
```

## 根本原因の分析（3層構造）

### 層 1: SendInput のフォーカス非同期レース

**メカニズム**:
- `set_focus()` → `SetForegroundWindow` は**部分的に非同期**（Raymond Chen）
  - 入力キュー切替は即時だが、アクティベーション通知（WM_ACTIVATE/WM_SETFOCUS）は非同期
  - 特にクロススレッドでは、ターゲットスレッドがメッセージをポンプするまで完了しない
- `SendInput` はシステム入力キューに注入し、**ルーティングはキュー取り出し時**に決定
- つまり `set_focus()` → `SendInput()` の間にレースが存在する

**証拠**:
- Raymond Chen: 「the window is becoming the foreground window, but it is not necessarily the foreground window yet」
- comp.os.ms-windows.programmer.win32: 「it takes some time for Windows to process the SetForegroundWindow request, and some keystrokes sent by SendInput are lost」
- PowerToys PR #1282: Microsoft 自身が「SendInput hack to workaround the SetForegroundWindow bug」と認識

**対策案**: `set_focus()` 後に `SendMessageTimeout(hwnd, WM_NULL, ...)` でフォーカス完了を同期待ち

### 層 2: WebView2 コールドスタートのレンダラ遅延（最重要）

**メカニズム**:
- ウィンドウは `visible: false` で作成 → WebView2 は `SetIsVisible(false)` → **Chromium レンダラがスロットル状態**
  - アニメーション停止、JS タイマー 1秒間隔に制限、レンダリング停止
- 初回 `show()` → `SetIsVisible(true)` → レンダラのアンスロットル開始
  - しかし**アンスロットル → 最初のフレーム描画完了**に 16〜48ms+ かかる
  - スタイル → レイアウト → プリペイント → ペイント → コンポジット → ディスプレイ
- `set_focus()` → `MoveFocus(PROGRAMMATIC)` は OS レベルでフォーカスを設定するが、
  **Chromium レンダラの Blink メインスレッドがまだ初回レイアウト中**の場合がある
- `MoveFocus` を `NavigationCompleted` 前に呼ぶと `COMException (0x8007139F)` が発生する報告あり

**証拠**:
- Microsoft: 「When IsVisible is false, the WebView is transparent and is not rendered」
- WebView2Feedback #3070: 「Chromium throttles down when Visibility.Collapsed」
- Rick Strahl: 初期化時に一瞬可視化してレンダラをプリウォームする手法を報告
- 既存コード: `61ms + 71ms overhead on first invocation` のコメント（main.rs）
- 既存コード: 2フレーム遅延 + 120ms/280ms リトライ = フォーカスが一発で確立しない前提の設計

**wry の実装**（WebView2 バインディング）:
```rust
// show() の実装
pub fn set_visible(&self, visible: bool) -> Result<()> {
    let _ = ShowWindow(self.hwnd, match visible { true => SW_SHOW, false => SW_HIDE });
    self.controller.SetIsVisible(visible)?;  // レンダラ状態変更
    Ok(())
}

// focus() の実装
pub fn focus(&self) -> Result<()> {
    self.controller.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC)
}
```
- `set_visible` と `focus` は**独立した非同期操作で同期機構がない**
- `show()` はレンダラのフレーム描画完了を**待たない**

### 層 3: WM_SYSCHAR のネイティブ HWND レベル処理

**メカニズム**:
- `WM_SYSCHAR` → `DefWindowProc` → `MessageBeep(0)` は**ネイティブ HWND のウィンドウプロシージャ**で処理
- JS の `preventDefault()` は Chromium レンダラプロセスの IPC 経由で返答するため、
  **ネイティブ側の `DefWindowProc` 呼び出しより後**に到着する
- つまり JS 側の Alt ガードは**音の防止には効果がない**（文字入力の制御には有効）

**証拠**:
- WebView2 の HWND サブクラスは現在のコードベースに**存在しない**
- `SearchWindow.tsx` の `e.altKey && e.key.length === 1` ガードは存在するが、
  これは JS レベルであり `MessageBeep` はネイティブ側で既に鳴っている

## 修正候補の評価

### A. SendMessageTimeout(WM_NULL) でフォーカス同期

```rust
// set_focus() 後に追加
use windows::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, WM_NULL, SMTO_NORMAL};
if let Ok(hwnd) = main.hwnd() {
    let hwnd = HWND(hwnd.0);
    unsafe {
        let mut result = 0usize;
        SendMessageTimeoutW(hwnd, WM_NULL, WPARAM(0), LPARAM(0),
            SMTO_NORMAL, 100, Some(&mut result));
    }
}
```

- 効果: フォーカス移行完了を保証してから SendInput を実行
- リスク: 低。WM_NULL は副作用なし
- 限界: 層 2（レンダラ遅延）は解決しない

### B. WebView2 HWND サブクラスで WM_SYSCHAR を握り潰す

```rust
// WebView2 の HWND をサブクラスし、WM_SYSCHAR を DefWindowProc に渡さない
SetWindowSubclass(webview_hwnd, subclass_proc, ...);
// subclass_proc で WM_SYSCHAR を LRESULT(0) で返す
```

- 効果: MessageBeep を根本的に阻止
- リスク: 中。WebView2 の内部 HWND 階層を理解する必要がある
- 限界: 音は止まるが、文字入力の欠落は解決しない

### C. WebView2 レンダラのプリウォーム

```rust
// setup フェーズで一瞬可視化
let _ = main.show();
std::thread::sleep(std::time::Duration::from_millis(100));
let _ = main.hide();
```

- 効果: コールドスタート時のレンダラ遅延を解消
- リスク: 低〜中。起動時に一瞬ウィンドウが見える可能性（`SW_SHOWNA` で回避可能か要検証）
- 限界: 起動時間が増加。レンダラが hide で再スロットルされる可能性

### D. send_alt_key_up の sleep 延長 (5ms → 50ms)

- 効果: 暫定的にレースウィンドウを縮小
- リスク: 低。ただし体感遅延に影響
- 限界: 環境依存。保証にならない

### E. 複合アプローチ（推奨）

1. **A + B の組み合わせ**: フォーカス同期 + WM_SYSCHAR インターセプト
   - フォーカス同期で SendInput の配送先を保証
   - WM_SYSCHAR インターセプトで音を根本阻止
2. **C は検証後に判断**: プリウォームの副作用（ちらつき、スロットル再開）を確認してから

## 技術的制約

- `SendInput` / `keybd_event` で Alt key-up を合成すると、他のアプリのフック処理と干渉する可能性がある
- `SetForegroundWindow` → `SendInput` のレースは Windows OS の構造的制約
- WebView2 の `IsVisible=false` スロットルは Chromium の設計上の動作で回避困難
- WebView2 HWND サブクラスは wry/Tauri が管理する HWND を外部から操作するためバージョン依存リスクあり
- JS 側の `preventDefault()` は Chromium IPC 経由のため、ネイティブ側の `DefWindowProc` 処理を阻止できない

## 参考資料

- Raymond Chen: [SetForegroundWindow immediately followed by GetForegroundWindow](https://devblogs.microsoft.com/oldnewthing/20161118-00/?p=94745)
- Raymond Chen: [Sharing an input queue takes what used to be asynchronous and makes it synchronous](https://devblogs.microsoft.com/oldnewthing/20130607-00/?p=4143)
- Raymond Chen: [You can't simulate keyboard input with PostMessage](https://devblogs.microsoft.com/oldnewthing/20250319-00/?p=110979)
- [SendInput function (MSDN)](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)
- [SetForegroundWindow function (MSDN)](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow)
- [WebView2 ICoreWebView2Controller::SetIsVisible (MSDN)](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2controller)
- [WebView2 Performance best practices (MSDN)](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/performance)
- [WebView2Feedback #3070: Chromium throttles down when Visibility.Collapsed](https://github.com/MicrosoftEdge/WebView2Feedback/issues/3070)
- [Rick Strahl: Fighting WebView2 Visibility on Initialization](https://weblog.west-wind.com/posts/2022/Jul/14/Fighting-WebView2-Visibility-on-Initialization)
- [PowerToys PR #1282: SendInput hack to workaround the SetForegroundWindow bug](https://github.com/microsoft/PowerToys/pull/1282)
- [comp.os.ms-windows.programmer.win32: SetForegroundWindow/SendInput timing problems](https://comp.os.ms-windows.programmer.win32.narkive.com/rYZnuS6o/setforegroundwindow-sendinput-timing-problems)
