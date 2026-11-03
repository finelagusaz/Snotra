//! フォルダ内エントリの列挙とフィルタ/ソート。
//!
//! ソート順は「`is_folder` 降順 → 展開回数降順 → 小文字名昇順 → path 昇順」で先頭が最優先。
//! path を最終の比較にした全順序なので、入力順に依存しない確定順序になる。
//!
//! 製品の経路は `read_dir_entries`（絞り込みなし）→ [`sort_entries_unlimited`] → 打鍵ごとの
//! [`filter_sorted`] である（#532 SU3 M2 B）。列挙の時点で絞り込んで上位 k 件だけを選ぶ旧経路
//! （`list_folder` / `score_entries`）は製品から呼ばれなくなっていたので撤去した（#1391）。

use nucleo_matcher::Utf32String;
use std::path::Path;

use crate::history::HistoryStore;
use crate::indexer::is_hidden_or_system;
use crate::query::normalize_for_match;
use crate::search::{SearchMode, fuzzy_match_score};
use crate::ui_types::{IconSource, SearchResult};

#[derive(Debug, Clone)]
pub struct DirEntryData {
    path: String,
    name: String,
    is_folder: bool,
}

pub(crate) fn read_dir_entries(
    dir: &Path,
    filter: &str,
    mode: SearchMode,
    show_hidden_system: bool,
) -> std::io::Result<Vec<DirEntryData>> {
    let read_dir = std::fs::read_dir(dir)?;
    let mut entries = Vec::new();
    let filter_norm = normalize_for_match(filter);
    let needle = Utf32String::from(filter_norm.as_ref());

    for entry in read_dir {
        let Ok(entry) = entry else {
            continue;
        };

        let name = entry.file_name().to_string_lossy().to_string();

        if !filter_norm.is_empty() && !matches_filter(&name, &filter_norm, &needle, mode) {
            continue;
        }

        // Windows では DirEntry::metadata() は GetFileAttributesW を呼び出し、
        // シンボリックリンクを辿った先の属性を返す（fs::metadata(path) と同等）。
        // これにより旧実装（is_visible_entry が fs::metadata を別途呼ぶ）と
        // シンボリックリンク挙動は変わらず、かつ追加の stat 呼び出しが不要になる。
        let meta = entry.metadata().ok();

        if !show_hidden_system && meta.as_ref().map(is_hidden_or_system).unwrap_or(false) {
            continue;
        }

        let path = entry.path();
        let is_folder = meta
            .as_ref()
            .map(|meta| meta.is_dir())
            .unwrap_or_else(|| path.is_dir());

        entries.push(DirEntryData {
            path: path.to_string_lossy().to_string(),
            name,
            is_folder,
        });
    }

    Ok(entries)
}

/// フォルダ列挙の全順序比較器。is_folder 降順 → 展開回数降順 → 小文字名昇順 → path 昇順。
/// path を最終 tie-breaker にして全順序化する（同名畳込みの tie が入力順へ漏れるのを防ぐ・#532 SU3 M2）。
fn folder_entry_cmp(
    entries: &[SearchResult],
    a: &(String, u32, usize),
    b: &(String, u32, usize),
) -> std::cmp::Ordering {
    let a_entry = &entries[a.2];
    let b_entry = &entries[b.2];
    b_entry
        .is_folder
        .cmp(&a_entry.is_folder)
        .then_with(|| b.1.cmp(&a.1))
        .then_with(|| a.0.cmp(&b.0))
        .then_with(|| a_entry.path.cmp(&b_entry.path))
}

/// (lower_name, exp_count, index) の並び替えキーを構築する。
fn build_sort_keys(entries: &[SearchResult], history: &HistoryStore) -> Vec<(String, u32, usize)> {
    entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let lower = normalize_for_match(&e.name).into_owned();
            let exp_count = if e.is_folder {
                history.folder_expansion_count(&e.path)
            } else {
                0
            };
            (lower, exp_count, i)
        })
        .collect()
}

/// keyed の順序で entries を並べ替えて所有権を取り出す（clone 回避の mem::replace）。
fn collect_by_keyed(
    mut entries: Vec<SearchResult>,
    keyed: Vec<(String, u32, usize)>,
) -> Vec<SearchResult> {
    keyed
        .into_iter()
        .map(|(_, _, i)| {
            std::mem::replace(
                &mut entries[i],
                SearchResult {
                    name: String::new(),
                    path: String::new(),
                    is_folder: false,
                    is_error: false,
                    icon: IconSource::FromPath,
                },
            )
        })
        .collect()
}

/// フィルタ非依存の確定順序で全件をソートする（上限なし）。egui folder 経路がナビゲーション時に
/// 1 度だけ呼び、driver がキャッシュして打鍵ごとに `filter_sorted` で同期に絞る（#532 SU3 M2 B）。
pub fn sort_entries_unlimited(
    entries: Vec<DirEntryData>,
    history: &HistoryStore,
) -> Vec<SearchResult> {
    let entries: Vec<SearchResult> = entries
        .into_iter()
        .map(|e| SearchResult {
            name: e.name,
            path: e.path,
            is_folder: e.is_folder,
            is_error: false,
            icon: IconSource::FromPath,
        })
        .collect();
    let mut keyed = build_sort_keys(&entries, history);
    keyed.sort_by(|a, b| folder_entry_cmp(&entries, a, b));
    collect_by_keyed(entries, keyed)
}

/// キャッシュ済みソート結果を表示名でフィルタし先頭 max 件を返す（#532 SU3 M2・同期・I/O 無し）。
/// egui folder 経路が打鍵ごとに呼ぶ。`read_dir_entries` のフィルタ段（`matches_filter`）を
/// キャッシュへ再適用する。ソート順はキャッシュ時点で確定済みゆえ再スコア不要(表示名のみ・§6.3)。
pub fn filter_sorted(
    cached: &[SearchResult],
    filter: &str,
    mode: SearchMode,
    max: usize,
) -> Vec<SearchResult> {
    let filter_norm = normalize_for_match(filter);
    if filter_norm.is_empty() {
        return cached.iter().take(max).cloned().collect();
    }
    let needle = Utf32String::from(filter_norm.as_ref());
    cached
        .iter()
        .filter(|r| matches_filter(&r.name, &filter_norm, &needle, mode))
        .take(max)
        .cloned()
        .collect()
}

pub fn error_result(dir: &Path) -> Vec<SearchResult> {
    vec![SearchResult {
        name: String::new(), // 表示文字列はUI層が決める。ロジック層は is_error: true の意味だけを持つ
        path: dir.to_string_lossy().to_string(),
        is_folder: false,
        is_error: true,
        icon: IconSource::FromPath,
    }]
}

/// `filter_norm` は絞り込みを [`normalize_for_match`] へ通したもの、`needle` はその UTF-32 形
/// （`Utf32String::from`・呼び出し 1 回に 1 度）でなければならない。Fuzzy の照合は検索と同じ
/// [`fuzzy_match_score`] を通す（#1276）。
///
/// **名前の側も同じ [`normalize_for_match`] を通す**（`SPEC.md`「4.2 クエリ正規化」・#1284）。片側だけ別の
/// 正規化にすると、連続空白や全角空白を含む名前で一致が割れる。
fn matches_filter(name: &str, filter_norm: &str, needle: &Utf32String, mode: SearchMode) -> bool {
    let name_norm = normalize_for_match(name);
    match mode {
        SearchMode::Prefix => name_norm.starts_with(filter_norm),
        SearchMode::Substring => name_norm.contains(filter_norm),
        SearchMode::Fuzzy => fuzzy_match_score(&name_norm, needle.slice(..)).is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::HistoryStore;
    use std::fs;
    use std::path::PathBuf;

    /// テスト用の作業ディレクトリを作り直して返す。
    ///
    /// 名前に `std::process::id()` を含める理由は `indexer.rs` の `temp_dir` の doc を正本とする（#978 / #985）。
    ///
    /// prefix は他モジュールと同じ `snotra_<module>_test_` 形に揃えてある——かつての `snotra_test_` はこの形が定まる前の名残であり、意図的な区別ではない（#985）。
    fn temp_dir_with_contents(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("snotra_folder_test_{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn temp_dir_name_contains_process_id() {
        let dir = temp_dir_with_contents("process_unique");
        let name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .expect("temp dir name");
        assert_eq!(
            name,
            format!("snotra_folder_test_process_unique-{}", std::process::id()),
            "作業ディレクトリ名に自プロセスの pid が入っていない（#985）"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    fn empty_history() -> HistoryStore {
        HistoryStore::empty()
    }

    /// 製品の経路をそのまま組む: 絞り込みなしで列挙（`folder_nav`）→ 全件ソート → 打鍵の絞り込み
    /// （`search_flow` の `filter_sorted`）。列挙に失敗したら `error_result` を返す点も製品と同じ。
    fn list_like_product(
        dir: &Path,
        filter: &str,
        mode: SearchMode,
        show_hidden_system: bool,
        history: &HistoryStore,
        max: usize,
    ) -> Vec<SearchResult> {
        match read_dir_entries(dir, "", mode, show_hidden_system) {
            Ok(entries) => {
                filter_sorted(&sort_entries_unlimited(entries, history), filter, mode, max)
            }
            Err(_) => error_result(dir),
        }
    }

    /// `empty_history()` は名前どおり空でなければならない（#963）。
    /// **CI では常に緑で開発機でだけ落ちる**性質は `engine.rs` の同名テストの doc を参照。
    #[test]
    fn empty_history_fixture_is_actually_empty() {
        assert!(
            empty_history().recent_launches(usize::MAX).is_empty(),
            "empty_history() が空でない — 実 history.bin を読んでいる"
        );
    }

    #[test]
    fn folder_list_returns_files_and_dirs() {
        let dir = temp_dir_with_contents("basic");
        fs::write(dir.join("file1.txt"), "").unwrap();
        fs::write(dir.join("file2.txt"), "").unwrap();
        fs::create_dir(dir.join("subdir")).unwrap();

        let results =
            list_like_product(&dir, "", SearchMode::Substring, true, &empty_history(), 100);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"file1.txt"));
        assert!(names.contains(&"file2.txt"));
        assert!(names.contains(&"subdir"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn folder_list_folders_come_before_files() {
        let dir = temp_dir_with_contents("order");
        fs::write(dir.join("alpha.txt"), "").unwrap();
        fs::create_dir(dir.join("zsubdir")).unwrap();

        let results =
            list_like_product(&dir, "", SearchMode::Substring, true, &empty_history(), 100);
        assert!(results[0].is_folder);
        assert!(!results.last().unwrap().is_folder);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn folder_list_filter_excludes_non_matching() {
        let dir = temp_dir_with_contents("filter");
        fs::write(dir.join("readme.txt"), "").unwrap();
        fs::write(dir.join("config.toml"), "").unwrap();
        fs::write(dir.join("build.rs"), "").unwrap();

        let results = list_like_product(
            &dir,
            "toml",
            SearchMode::Substring,
            true,
            &empty_history(),
            100,
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "config.toml");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn folder_list_filter_is_case_insensitive() {
        let dir = temp_dir_with_contents("filter_case");
        fs::write(dir.join("README.TXT"), "").unwrap();

        let results = list_like_product(
            &dir,
            "readme",
            SearchMode::Substring,
            true,
            &empty_history(),
            100,
        );
        assert_eq!(results.len(), 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn folder_list_respects_max_results() {
        let dir = temp_dir_with_contents("maxresults");
        for i in 0..10 {
            fs::write(dir.join(format!("file{}.txt", i)), "").unwrap();
        }

        let results = list_like_product(&dir, "", SearchMode::Substring, true, &empty_history(), 3);
        assert_eq!(results.len(), 3);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn folder_list_empty_dir_returns_empty() {
        let dir = temp_dir_with_contents("empty");

        let results =
            list_like_product(&dir, "", SearchMode::Substring, true, &empty_history(), 100);
        assert!(results.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn folder_list_nonexistent_dir_returns_empty() {
        // 名前へ pid を含めるのは、このテストが「存在しないこと」を共有 `%TEMP%` 上の固定名に期待しているからである（誰かが同名を作れば落ちる・#985）。
        let dir = std::env::temp_dir().join(format!(
            "snotra_folder_test_nonexistent_zzz-{}",
            std::process::id()
        ));
        let results =
            list_like_product(&dir, "", SearchMode::Substring, true, &empty_history(), 100);
        assert_eq!(results.len(), 1);
        assert!(results[0].is_error);
        assert_eq!(results[0].name, ""); // UI 文字列はロジック層に持たない
    }

    #[test]
    fn folders_sorted_alphabetically_when_equal_expansion_count() {
        let dir = temp_dir_with_contents("alpha_dirs");
        fs::create_dir(dir.join("zeta")).unwrap();
        fs::create_dir(dir.join("alpha")).unwrap();
        fs::create_dir(dir.join("mu")).unwrap();

        let results =
            list_like_product(&dir, "", SearchMode::Substring, true, &empty_history(), 100);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "mu", "zeta"]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn prefix_mode_matches_only_prefix() {
        let dir = temp_dir_with_contents("prefix_filter");
        fs::write(dir.join("report.txt"), "").unwrap();
        fs::write(dir.join("my_report.txt"), "").unwrap();

        let results =
            list_like_product(&dir, "rep", SearchMode::Prefix, true, &empty_history(), 100);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"report.txt"));
        assert!(!names.contains(&"my_report.txt"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn fuzzy_mode_matches_skipped_characters() {
        let dir = temp_dir_with_contents("fuzzy_filter");
        fs::write(dir.join("Visual Studio Code.txt"), "").unwrap();

        let results =
            list_like_product(&dir, "vsc", SearchMode::Fuzzy, true, &empty_history(), 100);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Visual Studio Code.txt");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn substring_mode_does_not_match_skipped_characters() {
        let dir = temp_dir_with_contents("substring_not_fuzzy");
        fs::write(dir.join("Visual Studio Code.txt"), "").unwrap();

        let results = list_like_product(
            &dir,
            "vsc",
            SearchMode::Substring,
            true,
            &empty_history(),
            100,
        );
        assert!(results.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn hidden_files_excluded_when_show_hidden_system_false() {
        let dir = temp_dir_with_contents("hidden_filter");
        fs::write(dir.join("visible.txt"), "").unwrap();
        let hidden_path = dir.join("hidden.txt");
        fs::write(&hidden_path, "").unwrap();
        std::process::Command::new("attrib")
            .args(["+H", hidden_path.to_str().unwrap()])
            .status()
            .expect("attrib command failed");

        let results = list_like_product(
            &dir,
            "",
            SearchMode::Substring,
            false,
            &empty_history(),
            100,
        );
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(results.len(), 1);
        assert!(names.contains(&"visible.txt"));
        assert!(!names.contains(&"hidden.txt"));

        let _ = fs::remove_dir_all(&dir);
    }

    // --- 並び順のテスト ---

    fn make_file(name: &str) -> DirEntryData {
        DirEntryData {
            name: name.to_string(),
            path: format!("C:\\test\\{}", name),
            is_folder: false,
        }
    }

    #[test]
    fn folder_cmp_is_total_order_by_path() {
        // "Café" と "Cafe" は normalize_for_match で同キー化する（アクセント畳込み）。
        // path tie-breaker が無いと、同キーの 2 件の順が read_dir 順へ漏れる。
        // empty_history() は既存 folder テストのヘルパー（`HistoryStore::empty()`）。
        let hist = empty_history();
        let entries = vec![
            DirEntryData {
                path: "C:\\d\\Cafe".into(),
                name: "Cafe".into(),
                is_folder: false,
            },
            DirEntryData {
                path: "C:\\d\\Café".into(),
                name: "Café".into(),
                is_folder: false,
            },
        ];
        let a = sort_entries_unlimited(entries.clone(), &hist);
        // 入力順を反転しても同一順序（path 昇順で確定）
        let mut rev = entries;
        rev.reverse();
        let b = sort_entries_unlimited(rev, &hist);
        assert_eq!(
            a.iter().map(|r| r.path.clone()).collect::<Vec<_>>(),
            b.iter().map(|r| r.path.clone()).collect::<Vec<_>>()
        );
        // path 昇順: "Cafe" < "Café"（バイト比較）
        assert_eq!(a[0].path, "C:\\d\\Cafe");
    }

    /// **並べ替えのキーも照合の入口を通す**（#1309）。前に空白を持つ名前は、その空白を除いた
    /// 名前の位置に並ぶ。キーだけを別の写像（空白を触らない小文字化と折りたたみ）へ戻すと、
    /// `' '` が英字より前に来て ` b` が `a` より先になる。
    #[test]
    fn sort_key_trims_leading_whitespace_like_the_filter() {
        let sorted =
            sort_entries_unlimited(vec![make_file(" b"), make_file("a")], &empty_history());
        assert_eq!(
            sorted.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["a", " b"]
        );
    }

    #[test]
    fn filter_sorted_empty_returns_prefix_up_to_max() {
        use crate::search::SearchMode;
        let cached = vec![sr("a"), sr("b"), sr("c")];
        let out = filter_sorted(&cached, "", SearchMode::Substring, 2);
        assert_eq!(
            out.iter().map(|r| r.name.clone()).collect::<Vec<_>>(),
            vec!["a", "b"]
        );
    }

    #[test]
    fn filter_sorted_matches_display_name_not_path() {
        use crate::search::SearchMode;
        // path に "zzz" を含むが name には含まない → filter "zzz" は 0 件（表示名のみ・§6.3）
        let cached = vec![SearchResult {
            name: "report".into(),
            path: "C:\\zzz\\report".into(),
            is_folder: false,
            is_error: false,
            icon: IconSource::FromPath,
        }];
        let out = filter_sorted(&cached, "zzz", SearchMode::Substring, 8);
        assert!(out.is_empty());
    }

    // テスト用ヘルパー（既存 tests mod にあれば再利用）
    #[cfg(test)]
    fn sr(name: &str) -> SearchResult {
        SearchResult {
            name: name.into(),
            path: format!("C:\\d\\{name}"),
            is_folder: false,
            is_error: false,
            icon: IconSource::FromPath,
        }
    }

    /// 絞り込みの Fuzzy は、needle に残った結合文字を検索と同じく書記素の先頭へ落として照合する（#1276）。
    ///
    /// 回帰テスト: `Utf32Str::new` へ戻すと needle が `Ascii` の生バイト（`x` + U+20DD の `0xE2 0x83 0x9D`）の
    /// まま照合され、どちらの行も不一致になる。`日本語` の行は needle の組み方だけに、`Cafe` の行は haystack と
    /// needle の両方の組み方に依る（haystack が `Ascii` の生バイトのままだと `(Ascii, Unicode)` の組になり
    /// 外れる）。拡張子は `.md` にしてある——`.txt` の `x` が 1 行目の needle に一致してしまうため。
    ///
    /// **結合文字は合成しないもの（U+20DD）を使う。** 合成できる結合文字は、照合の入口の合成（NFC・#1285）で
    /// 前の字と 1 字になってから ASCII へ畳まれる。needle も haystack も ASCII になり、書記素の扱いを測れない。
    #[test]
    fn fuzzy_filter_drops_combining_marks_in_needle_like_search() {
        use crate::search::SearchMode;
        let cached = vec![sr("日本語 x.md"), sr("Cafe\u{20dd}.md")];
        // 2 行の結果を集めてから 1 度だけ比べる——行ごとに assert すると、最初に落ちた行より後ろが報告から消える。
        let got: Vec<(&str, Vec<String>)> = ["x\u{20dd}", "e\u{20dd}"]
            .into_iter()
            .map(|filter| {
                let names = filter_sorted(&cached, filter, SearchMode::Fuzzy, 8)
                    .into_iter()
                    .map(|r| r.name)
                    .collect();
                (filter, names)
            })
            .collect();
        assert_eq!(
            got,
            [
                ("x\u{20dd}", vec!["日本語 x.md".to_string()]),
                ("e\u{20dd}", vec!["Cafe\u{20dd}.md".to_string()]),
            ]
        );
    }

    /// 純 ASCII の名前は、非 ASCII の needle に一致しない。`nucleo-matcher` 0.3.1 の `fuzzy_match` が
    /// `(Ascii, Unicode)` の組に無条件で `None` を返すためであり、検索側も同じである。**版を上げてこれが
    /// 赤になったら、まず上流の側が変わったかを見る。** needle の結合文字に U+20DD を使う理由は
    /// `fuzzy_filter_drops_combining_marks_in_needle_like_search` と同じである。
    #[test]
    fn fuzzy_filter_ascii_name_never_matches_non_ascii_needle() {
        use crate::search::SearchMode;
        let out = filter_sorted(&[sr("resume")], "e\u{20dd}", SearchMode::Fuzzy, 8);
        assert!(out.is_empty());
    }

    /// 旧実装が両辺を `Ascii` の生バイトとして突き合わせて得ていた一致を、修正後も保つ（#1276）。
    ///
    /// 回帰テスト: haystack だけを `Utf32String::from` の形へ替え、needle を `Utf32Str::new` のまま残すと、
    /// `(Unicode, Ascii)` の組になってこの一致が落ちる——片側だけ替える退行を検出する。
    #[test]
    fn fuzzy_filter_keeps_old_byte_level_matches() {
        use crate::search::SearchMode;
        let out = filter_sorted(
            &[sr("Re\u{301}sume\u{301}.docx")],
            "e\u{301}",
            SearchMode::Fuzzy,
            8,
        );
        assert_eq!(out.len(), 1);
    }

    /// **NFD で保存された名前に、NFC の絞り込みが当たる**（`SPEC.md`「4.2 クエリ正規化」・#1285）。
    /// macOS 由来のファイル名は NFD でありうる。合成しなかった頃は、#1276 の修正の前後どちらでも外れた。
    #[test]
    fn filter_matches_nfd_name_with_nfc_filter() {
        use crate::search::SearchMode;
        let cached = [sr("か\u{3099}いど.txt"), sr("かいど.txt")];
        for mode in [SearchMode::Prefix, SearchMode::Substring, SearchMode::Fuzzy] {
            let out: Vec<String> = filter_sorted(&cached, "が", mode, 8)
                .into_iter()
                .map(|r| r.name)
                .collect();
            assert_eq!(out, ["か\u{3099}いど.txt"], "{mode:?}");
        }
    }

    /// **照合器は名前の側に自前の折りたたみを当てない**（#1395）。名前も絞り込みも照合の入口で畳み済みである。
    /// nucleo-matcher の `normalize` を有効にしたままだと、照合器が名前の側にだけ 0.3.1 の壊れた表を当て
    /// （`ᴀ` → `A`）、畳まれていない絞り込みの `ᴀ` と食い違って外れた。
    #[test]
    fn fuzzy_filter_does_not_refold_the_name_inside_the_matcher() {
        use crate::search::SearchMode;
        let out = filter_sorted(&[sr("ᴀbc.txt")], "ᴀ", SearchMode::Fuzzy, 8);
        assert_eq!(out.len(), 1);
    }

    /// 絞り込みと名前の両辺に `SPEC.md`「4.2 クエリ正規化」の空白の正規化を通す（#1284）。
    ///
    /// 名前の側には Windows のファイル名に置ける空白（連続空白・U+3000・NBSP）を使う。返る名前は元の綴りの
    /// まま（期待値を元の綴りで書くことがその検査を兼ねる）。**空白だけの絞り込みは空と同じく全件である。**
    ///
    /// 回帰テスト: 絞り込み側と名前側のどちらか片方だけを、空白を正規化しない写像（小文字化と折りたたみだけ）へ戻しても落ちる。
    #[test]
    fn filter_normalizes_whitespace_on_both_sides_in_every_mode() {
        use crate::search::SearchMode;
        let names = [
            "report.txt",
            "a b.txt",
            "a  b.txt",
            "a\u{3000}b.txt",
            "a\u{a0}b.txt",
            "zzz.md",
        ];
        let cached: Vec<SearchResult> = names.iter().map(|n| sr(n)).collect();
        let a_b = vec![names[1], names[2], names[3], names[4]];
        let cases: [(&str, Vec<&str>); 5] = [
            ("report ", vec![names[0]]),
            (" report", vec![names[0]]),
            ("a  b", a_b.clone()),
            ("a b", a_b),
            ("   ", names.to_vec()),
        ];
        // 全組を集めてから 1 度だけ比べる——組ごとに assert すると、最初に落ちた組より後ろが報告から消える。
        let mut got = Vec::new();
        let mut want = Vec::new();
        for mode in [SearchMode::Prefix, SearchMode::Substring, SearchMode::Fuzzy] {
            for (filter, expected) in &cases {
                let out: Vec<String> = filter_sorted(&cached, filter, mode, 8)
                    .into_iter()
                    .map(|r| r.name)
                    .collect();
                got.push((mode, *filter, out));
                want.push((
                    mode,
                    *filter,
                    expected.iter().map(|s| s.to_string()).collect(),
                ));
            }
        }
        assert_eq!(got, want);
    }

    /// `read_dir_entries` も同じ [`matches_filter`] を通るので、絞り込み側の正規化がこの経路でも揃うことを
    /// 固定する（#1284）。
    #[test]
    fn read_dir_entries_filter_trims_whitespace() {
        let dir = temp_dir_with_contents("filter_trim");
        fs::write(dir.join("report.txt"), "").unwrap();
        fs::write(dir.join("other.md"), "").unwrap();

        let entries = read_dir_entries(&dir, "report ", SearchMode::Fuzzy, true).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["report.txt"]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn expansion_count_prioritizes_frequently_opened_folder() {
        // expansion_count が高いフォルダは同一グループ内で先頭に来る
        let mut history = empty_history();
        history.record_folder_expansion("C:\\test\\often");
        history.record_folder_expansion("C:\\test\\often");
        history.record_folder_expansion("C:\\test\\often");
        history.record_folder_expansion("C:\\test\\rarely");

        let entries = vec![
            DirEntryData {
                name: "rarely".to_string(),
                path: "C:\\test\\rarely".to_string(),
                is_folder: true,
            },
            DirEntryData {
                name: "often".to_string(),
                path: "C:\\test\\often".to_string(),
                is_folder: true,
            },
            DirEntryData {
                name: "never".to_string(),
                path: "C:\\test\\never".to_string(),
                is_folder: true,
            },
        ];
        // often(3回) > rarely(1回) > never(0回) の順に並ぶ
        let results = sort_entries_unlimited(entries, &history);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["often", "rarely", "never"]);
    }
}
