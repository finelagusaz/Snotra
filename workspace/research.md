# research — issue #537: 入力解釈を純関数化し `suppressNextQueryEffectRefresh` を撤廃する

## issue の要約

`ui/src/stores/search.ts` の module スコープ可変 boolean `suppressNextQueryEffectRefresh` を撤廃する。
このフラグは「プログラム的に `query` を変えるが、それに反応する `on(query, ...)` effect を **今回だけ**黙らせる」ワンショット機構。根本原因は **ユーザー入力とプログラムリセットが同じ signal・同じ effect 経路を通る**こと。

解決策: 入力解釈を純関数 `interpret(query, prefix, viewKind)` へ抽出し、検索の起動を「`query` を監視する effect」から「ユーザー入力ハンドラからの明示 dispatch」へ寄せる。プログラム的リセットは dispatch を呼ばない別経路にする。これで「今回だけ黙らせる」必要が消える。

## 関連コード（実測）

### `ui/src/stores/search.ts`

- **`suppressNextQueryEffectRefresh`（宣言 L68）**: 立てる 2 箇所・消費 1 箇所。
  - 立てる①: `resetForShow`（L735-737）— `query() !== ""` のとき `setQuery("")` の直前。
  - 立てる②: `executeInstantCommandSelected` 成功時（L652）— `setQuery("")` の直前。
  - 消費: query effect 冒頭（L316-320）— 立っていれば `false` に戻して early return。
- **query effect（L314-359）**: `createRoot` 内の `createEffect(on(query, (q) => {...}))`。
  - suppress 消費（L316-320）→ `viewKind()` の tool/folder ガード（L321-329・early return）→ `interpKind()` 経由の dispatch（instant / instant 資源掃除 / command / plain）。
  - `on(query, fn)` は `query` のみ購読し、`fn` 本体は untracked で走る（SolidJS の `on` の設計）。ゆえに **effect は「query が変わった瞬間」だけ発火**する純粋な入力ハンドラ。`instantCommandPrefix` や `viewKind` の変化では再発火しない（実測: L322/L334 の `viewKind()`/`interpKind()` 読みは `on` 内で購読を作らない）。
- **`interpKind` memo（L106-112）**: `viewKind()!=="results"` なら plain、`isInstantPrefix` なら instant、`/` 始まりなら command、他 plain。`query`+`instantCommandPrefix`+`viewKind` からの純粋導出（持続ラッチなし・#374/#455）。プリミティブ（文字列）を返し kind 変化時のみ下流へ伝播。
- **`isInstantPrefix`（L95-97）**: instant 検出の SSOT。空 prefix では false。
- **dispatch 3 関数**:
  - `handleInstantQueryInput(q)`（L253-273）: prefix/trimStart/input/filterName を **再抽出**（L256-261）し `scheduleInstantCommandFetch` へ委譲。
  - `handleCommandQueryInput(q)`（L277-302）: `/r` 特例（即時 runRefresh）/ `findCommand` 完全一致（`clearCommandModeState` + `cmd.action()`）/ noop（results クリア）。
  - `handlePlainQueryInput(q)`（L306-310）: `setSelected(0)` + `debouncedRefresh()`。
- **instant 資源掃除（L343-346）**: plain/command へ遷移時、`hasPendingInstantCommandFetch()` or `getInstantCommandItems().length>0` のときだけ instant の pending fetch / stale 候補を掃除。
- **`folderFilter` effect（L362-369）**: `on(folderFilter, () => { if (folderState()) { setSelected(0); debouncedRefresh(); } })`。folder モード時のみ refresh。**suppress フラグは使わず `folderState()` ガードで判定**。

### `ui/src/components/SearchWindow.tsx`

- **`handleInput`（L237-253）**: `viewKind()==="tool"` と `launching()` で早期リターン → folder モードなら `setFolderFilter(value)`、それ以外は `setQuery(value)`。**`interpKind` は読まない**（instant 中も打鍵受理・軸1+overlay のみに依存）。
- `setQuery` はここが唯一のユーザー入力起点。

### `ui/src/stores/search.test.ts`

- `setQuery(...)` 呼び出しは 2 種類に大別される（下表）。dispatch を **期待する**ものは新経路へ移行が要る。**memo（`interpKind`/`viewKind`/`allowsFolderNav`）を検証するもの・状態 setup は raw `setQuery` のまま**。
- 既存回帰網に「クエリ非空でも resetForShow 後は search 不発火」（L293-302）が **既にある** — suppress 意図の回帰ガード。撤廃後も緑を保つべき対象。

## 既存パターン（再利用可能）

- **primitive 抽出の前例**: `latestRun`(#540) / `exclusive`(#541) / `OwnedTimer`(#543) — resource と policy を分離し、SolidJS/api 非依存の純粋ファクトリへ切り出す設計。今回の `interpret`（純関数）はこの系譜の「純粋化」に連なる（ただし新規ファイル化までは不要 = 関数抽出で足りる。YAGNI）。
- **`interpKind` memo が既に純粋導出**: 分類ロジックは既に純粋。`interpret` はこれ + query effect の分岐前段（filterName 抽出等）を「副作用なしの意図」として一本化するもの。
- **dispatch 3 関数は既に分離済み**: `handleInstantQueryInput`/`handleCommandQueryInput`/`handlePlainQueryInput`。今回は「effect が呼ぶ」→「明示 dispatch が呼ぶ」に呼び出し元を差し替えるのみ（関数本体はほぼ不変）。

## 技術的制約

- **SolidJS 実行モデル**: signal 変更 → 同期 effect。`on(query, fn)` の `fn` は untracked。dispatch を明示呼び出しにしても、`setQuery(value)` の直後に `interpret(value, ...)` を読めば最新値が反映される（同期）。バッチ/順序の破壊がないことをテストで担保する。
- **Win32/IPC 境界**: なし（本 issue は UI 層内の制御フロー変更のみ）。
- **`interpKind` はプリミティブを返す契約**: `interpret` を memo で使うとき、memo は依然プリミティブ（`.kind` 文字列）を返さねばならない（オブジェクト union を下流へ流すと `query()` 依存の再計算が毎打鍵で走る・ui/CLAUDE.md）。`interpret` 内部でのオブジェクト割り当ては memo 計算内に閉じ、下流へは伝播しない。

## プログラム的 query 変化の挙動（撤廃後の差分検証・実測ベース）

effect を除くと「プログラム的 `setQuery` は dispatch しない」。各経路を追跡:

| 経路 | 現状 | 撤廃後 | 安全性 |
|---|---|---|---|
| `resetForShow` `setQuery("")` | suppress で effect 無効化。末尾で `skipRefresh` 判定に基づき明示 `runRefresh()` | dispatch なし。明示 `runRefresh()` 経路は不変 | ✓ 挙動保存（回帰網 L293-302 が担保） |
| `executeInstantCommandSelected` 成功 `setQuery("")` | suppress で plain 検索を防ぐ | dispatch なし。plain 検索は起きない | ✓ 挙動保存 |
| `clearCommandModeState` `setQuery("")` | effect が **再入**し空 query で refresh（`/` コマンド実行後） | dispatch なし → 追加の空 refresh が消える | ✓ 観測不能。真因は「results は `clearResults()` で既にクリア済み + 空クエリは `refreshResults` の empty_query 分岐で **api.search を呼ばない** + プリミティブ no-op」。**window hide に依存しない**（`/o`=openSettings は hide しない・plan-review 指摘）。むしろ「リアクティビティと戦う」残滓の除去 |
| `exitFolderExpansion` `setQuery(fs.savedQuery)` | savedQuery == 現 query（folder 中 query 凍結）ゆえ **no-op**・effect 不発火 | 同じく no-op・dispatch なし | ✓ 挙動保存 |

→ suppress が守っていた 2 意図（reset 後の不要 IPC 不発火 / instant 成功後の plain 誤発火防止）は、effect 除去 + 明示経路で **構造的に**保たれる。フラグという「守るべき順序契約」が不要になる。

## 未解決の疑問（→ plan で決着）

1. **`folderFilter` effect を明示化に含めるか**（issue が「検討」と明記）。
   - 分析: `folderFilter` effect は **suppress フラグを使っていない**。判定は `folderState()` ガード = 状態由来の述語で、順序に依らず常に正しい（フラグのような fragile なワンショットではない）。本 issue の受け入れ条件（suppress 撤廃）に **無関係**。
   - 暫定推奨: **含めない**（YAGNI・スコープ/リスク抑制）。残る非対称（query=明示 dispatch / folderFilter=effect）は「folder refresh は state signal への真のリアクション」という正当な差であり、consistency のためだけの拡張は #536 retrospective の「偶発的複雑さを設計で消す／過剰一般化しない」に反する。plan.md で決定として明記し、レビューに委ねる。
2. **`interpret` の戻り値の粒度**（分類のみ / filterName も / instantQuery も）。plan で確定。
