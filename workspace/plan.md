# plan.md — 検索モードの二軸モデル化（散在ガードの集約）

`type:refactor`（挙動不変）。issue 未起票。

## 背景・課題

検索ウィンドウの「いまどのモードか」という単一の不変条件が、**型ではなく 11 箇所以上の
散在ランタイムガードの協調**で守られている。各ガードが考慮するモード集合が一致しておらず、
3 つの綻びを生んでいる:

1. `shouldShowResults`（`search.ts:53`）が `toolSelectionState` を見ない。遠い `refreshResults`
   の indexing ガードが「indexing 中は結果を空にする」ため*偶然*壊れていないだけ（暗黙結合）。
2. `handleInput`（`SearchWindow.tsx:237`）が `instantCommandMode` を見ない。正しい挙動だが根拠は
   `// C2対策` コメントのみ（型に現れない非対称）。
3. `activateSelected` / `activateSelectedByIndex`（`search.ts:639/661`）が同じ `tool→instant→通常`
   ディスパッチを二重に持つ。

## 設計 — 「モード」を二軸 + オーバーレイに分解する

「モード」は単一の物ではなく、**直交する 2 軸を 1 つの壺に流し込んだ混合物**だった。
SPEC §8.6 の状態図・§18.5 の優先順位は既にこの構造を概念化している（コードだけが未追従）。

- **軸1 View**（結果リストを占めるもの・ビューフレームのスタック）: `results`（根）/ `folder` / `tool`。
  `folder`・`tool` は「直下の層を frame に保存して被さり、離脱で復元」する push/pop 構造。
  `tool` は `folder` の上に積まれうる（SPEC §18.5「直交」）。深さ ≤2。
- **軸2 Interp**（入力の意味・純粋関数・状態を持たない）: `plain` / `command` / `instant`。
  `View=results` のときだけ非 `plain` になりうる。
- **軸3 Overlay**（重なる状態フラグ・boolean のまま据え置き）: `indexing` / `launching`。

これらは無損失（平らな `mode` enum は優先順位で潰す時点で情報を捨てるため不採用）。

### 採用する実体化（A/B 問題の解消）

ネスト union（B案）も平らな mode メモも採らない。**既存ストレージシグナル
（`folderState` / `toolSelectionState` / `instantCommandMode`）を保ったまま、読み取りの
単一の源として `view()` / `interp()` の 2 メモを導出する**。スタックの積み重ねは既存 2 シグナルが
暗黙に保持し、pop/復元ロジックは既存 `exit*` 関数に既にある。`view()` はその先頭フレームへの射影。

→ 挙動変更ゼロ・enter/exit 関数に触れない・最小リスク。`instantCommandMode` ラッチの除去（純粋
導出化）は**任意の follow-up**（後述）に切り出し、本計画のスコープ外とする。

### 操作 × 軸 依存宣言表（実装の写像表）

凡例: View 値で分岐 / Interp 値で分岐 / Overlay / 軸を変える操作。

#### A. 入力ルーティング
| 操作 | 軸1 View | 軸2 Interp | 軸3 Overlay | 軸変更 |
|---|---|---|---|---|
| `handleInput` | `tool`→拒否 / `folder`→folderFilter / `results`→query | — | `launching`→拒否 | — |
| query effect | `tool`/`folder`→短絡 return | **分岐の主体** | — | Interp 確定 |
| `refreshResults` | `tool`→return / `folder`→listFolder / `results`→search | `instant`→return | `indexing`→クリア | — |

#### B. キーボード
| 操作 | 軸1 View | 軸2 Interp | 軸3 Overlay | 軸変更 |
|---|---|---|---|---|
| Escape | `tool`→pop / `folder`→pop / `results`→hide | — | — | 軸1 pop |
| Arrow Up/Down | 全View共通 | — | — | — |
| Arrow Right | `tool`→拒否 / 他→子展開 | `instant`→拒否 | — | 軸1 push(folder) |
| Arrow Left | `tool`→拒否 / `folder`→親 / `results`→親展開 | `instant`→拒否 | — | 軸1 push/変異 |
| Enter | `tool`→tool起動 / 他→activate | `instant`→IC実行 | — | 成功時 全軸 reset |
| Shift+Enter | `tool`→拒否 / 他→ツール選択 | `instant`→拒否 | — | 軸1 push(tool) |
| Alt+char ブロック | — | — | — | —（純キーボードガード） |

#### C. 起動ディスパッチ
| 操作 | 軸1 View | 軸2 Interp | 軸3 Overlay | 軸変更 |
|---|---|---|---|---|
| `dispatchActivate`（#5/#6 統合） | `tool`→launchWithSelectedTool | `instant`→executeInstantCommandSelected | 成功で launching | 成功時 全軸 reset |

#### D. 表示の導出（読むだけ）
| 操作 | 軸1 View | 軸2 Interp | 軸3 Overlay |
|---|---|---|---|
| `inputValue` | `tool`→ファイル名(ro) / `folder`→filter / `results`→query | — | — |
| `placeholderText` | View ごと | — | — |
| `shouldShowResults` | `tool`/`folder`→常に表示 | `instant`→表示 | `indexing`→`results+plain`のみ隠す |
| `skipIcons` | — | `instant`→true | — |

#### E. ライフサイクル（軸1 push/pop）
`enterFolderExpansion`/`enterToolSelection`=push、`exitFolderExpansion`/`exitToolSelection`=pop、
`navigateFolderUp`=folder frame 変異、`resetForShow`=全軸を根へ。

### 軸ではないもの（モデルに畳み込まない）
`activationInFlight`（並行性）・`searchGeneration`（世代/staleness）・`mainVisible`（窓可視）は
この 3 軸と独立。`view()`/`interp()` に混ぜない。

## 変更ファイル一覧

### 1. `ui/src/stores/search.ts` — 2 軸の導出 + 読み取り集約（中核）

軸の**プリミティブ判別子メモ**と網羅性ヘルパを追加。

> **【plan-review 反映・最重要】オブジェクト union メモを使わない。** `createMemo` が
> `{ kind: ... }` の新オブジェクトを毎回返すと、SolidJS 既定の `===` 等価では値不変でも
> 毎回下流へ伝播する。`interpKind` は `query()` に依存するため、results モードで**毎キーストローク**
> `shouldShowResults` / `skipIcons` / ResultsSection アイコン effect を再発火させ、`++iconRequestId`
> による取得 stale 化の実害が出る。→ **`"results"|"folder"|"tool"` 等の文字列プリミティブ**を返す
> メモにする。プリミティブは `===` 安定で `kind` 変化時のみ伝播する。frame が要る箇所
> （`inputValue` の tool ターゲット名等）は `toolSelectionState()` / `folderState()` を直読する。

```ts
export type ViewKind = "results" | "folder" | "tool";
export type InterpKind = "plain" | "command" | "instant";

/** 軸1: 結果リストを占める先頭ビュー（tool > folder > results の射影＝SPEC §18.5 優先度） */
const viewKind = createMemo<ViewKind>(() =>
  toolSelectionState() ? "tool" : folderState() ? "folder" : "results",
);

/** 軸2: 入力の意味。View=results のときだけ非 plain。既存シグナルの無損失な再パッケージ */
const interpKind = createMemo<InterpKind>(() => {
  if (viewKind() !== "results") return "plain";
  if (instantCommandMode()) return "instant";
  if (query().trimStart().startsWith("/")) return "command";
  return "plain";
});

function assertNever(x: never): never {
  throw new Error(`unhandled mode: ${x}`);
}
```

> 以降この計画で `view()` / `interp()` と記す箇所は、実装上すべて `viewKind()` / `interpKind()`
> プリミティブメモを指す。

`shouldShowResults`（L53）を switch へ書き換え（**綻び(1)解消・tool 枝を明示**）:

```ts
const shouldShowResults = createMemo(() => {
  if (results().length === 0) return false;
  switch (viewKind()) {
    case "tool":
    case "folder":
      return true;                            // indexing 中でも表示
    case "results":
      return instantCommandMode() || !indexing();  // instant は生シグナル直読（query 依存を持ち込まない）
    default:
      return assertNever(viewKind());
  }
});
```
> 無損失確認: 現行式 `results>0 && (!indexing || instant || folderState!==null)` と観測同値。
> tool-from-results は現行「!indexing 依存」→ 新「常に表示」。`tool×indexing` は到達不能のため
> 観測差ゼロ、暗黙の安全性を明示化（テスト `search.test.ts:629–690` は緑のまま）。
> **reactivity 注**: results 枝は `interpKind()` ではなく `instantCommandMode()` を直読する。
> instant ⟺ instantCommandMode()（results 文脈）であり、`query()` への依存を避けて plain 打鍵時の
> 不要な再計算をゼロにするため。

その他の集約:
- `refreshResults`（L121-124, 158）: `tool`/`instant` ガードを `view()`/`interp()` 参照に。
  folder/results 分岐・indexing クリアは `view()`/overlay で表現。
- `activateSelected`/`activateSelectedByIndex`（L639/661）: **共通プレフィックスのみ抽出**
  （plan-review 反映・完全統合しない）。両者は通常モードの解決が異なる（`resolveActivationTarget(preferredPath)`
  vs `flushPendingRefresh`+index clamp）ため、共有するのは tool/instant ディスパッチ部だけ。
  `tryModalActivate(index?: number): Promise<boolean> | null`（null=通常モードへフォールスルー）を
  抽出し、各関数の冒頭で呼ぶ。**tool/instant 判定は現行どおり `activationInFlight` ガードの前**に置く
  （順序を変えない）→ `/dry-check`。
- `resetForShow` の `skipRefresh`（L693）: `view().kind === "results" && interp().kind === "plain"
  && query() === ""` に。
- query effect（L210）の早期 return（suppress→tool→folder）: `view()` 参照に。instant/command の
  検出は **interp の評価器そのもの**なのでロジックは現状維持（query を live 読み）。
- export に `view` / `interp` を追加。

### 2. `ui/src/components/SearchWindow.tsx` — キーボード/入力/表示の集約

- Arrow Right/Left（L188/198）: `if (viewKind() === "tool" || interpKind() === "instant") break;`
  （**異なる軸を別条件で名指し**。現行 `tool() || instant()` の軸混在を解消）。
  - **`command` を含めないのは意図的**（plan-review 反映）。現行 `tool || instant` も command を見ておらず、
    command モードは結果が空（query effect の `/` 分岐が `clearResults`）ゆえ `r?.isFolder` が偽で展開不能。
    `interpKind()==="instant"` のみが現行と観測同値。`viewKind()!=="results"` に広げると挙動が変わる。
- Enter Shift+Enter（L215）: `view().kind !== "tool" && interp().kind !== "instant"` に。
- `handleInput`（L237-238）: `if (view().kind === "tool") return; if (launching()) return;`
  （**綻び(2)解消**: 入力可否は軸1+overlay のみ。interp は読まない＝コメント依存を型依存に）。
  folder/results の振り分けは `view()` switch。
- `inputValue`/`placeholderText`（L252/262）: `switch (view().kind)` に統一。
- import に `view` / `interp` を追加、不要になる個別シグナル import を整理。

### 3. `ui/src/MainApp.tsx` — 消費側の軸参照化（軽微）

- `skipIcons={instantCommandMode()}`（L323）→ `interp().kind === "instant"`。
- `shouldShowResults()` 消費（L247/321）は不変。

### 4. テスト

- `ui/src/stores/search.test.ts`: 既存 216 件は**緑のまま**（挙動不変）。新規 describe を追加:
  - `viewKind()`: results/folder/tool の射影（tool が folder の上で `tool` を返す＝直交性）。
  - `interpKind()`: plain/command/instant、および `viewKind()≠results` で常に `plain`。
  - `shouldShowResults` の `tool` 枝（`tool×indexing=true` でも表示）— 綻び(1)の明示化を固定。
  - **reactivity 回帰テスト**（plan-review 反映）: results モードで plain 文字を連続 `setQuery` しても
    `shouldShowResults()` のメモが**再計算で値を変えない**こと（プリミティブメモの伝播抑止を固定）。
- `ui/src/components/SearchWindow.test.tsx`: Arrow/Enter/handleInput の軸別ガードを既存パターンで
  検証（緑維持 + instant 中の入力受理を明示テスト）。
  - **【要対処・plan-review 反映】モック更新が必須**: `vi.mock("../stores/search", …)` はストア全体を
    差し替えるため、`SearchWindow` が import する `viewKind`/`interpKind` を `vi.hoisted` で
    `mockViewKind`/`mockInterpKind` として宣言し `vi.mock` ファクトリへ追加する。`beforeEach` で
    デフォルト（`mockViewKind → "results"`, `mockInterpKind → "plain"`）を設定。漏れると
    `viewKind is not a function` で CI 赤（`ui/CLAUDE.md` テスト注意点参照）。

### 5. ドキュメント同期

- `ui/CLAUDE.md`「単一ウィンドウの高さ管理」/「設計上の注意点」付近に
  **「状態モデル: 2 軸（View / Interp）+ overlays。`view()`/`interp()` が唯一の読み取りの源」**を追記。
- `SPEC.md` §18.5 / §19 / §8.6: 挙動不変のため状態図は無変更。実装が `view()`/`interp()` の
  単一の源を持ち §8.6 の状態図・§18.5 の優先順位と一対一対応する旨の参照を 1 行添える。
- `.claude/rules/ui.md`: 「モード判定は `view()`/`interp()` 経由。生シグナルを直接 if しない」を 1 行追加
  （エージェント設定の変更につき**ユーザー合意後**に実施）。

## 実装順序（フェーズ・各フェーズ test-green を保つ非破壊リファクタ）

1. **Phase 1**: `search.ts` に `view()`/`interp()`/`assertNever` を追加・export（純追加・挙動ゼロ）。
   `search.test.ts` に 2 メモの射影テストを追加 → 実行（Red→Green）。typecheck/test 緑。
2. **Phase 2**: `shouldShowResults` を switch へ。`search.test.ts:629–690` の真理値表が緑のままを確認 +
   tool 枝テスト追加。
3. **Phase 3**: store 側集約（`dispatchActivate` 統合・`refreshResults`・`resetForShow`・query effect
   早期 return）。`/dry-check dispatchActivate`。test 緑。
4. **Phase 4**: `SearchWindow.tsx` のガード（Arrow/Enter/handleInput/inputValue/placeholder）を
   軸参照へ。`SearchWindow.test.tsx` 緑 + instant 入力受理テスト。
5. **Phase 5**: `MainApp.tsx` skipIcons + ドキュメント同期（ui/CLAUDE.md・SPEC 参照・rules は合意後）。
6. **Phase 6**: 検証（下記）+ `/state-check`（直交性・リセット経路・§8.6 整合）+ `/plan-review` 反映。

## 不変条件

- **優先順位の単一化**: `tool > folder`（軸1）と `instant`（軸2・results 限定）は `view()`/`interp()`
  に**一箇所だけ**書く。11 箇所での再導出を廃す（SPEC §18.5 と一致）。
- **可視判定の無損失**: `shouldShowResults` の真理値表は現行と観測同値（tool×indexing 到達不能のため
  「tool 常に表示」への強化は観測差ゼロ）。テスト 629–690 で機械的に固定。
- **軸分離**: 入力受理は軸1（tool）+ overlay（launching）のみに依存。**Interp で打鍵を止めない**
  （`handleInput`）。これにより綻び(2)の非対称が「正しさ」として型に現れる。
- **直交性の保存**: `tool` は `folder` の上に積まれる（`view()` は先頭射影、storage が下層を保持、
  pop は既存 `exit*` が担う）。SPEC §18.5「直交」を維持。
- **網羅性**: `switch (viewKind())` / `switch (interpKind())` の default は `assertNever`。
  モード追加時に全分岐がコンパイルエラー化（綻び(3)の再発防止）。
- **reactivity 不変条件**（plan-review 反映）: 軸メモは**プリミティブ文字列**を返し、`kind` が実際に
  変わったときのみ伝播する。オブジェクト union を返すと毎計算で新 identity → 値不変でも下流発火。
  `shouldShowResults` の results 枝は `interpKind()` ではなく `instantCommandMode()` を直読し、
  `query()` 依存を持ち込まない（plain 打鍵で再計算ゼロ）。
- **軸の混入禁止**: `activationInFlight` / `searchGeneration` / `mainVisible` は軸に入れない。
- **挙動不変**: 状態機械の観測挙動は不変 → SPEC 状態図は無変更（refactor）。

## テスト方針

- **追加**: `search.test.ts` の `viewKind()`/`interpKind()`/`shouldShowResults(tool×indexing)`/
  reactivity 回帰 describe、`SearchWindow.test.tsx` の instant 入力受理（+ `mockViewKind`/`mockInterpKind`）。
- **検証コマンド**（docs/build-commands.md・.ts/.tsx = フロントエンド）:
  - `npm run typecheck` / `npm test`（既存 216 + 新規が緑）/ `npm run lint`
  - `npm run smoke:startup`
  - キーボードナビ（Arrow/Enter/Shift+Enter）を触るため **PR に `e2e` ラベル**を付与し
    CI（E2E & Smoke）で回す（カテゴリ C 相当の安全側）。
- **手動目視**: release build で 通常/フォルダ展開/ツール選択/インスタントコマンド/indexing 中の
  各モードと遷移（push/pop/Escape 復帰）を一通り確認。

## SPEC.md 更新要否

状態図・優先順位の**意図は不変**（§8.6 / §18.5 / §19 が既にこのモデルを記述）。挙動を変えないため
状態遷移の記述変更は不要。実装が `view()`/`interp()` の単一の源で図と一致する旨の参照追記のみ。

## スコープ外（任意 follow-up・別 issue 候補）

- **`instantCommandMode` ラッチの除去**: instant を `interpret(query, prefix)` から純粋導出にし
  持続シグナルを廃す。本計画の「読み取り集約」だけでも綻び(2)は解消するため**本質的には不要**。
  除去は query effect の debounce/IPC オーケストレーション + テスト 510–628 に触れる別リスク。
  純度向上のボーナスとして切り出す。

## plan-review 結果（Explore × 3：store / UI / テスト・SPEC・e2e）

### 要対処（本計画に反映済み）
1. **メモの毎キーストローク伝播**（最重要・三体が取りこぼした核心）: オブジェクト union メモは
   `===` 不一致で値不変でも伝播。`interpKind` は `query()` 依存ゆえ results 打鍵ごとに
   `shouldShowResults`/`skipIcons`/アイコン effect を再発火させ取得 stale 化の実害。
   → **プリミティブ判別子メモ**（§変更ファイル一覧 1）+ `shouldShowResults` results 枝の
   `instantCommandMode()` 直読（§不変条件 reactivity）で解消。
2. **`SearchWindow.test.tsx` モック更新必須**: `viewKind`/`interpKind` を `vi.hoisted`+`vi.mock` に
   追加せねば CI 赤。→ §変更ファイル一覧 4 に明記。
3. **`dispatchActivate` は完全統合せず共通プレフィックス抽出**（通常モード解決が両者で異なる・
   tool/instant 判定は `activationInFlight` ガード前）。→ §変更ファイル一覧 1 を `tryModalActivate` へ修正。

### 軽微（注記済み）
- Arrow ガードに `command` を含めないのは意図的（現行と観測同値・command は結果空で展開不能）。
  → §変更ファイル一覧 2 に注記。

### 問題なしと確認された項目
既存テスト保存 / tool×indexing 到達不能の証明 / SPEC 同期判断（§8.6・§18.5 と一致・参照追記のみ）/
e2e 前提維持 / Escape 優先鎖の整合 / `instantCommandMode` ラッチ除去のスコープ外切り出し /
`.claude/rules/ui.md` 追記の既存ルールとの無矛盾。

### 総評
completeness: **高** / 実装着手可否: **可（上記反映済み）**。
