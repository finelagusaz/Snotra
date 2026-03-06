# Plan: エラーハンドリング残骸の整理

## 方針

`void promise.then(fn => { unlisten = fn })` パターンの `.catch()` 欠如を修正する。変更は **ログ追加のみ** に限定し、制御フロー・状態遷移・戻り値には一切触れない。

## 変更対象と除外の判断

### 変更する (3ファイル・7箇所)

| # | ファイル | 行 | パターン | 修正内容 |
|---|---|---|---|---|
| 1 | `ResultsWindow.tsx` | L99-101 | `void api.getBootstrapPayload().then(...)` | `.catch(e => console.warn(...))` 追加 |
| 2 | `ResultsWindow.tsx` | L107-113 | `void listen("show-icons-changed", ...).then(...)` | 同上 |
| 3 | `ResultsWindow.tsx` | L124-129 | `void listen("visual-config-changed", ...).then(...)` | 同上 |
| 4 | `ResultsWindow.tsx` | L144-178 | `void listen("results-sync", ...).then(...)` | 同上 |
| 5 | `SearchWindow.tsx` | L66-71 | `void listen("window-shown", ...).then(...)` | 同上 |
| 6 | `SearchWindow.tsx` | L72-83 | `void getCurrentWindow().onFocusChanged(...).then(...)` | 同上 |
| 7 | `SearchWindow.tsx` | L87-91 | `void (async () => { ... })()` | try/catch + `console.warn` で囲む |

### 変更しない (理由付き)

| 箇所 | 理由 |
|---|---|
| `App.tsx` の listen 群 (L76-116) | `await Promise.all(...)` で await 済み。失敗は `onMount` async の unhandled rejection として表面化する |
| `App.tsx` の `saveSearchPlacement` (L150-153) | setTimeout 内の debounced fire-and-forget。位置保存失敗は非クリティカルで、ログを足しても確認する機会がない |
| `App.tsx` の `void hideMainAndResults()` (L90) | 直前に `console.warn("Failed to launch...")` がある文脈。hide 自体の失敗は Tauri API レベルの致命的問題で、warn 1行では対処にならない |
| `App.tsx` の `void controller.handleMainMoved(...)` (L158) | `resultsWindowController.ts` 内部に既に `.catch(console.error)` がある (L70-82) |
| `search.ts` の `void api.recordFolderExpansion(dir)` (L299) | 非クリティカルな履歴記録。意図的な fire-and-forget |
| `commands.ts` の `api.quitApp()` (L57) | アプリ終了。失敗してもログを見る機会がない |
| E2E テスト内の `.catch(() => {})` | テストハーネスのクリーンアップ。意図的 |

## 副作用リスクの検証

### 変更が安全である根拠

1. **制御フローを変えない**: `.catch()` の追加は Promise チェーンの末尾に付くだけ。`.then()` の成功パスには影響しない
2. **`unlisten` 変数の代入タイミングを変えない**: `.catch()` は `.then()` の後に付くため、成功時の `.then(fn => { unlisten = fn })` は従来通り実行される
3. **戻り値を変えない**: `void` で呼び出しているため戻り値は使われていない
4. **`onCleanup` の登録タイミングに干渉しない**: `onCleanup` は `.then()` より前の同期コンテキストで登録済み（`ui/CLAUDE.md` の不変条件）

### 確認すべき境界条件

- `.catch()` 内で `throw` しない → `console.warn` のみなので安全
- `.catch()` が `.then()` の実行を阻害しない → Promise の仕様上、`.then()` 成功後に `.catch()` は呼ばれない
- SearchWindow L87-91 の try/catch 追加で `focusInputWithRetries()` の呼び出しが変わらない → catch 内は `console.warn` + `return` のみ

## 実装順序

1. `ResultsWindow.tsx` — 4箇所に `.catch()` を追加
2. `SearchWindow.tsx` — 3箇所に `.catch()` / try-catch を追加
3. 検証: `npm run build` (typecheck + vite build)
4. 検証: `npx vitest run` (既存テスト通過)

## warn メッセージの命名規則

既存の `console.warn` に合わせ、`"<コンテキスト>: <何が失敗したか>"` 形式にする:

```
"ResultsWindow: failed to load bootstrap payload"
"ResultsWindow: failed to listen show-icons-changed"
"ResultsWindow: failed to listen visual-config-changed"
"ResultsWindow: failed to listen results-sync"
"SearchWindow: failed to listen window-shown"
"SearchWindow: failed to listen focus-changed"
"SearchWindow: failed to check initial visibility"
```
