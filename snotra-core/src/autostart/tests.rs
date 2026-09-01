//! `autostart` の純粋部（コマンドライン値の組み立てと本体パスの導出）のテスト。
//!
//! **OS I/O（`is_enabled` / `enable` / `disable`）はここで測らない**——実 `HKCU` を触るため、
//! 開発機のスタートアップ登録を書き換えてしまう。理由の全文は親モジュールの `//!`。

use std::path::{Path, PathBuf};

use super::{MAIN_EXE_FILE_NAME, command_line_for, main_exe_from};
#[cfg(windows)]
use super::{as_bytes, wide};

// 不変条件: `Run` 値は必ず引用符で括られる（引用符が無いと最初の空白でコマンドが切れる）。
#[test]
fn command_line_is_always_quoted() {
    let line = command_line_for(Path::new(r"C:\Apps\Snotra\snotra.exe"));
    assert!(
        line.starts_with('"') && line.ends_with('"'),
        "Run value must be quoted, got: {line}"
    );
}

// 不変条件: 空白を含むパス（`%LOCALAPPDATA%` 配下はユーザー名を含み、それは空白を含みうる）でも
// 引用符の内側にパス全体が入る。
#[test]
fn command_line_wraps_path_with_spaces() {
    let line = command_line_for(Path::new(
        r"C:\Users\John Doe\AppData\Local\Snotra\snotra.exe",
    ));
    assert_eq!(
        line,
        r#""C:\Users\John Doe\AppData\Local\Snotra\snotra.exe""#
    );
}

// 不変条件: 本体は設定アプリの兄弟である（実配置で 3 本が同一ディレクトリに同居する）。
#[test]
fn main_exe_is_sibling_of_settings_exe() {
    let derived = main_exe_from(Path::new(r"C:\Apps\Snotra\snotra-settings.exe"));
    assert_eq!(
        derived,
        Some(PathBuf::from(r"C:\Apps\Snotra").join(MAIN_EXE_FILE_NAME))
    );
}

// 不変条件: `REG_SZ` の `cbData` は「UTF-16 のバイト数（null 終端込み）」である。
//
// **要素数（`len()`）を渡すと値が半分で切れ、null 終端も失う。** この誤りは `enable()` の中でしか
// 現れないので、他のどの検査も見ない——`cargo test` / `clippy` / `cargo doc` / `governance:check` の
// すべてが緑のまま通ることを変異注入で実測した。これはその死角へ置いた唯一の検知器である。
#[cfg(windows)]
#[test]
fn as_bytes_counts_utf16_bytes_including_the_null_terminator() {
    let value = wide("ab");
    assert_eq!(value.len(), 3, "wide は null 終端を足すので 3 要素");
    assert_eq!(
        as_bytes(&value).len(),
        6,
        "cbData は要素数ではなくバイト数（3 要素 × 2）でなければならない"
    );
}

// 不変条件: 非 BMP 文字（サロゲートペア）でも要素数とバイト数の関係が崩れない。
#[cfg(windows)]
#[test]
fn as_bytes_handles_surrogate_pairs() {
    let value = wide("\u{1F600}");
    assert_eq!(value.len(), 3, "サロゲートペア 2 要素 + null 終端");
    assert_eq!(as_bytes(&value).len(), 6);
}

// 不変条件: 親ディレクトリを取れないパスからは導出しない（ルートそのものを渡された場合）。
#[test]
fn main_exe_from_root_is_none() {
    assert_eq!(main_exe_from(Path::new(r"C:\")), None);
}

// 不変条件: ディレクトリ成分を持たない相対パスからは導出しない。
// `Path::parent` は空パスを `Some` で返すため、そのまま join すると `Run` へ相対パスを書いてしまう。
#[test]
fn main_exe_from_bare_file_name_is_none() {
    assert_eq!(main_exe_from(Path::new("snotra-settings.exe")), None);
}
