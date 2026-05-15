---
paths:
  - "src-tauri/**/*.rs"
---

# src-tauri ルール

詳細は `src-tauri/CLAUDE.md` を参照。

- **engine ロックを async/blocking 境界またぎで保持しない**: データ抽出 → 即解放 → 処理の順
- **index-build フラグは `AppState::try_begin_index_build` / `finish_index_build` 経由**: `indexing` / `index_build_started` を直接 `store()` しない（外部からの force-reset は走行中ビルドのガードを踏み倒す）
- **子プロセス spawn → exit handler kill はペア**: `main.rs` の exit ハンドラに `.kill()` を忘れない
- **`WebviewWindowBuilder::build()` は setup フェーズのみ**: イベントループ中・IPC ハンドラからはデッドロック。ランタイムは show/hide のみ
- **Win32 API は PlatformBridge 経由**: IPC ハンドラから直接呼ばない。platform スレッドのメッセージループで実行する
- **`windows` クレート（v0.62）の API を使う前に**: 型・シグネチャの一致と feature フラグの宣言を確認する
- **ShellExecuteW + シェル拡張 → COM STA 必須**: `std::thread::spawn` + `CoInitializeEx` パターン。EXE は不要
- **イベント順序**: `language-changed` → `hotkey-registration-failed` の順（フロントエンドが正しい言語でメッセージを組み立てるため）
