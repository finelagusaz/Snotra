//! 検索結果として UI 層へ渡すデータ型。
//!
//! `snotra-core` の内部表現（`indexer::Entry` 等）とは別に、表示に必要な最小の形を定義する。
//!
//! **プロセス内で UI 層へ渡すだけの型であり、線上表現もオンディスク表現も持たない。**
//! 消費者は同一プロセスの egui view（`src-tauri/src/egui_shell/`）だけで、永続形式
//! （`index.bin` / `history.bin` 等）はこの型を通らない。**ゆえに serde を派生させないこと**
//! ——派生させた瞬間に、フィールド名・enum variant 名が「外から見える形式」に見え始めるが、
//! それを読む相手はどこにも居ない。

#[derive(Debug, Clone, PartialEq, Eq)]
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
/// derive は [`SearchResult`] と同じ集合を保つこと（`Default` を足すのはこちらだけ）。あちらの
/// `PartialEq` / `Eq` は `RowsSnapshot` の行比較が使うので、この enum が落とすと行全体で落ちる。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
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
    /// での剪定が別々に導くと、片方だけが `path` を見た瞬間に「抽出したのに引けない」
    /// 「抽出した直後に剪定で捨てる」が起きる（#1133）。
    ///
    /// **列挙失敗行（`is_error`）は `icon` の値によらずキーを持たない**（`SPEC.md`「3.4 アイコン」・
    /// #1134）。[`crate::folder::error_result`] は `path` に**実在ディレクトリの絶対パス**を入れる
    /// ので、`icon` だけを見ると本物のフォルダアイコンのキーになる。**要求側だけで弾いた版には、
    /// 前の世代で抽出したテクスチャがエラー行に描かれる経路が残っていた**（#1134 で辿った）——
    /// ここで折り込めば、キーを読む側が個別に条件を持たずに済む。
    pub fn icon_key(&self) -> Option<&str> {
        if self.is_error {
            return None;
        }
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
    fn icon_key_is_none_for_error_rows() {
        // 列挙失敗行（`SPEC.md`「3.4 アイコン」）。`folder::error_result` は `path` に**実在
        // ディレクトリ**を入れるので、`IconSource` だけを見ると本物のフォルダアイコンが引ける
        // （#1134）。キーを返す 2 つの variant（`FromPath` / `Explicit`）で測る。
        let err = SearchResult {
            is_error: true,
            ..row(r"C:\Windows", IconSource::FromPath)
        };
        assert_eq!(err.icon_key(), None, "FromPath のエラー行");
        let explicit_err = SearchResult {
            is_error: true,
            ..row(
                r"C:\Windows\notepad.exe {query}",
                IconSource::Explicit(r"C:\Windows\notepad.exe".into()),
            )
        };
        assert_eq!(explicit_err.icon_key(), None, "Explicit のエラー行");
    }

    #[test]
    fn icon_source_default_is_from_path() {
        // 既定が `FromPath` であることは費用の話でもある（`IconSource` の doc）。
        assert_eq!(IconSource::default(), IconSource::FromPath);
    }
}
