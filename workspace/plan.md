# Plan: getBootstrapPayload() の重複取得削減 (#166)

## 変更ファイル一覧

1. **`ui/src/ResultsApp.tsx`** — bootstrap 呼び出しを削除（テーマ初期適用の責務を ResultsWindow に移す）
2. **`ui/src/components/ResultsWindow.tsx`** — 既存の bootstrap 呼び出しにテーマ適用を追加
3. **`ui/src/lib/invoke.ts`** — 変更なし（`getBootstrapPayload` は MainApp から引き続き使用）
4. **`src-tauri/src/commands/config.rs`** — 変更なし

## 実装順序

### Phase 1: ResultsWindow.tsx に テーマ適用を追加

現状（L91-95）:
```ts
void api.getBootstrapPayload().then((bootstrap) => {
  setShowIcons(bootstrap.appearance.show_icons);
}).catch(...);
```

変更後:
```ts
void api.getBootstrapPayload().then((bootstrap) => {
  setShowIcons(bootstrap.appearance.show_icons);
  applyTheme(bootstrap.visual);
}).catch(...);
```

`applyTheme` の import を追加。

### Phase 2: ResultsApp.tsx から bootstrap を削除

現状:
```ts
import { getBootstrapPayload } from "./lib/invoke";
// ...
const bootstrap = await getBootstrapPayload();
applyTheme(bootstrap.visual);
```

変更後: bootstrap 呼び出しと `getBootstrapPayload` import を削除。**`visual-config-changed` リスナーと `applyTheme` import は残す**（設定変更時のテーマ反映に必要）。bootstrap 関連の削除対象は `getBootstrapPayload` の import・呼び出し・try/catch のみ。

## 不変条件

1. **テーマの初期適用は必ず行われる**: ResultsWindow.tsx の bootstrap 呼び出しでテーマ適用を行う。bootstrap 失敗時は CSS デフォルト値が使われる（現状と同じ振る舞い）
2. **テーマの継続的な更新は `visual-config-changed` イベントで行われる**: ResultsApp.tsx のリスナーが維持される（変更なし）
3. **`show_icons` の初期値は bootstrap から取得される**: 変更なし
4. **MainApp の bootstrap 呼び出しは影響を受けない**: MainApp は `auto_hide_on_focus_lost` を含む完全な bootstrap を必要とするため、変更しない

### テーマ適用タイミングの考慮

- ResultsApp.tsx の bootstrap: `onMount` の `async` 内で即座に呼ばれる
- ResultsWindow.tsx の bootstrap: `onMount` 内の fire-and-forget (`void ... .then()`) で呼ばれる
- どちらも非同期なので、テーマ適用のタイミングに実質的な差はない
- 結果: 初回描画でテーマ適用前に一瞬デフォルトスタイルが見える可能性があるが、これは現状でも同じ

## テスト方針

- `npm run build` — ビルド成功確認（型チェック含む）
- `npm test` — 既存テストが壊れないことを確認
- 手動確認: テーマ反映と show_icons 初期化が従来どおり動くこと

### 検証コマンド

```bash
npm run build    # 必須: typecheck + vite build
npm test         # 必須: 既存テスト維持
```

## SPEC.md 更新要否

**不要**。外部挙動の変更なし（内部の IPC 呼び出し回数削減のみ）。

## セルフレビュー

### 1. 対称コードパス
- `applyTheme` は MainApp / ResultsApp の両方で呼ばれていた → ResultsApp の bootstrap 内呼び出しを ResultsWindow に移動するだけ。MainApp 側に影響なし
- `visual-config-changed` リスナーは ResultsApp.tsx に残る（テーマ変更時の継続反映）
- **実装時の注意**: ResultsApp.tsx から削除するのは `getBootstrapPayload` の import・呼び出し・try/catch のみ。`applyTheme` import と `visual-config-changed` リスナーは残すこと（symmetric-check で確認済み）

### 2. 影響範囲の網羅性
- `getBootstrapPayload` の呼び出し箇所: MainApp.tsx:168、ResultsApp.tsx:20、ResultsWindow.tsx:93 → ResultsApp.tsx:20 を削除、ResultsWindow.tsx:93 にテーマ適用を追加
- `applyTheme` の呼び出し箇所: MainApp.tsx:169、ResultsApp.tsx:21、ResultsApp.tsx:14（イベントリスナー内） → ResultsApp.tsx:21 を削除し ResultsWindow.tsx に移動。イベントリスナー内の呼び出しは残す

### 3. 境界条件
- bootstrap 取得失敗時: 既存の catch で warn ログ。テーマもアイコンもデフォルト値で動作（現状と同じ）
- results ウィンドウが先に描画される場合: bootstrap は非同期なので、どちらにしても初期描画後にテーマが適用される（現状と同じ）

### 4. リソース管理
- 新規リソースの追加なし。既存リスナーの構造変更なし

### 5. 既存パターンとの整合
- 新規パターンの導入なし。既存の bootstrap 呼び出しパターンを流用

### 6. YAGNI 違反
- なし。要求範囲（bootstrap IPC 削減）に限定した最小変更

### 7. シンプル化の挑戦
- 新たな状態・型・コマンドを一切導入しない最もシンプルな案を選択
- 「ResultsWindow の bootstrap でテーマも適用する」という1行追加が変更の本質

### 8. 破壊不変条件の明示
- **テーマ未適用で results が表示される可能性**: bootstrap が遅れた場合、デフォルト CSS が見える。ただしこれは現状でも起こりうる（ResultsApp の bootstrap も非同期）。かつ results ウィンドウは初回は非表示で作成され、表示前に bootstrap が完了する可能性が高い
- **検知**: 手動確認（テーマが反映されることを目視）
