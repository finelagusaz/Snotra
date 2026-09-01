//! Windows のログオン時自動起動（`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`）の
//! 登録・解除・状態取得。
//!
//! **状態の正本は OS（レジストリ）であり、`config.toml` ではない。** 設定アプリのチェックボックスは
//! この値を直接読み書きし、`Config` には対応するフィールドを持たない——持たせると、バックアップの
//! export/import が機体ローカルの OS 状態を持ち運び、「初期設定に戻す」が登録解除の副作用を持つ。
//! 判断の全文と却下した代替案は `SPEC.md` §7.7。
//!
//! # 受容する残余
//!
//! - **OS I/O（[`is_enabled`] / [`enable`] / [`disable`]）にはテストが無い。** 実 `HKCU` を触るため、
//!   テストを書くと開発機のスタートアップ登録を書き換える。守っているのは純粋部
//!   （[`command_line_for`] / `main_exe_from`）だけである。測定のためだけの注入点は足さない
//!   （`docs/adr/ADR-no-test-only-injection-in-product-code.md`）
//! - **タスクマネージャーで無効化されても検知しない。** Windows は `Run` 値を消さず
//!   `Explorer\StartupApproved\Run` へ無効マークを置く。[`is_enabled`] の意味論は
//!   「`Run` 値が存在する」であって「実効的に有効」ではない

use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

/// `Run` に置く値名。
///
/// **変えると既存の登録が孤児になる**（古い名前の値が残り、新しい名前で二重に登録される）。
/// `SPEC.md` §7.7 が凍結している。
pub const RUN_VALUE_NAME: &str = "Snotra";

/// 登録する本体の実行ファイル名。設定アプリ自身の隣に居る。
pub const MAIN_EXE_FILE_NAME: &str = "snotra.exe";

/// ログオン時に起動するプログラムを列挙する、ユーザー単位のレジストリキー。
#[cfg(windows)]
const RUN_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// スタートアップ登録の操作が失敗した理由。
///
/// **表示文言は持たない**——この crate は UI 文字列を持たず、文言の組み立ては
/// `snotra-settings/src/i18n.rs` の責務である。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutostartError {
    /// 本体の実行ファイルを導けなかった（`current_exe` の失敗・親ディレクトリ不在・ファイル不在）。
    MainExeNotFound,
    /// レジストリ操作が失敗した。Win32 のエラーコードを持つ。
    Registry(u32),
}

/// `Run` 値に書く文字列。**必ず引用符で括る**。
///
/// 引用符が要るのは、既定のインストール先が `%LOCALAPPDATA%` 配下でパスにユーザー名を含み、
/// **ユーザー名は空白を含みうる**ためである。引用符の無い `Run` 値は最初の空白で切られる。
pub fn command_line_for(exe: &Path) -> String {
    format!("\"{}\"", exe.display())
}

/// 判定核（`current_exe` を読まないので並列テストから安全に測れる）。
///
/// `settings_exe` と同じディレクトリの [`MAIN_EXE_FILE_NAME`] を返す。親ディレクトリを取れない
/// パスと、親が空（＝ディレクトリ成分を持たない相対パス）のときは `None`——どちらも
/// `Run` 値として書けば壊れた相対パスになる。
fn main_exe_from(settings_exe: &Path) -> Option<PathBuf> {
    let dir = settings_exe.parent()?;
    if dir.as_os_str().is_empty() {
        return None;
    }
    Some(dir.join(MAIN_EXE_FILE_NAME))
}

/// `snotra-settings.exe` の隣にある `snotra.exe` の絶対パス。実在しなければ `None`。
pub fn main_exe_path() -> Option<PathBuf> {
    let settings_exe = std::env::current_exe().ok()?;
    let main_exe = main_exe_from(&settings_exe)?;
    main_exe.exists().then_some(main_exe)
}

/// UTF-16 の null 終端バッファ（Win32 の `W` 系 API へ渡す形）。
///
/// **`PCWSTR` を作るときは戻り値を必ずローカルへ束縛する**——式の中で `.as_ptr()` まで一息に
/// 書くと、指す先が同じ文の終わりで落ちるバッファになる。
#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// `Run` キーを `access` で開く。**バッファをローカルへ束縛する唯一の地点**であり、
/// 3 つの公開関数はここを通ることで [`wide`] の寿命の落とし穴を避ける。
#[cfg(windows)]
fn open_run_key(
    access: windows::Win32::System::Registry::REG_SAM_FLAGS,
) -> Option<crate::win_registry::RegKeyGuard> {
    let subkey = wide(RUN_SUBKEY);
    crate::win_registry::open_hkcu(windows::core::PCWSTR::from_raw(subkey.as_ptr()), access)
}

/// 値名 [`RUN_VALUE_NAME`] が `Run` に存在するか。**読み取り失敗は `false` として扱う。**
///
/// 誤る向きは「登録済みなのに未登録と表示する」側であり、逆より害が小さい——利用者が
/// チェックを入れ直せば上書きで治る。
#[cfg(windows)]
pub fn is_enabled() -> bool {
    use windows::Win32::System::Registry::{KEY_READ, RegQueryValueExW};
    use windows::core::PCWSTR;

    let Some(key) = open_run_key(KEY_READ) else {
        return false;
    };
    let name = wide(RUN_VALUE_NAME);
    unsafe {
        RegQueryValueExW(
            key.key(),
            PCWSTR::from_raw(name.as_ptr()),
            None,
            None,
            None,
            None,
        )
        .is_ok()
    }
}

#[cfg(not(windows))]
pub fn is_enabled() -> bool {
    false
}

/// 登録する。**既存値があっても現在のパスで上書きする**（移動したポータブル版の stale な
/// エントリはこれで治る）。
#[cfg(windows)]
pub fn enable() -> Result<(), AutostartError> {
    use windows::Win32::System::Registry::{KEY_WRITE, REG_SZ, RegSetValueExW};
    use windows::core::PCWSTR;

    let exe = main_exe_path().ok_or(AutostartError::MainExeNotFound)?;
    let value = wide(&command_line_for(&exe));
    let key = open_run_key(KEY_WRITE).ok_or(AutostartError::Registry(0))?;
    let name = wide(RUN_VALUE_NAME);
    // `RegSetValueExW` はバイト列を取るので、UTF-16 のバッファを null 終端込みでそのまま渡す。
    let bytes = unsafe { std::slice::from_raw_parts(value.as_ptr() as *const u8, value.len() * 2) };
    let status = unsafe {
        RegSetValueExW(
            key.key(),
            PCWSTR::from_raw(name.as_ptr()),
            Some(0),
            REG_SZ,
            Some(bytes),
        )
    };
    if status.is_err() {
        return Err(AutostartError::Registry(status.0));
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn enable() -> Result<(), AutostartError> {
    Ok(())
}

/// 解除する。**値が存在しなくても `Ok(())`**（`ERROR_FILE_NOT_FOUND` を成功へ畳む）。
///
/// 冪等にするのは、チェックボックスを外す操作が「値を消す」ではなく「登録されていない状態にする」
/// を意味するためである。外から先に消されていても利用者には成功として見えるべきである。
#[cfg(windows)]
pub fn disable() -> Result<(), AutostartError> {
    use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
    use windows::Win32::System::Registry::{KEY_WRITE, RegDeleteValueW};
    use windows::core::PCWSTR;

    let key = open_run_key(KEY_WRITE).ok_or(AutostartError::Registry(0))?;
    let name = wide(RUN_VALUE_NAME);
    let status = unsafe { RegDeleteValueW(key.key(), PCWSTR::from_raw(name.as_ptr())) };
    if status.is_err() && status != ERROR_FILE_NOT_FOUND {
        return Err(AutostartError::Registry(status.0));
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn disable() -> Result<(), AutostartError> {
    Ok(())
}
