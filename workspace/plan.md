# plan — issue #537: 入力解釈を純関数化し `suppressNextQueryEffectRefresh` を撤廃する

## 0. 判定: バグか仕様変更か

**リファクタ（挙動不変が受け入れ条件）**。SPEC.md の記載フロー・IPC 契約・状態遷移を **変えない**。ゆえに SPEC.md 更新は不要（§4 で再確認）。公開 API 不変。

## 1. 受け入れ条件（テスト可能な形）

- AC1: `suppressNextQueryEffectRefresh` の宣言・set・consume がコードから消える（grep で 0 件）。
- AC2: 純関数 `interpret(rawQuery, prefix, viewKind)` が存在し、副作用なしで意図（plain/command/instant + instant の filterName/instantQuery）を返す。単体テストで固定。
- AC3: ユーザー入力（`handleInput`）とプログラムリセット（`resetForShow`/instant 成功/`clearCommandModeState`）が別経路。前者だけが dispatch を呼ぶ。
- AC4: 挙動不変 — 既存 396+ テスト（移行後）が緑。特に (a) `resetForShow` 後に不要な `api.search` 不発火、(b) instant 成功後に plain 検索不発火、(c) tool/folder ガード維持、(d) leading/trailing debounce の等価。

## 2. 変更ファイル一覧

### 新規: `ui/src/lib/interpretQuery.ts`（純関数・SolidJS/api 非依存）

`folderNav.ts`/`windowHeight.ts` の「テスト可能な純ロジックを lib へ分離」パターンに倣う。

```ts
export type ViewKind = "results" | "folder" | "tool";
export type InterpKind = "plain" | "command" | "instant";

export type QueryIntent =
  | { kind: "plain" }
  | { kind: "command" }
  | { kind: "instant"; filterName: string; instantQuery: string };

/** instant モード検出述語（search.ts から移設・instant 判定の SSOT）。空 prefix では false。 */
export function isInstantPrefix(rawQuery: string, prefix: string): boolean {
  return prefix !== "" && rawQuery.trimStart().startsWith(prefix);
}

/** 入力 → 意図。副作用なし。viewKind!=="results" は常に plain（folder/tool 中は非 plain 化しない）。
 *  instant の parse（prefix 除去・スペース分割）を一箇所に集約し、handleInstantQueryInput と
 *  executeInstantCommandSelected の二重抽出を DRY 化する。 */
export function interpret(rawQuery: string, prefix: string, viewKind: ViewKind): QueryIntent {
  if (viewKind !== "results") return { kind: "plain" };
  if (isInstantPrefix(rawQuery, prefix)) {
    const input = rawQuery.trimStart().slice(prefix.length);
    const spaceIdx = input.indexOf(" ");
    const filterName = spaceIdx >= 0 ? input.slice(0, spaceIdx) : input;
    const instantQuery = spaceIdx >= 0 ? input.slice(spaceIdx + 1) : "";
    return { kind: "instant", filterName, instantQuery };
  }
  if (rawQuery.trimStart().startsWith("/")) return { kind: "command" };
  return { kind: "plain" };
}
```

### 変更: `ui/src/stores/search.ts`

1. **import 追加**: `import { interpret, type ViewKind, type InterpKind, type QueryIntent } from "../lib/interpretQuery";`（`isInstantPrefix` は import **しない**——下記 item2 参照）。
2. **型・述語の移設**: L85-86 の `ViewKind`/`InterpKind` 定義を削除し lib から re-export（`export type { ViewKind, InterpKind } from "../lib/interpretQuery";` は public API 維持のため。外部 import 者はゼロだが後方互換保険）。L95-97 の `isInstantPrefix` 定義を削除し lib へ移設。**search.ts へは import で呼び戻さない**——interpKind memo を interpret 経由（item3）にすると search.ts 内に `isInstantPrefix` の直接参照が残らず、import すると dead import になる（plan-review 影響範囲エージェント指摘 C。`noUnusedLocals` 無効ゆえビルドは通るが不要）。
3. **`interpKind` memo（L106-112）を interpret 経由へ**: `const interpKind = createMemo<InterpKind>(() => interpret(query(), instantCommandPrefix(), viewKind()).kind);`。**プリミティブ（`.kind` 文字列）を返す契約を維持**（オブジェクト union を下流へ流さない・ui/CLAUDE.md）。
4. **`suppressNextQueryEffectRefresh` 撤廃**: 宣言（L68）削除。
5. **query effect を明示 dispatch 関数 `dispatchQueryInput(value)` へ置換**（下記）。`createEffect(on(query, ...))`（L314-359）を **削除**し、同等の分岐を `dispatchQueryInput` に移す。
6. **`handleInstantQueryInput` を filterName 受け取りへ**: 内部再抽出（L256-261）を削除し `handleInstantQueryInput(filterName: string)` に。dispatcher が `intent.filterName` を渡す（DRY）。
7. **`executeInstantCommandSelected`（L631-635 と L652）**: instantQuery 抽出を interpret 経由に統一（`const intent = interpret(query(), instantCommandPrefix(), viewKind()); const instantQuery = intent.kind === "instant" ? intent.instantQuery : "";`）。成功時の `suppressNextQueryEffectRefresh = true;`（L652）を **削除**（raw `setQuery("")` は dispatch しないため不要）。
8. **`resetForShow`（L735-737）**: `if (query() !== "") { suppressNextQueryEffectRefresh = true; }` を **削除**。`skipRefresh` 判定と末尾の明示 `runRefresh()`（L742-744）は不変。
9. **export 更新**: `interpret` は lib から直接テスト。`dispatchQueryInput` を export（SearchWindow + テストが使う）。

`dispatchQueryInput`（旧 effect 本体と等価・**diff 最小化**のため分岐構造を保存）:

```ts
/** ユーザー入力の明示 dispatch（唯一の検索起動起点）。setQuery で query を更新し、interpret の
 *  意図に基づいて instant/command/plain へ振り分ける。プログラム的リセット（resetForShow 等）は
 *  この関数を呼ばない別経路であり、ゆえに「今回だけ effect を黙らせる」フラグは不要になった。 */
function dispatchQueryInput(value: string) {
  setQuery(value);
  const vk = viewKind();
  // tool/folder ガード（旧 effect の early return を保存・防御的。実運用では handleInput が
  // tool で早期リターン・folder で setFolderFilter に振るためここには results 時のみ到達）。
  if (vk === "tool") { trace("search:query_input:ignored_tool_selection", { query: value }); return; }
  if (vk === "folder") { trace("search:query_input:ignored_folder_mode", { query: value }); return; }
  trace("search:query_input", { query: value, trimmed: value.trim() });

  const intent = interpret(value, instantCommandPrefix(), vk);
  if (intent.kind === "instant") { handleInstantQueryInput(intent.filterName); return; }

  // instant 資源掃除（旧 L343-346・純粋導出ゆえ資源が現存するときだけ）
  if (hasPendingInstantCommandFetch() || getInstantCommandItems().length > 0) {
    cancelInstantCommandDebounce();
    clearInstantCommandItems();
  }
  switch (intent.kind) {
    case "command": handleCommandQueryInput(value); return;
    case "plain": handlePlainQueryInput(value); return;
    default: return assertNever(intent.kind);
  }
}
```

（`createRoot` 内には `folderFilter` effect のみが残る。§3 の folderFilter 決定を参照。`createEffect`/`on` の import は folderFilter effect が使い続けるため残す。）

### 変更: `ui/src/components/SearchWindow.tsx`

- `handleInput`（L250-252）の results 経路: `setQuery(value);` → `dispatchQueryInput(value);` に置換。import を `setQuery` → `dispatchQueryInput` へ（`setQuery` はここでは不要になる。他に SearchWindow が `setQuery` を使う箇所は無い＝L6 のみ）。folder 経路の `setFolderFilter(value)` は不変。

### 変更: `ui/src/stores/search.test.ts`

- **移行規則**: 「dispatch の副作用（`api.search`/`api.getInstantCommands`/`results`/`shouldShowResults`/`selected` の結果依存）を検証するテスト」の `setQuery(x)` を `dispatchQueryInput(x)` に置換。**`interpKind()`/`viewKind()`/`allowsFolderNav()` のみを同期検証するテスト・状態 setup・明示 `refreshResults()` を呼ぶテストは raw `setQuery` のまま**（`interpKind` は純粋 memo ゆえ raw で緑）。
- **移行対象（dispatch 副作用に依存）**: L424, L531, L542, L576, L655, L676, L694, L907, L915, L945, L956, L958, L960, L984, L989, L996, L998, L1000, L1015。
- **raw 維持（memo/setup/明示 refresh）**: L103, L153, L249, L270, L294, L387, L551, L555, L562, L632, L792, L797, L803, L811, L817, L819, L825, L848, L877, L1125。
  - **L551/555/562 は当初 migrate に誤分類**（plan-review 指摘 B）。これらは `interpKind()` のみを検証（「プレフィックスを消すとモード解除」「resetForShow でモード解除」）。`interpKind` は純粋 memo ゆえ raw `setQuery` で同期導出され緑。自ルール（memo 検証は raw）に照らし raw が正。付随の `getInstantCommands` モック・`runAllTimersAsync` は inert な no-op になる（害なし）。
- `dispatchQueryInput` を import に追加。
- **新規テスト**（§4 テスト方針）。

### 変更: `ui/CLAUDE.md`

- lib/ 節に `interpretQuery.ts`（純関数・interpret/isInstantPrefix/型）を追記。stores/search.ts 節の `suppressNextQueryEffectRefresh` 記述（L25 付近）を「`dispatchQueryInput`（明示 dispatch・唯一の検索起動起点）」へ書き換え。「インスタントコマンドモード > 検出」の「query effect 内で…先に startsWith(prefix)」を interpret/dispatch 由来へ更新。「状態モデル（2 軸）」の interpKind 記述に interpret 由来を反映。

### 変更: `.claude/skills/race-check/SKILL.md`（ユーザー合意済み・Phase 4 で code と同時適用）

- **L60** の Step 3 状態変更経路の表。旧フロー記述を新フローへ更新（独立再導出が指摘）。**エージェント設定の変更ゆえユーザー合意が要る領域だったが、合意取得済み**（本 refactor のスコープに包含）。
  - before: `| `handleInput` → `setQuery` → query effect | ユーザーのキー入力 | `query`, `results`, `selected`, `searchLane` 世代（`run`/`invalidate`）, `instantCommandItems` |`
  - after: `| `handleInput` → `dispatchQueryInput` | ユーザーのキー入力 | `query`, `results`, `selected`, `searchLane` 世代（`run`/`invalidate`）, `instantCommandItems` |`
  - **適用タイミング**: Phase 4（ドキュメント同期）で `search.ts`/`SearchWindow.tsx` の code 変更と **同時に**行い、skill と実コードの同期を保つ（skill を code より先行させない＝「存在しないフローを指す skill」を作らない）。
- `docs/development-principles.md:52`（isInstantPrefix SSOT の歴史的記述）は更新任意（二次・本 PR 対象外でよい）。

### 非挙動の縮退（記録のみ）

- `handleInstantQueryInput` を filterName 受け取りに変えると trace（`search.ts:262`）の `{prefix, input}` フィールドが失われる（dev 専用ログ・挙動不変）。必要なら trace 呼び出し側で補完する。

## 3. 決定: `folderFilter` effect は本 refactor に含めない

issue が「検討」と明記した点。**含めない**と決定する。根拠:
- `folderFilter` effect は `suppressNextQueryEffectRefresh` を **使っていない**。判定は `folderState()` ガード＝状態由来の述語で、set/consume の順序に依らず常に正しい（フラグのような fragile なワンショットではない）。本 issue の受け入れ条件（suppress 撤廃）に無関係。
- 残る非対称（query=明示 dispatch / folderFilter=effect）は「folder refresh は state signal への真のリアクション」という正当な差。consistency のためだけの拡張は #536 retrospective「偶発的複雑さを設計で消す／過剰一般化しない」に反する（YAGNI）。
- リスク抑制: effect 発火経路の変更対象を query 一本に絞ることで、差分が読みやすくレビュー可能になる。

## 4. 実装順序（フェーズ分け）

- **Phase 1 — 純関数抽出（Red→Green）**: `lib/interpretQuery.ts` + `lib/interpretQuery.test.ts` を新規作成。`interpret` の単体テストを先に書き（Red）、実装で緑に。search.ts はまだ触らない（既存緑を保つ）。この時点で `isInstantPrefix`/型は lib と search.ts に **重複**して構わない（Phase 2 で search 側を削除）。
- **Phase 2 — search.ts の載せ替え（挙動不変）**: interpKind memo を interpret 経由に、query effect → `dispatchQueryInput`、suppress 撤廃、handleInstantQueryInput(filterName)、executeInstant の instantQuery 統一。search.ts の `isInstantPrefix`/型定義を削除し lib import + re-export に。
- **Phase 3 — テスト移行 + 回帰網追加**: search.test.ts の移行規則適用 + 新規回帰テスト。SearchWindow.tsx の handleInput 差し替え。SearchWindow.test.tsx（L258/269 の setQuery モック検証）への影響確認（下記）。
- **Phase 4 — ドキュメント同期**: ui/CLAUDE.md 更新 + `.claude/skills/race-check/SKILL.md:60`（handleInput フロー・合意済み）を code 変更と同時に更新。
- 各 Phase 末で検証（PostToolUse hook の typecheck/clippy は沈黙=合格。手動 `npm run test` を Phase 2/3 で実行）。

### テスト方針（AC2/AC4）

1. **`interpret` 単体（新規 `lib/interpretQuery.test.ts`）**: plain / command(`/`) / instant(`@`) / 空 prefix は非 instant / filterName 抽出（スペース有無）/ instantQuery 抽出（スペース以降・スペース無しは ""）/ viewKind≠results は常に plain / leading whitespace（`  @goo`）。純関数ゆえ SolidJS 不要。
2. **suppress 意図の回帰ガード（既存強化）**:
   - `resetForShow` 後 `api.search` 不発火（L293-302 既存・撤廃後も緑を確認）。
   - instant 成功後 `api.search` 不発火（既存 L581-594 に `expect(api.search).not.toHaveBeenCalled()` を **追加**。plain 誤発火の直接ガード）。
3. **明示 dispatch の等価**: 移行後の debounce adapter（L930-968）/ instant adapter（L975-1007）が leading/trailing・items 即時クリアを引き続き固定。
4. **プログラム的 setQuery が dispatch しない（新規・任意）**: `setQuery("x")`（raw）直後 `runAllTimersAsync` で `api.search` 不発火を固定（effect 撤廃の直接証明。dispatchQuery 経由のみ検索する contract）。

### SearchWindow.test.tsx への影響（plan-review 指摘 A — 要対処）

`handleInput` の `setQuery` → `dispatchQueryInput` 化に伴い、以下すべてが必要（**当初計画は L284 のテストを見落としていた**。3 件の mockSetQuery アサートがある）:

1. **プラミング（必須・怠ると全 SearchWindow テストが RED）**: `vi.hoisted`（L7-14 付近）に `mockDispatchQueryInput: vi.fn()` を宣言し、`vi.mock("../stores/search", ...)` オブジェクト（L65-90 付近）に `dispatchQueryInput: mockDispatchQueryInput` を追加。追加しないと `SearchWindow.tsx` の `dispatchQueryInput` import が undefined になり handleInput が throw。
2. **アサート更新（3 件）**:
   - L269「通常モードで文字入力すると setQuery が呼ばれる」（アサート L274）→ `mockDispatchQueryInput` へ。
   - L277「インスタントコマンドモード中も文字入力を受理する」（アサート L284 `toHaveBeenCalledWith("@goo")`）→ `mockDispatchQueryInput` へ。**instant は `viewKind()==="results"` ゆえ handleInput の else 枝（旧 setQuery）を通る**ため、raw のままだと RED。
   - L258「ツール選択中に文字入力しても setQuery が呼ばれない」（アサート L265）→ `mockDispatchQueryInput` **not called** に更新（tool ガードで dispatch も呼ばれない意図を保存）。
3. `mockSetQuery` mock 自体は残しても無害（他で `setQuery` を呼ぶ経路なし）。

## 5. 不変条件（守るべき）

- **INV1（suppress 2 意図の保存）**: (a) `resetForShow` で query を空へ戻しても不要な検索 IPC を発火しない（明示 `runRefresh()` 経路と二重にならない）。(b) instant 成功で query を空へ戻したときの余計な検索を発火しない。→ effect 除去 + raw setQuery で **構造的に**保証（フラグ順序契約に依らない）。回帰網 §4-2 が実測ガード。
  - **framing 精緻化（plan-review 指摘）**: (b) で suppress が実際に抑えていたのは「plain 検索の誤発火」ではなく **空 refresh の churn（+ `searchLane` 世代前進）**。instant 成功後の query は空で、`refreshResults` は empty_query 分岐（L223-224）ゆえ **元々 api.search を呼ばない**。ゆえに追加テスト `expect(api.search).not.toHaveBeenCalled()` は before/after 両方で緑＝正しい回帰ガードだが、「plain 検索誤発火防止」という表現は過大。effect 除去はこの churn を構造的に消すため等価かつより明快。
- **INV2（interpKind 純粋導出・プリミティブ返し）**: interpret 経由でも `interpKind` は query+prefix+viewKind からの純粋導出を保ち、持続ラッチを持たず、**文字列（プリミティブ）を返す**（#374/#455 の設計・下流の毎打鍵再計算を防ぐ）。
- **INV3（tool/folder ガード）**: tool/folder モード時に query 入力が検索を上書きしないガードを `dispatchQueryInput` に保存。加えて `refreshResults` 冒頭の `viewKind()==="tool"` / `interpKind()==="instant"` early-return（L164-165）は **不変**（world 世代を進めない #534 核心）。
- **INV4（SolidJS 実行モデル）**: `setQuery(value)` 直後に `interpret(value, ...)` を同期で読むため、バッチ/順序は破壊されない。`dispatchQueryInput` は同期関数（async の await を跨がない）。
- **INV5（instant 資源掃除の条件）**: plain/command 遷移時の pending fetch / stale 候補掃除は「資源が現存するときだけ」（無ければ no-op）を保存。
- **異常系**: `dispatchQueryInput` は同期・例外を投げる下流呼び出しなし（`handleInstantQueryInput` の scheduleInstantCommandFetch は内部で onError 捕捉）。予期しない順序で呼ばれても、各呼び出しは独立に query を上書きするだけ（残留状態フラグを持たない＝フラグ撤廃の効用）。

## 6. SPEC.md 更新要否

**不要**。挙動不変（IPC 契約・状態遷移・フロー不変）。SPEC §18.5（instant 優先度）・§8.6（状態図）の記述は interpKind/viewKind の意味論に触れるが、それらの **意味は変えない**（interpret は同じ分類を返す）。実装内部の制御フロー（effect→明示 dispatch）は SPEC の管轄外。

## セルフレビュー（Step 5b・5a の結果を反映）

1. **対称コードパス**: query effect の「set/consume」対称は、suppress フラグの撤廃で **対称自体が消滅**（片側だけ残る危険なし）。instant 資源の生成（scheduleInstantCommandFetch）/破棄（cancelInstantCommandDebounce+clearInstantCommandItems）の対は掃除ロジック（INV5）で保存。`/symmetric-check` 対象。
2. **影響範囲の網羅性**: `setQuery` 全呼び出しを grep 済み（search.ts **4 call-site**: L142/L410/L653/L738、SearchWindow 1: L251、test **39 箇所**）。`ViewKind`/`InterpKind` 型の外部 import なし（grep 済み・テストはインライン union で代用）。`isInstantPrefix` 利用は interpKind のみ（grep 済み）。**`interpKind`/`viewKind` 消費者**（契約プリミティブ不変ゆえ無変更・完全性のため列挙）: search.ts の `allowsFolderNav`/`shouldShowResults`/`refreshResults` ガード/`tryModalActivate`/`resetForShow` skipRefresh、SearchWindow の複数箇所、**`MainApp.tsx:325` `skipIcons={interpKind()==="instant"}`**（独立再導出が指摘）。3 エージェント（影響範囲・不変条件・独立再導出）が独立に同一変更集合へ収束＝完全性の能動的証拠。
3. **境界条件**: 空 prefix / leading whitespace / スペース無し instant / viewKind≠results — §4-1 で網羅。プログラム的 setQuery 4 経路を research.md の表で追跡済み。
4. **リソース管理**: 新規リソース（timer/process/フラグ）を **導入しない**。むしろ可変フラグ 1 個を除去。lib の interpret は純関数（状態なし）。
5. **既存パターン整合**: lib への純ロジック分離（folderNav 前例）・dispatch 3 関数の再利用・interpKind memo の再実装。新規パターンなし。
6. **YAGNI**: interpret の戻り値は dispatcher と executeInstant が実際に使う field のみ（filterName/instantQuery）。folderFilter は含めない（§3）。新規ファイルは purity/isolated-test の実益がある範囲に限定。
7. **シンプル化の挑戦**: 可変 module フラグ → 純関数 + 明示経路への転換そのものが単純化。`dispatchQueryInput` は新状態を持たない同期関数。「この操作が失敗したら」= 純関数ゆえ失敗しない・残留状態なし（INV6 相当）。**懸念点**: (i) `lib/interpretQuery.ts` の新規ファイルは surface 増 → in-place 案と天秤。folderNav 前例と isolated-test 実益を根拠に採用（plan-review で再評価）。(ii) interpret が memo 内でオブジェクト割り当て → memo は `.kind`（プリミティブ）を返すため下流伝播は不変、割り当ては memo 計算内に閉じる（毎打鍵の再計算は query 変化時のみ＝現状と同じ）。(iii) **依存グラフの微拡大**（不変条件エージェント指摘）: 現行 interpKind は `viewKind()!=="results"` で早期 return し query/prefix を読まないが、interpret へは query()/prefix() を先行評価で渡すため folder/tool 中も購読し続ける。**挙動差なし**（値は "plain" 固定で伝播ゲート遮断・folder 中 query 凍結・tool 中 setQuery 皆無で再計算ほぼ発火せず）。許容。より厳密に現行の依存を保つなら memo を `viewKind()!=="results" ? "plain" : interpret(query(), prefix(), "results").kind` にできるが、viewKind ガードの二重化を招くため採らない（KISS）。
8. **破壊不変条件の明示**: 「戻ってこない系」（Win32 フック・ホットキー・IPC）は **本変更に無し**（UI 層内の制御フローのみ）。壊れたら即アウトなのは INV1（suppress 2 意図）で、検知手段は §4-2 の回帰テスト（`resetForShow`/instant 成功後の `api.search` 不発火）+ Phase 2/3 の `npm run test` 全緑。手動スモーク: `npm run tauri:dev` で (a) 検索→表示→再表示で余計な検索が走らないか、(b) instant コマンド実行後に空検索が走らないか、(c) `/` コマンド後の挙動、を trace（`localStorage.snotra_trace=1`）で確認。

## plan-review 結果（Step 5a・3 サブエージェント + Step 2b 独立再導出）

- **要対処（反映済み）**: (A) SearchWindow.test.tsx の mockSetQuery アサートは **3 件**（L269/277/258）+ vi.hoisted/vi.mock プラミング → §2 SearchWindow.test.tsx 節を全面改訂。
- **軽微（反映済み）**: (B) L551/555/562 の over-migration → raw へ移動。(C) `isInstantPrefix` を search.ts へ import しない（dead import 回避）→ item1/2 修正。(E) 件数プロース修正（search.ts 4・test 39）。framing 精緻化（INV1(b)・clearCommandModeState 真因）。interpKind 依存微拡大の明記。
- **独立導出との差分（Step 2b）**:
  - 漏れ（導出 ∖ plan）: `MainApp.tsx:325` の interpKind 消費者（無変更確認・列挙追加）、race-check SKILL.md:60 の旧フロー記述（**ユーザー合意取得済み**・Phase 4 で code と同時更新）、SearchWindow.test.tsx L284 の見落とし（= 要対処 A と一致）。
  - スコープ過剰（plan ∖ 導出）: なし（独立導出も新規ファイル化・instantQuery 統合を支持）。
  - 一致（完全性の証拠）: 変更集合の骨格（新規 interpretQuery.ts + 5 種の search.ts 変更 + SearchWindow.tsx + テスト移行）、**folderFilter 除外**、**SPEC.md 更新不要** が独立に再一致。3 視点収束。
- **`/state-check`・`/symmetric-check` の扱い**: 専用スキルは起動せず、観点を plan-review エージェントに織り込んで検証した（重複起動回避・チーム憲章「やりすぎでは」）。state-check 領域（tool/folder/instant ガード・reset 経路・2 軸直交性）は不変条件エージェントが INV3 で、symmetric-check 領域（suppress set/consume ペアの撤廃・instant 資源 生成/破棄の INV5）は影響範囲+不変条件エージェントが検証し、いずれも 要対処なし。2 軸モデルの分類（interpret が返す kind）は不変ゆえ直交性・SPEC §8.6 は変わらない。
- **総評**: completeness **高**。実装着手 **可**（要対処 A は計画へ反映済み。race-check SKILL.md 更新はユーザー合意取得済みで Phase 4 に包含。残る gate なし）。
