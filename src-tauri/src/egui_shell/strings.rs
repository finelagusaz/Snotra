//! egui 経路の UI 文言テーブル（#532 SU5）。**このテーブル自身が文言の正本である**——移行元
//! だった `ui/src/lib/i18n.ts` は SU7 のフロント撤去で消滅したため、突き合わせるべき外部の
//! parity 元はもう無い。**文言を足す・直すときは計画書・レビュー引用の文字列を写さず、実物の
//! ソースを開いて codepoint 単位で確認する**（SU5「…」vs「...」・SU6 En 末尾ピリオドの差は
//! 実物突合だけが捕捉した）。言語は呼び出しごとに引数で受ける——view.rs の `lang()` が
//! config `general.language` を毎フレーム live-read するため、config-applied wake（SU6）で
//! 言語切替が次フレームから反映される（起動時一回読みではない・#648(B) で旧記述を是正）。
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

/// 起動時ホットキー登録失敗通知（i18n.ts `notice.hotkey.initial_failed` と一字一句一致・
/// {hotkey} は書式挿入。英語文言に句点は付かない＝i18n.ts 実物どおり）。
/// `change_failed` と別キーなのは意図（旧ホットキー維持ではなく「開く手段が無い」告知）。
pub fn hotkey_initial_failed(l: Language, hotkey: &str) -> String {
    match l {
        Language::Ja => format!("ホットキー ({hotkey}) の登録に失敗しました。他のアプリが使用中の可能性があります"),
        Language::En => format!("Failed to register hotkey ({hotkey}). Another application may be using it"),
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

/// `reason` は**整形前の生の失敗理由**（`tauri_plugin_updater` のエラー文字列）。空なら
/// generic 文言のまま返す。
///
/// **`launch_failed` / `launch_timeout` の `detail`（呼び出し側が `" (msg)"` まで整形して渡す）
/// とは契約が違うため引数名を変えてある。** 区切りをこちら側に置くのは、「理由が空のとき
/// コロンだけ残る」という失敗様態をこのファイルのユニットテストで固定できるようにするため
/// ——呼び出し側で整形すると `view.rs` のインライン処理になり、検知手段が視覚スモークだけに
/// なる。区切り文字は元から 2 系統で違う（`" (msg)"` vs `": msg"`）ので、契約を揃える利得は
/// 元々無い（#654）。
pub fn update_failed(l: Language, reason: &str) -> String {
    let base = match l {
        Language::Ja => "更新に失敗しました",
        Language::En => "Update failed",
    };
    if reason.is_empty() { base.to_string() } else { format!("{base}: {reason}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #654: 失敗理由の併記。**空理由でコロンだけが残らない**ことがこのテストの要点で、
    /// 区切りを `strings.rs` 側に置いた理由そのものである（呼び出し側で整形すると
    /// `view.rs` のインライン処理になり、検知手段が視覚スモークだけになる）。
    #[test]
    fn update_failed_appends_reason_in_both_languages() {
        assert_eq!(update_failed(Language::Ja, ""), "更新に失敗しました");
        assert_eq!(update_failed(Language::En, ""), "Update failed");
        assert_eq!(
            update_failed(Language::Ja, "io error (os error 5)"),
            "更新に失敗しました: io error (os error 5)"
        );
        assert_eq!(update_failed(Language::En, "boom"), "Update failed: boom");
    }

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

    #[test]
    fn hotkey_initial_failed_matches_i18n() {
        // i18n.ts notice.hotkey.initial_failed の値と一字一句一致（2026-07-24 実物確認）。
        // change_failed と違い「他のアプリが使用中の可能性があります」で終わる。
        assert_eq!(
            hotkey_initial_failed(Language::Ja, "Alt+Q"),
            "ホットキー (Alt+Q) の登録に失敗しました。他のアプリが使用中の可能性があります"
        );
        assert_eq!(
            hotkey_initial_failed(Language::En, "Alt+Q"),
            "Failed to register hotkey (Alt+Q). Another application may be using it"
        );
    }
}
