# Research: Issue #149 — リリースビルドでアイコンが表示されない（CSP に connect-src 不足）

## Issue の要約

CI リリースビルドで全アイコンが emoji フォールバックになる。`tauri.conf.json` の CSP に `connect-src ipc: http://ipc.localhost` が不足しているため、Tauri v2 の custom protocol IPC がブロックされ、`ipc::Response`（バイナリ）が `ArrayBuffer` として届かない。

## 関連コード

### 直接影響

| ファイル | 役割 |
|---|---|
| `src-tauri/tauri.conf.json:29` | CSP 定義。`connect-src` が未指定 |

### 間接影響（変更不要だが動作に関わる）

| ファイル | 役割 |
|---|---|
| `src-tauri/src/commands/icon.rs` | `get_icons_batch` — 唯一の `ipc::Response` 返却コマンド |
| `src-tauri/src/icon.rs` | `encode_batch_binary` — バイナリフレーム生成 |
| `ui/src/components/ResultsWindow.tsx` | `parseBinaryBatch` — `ArrayBuffer` -> Blob URL パース |
| `ui/src/lib/invoke.ts` | `getIconsBatch` — `invoke<ArrayBuffer>` 呼び出し |

## 既存パターン

- CSP は `tauri.conf.json` の `app.security.csp` に文字列で定義
- Tauri v2 公式ドキュメントの推奨 CSP 例には `connect-src ipc: http://ipc.localhost` が含まれている
- 現在の CSP: `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:`

## 技術的制約

- Tauri v2 の IPC は `fetch('http://ipc.localhost/...')` を使う
- `connect-src` 未指定時、`default-src 'self'`（= `http://tauri.localhost`）が適用される
- `http://ipc.localhost` != `http://tauri.localhost` -> CSP がブロック
- ブロック後 Tauri は `postMessage` にフォールバック
- `postMessage` 経路では `ipc::Response` が `ArrayBuffer` として届かない
- JSON コマンドは `postMessage` でも動作するため、バイナリ応答のみ影響

### CI バイナリの strings で確認した IPC 処理

```js
// content-type で分岐
switch ((response.headers.get('content-type') || '').split(',')[0]) {
  case 'application/json': return response.json().then(...)
  case 'text/plain': return response.text().then(...)
  default: return response.arrayBuffer().then(...)  // binary path
}

// CSP ブロック時のフォールバック
// "IPC custom protocol failed ... fallback to postMessage"
customProtocolIpcFailed = true
sendIpcMessage(message)  // postMessage 経路
```

## 未解決の疑問

- なし。Tauri v2 公式ドキュメントに `connect-src ipc: http://ipc.localhost` が必要と明記されている。
