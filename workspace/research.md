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

## 計測 — baseline（#361 適用前 / 現行 main コード、2026-06-02）

`target/release/snotra.exe`（`EmptyWorkingSet` 含有を確認済み = #355/#360 適用済み）を起動し、
合成キー入力（`keybd_event`）で駆動。**検索を一切打たないクリーン状態**で計測。
計測スクリプト: `%TEMP%/snotra-mem-measure.ps1`。

- **指標**: snotra + WebView2 子孫の**プロセスツリー全体の総 WorkingSet64**（BFS 合算、共有 Edge DLL
  ページ込み）。PERFORMANCE.md の Private WS（共有 ~280MB 除外）とは**絶対値が異なる**。比較は相対差。
- **健全性確認**: 「show 時に WS 上昇 → hide+trim 後に下降」パターンで自動化を検証。
- 起動直後（hidden idle, trim 前）: 7 プロセス・総 405 MB（共有 DLL 込み）。

| 経路 | hide+trim 後（3 反復） | min | 備考 |
|---|---|---|---|
| frontend (Escape) | 61.4 / 50.5 / 53 MB | **50.5** | 3/3 安定（~50-61MB） |
| hotkey (Ctrl+K) | 18 / 10.6 / 68.5 MB | **10.6** | 2/3 clean。68.5 は toggle 同期ずれ（auto_hide 先行発火）で無効 |

**結論**: クリーン状態でも frontend hide は hotkey hide より総 WS を ~40MB 多く残す（~50 vs ~10MB）。
issue の前提（frontend 経路の trim 取りこぼし）はクリーン計測で**確証**。hot な 22.8 vs 9.4（Private）
より相対差は大きく、本修正（案A）は正当化される。

### 検証結果（#361 適用後 / 2026-06-02）

`npx tauri build --no-bundle` で release 再ビルド → 同スクリプトで after 計測。

| 経路 | baseline（前） | after（後） | 改善 |
|---|---|---|---|
| frontend(Escape) hide | 50.5/55 MB（min/avg・生 61.4/50.5/53） | **27.3/28.3 MB**（生 30.3/27.3/27.4） | ~23MB 減（~46%） |
| hotkey hide（参照・無変更） | 10.6 MB | 10.6 MB | — |

- frontend hide の総ツリー WS が**約半減**。frontend↔hotkey 差(~40MB)の **~57% を解消**。
- **残差 ~17MB** は frontend 経路が `suspend_webview`（TrySuspend）を行わない設計差に起因
  （hotkey 限定。#361 のスコープ外）。gap = trim タイミング成分（#361 で解消）+ suspend 成分（設計差）。
- 回帰なし: `npm run typecheck` 緑 / `npm test` 216/216 緑 / `smoke:startup` 5×0err 緑。
- 計測ノイズ: hotkey #3（baseline 68.5 / after 58.4）は toggle 同期ずれ（auto_hide 先行発火）で無効。
  Escape（非トグル）の frontend は前後とも 3/3 密集で信頼可。
- 記録先: `PERFORMANCE.md`「follow-up: frontend hide の trim タイミング修正（#361）」節。

### 残検証（PR）

- `e2e:tauri` はローカル未実行。PR に `e2e` ラベルを付与し CI（E2E & Smoke workflow）で smoke+e2e を回す
  （カテゴリ C: hide 順序変更）。
