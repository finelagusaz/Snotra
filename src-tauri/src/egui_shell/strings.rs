//! egui 経路の UI 文言テーブル（#532 SU5）。`ui/src/lib/i18n.ts` の同キー値と一字一句一致
//! させる（parity の正本は i18n.ts）。言語は config `general.language` を起動時に一回読む
//! 静的解決（hot-reload＝`language-changed` 追従は SU6 の config 反映で拡張・spec 決定 10）。
//! snotra-core は「UI 表示文字列を持たない」規約のため、文言はこの crate（UI 層）に置く。

use snotra_core::config::Language;

pub fn search_hint(l: Language) -> &'static str {
    match l {
        Language::Ja => "検索...",
        Language::En => "Search...",
    }
}

pub fn tool_select_hint(l: Language) -> &'static str {
    match l {
        Language::Ja => "ツールを選択...",
        Language::En => "Select a tool...",
    }
}

pub fn indexing_hint(l: Language) -> &'static str {
    match l {
        Language::Ja => "インデックス構築中...",
        Language::En => "Building index...",
    }
}

pub fn launching(l: Language) -> &'static str {
    match l {
        Language::Ja => "起動中...",
        Language::En => "Launching...",
    }
}

/// detail は `notifyLaunchFailure` parity: message があれば " (msg)"、無ければ空文字を渡す。
pub fn launch_failed(l: Language, detail: &str) -> String {
    match l {
        Language::Ja => format!("起動に失敗しました{detail}"),
        Language::En => format!("Launch failed{detail}"),
    }
}

/// timeout＝「結果不明」の意味論（spec 決定 8）。WebView2 の文言が既にこの意味を持つ。
pub fn launch_timeout(l: Language, detail: &str) -> String {
    match l {
        Language::Ja => format!("起動に時間がかかっています{detail}"),
        Language::En => format!("Launch is taking a while{detail}"),
    }
}

/// ホットキー登録失敗通知（i18n.ts `notice.hotkey.change_failed` と一字一句一致・
/// {hotkey} は書式挿入。英語文言に句点は付かない＝i18n.ts 実物どおり）。
pub fn hotkey_change_failed(l: Language, hotkey: &str) -> String {
    match l {
        Language::Ja => format!("ホットキー ({hotkey}) の登録に失敗しました。元のホットキーを維持します"),
        Language::En => format!("Failed to register hotkey ({hotkey}). Keeping the previous hotkey"),
    }
}

pub fn update_available(l: Language, version: &str) -> String {
    match l {
        Language::Ja => format!("v{version} が利用可能です"),
        Language::En => format!("v{version} is available"),
    }
}

pub fn update_install_now(l: Language) -> &'static str {
    match l {
        Language::Ja => "今すぐ更新",
        Language::En => "Update now",
    }
}

pub fn update_dismiss(l: Language) -> &'static str {
    match l {
        Language::Ja => "閉じる",
        Language::En => "Dismiss",
    }
}

pub fn update_installing(l: Language) -> &'static str {
    match l {
        Language::Ja => "インストール中...",
        Language::En => "Installing...",
    }
}

pub fn update_failed(l: Language) -> &'static str {
    match l {
        Language::Ja => "更新に失敗しました",
        Language::En => "Update failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_are_interpolated_in_both_languages() {
        assert_eq!(launch_failed(Language::Ja, " (exe not found)"), "起動に失敗しました (exe not found)");
        assert_eq!(launch_failed(Language::En, ""), "Launch failed");
        assert_eq!(update_available(Language::En, "1.2.3"), "v1.2.3 is available");
    }

    #[test]
    fn timeout_wording_is_indeterminate_not_failure() {
        // spec 決定 8: timeout は「失敗」でなく「結果不明」。文言に「失敗」を含めない。
        assert!(!launch_timeout(Language::Ja, "").contains("失敗"));
        assert!(!launch_timeout(Language::En, "").to_lowercase().contains("failed"));
    }

    #[test]
    fn hotkey_change_failed_matches_i18n() {
        // i18n.ts notice.hotkey.change_failed の値と一字一句一致（2026-07-24 実物確認）。
        assert_eq!(
            hotkey_change_failed(Language::Ja, "Alt+Q"),
            "ホットキー (Alt+Q) の登録に失敗しました。元のホットキーを維持します"
        );
        assert!(hotkey_change_failed(Language::En, "Alt+Q").contains("Alt+Q"));
    }
}
