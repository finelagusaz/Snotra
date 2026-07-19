---
paths:
  - "ui/src/**/*.{ts,tsx}"
---

# ui ルール（ルーター）

事実の正準は `ui/CLAUDE.md` とコード。要約コピーは置かず正準へ指す。位置はファイル名で断定せず**見出し名・シンボル名で grep**（#588）。

## 読む正準（`ui/CLAUDE.md` の該当節）

- `await` 後に保存状態を復元するなら staleness チェック（lane タスクは `isStale()`・起動フローは `disturbed()`・非 lane は `searchLane.invalidate()` で in-flight を supersede）: 「実装パターン」+ `lib/latestRun.ts` / `lib/exclusive.ts`
- モード遷移でデバウンスをキャンセル（`OwnedTimer` の `cancel()` を `cancelDebounce()` / `cancelInstantCommandDebounce()` 経由で）: `lib/ownedTimer.ts` +「実装パターン」
- Blob URL 早期リターン時は全 URL を revoke（`parseBinaryBatch` の stale guard 等）: 「Blob URL 管理の不変条件」
- ウィンドウサイズ変更は `shouldShowResults` の effect のみ（`untrack()` で幅を依存から外し `set_size` ループバック回避）: 「単一ウィンドウの高さ管理」+「実装パターン」
- 選択はリスト行インデックス（`number`）で参照（パス文字列を使わない・ツール選択でパス非一意）: 「マウスイベントハンドラ」
- Effect 内で自身が依存するシグナルを set しない（無限ループ・やむを得なければ `untrack()`）: 「実装パターン」
- モード判定は `viewKind()` / `interpKind()` 経由（軸メモはプリミティブを返す・`interpKind` は純粋導出）: 「実装パターン」+「状態モデル（2 軸 + オーバーレイ）」

## 引き金 → 検査

- スタイル・レイアウト・テキスト表示に影響する変更: overflow / clipping / フォントレンダリング / コンテンツサイズの極大・極小を検証対象に含め、PR 作成前にビルドして目視確認する（レンダリング欠陥は自動テストで捕捉しにくい）
