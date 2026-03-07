# Research — main と results のフロントエンドを別エントリポイントに分割 (#163)

## issue の要約

2つの WebView（main/results）が同一 JS バンドルを重複ロードしているため、Vite multi-page build でエントリポイントを分割し、初期メモリと parse/compile コストを下げる。

## 現状分析

### バンドル構成

- 単一バンドル: `index-*.js` **54.58 kB**（gzip 17.18 kB）
- 両 WebView が同じ `index.html` → 同じ JS を読み込む
- `App.tsx` で `getCurrentWindow().label` により `SearchWindow` / `ResultsWindow` を出し分け

### 依存関係の分析

**results 側が実際に使うモジュール（ソースサイズ）:**
| モジュール | サイズ |
|-----------|--------|
| `ResultsWindow.tsx` | 8.4 kB |
| `ResultRow.tsx` | 2.0 kB |
| `invoke.ts`（一部関数のみ） | 4.2 kB |
| `truncatePath.ts` | 3.8 kB |
| `searchEvents.ts` | 0.9 kB |
| `types.ts` | 0.7 kB |
| `theme.ts` | 0.6 kB |
| **合計** | **~20.6 kB** |

**results 側が不要だが現在ロードされるモジュール:**
| モジュール | サイズ |
|-----------|--------|
| `search.ts`（最大モジュール） | 19.0 kB |
| `SearchWindow.tsx` | 9.8 kB |
| `resultsWindowController.ts` | 8.3 kB |
| `perf.ts` | 3.4 kB |
| `i18n.ts` | 1.8 kB |
| `commands.ts` | 1.7 kB |
| `folderNav.ts` | 1.6 kB |
| `pathQuery.ts` | 1.4 kB |
| `hotkeyValidation.ts` | 1.4 kB |
| `folder.ts`, `tool-selection.ts` | 0.8 kB |
| **合計不要コード** | **~49.2 kB** |

### 効果見積もり

54.58 kB バンドルの内訳推定:
- フレームワーク（solid-js + @tauri-apps/api）: ~30 kB
- アプリケーション固有コード: ~24 kB

分割後の見積もり:
| | 現在 | 分割後 |
|--|------|--------|
| main bundle | 54.58 kB | ~50 kB |
| results bundle | 54.58 kB | ~38 kB |
| **合計ロード量** | **109 kB** | **~88 kB（19% 削減）** |
| results parse/compile | 54.58 kB | ~38 kB（**31% 削減**） |

メモリ面:
- results WebView が `search.ts`（19 kB src）の SolidJS store を持たないため JS ヒープ減少
- V8 compiled code も比例して減少
- WebView2 ベースメモリ（~30-50 MB/WebView）に対して JS ヒープの差は控えめ

### 結論

バンドルサイズ効果は moderate（合計 19% 削減、results 側 31% 削減）。実装コストが低く（HTML/エントリポイント追加 + Vite config + Rust URL 変更）、コードの責務分離も改善されるため実施価値あり。

## 関連コード

### フロントエンド
- `ui/index.html` — 唯一の HTML エントリ
- `ui/src/index.tsx` — 唯一の JS エントリ（`render(() => <App />, root)`）
- `ui/src/App.tsx` — ウィンドウラベル分岐 + main 固有リスナー（7つの listen）+ テーマ適用

### Rust バックエンド
- `src-tauri/tauri.conf.json` — main ウィンドウ定義
- `src-tauri/src/commands/window.rs:24` — `ensure_results_window` で `WebviewUrl::App(Default::default())` → `index.html`
- `src-tauri/src/main.rs:421` — setup で `ensure_results_window` 呼び出し
- `src-tauri/src/main.rs:382-414` — main WebView に AcceleratorKeyPressed ハンドラ登録（main のみ）

### ビルド
- `vite.config.ts` — 現在 single-page（`root: "ui"`, `build.outDir: "../dist"`）

### テスト・スモーク
- `scripts/smoke-startup.ps1` — `main:ensure_window:ok` で results/about/settings を検証
- `e2e/tauri.slash.e2e.ts` — Playwright E2E

## 既存パターン

- Vite multi-page build: `build.rollupOptions.input` で複数 HTML を指定する公式パターン
- Tauri v2: `WebviewUrl::App("results.html".into())` で各ウィンドウに別 HTML を割り当て可能
- dev server: multi-page でも単一ポート（5173）で両 HTML を配信

## 技術的制約

- `tauri.conf.json` の `build.frontendDist` / `build.devUrl` はアプリ全体の設定。ウィンドウ個別 URL は `WebviewUrl` で制御
- Vite dev server では `http://localhost:5173/main.html` / `http://localhost:5173/results.html` でアクセス
- `tauri.conf.json` の main window は URL 未指定で `frontendDist` のルート（= `index.html` → `main.html` に変更可能）
- `App.tsx` の main 固有ロジック（8つの listen + resultsWindowController + initIndexingState）を `MainApp.tsx` に移す必要あり
- results 側の `App.tsx` ロジックは `visual-config-changed` listen + `getBootstrapPayload` + `applyTheme` のみ

## 未解決の疑問

特になし。
