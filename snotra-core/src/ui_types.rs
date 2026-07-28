//! 検索結果・ツール一覧など、UI 層へ渡すデータ型。
//!
//! `snotra-core` の内部表現とは別に、UI が読む形として serde 派生を持つ DTO を定義する
//! （`#[serde(rename_all = "camelCase")]` は永続・IPC 形式に触れるため変更しない）。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub name: String,
    pub path: String,
    pub is_folder: bool,
    pub is_error: bool,
}
