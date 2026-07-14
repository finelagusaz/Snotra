---
name: race-check
description: "async 関数を新規追加・変更したとき、または計画レビュー時に使用。各 await 地点の状態競合リスクを検証する。"
argument-hint: "[関数名: await 対象, 例: 'executeInstantCommandSelected: await api.executeInstantCommand()']"
allowed-tools:
  - Read
  - Grep
  - Glob
---

$ARGUMENTS の async 関数について、各 `await` 地点での状態競合リスクを検証する。
$ARGUMENTS が空の場合は、会話の直近の変更内容から対象を推定する。

実装後のコードレビューだけでなく、`workspace/plan.md` の計画レビューにも使える。計画段階で「この `await` は安全か？」を検証し、見落としがあれば計画を更新してから実装に進む。

## 背景

このプロダクトのレースコンディションは「`await` の前後で世界が変わりうる」という1点に帰着する。典型的な発生パターン:

1. **状態保存→ `await` → 無条件復元**: `await` 中にユーザー操作で状態が変わると、復元が新しい状態を上書きする
2. **入力ガード不足**: `await` 中にユーザーがクエリを変更し、`dispatchQueryInput` が新しい検索を発火する
3. **世代カウンタ検証漏れ**: `latestRun` の `isStale()`（world 世代カウンタを内包。旧: `searchGeneration` 直読）による staleness チェックを新コードに適用し忘れる
4. **再入**: 同じ async 関数が `await` 中に再度呼び出される

## Step 1 — await 地点の列挙

$ARGUMENTS から対象の async 関数を特定し、ソースコードを読む。

関数内の各 `await` 地点を列挙する:

```
await 地点 1: <line> — <await 対象の説明>
await 地点 2: <line> — <await 対象の説明>
```

## Step 2 — 共有状態の特定

各 `await` の前後で読み書きする共有状態を特定する。対象:

| 種類 | 例 |
|------|-----|
| SolidJS シグナル | `results()`, `selected()`, `query()`, `launching()` |
| モジュールスコープ変数 | `searchLane`（`latestRun`・world 世代を内包）, `activationLane`（`exclusive`・in-flight mutex を内包）, `instantCommandItems` |
| 外部ストアのシグナル | `folderState()`, `toolSelectionState()`, `interpKind()` |

```
await 地点 1:
  読み取り（前）: <変数リスト>
  書き込み（前）: <変数リスト>
  読み取り（後）: <変数リスト>
  書き込み（後）: <変数リスト>
```

## Step 3 — 状態変更経路の検索

各 `await` 地点について、`await` 中に共有状態を変更しうる経路を grep で検索する。主な経路:

| 経路 | トリガー | 影響する状態 |
|------|---------|------------|
| `handleInput` → `dispatchQueryInput` | ユーザーのキー入力 | `query`, `results`, `selected`, `searchLane` 世代（`run`/`invalidate`）, `instantCommandItems` |
| `handleKeyDown` → `activateSelected` | Enter キー | `activationLane`（in-flight）, `results`, `selected` |
| `handleKeyDown` → `exitFolderExpansion` | Escape キー | `folderState`, `results`, `selected`, `searchLane` 世代（`invalidate`） |
| `resetForShow` | `window-shown` イベント | 全状態リセット |
| `indexing-complete` イベント → `runRefresh` | バックエンド通知 | `indexing`, `results`, `searchLane` 世代（`run`） |

各経路について、`await` 中に実際に到達可能かを判定する（ガードの有無を確認）。

## Step 4 — 4観点での検証

各 `await` 地点を以下の4観点で検証する:

### 4a. 入力ガード

`await` 中にユーザー入力で状態が変わる経路にガードがあるか？

- `handleInput` に `launching()` / `toolSelectionState()` 等のガードがあるか
- `handleKeyDown` の各分岐に該当するガードがあるか
- ガードがない場合: **入力経路が開いている** → リスクあり

### 4b. staleness チェック

`await` 後に状態を参照・復元する箇所で `latestRun` の `isStale()`（等）による世代チェックがあるか？

- lane タスクが `searchLane.run()` の ctx（`isStale`/`requestId`）を受け取り、`await` 後に `if (isStale()) return;` で適用スキップしているか
- 保存状態を復元する箇所で「他変化なし」を検証しているか。起動フローなら `withLaunchLifecycle` が配る `disturbed()` 述語（`if (!disturbed())`）で、それ以外は `await` 前に `searchLane.current()` をキャプチャした世代比較で検証する（例: `executeInstantCommandSelected` の失敗ロールバックは `disturbed()` を使う・#539）
- 検証なしで `setResults` / `setSelected` を呼んでいないか

### 4c. ローカルキャプチャ

`await` をまたいで参照するモジュールスコープの `let` 変数を `const` にキャプチャしているか？

- `await` 後に直接参照している `let` 変数がないか
- シグナルの `.call()` は都度最新値なので問題なし（ただし「保存した値を復元」する場合は 4b の対象）

### 4d. 再入ガード

同じ関数が `await` 中に再度呼び出される経路があるか？

- `activationLane`（`exclusive` mutex）等でガードされているか
- ガードの解除（`finally` ブロック）が確実か

## Step 5 — 各 await 地点の判定

```
await 地点 1: <line> — <説明>
  4a 入力ガード:     [OK] launching() で handleInput をブロック
                     [問題] handleInput が launching() を確認していない
  4b staleness:      [OK] isStale() でスキップ / !disturbed() で検証済み
                     [問題] 無条件で setResults() を呼んでいる
  4c ローカルキャプチャ: [OK] 全 let 変数をキャプチャ済み
                     [問題] instantCommandItems を await 後に直接参照
  4d 再入ガード:     [OK] activationLane（exclusive）でガード済み
                     [問題] ガードなし
  総合判定: [安全] / [要修正: <具体的な修正内容>]
```

## 出力

**根拠の規律**: 全判定に根拠（`file:line` または grep 結果）を付ける。コードを確認せずに下した判定は [OK] とせず [要確認] として報告する。

全 `await` 地点の判定をマトリクス形式でまとめる。
問題が見つかった場合は修正案を提示する。
全地点が安全な場合は「全 await 地点で状態競合リスクなし」と明示する。
