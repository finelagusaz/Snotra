# plan.md — issue #361

frontend hide 経路の `EmptyWorkingSet` trim を `win.hide()` 完了後に走らせ、回収を hotkey 経路
（~9MB）に近づける。**案A**（JS 側順序入れ替え + `hideMainWindow()` への統合）を採用。

## 変更ファイル一覧

### 1. `ui/src/lib/commands.ts` — 順序入れ替え（中核修正）

`hideMainWindow()` を `hide` → `notify` の順に直す。

```ts
/** main webview コンテキストから呼び出すこと（getCurrentWindow() が main を返す前提）。
 *  全 frontend hide（Escape / Enter / Shift+Enter / クリック起動 / フォーカス喪失 / /s）の
 *  単一チョークポイント。notifyMainHidden() の trim（EmptyWorkingSet）は win.hide() 完了後に
 *  走らせる——可視中に trim するとレンダラがページを再 touch し回収が削がれるため
 *  （hotkey 経路と同じ hide→trim 順に揃える。issue #361）。 */
export async function hideMainWindow() {
  await getCurrentWindow().hide();
  api.notifyMainHidden().catch(() => {});
}
```

### 2. `ui/src/MainApp.tsx` — インライン 2 経路を hideMainWindow へ統合（DRY）

import 追加:
```ts
import { hideMainWindow } from "./lib/commands";
```

`hideMain()`（フォーカス喪失, ~L50）:
```ts
const hideMain = async () => {
  if (!mainVisible()) return;
  setMainVisible(false);   // eager: ResultsSection を即座に畳み Blob URL を hide 前に解放
  await hideMainWindow();  // hide → trim の順は hideMainWindow に集約
};
```

`handleClickResult()`（クリック起動, ~L287）の launched ブロック:
```ts
if (launched) {
  setMainVisible(false);
  void hideMainWindow();
} else {
  console.warn("Failed to launch clicked result", { index });
}
```

- `setMainVisible(false)` は**残す**: MainApp ローカルシグナルで、eager に呼ぶことで
  `ResultsSection visible` が即 false → `cache.revokeAll()` が hide 前に走る（trim 前に Blob 解放）。
  commands.ts からはこのシグナルに触れないため MainApp 側に残すのが正しい責務分離。
- `api.notifyMainHidden()` / `win.hide()` のインライン呼び出しは削除（hideMainWindow に集約）。

### 3. `ui/src/lib/commands.test.ts` — 順序不変条件のユニットテスト追加（Red→Green）

```ts
import { findCommand, hideMainWindow, SLASH_COMMANDS } from "./commands";

describe("hideMainWindow", () => {
  beforeEach(() => vi.clearAllMocks());

  it("hide 完了後に notifyMainHidden(trim) を呼ぶ（#361: 可視中 trim 回避）", async () => {
    const order: string[] = [];
    mockMainHide.mockImplementation(async () => { order.push("hide"); });
    vi.mocked(api.notifyMainHidden).mockImplementation(async () => { order.push("notify"); });

    await hideMainWindow();

    expect(order).toEqual(["hide", "notify"]);
  });
});
```
- 既存コード（notify→hide）では `["notify","hide"]` となり **落ちる**（Red 確認）。
- 修正後 `["hide","notify"]` で **通る**（Green）。
- 全 frontend hide が `hideMainWindow()` に集約されるため、この 1 テストで 3 経路すべての
  順序不変条件を守れる。

### 4. ドキュメント同期

- `ui/CLAUDE.md` 「実装パターン」に `hideMainWindow()` = 全 frontend hide の単一チョークポイント・
  `hide → notify(trim)` 順である旨を 1 行追記。
- `src-tauri/CLAUDE.md` は無変更（`notify_main_hidden` 不変・「全 hide 経路に適用」も真）。
- SPEC.md 無変更（状態遷移不変）。

## 実装順序（フェーズ）

1. **Phase 1**: commands.test.ts に順序テスト追加 → 実行して Red 確認。
2. **Phase 2**: commands.ts の `hideMainWindow()` 順序入れ替え → テスト Green 確認。
3. **Phase 3**: MainApp.tsx の 2 経路を hideMainWindow へ統合（import 追加・インライン削除）。
4. **Phase 4**: ドキュメント同期（ui/CLAUDE.md）。
5. **Phase 5**: 検証（下記）。

## 不変条件

- **順序不変条件**: `hideMainWindow()` は `win.hide()` の解決後にのみ `notifyMainHidden()` を呼ぶ。
  → commands.test.ts で機械的に保証。
- **Blob URL ライフサイクル**: `cache.revokeAll()` は **signal 駆動**（event 駆動ではない）。
  `ResultsSection visible = shouldShowResults() && mainVisible()` が false → `iconsEnabled` メモ
  が false → `cache.revokeAll()`（ResultsSection.tsx ~L105）。
  - MainApp 経路: eager `setMainVisible(false)` で hide 前に駆動（変更なし）。
  - commands.ts 経路: window-hidden イベント受信が `setMainVisible(false)` を駆動（hide 後）。
    window 非可視後の解放のため視覚影響なし。revoke は必ず実行されリークなし。
- **`main_visible` フラグ**: hide 後に false 化（数 ms〜~35ms 遅延）。hotkey hide は冪等のため
  この窓で hotkey が押されても機能破壊なし（show 意図が 1 回空振りしうる極小窓のみ。既存の
  fire-and-forget IPC 往復遅延でも同等の窓は存在）。
- **best-effort 維持**: trim 失敗・notify 失敗（`.catch(() => {})`）は機能に影響しない。
- **異常系**: `win.hide()` が reject した場合 → `notifyMainHidden()` は呼ばれない
  （`await` で例外伝播）。現状は notify が先に走るため、reject 時に notify だけ走る不整合があった。
  reorder により「hide 成功時のみ trim」となり整合性は**向上**。hide reject は通常発生しないが、
  発生時は窓が可視のまま残り次の hide 試行で再度処理される（回復可能）。

## テスト方針

- **追加テスト**: `ui/src/lib/commands.test.ts` の `hideMainWindow` describe（順序不変条件）。
- **検証コマンド**（docs/build-commands.md カテゴリ準拠。.ts/.tsx 変更 = フロントエンド）:
  - `npm run typecheck`（TypeScript）
  - `npm test`（vitest: commands.test.ts / SearchWindow.test.tsx が緑のまま）
  - `npm run lint`
  - `npm run smoke:startup`（起動スモーク）
  - release build（`npm run tauri build` 相当）で目視ライフサイクル確認。
- **手動計測（issue 必須項目「クリーンな再計測」）**: release build を起動し、検索を実行せず
  フォーカス喪失で hide → working set を計測。frontend 経路が hotkey 経路（~9MB）に近づくことを確認。
  → ユニット/smoke では代替不能。実装後にユーザーへ手動計測を依頼 or 環境が許せば実施。

## SPEC.md 更新要否

不要（状態遷移・IPC 契約・状態フラグの文書化挙動は不変。trim タイミングは実装詳細）。

## セルフレビュー

### Step 5a — plan-review（並列 Explore × 2: TS フロント / Rust 境界）

- **要対処: 0 件。** 両レイヤーとも計画は堅実、見落としなしと判定。
- **対称ペア（show/hide）**: Rust 境界エージェントが専項検証 →「問題なし」。
  show（`show_main_and_emit`: ... → `show()` → `main_visible.store(true)` → `emit(window-shown)`）と
  hotkey hide（`w.hide()` → `main_visible.store(false)` → `emit(window-hidden)` → suspend → trim）は
  どちらも「visibility 変更 → flag 更新 → emit」順で対称。frontend hide も `notify_main_hidden` に
  集約され全 hide が Rust 一元管理に揃う。hotkey 経路無変更は正当（同期 main thread・suspend 要件）。
- **`main_visible` レース**: hide 完了後に notify するため Win32 `IsVisible` は既に false。stale
  `main_visible=true` 窓で hotkey が押されても hide は冪等（無害）。**現状の fire-and-forget IPC でも
  同等窓は既存**で、reorder は機能破壊しない。異常系（hide reject）整合性はむしろ向上。
- **軽微（実装時対応）**:
  1. commands.test.ts に `hideMainWindow` を import 追加（Phase 1 で対応）。
  2. ui/CLAUDE.md の追記位置は `lib/` セクションの commands.ts 行に明示（Phase 4 で対応）。

### Step 5b — セルフレビューチェックリスト

1. **対称コードパス**: ✓ show/hide を 5a で検証済み（問題なし）。
2. **影響範囲の網羅性**: ✓ `hideMainWindow`/`notifyMainHidden`/`win.hide()` を grep。frontend hide は
   commands.ts(1) + MainApp.tsx(2) の 3 経路のみ。SearchWindow の Escape/Enter/Shift+Enter は既に
   hideMainWindow 経由。
3. **境界条件**: ✓ hide reject 時・フォーカス喪失 debounce 中の hotkey・二重 hide を検証済み。
4. **リソース管理**: ✓ Blob URL revoke（signal 駆動）が reorder 後も確実に実行。新規 listen/timer/
   プロセス導入なし（破棄ペア不要）。
5. **既存パターン整合**: ✓ `hideMainWindow()` 既存関数への集約。新パターン導入なし。
6. **YAGNI**: ✓ 順序入れ替え + DRY 統合のみ。案B（タイマー defer）の複雑性を回避。
7. **シンプル化**: ✓ 新状態（AtomicBool/Mutex/子プロセス）導入なし。タイマー不要。「hide が reject
   したら notify されない＝trim されない」を異常系として明記済み。
8. **破壊不変条件**: hotkey toggle が `main_visible` に依存（Win32 フック系の「戻ってこない」リスクは
   本変更にはない＝Rust 側無変更）。検知手段: commands.test.ts 順序テスト + smoke:startup +
   手動ライフサイクル（フォーカス喪失/Escape/クリック/hotkey の全経路で hide→再表示）。
