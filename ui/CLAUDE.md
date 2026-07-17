# ui

SolidJS + TypeScript フロントエンド。Tauri IPC 経由で Rust バックエンドと通信。

各ルールは「**太字 = 守る指示**、後続 = 理由・経緯」の形式。迷ったら太字部分に従えば安全。

## モジュール構成

### エントリポイント（single-page build）

- `../main.html` / `main.tsx` → `MainApp.tsx`: 検索ウィンドウ用エントリ。テーマ適用、ウィンドウ位置復元、イベントリスナー登録、動的ウィンドウ高さ管理
- `vite-env.d.ts`: Vite client の ambient 型宣言（`/// <reference types="vite/client" />`）。責務を持つモジュールではないが、`tsconfig` の program に含まれるため型検査の対象になる

### components/

- `SearchWindow.tsx`: 検索入力 + キーボードナビゲーション + スラッシュコマンド補完 + ドラッグ移動
- `ResultsSection.tsx`: 検索結果をメインウィンドウ内にインライン描画。`search.ts` のシグナル（`results`・`selected`）を直接参照。アイコンのバイナリバッチ取得・Blob URL 管理・スクロール追従を担当
- `ResultRow.tsx`: アイコン + 名前 + パス + フォルダバッジ（1行分の描画）
- `UpdateToast.tsx`: 自動更新通知トースト（2行 52px）。`canInstall` prop で [今すぐ更新] ボタンの表示を制御

### stores/

- `search.ts`: 検索状態管理（クエリ/結果/選択/モード切替/`shouldShowResults` メモシグナル）の調停役
  - 主要な公開関数: `resetForShow()`（window-shown 時の全状態リセット）、`refreshResults()`（ソースに応じた検索実行）、`initIndexingState()`（起動時のインデックス状態初期化 + `indexing-started` / `indexing-complete` リスナー登録）
  - **検索起動の単一起点**: `dispatchQueryInput(value)`（ユーザー入力ハンドラ `SearchWindow.handleInput` からの明示 dispatch・唯一の検索起動起点）。旧 `createEffect(on(query, ...))` + `suppressNextQueryEffectRefresh` ワンショットフラグを撤廃し、入力解釈を純関数 `interpret`（`lib/interpretQuery.ts`）へ抽出。プログラム的リセット（`resetForShow`・instant 成功時の `setQuery("")` 等）は raw `setQuery` で **dispatch を経由しない別経路**ゆえ検索を起動しない（経路分離＝#537。旧フラグが担っていた「effect を今回だけ黙らせる」役割を構造で置換）
  - **横断規約の choke point**: `searchLane`（`lib/latestRun.ts` の `createLatestRun()` インスタンス。検索/データ lane の world 世代 + staleness を所有。`run()`＝最新実行、`invalidate()`＝モード遷移・起動が in-flight を supersede、`current()`＝perf requestId 源）、`activationLane`（`lib/exclusive.ts` の `createExclusive()` インスタンス。起動 lane の単一 mutex。実行中の 2 つ目の起動を拒否＝single-flight。`searchLane` の supersede と対をなす並行方針・#535）、`withLaunchLifecycle()`（起動フロー三種の共通骨格）、`saveView()`/`restoreView()`（`SavedViewState` の退避/復元）、`popView()`（モーダルビュー 1 段 pop の統一規律＝ViewStack。`invalidate`→`restoreView`→`kind` 別 onExit でスロットを null 化。`exitFolderExpansion`/`exitToolSelection` が委譲・#538）、`allowsFolderNav()`（フォルダ展開・ツール選択遷移の許可述語、`viewKind`/`interpKind` 由来）
  - flush 追跡（`refreshInFlight`/`trackRefresh`/`flushPendingRefresh`）は refresh lane 固有のため `searchLane` に吸収せず search.ts に残す（instant fetch や直接 `refreshResults()` を activation の待受対象に載せない現挙動を保つ・#534）
  - `launchNotice.ts` を re-export し公開 API を単一箇所に保つ（`instantCommand.ts` は re-export しない・関数を個別 import）
- `instantCommand.ts`: インスタントコマンド候補一覧の状態（`instantCommandItems`）と 30ms デバウンス IPC 取得（`scheduleInstantCommandFetch`。timer は `lib/ownedTimer.ts` の `fetchTimer`＝`createOwnedTimer(30)`・leading なし・`arm` のみ。候補一覧の即時クリアは debounce と別関心事として arm に混ぜず呼び出し時点で実行）。`api`/`lib/types` のみに依存し `search.ts` へ逆依存しない（循環 import 回避）。staleness 判定・世代更新は呼び出し側が検索/データ lane と共有する `run`（`searchLane.run`）を注入して担う（旧: `nextRequestId`/`isStale` hooks。#534 で `run` 1 本へ集約）
- `launchNotice.ts`: 起動失敗・ホットキー失敗の一時通知（`launchNotice` シグナル + 自動クリアタイマー）。`notifyLaunchFailure`/`setLaunchNoticeWithAutoClear`/`setHotkeyFailureNotice`/`clearLaunchNotice` を提供
- `folder.ts`: フォルダモードの状態（`FolderFrame` シグナル + `folderFilter`）。`FolderFrame` は `lib/types.ts` の `SavedViewState` を `kind: "folder"` 判別子付きで拡張し、離脱時復元用の `restoreQuery` を持つ（tool の `launchQuery` とは別概念・`ModalFrame` union で型分離・#538）
- `tool-selection.ts`: ツール選択モードの状態（`ToolSelectionFrame` シグナル）。`ToolSelectionFrame` は `lib/types.ts` の `SavedViewState` を `kind: "tool"` 判別子付きで拡張し、起動引数用の `launchQuery`（folder の `restoreQuery` と別概念・型分離）と 2 段スタック復帰用の `savedFolderFilter` を持つ（#538）

### lib/

- `invoke.ts`: 型付き Tauri IPC ラッパー
- `theme.ts`: CSS 変数によるテーマ適用
- `types.ts`: TypeScript 型定義の集約先（DRY）
- `commands.ts`: スラッシュコマンド定義（`/r` `/o` `/s` `/q`）と `SLASH_COMMANDS` 配列・`findCommand()` 関数
  - **`hideMainWindow()` は全 frontend hide（Escape / Enter / Shift+Enter / クリック起動 / フォーカス喪失 / `/s`）の単一チョークポイント**。MainApp.tsx のフォーカス喪失・クリック起動経路もこの関数に集約する
  - **`await win.hide()` → `notifyMainHidden()` の順を守る** — `notifyMainHidden` 内の suspend（TrySuspend）+ `EmptyWorkingSet` trim を hide 完了後に走らせることで、可視中の再 touch による working set 回収の取りこぼしと suspend の前提（非表示）崩れを避ける（hotkey 経路と同じ hide→suspend→trim 順。#361）
- `i18n.ts`: 多言語対応（日本語・英語）。`TranslationKey` 型と `t(key, params?)` 関数。`{param}` 形式プレースホルダー対応。SolidJS シグナルで言語を管理し、`setLanguage()` で切替。初期言語は `navigator.language` から同期的に決定（bootstrap 到着前のフラッシュ防止）
- `folderNav.ts`: フォルダナビゲーション純粋ロジック（`computeParentDir`・`clampSelectedIndex`）。ドライブルート・UNC パス対応。テスト可能なため `stores/` から分離
- `interpretQuery.ts`: 入力解釈の純関数（`interpret`・`isInstantPrefix`・`ViewKind`/`InterpKind`/`QueryIntent` 型）。「入力（query + prefix + viewKind）→ 意図（plain/command/instant + instant の filterName/instantQuery）」を副作用なしで返す。instant 判定・parse の SSOT。`stores/search.ts` の `interpKind` memo と `dispatchQueryInput` がこれを消費する（SolidJS/api 非依存・テスト可能なため分離・#537）
- `iconBatch.ts`: バイナリバッチ形式のアイコンデータをパースし、パスごとの Blob URL に変換する（`parseBinaryBatch`）。ResultsSection で使用
- `lruIconCache.ts`: Blob URL 管理付き LRU アイコンキャッシュ（`LruIconCache` クラス）。ResultsSection で使用
- `truncatePath.ts`: Canvas API でフォント依存のピクセル幅を計測し、長いパスを中間省略する（`truncatePath`）。結果はキャッシュ済み
- `windowHeight.ts`: ウィンドウの論理高さ計算（`computeWindowHeight`）。結果表示・トースト有無に応じた高さをピクセルで算出。テスト可能なため `stores/` から分離
- `latestRun.ts`: latest-wins（supersede）調停 primitive（`createLatestRun()`）。world 世代 + staleness を所有し `run`（最新実行）/`invalidate`（supersede）/`current`（世代読取）を提供。SolidJS/api 非依存の純粋ファクトリ。`stores/search.ts` の `searchLane` が唯一の利用者（#534。flush 追跡は含まない＝search.ts 側の関心）
- `exclusive.ts`: mutex/single-flight 調停 primitive（`createExclusive()`）。in-flight フラグを内部で所有し「実行中なら拒否（`undefined`）、完了時に必ず解放（try/finally）」を提供する callable を返す。task は同期起動する（呼び出し側の同期プレフィックスのキャプチャタイミングを保つ）。SolidJS/api 非依存の純粋ファクトリ。`stores/search.ts` の `activationLane`（起動 lane）が唯一の利用者。検索 lane の `latestRun`（supersede）と対をなす（#535）
- `ownedTimer.ts`: 所有ワンショットタイマー primitive（`createOwnedTimer(ms)`）。`setTimeout`/`clearTimeout` ハンドルを内部所有し `arm`（前回破棄 → `msOverride ?? ms` 後に fn を 1 回）/`cancel`（冪等・teardown 兼務）/`isPending` を提供。**timer resource のみを所有し debounce policy（leading 等）は持たない**。`arm` の第2引数 `msOverride`（任意）も policy ではなく resource 属性（呼び出し単位で ms を差し替える）として設計方針と矛盾しない（`launchNoticeTimer` が可変 delayMs のため使用）。`arm` は fn を `setTimeout` の中でしか呼ばず同期発火しないため再入が安全（契約不要）。SolidJS/api 非依存の純粋ファクトリ。7 インスタンスが利用: `stores/search.ts` の `refreshTimer`（検索 50ms・leading は `!isPending()` から導出）、`stores/instantCommand.ts` の `fetchTimer`（instant 30ms・leading なし）（以上 #536）、`stores/launchNotice.ts` の `launchNoticeTimer`（通知自動クリア・`msOverride` で可変 delayMs 2400/3000/5000）、`MainApp.tsx` の `blurTimer`（フォーカス喪失 100ms 猶予）/`moveTimer`（ウィンドウ移動 500ms debounce）、`components/SearchWindow.tsx` の `focusRetryTimer120`/`focusRetryTimer280`（フォーカスリトライ、2本同時保留）（以上 #544）
- `perf.ts`: 開発時専用パフォーマンス計測（`localStorage.snotra_perf=1` で有効化）。入力→検索→描画の3フェーズ時間を計測し P50/P95 を `console.table` 出力
- `trace.ts`: 開発時専用トレースログ（`localStorage.snotra_trace=1` で有効化）。`trace(event, data)` で `console.debug` 出力

## インスタントコマンドモード

プレフィックス（デフォルト `@`）で始まる入力はインスタントコマンドモードに入る。`interpKind() === "instant"`（query + prefix からの純粋導出）で判定する。

- **検出**: 純関数 `interpret`（`lib/interpretQuery.ts`）が prefix 一致（`isInstantPrefix`）をスラッシュコマンド判定より先に評価。`interpKind` memo と `dispatchQueryInput` の分岐が同一の `interpret` を消費する（分類・parse の SSOT）
- **結果表示**: `getInstantCommands()` IPC で前方一致フィルタ済みコマンドを取得し `SearchResult[]` に変換
- **実行**: `executeInstantCommand(name, query)` IPC。クリップボード読み取り・変数展開・ShellExecuteW はバックエンド側
- **shouldShowResults**: `indexing()` 中でもインスタントコマンドモードなら結果を表示
- **ガード**: ArrowRight/Left（フォルダ展開）、Shift+Enter（ツール選択）、handleInput の indexing ガードをバイパス
- **アイコン**: インスタントコマンドモード中は `skipIcons` prop で取得をスキップ
- **プレフィックス更新**: bootstrap payload + `instant-prefix-changed` イベントで同期

## テスト基盤

### 構成

- `vitest.config.ts` に `vite-plugin-solid({ hot: false })` を設定（`hot: false` は Windows の `@solid-refresh` URL 解決エラー回避。macOS でも無害）
- テストファイルパターン: `ui/src/**/*.test.{ts,tsx}`
- デフォルト環境: `node`。コンポーネントテスト（`.test.tsx`）は先頭に `// @vitest-environment jsdom` を付けて個別に jsdom を使用
- **テストファイルも tsconfig の program に含まれ、typecheck の対象**（#474）。vitest は esbuild 変換で型検査をしないため、テストと実装の型契約のずれは tsc（`npm run typecheck` / PostToolUse hook）が検知する

### 注意点

- **vite-plugin-solid により SolidJS のリアクティブ初期化が走る**: SolidJS モジュール（`search.ts` 等）を import するテストは、`requestAnimationFrame` 等のブラウザ API スタブが必要。`vi.hoisted(() => { globalThis.requestAnimationFrame = ... })` でモジュールロード前に差し込むこと（`vi.stubGlobal` はホイストされないため間に合わない）
- **`lib/` モジュールが `stores/` を import するようになった場合**: そのモジュールのテストファイルに `vi.mock("../stores/...")` を追加する。追加しないと transitive import 経由で SolidJS のモジュールレベルコードが走り `requestAnimationFrame` 等の未定義エラーが CI で発生する。`vi.hoisted` でモック関数を宣言してから `vi.mock` ファクトリ内で参照するパターンを使う
- **Canvas API モック**: `truncatePath.ts` のように遅延初期化（初回呼び出しまで `document.createElement` しない）の場合、`vi.stubGlobal("document", { createElement: ... })` を `beforeAll` で設定すれば jsdom 不要でテスト可能
- **コンポーネントテストでは `render(() => <Component />)` を使う**: SolidJS の `render` は関数ラッパーが必須（React と異なり直接 JSX を渡さない）
- `cspValidation.test.ts`: Tauri v2 IPC の CSP 検証（`connect-src` に `ipc:` と `http://ipc.localhost` が必要）
- **`beforeEach` の `vi.clearAllMocks()` は `.mock.calls` を消すが `mockImplementation`/`mockResolvedValue` の差し替えは復元しない**: あるテストで api モックの実装を差し替える（例: `mockImplementation` で deferred 化）と、次のテストへ漏れる（`vi.mock` の factory 既定には戻らない）。実装を差し替えたテストに依存する後続テストは、そのモックを自前で再設定すること（`beforeEach` が明示的に再設定するのは `api.search` のみ）。漏れは deferred なら timeout で loud に落ちるが、`mockResolvedValue` の漏れは誤った値で緑になる（false green）

### コンポーネントテスト（.test.tsx）のモックパターン

ストアをモック化するコンポーネントテストでは、`vi.mock` ファクトリがホイストされるため、モック関数を `vi.hoisted` 内で宣言する必要がある。ストアテスト（`search.test.ts`）のように `vi.mock` → `import` の順序制御だけでは不十分（コンポーネントの transitive import でモジュールロード順が変わるため）:

```typescript
const { mockFn1, mockFn2 } = vi.hoisted(() => ({
  mockFn1: vi.fn(),
  mockFn2: vi.fn(),
}));
vi.mock("../stores/search", () => ({ fn1: mockFn1, fn2: mockFn2 }));
```

- `SearchWindow.test.tsx` が基盤実装。新しいコンポーネントテストはこのパターンを踏襲する
- Tauri API（`@tauri-apps/api/window`, `@tauri-apps/api/event`）は空モックで無害化
- **自動 cleanup は登録されない（vitest globals 無効のため）**: `@solidjs/testing-library` の auto-cleanup はグローバル `afterEach` に依存する。明示的に `afterEach(() => cleanup())` を書かないと、前のテストのコンポーネントがシグナル購読を持ったまま残り、後続テストのシグナル更新に反応してモック呼び出し回数を汚染する（`ResultsSection.test.tsx` で実証済み）
- **effect の再実行を検証するテストではストアモックに実シグナルを使う**: `vi.fn(() => value)` の静的モックでは購読が発生せず effect が再実行されない。`vi.mock` の async ファクトリ内で `createSignal` を作り、setter を `__setResults` 等の名前で一緒に export してテストから駆動する（`ResultsSection.test.tsx` が基盤実装）
- **コンポーネント境界の配線テストは子コンポーネントを props 捕捉モックにする**: 親（`MainApp` 等）のハンドラ配線・props 導出を検証するとき、子を `(props) => { captured = props; return null; }` でモックすると、jsdom 描画に依存せず「どの props / コールバックが渡ったか」を直接アサートできる（`MainApp.test.tsx` が基盤実装。Solid の props は getter なので捕捉後も live 値を返す）
- **導出アクセサ（`viewKind`/`interpKind` 等）のモックは下位シグナルモックから導出させる**: ストアが既存シグナルから導出する派生アクセサを追加したとき、そのモックを下位シグナルモック（`mockToolSelectionState` 等）を読む実装（`vi.fn(() => mockToolSelectionState() ? ... : ...)`）にすると、既存テストが下位シグナルを set するだけで派生も追従し、テスト本体を書き換えずに緑を維持できる。`vi.clearAllMocks()` は `vi.fn(impl)` の実装を保持するため、`beforeEach` で下位モックを再設定すれば導出モックも最新値を反映する

## 実装パターン

- 検索ウィンドウのドラッグ移動は `.search-bar` の `data-tauri-drag-region` 属性で実現。`<input>` には付与しないため入力操作は維持される
- ドラッグ開始時の一時的なフォーカス喪失で `auto_hide_on_focus_lost` が誤発火するため、`onFocusChanged` の非表示処理に 100ms の猶予を設けフォーカス復帰時にキャンセルする設計
- **`async` 関数内で `await` をまたぐ可変変数はローカルキャプチャする**: `let` 変数やモジュールスコープの可変変数を `await` をまたいで参照する場合、関数冒頭で `const` にコピーしてから使う。`await` 中に外部イベントで値が書き換わると後続処理が意図しない値を参照する（例: `const visibleCount = cachedMaxResults`）
- **`await` 後に保存状態を復元する場合は staleness チェックを入れる**: `await` 前に保存した状態を失敗時に復元するパターンでは、`latestRun` の `isStale()`（lane タスク内）または起動フローでは `withLaunchLifecycle` が invalidate 直後に捕捉した世代と比較する `disturbed()` 述語（例: `executeInstantCommandSelected` の失敗ロールバックが `if (!disturbed())`）で「`await` 中に状態が変わっていないか」を検証してから復元する。無条件復元は新しい状態を上書きするレースコンディションを生む。加えて、`await` 中にユーザー入力で状態が変わること自体を防ぐガード（`handleInput` の `launching()` チェック等）を根本対策として併用する
- 検索デバウンスは leading edge（初回即時発火）+ trailing 50ms。timer は `lib/ownedTimer.ts` の `refreshTimer`（`createOwnedTimer(50)`）が所有し、leading は `debouncedRefresh` が `!refreshTimer.isPending()`（＝バースト先頭。旧 `leadingFired` フラグは `timer !== undefined` と等価ゆえ廃止）から導出して即時 `runRefresh()`、trailing は `refreshTimer.arm()`。`cancelDebounce()` は `refreshTimer.cancel()` へ委譲（#536）
- **`createEffect` に高さ計算と無関係なシグナルを含めない**: `cachedWidth` のように effect の出力（`setSize` の引数）には必要だが変更トリガーにすべきでないシグナルは `untrack()` で依存から外す。特に Tauri の `resizable: false` 環境では、幅変化は Rust 側 `set_size` → `onResized` → JS `setSize` のループバックになりうる
- **`createMemo` がオブジェクトを返すと毎計算で下流へ伝播する**: SolidJS 既定の `===` 等価ではオブジェクトリテラルが毎回新 identity となり、`kind` 等の値が不変でも購読側の memo/effect を再発火させる。頻繁に変わるシグナル（`query()` 等）に依存する派生メモは**プリミティブを返す**か `createMemo(fn, { equals })` で等価関数を与える（例: `viewKind()`/`interpKind()` は文字列を返すため `kind` 変化時のみ伝播し、plain 打鍵では下流を再計算しない）
- **DEV 限定コードは `import.meta.env.DEV` でガードする**: `trace()` や `performance.now()` など開発時のみ必要な処理は、呼び出し側でも `import.meta.env.DEV` で囲む。Vite がプロダクションビルドでデッドコード除去するため、呼び出し先がノーオペレーションでも呼び出し側の引数計算コストは残る
- **テーマ変更時はフォント依存キャッシュをクリアする**: `truncatePath.ts` の Canvas 測定キャッシュなど、フォント情報をキーに含むキャッシュは `visual-config-changed` イベントでクリアする。キーにフォントを含むため誤った結果は返さないが、stale エントリがメモリを占有し続ける

## 単一ウィンドウの高さ管理

検索バーと検索結果は1つの Tauri ウィンドウ内に共存する。結果の表示/非表示はシグナルで管理し、ウィンドウ高さは動的に変更する。

- `shouldShowResults` メモシグナル: `results().length > 0` かつ `switch(viewKind())`（tool/folder は常に表示、results は `interpKind() === "instant" || !indexing()`）— 結果を表示すべきかの判定（ツール選択中・フォルダモード中・インスタントコマンドモード中はインデックス構築中でも結果を表示）。詳細は「状態モデル（2 軸）」を参照
- `mainVisible` ローカルシグナル: `window-shown` / `window-hidden` イベントで同期される — ウィンドウが可視かの判定
- `ResultsSection` の `visible` prop: `shouldShowResults() && mainVisible()` — 実際の描画と Blob URL ライフサイクルを制御
- `createEffect` でウィンドウ高さを計算: `shouldShowResults()` が true なら `SEARCH_BAR_HEIGHT + maxResults * RESULT_ROW_HEIGHT + RESULTS_PADDING`、false なら `SEARCH_BAR_HEIGHT`
- Rust 側の `show_main_and_emit` で毎回 52px にリセット → フロントエンドが結果に応じて拡張。これにより表示時の一瞬のフラッシュを防止する

## Blob URL 管理の不変条件

- アイコンの Blob URL は `LruIconCache`（`lruIconCache.ts`）が一元管理する。`URL.createObjectURL` で生成した URL は必ず `cache.set()` または早期リターン時の明示的 `revokeObjectURL` で回収する
- `parseBinaryBatch` で Blob URL を生成した後、`cache.set()` に到達する前に早期リターンするパス（stale guard 等）では、`parsed` 内の全 URL を明示的に `revokeObjectURL` すること
- `ResultsSection` の `visible` prop が `false` になったとき `cache.revokeAll()` + `setIconNotify(reconcile({}))`（per-path 通知の全リセット）で Blob URL を一括解放する

## 設計上の注意点

### 状態モデル（2 軸 + オーバーレイ）

検索ウィンドウの「モード」は単一の型ではなく、`search.ts` の 2 つの**プリミティブ判別子メモ**で導出する。散在ガードの優先度を一箇所に集約し、生シグナルの直接 if を避ける（SPEC §8.6 状態図 / §18.5 優先度と一対一対応）。

- **軸1 `viewKind()`**（`"results" | "folder" | "tool"`）: 結果リストを占める先頭ビュー＝モーダルビュースタックの頂点の種類。`stackTop()?.kind ?? "results"`（`stackTop()` = `toolSelectionState() ?? folderState()`、tool > folder > results）。frame の `kind` 判別子を射影する（#538）。tool は folder の上に積まれうる（直交）。
- **軸2 `interpKind()`**（`"plain" | "command" | "instant"`）: 入力の意味。`viewKind()==="results"` のときだけ非 plain（folder/tool 中は plain）。`query` + `instantCommandPrefix` からの純粋導出（持続ラッチを廃止し、二軸とも真の派生に統一）。分類の実体は純関数 `interpret`（`lib/interpretQuery.ts`）で、memo は `interpret(...).kind`（プリミティブ）を返す。
- **オーバーレイ**: `indexing()` / `launching()` は軸ではなく boolean。どのモードにも重なる。

実装規約:

- **モード判定は `viewKind()`/`interpKind()` 経由**。`toolSelectionState()`/`folderState()` を直接 if して優先度を再導出しない（frame の値が要る箇所＝`inputValue` の `targetPath`・`placeholderText` の `currentDir` は storage を直読してよい）
- **軸メモはプリミティブを返す**。オブジェクト union は `===` 等価で毎計算が新 identity となり、`query()` 依存の `interpKind` が plain 打鍵ごとに下流（`shouldShowResults`/`skipIcons`/アイコン effect）を再発火させる
- **入力受理（`handleInput`）は軸1 + overlay のみに依存**。`interpKind` は読まない（インスタントコマンド中も打鍵を受理するため）
- **網羅 switch の default は `assertNever`**（モード追加時の分岐漏れをコンパイルエラー化）。表示関数のように網羅性が degrade 許容な箇所は viewKind 経由の if でもよい

### マウスイベントハンドラ

クリック起動 (`handleClickResult`) とダブルクリック選択 (`handleDoubleClickResult`) はどちらも **リスト行インデックス（`number`）** を引数として受け取る。パス文字列を使ってはならない。理由: パスは通常検索では一意だが、ツール選択モード中は同一 exe の複数ツールが同じパス（`tool.exe`）を持ちうるため非一意になる。インデックスは全コンテキストで常に一意。

マウスホバーは CSS `:hover` による視覚フィードバックのみ。`selected` シグナルは変更しない（キーボードナビゲーションとの干渉を防ぐため）。

### i18n キー設計のルール

- **新キー追加前に既存キーを確認する**: `ui/src/lib/i18n.ts` に新しい `TranslationKey` を追加する前に、同じ文字列値を持つ既存キーがないか確認する。特に `settings.*` 名前空間のキーと機能的に同一の文字列を別名で追加しない
- **動的文字列は `{param}` テンプレートで管理する**: `t("key") + variable` の文字列末尾連結ではなく、`t("key", { param: value })` の `{param}` 置換に統一する。語順が言語によって変わる場合でも対応でき、t() の設計意図と一致する
- **実装しない機能のコメントは書かない（YAGNI）**: i18n モジュールに「将来 locales/ ファイルで上書き可能にする予定」等の未実装計画をコメントで残さない
