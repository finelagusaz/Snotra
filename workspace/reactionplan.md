# レビュー指摘対応 実装計画

**基準**: `review.md` の指摘 10 件に対し、コードベースの現状と照合した修正方針を示す。

---

## 修正 1: blur ハンドラの非同期レースコンディション + 未ハンドル rejection（Critical + High）

**対象**: `ui/src/App.tsx` L98-115 `registerAutoHideOnFocusLost`

**根本原因**: `setTimeout` のコールバックを `async` にしたが、`clearTimeout` は実行開始後の async 処理をキャンセルできない。加えて try/catch がないため IPC 失敗時に unhandled rejection が発生する。

**修正方針**: キャンセルフラグ + try/catch の導入（review.md #1, #2 を同時解決）

```typescript
// ui/src/App.tsx L98-115 を以下に置換
registerAutoHideOnFocusLost = () => {
  let blurTimer: ReturnType<typeof setTimeout> | undefined;
  let blurCancelled = false;
  win.onFocusChanged(({ payload: focused }) => {
    if (!focused) {
      blurCancelled = false;
      blurTimer = setTimeout(async () => {
        try {
          if (blurCancelled) return;
          const sw = await WebviewWindow.getByLabel("settings");
          const aw = await WebviewWindow.getByLabel("about");
          if (blurCancelled) return;
          const settingsVisible = sw && await sw.isVisible();
          const aboutVisible = aw && await aw.isVisible();
          if (blurCancelled) return;
          if (!settingsVisible && !aboutVisible) {
            await hideMainAndResults();
          }
        } catch (e) {
          console.warn("auto-hide focus check failed:", e);
        }
      }, 100);
    } else {
      blurCancelled = true;
      clearTimeout(blurTimer);
    }
  });
};
```

**変更点**:
- `blurCancelled` フラグを追加し、各 `await` の直後でチェック
- focus 復帰時に `blurCancelled = true` をセット
- async コールバック全体を try/catch で囲む
- `void hideMainAndResults()` を `await hideMainAndResults()` に変更し、try/catch で捕捉可能にする（review.md #10 も同時解決）

**検証**: `npx vite build`

---

## 修正 2: AboutWindow の Escape キーが効かない問題（High）

**対象**: `ui/src/components/AboutWindow.tsx` L10-19

**根本原因**: `<div>` はデフォルトでフォーカスを受け取れない。`SettingsWindow` はボタン・入力要素を持つためバブルアップで動作するが、`AboutWindow` はリンクのみで空白部分をクリックすると Escape が効かない。

**修正方針**: `window.addEventListener` + `onCleanup` パターンに変更（`ui/CLAUDE.md` のリソース管理パターンに準拠）

```typescript
// ui/src/components/AboutWindow.tsx
import { type Component, onMount, onCleanup } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-shell";

const AboutWindow: Component = () => {
    // ...既存の定数定義...

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

    return (
        <div
            // onKeyDown を削除
            style={{...}}
        >
```

**変更点**:
- `solid-js` から `onMount`, `onCleanup` を追加 import
- `handleKeyDown` 関数を削除し、`onMount` 内で `window.addEventListener` に置換
- `<div>` から `onKeyDown` 属性を削除
- `onCleanup` で確実にリスナーを解除

**同時修正（SettingsWindow の対称ペア確認）**: `SettingsWindow.tsx` も同様に `<div onKeyDown>` パターンを使っている。SettingsWindow はボタンにフォーカスが当たるため現状動作するが、一貫性のため同じ `window.addEventListener` パターンに揃える。

```typescript
// ui/src/components/SettingsWindow.tsx
import { type Component, Show, onMount, onCleanup } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";

const SettingsWindow: Component = () => {
    onMount(() => {
        loadDraft();
        const handler = (e: KeyboardEvent) => {
            if (e.key === "Escape") {
                e.preventDefault();
                void getCurrentWindow().close();
            }
        };
        window.addEventListener("keydown", handler);
        onCleanup(() => window.removeEventListener("keydown", handler));
    });

    return (
        <div class="settings-window">
            // onKeyDown を削除
```

**変更点**:
- `solid-js` の import に `onCleanup` を追加
- `handleKeyDown` 関数を削除し、`onMount` 内で `window.addEventListener` に置換
- `<div>` から `onKeyDown` 属性を削除

**注意（SettingsGeneral.tsx との相互作用）**: SettingsGeneral L96-97 の `e.stopPropagation()` はバブルアップを止める目的だが、`window.addEventListener` はキャプチャ/バブルではなく直接 window で受けるため、`stopPropagation` では止められない。ホットキー入力中の Escape を window リスナーが拾わないよう、window リスナー側で「ホットキー入力フィールドにフォーカスがある場合はスキップ」するガードが必要。

```typescript
// SettingsWindow.tsx の window listener に追加
const handler = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
        // ホットキー入力中は window-close を抑止
        const active = document.activeElement;
        if (active?.classList.contains("hotkey-input")) return;
        e.preventDefault();
        void getCurrentWindow().close();
    }
};
```

**検証**: `npx vite build`

---

## 修正 3: `parseFloat` の NaN ガード追加（Medium）

**対象**: `ui/src/components/SettingsSearch.tsx` L100-103

**根本原因**: 入力欄を空にすると `parseFloat("")` → `NaN` が設定ドラフトに格納される。

**修正方針**: NaN チェックとクランプ

```typescript
// ui/src/components/SettingsSearch.tsx L100-104 を置換
onInput={(e) => {
  const val = parseFloat(e.currentTarget.value);
  if (!Number.isNaN(val)) {
    updateDraft((c) => {
      c.search.fuzzy_history_cap_ratio = Math.max(0, Math.min(1, val));
    });
  }
}}
```

**検証**: `npx vite build`

---

## 修正 4: `hideAllWindowsFn` / `initCommands` の不統一解消（Medium）

**対象**: `ui/src/lib/commands.ts`, `ui/src/components/SearchWindow.tsx`

**根本原因**: `/a`・`/o` が `hideResultsWindow()` を直接呼ぶようになったが、`/s` は依然 `hideAllWindowsFn?.()` 経由。隠蔽戦略が不統一。

**修正方針**: `/s` も直接 `WebviewWindow` を使う方式に統一し、`hideAllWindowsFn` / `initCommands` を削除する。

`/s`（インデックス再構築）は全ウィンドウ（main + results）を隠す必要がある。`SearchWindow.tsx` の `hideAllWindows()` と同等の処理を `commands.ts` 内に移す。

```typescript
// ui/src/lib/commands.ts — 修正後全体
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { getCurrentWindow } from "@tauri-apps/api/window";
import * as api from "./invoke";

export interface SlashCommand { /* 変更なし */ }

async function hideResultsWindow() {
  const rw = await WebviewWindow.getByLabel("results");
  if (rw) await rw.hide();
}

async function hideAllWindows() {
  getCurrentWindow().hide();
  await hideResultsWindow();
}

export const SLASH_COMMANDS: SlashCommand[] = [
  // /r — 変更なし
  {
    command: "/a", label: "/a", description: "バージョン情報",
    action: async () => {
      await hideResultsWindow();
      await api.openAbout();
    },
  },
  {
    command: "/o", label: "/o", description: "設定を開く",
    action: async () => {
      await hideResultsWindow();
      await api.openSettings();
    },
  },
  {
    command: "/s", label: "/s", description: "インデックス再構築",
    action: async () => {
      await hideAllWindows();
      await api.rebuildIndex();
    },
  },
  // /q — 変更なし
];

// initCommands を削除
// findCommand, filterCommands — 変更なし
```

**連鎖修正**:
- `SearchWindow.tsx` L21: `initCommands` の import を削除
- `SearchWindow.tsx` L66: `initCommands(hideAllWindows);` 呼び出しを削除
- `SearchWindow.tsx` L25-31: `hideAllWindows` 関数を削除（`commands.ts` に移動済み。ただし `SearchWindow.tsx` L122-123, L169 で直接呼んでいるため、`commands.ts` から export して import し直す）

```typescript
// commands.ts に追加
export { hideAllWindows };

// SearchWindow.tsx — import 変更
import { hideAllWindows } from "../lib/commands";
// ローカルの hideAllWindows 定義を削除
```

**テスト修正** (`commands.test.ts`):
- `/s` テスト: `initCommands` 呼び出しを削除し、`getCurrentWindow().hide()` のモックと `WebviewWindow.getByLabel` モックで順序検証に変更
- `/q` テスト: `initCommands` 呼び出しを削除
- `initCommands` の import を削除

```typescript
// commands.test.ts — 修正概要
import { findCommand } from "./commands";  // initCommands を削除

// window モック追加
const mockMainHide = vi.fn(async () => {});
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({ hide: mockMainHide })),
}));

// /s テスト
it("/s hides all windows before rebuildIndex", async () => {
  const order: string[] = [];
  mockMainHide.mockImplementation(() => { order.push("hideMain"); });
  mockResultsHide.mockImplementation(async () => { order.push("hideResults"); });
  vi.mocked(api.rebuildIndex).mockImplementation(async () => {
    order.push("rebuildIndex");
    return true;
  });
  const cmd = findCommand("/s");
  await cmd!.action();
  expect(order).toEqual(["hideMain", "hideResults", "rebuildIndex"]);
});

// /q テスト — initCommands 呼び出しを削除するだけ
```

**検証**: `npm test` + `npx vite build`

---

## 修正 5: テストで `getByLabel` の引数を検証（Low）

**対象**: `ui/src/lib/commands.test.ts`

**修正方針**: `/a` と `/o` のテストに `getByLabel("results")` の呼び出し検証を追加。

```typescript
// /a テスト末尾に追加
expect(WebviewWindow.getByLabel).toHaveBeenCalledWith("results");

// /o テスト末尾に追加
expect(WebviewWindow.getByLabel).toHaveBeenCalledWith("results");
```

**検証**: `npm test`

---

## 修正 6: blur ハンドラの挙動確認（Low — 設計判断）

**対象**: `ui/src/App.tsx` L105-108（review.md #8）

**現状の挙動**: settings/about が表示中なら、ユーザーが外部アプリに切り替えても main ウィンドウが隠れない。

**判断**: これは意図的な設計変更であると推定する（settings/about を開いた状態で検索ウィンドウを残すのが目的）。ただし、settings/about を閉じた後も main が残り続けるケースがないか確認が必要。settings/about を閉じた時点で main のフォーカスは失われているため、`onFocusChanged` は再発火しない → main は表示されたまま残る。

**対策案**（実装するかはユーザー判断）: settings/about ウィンドウの close イベントをリッスンし、main ウィンドウが非フォーカスなら隠す。現時点では「意図確認済み」として保留とする。

---

## 実施順序

| 順序 | 修正 | 理由 |
|------|------|------|
| 1 | 修正 4（`initCommands` 統一） | 他の修正の前提となるコード整理 |
| 2 | 修正 1（blur レースコンディション + rejection） | Critical + High の同時解決 |
| 3 | 修正 2（AboutWindow / SettingsWindow の Escape） | High、SettingsGeneral との相互作用あり |
| 4 | 修正 3（NaN ガード） | 独立した修正 |
| 5 | 修正 5（テスト引数検証） | 修正 4 のテスト修正と同時に実施可能 |
| 6 | 修正 6（blur 挙動確認） | 設計判断、保留 |

## 検証チェックリスト

- [ ] `npx vite build` — フロントエンドビルド成功
- [ ] `npm test` — 全テストパス
- [ ] `npm run smoke:startup` — スモークテストパス（ウィンドウ生成に変更あり）
