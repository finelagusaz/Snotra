# Retrospective — PlatformBridge 並列初期化サイクル (#72)

対象フェーズ: 起動フロー調査（research.md）→ 実装計画（plan.md）→ 実装 → レビュー指摘対応 → SPEC.md / CLAUDE.md 更新

---

## 1. 実施した内容

### 調査・計画

- `code-optimizer-reviewer` エージェントで起動〜トレイ表示フローを全ファイル走査し `workspace/research.md` に出力
- 最適化候補を優先度別に整理し、lazy 化（見送り）と並列化（採用）を判断
- `workspace/plan.md` に実装計画を策定

### 実装（`feat/72-parallel-platform-init`）

- `PlatformBridge::start()` を `begin()`（非ブロッキング spawn）＋ `PlatformBridgePending::wait()`（recv ブロック）に分割
- `setup()` 内の順序を「spawn → ウィンドウ生成 × 3 → wait」に変更し、Win32 初期化とウィンドウ生成を並列化
- トレイ・ホットキーの有効化をセットアップ完了後にコマンド経由で行う形に変更

### SPEC.md / CLAUDE.md 更新

- §7.5: 並列 spawn・トレイ後出し・ホットキー後出しの3つの不変条件を追記
- §9: トレイアイコンはウィンドウ生成完了後に表示する旨を追記
- CLAUDE.md: 「有効化 ≥ リスナー登録」パターンを横断的実装パターンに追記

---

## 2. 発見したバグとパターン

### バグ A: トレイがウィンドウ生成前に表示される（plan 段階で検出）

**根本原因**: `platform_thread_loop` は `thread_id_tx.send()` 後すぐに `TrayIcon::create()` を実行する。`begin()` を window 生成前に呼ぶと、platform thread が T≈15ms でトレイを表示してしまい、about/settings ウィンドウの生成（T≈300ms）が終わる前にトレイが操作可能になる。

**壊れた不変条件**: トレイ右クリックメニューから呼び出せるウィンドウは、トレイ表示前に生成完了していなければならない。

**修正経路**: SPEC §7.5 の「タスクトレイ描写前に終わらせておくのが安全」要件を起点に plan の図と実際の非同期挙動のずれを発見。`SetTrayVisible(true)` をリスナー登録後に main から送る形に修正。

### バグ B: ホットキーが hotkey-pressed リスナー登録より前に有効化される（レビューで検出）

**根本原因**: `platform_thread_loop` は `thread_id_tx.send()` 後すぐに `hotkey::register()` を実行する（T≈2ms）。しかし `hotkey-pressed` リスナーは manage・ウィンドウ生成完了後に登録される（T≈305ms）。この約 303ms の空白期間に押されたホットキーは `app_handle.emit("hotkey-pressed")` が発行されても受け手がなく無音で破棄される。

**壊れた不変条件**: `hotkey::register()` は常に `hotkey-pressed` リスナー登録後に呼ばれなければならない。

**修正経路**: `platform_thread_loop` の init から `hotkey::register()` を削除し、`PlatformCommand::RegisterInitialHotkey` を追加。main が `hotkey-pressed` リスナーを登録した直後にこのコマンドを送信。`process_commands` に `app_handle: &AppHandle` を追加して失敗通知を保持。

---

## 3. 構造的ミスのパターン（今後への教訓）

### 「意図した順序図」と「実際の非同期挙動」のずれ

plan.md のシーケンス図は「manage → hotkey::register → TrayIcon::create」という意図した順序を示していたが、`mpsc::channel` は送信側をブロックしないため、platform thread は main が manage を呼ぶより遥かに先に hotkey/tray の処理を終えてしまう。

**教訓**: 並列・非同期の処理順序を図で示す際は「その順序がコードで強制されているか（チャネル/同期プリミティブがあるか）」を必ず確認する。「意図した順序」と「実際の実行順序」は別物。

### plan に含まれるリスク分析の不足

初版 plan.md のリスク分析は「既存の競合ウィンドウと同等」と誤判定していた。並列化の前後で「トレイ表示タイミング（T≈15ms vs T≈315ms）」が大きく変わることを定量的に追いきれなかった。

**教訓**: 並列化の計画においては「各処理が完了するタイミング（相対 ms）」を明示し、依存関係のある処理が「新タイミングでも正しい順序になるか」を確認する。

---

## 4. 残存リスク（前サイクルから引き継ぎ）

| 問題 | 場所 | 判断 |
|------|------|------|
| ホットキー登録失敗時の検索UI表示フォールバック未実装 | platform.rs → App.tsx | `initial-hotkey-failed` イベント発行済みだがフロント側ハンドラなし。要実装 |
| UNC ルート境界 `\\server\share\` での停止が不完全 | ui/src/stores/search.ts | `\\server` まで遡れてしまう。UNC 利用者のみ影響 |
| App.tsx の win.on* 系 cleanup 未登録 | App.tsx | HMR のみ影響・TODO.md M6 に記録済み・保留 |
| IME タイミング競合 | platform.rs | HWND 直接渡しで緩和済み・理論上残存 |
| WM_CONTEXTMENU と WM_RBUTTONUP の二重メニュー | platform.rs | 一部環境での二重表示リスク・未対応 |
| requestId の二重管理 | App.tsx / search.ts | 現状整合・設計上の注意点として記録 |
| commands.rs の `"hotkey_registration_failed"` | commands.rs | ユーザーに英語コードが表示される責務違反・未対応 |
