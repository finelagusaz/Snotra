//! Win32 レジストリ API の薄い共有層（`HKEY` の RAII と `HKEY_CURRENT_USER` 配下のキー開き）。
//!
//! **持つのは「開いて、閉じ忘れない」だけである**——読み書きの手順（値のサイズ取得 → 取得の 2 段、
//! `REG_EXPAND_SZ` の展開、削除の冪等化）は用途ごとに違うので、各呼び出し側が持つ。
//!
//! 共有する理由は [`RegKeyGuard`] が `RegCloseKey` を呼ぶだけの型であり、**片方だけが変わる将来を
//! 挙げられない**ことによる（`AGENTS.md`「検証の作法」の重複排除の判定）。

use windows::Win32::Foundation::WIN32_ERROR;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, REG_SAM_FLAGS, RegCloseKey, RegOpenKeyExW,
};
use windows::core::PCWSTR;

/// レジストリキーの RAII ガード。Drop 時に自動で `RegCloseKey` を呼ぶ。
pub(crate) struct RegKeyGuard(HKEY);

impl RegKeyGuard {
    /// 生の `HKEY`。**ガードより長生きさせない**（Drop で閉じられる）。
    pub(crate) fn key(&self) -> HKEY {
        self.0
    }
}

impl Drop for RegKeyGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

/// `HKEY_CURRENT_USER\<subkey>` を `access` で開く。
///
/// **失敗は Win32 のエラーコードを持って返る**——`Option` にすると呼び出し側が「開けなかった」を
/// 表す番号を自分で作ることになり、`0`（`ERROR_SUCCESS`）を失敗として利用者へ見せる形になる。
///
/// **キーを作らない。** 作成には `RegCreateKeyExW` が要るが、あれは `windows` crate の
/// `Win32_Security` feature を要求する（`SECURITY_ATTRIBUTES` を引数に取るため・実測）。
/// この crate の呼び出し先はいずれも Windows が用意する既存キーなので、開くだけで足りる。
pub(crate) fn open_hkcu(subkey: PCWSTR, access: REG_SAM_FLAGS) -> Result<RegKeyGuard, WIN32_ERROR> {
    unsafe {
        let mut raw_key = HKEY::default();
        let status = RegOpenKeyExW(HKEY_CURRENT_USER, subkey, Some(0), access, &mut raw_key);
        if status.is_err() {
            return Err(status);
        }
        Ok(RegKeyGuard(raw_key))
    }
}
