# plan — issue #379 instantCommandMode 持続ラッチを純粋導出へ

## 設計

`interpKind` を「`instantCommandMode()` latch の無損失な再パッケージ」から「viewKind + query + prefix の真の純粋関数」へ昇格させ、持続シグナルを廃する。挙動不変（全経路でラッチ値 ＝ `query.startsWith(prefix)` を調査で確認済み）。#374 二軸モデルの完成。

## 変更ファイル一覧

1. **`ui/src/stores/search.ts`**（中核リファクタ）
   - `interpKind`（67-72）を query+prefix 純粋導出へ:
     ```ts
     const interpKind = createMemo<InterpKind>(() => {
       if (viewKind() !== "results") return "plain";
       const q = query().trimStart();
       const prefix = instantCommandPrefix();
       if (prefix && q.startsWith(prefix)) return "instant";
       if (q.startsWith("/")) return "command";
       return "plain";
     });
     ```
   - `shouldShowResults`（85）: `instantCommandMode()` → `interpKind() === "instant"`。84 コメントを「interpKind 経由でも plain 打鍵では値不変ゆえ非伝播」へ更新
   - signal 削除（20）、export 削除（825）
   - setter 削除: 276（true）、664・744（false）。**周辺のクリーンアップ（`instantCommandItems=[]`・timer clear・`setQuery("")`）は残す**
   - 309-316: `setInstantCommandMode(false)` 削除。ガードを `if (instantCmdDebounceTimer !== undefined || instantCommandItems.length > 0)` へ（pending/stale instant 状態の掃除を非リアクティブに継続）。**正当性**: ラッチ撤去後は固着すべき latch が存在せず、`interpKind` は常に現在 query から導出（prefix を消せば即 plain）。このガードは「掃除すべき資源（timer/items）が現に存在するときだけ掃除する」意味で、両者とも空なら掃除は no-op ゆえスキップで正しい。誤起動は interpKind="plain"（→ tryModalActivate が executeInstantCommandSelected を呼ばない）+ `instantCommandItems` 空の二重防御。**なぜこの条件か**を `:84` 同様コメントで明記する

2. **`ui/src/stores/search.test.ts`**: `instantCommandMode()` の import/assert を `interpKind()`（`=== "instant"` / `!== "instant"` / `=== "plain"`）へ移行

3. **`ui/src/components/SearchWindow.test.tsx`**（plan-review で要対処を確認）:
   - `mockInstantCommandMode` を撤去: hoisted 宣言（L10）・定義（L33）・mock エントリ（L84）・`mockInterpKind` 内の導出参照（L52）
   - `mockInterpKind`（L50-55）を query+prefix 導出へ再定義（既定 prefix `"@"` をハードコード。component テストは prefix を変えないため `mockInstantCommandPrefix` は追加しない＝YAGNI）:
     ```ts
     if (mockToolSelectionState() || mockFolderState()) return "plain";
     const q = mockQuery().trimStart();
     if (q.startsWith("@")) return "instant";
     if (q.startsWith("/")) return "command";
     return "plain";
     ```
   - instant を発火させるテスト本体（L177/276/311 の `mockInstantCommandMode.mockReturnValue(true)`）と beforeEach（L144 の `false` リセット）を `mockQuery.mockReturnValue("@x")` へ移行（beforeEach の L134 `mockQuery=""` がリセットを担うため既存パターンと整合）。ハンドラが query 内容に敏感な場合のみ `mockInterpKind.mockReturnValue("instant")` 直接設定にフォールバック（実装時に各テストを読んで決定）

4. **`ui/CLAUDE.md`**: L45・L103・L122・L127 の `instantCommandMode` 記述を `interpKind`（純粋導出）へ同期

5. **`.claude/rules/ui.md`**: モード判定ルールの `instantCommandMode()` 直 if 言及を削除/調整

6. **`docs/architecture.md`**（plan-review で漏れを検出）: L96（`shouldShowResults` 式 `... || instantCommandMode()`）→ `... || interpKind() === "instant"`、L143（「`instantCommandMode` シグナル + query effect でモード切替」）→ interpKind 純粋導出の記述へ同期

## 実装順序（段階的 TDD — 各フェーズ緑を維持）

- **Phase 0（テスト assertion 移行・latch 残置）**: `search.test.ts` / `SearchWindow.test.tsx` の `instantCommandMode()` を `interpKind()` へ書換え。latch がまだ生きているため緑のまま（= 現状で interpKind===latch を実証）。export 依存を先に切る。
- **Phase 1（interpKind 純粋導出・latch 残置）**: `interpKind` を query+prefix 導出へ。`shouldShowResults` を `interpKind()` 読みへ。latch setter はまだ残す（冗長だが無害）。typecheck + test 緑を確認。
- **Phase 2（latch 撤去）**: signal（20）・export（825）・setter（276/664/744）削除、309 ガードを非リアクティブ条件へ。typecheck（残存参照のコンパイルエラー検出）+ test 緑。
- **Phase 3（ドキュメント同期）**: `ui/CLAUDE.md` / `.claude/rules/ui.md` を更新。

## 不変条件

- **挙動不変**: instant 検出・instant 結果取得（getInstantCommands debounce）・実行（executeInstantCommand）・失敗復元・reset・skipIcons・キーボードガードがすべて同一。
- **reactivity 保全**: `shouldShowResults` は plain 打鍵で再実行しない（`interpKind` プリミティブメモの値ゲート伝播）。`interpKind` は string を返す（オブジェクト union 禁止＝#374 の不変条件）。
- **クリーンアップの非欠落**: latch setter 削除時、`instantCmdDebounceTimer` clear・`instantCommandItems=[]`・`setQuery("")` を**消さない**。これらが消えると stale instant コマンドの誤起動 / IPC リークが起こる。
- **空 prefix ガード**: `interpKind` に `prefix &&` を入れる（query effect `:269` と同型）。欠くと空 prefix 時に全入力が instant 化。
- **異常系**: 純粋導出は状態を持たないため「失敗で固着する持続フラグ」自体が消滅（false に戻す経路の欠落リスクが構造的に無くなる）。instant 実行失敗時は query 不変 → interpKind は instant のまま（候補復元と整合）。

## テスト方針

- 既存テストを**回帰ハーネス**として使用（assertion 移行のみ、シナリオは不変）:
  - `search.test.ts`: instantCommandMode describe（512）・executeInstantCommandSelected（558）・refreshResults ガード（614）・shouldShowResults（630, 特に 664 indexing+instant）・interpKind（772）
  - `SearchWindow.test.tsx`: キーボードガード（interpKind==="instant" で ArrowRight/Left/Shift+Enter バイパス）
- **追加テスト（純粋導出の明示）**: 「latch なしで setQuery('@x') → 即 `interpKind()==="instant"`（runAllTimersAsync 不要＝同期導出）」を 1 件追加し、持続シグナルに依存しない導出を固定する。
- 検証コマンドは SSOT `docs/build-commands.md` のカテゴリ（ui 変更 = typecheck + vitest）に従う。コマンド文字列は本 plan に直書きしない。

## SPEC.md 更新要否

**不要**。§8.6 状態図（L432 `InstantCommandMode`）・§19 はユーザー体験上のモードと挙動を記述し、モード概念・遷移・表示規則は不変。変わるのは内部表現（持続ラッチ→純粋導出）のみで SPEC の管轄外。

## セルフレビュー

### /plan-review 結果（Explore ×3）
- **reactivity・挙動同一性（Agent 1）**: 主張1（latch 値＝`query.startsWith(prefix)`）・主張2（`shouldShowResults` の値ゲート非再実行）ともに**成立**を確認。空 prefix ガード `prefix &&` 必須（反映済み）。
- **対称ペア・掃除・async（Agent 2）**: 「要対処（309 ガード破綻）」を提起 → **検証の結果、誤警報**。Agent 2 は旧不変条件（`instantCommandMode` 固着）で新世界を裁いていた。ラッチ撤去後は固着 latch が無く、Agent 2 の stale シナリオでは timer/items とも空＝掃除 no-op、interpKind="plain" で誤起動も防がれる。ガード正当性を plan に明記済み。set/clear 掃除の対称性・async 読み安全性（await をまたぐ interpKind 読み無し）は問題なし。
- **テスト・doc・スコープ（Agent 3）**: 要対処 2 件を**実ファイルで確認・反映** — (a) `docs/architecture.md` L96/L143 を doc-sync に追加、(b) `SearchWindow.test.tsx` の `mockInterpKind` が `mockInstantCommandMode` 導出（L52）+ テスト本体 L177/276/311 が直接 set → 移行手順を精密化。SPEC 非更新判断は妥当（§8.6 L432・§19 は内部表現非依存）、E2E 影響なし、YAGNI 遵守。

### 5b チェックリスト
1. **対称コードパス**: instant の enter（276）/exit（314/664/744）と各掃除の対称性を Agent 2 が検証。setter 削除時の掃除非欠落を不変条件化。
2. **影響範囲の網羅性**: `instantCommandMode`/`interpKind` の全消費者を grep（production: MainApp/SearchWindow=interpKind のみ、latch export はテスト専用）。doc は ui/CLAUDE.md・rules/ui.md・**architecture.md**（追加）。
3. **境界条件**: 空 prefix（`prefix &&` ガード）、IPC stale 時の掃除 no-op、失敗復元時 query 不変→interpKind=instant 維持、viewKind=tool/folder 時の interpKind=plain ゲート。
4. **リソース管理**: 純粋導出は状態レス＝「false に戻す経路欠落で固着」リスクが構造的に消滅。timer（clearTimeout）/items（=[]）の掃除は残置を不変条件化。
5. **既存パターン整合**: #374 の二軸プリミティブメモを完成させるのみ。新パターン皆無。テストは派生モック導出（ui/CLAUDE.md パターン）に追従。
6. **YAGNI**: scope は issue 要求（シグナル廃止+純粋導出+テスト/doc 同期）に限定。`mockInstantCommandPrefix` は追加しない（prefix 不変のテストに不要）。
7. **シンプル化**: 持続シグナル 1 個 + setter 4 箇所を消し、interpKind を真の純粋関数化。むしろ複雑さを減らす方向。
8. **破壊不変条件**: Win32 フック/ホットキー/IPC など「戻ってこない系」は無関係（フロントエンドのリアクティブ状態のみ）。検知は段階的 TDD（各 Phase で typecheck + vitest 緑）+ 既存 226+ テストの回帰ハーネス。

### check スキルの判定（透明性）
- `/state-check`・`/symmetric-check`: 固有観点（モード直交性・reset 経路・入力分岐・SPEC §8.6 整合 / enter-exit 対称と掃除）を plan-review の Agent 1・2 に明示割当して検証済み。重複起動は避けた。
- `/race-check`・`/cache-check`: 非該当（async 関数の新規追加・シグネチャ変更なし / インクリメンタル再利用述語でない）。await をまたぐ interpKind 読みは Agent 2 が安全と確認。

### 総評
- completeness: **高**（要対処 2 件を実ファイル確認の上で反映、誤警報 1 件を検証して棄却）
- 着手可否: **可**（段階的 TDD で Phase 0→3、各 Phase 緑を実証しながら進める）
