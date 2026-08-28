//! 設定・履歴・索引・アイコン・ウィンドウ位置を置くディレクトリの解決。
//!
//! **この crate で保存先を導く経路はここである**——`config.toml` / `history.bin` / `index.bin` /
//! `icons.bin` / `window.bin` のすべてが [`Config::config_dir`] から派生する。env 上書きの契約
//! （そのまま使う・空文字は未設定・展開も絶対化もしない）は [`Config::config_dir_from`] の
//! rustdoc が正本である。

use std::ffi::OsString;
use std::path::PathBuf;

use super::Config;

/// 保存先ディレクトリを上書きする環境変数（#803）。
/// 検証・デバッグで実 config を壊さずに別プロファイルで起動するための開発向けハッチ。
const ENV_CONFIG_DIR: &str = "SNOTRA_CONFIG_DIR";

impl Config {
    /// Returns true if this is the first run (no config file exists yet).
    /// Must be called before `Config::load()` since load() creates the file.
    pub fn is_first_run() -> bool {
        match Self::config_path() {
            Some(path) => !path.exists(),
            None => true,
        }
    }

    /// 設定・履歴・索引・アイコン・ウィンドウ位置を置くディレクトリ。
    ///
    /// 既定は `dirs::config_dir()/Snotra`（Windows では `%APPDATA%\Snotra`）で、
    /// 環境変数 `SNOTRA_CONFIG_DIR` を設定するとその値が**そのまま**保存先になる。
    /// **この crate で保存先を導く経路はここ 1 つだけ**であり、`config.toml` /
    /// `history.bin` / `index.bin` / `icons.bin` / `window.bin` のすべてがここから派生する。
    pub fn config_dir() -> Option<PathBuf> {
        Self::config_dir_from(std::env::var_os(ENV_CONFIG_DIR), dirs::config_dir())
    }

    /// `config_dir()` の判定核（env を読まないので並列テストから安全に測れる）。
    ///
    /// - 上書きは**そのまま**使い、`Snotra` を付け足さない。既定側だけが付ける
    ///   この非対称は意図的である——検証スクリプトが渡したパスの下に更に階層を作らせないため。
    /// - **空文字は「未設定」として扱う。** `PathBuf::from("")` は相対パスであり、
    ///   そのまま使うと `config.toml` がカレントディレクトリへ落ちる。
    /// - **展開も絶対化もしない。** Windows は `var_os` の `%VAR%` を展開せず、相対パスは
    ///   CWD 起点になる。**不正な値を既定へ落とす設計にはしない**——書き損じたときに既定へ
    ///   落ちると検証がユーザーの実 config を触ってしまい、この env ハッチの目的が裏返る。
    fn config_dir_from(override_dir: Option<OsString>, base: Option<PathBuf>) -> Option<PathBuf> {
        if let Some(dir) = override_dir
            && !dir.is_empty()
        {
            return Some(PathBuf::from(dir));
        }
        base.map(|p| p.join("Snotra"))
    }

    pub fn config_path() -> Option<PathBuf> {
        Self::config_dir().map(|p| p.join("config.toml"))
    }
}

#[cfg(test)]
mod tests;
