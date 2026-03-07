# Research: getBootstrapPayload() の重複取得削減 (#166)

## issue の要約

起動直後に `getBootstrapPayload()` IPC が複数回（最大3回）呼ばれ、`Engine` の `Mutex` ロック競合を起こしている。results ウィンドウ側の bootstrap 呼び出しを削減し、軽量な初期化経路を作る。

## 現状の bootstrap 呼び出し箇所（3箇所）

### 1. MainApp.tsx:168
```ts
bootstrap = await api.getBootstrapPayload();
applyTheme(bootstrap.visual);
// + bootstrap.general.auto_hide_on_focus_lost
```
**使用フィールド**: `visual`（テーマ適用）、`general.auto_hide_on_focus_lost`（フォーカス喪失時の自動非表示）

### 2. ResultsApp.tsx:20
```ts
const bootstrap = await getBootstrapPayload();
applyTheme(bootstrap.visual);
```
**使用フィールド**: `visual`（テーマ適用のみ）

### 3. ResultsWindow.tsx:93
```ts
void api.getBootstrapPayload().then((bootstrap) => {
  setShowIcons(bootstrap.appearance.show_icons);
});
```
**使用フィールド**: `appearance.show_icons`（アイコン表示の初期値のみ）

## 関連コード

### Rust 側: `src-tauri/src/commands/config.rs`
- `get_bootstrap_payload()`: `state.engine.lock()` を取得して `BootstrapPayload` を構築
- `BootstrapPayload` = `visual` + `general` + `appearance` + `indexing`

### Rust 側: `src-tauri/src/config_watcher.rs`
- 設定変更時に `visual-config-changed` イベントを emit（全ウィンドウに配信）
- 設定変更時に `show-icons-changed` イベントを emit

### フロント側リスナー（既存）
- **ResultsApp.tsx**: `visual-config-changed` をリッスン → `applyTheme(event.payload)`
- **ResultsWindow.tsx**: `show-icons-changed` をリッスン → `setShowIcons(event.payload)`
- **ResultsWindow.tsx**: `visual-config-changed` をリッスン → フォント再計測

### 結論: results 側の bootstrap は「初期値取得」だけに使われている
- テーマの「継続的な更新」は `visual-config-changed` イベントで既に対応済み
- `show_icons` の「継続的な更新」は `show-icons-changed` イベントで既に対応済み
- bootstrap IPC は**初期値を得る**ためだけに呼ばれている

## 既存パターン

- `config_watcher.rs` は設定変更時にイベントを emit する仕組みが確立済み
- results ウィンドウは既にテーマ変更イベントと show_icons 変更イベントをリッスンしている

## 設計の選択肢

### 案A: results 用の軽量 IPC コマンド `get_results_bootstrap` を追加
- Rust 側に新コマンド追加（`visual` + `show_icons` のみ返す）
- IPC 回数は減らないが、ロック保持時間が短くなる可能性がある
- **問題**: 本質的に IPC 回数は減らない（2回→1回の統合は可能だが新コマンドが増える）

### 案B: ResultsApp と ResultsWindow の bootstrap を統合（1回に）
- ResultsApp.tsx で bootstrap を1回だけ呼び、テーマ適用 + `show_icons` 初期値を ResultsWindow にプロパティで渡す
- **問題**: ResultsApp → ResultsWindow への props 受け渡しが必要で、SolidJS コンポーネント構造の変更が必要

### 案C: ResultsApp の bootstrap を廃止し ResultsWindow に統合（results 側 1回に）★推奨
- ResultsApp.tsx から bootstrap 呼び出しを削除
- ResultsWindow.tsx の既存 bootstrap 呼び出しでテーマ適用も行う
- MainApp: 1回、results: 1回 = 合計2回（現状3回→2回）
- **メリット**: 最もシンプル。新コマンド不要、新 props 不要、既存構造の最小変更

### 案D: results 側 bootstrap を完全廃止し、main からイベントで初期値を配信
- MainApp が bootstrap 取得後にイベントで results に初期値を配信
- **問題**: results ウィンドウの生成順序やイベントリスニング開始タイミングの依存が増える。複雑度が高い

## 推奨: 案C

**理由**:
1. 最もシンプル（変更行数が最小）
2. 新しいコマンド・型・イベント・props を導入しない
3. bootstrap IPC が 3回→2回に減る
4. results 側の IPC は1回に集約される
5. `show_icons` と `visual` の初期値取得が1回の IPC で済む

## 技術的制約

- Win32 依存なし（フロントエンドのみの変更）
- IPC 境界: `get_bootstrap_payload` は既存コマンドをそのまま使用
- SolidJS リアクティブ: `applyTheme` はグローバルな CSS 変数操作なので、呼び出し元がどのコンポーネントでも問題ない

## 未解決の疑問

なし。変更は明確で、既存パターンの組み替えで対応可能。
