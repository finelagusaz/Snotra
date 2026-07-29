//! 検索結果として UI 層へ渡すデータ型。
//!
//! `snotra-core` の内部表現（`indexer::Entry` 等）とは別に、表示に必要な最小の形を定義する。
//!
//! **serde 派生と `#[serde(rename_all = "camelCase")]` は消費者を失っている**（#836 で実測）。
//! これらは WebView2 フロントへの IPC 形式のためのもので、#532 SU7 の撤去で相手が消えた
//! ——`SearchResult` を実際にシリアライズする呼び出し点はリポジトリに 1 つも無く、永続形式
//! （`index.bin` / `history.bin` 等）にも入らない。**撤去は別作業**（同じ SU7 残滓のクラスとして
//! `FolderExpansionState` は #836 で消した）。**「IPC 形式ゆえ変更しない」という旧記述は
//! 撤回済みである**——保護する理由が既に無いものを保護していた。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub name: String,
    pub path: String,
    pub is_folder: bool,
    pub is_error: bool,
}
