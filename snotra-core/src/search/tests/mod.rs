//! `search.rs` のユニットテスト。責務単位のサブモジュールへ分割（#597）。
//!
//! 各サブモジュールは `crate::search` の descendant であり、private 実装（フィールド・
//! `ScoredEntry`・`adjusted_history_boost` 等）へ直接アクセスできる。共通 fixture のみ
//! `common` に集約し、テスト固有ヘルパーは各モジュールに閉じる。

mod common;

mod basic;
mod incremental;
mod migemo;
mod path;
mod performance;
mod ranking;
