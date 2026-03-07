# Plan — main と results のフロントエンドを別エントリポイントに分割 (#163)

## 変更ファイル一覧

1. **`ui/main.html`** — 新規。main ウィンドウ用 HTML エントリ
2. **`ui/results.html`** — 新規。results ウィンドウ用 HTML エントリ
3. **`ui/src/main.tsx`** — 新規。main ウィンドウ用 JS エントリ（MainApp をレンダリング）
4. **`ui/src/results.tsx`** — 新規。results ウィンドウ用 JS エントリ（ResultsApp をレンダリング）
5. **`ui/src/MainApp.tsx`** — 新規。現 `App.tsx` の main 分岐ロジックを移行
6. **`ui/src/ResultsApp.tsx`** — 新規。現 `App.tsx` の results 分岐ロジック（テーマ適用のみ）+ `ResultsWindow` レンダリング
7. **`ui/src/App.tsx`** — 削除
8. **`ui/src/index.tsx`** — 削除
9. **`ui/index.html`** — 削除
10. **`vite.config.ts`** — multi-page build 設定追加
11. **`src-tauri/src/commands/window.rs`** — `WebviewUrl` を `results.html` に変更
12. **`src-tauri/tauri.conf.json`** — main ウィンドウの URL を `main.html` に変更（必要な場合）
13. **`ui/CLAUDE.md`** — エントリポイント構成を更新

## 実装順序

### Phase 1: HTML + エントリポイント作成

`ui/main.html`:
```html
<!DOCTYPE html>
<html lang="ja">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Snotra</title>
    <link rel="stylesheet" href="src/styles/global.css" />
  </head>
  <body>
    <div id="root"></div>
    <script src="src/main.tsx" type="module"></script>
  </body>
</html>
```

`ui/results.html`: 同構造で `src/results.tsx` を読む。

`ui/src/main.tsx`:
```tsx
import { render } from "solid-js/web";
import MainApp from "./MainApp";

const root = document.getElementById("root");
if (root) {
  render(() => <MainApp />, root);
}
```

`ui/src/results.tsx`: 同構造で `ResultsApp` をレンダリング。

### Phase 2: App.tsx の分割

**`MainApp.tsx`**: 現 `App.tsx` から以下を移行:
- `onMount` 内の `label === "main"` ブロック全体（8つの listen + controller + initIndexingState + resize/move）
- `visual-config-changed` listen + `getBootstrapPayload` + `applyTheme`
- `auto_hide_on_focus_lost` ロジック
- `onCleanup`（unlistenFns）
- レンダリング: `<SearchWindow />`（Switch/Match 不要）

**`ResultsApp.tsx`**: 現 `App.tsx` から以下を移行:
- `visual-config-changed` listen + `getBootstrapPayload` + `applyTheme`（テーマのみ）
- `onCleanup`（unlistenFns）
- レンダリング: `<ResultsWindow />`

テーマ適用は両方に必要だが、コードは 10 行程度で DRY 抽出不要（2箇所まで許容）。

### Phase 3: Vite multi-page 設定

`vite.config.ts`:
```ts
import { resolve } from "path";

export default defineConfig({
  // ...existing...
  build: {
    target: "esnext",
    outDir: "../dist",
    rollupOptions: {
      input: {
        main: resolve(__dirname, "ui/main.html"),
        results: resolve(__dirname, "ui/results.html"),
      },
    },
  },
});
```

### Phase 4: Rust 側 URL 変更

`src-tauri/src/commands/window.rs` L24:
```rust
// Before:
WebviewUrl::App(Default::default())
// After:
WebviewUrl::App("results.html".into())
```

`src-tauri/tauri.conf.json`: main ウィンドウの URL 設定。`tauri.conf.json` ではウィンドウ個別の URL は `url` フィールドで指定可能。デフォルトは `index.html` なので `main.html` に変更が必要:
```json
{
  "app": {
    "windows": [
      {
        "label": "main",
        "url": "main.html",
        ...
      }
    ]
  }
}
```

### Phase 5: 旧ファイル削除

- `ui/index.html` 削除
- `ui/src/index.tsx` 削除
- `ui/src/App.tsx` 削除

### Phase 6: テスト・ドキュメント

- `npm test` — 既存テストが通ることを確認
- `npm run build` — 2つのバンドルが生成されることを確認
- `cargo check -p snotra` — Rust 側 URL 変更の型チェック
- `ui/CLAUDE.md` のエントリポイントセクションを更新

## 不変条件

1. **main と results が別バンドルでビルドされる**: `dist/main.html` + `dist/results.html` が独立した JS を読む
2. **results バンドルに search.ts が含まれない**: results.tsx → ResultsApp → ResultsWindow の依存グラフに `stores/search.ts` が入らない
3. **テーマ適用は両ウィンドウで機能する**: `visual-config-changed` listen + `applyTheme` は両方のエントリポイントに存在
4. **既存の E2E・スモークテストが壊れない**: ウィンドウラベル（main/results）は変更しない

## テスト方針

- `npm test` — フロントエンドユニットテスト
- `npm run build` — 2バンドル生成を確認（出力に `main.html` + `results.html` が表示される）
- `cargo check -p snotra` — Rust URL 変更の型チェック
- バンドルサイズ比較: ビルド出力で main.js / results.js のサイズを記録し、効果を検証

## SPEC.md 更新要否

**不要**。挙動変更なし（内部のビルド構成変更のみ）。

## セルフレビュー

### 1. 対称コードパス
- `MainApp` / `ResultsApp` の対称ペアとしてテーマ適用ロジックが両方に必要 — 確認済み

### 2. 影響範囲の網羅性
- フロントエンド: `index.html` / `index.tsx` / `App.tsx` を参照する箇所を全検索する必要あり
- Rust: `WebviewUrl::App(Default::default())` は `window.rs` の 1 箇所のみ
- `tauri.conf.json`: main window の URL デフォルトが `index.html` → `main.html` に変更必要
- E2E テスト: ウィンドウラベルは変更しないため影響なし
- スモークテスト: `main:ensure_window:ok` の検証対象ラベルは変更なし

### 3. 境界条件
- dev server: `http://localhost:5173/main.html` でアクセス可能か要確認（Vite multi-page は公式サポート）
- `tauri dev` 時の `devUrl`: `http://localhost:5173` がルートで、各 HTML は相対パスでアクセスされるため問題なし

### 4. リソース管理
- 新規リソース追加なし。既存の listen/unlisten 構造をそのまま移行

### 5. 既存パターンとの整合
- Vite multi-page は公式パターン。新規独自パターンの導入なし

### 6. YAGNI 違反
- テーマ適用の共通化ヘルパー抽出は見送り（2箇所のみ、DRY 閾値内）
- `invoke.ts` の results 用サブセット抽出は行わない（tree-shaking で自動的に不要関数は除外される）

### 7. シンプル化の挑戦
- `App.tsx` を分割する代わりに、結果的に2つのシンプルなコンポーネントになる。Switch/Match 分岐が消えてコードが単純化される
- 新規状態・Mutex・子プロセスの追加なし

### 8. 破壊不変条件の明示
- **ウィンドウ URL ルーティング**: `tauri.conf.json` の `url` と `WebviewUrl::App` が正しい HTML を指さないとウィンドウが空白になる。ビルド出力確認 + `cargo check` で検知
- **dev server ルーティング**: `tauri dev` で両 HTML にアクセスできないと開発不能。Vite multi-page の dev server 動作は公式サポートのため低リスク
