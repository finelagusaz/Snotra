# research: #536 検索/instant の debounce を所有 Debouncer primitive に統合する

## issue の要約

フロントに散在する「所有者の曖昧な生タイマー」を、`.schedule()` / `.cancel()` / `.isPending()` / `.dispose()` を持つ小さな `Debouncer` primitive に畳み込む。対象は 2 つ:

1. **検索 debounce**（`search.ts`）: leading edge + trailing 50ms。`debounceTimer` + `leadingFired` フラグ + `DEBOUNCE_MS`。
2. **instant 候補 debounce**（`stores/instantCommand.ts`）: leading なし・trailing 30ms。`instantCmdDebounceTimer` + `INSTANT_CMD_DEBOUNCE_MS`。

生 `setTimeout`/`clearTimeout` とフラグを primitive 内部に隠蔽し、二層の別実装を「同一 primitive の 2 インスタンス」に統合する。**公開 API 不変・挙動不変**（`type:refactor`, `size:S`）。

## 関連コード（影響を受けるファイル・関数）

### 新規

- `ui/src/lib/debouncer.ts`（新規）: `createDebouncer({ ms, leading })` 純粋ファクトリ。既存の `lib/latestRun.ts`・`lib/exclusive.ts` と同じ「純粋ファクトリ・SolidJS/api 非依存・構造の理由を注釈」の作法に倣う。
- `ui/src/lib/debouncer.test.ts`（新規）: 単体テスト。`latestRun.test.ts`・`exclusive.test.ts` が手本。

### `ui/src/stores/search.ts`（検索 debounce の載せ替え）

現状（grep 実測の全出現）:

- `const DEBOUNCE_MS = 50;`（L60）
- `let debounceTimer`（L61）/ `let leadingFired = false`（L63）
- `cancelDebounce()`（L143–149）— タイマー破棄 + `leadingFired = false`
- `debouncedRefresh()`（L151–166）— leading 即時 `void runRefresh()` + trailing タイマー再セット
- `debouncedRefresh()` **呼び出し**: L318（`handlePlainQueryInput`）, L375（folderFilter effect）
- `cancelDebounce()` **呼び出し（6 箇所）**: L263（`handleInstantQueryInput`）, L289 / L298 / L306（`handleCommandQueryInput` の 3 枝: `/r` / 完全一致 cmd / slash-noop）, L413（`exitFolderExpansion`）, L555（`enterToolSelection`）
- `debounceTimer !== undefined` **判定 + cancel**: L456–458（`flushPendingRefresh`）— 「保留中なら cancel して即 run」の trailing 取りこぼし防止経路。**Debouncer が `isPending()` を持つ必要がある根拠。**

補足: `resetForShow`（L732–754）は `cancelInstantCommandDebounce()`（L742）を呼ぶが `cancelDebounce()`（検索側）は**呼ばない**。issue 本文の「resetForShow の計 6 箇所」は不正確で、実際の検索 `cancelDebounce()` 呼び出しは上記 6 箇所（`handleCommandQueryInput` の 3 枝を個別に数える）。**resetForShow に検索 debounce の cancel を新規追加してはならない**（現挙動を変える）。

### `ui/src/stores/instantCommand.ts`（instant debounce の載せ替え）

- `const INSTANT_CMD_DEBOUNCE_MS = 30;`（L6）
- `let instantCmdDebounceTimer`（L12）
- `hasPendingInstantCommandFetch()`（L27–29）— `instantCmdDebounceTimer !== undefined`
- `cancelInstantCommandDebounce()`（L32–37）— タイマー破棄
- `scheduleInstantCommandFetch()`（L46–78）— `instantCommandItems = []`（**即時クリアの副作用**）→ `cancelInstantCommandDebounce()` → `setTimeout(30ms)` で `deps.run(...)`
- `instantCmdDebounceTimer` **判定**: L28, L33–35, L58–59

## 既存パターン（再利用元）

### 模範 primitive の作法（`lib/latestRun.ts` / `lib/exclusive.ts`）

- `createXxx()` 純粋ファクトリ → callable もしくはメソッド束を返す。
- 内部可変状態（世代カウンタ / in-flight フラグ）を closure が**唯一の書き換え経路**として所有。
- SolidJS / api / Tauri に非依存（純粋・テスト可能）。
- **構造の「なぜ」を JSDoc に厚く注釈**（同期起動する理由・再入不可の理由など）。
- テスト（`*.test.ts`）は `describe(createXxx)` で各契約を 1 it ずつ。gate（外部解放できる Promise）で await 中の順序を制御。

### 既存 debounce テストの担保内容

- `search.test.ts` は `vi.useFakeTimers()` + `vi.runAllTimersAsync()` で query effect 経由の debounce タイミングを制御。debounce は**間接的**に検証される（専用 describe は無い）。instant モードのテスト（L526–624）が `getInstantCommands` フロー＝instant debounce を通過する。
- 検索 debounce の leading+trailing 挙動を**直接**ピン留めするテストは現状無い。→ 新規 `debouncer.test.ts` がここを埋める（測定＝回帰検出の主装置）。

## 二層統合の可否（issue が「実装時に測定して判断」と留保した点）

現状 2 実装の差分を構造比較すると、**差異は 3 点のみ**:

| 観点 | 検索 debounce | instant debounce |
|---|---|---|
| leading edge | あり（初回即時 `runRefresh()`） | なし |
| trailing ms | 50 | 30 |
| 付随副作用 | なし | `instantCommandItems = []`（schedule 時に即時クリア） |

タイマー再セット・trailing 発火・cancel（タイマー破棄）のロジックは**完全に同一**。したがって:

- 差異 (1)(2) は `createDebouncer({ ms, leading })` の**設定パラメータ 2 個**で吸収できる。
- 差異 (3) は issue 明記のとおり「debounce とは別関心事」。**primitive に含めず**、`scheduleInstantCommandFetch` 側に `instantCommandItems = []` を残す。

⇒ **統合は挙動を変えない**（構造的等価）。測定の実体は「`leading:true/false` を切り替えたパラメトリック単体テストが緑」+「既存 `search.test.ts` / instant テストが緑」。いずれかが赤なら統合が挙動を変えた証拠として分離へ差し戻す。

## 保つべき不変条件（issue より）

1. 検索: **leading edge（区間最初の入力で即時発火）+ trailing 50ms** を厳密維持（体感速度直結）。
2. instant: **leading なし・trailing 30ms**（`INSTANT_CMD_DEBOUNCE_MS`）を維持。
3. `flushPendingRefresh` の「保留タイマーがあれば cancel して即 run」経路を維持 → **`isPending()` API 必須**。
4. `scheduleInstantCommandFetch` の「呼び出し時点で候補一覧を即クリア」副作用は debounce と**別関心事**として分離（primitive に混ぜない）。
5. 公開 API 不変（`hasPendingInstantCommandFetch` / `cancelInstantCommandDebounce` / `scheduleInstantCommandFetch` / `refreshResults` 等のシグネチャ・export は変えない）。

## 技術的制約

- **リアクティブ制約**: `debouncedRefresh` の leading は `void runRefresh()` を**同期起動**する（`refreshResults` の同期プレフィックスが `query()` を同 tick で読む）。primitive の `schedule` も leading fn を同期呼び出しする必要がある（`exclusive.ts` の「task を同期起動する」不変条件と同種）。
- **fake timers**: primitive は `setTimeout`/`clearTimeout` を直接使う。テストは `vi.useFakeTimers()`（既存 `search.test.ts` と整合）。
- **Win32 / IPC 非依存**: 本 issue は純粋フロントのタイミング制御のみ。Win32 API・IPC 契約・on-disk 形式に一切触れない。
- **循環 import**: `instantCommand.ts` は `search.ts` へ逆依存しない設計。`debouncer.ts` は `lib/` に置き `api`/`solid-js` 非依存にすることで両 store から安全に import できる（`latestRun.ts` と同位置）。

## スコープ外（触らない）

- `launchNotice.ts` の `launchNoticeTimer`、`SearchWindow.tsx` の focus retry timers、`MainApp.tsx` の `blurTimer`/`moveTimer`。**同 primitive を将来流用できる設計にはするが、本 issue では展開しない。**
- `resetForShow` への検索 debounce cancel の新規追加（現状無いものを足さない）。

## SPEC.md 更新要否

**不要。** SPEC.md は debounce の内部（50ms/leading/30ms）を明文化していない（grep 実測）。L598「debounce をキャンセルして即座に action() を実行」・L381「移動位置をデバウンス保存」・L609「30ms未満」は観測可能な挙動レベルで、本 refactor では不変。IPC 契約・状態遷移も不変。

## ドキュメント更新（挙動不変でも必要）

- `ui/CLAUDE.md`: lib/ セクションに `debouncer.ts` を追加（ファイル新規＝モジュール構成更新の規約）。L108 の検索 debounce 説明・L29 の `scheduleInstantCommandFetch` 説明を primitive 経由に更新。
- `.claude/rules/ui.md`: 「モード遷移時にデバウンスをキャンセル」を「所有 Debouncer の `cancel()`」へ表現更新。

## 未解決の疑問

なし。issue が唯一留保した「二層統合の可否」は上記の構造比較 + パラメトリックテストで実装時に決着する（統合可の見込み）。`dispose()` は本 issue のスコープ（search/instant）では呼ばれない — 将来の per-component タイマー流用のための API 契約として primitive に含め、単体テストで担保する（issue のテスト方針「dispose 後 no-op」・位置づけ「将来流用できる設計」に明記済み）。
