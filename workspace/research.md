# research.md — issue #361

## issue の要約

#355 / PR #360 で hide 時に Win32 `EmptyWorkingSet` をプロセスツリーへ適用しアイドル
working set を回収する仕組みを実装した。実測で **hotkey hide は 112→9.4MB**、
**frontend hide（Escape 等）は 98.5→22.8MB** と frontend 経路の回収が ~13MB 控えめ。

原因: frontend hide では trim（`notify_main_hidden` 内）が **`win.hide()` 完了前に**走る。
`notifyMainHidden()` が fire-and-forget（await しない）で tokio IPC スレッドに投げられ、
`await win.hide()` と並行するため、**まだ可視な状態で trim** が走りレンダラが直後にページを
再 touch → trim 効果が削がれる。hotkey 経路（main.rs、メインスレッドで同期実行）は
`w.hide()` → emit → suspend → trim の順で hide 完了後に trim するためこの問題がない。

本 issue は frontend 経路の trim タイミングを hide 完了後にずらし、回収差 ~13MB を詰める
follow-up（polish, type:refactor）。

## 関連コード

### frontend hide 経路（全 3 箇所）

| 箇所 | 現状の順序 | 経由する操作 |
|---|---|---|
| `ui/src/lib/commands.ts:18` `hideMainWindow()` | `notifyMainHidden()` → `await win.hide()` | Escape・Enter・Shift+Enter（SearchWindow.tsx）・スラッシュ `/s` |
| `ui/src/MainApp.tsx:50` `hideMain()`（フォーカス喪失） | `setMainVisible(false)` → `notifyMainHidden()` → `await win.hide()` | onFocusChanged の 100ms debounce 後 |
| `ui/src/MainApp.tsx:287` `handleClickResult()`（クリック起動） | `setMainVisible(false)` → `notifyMainHidden()` → `void win.hide()` | 結果行クリック |

- SearchWindow.tsx の Escape(`:172`) / Enter(`:227`) / Shift+Enter(`:221`) はすべて
  `hideMainWindow()` 経由 → commands.ts を直せば自動的に揃う。
- MainApp.tsx の 2 箇所だけが順序をインラインで重複保持している（DRY 違反）。

### Rust チョークポイント

`src-tauri/src/commands/system.rs:41` `notify_main_hidden`:
```rust
pub fn notify_main_hidden(state: State<AppState>, app: AppHandle) {
    state.main_visible.store(false, Ordering::SeqCst);   // ① UI 表示用フラグ
    let _ = app.emit("window-hidden", ());               // ② JS の Blob URL 一括解放を駆動
    crate::working_set::trim_idle_working_set(...);       // ③ EmptyWorkingSet
}
```
全 frontend hide が必ずここを通る。Rust 側は無変更で、JS 側の呼び出しタイミングのみ直す。

### hotkey 経路（参考・無変更）

`src-tauri/src/main.rs:565-585`（`app_handle.listen("hotkey-pressed")` コールバック = 同期）:
`w.hide()` → `main_visible.store(false)` → `emit("window-hidden")` → `suspend_webview` → `trim`。
hide 完了後に trim するため深く回収できる。**これが frontend 経路の目標形。**

## 既存パターン

- `hideMainWindow()` は既に frontend hide の共通関数として存在するが、MainApp.tsx の
  2 経路はこれを使わずインライン重複している。**案A の DRY 統合は既存関数への集約**であり、
  新パターン導入ではない（KISS / 既存パターン踏襲）。
- `EmptyWorkingSet` は trim が hide 前後どちらで走っても無害（src-tauri/CLAUDE.md
  「show 側に逆操作は不要・再 fault するだけ」）。**正しさではなく回収効率の問題。**

## 技術的制約・留保観点（issue 記載）

1. **`window-hidden` emit 順序依存**: `notify_main_hidden` が emit する `window-hidden` は
   フロントの Blob URL 一括解放（`ResultsSection` visible→false → `cache.revokeAll()`）を駆動
   （ui/CLAUDE.md「Blob URL 管理の不変条件」）。
   - 検証: MainApp の 2 経路は `setMainVisible(false)` を**先に同期で**呼んでおり、
     Blob 解放はその時点で既に駆動される（`ResultsSection visible = shouldShowResults() && mainVisible()`）。
     window-hidden イベントの `setMainVisible(false)` は冪等な二重 set。よって emit を hide 後に
     ずらしても MainApp 経路の Blob 解放タイミングは変わらない。
   - commands.ts 経路（Escape/Enter）は eager set がなく window-hidden イベント頼り。
     emit が hide 後になると Blob 解放も hide 後になるが、**window 非可視後の解放のため視覚影響なし**
     （むしろ可視中に結果が消える微小フラッシュを避けられる）。リークなし（revoke は実行される）。

2. **`main_visible` タイミングのレース**: hide 後に `main_visible.store(false)` を遅らせると、
   `await win.hide()` の間（数 ms〜~35ms）`main_visible=true` のまま。この窓で hotkey が押されると
   listener が `visible=true && toggle` と判定し hide 経路を再実行する。
   - hide は冪等（既に隠れた窓を hide しても無害）。show 意図の hotkey が 1 回空振りしうるが、
     窓は < ~35ms で人為的に狙えない。**現状も notifyMainHidden は fire-and-forget で IPC 往復
     遅延があり同様の窓が既存**。reorder は既存の極小窓を僅かに広げるのみで機能破壊なし。

3. **best-effort・機能不変**: trim 失敗は機能に影響しない（全 Win32 失敗を握りつぶす）。
   hide の体感レイテンシは増やさない（`notifyMainHidden` は hide 後 fire-and-forget のまま）。

## 採用案: 案A（JS 側・順序入れ替え + hideMainWindow 統合）

- 案A: `await win.hide()` の**後**に `notifyMainHidden()`。frontend hide が散在するため
  **先に `hideMainWindow()` へ統合**してから 1 箇所で直す。タイマー不要・チョークポイント統合で
  DRY 改善。→ **KISS。採用。**
- 案B（Rust 側 trim を ~50-100ms defer）は async タスク/タイマーの複雑性が増し、固定ディレイは
  hide 完了の実時間と無関係。→ 不採用。
- 統合の副次効果: 全 frontend hide が `hideMainWindow()` 1 関数を通るため、**順序不変条件を
  commands.test.ts の 1 ユニットテストで守れる**。

## SPEC.md 更新要否

**不要。** SPEC.md は状態遷移（`SearchVisible → Standby` on Escape/focus_lost/hotkey）のみ規定し、
`EmptyWorkingSet` の trim タイミング・`notifyMainHidden` の順序は記載していない。状態機械の挙動は
完全に不変（hide → Standby）。trim タイミングは実装詳細（メモリ最適化）。

## ドキュメント同期

- `ui/src/lib/commands.ts`: `hideMainWindow()` のコメントを「hide 完了後に trim を走らせるため
  notify は hide の後」に更新。
- `ui/CLAUDE.md`: `hideMainWindow()` が全 frontend hide の単一チョークポイントで
  「hide → notify(trim)」順である旨を実装パターンに追記。
- `src-tauri/CLAUDE.md`: `notify_main_hidden` は無変更のため記述は維持（「全 hide 経路に適用」も真）。

## 未解決の疑問

- **クリーンな再計測**（直前検索なしのフォーカス喪失 hide で frontend が hotkey ~9MB に近づくか）は
  release build + 手動フォーカス喪失トリガー + working set 計測が必要。ユニット/smoke では代替不能。
  実装フェーズで build + `smoke:startup` で回帰なしを確認し、メモリ差分は手動計測を要する旨を明記する。
