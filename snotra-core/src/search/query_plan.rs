//! 検索クエリ計画（`QueryPlan` と `prepare_query_plan`）を search.rs から分離（#599）。
//!
//! 1 回の検索呼び出しについて、正規化クエリ・dot/path 判定・Fuzzy bitmask・migemo かな
//! クエリ・UTF-32 needle・パス照合クエリ・履歴キーを**純粋に**導出する（エントリ走査も
//! 履歴更新も行わない自由関数）。正本は `crate::query` の正規化群であり、ここは検索固有の
//! 組み立て責務のみを持つ。incremental 判定と前回状態の read/write は親 `search.rs` の
//! `IncrementalCache`（`can_reuse` / `update`・#601）。

use std::borrow::Cow;

use nucleo_matcher::Utf32String;

use crate::query::{char_bitmask, normalize_history_query_key, normalize_query, to_kana};

use super::{SearchMode, SearchOptions, kana_char_mask};

/// 1 回の検索呼び出しでクエリから 1 度だけ導出する、スコアリング共有データ。
/// `search_with_options` の候補準備フェーズ（`prepare_query_plan`）が構築し、
/// `IncrementalCache::can_reuse` と `score_one_entry` が `&QueryPlan` で共有する。
/// `norm_query` のみ入力 `query` を借用しうる（ASCII 小文字クエリはゼロアロケーション）。
/// `needle_u32` は `norm_query` から生成後は独立（借用しない）。
pub(super) struct QueryPlan<'a> {
    /// 正規化済みクエリ（アクセント折畳み・連続スペース圧縮・小文字化）。
    pub(super) norm_query: Cow<'a, str>,
    /// `norm_query` が `.` を含むか（file_name スコアリングと incremental ガードに連動）。
    pub(super) has_dot: bool,
    /// 生クエリがパス区切り（`\` `/` `¥`）を含むか。
    pub(super) has_path_sep: bool,
    /// Fuzzy モードのビットマスク pre-filter 用クエリマスク（非 Fuzzy では 0）。
    pub(super) query_mask: u64,
    /// migemo 用ひらがな変換クエリ（ASCII 残留や条件未達のとき `None`）。
    pub(super) kana_query: Option<String>,
    /// kana_query 用の損失あり文字存在マスク。kana_query がないときは `None`。
    pub(super) kana_query_mask: Option<u64>,
    /// `norm_query` の UTF-32 事前計算（Fuzzy マッチで全スレッド共有）。
    pub(super) needle_u32: Utf32String,
    /// パスマッチ用クエリ（生クエリベース・アクセント/連続スペース保持）。`has_path_sep` 時のみ。
    pub(super) path_query: Option<String>,
    /// パスクエリの履歴照合キー（`normalize_history_query_key` 由来）。`path_query` とは別。
    pub(super) path_history_key: Option<String>,
}

/// 検索呼び出しの候補準備フェーズ。クエリを解析して [`QueryPlan`] を組み立てる。
/// `norm_query` が空のとき `None`（呼び出し側は空結果を返す）。`self` 非依存の自由 fn。
pub(super) fn prepare_query_plan<'a>(
    query: &'a str,
    mode: SearchMode,
    options: &SearchOptions,
) -> Option<QueryPlan<'a>> {
    let norm_query = normalize_query(query);
    if norm_query.is_empty() {
        return None;
    }

    let has_dot = norm_query.contains('.');
    // Bitmask pre-filter is only used in Fuzzy mode; skip the computation for others.
    let query_mask = if mode == SearchMode::Fuzzy {
        char_bitmask(&norm_query)
    } else {
        0
    };

    // Migemo: ローマ字 ASCII クエリをひらがなに変換した kana_query を生成する。
    // kana に ASCII アルファベットが残留する場合（"dok" → "どk" 等）は None にする。
    let kana_query: Option<String> = if options.migemo_enabled
        && norm_query.is_ascii()
        && norm_query.chars().count() >= options.migemo_min_chars
    {
        let k = to_kana(norm_query.as_ref());
        if k != norm_query.as_ref() && !k.bytes().any(|b| b.is_ascii_alphabetic()) {
            Some(k)
        } else {
            None
        }
    } else {
        None
    };
    let kana_query_mask = kana_query.as_deref().map(kana_char_mask);

    // Pre-compute needle as UTF-32 once per search call and share it across threads.
    // Reusing the same Utf32String avoids repeated O(|query|) char conversion per entry.
    let needle_u32 = Utf32String::from(norm_query.as_ref());

    // Path matching: クエリにパス区切り文字（\ / ¥）を含む場合、normalized_key（フルパス）
    // に対して Substring マッチを試みる。
    // normalize_query() は連続スペースを潰すが normalize_entry_key() は保持するため、
    // パスマッチ用クエリは生クエリから normalize_entry_key() 相当で正規化する。
    // これにより "C:\My  Tools\" のような連続スペースを含むパスにもマッチする。
    // ¥（U+00A5）は日本語 Windows でバックスラッシュとして使われるため対象に含める。
    let has_path_sep = {
        let q = query.trim();
        q.contains('\\') || q.contains('/') || q.contains('\u{00a5}')
    };
    let path_query: Option<String> = if has_path_sep {
        // normalize_entry_key と同じ正規化: 小文字化 + / と ¥ を \ に統一
        let trimmed = query.trim();
        let mut pq = String::with_capacity(trimmed.len());
        for ch in trimmed.chars() {
            if ch == '/' || ch == '\u{00a5}' {
                pq.push('\\');
            } else {
                pq.extend(ch.to_lowercase());
            }
        }
        Some(pq)
    } else {
        None
    };
    // 履歴キー: normalize_history_query_key で一元化（normalize_query + パス区切り統一）。
    // path_query は生クエリベースでスペース/アクセントの扱いが異なるため別途作る。
    let path_history_key: Option<String> = if has_path_sep {
        Some(normalize_history_query_key(query).into_owned())
    } else {
        None
    };

    Some(QueryPlan {
        norm_query,
        has_dot,
        has_path_sep,
        query_mask,
        kana_query,
        kana_query_mask,
        needle_u32,
        path_query,
        path_history_key,
    })
}
