# 調査（issue #544）

## issue の要約

#536 で新設した `lib/ownedTimer.ts`（`createOwnedTimer(ms)`: `arm`/`cancel`/`isPending`）を、#536 のスコープ外として残った「所有者の曖昧な生タイマー」3箇所へ流用する。すべて trailing-only（leading policy なし）のため OwnedTimer をそのまま流用できる。挙動不変・公開 API 不変。

対象:
- `ui/src/components/SearchWindow.tsx`: focus retry timers（120ms/280ms の2本）
- `ui/src/MainApp.tsx`: `blurTimer`（100ms）/ `moveTimer`（500ms）
- `ui/src/stores/launchNotice.ts`: `launchNoticeTimer`（可変 delayMs: 2400/3000/5000）

## 関連コード

### `lib/ownedTimer.ts`（現状の契約）

- `createOwnedTimer(ms: number): OwnedTimer` — `ms` はインスタンス生成時に固定。
- `arm(fn: () => void): void` — 保留中タイマーを破棄し `ms` 後に `fn` を1回呼ぶ。**ms のオーバーライドは無い**。
- `cancel(): void` — 冪等。teardown 兼務。
- `isPending(): boolean`。
- 設計方針（JSDoc）: timer resource のみを所有し、debounce policy（leading 等）は持たない。fn は setTimeout の中でしか呼ばれず同期発火しない＝再入安全。

既存利用例（#536 で導入済み）:
- `stores/search.ts`: `refreshTimer = createOwnedTimer(DEBOUNCE_MS)`（50ms）。leading は `!refreshTimer.isPending()` から呼び出し側で導出し、trailing は `refreshTimer.arm(() => void runRefresh())`。`cancelDebounce()` は `refreshTimer.cancel()` に委譲。
- `stores/instantCommand.ts`: `fetchTimer = createOwnedTimer(INSTANT_CMD_DEBOUNCE_MS)`（30ms）。`fetchTimer.arm(() => { void deps.run(async (...) => {...}); })`。

### `SearchWindow.tsx`（L36-77）

- `focusRafHandle: number | undefined` — `requestAnimationFrame` ハンドル（2フレーム defer）。**OwnedTimer の対象外**（setTimeout ではない）。
- `focusRetryTimers: ReturnType<typeof setTimeout>[] = []` — 配列に2本の `setTimeout` ハンドルを push（120ms・280ms）。`clearFocusRetryTimers()` で全クリア + 配列を空にする。
- `focusInputWithRetries()`: `clearFocusRetryTimers()` → `focusInputSoon()`（即時）→ 120ms/280ms 後にそれぞれ `focusInputSoon()` を再実行する2本の setTimeout を push。
- 呼び出し元: `window-shown` イベント、`onFocusChanged(focused=true)`、初期可視性チェック（起動タイミングのフォールバック）。`onFocusChanged(focused=false)` で `clearFocusRetryTimers()`。

**2本同時に保留しうる**ため、OwnedTimer 1インスタンスでは足りない（arm は「前回の保留を破棄」する1本持ちのため、120ms 用と280ms 用で別インスタンスが必要）。

### `MainApp.tsx`

- `blurTimer`（L40, L66, L79, L269）: `onFocusChanged(focused=false)` で 100ms 後に `hideMain()` を呼ぶ。`blurCancelled` フラグを併用（`focused=true` で `blurCancelled=true` + `clearTimeout(blurTimer)`、blur 時に `blurCancelled=false` に戻す）。
  - **観察**: `blurCancelled` は `clearTimeout` と常に同期して更新されるため、JS のシングルスレッド性質上 `clearTimeout` 後に fn 本体が走ることは無く、`if (blurCancelled) return;` チェックは論理的に到達不能（dead code）。ただし本 issue のスコープは primitive 載せ替えのみ・挙動不変が要件のため、このチェックは維持する（削除は別 issue の判断）。
- `moveTimer`（L41, L151-157, L270）: `onMoved` で 500ms debounce 後に `api.saveSearchPlacement()` を呼ぶ。`latestMoveEvent` カウンタで `moveEvent !== latestMoveEvent` を fn 内でチェックしているが、これも `clearTimeout` 直後の再セットで同様に到達不能な保険（`blurCancelled` と同型の冗長性）。同じ理由で維持する。
- `onCleanup`（L269-270）で両方 `clearTimeout`。

### `stores/launchNotice.ts`

- `launchNoticeTimer: ReturnType<typeof setTimeout> | undefined`（L8）。
- `clearLaunchNotice()`: pending なら `clearTimeout` + `undefined` 化 + 通知クリア。
- `setLaunchNoticeWithAutoClear(message, delayMs = 2400)`: `clearLaunchNotice()` → 通知セット → `delayMs` 後に自動クリア。
- **`delayMs` は呼び出し元ごとに可変**（`grep` で実測）:
  - `notifyLaunchFailure`（本ファイル内 L34-41）: 引数省略 → デフォルト `2400`。
  - `setHotkeyFailureNotice`（本ファイル内 L29-31）: `5000`。
  - `lib/commands.ts` L45-48（`/o` コマンド、indexing 中の openSettings 失敗）: `3000`。
  - 呼び出し元は他に `stores/search.ts`（re-export のみ、直接呼び出しなし）。

**`createOwnedTimer(ms)` は ms をインスタンス生成時に固定する契約のため、可変 `delayMs` をそのまま渡せない。** #536 の issue 本文でも launchNoticeTimer は「参考: 同種だが本 issue の主対象ではない」として明示的にスコープ外にされており、可変 ms への対応は #536 時点で未検討。

## 既存パターン

- `arm`/`cancel` のペアリングは `search.ts`/`instantCommand.ts` で確立済み（対称的な resource 生成/破棄パターン）。
- 複数タイマーを同時に持つ既存パターンは無い（既存2利用例はいずれも単一インスタンス）。focus retry の「2本同時保留」は新規の利用形態。
- 可変 duration を持つタイマーの既存パターンも無い。

## 技術的制約

- Win32 API 依存なし（純粋 JS/TS の setTimeout ラップ）。
- `arm(fn)` は `fn: () => void` 型。async 関数を直接渡すと TypeScript の「void 返り値関数への代入は戻り値の型を問わない」規則により型検査は通るが、既存コードは `() => void asyncCall()` の形で明示的に `void` 演算子を使う規約（`search.ts`/`instantCommand.ts` で確認済み）。新規移植箇所もこの規約に合わせる。
- `ownedTimer.ts` の JSDoc は「timer resource のみを所有し policy を持たない」と明記。`launchNoticeTimer` の可変 ms 対応で `arm` にオーバーライド引数を足すことは、「ms（duration）」は resource 側の属性であり leading 等の policy ではないため、この設計方針と矛盾しない（要 `arm` シグネチャの後方互換な拡張）。

## 未解決の疑問

- `arm` に `msOverride?: number` を追加するかどうかは実装判断（既存2呼び出し元は非破壊的拡張の影響を受けない）。plan.md で確定する。
