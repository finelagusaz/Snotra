//! egui 検索ウィンドウの純粋状態核（#532 SU3）。query/選択/結果と 2 軸モード導出・遷移を
//! egui/Win32 非依存で持ち、driver（view.rs）から駆動される。ユニットテスト対象。

use snotra_core::ui_types::SearchResult;

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

/// フォルダ展開モードの退避/復元単位（`Option<FolderFrame>`・#532 SU3 M2）。深掘りは push でなく
/// current_dir 書き換え。フォルダ内フィルタは frame でなく SearchState.folder_filter が持つ。
// TODO(SU3 M2 Task 3): view.rs が消費したら除去
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct FolderFrame {
    pub restore_query: String,
    pub restore_results: Vec<SearchResult>,
    pub restore_selected: usize,
    pub current_dir: String,
}

/// Escape ラダーの分岐（driver が side-effect を実行）。M2 は folder 段と top-level のみ
/// （instant/command 解除段は M3）。
// TODO(SU3 M2 Task 3): view.rs が消費したら除去
#[allow(dead_code)]
#[derive(Debug, PartialEq, Eq)]
pub enum EscapeOutcome {
    /// folder → 展開前検索状態へ復帰済み（driver は追加操作なし）
    RestoredSearch,
    /// top-level → hide 要求（driver が emit）
    Hide,
}

/// 検索ウィンドウの純粋状態。M1 は results 軸のみ（folder stack は M2）。
pub struct SearchState {
    query: String,
    results: Vec<SearchResult>,
    selected: usize,
    folder: Option<FolderFrame>,
    folder_filter: String,
    folder_gen: u64,
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            results: Vec::new(),
            selected: 0,
            folder: None,
            folder_filter: String::new(),
            folder_gen: 0,
        }
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn set_query(&mut self, q: String) {
        self.query = q;
    }

    pub fn results(&self) -> &[SearchResult] {
        &self.results
    }

    /// 結果を差し替える。選択は範囲内へクランプ（空なら 0）。
    pub fn set_results(&mut self, results: Vec<SearchResult>) {
        self.results = results;
        self.selected = clamp_selected(self.results.len(), self.selected);
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    /// 軸1。folder モードなら Folder、それ以外 Results（tool は SU3.5）。
    pub fn view_kind(&self) -> ViewKind {
        if self.folder.is_some() { ViewKind::Folder } else { ViewKind::Results }
    }

    /// 軸2。prefix は driver が config live-read で渡す。
    pub fn interp(&self, prefix: &str) -> QueryIntent {
        interpret(&self.query, prefix, self.view_kind())
    }

    /// ↑↓ ナビ。delta 移動して結果範囲へクランプ（空なら 0 のまま・端で飽和）。
    pub fn move_selection(&mut self, delta: i32) {
        if self.results.is_empty() {
            self.selected = 0;
            return;
        }
        let max = self.results.len() as i32 - 1;
        let next = (self.selected as i32 + delta).clamp(0, max);
        self.selected = next as usize;
    }

    /// 通常検索 → folder 突入。展開前状態を frame に退避し gen を進める。token を返す。
    // TODO(SU3 M2 Task 3): view.rs が消費したら除去
    #[allow(dead_code)]
    pub fn enter_folder(&mut self, dir: String) -> u64 {
        self.folder = Some(FolderFrame {
            restore_query: self.query.clone(),
            restore_results: self.results.clone(),
            restore_selected: self.selected,
            current_dir: dir,
        });
        self.folder_filter.clear();
        self.selected = 0;
        self.folder_gen += 1;
        self.folder_gen
    }

    /// folder 内で親/子へ遷移（frame の current_dir を書き換え・push しない）。token を返す。
    // TODO(SU3 M2 Task 3): view.rs が消費したら除去
    #[allow(dead_code)]
    pub fn navigate_folder(&mut self, dir: String) -> u64 {
        if let Some(f) = self.folder.as_mut() {
            f.current_dir = dir;
        }
        self.folder_filter.clear();
        self.selected = 0;
        self.folder_gen += 1;
        self.folder_gen
    }

    // TODO(SU3 M2 Task 3): view.rs が消費したら除去
    #[allow(dead_code)]
    pub fn folder_current_dir(&self) -> Option<&str> {
        self.folder.as_ref().map(|f| f.current_dir.as_str())
    }

    /// folder 中の親ディレクトリ（ルート終端で None）。
    // TODO(SU3 M2 Task 3): view.rs が消費したら除去
    #[allow(dead_code)]
    pub fn parent_dir(&self) -> Option<String> {
        self.folder.as_ref().and_then(|f| compute_parent_dir(&f.current_dir))
    }

    // TODO(SU3 M2 Task 3): view.rs が消費したら除去
    #[allow(dead_code)]
    pub fn folder_gen(&self) -> u64 {
        self.folder_gen
    }

    /// 遅延到着したナビ結果を受理してよいか（token 一致 ∧ folder 中）。driver が false なら破棄。
    // TODO(SU3 M2 Task 3): view.rs が消費したら除去
    #[allow(dead_code)]
    pub fn accept_folder_result(&self, token: u64) -> bool {
        token == self.folder_gen && self.folder.is_some()
    }

    // TODO(SU3 M2 Task 3): view.rs が消費したら除去
    #[allow(dead_code)]
    pub fn folder_filter(&self) -> &str {
        &self.folder_filter
    }

    // TODO(SU3 M2 Task 3): view.rs が消費したら除去
    #[allow(dead_code)]
    pub fn set_folder_filter(&mut self, f: String) {
        self.folder_filter = f;
        self.selected = 0;
    }

    /// Escape ラダー（M2）: folder 中は展開前状態へ復帰、top-level は Hide。
    /// folder 離脱時は folder_gen を進めて遅延到着した旧ナビ結果を無効化する。
    // TODO(SU3 M2 Task 3): view.rs が消費したら除去
    #[allow(dead_code)]
    pub fn on_escape(&mut self) -> EscapeOutcome {
        if let Some(f) = self.folder.take() {
            self.query = f.restore_query;
            self.results = f.restore_results;
            self.selected = clamp_selected(self.results.len(), f.restore_selected);
            self.folder_filter.clear();
            self.folder_gen += 1; // 離脱経路でも失効
            EscapeOutcome::RestoredSearch
        } else {
            EscapeOutcome::Hide
        }
    }

    /// resetForShow 相当。show のたびに driver が呼ぶ。folder モードも解除し gen を進める
    /// （hide 前の in-flight ナビ結果を再表示後に轢かせない）。
    pub fn reset(&mut self) {
        self.query.clear();
        self.results.clear();
        self.selected = 0;
        self.folder = None;
        self.folder_filter.clear();
        self.folder_gen += 1;
    }
}

impl Default for SearchState {
    fn default() -> Self {
        Self::new()
    }
}

/// 選択 index を [0, len) へクランプ（len==0 は 0）。folderNav.clampSelectedIndex 相当。
pub(crate) fn clamp_selected(len: usize, idx: usize) -> usize {
    if len == 0 {
        0
    } else {
        idx.min(len - 1)
    }
}

/// 親ディレクトリを返す。ルート（`C:\` / `\\server\share\`）で None。folderNav.computeParentDir 相当。
// TODO(SU3 M2 Task 3): view.rs が消費したら除去
#[allow(dead_code)]
pub(crate) fn compute_parent_dir(current_dir: &str) -> Option<String> {
    // 末尾 `\` を剥がす（ただしドライブルート "X:\" は保持しない — 後段で判定）。
    let normalized = if current_dir.len() > 3 && current_dir.ends_with('\\') {
        &current_dir[..current_dir.len() - 1]
    } else {
        current_dir
    };
    // UNC ルート判定: \\server\share（2 セグメント以下）は終端。
    if let Some(rest) = normalized.strip_prefix("\\\\") {
        let parts: Vec<&str> = rest.trim_end_matches('\\').split('\\').filter(|p| !p.is_empty()).collect();
        if parts.len() <= 2 {
            return None;
        }
    }
    // 最後の `\segment` を削る。
    let cut = normalized.rfind('\\')?;
    let mut parent = normalized[..cut].to_string();
    // "X:" → "X:\"（ドライブルートは末尾 `\` 必須）。
    if parent.len() == 2 && parent.as_bytes()[1] == b':' && parent.as_bytes()[0].is_ascii_alphabetic() {
        parent.push('\\');
    }
    if parent.is_empty() || parent == normalized {
        return None;
    }
    // \\server（share 未満）は終端。
    if let Some(rest) = parent.strip_prefix("\\\\") {
        let parts: Vec<&str> = rest.trim_end_matches('\\').split('\\').filter(|p| !p.is_empty()).collect();
        if parts.len() < 2 {
            return None;
        }
    }
    Some(parent)
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

    fn res(name: &str) -> SearchResult {
        SearchResult { name: name.into(), path: format!("C:/{name}.exe"), is_folder: false, is_error: false }
    }

    #[test]
    fn set_results_clamps_selection() {
        let mut s = SearchState::new();
        s.set_results(vec![res("a"), res("b"), res("c")]);
        s.move_selection(2); // selected = 2
        assert_eq!(s.selected(), 2);
        s.set_results(vec![res("a")]); // 縮小 → クランプ
        assert_eq!(s.selected(), 0);
    }

    #[test]
    fn move_selection_saturates_at_bounds() {
        let mut s = SearchState::new();
        s.set_results(vec![res("a"), res("b")]);
        s.move_selection(-1); // 端で 0 飽和
        assert_eq!(s.selected(), 0);
        s.move_selection(1);
        assert_eq!(s.selected(), 1);
        s.move_selection(1); // 端で 1 飽和
        assert_eq!(s.selected(), 1);
    }

    #[test]
    fn move_selection_on_empty_is_zero() {
        let mut s = SearchState::new();
        s.move_selection(1);
        assert_eq!(s.selected(), 0);
    }

    #[test]
    fn reset_clears_everything() {
        let mut s = SearchState::new();
        s.set_query("abc".into());
        s.set_results(vec![res("a"), res("b")]);
        s.move_selection(1);
        s.reset();
        assert_eq!(s.query(), "");
        assert!(s.results().is_empty());
        assert_eq!(s.selected(), 0);
    }

    #[test]
    fn interp_reads_current_query() {
        let mut s = SearchState::new();
        s.set_query("@g".into());
        assert_eq!(s.interp("@"), QueryIntent::Instant { filter_name: "g".into(), instant_query: String::new() });
    }

    #[test]
    fn parent_dir_drive_and_unc_roots() {
        assert_eq!(compute_parent_dir("C:\\a\\b"), Some("C:\\a".to_string()));
        assert_eq!(compute_parent_dir("C:\\a"), Some("C:\\".to_string()));
        assert_eq!(compute_parent_dir("C:\\"), None); // ドライブルート終端
        assert_eq!(compute_parent_dir("\\\\srv\\share\\x"), Some("\\\\srv\\share".to_string()));
        assert_eq!(compute_parent_dir("\\\\srv\\share"), None); // UNC 共有ルート終端
        assert_eq!(compute_parent_dir("\\\\srv"), None); // UNC 不完全
    }

    #[test]
    fn enter_folder_saves_view_and_switches_kind() {
        let mut s = SearchState::new();
        s.set_query("fire".into());
        s.set_results(vec![res("a"), res("b")]);
        s.move_selection(1); // selected=1
        assert_eq!(s.view_kind(), ViewKind::Results);
        let tok = s.enter_folder("C:\\proj".into());
        assert_eq!(s.view_kind(), ViewKind::Folder);
        assert_eq!(s.folder_current_dir(), Some("C:\\proj"));
        assert_eq!(s.folder_filter(), "");
        assert_eq!(s.folder_gen(), tok);
        // query は展開前語を保持（相乗りしない）
        assert_eq!(s.query(), "fire");
    }

    #[test]
    fn navigate_folder_bumps_gen_and_clears_filter() {
        let mut s = SearchState::new();
        let t1 = s.enter_folder("C:\\a".into());
        s.set_folder_filter("x".into());
        let t2 = s.navigate_folder("C:\\a\\b".into());
        assert!(t2 > t1);
        assert_eq!(s.folder_current_dir(), Some("C:\\a\\b"));
        assert_eq!(s.folder_filter(), "");
    }

    #[test]
    fn escape_folder_restores_then_hides() {
        let mut s = SearchState::new();
        s.set_query("fire".into());
        s.set_results(vec![res("a"), res("b"), res("c")]);
        s.move_selection(2); // selected=2
        s.enter_folder("C:\\proj".into());
        s.set_folder_filter("x".into());
        // 1 回目の Escape → 展開前状態へ復帰
        assert_eq!(s.on_escape(), EscapeOutcome::RestoredSearch);
        assert_eq!(s.view_kind(), ViewKind::Results);
        assert_eq!(s.query(), "fire");
        assert_eq!(s.results().len(), 3);
        assert_eq!(s.selected(), 2);
        // 2 回目の Escape（results + plain）→ hide
        assert_eq!(s.on_escape(), EscapeOutcome::Hide);
    }

    #[test]
    fn stale_folder_result_is_rejected() {
        let mut s = SearchState::new();
        let t1 = s.enter_folder("C:\\a".into());
        let t2 = s.navigate_folder("C:\\a\\b".into());
        assert!(!s.accept_folder_result(t1)); // 旧 token は破棄
        assert!(s.accept_folder_result(t2)); // 最新 token は受理
    }

    #[test]
    fn escape_invalidates_gen_so_late_nav_result_is_dropped() {
        let mut s = SearchState::new();
        s.set_results(vec![res("a")]);
        let tok = s.enter_folder("C:\\a".into());
        s.on_escape(); // folder 離脱 → gen 失効 + folder None
        assert!(!s.accept_folder_result(tok)); // 離脱後は旧ナビ結果を受理しない
        assert_eq!(s.view_kind(), ViewKind::Results);
    }

    #[test]
    fn reset_invalidates_folder_gen_and_clears_mode() {
        let mut s = SearchState::new();
        let tok = s.enter_folder("C:\\a".into());
        s.reset(); // show 時 reset
        assert!(!s.accept_folder_result(tok));
        assert_eq!(s.view_kind(), ViewKind::Results);
        assert_eq!(s.folder_filter(), "");
    }

    #[test]
    fn folder_mode_interp_is_plain_even_with_prefix() {
        let mut s = SearchState::new();
        s.enter_folder("C:\\a".into());
        s.set_folder_filter("@x".into()); // folder_filter に @ が入っても
        // interp は view_kind()==Folder ゆえ Plain（query 相乗りしないので query は空のまま）
        assert_eq!(s.interp("@"), QueryIntent::Plain);
    }

    #[test]
    fn stale_token_rejected_after_escape_and_reenter_while_folder_is_some() {
        // enter/escape/re-enter は folder_gen を進めるので、離脱前の token は
        // 再突入で folder.is_some()==true に戻っても受理されない（staleness の
        // defense-in-depth: is_some ガードと gen bump の両方が効いていることを固定）。
        let mut s = SearchState::new();
        s.set_results(vec![res("a")]);
        let t1 = s.enter_folder("C:\\a".into());
        s.on_escape(); // folder=None・gen 進む
        let t2 = s.enter_folder("C:\\a".into()); // 再突入・folder=Some・gen 進む
        assert!(!s.accept_folder_result(t1)); // 旧 token は folder Some でも拒否
        assert!(s.accept_folder_result(t2)); // 最新 token は受理
        assert_eq!(s.view_kind(), ViewKind::Folder);
    }
}
