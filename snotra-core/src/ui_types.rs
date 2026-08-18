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
    /// この行のアイコンをどこから取るか（`SPEC.md`「3.4 アイコン」）。読むのは
    /// [`SearchResult::icon_key`] だけにする。
    pub icon: IconSource,
}

/// 結果行のアイコン抽出キーの出所（`SPEC.md`「3.4 アイコン」）。
///
/// **既定を `FromPath` にしてあるのは費用のためである。** 既定を `Explicit(path.clone())` で
/// 表すと、平文検索の全行（`result_limit` は既定 200・設定次第 1000）に `String` の確保が
/// もう 1 本乗る——行はフレームごとに snapshot へ複製されるので、そこは足してよい場所ではない。
///
/// derive は [`SearchResult`] と同じ集合を保つこと（あちらが `PartialEq` / `Eq` を
/// `RowsSnapshot` の行比較に使い、`Serialize` / `Deserialize` を #836 の残滓として持つ）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum IconSource {
    /// `path` をそのままキーにする（`path` がファイルを指す行の既定）。
    #[default]
    FromPath,
    /// アイコンを取らない（`path` がファイルを指さない行）。
    Skip,
    /// 別のファイルをキーにする（表示文字列と実体が違う行）。
    Explicit(String),
}

impl SearchResult {
    /// アイコン抽出のキー。`None` は「この行のアイコンは取らない」。
    ///
    /// **キーを読む箇所はこの 1 つの導出を通すこと。** 抽出の要求・テクスチャの引き・可視集合
    /// での剪定の 3 か所が別々に導くと、片方だけが `path` を見た瞬間に「抽出したのに引けない」
    /// 「抽出した直後に剪定で捨てる」が起きる（#1133）。
    pub fn icon_key(&self) -> Option<&str> {
        match &self.icon {
            IconSource::FromPath => Some(&self.path),
            IconSource::Skip => None,
            IconSource::Explicit(key) => Some(key),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{IconSource, SearchResult};

    fn row(path: &str, icon: IconSource) -> SearchResult {
        SearchResult {
            name: "n".into(),
            path: path.into(),
            is_folder: false,
            is_error: false,
            icon,
        }
    }

    #[test]
    fn icon_key_from_path_returns_the_row_path() {
        assert_eq!(
            row(r"C:\a\b.exe", IconSource::FromPath).icon_key(),
            Some(r"C:\a\b.exe")
        );
    }

    #[test]
    fn icon_key_skip_returns_none() {
        // `path` が何であっても取らない（URL・description 等・`SPEC.md` §3.4）。
        assert_eq!(
            row("https://example.com/?q={query}", IconSource::Skip).icon_key(),
            None
        );
    }

    #[test]
    fn icon_key_explicit_returns_the_key_not_the_path() {
        // **`path` を返してはならない**——表示文字列と抽出キーが別物である行のための variant。
        let r = row(
            r"C:\Windows\notepad.exe {query}",
            IconSource::Explicit(r"C:\Windows\notepad.exe".into()),
        );
        assert_eq!(r.icon_key(), Some(r"C:\Windows\notepad.exe"));
    }

    #[test]
    fn icon_source_default_is_from_path() {
        // 既定が `FromPath` であることは費用の話でもある（`IconSource` の doc）。
        assert_eq!(IconSource::default(), IconSource::FromPath);
    }
}
