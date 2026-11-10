//! 設定・履歴・索引・アイコン・ウィンドウ位置を置くディレクトリの解決と、分離プロファイルの判定。
//!
//! 分離プロファイル（`SNOTRA_CONFIG_DIR` が実プロファイル以外を指す起動）の判定と識別子は
//! [`Config::isolated_profile_key`] が持つ。保存先と同じ env から導くので、ここに置く。
//!
//! **この crate で保存先を導く経路はここである**——`config.toml` / `history.bin` / `index.bin` /
//! `icons.bin` / `window.bin` のすべてが [`Config::config_dir`] から派生する。env 上書きの契約
//! （そのまま使う・空文字は未設定・展開も絶対化もしない）は [`Config::config_dir_from`] の
//! rustdoc が正本である。**`dirs::config_dir()` を直接呼ぶ箇所はこの crate の他に無い**（#803）
//! ——別経路で組むと `SNOTRA_CONFIG_DIR` が効かず「導出が 2 経路」の欠陥になる
//! （`docs/development-principles.md`「config の値は、読まれなくても誰も気づかない」）。判定核
//! [`Config::config_dir_from`] は env を読まないので並列テストから安全に測れるが、その代償として
//! `config_dir()` が `dirs::config_dir()` を呼んでいること自体は純粋関数のテストからは見えない。
//! その結線は `config_dir_is_wired_to_dirs_config_dir_with_snotra_suffix` が env を**読むだけ**で pin する。
//! Windows では `dirs::config_dir()` と `dirs::data_dir()` は同一（どちらも RoamingAppData・dirs 6.0.0 で
//! 実測し、7.0.0 でも `win.rs` の両行は不変）なので、実際に危険な取り違えは `config_local_dir()` /
//! `data_local_dir()`（LocalAppData）である。

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::Config;
use super::paths::normalize_scan_path_key;

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
        if let Some(dir) = effective_override(override_dir) {
            return Some(PathBuf::from(dir));
        }
        base.map(|p| p.join("Snotra"))
    }

    /// 分離プロファイル（`SNOTRA_CONFIG_DIR` が実プロファイル以外を指す起動）なら、その
    /// プロファイルを識別する 16 桁の小文字 16 進を返す。そうでなければ `None`。
    ///
    /// 単一インスタンス制御の名前・スタートアップ登録の可否・既定ホットキーがこれで分かれる
    /// （`SPEC.md` §13）。判定核は [`Config::isolated_profile_key_from`]。
    pub fn isolated_profile_key() -> Option<String> {
        Self::isolated_profile_key_from(std::env::var_os(ENV_CONFIG_DIR), dirs::config_dir())
    }

    /// `isolated_profile_key()` の判定核（env を読まない）。
    ///
    /// - 上書きの有無は `config_dir_from` と同じ述語で決める。食い違うと、保存先は実プロファイル
    ///   なのに単一インスタンスだけが分かれ、2 プロセスが同じファイルを書く。
    /// - **保存先の契約（そのまま使う）は変えず、鍵の導出だけを正規化する。** 同じディレクトリを
    ///   別の綴りで指した 2 プロセスは同じ鍵になり、排他のまま保たれる。実プロファイルを別の
    ///   綴りで指す上書きは `None` になる。
    /// - `base` を導けない環境でも、上書きがあれば分離する。ここで `None` を返すと、検証の起動が
    ///   普段使いの Snotra へ転送されて黙って終わる。
    fn isolated_profile_key_from(
        override_dir: Option<OsString>,
        base: Option<PathBuf>,
    ) -> Option<String> {
        let dir = effective_override(override_dir)?;
        let identity = profile_identity(Path::new(&dir));
        if let Some(base) = base
            && profile_identity(&base.join("Snotra")) == identity
        {
            return None;
        }
        Some(format!("{:016x}", fnv1a64(identity.as_bytes())))
    }

    pub fn config_path() -> Option<PathBuf> {
        Self::config_dir().map(|p| p.join("config.toml"))
    }
}

/// 上書きが有効か。`None` と空文字は未設定として扱う（`PathBuf::from("")` は相対パスで、
/// そのまま使うと `config.toml` がカレントディレクトリへ落ちる）。
fn effective_override(override_dir: Option<OsString>) -> Option<OsString> {
    override_dir.filter(|dir| !dir.is_empty())
}

/// 同じディレクトリかを比べるための鍵。CWD 起点で絶対化し（存在を要求しない）、
/// スキャンパスと同じ規則で正規化する。絶対化に失敗しても鍵を作る（作らずに `None` を返すと、
/// 分離プロファイルなのに分離されない）。
///
/// 大文字小文字の同一視は `to_lowercase` で、NTFS の大文字化表とは一致しない（`ß` など）。
/// その差で別の鍵になるディレクトリ名は、別のプロファイルとして扱う（`SPEC.md` §13）。
fn profile_identity(path: &Path) -> String {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    normalize_scan_path_key(&absolute.to_string_lossy())
}

/// FNV-1a（64 bit）。別ビルドのプロセス同士で同じ鍵を導くので、リリース間で値が安定しない
/// `DefaultHasher` は使えない。
fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes.iter().fold(OFFSET_BASIS, |hash, &b| {
        (hash ^ u64::from(b)).wrapping_mul(PRIME)
    })
}

#[cfg(test)]
mod tests;
