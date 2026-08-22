---
paths:
  - "src-tauri/**/*.rs"
---

# src-tauri ルール（ルーター）

事実の正本は `src-tauri/CLAUDE.md` とコード。要約コピーは置かず正本へ指す。ただし CLAUDE.md に正本の無い src-tauri 固有の不変条件は「この rule が正本」節に残す。位置はファイル名・行で断定せず**見出し名・シンボル名で grep**（#588）。

## 読む正本（`src-tauri/CLAUDE.md` の該当節）

- index-build フラグは `try_begin_index_build` / `finish_index_build` 経由（`indexing` / `index_build_started` を直接 store しない）: 「実装パターン」
- ランタイムでウィンドウ生成しない（メッセージポンプ進行を要求しデッドロック・生成は setup 限定）: 「ウィンドウ生成の制約」
- `windows` クレート（v0.62）の API 型・feature フラグを使用前に確認: 「Win32 / Tauri 注意事項」
- `ShellExecuteW` + シェル拡張は COM STA 必須（`std::thread::spawn` + `CoInitializeEx`・EXE は不要）: 「Win32 / Tauri 注意事項」
- engine ロックは async/blocking 境界またぎで保持しない（抽出 → 即解放 → 処理）: `snotra-core/CLAUDE.md`「engine.rs のロック最小化パターン」

## この rule が正本（CLAUDE.md に無い src-tauri 固有）

- **`snotra-settings.exe` 等の子プロセスを spawn したら、`main.rs` の exit ハンドラで `child.kill()` する**（生成/破棄のペア）
- **Win32 を呼ぶ経路の新設は `PlatformBridge` 経由を既定とする**: `platform::{PlatformBridge, PlatformCommand}` で platform スレッドのメッセージループへ委ねる。**既存の直呼びは残る**——個別の理由は `src-tauri/CLAUDE.md`「Win32 / Tauri 注意事項」
- **状態フラグを true にしたら false に戻す経路とセットで設計する**: 戻す責務の関数に `#[must_use]` を付け `let _ =` 無視を compile-fail 検出（実例 `launch_settings_process`）。一般則は `AGENTS.md`「事前調査」
- **Win32 依存モジュール（`ime.rs`・`platform/` の `hotkey.rs` 等）はユニットテスト前提にしない**

## トリガー → 検査

- 並行境界（worker・channel・drain・listener・共有状態・live-read・async）を追加/変更したら: `/race-check`（**述語の全文は常時ロードの `AGENTS.md`「条件別チェック」表が SSOT**——ここに写しは置かない。`.await` は `egui_shell/` の updater 経路に実在する）
- ホットキー・ウィンドウ生成/表示順・スラッシュコマンド経路を変更したら: カテゴリ A に加え `docs/build-commands.md` カテゴリ C（`smoke:startup` / `smoke:egui`）も該当。post-edit hook は A（fmt/clippy/test）だけで「沈黙 = 合格」は C を含まない。検証カテゴリは拡張子でなく変更が触れるコードパスの意味で決める（#558）
