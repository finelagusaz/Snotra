# Research: Issue #155 — 起動直後のホットキー表示で検索フレーズの先頭が飛ぶ

## Issue の要約

アプリ起動直後に Alt+Q でウィンドウを呼び出すと、最初の数文字が入力されない、または Windows の警告音（ビープ）が鳴る。先頭の文字が「飛ぶ」現象。

## 再現条件

1. アプリ起動
2. Alt+Q で呼び出し
3. すぐに文字を入力
4. 先頭文字が欠落 / ビープ音

## 関連コード

### ホットキー検知〜ウィンドウ表示の完全シーケンス

| フェーズ | ファイル | 行 | 処理 |
|---|---|---|---|
| 1. WM_HOTKEY 受信 | `platform/mod.rs` | L182-183 | `emit("hotkey-pressed", ())` |
| 2. Alt 押下チェック | `main.rs` | L93-101 | `is_alt_pressed()` → `GetAsyncKeyState(VK_MENU)` |
| 3. Alt リリース待機 | `main.rs` | L109-126 | 10ms ポーリング、最大 350ms タイムアウト |
| 4. ウィンドウ表示 | `main.rs` | L128-180 | `show()` → `set_focus()` → `TurnOffIme` → `emit("window-shown")` |
| 5. IME オフ | `platform/mod.rs` | L252-261 | platform スレッドで `ImmSetOpenStatus(false)` |
| 6. input フォーカス | `SearchWindow.tsx` | L35-66 | 2フレーム遅延 + 120ms/280ms リトライ |

### Alt キー処理の現状

- `main.rs:92-107`: `GetAsyncKeyState` で Alt 物理キーの押下状態を検出
- `main.rs:109-126`: Alt が押されていれば別スレッドで最大 350ms 待機してからウィンドウ表示
- `SearchWindow.tsx:146-151`: `e.altKey && e.key.length === 1` を `preventDefault()` でブロック（ビープ対策）
- **`keybd_event` / `SendInput` による Alt キーリリースのシミュレーションは存在しない**

### IME 処理の現状

- `show_main_and_emit` 内で `set_focus()` 後に `TurnOffIme` コマンドを platform スレッドに送信
- platform スレッドでの処理は非同期（`WM_PLATFORM_WAKE` を待って実行）
- コード内コメントに「narrow timing window exists」と記載済み

### フロントエンド input フォーカスの現状

- `window-shown` イベント受信後、`requestAnimationFrame` 2回分（~33ms）遅延してから `focus()`
- 120ms, 280ms でリトライ
- `onFocusChanged` でウィンドウフォーカス取得時にもリトライ

## 分析: 考えられる原因

### 原因 1: Alt キーの「残留」（最有力）

**メカニズム**: Alt+Q でホットキー発火 → Alt リリース待機 → ウィンドウ表示。しかし:
- `GetAsyncKeyState` は「今この瞬間」のキー状態を返す。ホットキーイベント時の状態ではない
- ユーザーが Alt をすばやくリリースした場合、待機ループが開始される前に Alt がリリースされている可能性がある。しかし **OS のキーボード状態は Alt が押されたまま** の場合がある
- `RegisterHotKey` は `MOD_NOREPEAT` つきで登録されている。Windows は `WM_HOTKEY` を送る際、修飾キーの key-up を自前で合成しない
- WebView2 側では、OS の Alt 状態が残っているため、最初のキー入力が `altKey: true` として解釈される → ビープ音

**証拠**:
- `SearchWindow.tsx:146-151` に `altKey` ガードが **既に存在** する = この問題は認知されている
- しかし `preventDefault()` は WebView2 のイベント段階で動作するため、OS レベルのビープ音は防げない場合がある

### 原因 2: input フォーカスの遅延

**メカニズム**: ウィンドウ表示後、input にフォーカスが移るまで ~33ms + α の遅延がある。この間のキー入力は input に届かない。

**証拠**:
- 2フレーム遅延 + リトライ（120ms/280ms）= フォーカス確立が不安定であることを示唆
- 「起動直後」に限定される = WebView2 の初回表示が特に遅い（コメント: `61ms + 71ms overhead on first invocation`）

### 原因 3: IME の競合

**メカニズム**: `set_focus()` と `TurnOffIme` が異なるスレッドで非同期実行。IME がオフになる前にキー入力が到着すると、IME がキーを消費する。

**証拠**:
- コード内コメントに race condition の記載あり
- ただし「Residual race is theoretical and not observed in practice」とも記載

### 原因 4: 起動直後特有の cold-start 遅延

**メカニズム**: 起動直後は WebView2 の初期化、JS の初回実行、イベントリスナーの登録が完了していない可能性がある。`window-shown` イベントがリスナー登録前に emit されると、フォーカス処理が走らない。

**証拠**:
- `SearchWindow.tsx:117-127` にフォールバック処理がある（「startup timing: if first window-shown was emitted before this listener mounted」）
- しかしこのフォールバックも非同期（`await getCurrentWindow().isVisible()`）

## 不具合か勘違いかの判定

### 不具合として確認できる要素

1. **Alt キー残留問題は構造的に存在する**: `GetAsyncKeyState` ベースのポーリングでは、Alt の物理リリースと OS のキー状態リセットにタイムラグがある。`MOD_NOREPEAT` は WM_HOTKEY の繰り返し防止であり、修飾キーの key-up 合成とは無関係
2. **input フォーカス遅延は構造的に存在する**: 2フレーム遅延 + リトライは「フォーカスが一発で確立しない」前提の設計
3. **コード内に既存の対策が複数ある**: Alt ガード、リトライ、フォールバック — いずれも問題の存在を裏付ける

### 勘違いの可能性

- 他アプリの Alt フック干渉（PowerToys、Wox 等）による環境依存問題の可能性はある
- ただしコード上の構造的な問題が存在する以上、少なくとも改善の余地はある

### 結論

**不具合として扱うべき**。ただし完全な解消は困難な可能性がある（OS レベルの Alt キー状態は制御しきれない）。改善策として「Alt リリース後のキー状態クリア」と「input フォーカスの高速化」の両面からアプローチする。

## 技術的制約

- `SendInput` / `keybd_event` で Alt key-up を合成すると、他のアプリのフック処理と干渉する可能性がある
- `SetForegroundWindow` の制約: ユーザーの現在のフォアグラウンドウィンドウからでなければ失敗する場合がある
- WebView2 の `focus()` は DOM レベルの操作であり、OS レベルのウィンドウフォーカスとは別レイヤー

## 未解決の疑問

- `GetAsyncKeyState` がリリース後もしばらく pressed を返す具体的な時間幅は環境依存。実測データがない
- WebView2 の初回表示で JS のイベントリスナーが確実に登録されるタイミングの保証がない
