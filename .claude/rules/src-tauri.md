---
paths:
  - "src-tauri/**/*.rs"
---

# src-tauri ルール（ルーター）

事実の正本は `src-tauri/CLAUDE.md` とコードである。要約のコピーは置かず、正本を指す。ただし、CLAUDE.md に正本が無い src-tauri 固有の不変条件は、「この rule が正本」節に残す。位置はファイル名や行番号で決めつけず、見出し名やシンボル名で grep して探す（#588）。

## 読む正本（`src-tauri/CLAUDE.md` の該当節）

- ランタイムでウィンドウを生成しない（メッセージポンプが進むことを要求するので、デッドロックする。生成は setup の中だけで行う）: 「ウィンドウ生成の制約」
- `ShellExecuteW` とシェル拡張には COM STA が要る（EXE には要らない）: `commands/launch.rs` の `launch_item_core`
- engine のロックを、async と blocking の境界をまたいで持ち続けない（値を取り出したらすぐ解放し、それから処理する）: `snotra-core/CLAUDE.md`「engine.rs のロック最小化パターン」

## この rule が正本（CLAUDE.md に無い src-tauri 固有）

- `snotra-settings.exe` などの子プロセスを spawn したら、`main.rs` の exit ハンドラで `child.kill()` する（生成と破棄のペア）。
- Win32 を呼ぶ経路を新しく作るときは、`PlatformBridge` を通すことを既定にする。`platform::{PlatformBridge, PlatformCommand}` で、platform スレッドのメッセージループへ処理を渡す。既存の直接の呼び出しは残っている。
- 状態フラグを true にしたら、false に戻す経路とセットで設計する。戻す責務を持つ関数に `#[must_use]` を付け、`let _ =` での無視を compile-fail で検出する（実例は `launch_settings_process`）。一般則は `AGENTS.md`「条件別チェック（トリガー → 参照先）」の対称ペアの行にある。
- Win32 に依存するモジュール（`ime.rs`、`platform/` の `hotkey.rs` など）は、ユニットテストできる前提にしない。

## トリガー → 検査

- 並行境界（worker・channel・drain・listener・共有状態・live-read・async）を追加/変更したら: `/race-check`（述語の全文の SSOT は、常時ロードされる `AGENTS.md`「条件別チェック」表である。ここに写しは置かない。`.await` は `egui_shell/` の updater の経路に実際にある）
- ホットキー・ウィンドウの生成/表示順・スラッシュコマンドの経路を変更したら: カテゴリ A に加えて、`docs/build-commands.md` カテゴリ C（`smoke:startup` / `smoke:egui`）も該当する。検証カテゴリは、拡張子ではなく、変更が触れるコードパスの意味で決める（#558）。
