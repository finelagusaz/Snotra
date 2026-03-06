# Plan: Issue #149 — リリースビルドでアイコンが表示されない（CSP に connect-src 不足）

## 変更ファイル一覧

| ファイル | 変更内容 |
|---|---|
| `src-tauri/tauri.conf.json` | CSP に `connect-src ipc: http://ipc.localhost` を追加 |

## 実装順序

### Phase 1: CSP 修正（1ファイル）

`src-tauri/tauri.conf.json` の `app.security.csp` を修正:

**Before:**
```
default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:
```

**After:**
```
default-src 'self'; connect-src ipc: http://ipc.localhost; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:
```

Tauri v2 公式ドキュメントの推奨に従い、`connect-src ipc: http://ipc.localhost` を追加する。

## 不変条件

- `invoke()` で `ipc::Response` を返すコマンドの戻り値は `ArrayBuffer` でなければならない
- CSP は custom protocol IPC（`http://ipc.localhost`）をブロックしてはならない
- 既存の `default-src`, `script-src`, `style-src`, `img-src` ディレクティブを変更しない

## テスト方針

- `npm run build` でフロントエンドビルドが通ること（CSP は JSON 文字列なのでビルドに影響しないが確認）
- リリースビルド（`npx tauri build --no-bundle`）後、実バイナリでアイコンが表示されることを手動確認
- 既存テスト: `npm test` が通ること

## SPEC.md 更新要否

不要。CSP はインフラ設定であり、ユーザー向け挙動の仕様変更ではない。

## セルフレビュー

1. **対称コードパス**: CSP は全ウィンドウ（main/results）に共通適用される。ウィンドウ別の設定は不要。
2. **影響範囲の網羅性**: `ipc::Response` を使うコマンドは `get_icons_batch` のみ（grep 確認済み）。他のコマンドは JSON なので影響なし。
3. **境界条件**: `connect-src` は IPC 通信のみ制御。`default-src`, `script-src`, `img-src` は変更しないため副作用なし。
4. **リソース管理**: 該当なし（設定変更のみ）。
5. **既存パターンとの整合**: Tauri v2 公式ドキュメントの推奨 CSP 例に合致。
6. **YAGNI 違反**: なし。最小限の1行追加。
7. **シンプル化の挑戦**: CSP 文字列に1ディレクティブ追加するだけ。これ以上シンプルにはできない。
8. **破壊不変条件の明示**: CSP の `connect-src` が欠けると IPC custom protocol がブロックされ、バイナリ応答が壊れる。検証: リリースビルド後の手動確認（アイコン表示）。
