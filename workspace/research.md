# research — issue #379 instantCommandMode 持続ラッチを純粋導出へ

## issue の要約

`instantCommandMode` の持続シグナル（ラッチ）を廃し、instant を `query` + `prefix` からの**純粋導出**にする。#374 の二軸モデル（viewKind/interpKind）を真に完成させる作業。挙動不変リファクタ。

## 調査の核心 — ラッチは「半分しか派生していない interpKind」の残滓

`interpKind` の docstring 自身が「`instantCommandMode()` latch の無損失な再パッケージ」と認めている。`viewKind` は真の派生（`toolSelectionState`/`folderState` の関数）だが、`interpKind` は隠れた持続ラッチを読む（`search.ts:69`）。#379 はこのラッチを消し、`interpKind` を「viewKind + query + prefix」の純粋関数にする。

### ラッチが query と常に同期している証明（挙動不変の根拠）

- **set true** (`search.ts:276`): query effect で `query.trimStart().startsWith(prefix)` のとき。同期実行。
- **set false** (`:314`): query effect で prefix が外れたとき。同期実行。
- **set false** (`:664` instant 実行成功 / `:744` resetForShow): いずれも直後に `setQuery("")` と**対**。純粋導出なら query="" → 自動的に "plain"。同結果。
- query effect は createEffect(on(query)) で**同期発火**するため、results ビューでは「ラッチ値 ＝ `query.startsWith(prefix)`」が常に成立。
- `interpKind() === "instant"` を読む全箇所（`:160` refreshResults ガード / `:685` tryModalActivate / `:735` resetForShow skipRefresh）は**同期実行 or fresh な query 読み**。await をまたいで古いラッチに依存する箇所は皆無。
- 失敗復元パス（`:650-656`）は query を変えない → 純粋導出でも `interpKind==="instant"` 維持。テスト `search.test.ts:593` の期待と一致。

→ **純粋導出は全テストシナリオで挙動同一**。

### #374 の reactivity 最適化は保てる

`shouldShowResults`（`:85`）は現状 `instantCommandMode()` を**生直読**して `query` 依存を避けている（`:84` コメント）。これを `interpKind() === "instant"` に置換しても、**`interpKind` はプリミティブメモ**ゆえ通知は値ゲート：plain 打鍵（"ab"→"abc"、interpKind="plain" 不変）では下流へ伝播せず、`shouldShowResults` は再実行されない。command↔plain 遷移時に memo 本体が走るが boolean 出力不変＝下流無伝播。負荷は無視可能。`interpKind` 自体は現状も毎打鍵 recompute（`:70` で query 読み）しており新規コストなし。

## 関連コード（影響範囲）

| 箇所 | 現状 | 変更 |
|---|---|---|
| `search.ts:20` | `instantCommandMode`/`setInstantCommandMode` signal | **削除** |
| `search.ts:67-72` | `interpKind` が latch を読む | **query+prefix 純粋導出**へ（空 prefix ガード `prefix &&` を `:269` と同型に） |
| `search.ts:85` | `shouldShowResults` が latch 生直読 | `interpKind() === "instant"` へ。`:84` コメント更新 |
| `search.ts:276` | `setInstantCommandMode(true)` | **削除**（IPC debounce 等の他処理は残す） |
| `search.ts:309-316` | latch 解除 + timer/items クリーンアップ | setter 削除。クリーンアップは残し、ガード `if(instantCommandMode())` を非リアクティブ条件 `if(instantCmdDebounceTimer!==undefined \|\| instantCommandItems.length>0)` へ |
| `search.ts:664,744` | `setInstantCommandMode(false)` | **削除**（直後 `setQuery("")` が派生を plain にする）。`instantCommandItems=[]` 等は残す |
| `search.ts:825` | `instantCommandMode` export | **削除** |
| `search.test.ts` | `instantCommandMode()` を import/assert（58,519,538,542,549,552,577,593,619,680,788） | `interpKind()`/`interpKind()==="instant"` へ移行 |
| `SearchWindow.test.tsx:84` | `instantCommandMode: mockInstantCommandMode` モック | 削除（production は不参照）。`mockInterpKind` は既存パターンで直接/導出 |

### instantCommandMode の外部消費者

- **production: ゼロ**（`MainApp.tsx:323` `skipIcons`・`SearchWindow.tsx:190/200/217` ガードはすべて `interpKind()` 経由）。export 削除の影響はテストのみ。

## 既存パターン（再利用）

- **二軸プリミティブメモ**（`viewKind`/`interpKind`）= #374 で確立。本変更は `interpKind` を「latch 包み」から「真の派生」へ昇格させるだけで、新パターン導入なし。
- **段階的 TDD + 派生テストモック**（#374 retrospective）: 既存テストを assertion 移行で緑維持。
- **クリーンアップ非リアクティブ条件**: `instantCmdDebounceTimer`/`instantCommandItems` は元々非リアクティブ（plain `let`）。これらでガードするのは既存の状態管理と整合。

## 技術的制約

- **SolidJS メモ等価**: 既定 `===`。`interpKind` はプリミティブ（string）を返すため値ゲート伝播（#374 で確立、`ui/CLAUDE.md` 実装パターンに明記）。
- **Win32 / IPC**: 無関係（フロントエンドのリアクティブ状態のみ）。IPC（getInstantCommands/executeInstantCommand）の呼び出しタイミングは不変。
- **空 prefix**: `interpKind` でも `prefix &&` ガード必須（空 prefix で全入力が instant 化するのを防ぐ。SPEC §19 L705）。

## ドキュメント同期

- **SPEC.md: 更新不要**。§8.6 状態図(L432 `InstantCommandMode`)・§19 はユーザー体験上のモード概念と挙動を記述。モード・遷移・表示規則はすべて不変。変わるのは内部表現（ラッチ→派生）のみで SPEC の管轄外。
- **`ui/CLAUDE.md`: 更新必要**（L45「`instantCommandMode` シグナルで状態管理」/ L103 shouldShowResults / L122「latch の無損失な再パッケージ」/ L127 モード判定ルール）。
- **`.claude/rules/ui.md`: 更新必要**（モード判定ルールの `instantCommandMode()` 直 if 言及）。

## 未解決の疑問

- なし（挙動同一性・reactivity 保全・消費者範囲・SPEC 非対象をすべて確認済み）。
