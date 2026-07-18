---
paths:
  - "src-tauri/**/*.rs"
---

# src-tauri ルール

詳細は `src-tauri/CLAUDE.md` を参照。

- **engine ロックを async/blocking 境界またぎで保持しない**: データ抽出 → 即解放 → 処理の順
- **index-build フラグは `AppState::try_begin_index_build` / `finish_index_build` 経由**: `indexing` / `index_build_started` を直接 `store()` しない（外部からの force-reset は走行中ビルドのガードを踏み倒す）
- **子プロセス spawn → exit handler kill はペア**: `main.rs` の exit ハンドラに `.kill()` を忘れない
- **ウィンドウの生成は setup フェーズに限り、ランタイムは取得と表示制御だけ**: ランタイム（イベントループ中・IPC ハンドラ）では既存ウィンドウを取得して show/hide するに留める。ウィンドウ生成（`WebviewWindowBuilder::build()`）は WebView2 初期化がメッセージポンプの進行を要求するため、そこで呼ぶとデッドロックする。現状ランタイムに生成経路は無く、これは再び生成を持ち込む時のための予防規範
- **Win32 API は PlatformBridge 経由**: IPC ハンドラから直接呼ばない。platform スレッドのメッセージループで実行する
- **`windows` クレート（v0.62）の API を使う前に**: 型・シグネチャの一致と feature フラグの宣言を確認する
- **ShellExecuteW + シェル拡張 → COM STA 必須**: `std::thread::spawn` + `CoInitializeEx` パターン。EXE は不要
- **イベント順序**: `language-changed` → `hotkey-registration-failed` の順（フロントエンドが正しい言語でメッセージを組み立てるため）
- **状態フラグを `true` にしたら `false` に戻す経路とセットで設計する**: 戻す責務を持つ関数は `#[must_use]` を付けて `let _ =` による無視をコンパイル時に検出する（実例 `window.rs::launch_settings_process`。index-build フラグは上記 `try_begin_index_build`/`finish_index_build` 経由）
- **Win32 依存モジュール（`ime.rs`・`platform/` 内の `hotkey.rs` 等）はユニットテスト前提にしない**
- **ホットキー・ウィンドウ生成/表示順・スラッシュコマンド経路を変更したら、カテゴリ A に加え `docs/build-commands.md` カテゴリ C（`smoke:startup` / `e2e:tauri`）も該当する**: post-edit hook が撃つのはカテゴリ A（clippy/test）だけで、「沈黙 = 合格」は C を含まない。C は手元で回すか、PR で `e2e.yml`（対象 paths で自動起動）に委ねるかを明示的に選ぶ。検証カテゴリは拡張子（`.rs`→A）でなく、変更が触れるコードパスの意味で決める（#558 でこの写像を早合点し C を見落とした）
