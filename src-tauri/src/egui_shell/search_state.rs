//! egui 検索ウィンドウの純粋状態核（#532 SU3）。query/選択/結果と 2 軸モード導出・遷移を
//! egui/Win32 非依存で持ち、driver（view.rs）から駆動される。ユニットテスト対象。

/// 軸1: モーダルビュースタック頂点の種類。M1 は Results のみ到達（Folder は M2、tool は SU3.5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewKind {
    Results,
    Folder,
}

/// 軸2: 入力の意味。view_kind != Results のときは常に Plain。instant は parse 済みの
/// filter_name（コマンド名）/ instant_query（スペース以降）を持つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryIntent {
    Plain,
    Command,
    Instant {
        filter_name: String,
        instant_query: String,
    },
}

/// instant 判定述語（instant 検出の SSOT）。空 prefix では false（全入力が instant 化するのを防ぐ）。
/// trimStart 規則もここに集約する。
pub fn is_instant_prefix(raw_query: &str, prefix: &str) -> bool {
    !prefix.is_empty() && raw_query.trim_start().starts_with(prefix)
}

/// 入力 → 意図。副作用なし。view_kind != Results は常に Plain（folder 中は非 plain 化しない）。
/// instant の parse（prefix 除去・スペース分割）を一箇所に集約する（DRY）。
pub fn interpret(raw_query: &str, prefix: &str, view_kind: ViewKind) -> QueryIntent {
    if view_kind != ViewKind::Results {
        return QueryIntent::Plain;
    }
    if is_instant_prefix(raw_query, prefix) {
        // starts_with(prefix) を通ったので trimmed[prefix.len()..] は char 境界。
        let input = &raw_query.trim_start()[prefix.len()..];
        match input.find(' ') {
            Some(idx) => QueryIntent::Instant {
                filter_name: input[..idx].to_string(),
                instant_query: input[idx + 1..].to_string(),
            },
            None => QueryIntent::Instant {
                filter_name: input.to_string(),
                instant_query: String::new(),
            },
        }
    } else if raw_query.trim_start().starts_with('/') {
        QueryIntent::Command
    } else {
        QueryIntent::Plain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_query_is_plain() {
        assert_eq!(interpret("firefox", "@", ViewKind::Results), QueryIntent::Plain);
    }

    #[test]
    fn slash_prefix_is_command() {
        assert_eq!(interpret("/r", "@", ViewKind::Results), QueryIntent::Command);
    }

    #[test]
    fn instant_prefix_without_space() {
        assert_eq!(
            interpret("@goog", "@", ViewKind::Results),
            QueryIntent::Instant { filter_name: "goog".into(), instant_query: String::new() }
        );
    }

    #[test]
    fn instant_prefix_with_space_splits_filter_and_query() {
        assert_eq!(
            interpret("@google rust egui", "@", ViewKind::Results),
            QueryIntent::Instant { filter_name: "google".into(), instant_query: "rust egui".into() }
        );
    }

    #[test]
    fn empty_prefix_never_instant() {
        // 空 prefix では全入力が instant 化しない（bootstrap 前など）。
        assert_eq!(interpret("@x", "", ViewKind::Results), QueryIntent::Plain);
        assert!(!is_instant_prefix("@x", ""));
    }

    #[test]
    fn folder_view_is_always_plain() {
        // folder 中は prefix/slash に関わらず plain（フォルダフィルタとして扱う）。
        assert_eq!(interpret("@x", "@", ViewKind::Folder), QueryIntent::Plain);
        assert_eq!(interpret("/r", "@", ViewKind::Folder), QueryIntent::Plain);
    }

    #[test]
    fn leading_whitespace_trimmed_for_detection() {
        assert_eq!(
            interpret("  @a", "@", ViewKind::Results),
            QueryIntent::Instant { filter_name: "a".into(), instant_query: String::new() }
        );
    }
}
