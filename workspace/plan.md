# plan: issue #544 — 残りの component タイマーを OwnedTimer へ流用する

## 種別判定（AGENTS.md Step 0）

**refactor（挙動保存）**。SPEC.md 記載のフロー・IPC 契約・状態遷移は変えない。`ownedTimer.ts` の公開 API（`OwnedTimer` インターフェース）に後方互換な拡張を1点加える（`arm` に任意の `msOverride` 引数）。

## 変更ファイル一覧

### 1. `ui/src/lib/ownedTimer.ts`（primitive 拡張）

`launchNoticeTimer` の delayMs が呼び出し元ごとに可変（2400/3000/5000、research.md 参照）なため、`createOwnedTimer(ms)` の固定 ms だけでは表現できない。`arm` に**任意**の `msOverride` を追加する（後方互換・既存2呼び出し元は無変更で動く）:

```ts
export interface OwnedTimer {
  /** 保留中タイマーを破棄し、`msOverride ?? ms`（生成時の既定値）後に `fn` を1回だけ呼ぶ。 */
  arm(fn: () => void, msOverride?: number): void;
  cancel(): void;
  isPending(): boolean;
}

export function createOwnedTimer(ms: number): OwnedTimer {
  let timer: ReturnType<typeof setTimeout> | undefined;
  return {
    arm(fn, msOverride) {
      if (timer !== undefined) clearTimeout(timer);
      timer = setTimeout(() => {
        timer = undefined;
        fn();
      }, msOverride ?? ms);
    },
    cancel() { /* 既存のまま */ },
    isPending() { /* 既存のまま */ },
  };
}
```

JSDoc に「`msOverride` は呼び出し単位で ms を差し替える resource 属性であり、leading 等の policy ではない（設計方針と矛盾しない）」旨を追記する。

### 2. `ui/src/lib/ownedTimer.test.ts`（テスト追加）

`msOverride` の挙動を検証するケースを追加（Red→Green で先に書く）:
- `arm(fn, override)` で override 側の ms が使われる（生成時の ms は無視される）。
- `msOverride` 省略時は生成時の `ms` が使われる（既存ケースの回帰確認、変更不要）。
- 同一インスタンスへ異なる `msOverride` で連続 `arm` した場合、前回の保留が破棄され新しい override の ms で発火する（burst 相当）。
- `msOverride` に `0` を渡した場合も正しく `0` として扱われる（`msOverride ?? ms` は `??` のため `0` はフォールバックしない）。plan-review で指摘された境界値。

### 3. `ui/src/stores/launchNotice.ts`

```ts
import { createOwnedTimer } from "../lib/ownedTimer";

const [launchNotice, setLaunchNotice] = createSignal<string | null>(null);
const launchNoticeTimer = createOwnedTimer(2400); // 既定値。実際の ms は arm 側で毎回 override される

export function clearLaunchNotice() {
  launchNoticeTimer.cancel();
  if (launchNotice() !== null) {
    setLaunchNotice(null);
  }
}

export function setLaunchNoticeWithAutoClear(message: string, delayMs?: number) {
  clearLaunchNotice();
  setLaunchNotice(message);
  launchNoticeTimer.arm(() => {
    setLaunchNotice(null);
  }, delayMs);
}
```

`delayMs` は `= 2400` のデフォルト引数から `?: number`（省略可）へ変える。`launchNoticeTimer.arm(fn, undefined)` は `arm` 内部の `msOverride ?? ms` で生成時の `2400` にフォールバックするため、**2400 という値は `createOwnedTimer(2400)` の1箇所にのみ存在する**（旧実装はデフォルト引数の `2400` と同じ値を2箇所に書く必要はなかったが、今回 `arm` 側に fallback ロジックが移るため、デフォルト引数として重複させず `?:` にするのが素直）。呼び出し形（引数を省略する/3000/5000 を渡す）は不変なので `setHotkeyFailureNotice`/`notifyLaunchFailure`/`lib/commands.ts` の呼び出し元は無変更（`setLaunchNoticeWithAutoClear` の公開シグネチャ・挙動は不変）。

**この変更で意識的に踏み込む決定**: issue 本文は3対象すべてを「そのまま流用できる」と述べているが、`launchNoticeTimer` は可変 `delayMs`（2400/3000/5000）を持つため「そのまま」は成立しない。検討した代替案は「`launchNoticeTimer` は生 `setTimeout` のまま残し、固定 ms の4本（focus retry 2 + blurTimer + moveTimer）だけ移植する」。不採用の理由: issue が `launchNoticeTimer` を対象に明記しており、`arm` への `msOverride` 追加は「ms は resource 属性であり leading 等の policy ではない」という `ownedTimer.ts` の既存設計方針と矛盾しないため。ただし `ownedTimer.ts` は #536 で複数レンズの敵対的レビューを経た primitive のため、この拡張は `/plan-review` で明示的に検証する。

### 4. `ui/src/MainApp.tsx`

- `import { createOwnedTimer } from "./lib/ownedTimer";` を追加。
- `let blurTimer: ReturnType<typeof setTimeout> | undefined;` → `const blurTimer = createOwnedTimer(100);`
- `let moveTimer: ReturnType<typeof setTimeout> | undefined;` → `const moveTimer = createOwnedTimer(500);`

**発見した非対称性（research.md より）**: 現行コードは `moveTimer` 側は毎回 `clearTimeout(moveTimer)` してから再セットするが、`blurTimer` 側は blur 分岐で **`clearTimeout` を呼ばずに `blurTimer = setTimeout(...)` で上書き**している。連続する2回の blur イベント（間に focus が挟まらない場合）では前回のタイマーハンドルが孤児化し、`blurCancelled` フラグだけが「孤児タイマー発火時に誤って `hideMain()` してしまう」のを防ぐ実働のガードになっている（`blurCancelled` は focus 分岐でのみ true になり、孤児タイマーの発火時点でまだ true のままなら return する）。**`arm()` は呼ぶたびに必ず前回の保留を破棄してから新しいタイマーを張る**ため、blur 分岐を `blurTimer.arm(...)` に置き換えるだけでこの孤児化バグ自体が構造的に消える。これにより移行後は `blurCancelled` が判定不能（到達不能）になる。`moveTimer` 側の `moveEvent !== latestMoveEvent` 比較は、旧実装でも常に `clearTimeout` してから再セットしていたため孤児化の余地が元々無く、**移行前後を問わず常に dead code**（`clearTimeout` が保証する「古い callback は発火しない」の重複チェック）。

**決定**: 両ガード（`blurCancelled` と `moveEvent`/`latestMoveEvent`）は削除する。理由: (1) 移行後は構造的に到達不能なコードであり、残すと「何を守っているのか」を読者に誤解させる、(2) issue 自身が挙げる移行動機（「arm/cancel のペアリングが primitive の構造で保証され...clear 漏れを防ぐ」）を体現する変更であり、スコープ外の追加清掃ではなくこの refactor が正しく primitive を使うことの直接の帰結、(3) `blurTimer` の「100ms 猶予」という**観測可能な**不変条件（ドラッグ中の一時的フォーカス喪失で誤発火しない）は変わらない——むしろ孤児タイマーによる稀な誤 hide の可能性が消える分、厳密には旧実装より正しくなる（動作の後退ではない）。

**先例との整合**: この削除は #536 で `leadingFired` フラグを `refreshTimer.isPending()` と等価と判断して削除した先例と同じロジック——「手書きの状態フラグが、primitive の構造的保証（`timer !== undefined` 相当の判定）と等価になったら、フラグを primitive の判定に置き換える／不要なら消す」。`moveEvent` は移行前から恒偽（`clearTimeout` が既に保証）、`blurCancelled` は移行によって初めて恒偽になる（blur 分岐に `clearTimeout` が無かったため旧実装では稀な連続 blur ケースで実働していた）という違いはあるが、「primitive の保証で置き換えられるようになったら消す」という判断基準は共通。

`/symmetric-check` と `/plan-review` でこの判断の妥当性を検証する（実施済み・5体のサブエージェント全員が独立に同じ結論に到達。詳細はレビュー結果を参照）。

- `registerAutoHideOnFocusLost` 内:
  ```ts
  const unlistenFocus = await win.onFocusChanged(({ payload: focused }) => {
    if (!focused) {
      blurTimer.arm(() => {
        void (async () => {
          try {
            // 統合後は results ウィンドウが同一ウィンドウ内のため、
            // is_main_foreground によるプロセス ID 比較は不要。
            await hideMain();
          } catch (e) {
            console.warn("auto-hide focus check failed:", e);
          }
        })();
      });
    } else {
      blurTimer.cancel();
    }
  });
  ```
  （`blurCancelled` 宣言・参照を削除。`try/catch` の中身は不変）
- `onMoved` ハンドラ内:
  ```ts
  const unlistenMainMoved = await win.onMoved(() => {
    moveTimer.arm(() => {
      void (async () => {
        await api.saveSearchPlacement();
      })();
    });
  });
  ```
  （`latestMoveEvent`/`moveEvent` 宣言・参照を削除）
- `onCleanup`:
  - `clearTimeout(blurTimer); clearTimeout(moveTimer);` → `blurTimer.cancel(); moveTimer.cancel();`

### 5. `ui/src/components/SearchWindow.tsx`

- `import { createOwnedTimer } from "../lib/ownedTimer";` を追加。
- `const focusRetryTimers: ReturnType<typeof setTimeout>[] = [];` → 2本の固定用途インスタンスに置換:
  ```ts
  const focusRetryTimer120 = createOwnedTimer(120);
  const focusRetryTimer280 = createOwnedTimer(280);
  ```
- `clearFocusRetryTimers()`:
  ```ts
  function clearFocusRetryTimers() {
    if (focusRafHandle !== undefined) {
      cancelAnimationFrame(focusRafHandle);
      focusRafHandle = undefined;
    }
    focusRetryTimer120.cancel();
    focusRetryTimer280.cancel();
  }
  ```
- `focusInputWithRetries()`:
  ```ts
  function focusInputWithRetries() {
    clearFocusRetryTimers();
    focusInputSoon();
    focusRetryTimer120.arm(() => focusInputSoon());
    focusRetryTimer280.arm(() => focusInputSoon());
  }
  ```
- `focusRafHandle`（rAF 2フレーム defer）は setTimeout ではないため対象外・無変更。

## 実装順序（依存関係）

1. `ownedTimer.ts` の `arm` シグネチャ拡張 + `ownedTimer.test.ts` に override テストを追加（Red→Green）。他3ファイルの前提となる基盤変更のため最初に完了させる。
2. `launchNotice.ts`（`msOverride` を使う唯一の新規箇所。ownedTimer 拡張の動作確認を兼ねる）。
3. `MainApp.tsx`（`blurTimer`/`moveTimer`。override 不使用、固定 ms のみ）。
4. `SearchWindow.tsx`（focus retry の2インスタンス化。override 不使用）。

2〜4 は互いに独立ファイルのため順序に依存関係は無いが、上記順で1ファイルずつ検証しながら進める。

## 不変条件

- **`blurTimer` の 100ms 猶予**（ドラッグ中の一時的フォーカス喪失で `auto_hide_on_focus_lost` が誤発火するのを防ぐ・`ui/CLAUDE.md`）は ms 値・発火条件とも不変。`blurCancelled` フラグは削除する（移行後は構造的に到達不能。理由は「変更ファイル一覧 §4」参照）。
- **`launchNotice` の自動クリアは単一タイマー再利用で競合防止**（`docs/architecture.md`）。`arm` が前回の保留を破棄する点は変わらないため、`msOverride` 導入後も同一挙動（新しい `arm` 呼び出しが前回の保留中クリアをキャンセルして新しい delayMs で張り直す）。
- **`moveTimer` はウィンドウ移動位置のデバウンス保存**（`SPEC.md`）。500ms 固定・タイミング不変。`latestMoveEvent` 比較は削除する（`clearTimeout`/`arm` いずれでも常に到達不能だった重複チェック）。
- **focus retry の2本同時保留**: 120ms 用・280ms 用がそれぞれ独立してキャンセル・再張り可能であること（`clearFocusRetryTimers()` が両方を確実に cancel する）。
- **`arm` の `msOverride` 拡張は既存2呼び出し元（`refreshTimer`/`fetchTimer`）に影響しない**: 両者とも `msOverride` を渡さないため `?? ms` で生成時の値にフォールバックし、既存テスト（`search.test.ts`/instantCommand 関連）は無変更で green のまま。
- **リソース生成/破棄ペア**: 新設する5インスタンス（`launchNoticeTimer`/`blurTimer`/`moveTimer`/`focusRetryTimer120`/`focusRetryTimer280`）はいずれもモジュール/コンポーネントスコープで1回生成され、対応する `cancel()` 呼び出し箇所（既存の clear 経路）にそのまま置換される。生成箇所と破棄箇所が1:1で対応することを実装時に確認する。
- **`blurCancelled`/`moveEvent`・`latestMoveEvent` の削除は「別ガードの削除」であり「OwnedTimer の生成/破棄ペア」そのものではない**が、対称性の観点では「`arm()` が cancel-then-set を構造的に保証するようになった結果、手書きの重複ガードが不要になった」という副作用として扱う。`/symmetric-check` でこの2点も明示的にレビュー対象に含める。

## テスト方針

- `ownedTimer.test.ts`: `msOverride` の新規ケース3件を追加（上記「変更ファイル一覧 §2」参照）。fake timer で検証。
- 既存テスト（`search.test.ts`、`ownedTimer.test.ts` の既存ケース、`MainApp.test.tsx`、`SearchWindow.test.tsx`）はいずれも今回変更する3ファイルのタイマー挙動を直接アサートしていない（research.md で grep 確認済み）ため、回帰確認は既存スイート green で足りる。
- 検証コマンド: `npm run test -w ui`（または `docs/build-commands.md` の該当コマンド）、`npm run typecheck -w ui`。PostToolUse hook が `.ts`/`.tsx` 編集時に typecheck を自動実行する。
- 手動 smoke（挙動不変ゆえ既存テスト+手動確認で担保、issue記載通り）:
  - ウィンドウ表示直後にフォーカスが検索欄に入ること（focus retry）。
  - ドラッグ中の一時的フォーカス喪失でウィンドウが隠れないこと（blurTimer 100ms 猶予）。
  - ウィンドウ移動後、位置が保存されること（moveTimer 500ms debounce）。
  - 起動失敗・ホットキー失敗・indexing 中の /o 実行で通知が出て、それぞれ想定 ms 後に消えること（launchNoticeTimer 可変 ms）。

## SPEC.md 更新要否

**不要**。挙動不変のリファクタリングであり、SPEC.md に記載されたフロー・IPC契約・状態遷移に変更はない。

## セルフレビュー

### 5a. check スキル（実施結果）

- `/plan-review`: **実施済み**。担当ファイル別 Explore サブエージェント4体 + 独立導出 Plan サブエージェント1体（Step 2b）を並列実行。要対処0件、軽微な懸念4件（`msOverride=0`境界値テストの追加、JSDocの resource/policy 境界明記、`blurCancelled`の「実働ガード」断定根拠がTauri外部挙動依存という留保、`launchNotice.ts`の「シグネチャ変更」という表現の精度）——いずれも plan.md に反映済み。独立導出は計画と完全一致（focus retry 2インスタンス化・`moveEvent`恒偽・`launchNoticeTimer`の`arm`拡張必要性）し、加えて`blurCancelled`削除の理由づけに**#536の`leadingFired`削除の先例**という枠組みを提示（反映済み）。
- `/symmetric-check`: **実施済み**。5インスタンス全ての生成/arm/cancel箇所を`file:line`根拠付きで検証し全て[適用]（対称成立）。`onCleanup`のteardown順序（unlisten→cancel）も検証済み。`blurCancelled`/`moveEvent`削除もケース別に安全性確認済み。grepで issue 対象外の生タイマーが`ui/src`に残っていないことも確認（テストファイルのrAFスタブのみで無関係）。見落としなし。
- `/race-check`: 対象外と判断。新規 async 関数を追加しない（`blurTimer`/`moveTimer` の非同期本体は既存の async IIFE をそのまま移植するのみで、await 地点・staleness チェックのロジックは変更しない）。
- `/cache-check`/`/persistence-check`/`/state-check`: 対象外（キャッシュ・永続化・UI モード遷移に触れない）。

### 5b. セルフレビューチェックリスト

1. **対称コードパス**: `arm`/`cancel` ペア。5インスタンス全てで生成1箇所・破棄1箇所（既存clear経路）を1:1対応させる。`/symmetric-check` で機械的に確認する。
2. **影響範囲の網羅性**: `grep` で `focusRetryTimers|focusRafHandle|blurTimer|moveTimer|launchNoticeTimer` の全参照を洗い出し済み（research.md）。3ファイル内に閉じており外部からの直接参照は無い。
3. **境界条件**: `moveTimer`/`blurTimer` の「クリア直後に再 arm」「cancel 後の再 arm」は OwnedTimer 側で既にテスト済み（burst/cancel ケース、`ownedTimer.test.ts` 既存）。focus retry の「2本同時 arm→片方だけ cancel されない」は目視コードレビューで担保（`clearFocusRetryTimers` が両方 cancel）。
4. **リソース管理**: 上記「不変条件」参照。生成/破棄ペアを明記済み。
5. **既存パターンとの整合**: `arm(() => void asyncCall())` の慣習を踏襲。`msOverride` 拡張は「resource 属性（ms）であり policy ではない」ため `ownedTimer.ts` の設計方針（JSDoc）と矛盾しない。
6. **YAGNI 違反**: `msOverride` は launchNoticeTimer の実需要（3種類の delayMs が実在）に基づく最小拡張。他に汎用化・抽象化は追加しない。
7. **シンプル化の挑戦**: 検討した代替案
   - 案A（採用）: `arm(fn, msOverride?)` の後方互換拡張。
   - 案B: `createOwnedTimer()` を ms 無しにして毎回 `arm(fn, ms)` で必須指定。既存2呼び出し元（`refreshTimer`/`fetchTimer`）の呼び出し箇所を全て書き換える必要があり、diff が不必要に広がる。不採用。
   - 案C: `launchNoticeTimer` だけ生の `setTimeout` のまま残す。issue が明示的に対象としているため不採用。
   - 案Aが最小diffで既存契約を壊さない。
   - `blurCancelled`/`latestMoveEvent` 比較は削除する。`moveEvent` 側は移行前後を問わず常に dead code（`clearTimeout` が既に保証）。`blurCancelled` は移行前は「blur 分岐が `clearTimeout` を呼ばない」ことに起因する孤児タイマー対策として実働していたが、`arm()` へ統一することでその孤児化自体が構造的に起こらなくなり、移行後は同じく到達不能になる（詳細は「変更ファイル一覧 §4」）。残すと「何を守っているか」を読者に誤解させるため削除する。
8. **破壊不変条件の明示**:
   - `blurTimer` 誤発火（ドラッグ中フォーカス喪失でウィンドウが消える）は UX 直撃のリスク。検知手段: 手動 smoke（上記）+ 既存 `auto_hide_on_focus_lost` 関連の目視確認。
   - `focusRetryTimer` の cancel 漏れは起動直後にフォーカスが入らない不具合として顕在化する。検知手段: 手動起動確認（コールドスタート）。
   - いずれも Win32 フック・ホットキー・プロセス間通信ではなく JS 内 setTimeout の範囲に閉じるため「戻ってこない」系のリスクは無い（最悪ケースでも setTimeout の再発火忘れ程度で、次回ウィンドウ表示/操作で状態はリセットされる）。
