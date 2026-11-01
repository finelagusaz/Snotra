//! egui view・トレイが共有する core 関数群（旧 `#[tauri::command]` ラッパーは #532 SU7 のフロント撤去で消滅）。
//!
//! # 共有 core 関数の返り値契約
//!
//! 旧 IPC コマンドの 3 系統規約（#434）のうち、フロント撤去後も残る契約:
//!
//! 1. **読み取り・検索系**: 素の `T` を返す。エラーは DTO 内の `is_error` フラグで運び、表示文字列は UI 層が決める
//! 2. **起動系**: `LaunchResult { status, code, message }`（`launch_item_core` / `launch_with_tool_core` /
//!    `execute_instant_action_core`）
//! 3. **失敗しうる操作系**: `Result<T, String>`。「実行できない状態」（インデックス構築中など）も `Err(定数)` で
//!    表す（`open_settings` / `rebuild_index`）
//!
//! **`bool` 返しを新しい関数で使わない**——「成功/失敗」と「実行できない状態」を混同しやすい。
//! 「実行できない状態」を足すときは既存の定数（`ERR_INDEXING_IN_PROGRESS`）を再利用するか、
//! 命名を `ERR_<状態>` に揃える。

mod icon;
pub(crate) mod instant;
pub(crate) mod launch;
mod system;
mod window;

pub(crate) use icon::*;
pub use launch::*;
pub use system::*;
pub use window::*;

// Re-export pub(crate) items that main.rs references
pub(crate) use window::launch_settings_process;

/// Thin wrapper kept so call sites read `trace_command(...)`; logic lives in
/// the shared `crate::trace` module (deduped with `main.rs`'s `trace_main`, #433).
pub(crate) fn trace_command(event: &str, data: serde_json::Value) {
    crate::trace::trace(event, data);
}
