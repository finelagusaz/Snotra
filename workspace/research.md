# research.md — issue #538 folder/tool モーダル遷移を明示的 ViewStack に再設計

## issue の要約

folder 展開・tool 選択という 2 つのモーダルビューは、`search.ts` が results/selected/query を手動でスナップショット退避 → 離脱時に復元することで実現している。issue は次の 3 つの footgun を「明示的 ViewStack（push/pop）」で解消することを提案:

1. **`savedQuery` 同名別概念**: `FolderFrame.savedQuery`（離脱時 `setQuery` で復元）と `ToolSelectionFrame.savedQuery`（launch へ渡すだけ・復元しない）が同名で別概念を担う型 footgun。
2. **暗黙の setX 順序**: 各 `enter*`/`exit*` が `setFolderState`/`setToolSelectionState`/`setQuery` 等を「特定の順序で」呼ぶ（例: `exitFolderExpansion` の「`setFolderState(null)` を `setQuery` より先に」）。
3. **暗黙のスタック構造**: tool は folder の上に積まれうる（2 段スタック）が、スタックとして明示的にモデル化されていない。

**要求判断（済）**: 本 issue は「費用対効果が疑わしく wontfix も正当」と再評価を求める案件。前提条件（項番 1〜4 = #534/#535/#536/#537 すべて CLOSED）が満たされたため、ユーザー確認の結果 **フル ViewStack で進める**方針が確定。

## 関連コード

### 状態の源（scope-out で維持）
- `ui/src/stores/folder.ts`: `folderState: FolderFrame|null`（`SavedViewState` 拡張 + `currentDir` + `savedQuery`）、`folderFilter`。
- `ui/src/stores/tool-selection.ts`: `toolSelectionState: ToolSelectionFrame|null`（`SavedViewState` 拡張 + `targetPath`/`targetIsFolder`/`tools`/`savedQuery`/`savedFolderFilter`）。
- `ui/src/lib/types.ts`: `SavedViewState = { savedResults, savedSelected }`（choke の共通「戻り先」）。

### 調停役（search.ts）
- **choke point**: `saveView()`（`{ savedResults: results(), savedSelected: selected() }`）/ `restoreView(saved)`（`updateResults` + `setSelected`）。frame 固有フィールドは呼び出し側でスプレッド合成/復元。
- **viewKind**（軸1・memo・プリミティブ）: `toolSelectionState() ? "tool" : folderState() ? "folder" : "results"`（tool > folder > results＝SPEC §18.5 優先度の射影）。
- **遷移 5 関数**:
  - `enterFolderExpansion(dir)`: 新規時のみ `saveView()`+`savedQuery: query()` を退避（深掘り/`navigateFolderUp` は `{...fs, currentDir}` でフレーム内書き換え・**push しない**）。`cancelDebounce`/`invalidate` は**呼ばない**。末尾 `runRefresh()`。
  - `exitFolderExpansion(): boolean`: `cancelDebounce` → `invalidate` → `restoreView(fs)` → `setFolderState(null)`（**setQuery より先**）→ `setFolderFilter("")` → `setQuery(fs.savedQuery)`。ガードは `!fs` → false。
  - `navigateFolderUp()`: `computeParentDir` → `{...fs, currentDir: parent}` → `setFolderFilter("")` → `setSelected(0)` → `runRefresh()`。
  - `enterToolSelection(result): Promise<boolean>`: `cancelDebounce` → `getMatchingTools`（async）→ **tools ≤ 1 なら `activateSelected()` フォールバック** → frame を `saveView()`+`savedQuery`+`savedFolderFilter` で合成 → `invalidate` → `updateResults(toolResults)` → `setSelected(0)`。返り値 false（起動していない）。
  - `exitToolSelection(): boolean`: `invalidate` → `restoreView(frame)` → `setToolSelectionState(null)` → `setFolderFilter(frame.savedFolderFilter)`（**2 段スタック復帰**）。`cancelDebounce` は**呼ばない**（tool 中は入力無効＝保留 timer 無し）。ガードは `!frame` → false。
- **frame 値の直読（launch/表示）**: `launchWithSelectedTool` が `frame.targetPath`/`frame.savedQuery`（launch 引数）、`SearchWindow.inputValue` が `ts.targetPath`、`placeholderText` が `fs.currentDir`。

### 消費側（components）
- `SearchWindow.tsx`:
  - **Escape の boolean 短絡**: `if (!exitToolSelection() && !exitFolderExpansion()) hideMainWindow();`（tool を先に試す＝頂点から1段 pop）。
  - ArrowRight/Left → `enterFolderExpansion`/`navigateFolderUp`（`allowsFolderNav()` ガード）。
  - Shift+Enter → `enterToolSelection`。
  - `inputValue`/`placeholderText` は `viewKind()` で分岐し frame 値を storage 直読。

## 既存パターン（再利用）

- **純粋 primitive の lib/ 抽出（#534-536）**: `latestRun`/`exclusive`/`ownedTimer` は resource + discipline を所有する SolidJS/api 非依存ファクトリ。ただし ViewStack の push/pop は `results`/`selected` シグナルと 2 つの frame シグナルに密結合するため、lib/ への抽出は注入だらけで割に合わない（→ 制約参照）。**push/pop discipline は search.ts 内に置く**（`saveView`/`restoreView` と同じ判断）。
- **discriminated union + `assertNever`**: `viewKind`/`shouldShowResults`/`dispatchQueryInput` が網羅 switch + `assertNever` で分岐漏れをコンパイルエラー化。frame に `kind` 判別子を足せば `popView` も同じ規律に載る。
- **プリミティブ判別子メモ**: viewKind/interpKind は「オブジェクト union を返すと毎計算で新 identity → 下流再発火」を避けプリミティブ（文字列）を返す規約（ui/CLAUDE.md）。

## 技術的制約

- **配列シグナル/配列メモは禁止アンチパターン**: `viewStack: ModalFrame[]` を signal や memo で持つと、毎計算で新 identity となり viewKind 等の下流を plain 打鍵ごとに再発火させる（ui/CLAUDE.md「createMemo がオブジェクトを返すと毎計算で下流へ伝播」）。**ゆえにスタックは配列で表現しない** — 2 つの順序付きスロット（`folderState`/`toolSelectionState`）+ 頂点射影 `toolSelectionState() ?? folderState()` で表す。これがスコープ外「signal 分割維持」を字義どおり守る唯一の idiomatic 形。
- **循環 import 回避**: `folder.ts`/`tool-selection.ts` は solid-js と types のみ依存（`search.ts` へ逆依存しない）。単一 `viewStack.ts` に SSOT を移して両者を派生化すると循環または責務移動になり scope-out に反する。**SSOT は 2 シグナルのまま**。
- **リアクティブ順序の暗黙不変条件**: `exitFolderExpansion` の「`setFolderState(null)` を `setQuery` より先に」。#537 で raw `setQuery` が検索 effect を起動しなくなったため**現在は vestigial の可能性が高い**（folder 中 query() は不変＝`fs.savedQuery` のままで、離脱時 `setQuery(fs.savedQuery)` は同値 no-op）が、コストゼロで保てるため**順序を厳密に保存する**（推測で撤去しない）。`batch()` でまとめると flush タイミングが変わりうるため**現状の非 batch 逐次 set を維持**。
- **enter の非対称性は意味を持つ**: `enterFolderExpansion` は `cancelDebounce`/`invalidate` を呼ばず、`enterToolSelection` は両方呼ぶ。folder 深掘りは push せずフレーム内書き換え。これらは統一 pushView に無理に畳むと条件分岐で価値が薄れる。**enter 側は薄い helper 止まり、統一の主眼は exit（popView）と型分離**。
- **`cancelDebounce` の対称化は behavior-preserving**: `popView` 先頭で常に `cancelDebounce` を呼んでよい（tool 側は保留 timer 無し＝`ownedTimer.cancel()` は冪等 no-op）。
- **Win32 依存なし**: 本件は純 UI リアクティブ状態の再設計。`SendInput`/`ShowWindow` 等の非同期 API は無関係。
- **永続化なし**: frame は in-memory signal のみ（`window.bin` 等に serialize されない・grep 実測）。`/persistence-check` 不要。version バンプ不要。

## 未解決の疑問

- **「フル ViewStack」の期待形**: ユーザーが選んだ「フル」が (a) 配列 `ModalFrame[]` の literal stack を指すのか、(b) 「2 スロット + 頂点射影 + 統一 push/pop discipline + 型分離」を指すのか。技術的制約（配列アンチパターン・循環 import・scope-out）から (b) が唯一 idiomatic かつ制約適合であり、本計画は (b) を採る。(a) は plan-review とユーザーのレビューで veto 可能なよう、却下代替として plan.md に明記する。
