# Plan: Issue #155 — 起動直後のホットキー表示でビープ音 / 先頭文字欠落

## 前提

これは「バグ」— SPEC.md の「ホットキーで表示 → 即入力可能」という意図に対して、ビープ音・先頭文字欠落はバグ。

## 根本原因（3層構造）

1. **SendInput フォーカス非同期レース**: `set_focus()` → `SendInput()` の間に非同期ギャップがあり、Alt key-up が正しいウィンドウに届かない場合がある
2. **WebView2 コールドスタートのレンダラスロットル**: `visible: false` で作成 → `SetIsVisible(true)` 後、初回フレーム描画完了まで 16〜48ms+ の遅延。その間キー入力が正しく処理されない
3. **WM_SYSCHAR → DefWindowProc → MessageBeep**: Alt 修飾が残った状態で文字キーが来ると、ネイティブ HWND レベルで `MessageBeep(0)` が呼ばれる。JS の `preventDefault()` はレンダラ IPC 経由のため間に合わない

## 実装済み

- MenuMaskKey 技法で `send_alt_key_up()` を改善（ダミーキー vkE8 + VK_MENU/LMENU/RMENU）
- `send_alt_key_up()` を `show()` + `set_focus()` の後に移動

**手動テスト結果**: ビープ音が依然として発生 → 層 1 の対策だけでは不十分

---

## 改善方針（改訂版）

2つのフェーズで根本的に対処する。

### フェーズ P1: AcceleratorKeyPressed で WM_SYSCHAR を阻止（Rust 側）

#### 目的

WebView2 の公式 API `ICoreWebView2Controller::add_AcceleratorKeyPressed` を使い、Alt 残留による `WM_SYSKEYDOWN` を `Handled` にマークする。これにより `TranslateMessage` → `WM_SYSCHAR` → `DefWindowProc` → `MessageBeep` の経路を根本的に遮断する。

#### 技術的根拠

- `AcceleratorKeyPressed` は WebView2 公式 API（SDK 0.9.430 以降、全バージョンで利用可能）
- ハンドラは**同期的に実行**され、`SetHandled(true)` でイベントを消費できる
- `KeyEventKind::SystemKeyDown` (=2) は `WM_SYSKEYDOWN` に対応
- `SetHandled(true)` すると WebView2 は `TranslateMessage` / `DefWindowProc` にメッセージを渡さない → `WM_SYSCHAR` が生成されず `MessageBeep` も鳴らない
- Tauri の `with_webview()` → `PlatformWebview::controller()` → `ICoreWebView2Controller` でアクセス可能
- `webview2_com` 0.38.2 に `AcceleratorKeyPressedEventHandler::create()` が存在

#### 変更ファイル

| ファイル | 変更内容 |
|---|---|
| `src-tauri/src/main.rs` | setup フェーズで `add_AcceleratorKeyPressed` ハンドラを登録 |

#### 実装詳細

setup フェーズ（`ensure_window("main", ...)` の後）で登録:

```rust
// main ウィンドウの WebView2 controller に AcceleratorKeyPressed ハンドラを登録し、
// Alt 残留による WM_SYSKEYDOWN (SystemKeyDown) を握り潰す。
// これにより TranslateMessage → WM_SYSCHAR → DefWindowProc → MessageBeep(0) を阻止。
if let Some(main) = app.get_webview_window("main") {
    main.with_webview(move |platform_webview| {
        #[cfg(windows)]
        {
            use webview2_com::Microsoft::Web::WebView2::Win32::{
                ICoreWebView2AcceleratorKeyPressedEventArgs,
                ICoreWebView2Controller,
                COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN,
            };
            use webview2_com::AcceleratorKeyPressedEventHandler;

            let controller: ICoreWebView2Controller = platform_webview.controller();
            let handler = AcceleratorKeyPressedEventHandler::create(Box::new(
                move |_controller, args| {
                    if let Some(args) = args {
                        let mut kind = COREWEBVIEW2_KEY_EVENT_KIND(0);
                        unsafe { args.KeyEventKind(&mut kind)? };
                        if kind == COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN {
                            // Alt+char の WM_SYSKEYDOWN を Handled にし、
                            // WM_SYSCHAR → MessageBeep を阻止
                            unsafe { args.SetHandled(true)? };
                        }
                    }
                    Ok(())
                },
            ));
            let mut token = 0i64;
            unsafe {
                let _ = controller.add_AcceleratorKeyPressed(&handler, &mut token);
            }
        }
    }).expect("with_webview should succeed in setup phase");
}
```

#### 不変条件

- `with_webview` は setup フェーズで呼ぶ（イベントループ中はデッドロックする — src-tauri/CLAUDE.md 参照）
- `SYSTEM_KEY_DOWN` のみを対象とする。`KEY_DOWN` / `KEY_UP` / `SYSTEM_KEY_UP` は通常のキーボード操作に必要なためハンドルしない
- **すべての `SYSTEM_KEY_DOWN` を `Handled` にする**: Snotra の検索ウィンドウにはメニューバーがなく、Alt+文字のシステムキーコマンドを一切使用しない。Alt+F4 は `VK_F4` で `key.length === 1` ではないが `SYSTEM_KEY_DOWN` の `VK_F4` にマッチする → これも Handled にして良いか要検討
- **Alt+F4 の扱い**: `VK_F4` (0x73) は `SYSTEM_KEY_DOWN` で来る。Snotra は `CloseRequested` → `prevent_close()` → `hide()` で処理するため、Alt+F4 を Handled にしても `CloseRequested` イベントが発生しなくなる可能性がある。安全のため **`VK_F4` は除外**する

#### 修正後のハンドラ（Alt+F4 除外版）

```rust
move |_controller, args| {
    if let Some(args) = args {
        let mut kind = COREWEBVIEW2_KEY_EVENT_KIND(0);
        unsafe { args.KeyEventKind(&mut kind)? };
        if kind == COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN {
            let mut vk = 0u32;
            unsafe { args.VirtualKey(&mut vk)? };
            // Alt+F4 (VK_F4=0x73) は OS のウィンドウ閉じ処理に委ねる
            if vk != 0x73 {
                unsafe { args.SetHandled(true)? };
            }
        }
    }
    Ok(())
}
```

#### リスク

- `with_webview` は Tauri バージョン依存（マイナーバージョン固定推奨）→ 既に `tauri = "2"` で使用中。`with_webview` のシグネチャ変更リスクは Tauri v2 内では低い
- `AcceleratorKeyPressed` ハンドラの登録解除（`remove_AcceleratorKeyPressed`）は不要（ウィンドウと同じライフサイクル）
- `webview2_com` クレートは Tauri の transitive dependency。直接依存に追加する必要があるか要確認

---

### フェーズ P2: フォーカス同期の強化（Rust 側）

#### 目的

`set_focus()` 後に `SendMessageTimeoutW(WM_NULL)` を挿入し、フォーカス移行の完了を同期的に待つ。これにより `send_alt_key_up()` の `SendInput` が確実に WebView2 HWND に配送される。

#### 技術的根拠

- Raymond Chen 推奨のパターン: `SetForegroundWindow` 後に `SendMessageTimeout(WM_NULL)` を送ると、ターゲットウィンドウが前述のアクティベーション通知（`WM_ACTIVATE` / `WM_SETFOCUS`）を処理し終わるまでブロックする
- `WM_NULL` は副作用がない。`SMTO_NORMAL` で最大 100ms タイムアウト
- `SendMessageTimeoutW` は既存の `Win32_UI_WindowsAndMessaging` feature で利用可能（追加不要）

#### 変更ファイル

| ファイル | 変更内容 |
|---|---|
| `src-tauri/src/main.rs` | `show_main_and_emit` 内、`set_focus()` 後に `SendMessageTimeoutW` 追加 |

#### 実装詳細

`show_main_and_emit()` の `set_focus()` 後:

```rust
let _ = main.set_focus();

// Ensure focus transfer is fully processed before sending synthetic
// key-ups.  SetForegroundWindow is partially asynchronous; sending
// WM_NULL via SendMessageTimeout blocks until the target window has
// processed all pending activation messages (WM_ACTIVATE, WM_SETFOCUS).
#[cfg(windows)]
if let Ok(hwnd) = main.hwnd() {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, WM_NULL, SMTO_NORMAL,
    };
    let hwnd = HWND(hwnd.0);
    let mut result = 0usize;
    unsafe {
        let _ = SendMessageTimeoutW(
            hwnd,
            WM_NULL,
            windows::Win32::Foundation::WPARAM(0),
            windows::Win32::Foundation::LPARAM(0),
            SMTO_NORMAL,
            100,  // 100ms timeout — sufficient for focus sync
            Some(&mut result),
        );
    }
}

send_alt_key_up();
```

#### 不変条件

- `SendMessageTimeoutW` は `set_focus()` と `send_alt_key_up()` の間に挿入する
- タイムアウト 100ms は UI スレッドのブロックとして許容範囲（ホットキー表示は既に Alt 待機で最大 350ms かかる設計）
- `SMTO_NORMAL` を使用（`SMTO_ABORTIFHUNG` ではない — 自プロセスのウィンドウなのでハングしない前提）

---

## 旧フェーズの扱い

| 旧フェーズ | 状態 | 理由 |
|---|---|---|
| フェーズ 1 (MenuMaskKey) | ✅ 実装済み・維持 | `send_alt_key_up()` 自体は引き続き有効。P2 でフォーカス同期後に呼ばれるため、正しいウィンドウに届くようになる |
| フェーズ 2 (RAF 1フレーム化) | ❌ 見送り | P1 の AcceleratorKeyPressed が WM_SYSCHAR を根本阻止するため、フォーカス速度の改善は副次的。2フレーム遅延はコールドスタートの安全マージンとして維持 |
| フェーズ 3 (Alt ガード文字救済) | ❌ 見送り | P1 が WM_SYSKEYDOWN を Handled にするため、JS 側に Alt+char イベントが届かなくなる。文字救済は不要 |

---

## テスト方針

### ビルド検証（必須）

- `cargo check -p snotra-core -p snotra`
- `npm run build`

### 手動検証（必須）

1. アプリ起動直後に Alt+Q → すぐ「abc」入力 → 「abc」が欠落なく入力されること
2. `Windows Background.wav` のビープ音が鳴らないこと
3. Alt+Q でトグル（hide → show → hide → show）後に同じ検証
4. Alt+F4 でウィンドウが非表示になること（CloseRequested → hide が動作すること）
5. 他アプリ（Explorer, VS Code）が Alt+Q 後に異常動作しないこと
6. F10 キー（メニューアクティベーション）が Snotra 内で異常動作しないこと

### 注意事項

- `webview2_com` クレートを `src-tauri/Cargo.toml` の直接依存に追加する必要がある場合、バージョンは Cargo.lock の `0.38.2` に合わせる
- `with_webview` のクロージャは `Send + 'static` が要求される。`controller` は `Clone` なのでムーブ可能

## SPEC.md 更新要否

不要。既存の「ホットキーで表示」仕様の範囲内の改善。

---

## セルフレビュー

### 1. 対称コードパス

- `show` / `hide` ペア: `hide` パスでは AcceleratorKeyPressed は影響しない（非表示中はキー入力が来ない）→ 変更なし ✓
- `send_alt_key_up()` の呼び出し: `show_main_and_emit()` 内に統合済み。全3コールサイトで統一 ✓
- `add_AcceleratorKeyPressed` / `remove_AcceleratorKeyPressed`: ウィンドウと同ライフサイクルのため remove 不要 ✓

### 2. 影響範囲の網羅性

- `AcceleratorKeyPressed` は `main` ウィンドウのみ登録。`results` ウィンドウは `WS_EX_NOACTIVATE` でキーボードフォーカスを持たないため対象外 ✓
- `SendMessageTimeoutW` は `show_main_and_emit()` 内のみ。全コールサイト（hotkey Alt待機/直接/起動時表示）で自動適用 ✓
- `SYSTEM_KEY_DOWN` を Handled にすることで JS 側の `e.altKey && e.key.length === 1` ガードに到達するイベントが減る。ただしガードは残しても無害 ✓

### 3. 境界条件

- Alt+F4: VK_F4 (0x73) を除外 → CloseRequested が正常動作 ✓
- F10 (メニュー起動): F10 は `WM_SYSKEYDOWN` で来る。Handled にすることでメニュー起動を抑制 → Snotra にメニューバーはないため問題なし ✓
- Ctrl+Alt+文字: Ctrl が押されている場合も `SYSTEM_KEY_DOWN` で来る可能性がある → ホットキーは `RegisterHotKey` で別経路処理されるため影響なし ✓
- `with_webview` が失敗した場合: `.expect()` でパニック → setup フェーズでのパニックはアプリ起動失敗として適切 ✓
- `webview2_com` の型不一致: Tauri の transitive dependency と同一バージョンであることを Cargo.lock で確認済み (0.38.2) ✓

### 4. リソース管理

- `AcceleratorKeyPressedEventHandler`: COM オブジェクト。参照カウントで管理され、controller が破棄されると自動解放 ✓
- `token` (i64): ハンドラの登録 ID。remove しないため保持不要 ✓
- `SendMessageTimeoutW`: ブロッキング呼び出し。タイムアウト 100ms。リソースなし ✓

### 5. 既存パターンとの整合

- `with_webview` は既存コードベースで未使用だが、Tauri 公式 API。既存パターンの拡張 ✓
- `SendMessageTimeoutW` は既存コードベースで未使用だが、`PostThreadMessageW` パターンと整合 ✓
- `webview2_com` 型の直接使用は新規パターン。ただし Tauri の `PlatformWebview::controller()` がこの型を返すため、使用は自然 ✓

### 6. YAGNI 違反チェック

- 旧フェーズ 2, 3 を見送り → 不要な改善を削除。YAGNI 準拠 ✓
- `AcceleratorKeyPressed` で全 `SYSTEM_KEY_DOWN` を対象（VK_F4 除く）→ 個別キーの条件分岐は不要。シンプル ✓

### 7. シンプル化の挑戦

- P1 は ~20行の setup コード。WebView2 公式 API のため信頼性が高い ✓
- P2 は ~10行の `SendMessageTimeoutW` 挿入。副作用なし ✓
- WH_GETMESSAGE フックや HWND サブクラスといった複雑な代替案を排除 ✓

### 8. 破壊不変条件

- **`SYSTEM_KEY_DOWN` を Handled にする**: Alt+文字の入力が JS レイヤーに `altKey: true` として届かなくなる。既存の Alt ガードが空振りするが、ガードが不要になるだけなので安全。`keydown` イベント自体は `altKey: false` で届くか、届かないかのどちらか → 手動テストで確認 ✓
- **`SendMessageTimeoutW` がタイムアウトした場合**: フォーカス同期が不完全なまま `send_alt_key_up()` に進む。現状と同じ挙動なので退行なし ✓
- **`with_webview` クロージャ内のパニック**: COM API 呼び出しは `Result` を返す。`?` で伝播し、クロージャの `Result<()>` として返る。パニックはしない ✓
