---
paths:
  - "ui/src/**/*.{ts,tsx}"
---

# ui ルール

詳細は `ui/CLAUDE.md` を参照。

- **await 後の状態復元は staleness チェック必須**: `latestRun` primitive の world 世代カウンタで「await 中に状態が変わっていないか」を検証してから復元する。lane タスクは `searchLane.run()` の ctx から受け取る `isStale`/`requestId` で（`await` 後 `if (isStale()) return;`）、lane 外の保存状態復元（例: `executeInstantCommandSelected`）は `await` 前に `searchLane.current()` をキャプチャし `current() === captured + 1` の基準値比較で検証する。非 lane コード（モード遷移・起動）は `searchLane.invalidate()` で world を進めて in-flight を supersede する
- **モード遷移時にデバウンスをキャンセル**: フォルダ離脱・ツール選択離脱・インスタントコマンド実行時に、そのモードが所有する OwnedTimer（`refreshTimer`/`fetchTimer`、`lib/ownedTimer.ts`）を `cancel()` で破棄する（`cancelDebounce()`/`cancelInstantCommandDebounce()` 経由。旧: 生 `setTimeout` の手動 clear）
- **Blob URL 早期リターン時は全 URL を revoke**: `parseBinaryBatch` → stale guard で抜ける場合、`parsed` 内の URL を個別に `revokeObjectURL` しないとリーク
- **`set_size()` は `shouldShowResults` の effect だけが呼ぶ**: 他の箇所からウィンドウサイズを変更しない
- **選択はインデックス（number）で参照**: パス文字列を使わない（ツール選択モードでパスが重複する）
- **Effect 内で自身が依存するシグナルを set しない**: 無限ループの原因。やむを得ない場合は `untrack()` で切る
- **モード判定は `viewKind()`/`interpKind()` 経由**: `toolSelectionState()`/`folderState()` を直接 if して優先度を再導出しない（frame 値が要る箇所は storage 直読可）。軸メモはプリミティブを返す（オブジェクト union は毎計算で新 identity となり下流を再発火させる）。`interpKind` は `query`+`prefix` の純粋導出（持続ラッチを持たない）
- **レイアウト崩れの観点で検証する**: スタイル・レイアウト・テキスト表示に影響する変更では、overflow／clipping／フォントレンダリング／コンテンツサイズが極端に大きい or 小さいケースを検証対象に含める。PR 作成前にビルドして目視確認する
