# コードレビュー結果

**対象**: 未コミットの変更（7ファイル、+90/-15行）
**日付**: 2026-02-26
**ブランチ**: main

---

## Critical

### 1. blur ハンドラの非同期レースコンディション

**ファイル**: `ui/src/App.tsx` L102-110

`setTimeout` のコールバックを `async` に変更しているが、`clearTimeout` は既に実行開始された非同期処理をキャンセルできない。

**再現シナリオ**:

1. blur 発生 → 100ms 経過 → async コールバック開始
2. `getByLabel` の IPC 待機中にユーザーがフォーカスを戻す
3. `clearTimeout` が呼ばれるが既に遅い
4. `isVisible()` の結果で `hideMainAndResults()` が誤発火

**修正案**: キャンセルフラグの導入

```typescript
let blurCancelled = false;
win.onFocusChanged(({ payload: focused }) => {
  if (!focused) {
    blurCancelled = false;
    blurTimer = setTimeout(async () => {
      if (blurCancelled) return;
      const sw = await WebviewWindow.getByLabel("settings");
      const aw = await WebviewWindow.getByLabel("about");
      if (blurCancelled) return;
      const settingsVisible = sw && await sw.isVisible();
      const aboutVisible = aw && await aw.isVisible();
      if (blurCancelled) return;
      if (!settingsVisible && !aboutVisible) {
        void hideMainAndResults();
      }
    }, 100);
  } else {
    blurCancelled = true;
    clearTimeout(blurTimer);
  }
});
```

---

## High

### 2. async setTimeout 内の未ハンドル Promise rejection

**ファイル**: `ui/src/App.tsx` L102-110

`getByLabel()` や `isVisible()` がウィンドウ破棄タイミングで throw した場合、try/catch がないため unhandled rejection になる。

**修正案**: async コールバック全体を try/catch で囲む

```typescript
blurTimer = setTimeout(async () => {
  try {
    const sw = await WebviewWindow.getByLabel("settings");
    // ...rest of logic
  } catch (e) {
    console.warn("auto-hide focus check failed:", e);
  }
}, 100);
```

### 3. AboutWindow の `onKeyDown` が発火しない可能性

**ファイル**: `ui/src/components/AboutWindow.tsx` L18-19

`<div>` はデフォルトでフォーカス不可能。`SettingsWindow` はボタンや入力要素にフォーカスが当たるためバブルアップで動作するが、`AboutWindow` はリンクしかなく、空白領域をクリックした場合 Escape が効かない。

**修正案A**: `tabIndex` + 自動フォーカス

```tsx
<div tabIndex={0} onKeyDown={handleKeyDown} ref={(el) => el.focus()} ...>
```

**修正案B**: window レベルリスナー（`ui/CLAUDE.md` のリソース管理パターンに合致）

```typescript
import { onMount, onCleanup } from "solid-js";
onMount(() => {
  const handler = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      void getCurrentWindow().close();
    }
  };
  window.addEventListener("keydown", handler);
  onCleanup(() => window.removeEventListener("keydown", handler));
});
```

---

## Medium

### 4. `handleKeyDown` の重複

**ファイル**: `AboutWindow.tsx` L10-15 / `SettingsWindow.tsx` L23-28

完全に同一の Escape-to-close ロジックが2箇所に存在する。プロジェクトの DRY 原則（「2回まで許容、3回目で抽出」）により現時点では許容範囲だが、今後他のウィンドウが増える場合は共通ユーティリティへの抽出を検討すべき。

### 5. `hideAllWindowsFn` / `initCommands` の部分的な死コード化

**ファイル**: `ui/src/lib/commands.ts` L11

`/a` と `/o` が `hideAllWindowsFn` を使わなくなり、直接 `WebviewWindow` を参照する方式に変更された。一方 `/s` は依然 `hideAllWindowsFn` を使用。隠蔽戦略が不統一になっており保守時に混乱を招く。

**修正案**:
- `/s` も `WebviewWindow` 直接参照に統一し、`hideAllWindowsFn` / `initCommands` を削除する
- もしくは `/a`・`/o` も注入パターンに揃える

### 6. `parseFloat` の NaN ガード不在

**ファイル**: `ui/src/components/SettingsSearch.tsx` L100-103

入力欄を空にすると `parseFloat("")` が `NaN` を返し、そのまま設定ドラフトに格納される。バックエンドに `NaN` が渡ると予期しない動作になる。

**修正案**:

```typescript
onInput={(e) => {
  const val = parseFloat(e.currentTarget.value);
  if (!Number.isNaN(val)) {
    updateDraft((c) => {
      c.search.fuzzy_history_cap_ratio = Math.max(0, Math.min(1, val));
    });
  }
}}
```

### 7. `stopPropagation` のスコープ

**ファイル**: `ui/src/components/SettingsGeneral.tsx` L96-97

ホットキー入力フィールドで Escape/Backspace のみ `stopPropagation` しているが、他のキーは伝播する。現状のコードでは問題ないが、SettingsWindow の `handleKeyDown` と組み合わせた際の意図が暗黙的。コメントの補足は適切。

---

## Low

### 8. テストで `getByLabel` の引数を検証していない

**ファイル**: `ui/src/lib/commands.test.ts` L5-10

モックがどのラベルでも同じオブジェクトを返すため、`hideResultsWindow()` 内のラベル文字列 `"results"` が誤っていても検出できない。

**修正案**:

```typescript
expect(WebviewWindow.getByLabel).toHaveBeenCalledWith("results");
```

### 9. blur ハンドラの挙動変更に関する意図確認

**ファイル**: `ui/src/App.tsx` L105-108

settings/about が表示中であれば、ユーザーが無関係な外部アプリに切り替えても main ウィンドウが隠れなくなる。これが意図的な仕様か確認が必要。

### 10. `void` vs `await` の不統一

**ファイル**: `ui/src/App.tsx` L108

async コールバック内で `hideMainAndResults()` を `void`（fire-and-forget）で呼んでいる。async 関数内であれば `await` にして try/catch で捕捉可能にする方が安全。

---

## 対処優先度まとめ

| # | 重要度 | ファイル | 問題 |
|---|--------|----------|------|
| 1 | **Critical** | `App.tsx` | 非同期レースコンディション — キャンセルフラグ必須 |
| 2 | **High** | `App.tsx` | try/catch 不在による unhandled rejection |
| 3 | **High** | `AboutWindow.tsx` | `<div>` がフォーカス不可で Escape が効かない |
| 4 | Medium | 2ファイル | `handleKeyDown` の重複（現時点は許容範囲） |
| 5 | Medium | `commands.ts` | `hideAllWindowsFn` の不統一 |
| 6 | Medium | `SettingsSearch.tsx` | `NaN` ガード不在 |
| 7 | Medium | `SettingsGeneral.tsx` | `stopPropagation` スコープ（現状問題なし） |
| 8 | Low | `commands.test.ts` | ラベル引数の検証不足 |
| 9 | Low | `App.tsx` | 外部アプリ切替時の挙動変更 |
| 10 | Low | `App.tsx` | `void` vs `await` の不統一 |
