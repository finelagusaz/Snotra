# WebView2 パフォーマンス最適化 — 実装計画 v3

## 概要

ランチャーは「大半の時間が非表示」という使用パターンを持つ。非表示中に WebView2 のレンダラーを中断し、メモリ・CPU 使用量を低減する。

## 検証で判明した制約（v1 → v2 → v3 の変更理由）

### 制約 1: COM インターフェースは `!Send + !Sync`

`ICoreWebView2_3` / `ICoreWebView2_19` は `IUnknown(NonNull<c_void>)` のニュータイプ。
`NonNull<T>` は `!Send + !Sync`。windows-core / webview2-com のどこにも
`unsafe impl Send/Sync` はない。

**結果**: Tauri のマネージドステート（`T: Send + Sync + 'static` 要求）に COM インターフェースを直接保存できない。

**対策**: COM 呼び出しはすべて `with_webview()` 経由で行う。`with_webview()` はメインスレッドにディスパッチするため、STA COM オブジェクトのスレッド安全性も保証される。

### 制約 2: TrySuspend は IsVisible=false が必須

> "The CoreWebView2Controller's IsVisible property must be false when the API is called.
> Otherwise, the API fails with HRESULT_FROM_WIN32(ERROR_INVALID_STATE)."

**対策**: TrySuspend は必ず `window.hide()` の**後**に呼ぶ。Tauri の `hide()` は内部で WebView2 コントローラの `put_IsVisible(false)` を呼ぶため、hide → TrySuspend の順序で制約を満たす。

### 制約 3: TrySuspend と MemoryUsageTargetLevel の混用禁止

> "It is not advisable to mix them."
> "TrySuspend automatically sets MemoryUsageTargetLevel to LOW;
> Resume automatically sets it to NORMAL."

**対策**: **TrySuspend / Resume のみ使用する**（MemoryUsageTargetLevel は使わない）。TrySuspend が自動的に MemoryUsageTargetLevel を Low に設定するため、同等の効果を得られる。

### 制約 4: `with_webview()` の同期性はコンテキスト依存

| 呼び出し元 | 動作 |
|---|---|
| setup フェーズ（メインスレッド） | **同期**（インライン実行） |
| イベントリスナー（`app.listen` コールバック） | **同期**（メインスレッドで実行されるため） |
| IPC コマンドハンドラ（tokio スレッドプール） | **非同期**（fire-and-forget、メインスレッドにディスパッチ） |
| `std::thread::spawn` 内 | **非同期**（fire-and-forget） |

### 制約 5: Suspend 中の emit 動作

Suspend 中はレンダラープロセスが中断される。`emit()` の API 呼び出し自体は成功するが、JS の `message` イベントハンドラはレンダラー復帰まで発火しない（メッセージはキューイングされ、Resume 後にバースト配信される）。

**対策**: show パスでは Resume → emit の順序を厳守。

### 制約 6: IPC 経由の suspend は IsVisible 制約を満たせない（v3 で追加）

フロントエンド起因の hide は `notifyMainHidden()` → `win.hide()` の順で発行される（MainApp.tsx:49-50, commands.ts:19-20, MainApp.tsx:264-265）。`notifyMainHidden` は IPC（tokio スレッド）で実行され、内部の `with_webview(TrySuspend)` はメインスレッドにディスパッチされる。一方 `win.hide()` も Tauri API（メインスレッドにディスパッチ）。

到達順序の問題:
```
tokio スレッド: with_webview(TrySuspend) をメインスレッドキューに投入
FE (WebView): win.hide() をメインスレッドキューに投入

メインスレッドキュー処理:
  1. TrySuspend → IsVisible=true（hide未処理）→ ERROR_INVALID_STATE で失敗
  2. hide() → IsVisible=false
```

**結果**: `notify_main_hidden` からの suspend は空振りする。

**対策**: `notify_main_hidden` では suspend を呼ばない。suspend はホットキートグル（メインスレッドで同期実行、hide → suspend の順序が保証される）のみに限定する。

## アーキテクチャ: `with_webview()` パターン

COM インターフェースを保存せず、必要な時に `with_webview()` で都度アクセスする。

```
[hide パス — ホットキートグルのみ]
  w.hide()           → メインスレッドで IsVisible=false + ウィンドウ非表示（同期）
  w.with_webview()   → メインスレッドで TrySuspend（同期）

[hide パス — フロントエンド起因（Escape/クリック/フォーカス喪失）]
  notifyMainHidden   → main_visible=false + emit（suspend なし）
  win.hide()         → メインスレッドで IsVisible=false

[show パス]
  w.with_webview()   → メインスレッドで Resume
  w.set_size()       → メインスレッドでサイズリセット
  w.show()           → メインスレッドで IsVisible=true + ウィンドウ表示
  app.emit()         → JS にイベント配信（レンダラー復帰後に処理）
```

### ホットキーリスナーのコンテキスト

`app.listen("hotkey-pressed")` のコールバックはメインスレッドで実行される。
→ `with_webview()` は同期実行 → hide/show の順序制約を自然に満たす。

**例外**: Alt キー待ちの場合、`std::thread::spawn` 内から `show_main_and_emit` が呼ばれる。
この場合 `with_webview()` は非同期だが、Tauri の内部ディスパッチキューで順序が保証される
（`with_webview` → `set_size` → `show` → ... はすべて同じキューに投入）。

### フロントエンド起因の hide で suspend が効かない影響

フロントエンド起因の hide（Escape、クリック起動、フォーカス喪失）では suspend が実行されない。しかし:
- これらの hide の直後にユーザーがホットキーで再表示 → 使用 → ホットキーで hide するケースが多い
- ホットキー hide 時に確実に suspend される
- 長時間放置のケースはホットキーでの hide/show サイクルを経由するため、最終的には suspend される

## 影響範囲

### 触るファイル

| ファイル | 変更内容 |
|---|---|
| `src-tauri/src/main.rs` | `suspend_webview` / `resume_webview` ヘルパー追加。ホットキー hide パスに suspend、`show_main_and_emit` に resume 追加 |
| `src-tauri/CLAUDE.md` | TrySuspend/Resume パターンを記録 |

### 触らないファイル

| ファイル | 理由 |
|---|---|
| `src-tauri/src/state.rs` | COM インターフェースを保存しない |
| `src-tauri/src/commands/system.rs` | IPC 経由の suspend は空振りするため追加しない（v3 で変更） |
| `src-tauri/Cargo.toml` | 追加 feature flag 不要 |
| `ui/src/**` | フロントエンド変更なし |
| `snotra-core/` | 純ロジック。WebView2 関連なし |

### 対称コードパス

**hide トリガー（全5経路）**:

| # | 経路 | 呼び出し元 | suspend |
|---|---|---|---|
| H1 | ホットキートグル | `main.rs:506-515`（メインスレッド） | ✅ hide → suspend（同期、IsVisible=false 保証） |
| H2 | Escape キー | FE `hideMainWindow()` → IPC `notify_main_hidden` | ❌ 空振りリスク（制約 6） |
| H3 | クリック/ダブルクリック起動 | FE `handleClickResult` → IPC `notify_main_hidden` | ❌ 同上 |
| H4 | フォーカス喪失 auto-hide | FE `hideMain()` → IPC `notify_main_hidden` | ❌ 同上 |
| H5 | スラッシュコマンド `/s` `/q` | FE `hideMainWindow()` → IPC `notify_main_hidden` | ❌ 同上 |

**show トリガー（1箇所）**:

| # | 経路 | 呼び出し元 | resume |
|---|---|---|---|
| S1 | `show_main_and_emit` | ホットキー / single-instance / show_on_startup | ✅ resume → show（順序保証） |

## 実装ステップ

### Step 1: suspend / resume ヘルパー関数の定義（`main.rs`）

```rust
/// Suspend the WebView2 renderer to reduce memory/CPU while hidden.
/// Must be called AFTER hide() (IsVisible=false required by WebView2).
/// Best-effort: silently ignored if WebView2 runtime is too old (< Edge 88)
/// or IsVisible is still true.
#[cfg(windows)]
fn suspend_webview(window: &tauri::WebviewWindow) {
    let _ = window.with_webview(|platform_webview| {
        use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_3;
        use webview2_com::TrySuspendCompletedHandler;
        use windows::core::Interface;

        let controller = platform_webview.controller();
        let Ok(webview) = (unsafe { controller.CoreWebView2() }) else { return };
        let Ok(webview3) = (unsafe { webview.cast::<ICoreWebView2_3>() }) else { return };

        let handler = TrySuspendCompletedHandler::create(Box::new(|_result, _is_successful| {
            Ok(())
        }));
        let _ = unsafe { webview3.TrySuspend(&handler) };
    });
}

/// Resume the WebView2 renderer before showing the window.
/// Best-effort: silently ignored if not suspended or runtime too old.
#[cfg(windows)]
fn resume_webview(window: &tauri::WebviewWindow) {
    let _ = window.with_webview(|platform_webview| {
        use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_3;
        use windows::core::Interface;

        let controller = platform_webview.controller();
        let Ok(webview) = (unsafe { controller.CoreWebView2() }) else { return };
        let Ok(webview3) = (unsafe { webview.cast::<ICoreWebView2_3>() }) else { return };

        let _ = unsafe { webview3.Resume() };
    });
}

#[cfg(not(windows))]
fn suspend_webview(_window: &tauri::WebviewWindow) {}

#[cfg(not(windows))]
fn resume_webview(_window: &tauri::WebviewWindow) {}
```

**設計判断**:
- `cast()` 失敗（古いランタイム）→ 早期リターン。最適化スキップ、従来通り動作
- `TrySuspend` コールバックは空。ベストエフォートなので結果を使わない
- 関数ごとに `cast()` を呼ぶオーバーヘッドは `QueryInterface` 1回（~μs）。hide/show は秒単位の操作なので無視できる
- `#[cfg(windows)]` ガードで非 Windows ビルド時はノーオペレーション

### Step 2: ホットキー hide パスに suspend を追加（`main.rs:506-515`）

```rust
if visible && toggle {
    if let Some(w) = handle_for_hotkey.get_webview_window("main") {
        let _ = w.hide();                    // IsVisible=false（同期）
        suspend_webview(&w);                 // TrySuspend（同期、IsVisible=false 保証）
    }
    if let Some(state) = handle_for_hotkey.try_state::<AppState>() {
        state.main_visible.store(false, Ordering::SeqCst);
    }
    let _ = handle_for_hotkey.emit("window-hidden", ());
}
```

**順序**: `hide()` → `suspend_webview()` → visibility 更新 → emit。
ホットキーリスナーはメインスレッドで実行されるため、hide も with_webview も同期。

### Step 3: show パスに resume を追加（`main.rs` `show_main_and_emit`）

```rust
fn show_main_and_emit(app_handle: &AppHandle, ime_control: bool) {
    let t0 = Instant::now();
    let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;

    if let Some(main) = app_handle.get_webview_window("main") {
        // Resume WebView2 renderer before any window operations.
        // Must precede show() and emit() so the renderer can process
        // DOM updates and receive events.
        resume_webview(&main);

        trace_main("show_main:start", json!({ "ms": ms(t0.elapsed()) }));
        // ... 既存処理（set_size → position → show → emit）...
    }
}
```

**順序**: `resume_webview()` → `set_size()` → `position()` → `show()` → `emit()`。

Resume は同期 API。ドキュメント:
> "The app can interact with the WebView immediately after Resume."

`with_webview()` のコンテキスト別動作:
- ホットキーリスナー（メインスレッド）→ 同期実行 → Resume 完了後に show
- Alt 待ちスレッド（spawned thread）→ 非同期ディスパッチ → メインスレッドのキューで resume → set_size → show の順序が保証される

### Step 4: CLAUDE.md 更新

`src-tauri/CLAUDE.md` に以下を追記:
- WebView2 TrySuspend/Resume パターン（hide 時 suspend、show 時 resume）
- `with_webview()` の同期/非同期動作の注意事項
- IPC 経由での suspend が空振りする理由と、ホットキートグル限定の設計判断

### Step 5: 検証

1. `cargo check -p snotra -p snotra-core -p snotra-settings`（コンパイル確認）
2. 動作確認:
   - ホットキーで hide → タスクマネージャーで WebView2 レンダラープロセスの CPU が 0% に落ちるか
   - ホットキーで show → 検索バーにフォーカスが当たり、入力・検索が正常動作するか
   - Escape で hide → show → 正常動作するか
   - hide → 30秒待機 → show → 検索応答性が劣化しないか
   - hide → すぐ show（100ms 以内）→ 正常動作するか（TrySuspend 完了前の Resume）
3. メモリ計測: タスクマネージャーで非表示時のメモリ使用量を before/after 比較

## レースコンディション分析（v3 で追加）

### S1: 高速ホットキートグル（hide → 即 show）

メインスレッドで同期実行。hide → TrySuspend → (次の hotkey) → Resume → show。全操作が FIFO。**安全。**

### S2: Escape hide → 即ホットキー show

IPC の `notify_main_hidden` は suspend を呼ばない（v3 で削除）。ホットキー show は Resume を呼ぶが、suspend されていないので Resume は no-op。**安全。**

### S3: Alt 待ちスレッドの show ↔ ホットキー hide

既存のレースコンディション（suspend/resume 追加前から存在）。suspend/resume の追加で悪化しない:
- 仮に suspend が成功した状態で show が来ても、show 時の Resume で正常復帰
- 仮に show 後に suspend が来ても、IsVisible=true で TrySuspend が失敗 → 無視

**既存のレース。変更による影響なし。**

## リスクと対策

| リスク | 深刻度 | 対策 |
|---|---|---|
| TrySuspend 完了前に Resume が呼ばれる（hide→即show） | 低 | Resume は TrySuspend を事実上キャンセル。IsSuspended=false のまま。正常動作 |
| Suspend 中に emit が届かない | 低 | show パスで Resume → emit の順。メッセージはキューイングされ Resume 後に配信 |
| Resume のレイテンシ | 低 | 同期 API。「immediately after Resume」とドキュメントに明記 |
| 古い WebView2 ランタイム（Edge < 88） | 低 | `cast()` 失敗 → 早期リターン → 従来通り動作 |
| DevTools 開いている状態で TrySuspend | なし | suspend しない（isSuccessful=false）。ベストエフォート |
| FE hide 経路で suspend が効かない | 低 | 次のホットキー hide サイクルで確実に suspend。長時間非表示の大半はホットキー経由 |

## 将来の拡展

- ホットキー hide 時に即 suspend ではなく、500ms 遅延で suspend する（hide → 即 show の高速トグルでの不要な suspend/resume サイクルを回避）
- `SNOTRA_TRACE` 有効時に `IsSuspended()` でログ出力（デバッグ用）
- FE hide 経路でも suspend を効かせたい場合: フロントエンド側で `await win.hide()` の完了後に専用 IPC（`suspend_webview` コマンド）を呼ぶ設計に変更
