# Research: エラーハンドリングの残骸分析

## 背景

パフォーマンス最適化 (#116) 等の大規模リファクタを経て、エラーハンドリングがリファクタ前の前提のまま残っている箇所がある。`fetchIcons` の silent catch (#149 で発見) と同種のパターンをコードベース全体で調査した。

## 分類基準

- **要対処**: エラーが握りつぶされ、障害時の診断手がかりが失われる
- **許容**: fire-and-forget が意図的で、失敗しても UX に影響しない
- **良好**: 適切にログ出力またはリカバリされている

---

## 要対処: silent catch / 未ハンドル Promise

### 1. ResultsWindow.tsx — listen 登録の未ハンドル (3箇所)

| 箇所 | 行 | パターン |
|---|---|---|
| `listen("show-icons-changed", ...)` | L107-113 | `.then(fn => { unlisten = fn })` のみ、`.catch()` なし |
| `listen("visual-config-changed", ...)` | L124-129 | 同上 |
| `listen("results-sync", ...)` | L144-178 | 同上 |

**失われる情報**: listen 登録自体が失敗した場合、`unlisten` が未代入のまま残り、cleanup が no-op になる。加えてエラーログなし。

**リスク**: 低〜中。listen 登録の失敗は極めてまれだが、失敗時にリスナーリークする。

### 2. ResultsWindow.tsx — getBootstrapPayload 未ハンドル

```tsx
// L99-101
void api.getBootstrapPayload().then((bootstrap) => {
  setShowIcons(bootstrap.appearance.show_icons);
});
```

**失われる情報**: ブートストラップ取得失敗時、`showIcons` がデフォルト値のまま固定。エラーログなし。

**リスク**: 低。起動直後の1回きりの呼び出しで、IPC が動作していれば失敗しない。

### 3. SearchWindow.tsx — listen 登録の未ハンドル (2箇所)

| 箇所 | 行 | パターン |
|---|---|---|
| `listen("window-shown", ...)` | L66-71 | `.then(unlisten => ...)` のみ |
| `getCurrentWindow().onFocusChanged(...)` | L72-83 | 同上 |

**失われる情報**: #1 と同構造。listen 失敗時に cleanup 不能 + エラーログなし。

### 4. App.tsx — saveSearchPlacement 未ハンドル

```tsx
// L150-153
void (async () => {
  if (moveEvent !== latestMoveEvent) return;
  await api.saveSearchPlacement(Math.round(logicalPos.x), Math.round(logicalPos.y));
})();
```

**失われる情報**: ウィンドウ位置の永続化失敗がログなしで消える。次回起動時にウィンドウ位置がリセットされるが原因不明になる。

**リスク**: 低。保存先は config.toml で、書き込み失敗は通常ありえない。

---

## 許容: 意図的な fire-and-forget

### 5. search.ts — recordFolderExpansion (L299)

```tsx
void api.recordFolderExpansion(dir);
```

**判断**: 非クリティカルな履歴記録。失敗しても検索動作に影響なし。

### 6. commands.ts — quitApp (L57)

```tsx
api.quitApp();
```

**判断**: アプリ終了コマンド。失敗してもプロセスは終了途中であり、ログを見る機会がない。

### 7. App.tsx — hideMainAndResults / handleMainMoved (L90, L158)

```tsx
void hideMainAndResults();
void controller.handleMainMoved(logicalPos);
```

**判断**: UI 操作の fire-and-forget。hideMainAndResults は内部で try/catch なしだが、Tauri ウィンドウ API の失敗は致命的ではない。

### 8. E2E テスト内の .catch(() => {}) (複数箇所)

**判断**: テストハーネスのクリーンアップ。プライマリエラーを隠さないための意図的な握りつぶし。

---

## 良好: 適切なハンドリング

| 箇所 | 行 | パターン |
|---|---|---|
| `search.ts` refreshResults | L351-354 | `.catch(e => { trace(...); console.error(...) })` |
| `resultsWindowController.ts` position apply | L70-82 | `.catch(e => { console.error(...); state cleanup })` |
| `resultsWindowController.ts` geometry init | L201-219 | `try/catch` + `console.warn` |

---

## Rust 側の状況

Rust コードベースは概ね良好。

- `src-tauri/src/commands/launch.rs`: エラーパスは `LaunchResult` で型安全に返却
- `src-tauri/src/commands/window.rs`: `let _ = set_window_no_activate(...)` 等はウィンドウ装飾の非クリティカル操作で許容
- `src-tauri/src/main.rs`: `show_main_and_emit` の失敗はログ出力済み

---

## 総括

| 分類 | 件数 | 代表パターン |
|---|---|---|
| 要対処 | 4 | listen 登録の `.catch()` 欠如、Promise 未ハンドル |
| 許容 | 4 | fire-and-forget（履歴記録、quit、hide） |
| 良好 | 3 | console.error/warn + 状態リカバリ |

**共通パターン**: `void listen(...).then(fn => { unlisten = fn })` が最も多い問題パターン（5箇所）。listen 登録は通常失敗しないが、失敗時にリスナーリーク + 診断不能になる。一括で `.catch(e => console.warn(...))` を付与するのが費用対効果が高い。

**Rust 側は対処不要**。問題はフロントエンド（TypeScript）に集中している。
