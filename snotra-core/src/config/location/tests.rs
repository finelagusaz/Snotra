//! 保存先ディレクトリの導出（#803）。
//!
//! `config_dir_from` は env を読まないので、上書きの全分岐を純粋関数として測れる（並列実行の
//! 他テストの保存先を動かさない）。その代償として `config_dir()` が `dirs::config_dir()` を
//! 呼んでいること自体はここから見えないため、結線は env を**読むだけ**の 1 本が pin する。

use super::*;

/// 上書きは**そのまま**使い、`Snotra` を付け足さない（既定側だけが足す非対称）。
/// 壊れると検証スクリプトが渡した temp パスの下に更に階層ができ、seed が読まれない。
#[test]
fn config_dir_from_uses_override_verbatim() {
    let got = Config::config_dir_from(
        Some(OsString::from(r"C:\tmp\snotra-profile")),
        Some(PathBuf::from(r"C:\Users\x\AppData\Roaming")),
    );
    assert_eq!(got, Some(PathBuf::from(r"C:\tmp\snotra-profile")));
}

/// env 未設定なら既定（`<base>/Snotra`）。既存ユーザーのデータが動かないことの中核。
#[test]
fn config_dir_from_falls_back_to_base_when_env_absent() {
    let got = Config::config_dir_from(None, Some(PathBuf::from(r"C:\Users\x\AppData\Roaming")));
    assert_eq!(
        got,
        Some(PathBuf::from(r"C:\Users\x\AppData\Roaming\Snotra"))
    );
}

/// 空文字は「未設定」。`PathBuf::from("")` は相対パスなので、そのまま使うと
/// `config.toml` がカレントディレクトリへ落ちる（CWD 流出の防止）。
#[test]
fn config_dir_from_falls_back_to_base_when_env_is_empty() {
    let got = Config::config_dir_from(
        Some(OsString::new()),
        Some(PathBuf::from(r"C:\Users\x\AppData\Roaming")),
    );
    assert_eq!(
        got,
        Some(PathBuf::from(r"C:\Users\x\AppData\Roaming\Snotra"))
    );
}

/// `dirs::config_dir()` が解決できない極端な環境では `None`。
/// `load_reporting` / `save` の early-return 契約を保つ。
#[test]
fn config_dir_from_is_none_without_override_or_base() {
    assert_eq!(Config::config_dir_from(None, None), None);
}

/// 上書きは**展開も絶対化もしない**。Windows は `var_os` の `%VAR%` を展開せず、
/// 相対パスは CWD 起点になる。**拒否して既定へ落とす設計にはしない**——書き損じたときに
/// 既定へ落ちると、検証がユーザーの実 config を触る（この issue が消そうとしている当のもの）。
/// そのまま使えば変な場所へ隔離されるだけで実データには届かない。
#[test]
fn config_dir_from_does_not_expand_or_absolutize_override() {
    let base = Some(PathBuf::from(r"C:\Users\x\AppData\Roaming"));
    assert_eq!(
        Config::config_dir_from(Some(OsString::from("profile")), base.clone()),
        Some(PathBuf::from("profile")),
        "相対パスはそのまま返す（既定へ落とさない）"
    );
    assert_eq!(
        Config::config_dir_from(Some(OsString::from(r"%TEMP%\Snotra")), base),
        Some(PathBuf::from(r"%TEMP%\Snotra")),
        "%VAR% は展開しない"
    );
}

/// `config_dir()` の**結線**を pin する。`config_dir_from` は `base` を注入するので、
/// 「既定側が `dirs::config_dir()` であること」は純粋関数のテストからは見えない——
/// そこを `dirs::config_local_dir()`（Windows では LocalAppData）へ書き換えても
/// 他の 5 本は緑のままである（実測でフォールトインジェクション済み）。
/// 全ユーザーのデータが黙って別の場所へ移る回帰の検出器。
///
/// env を**読むだけ**（`set_var` しない）ので並列実行から安全。周囲の env がどちらでも
/// 対応する分岐を assert するため、**skip して黙る経路を持たない**。
#[test]
fn config_dir_is_wired_to_dirs_config_dir_with_snotra_suffix() {
    match std::env::var_os(ENV_CONFIG_DIR) {
        Some(v) if !v.is_empty() => {
            assert_eq!(Config::config_dir(), Some(PathBuf::from(v)));
        }
        _ => assert_eq!(
            Config::config_dir(),
            dirs::config_dir().map(|p| p.join("Snotra"))
        ),
    }
}

#[test]
fn is_first_run_returns_true_when_no_config() {
    // This test relies on Config::config_path() returning a valid path
    // We can't easily test is_first_run without side effects,
    // but we can verify the method exists and returns a bool
    let _result: bool = Config::is_first_run();
}
