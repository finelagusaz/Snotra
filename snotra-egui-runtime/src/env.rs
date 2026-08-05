//! trace ハッチ（`SNOTRA_EGUI_*_TRACE`）の env 述語。
//!
//! **空文字を「未設定」として扱う唯一の場所である。** `var_os(..).is_some()` は `Some("")` を
//! 真と読むため、値を消したつもりの空文字で計器が点く。PowerShell の
//! `[Environment]::SetEnvironmentVariable($name, $null, 'Process')` は変数を消さず**空文字で
//! 作る**ので、この経路は実際に踏まれた——#872 の測定ハーネス（決着後に撤去済み。経緯と実測は
//! `ADR-egui-trace-hatch-empty-only`）が反復ごとの env 復元でこれを作り、**2 反復目以降の
//! 全測定が計器つきで走っていた**（実測 26/27 反復）。計器は 1 事象あたり 2 行の stderr を
//! 挟むため、率も故障の現れ方も変える。
//!
//! **`src-tauri/src/trace.rs` の `env_flag`（`1|true|yes|on` の許可リスト）へは寄せない。**
//! こちらのハッチには PowerShell 側にも緩い読み手が居り（`scripts/lib/SnotraSmoke.psm1` の
//! `Send-SnotraKey`）、許可リストにすると `=0` のような値で**新しい食い違いが生まれる**
//! ——変更前は両者とも ON で一貫していたところが、Rust だけ OFF になる。実バグは空文字
//! ちょうどなので、**新しい分類を 1 つも作らずに塞ぐ**方を採る。
//!
//! 同じ「空文字は未設定」の判断は `snotra-core/src/config.rs` の `config_dir_from` にもあり、
//! そちらは `PathBuf::from("")` が CWD 相対になることを理由に挙げている。

/// 判定核（env を読まないので並列テストから安全に、網羅的に測れる）。
///
/// 判定核を分ける形は `snotra-core` の `config_dir_from` と同じ流儀である——edition 2024 では
/// `std::env::set_var` / `remove_var` が `unsafe` であり、env を触るテストは並列実行とも
/// 噛み合わない（`cargo test` は同一プロセス内でテストを並列に走らせる）。
fn is_enabled(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|v| !v.is_empty())
}

/// trace ハッチが有効か。**空文字は未設定として扱う。**
pub(crate) fn trace_hatch_enabled(name: &str) -> bool {
    is_enabled(std::env::var_os(name).as_deref())
}

#[cfg(test)]
mod tests {
    use super::is_enabled;
    use std::ffi::OsStr;

    #[test]
    fn empty_value_is_treated_as_unset() {
        assert!(!is_enabled(None), "未設定は偽");
        assert!(
            !is_enabled(Some(OsStr::new(""))),
            "空文字は偽（#872 の実バグはこれちょうどである）"
        );
    }

    #[test]
    fn any_non_empty_value_enables() {
        // **許可リストにしない。** `0` や `false` も真である——PowerShell 側の読み手
        // （`if ($env:X)`）が同じ判定であり、値クラスごとの食い違いを作らないため。
        for v in ["1", "0", "true", "false", "on", "verbose", " "] {
            assert!(
                is_enabled(Some(OsStr::new(v))),
                "{v} は真（空でなければ点く）"
            );
        }
    }
}
