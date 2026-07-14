# plan.md — issue #538 folder/tool モーダル遷移を明示的 ViewStack に再設計

## 設計方針（確定）

スコープ外制約「`folder.ts`/`tool-selection.ts` の signal 分割維持」+ 配列アンチパターン + 循環 import 回避から、ViewStack を次の 3 要素で実現する（research.md 「未解決の疑問」で選んだ形 (b)）:

1. **discriminated `kind` 判別子**を両 frame に付与 → `ModalFrame = FolderFrame | ToolSelectionFrame` の union で型分離が効く。
2. **頂点射影**: `viewKind = (toolSelectionState() ?? folderState())?.kind ?? "results"`（現 `? :` 連鎖と等価・プリミティブ返却・tool > folder > results 優先度を保存）。「スタック頂点の種類の純関数」を実現。
3. **統一 pop discipline `popView(frame)`**: 共通 `invalidate → restoreView` の後、`kind` 網羅 switch で frame 種別ごとの onExit を施しスロットを null 化。個別 setX 順序をここへ吸収。
4. **footgun 型分離**: `savedQuery` を用途別に改名 — folder は `restoreQuery`（離脱時復元）、tool は `launchQuery`（launch 引数）。union + 改名で「同名別概念」が型で分離され、取り違えはコンパイルエラーになる。

**push は別関数化しない**（判断）: push の核 = `saveView()` choke（既存）+ kind タグ付き frame 構築（one-liner）。`enterFolder`（新規のみ snapshot・深掘りは frame 内書き換え）と `enterTool`（async + fallback）は非対称で、統一 pushView は条件分岐で価値が薄れる。統一 discipline の主眼は**順序不変条件が集中する pop 側**と型分離に置く。→ 却下代替（literal `ModalFrame[]` 配列）は末尾セルフレビューに記載。

## 変更ファイル一覧

### 1. `ui/src/lib/types.ts`
- `SavedViewState` は共通基底のまま維持（`savedResults`/`savedSelected`）。doc コメントを「footgun は型で分離済み（folder=`restoreQuery`/tool=`launchQuery`）」に更新。
- `ModalFrame` union は types.ts に置かない（folder.ts/tool-selection.ts への逆 import が循環を生む）。search.ts 内で `FolderFrame | ToolSelectionFrame` を直接使う。

### 2. `ui/src/stores/folder.ts`
- `FolderFrame` に `kind: "folder"` を追加。`savedQuery` → `restoreQuery` に改名（doc コメントも「tool の launchQuery とは別概念」へ更新）。

### 3. `ui/src/stores/tool-selection.ts`
- `ToolSelectionFrame` に `kind: "tool"` を追加。`savedQuery` → `launchQuery` に改名（doc コメント更新）。`savedFolderFilter` はそのまま（2 段スタック復帰用）。

### 4. `ui/src/stores/search.ts`
- **型 import 追加**（plan-review W1）: `import type { FolderFrame } from "./folder"` / `import type { ToolSelectionFrame } from "./tool-selection"`。ファイル内に `type ModalFrame = FolderFrame | ToolSelectionFrame`（union は types.ts に置かない＝逆 import 循環回避。`ModalFrame` はコード上この 1 定義のみ・不変条件記述で使う概念名）。
- **viewKind**: `(toolSelectionState() ?? folderState())?.kind ?? "results"` へ（プリミティブ返却・優先度不変）。
- **popView(frame) 新設**（choke point）。**cancelDebounce は folder 経路のみ・invalidate より前**（現 `exitFolderExpansion` の順序を厳密保存。tool 経路は現状 cancelDebounce を呼ばない挙動を保つ＝挙動不変。plan-review 発見1 参照）:
  ```
  function popView(frame: ModalFrame): boolean {
    // folder のみ: folderFilter 入力が張った保留 timer を破棄（invalidate より前＝現 exitFolderExpansion の順）。
    // tool 経路で cancelDebounce を呼ばないのは現挙動の厳密保存（enterToolSelection の await 窓で稀に残る
    // timer も現状どおり残す）。両経路で cancel すると挙動変化になるため、folder 固有にとどめる。
    if (frame.kind === "folder") cancelDebounce();
    searchLane.invalidate();
    restoreView(frame);
    switch (frame.kind) {
      case "folder":
        setFolderState(null);      // setFolderFilter("") より先（null 後なら folderFilter effect が
                                   // debouncedRefresh をスキップ＝stray refresh 防止。真の load-bearing 順序）
        setFolderFilter("");
        setQuery(frame.restoreQuery);
        break;
      case "tool":
        setToolSelectionState(null);
        setFolderFilter(frame.savedFolderFilter);  // 2 段スタック復帰
        break;
      default:
        return assertNever(frame);
    }
    return true;
  }
  ```
- **exitFolderExpansion/exitToolSelection** を薄いガード + `popView` 委譲へ:
  ```
  function exitFolderExpansion(): boolean { const fs = folderState(); return fs ? popView(fs) : false; }
  function exitToolSelection(): boolean { const f = toolSelectionState(); return f ? popView(f) : false; }
  ```
  （各々「自分のスロットが在れば pop」。Escape の `!exitTool() && !exitFolder()` 短絡＝tool 優先は不変。）
- **enterFolderExpansion**: 新規 frame に `kind: "folder"`, `savedQuery` → `restoreQuery` を反映。深掘り `{...fs, currentDir}`・`navigateFolderUp` の `{...fs, currentDir: parent}` は spread が kind を保つため無変更。
- **enterToolSelection**: frame を `const frame: ToolSelectionFrame = { kind: "tool", ... , launchQuery: query(), savedFolderFilter: folderFilter() }` に（型注釈で全必須フィールド充足をコンパイル検査）。
- **launchWithSelectedTool**: `frame.savedQuery` → `frame.launchQuery`（2 箇所: trace の query, `api.launchWithTool` 引数）。
- **doc コメント更新**（plan-review W2）: `saveView`（`search.ts:48`）/`restoreView`（`search.ts:55`）の「savedQuery 等」を参照する stale コメントを、型分離後（restoreQuery/launchQuery）+ popView 層の説明へ更新。
- **クリア系は pop discipline の外に置く**（独立再導出の指摘）: `resetForShow`・`launchAndReset`/`launchWithSelectedTool` の onSuccess は両スロット null + `clearResults`（スナップショット復元を伴わない全消し＝pop ではない）。現状のまま維持し popView に巻き込まない。

### 5. `ui/src/stores/search.test.ts`
- 既存 fixture 更新: **frame literal は search.test.ts 全体に散在**（plan-review R1・実測）。inline `setToolSelectionState({...})` が約 10 箇所 + 定数 `FOLDER_FRAME`/`TOOL_FRAME` の 2 個。すべてに `kind` 追加 + `savedQuery` → `restoreQuery`/`launchQuery` が要る。**個別行列挙に依存せず、Phase 1 の `npm run typecheck`（実 setter へ渡す型付き literal ゆえ `kind`/改名欠落が全件型エラー化）を改名検出器として全サイトを機械補足する**（AGENTS.md「accessor/型の改名は compile-fail を改名検出器に」）。enterToolSelection の `savedQuery` アサーション → `launchQuery`（テスト名も併せて）。
- **新 describe「ViewStack push/pop（2 段スタック復元）」**（issue テスト方針）:
  - results→folder: enterFolderExpansion が results/selected を snapshot、`folderState().kind==="folder"`、`restoreQuery` に query 捕捉。
  - folder→tool: 上に enterToolSelection、`savedFolderFilter` に folder の filter 捕捉、`kind==="tool"`。
  - pop tool: exitToolSelection で results/selected 復元 + folderFilter が folder のものに復帰、folderState は残存。
  - pop folder: exitFolderExpansion で results/selected 復元 + query が restoreQuery に復帰、folderState null → viewKind "results"。
  - pop 順序: tool を先に pop する（`exitToolSelection() || exitFolderExpansion()` の短絡と一致）。

### 6. `ui/CLAUDE.md`
- stores/folder.ts・tool-selection.ts の記述に「`kind` 判別子 + 用途別クエリ名（restoreQuery/launchQuery）」を反映。search.ts の choke point 一覧に `popView`（pop discipline）を追記。

### 影響なしを確認した箇所（触らない）
- `SearchWindow.tsx`: Escape 短絡・enter 呼び出し・inputValue/placeholderText は公開 API 不変ゆえ無変更（frame 直読は `targetPath`/`currentDir` で改名対象外）。
- `SearchWindow.test.tsx`: frame の**部分モック**（`{ targetPath }` 等・loose typed）で `savedQuery` 未参照 → 無変更（kind 追加も loose mock ゆえ不要。実装時に typecheck で最終確認）。
- SPEC.md: 後述（更新不要）。

## 実装順序（フェーズ分け）

- **Phase 1（型分離）**: types.ts コメント → folder.ts（kind + restoreQuery）→ tool-selection.ts（kind + launchQuery）。この時点で search.ts はコンパイルエラー（`savedQuery` 参照）＝**改名検出器**として全参照箇所を炙り出す。
- **Phase 2（search.ts）**: viewKind 射影化 → popView 新設 → exit 2 関数を委譲 → enter 2 箇所の frame 構築 + launchWithSelectedTool の launchQuery 反映。`npm run typecheck` green を確認。
- **Phase 3（テスト）**: 既存 fixture 更新（Red にならないこと）→ 新 2 段スタック describe 追加。`npm run test`（ui）green。各 Phase 完了で検証 green を確認（PostToolUse hook が clippy/typecheck を自動発火）。
- **Phase 4（ドキュメント）**: ui/CLAUDE.md 反映。SPEC 更新不要の裏取り。

## 不変条件（保つべきもの）

1. **2 段スタック復元**: tool pop → `savedFolderFilter` で folder のフィルタ復帰（folderState 残存）。folder pop → `restoreQuery` で query 復帰。復元順序は tool→folder（Escape 短絡）。
2. **直交性・優先度（SPEC §18.5）**: `toolSelectionState !== null` > `folderState !== null` > results。viewKind=頂点.kind が保存。tool は folder の上に直交して積まれる。
3. **boolean 契約**: `exitToolSelection`/`exitFolderExpansion` は自スロット在時のみ true。`enterToolSelection: Promise<boolean>`（≤1 ツールで fallback 起動時 true）。公開シグネチャ全て不変。
4. **リアクティブ順序（真の load-bearing 順序に修正・plan-review 発見2）**: folder pop で `setFolderState(null)` を **`setFolderFilter("")` より先**（null 後なら folderFilter effect の `if (folderState())` ガードが stray refresh をスキップ）。「setQuery より先」という旧コメント文言は #537 以降 vestigial（folder 中 query 不変ゆえ `setQuery(restoreQuery)` は同値 no-op）だが、3 set の順序（null → filter → query）を厳密保存すれば挙動は保たれる。非 batch 逐次 set を維持（batch 化しない）。
5. **cancelDebounce の folder 固有性（挙動不変・plan-review 発見1 で修正）**: popView の cancelDebounce は **folder 経路のみ・invalidate より前**。tool 経路は現 `exitToolSelection` が cancelDebounce を呼ばない挙動を厳密保存する。**両経路で cancel すると挙動変化**（下記の稀な stray refresh を抑制してしまう）になり issue の「挙動不変」に反するため、対称化しない。
6. **enter 非対称の保存**: enterFolder は invalidate/cancelDebounce を呼ばない・深掘りは frame 書き換え。enterTool は両方呼ぶ。enter 側は frame 形状のみ変更。
7. **frame 値直読**: launchWithSelectedTool=launchQuery、inputValue/placeholderText=targetPath/currentDir。
8. **網羅性のコンパイル担保**: popView の switch + `assertNever(frame)` で 3 つ目の modal kind 追加を型エラー化。
9. **viewKind プリミティブ**: string（kind）返却で値ゲート伝播。配列 signal/memo を導入しない。

### 破壊不変条件（壊れたら即アウト・検知手段付き）
- **Escape 段階離脱**: 壊れるとモーダルから抜けられず hideMainWindow 誤発火。検知＝search.test.ts の exit テスト + 新 2 段スタック describe（pop 順序）。
- **型分離の取り違え**（launchQuery⇄restoreQuery）: launch に誤クエリ or 復元失敗。検知＝union 型分離によりコンパイルエラー（tsc/PostToolUse hook）+ enterTool の launchQuery アサーション + folder 復元テスト。
- **順序不変条件の破壊**: folder pop の set 順序が変わると（vestigial とはいえ）想定外の中間リアクティブ状態。検知＝手動での順序レビュー + 既存 folder/instant テストの green 維持。

## テスト方針

- **追加/更新テスト**:
  - 更新: exitToolSelection fixture（kind/launchQuery）、enterToolSelection の launchQuery アサーション。
  - 追加: 「ViewStack push/pop（2 段スタック復元）」describe（results→folder→tool→pop→pop の順序・folderFilter 復帰・restoreQuery 復帰・folderState 残存/消滅）。
- **検証コマンド**（docs/build-commands.md 準拠）: `npm run typecheck`（ui）、`npm run test`（ui・vitest）。PostToolUse hook が `.ts` 編集で typecheck を自動発火（沈黙=合格）。
- **Red→Green**: Phase 1 の改名で search.ts が compile-fail する（改名検出器）。新テストは実装前に Red を確認（2 段スタックの pop 順序）。

## SPEC.md 更新要否

**更新不要**。SPEC §18.5 は「`toolSelectionState !== null` > `folderState !== null` > 通常モード」「toolSelectionState は folderState と直交」と記述。本 refactor は 2 シグナルを SSOT として維持し、viewKind=頂点.kind は同じ優先度を射影するため、§18.5 の文言は as-built と一致し続ける。挙動・IPC 契約・状態遷移・公開 API はいずれも不変（issue「挙動不変・公開 API 不変」）。→ AGENTS.md step 0 の「文書化された挙動を変えたら仕様変更」に**該当しない**。Phase 4 で §18.5 の文言が実装と一致することを裏取りする。

## スコープ外で発見した潜在事項（本 issue では修正しない）

- **enterToolSelection の await 窓での stray refresh race**（plan-review 発見1）: `enterToolSelection` は先頭で `cancelDebounce()` するが、`await api.getMatchingTools()` 中はまだ tool frame 未設定＝`handleInput` のガード（`viewKind()==="tool"`）が効かず `launching()` も false。この窓でユーザーが打鍵すると `debouncedRefresh` が timer を張り、tool 化後も pending として残る。Escape で `exitToolSelection`（cancelDebounce 呼ばず）した直後に timer が発火すると、復元スナップショットを stray refresh が上書きしうる（窓は狭い: getMatchingTools 解決が速く、かつ残余 <50ms で Escape）。**本 refactor はこの現挙動を保存する**（挙動不変が要件）。修正するなら別 issue（`enterToolSelection` の await 中入力ガード or exit の cancel）が適切。**この計画では意図的に温存する**。

## セルフレビュー

### 5a. check スキル結果

- **/plan-review**（Explore 2 + Plan 1 独立再導出）: 下記に反映済み。
  - **要対処 R1**（test fixture 列挙の過小）→ §5 を「typecheck を改名検出器として全サイト補足」へ修正済み。出荷破損リスクは無い（Phase 1 の tsc が全件炙り出す）。
  - **軽微 W1**（search.ts の型 import 欠落）→ §4 に import 追記済み。
  - **軽微 W2**（saveView/restoreView の stale コメント）→ §4 に更新項目追記済み。
  - **発見1**（cancelDebounce 対称化は挙動変化・race あり）→ popView を folder 固有 cancelDebounce に修正済み（不変条件 5）。潜在 race は上記スコープ外へ記録。
  - **発見2**（リアクティブ順序コメント誤記）→ 不変条件 4 を真の load-bearing 順序（setFolderFilter より先）へ修正済み。
  - **独立再導出との一致（完全性の能動的証拠）**: 設計判断（2スロット+射影 ＞ 単一配列 signal・scope-out + 配列アンチパターンが論拠）、kind 判別子、popView switch + assertNever（onExit メソッド却下）、exit のスロット固有性維持（boolean 契約）、viewKind 読者は全てプリミティブ契約消費で無改修、SPEC §18.5 更新不要 — **主要判断がすべて独立に再一致**。production コード + e2e は変更集合で漏れなく被覆（唯一の実質漏れは test fixture 列挙で、これは typecheck が補足）。
- **/state-check**（直交性・リセット経路・入力分岐・SPEC §8.6/§18.5）: **状態モデルとの不整合なし**。新モード追加ではなく viewKind 射影の実装差し替え + exit の集約ゆえ、直交性（tool>folder>results）・resetForShow の両スロット null 化・Escape 連鎖（`exitTool() \|\| exitFolder()`）・SearchWindow.tsx 入力分岐（値契約不変）はすべて保存。SPEC §8.6 のガードはシグナル存在（`!toolSelectionState`/`folderState`）で表現され、両シグナル維持ゆえ図は as-built のまま正確 → 更新不要。

### 5b. セルフレビューチェックリスト

1. **対称コードパス**: enter/exit・push/pop・snapshot(saveView)/restore(restoreView)・生成(set 非null)/破棄(set null) の対称ペアを /plan-review Explore で検証。両側被覆確認済み。exit のスロット固有性（頂点 pop でなく自スロット pop）で boolean 契約を維持。
2. **影響範囲の網羅性**: `savedQuery`/`folderState`/`toolSelectionState`/enter*/exit*/`viewKind`/frame 型を ui/src・e2e で grep。production/e2e は被覆、test fixture は typecheck で補足。
3. **境界条件**: 2 段スタック（results→folder→tool→pop→pop）・tool 単独・folder 単独・≤1 ツールの fallback・Escape 段階離脱を新テストで網羅。
4. **リソース管理**: frame は in-memory signal（生成/破棄ペア）。永続化なし（/persistence-check 不要・実測）。timer（refreshTimer）の cancel 経路は folder pop で保存、tool pop は現挙動どおり呼ばない。
5. **既存パターンとの整合**: discriminated union + `assertNever`（既存 viewKind/shouldShowResults 規約）、プリミティブ判別子メモ、saveView/restoreView choke を踏襲。新規パターンなし。
6. **YAGNI 違反**: 単一配列 signal / lib/viewStack.ts 抽出 / pushView 別関数化 / onExit メソッド化 — いずれも見送り（独立再導出も同結論）。push は saveView choke + kind タグ付き構築の one-liner。
7. **シンプル化の挑戦**: 新規状態フラグ・Mutex・子プロセスは導入しない。追加するのは frame の `kind` フィールド（判別子）と popView（既存 exit ロジックの集約）のみ。「この操作が失敗したら」= frame 構築は同期・失敗経路なし。enterToolSelection の async 失敗（getMatchingTools throw）は現状どおり catch → false（frame 未設定）。
8. **破壊不変条件の明示**: Escape 段階離脱・型分離の取り違え・順序不変条件を「破壊不変条件」節に検知手段付きで列挙済み。型分離は union でコンパイル担保、Escape は search.test.ts + 新 2 段スタック describe が検知。

**実装着手可否: 可**（計画 completeness 高。要対処 R1 は反映済み、残りは typecheck が機械補足する自己修復的項目）。
