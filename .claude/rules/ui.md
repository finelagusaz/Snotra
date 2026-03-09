---
paths:
  - "ui/src/**/*.{ts,tsx}"
---

# ui ルール

詳細は `ui/CLAUDE.md` を参照。

- **await 後の状態復元は staleness チェック必須**: `searchGeneration` カウンタで「await 中に状態が変わっていないか」を検証してから復元する
- **モード遷移時にデバウンスをキャンセル**: フォルダ離脱・ツール選択離脱・インスタントコマンド実行時に、保留中の RAF / setTimeout を破棄する
- **Blob URL 早期リターン時は全 URL を revoke**: `parseBinaryBatch` → stale guard で抜ける場合、`parsed` 内の URL を個別に `revokeObjectURL` しないとリーク
- **`set_size()` は `shouldShowResults` の effect だけが呼ぶ**: 他の箇所からウィンドウサイズを変更しない
- **選択はインデックス（number）で参照**: パス文字列を使わない（ツール選択モードでパスが重複する）
- **Effect 内で自身が依存するシグナルを set しない**: 無限ループの原因。やむを得ない場合は `untrack()` で切る
