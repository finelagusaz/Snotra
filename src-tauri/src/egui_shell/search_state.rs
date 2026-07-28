//! egui 検索ウィンドウの純粋状態核（#532 SU3）。query/選択/結果と 2 軸モード導出・遷移を
//! egui/Win32 非依存で持ち、driver（launcher_controller.rs）から駆動される。ユニットテスト対象。

use snotra_core::config::OpenerTool;
use snotra_core::ui_types::SearchResult;

/// 軸1: モーダルビュースタック頂点の種類。Results/Folder/Tool の 3 段ラダー（Folder は M2 で
/// 到達可能化・#532 SU3 M2、Tool は SU3.5 で到達可能化・#532 SU3.5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewKind {
    Results,
    Folder,
    Tool,
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

/// フォルダ展開直後、列挙結果（cache）も失敗行（error）も未着の間は true（#636 レビュー Finding A）。
/// この窓では `results` が展開前ビューの残存物なので、driver は起動（Enter/クリック）を抑止する
/// ——dead/slow UNC でロードが滞留すると、前ビューの誤項目を起動しうるため。前フレーム結果の保持は
/// フリッカ回避の意図的設計（launcher_controller.rs run_search）ゆえ温存し、不可逆な起動だけを止める。Results
/// モードや列挙完了（cache/error いずれか到着）後は false で、通常どおり起動できる。
pub fn folder_load_pending(view_kind: ViewKind, has_folder_cache: bool, has_folder_error: bool) -> bool {
    view_kind == ViewKind::Folder && !has_folder_cache && !has_folder_error
}

/// フォルダ展開モードの退避/復元単位（`Option<FolderFrame>`・#532 SU3 M2）。深掘りは push でなく
/// current_dir 書き換え。フォルダ内フィルタは frame でなく SearchState.folder_filter が持つ。
#[derive(Debug, Clone)]
pub struct FolderFrame {
    pub restore_query: String,
    pub restore_results: Vec<SearchResult>,
    pub restore_selected: usize,
    pub current_dir: String,
}

/// ツール選択モードの退避/復元単位（#532 SU3.5・SolidJS ToolSelectionFrame parity）。
/// tool は folder の上に積まれうる（§18.5 直交）が、tool-on-tool は Option ゆえ表現不能。
/// restore_query を持たない——tool 中は入力無効（§18.5）で query 不変ゆえ復元不要
/// （SolidJS popView の tool 段も query を復元しない）。launch_query は起動 API へ渡す
/// 元クエリで復元には使わない（SolidJS #538 の launchQuery / restoreQuery 型分離）。
/// target_path/target_is_folder/tools/launch_query は driver（launcher_controller.rs）が
/// tool_frame() 越しに読む（`shift_activate` / `execute_tool_selected`・#532 SU3.5 Task 3）。
#[derive(Debug, Clone)]
pub struct ToolFrame {
    pub restore_results: Vec<SearchResult>,
    pub restore_selected: usize,
    pub target_path: String,
    pub target_is_folder: bool,
    pub tools: Vec<OpenerTool>,
    pub launch_query: String,
    pub saved_folder_filter: String,
}

/// Escape ラダーの分岐（driver が side-effect を実行）。tool 段・folder 段・top-level の
/// 3 段ラダー（instant/command 解除段は M3 で確定済み・prefix hot-change は別軸）。
#[derive(Debug, PartialEq, Eq)]
pub enum EscapeOutcome {
    /// folder → 展開前検索状態へ復帰済み（driver は追加操作なし）
    RestoredSearch,
    /// tool → 直下ビュー（folder/results）へ復帰済み（driver は folder cache を保持したまま
    /// repaint のみ。RestoredSearch と違い cache/error を破棄しない——folder が下に生きている）
    RestoredFromTool,
    /// top-level → hide 要求（driver が emit）
    Hide,
}

/// slash コマンドの写像（§15.2）。History(`/r`) だけは結果注入型（履歴を表示して留まる）で、
/// driver が run_search の Command 分岐へ振る。他 3 つは fire-once の副作用型。
/// driver（launcher_controller.rs）が消費する（#532 SU3 M3 Task 2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashCmd {
    History,
    OpenSettings,
    RebuildIndex,
    Quit,
}

/// trim 後の完全一致で slash コマンドを引く（§15.3 即実行の判定・commands.ts findCommand parity・
/// 大文字小文字は区別する）。部分入力・引数付きは None（候補表示なし・§15.3）。
/// driver（launcher_controller.rs）が消費する（#532 SU3 M3 Task 2）。
pub fn find_slash_command(query: &str) -> Option<SlashCmd> {
    match query.trim() {
        "/r" => Some(SlashCmd::History),
        "/o" => Some(SlashCmd::OpenSettings),
        "/s" => Some(SlashCmd::RebuildIndex),
        "/q" => Some(SlashCmd::Quit),
        _ => None,
    }
}

/// 検索ウィンドウの純粋状態。results 軸に加え folder 軸（folder / folder_filter / folder_gen・
/// M2 で追加・#532 SU3 M2）と tool 軸（tool・SU3.5 で追加・#532 SU3.5）を持つ。
pub struct SearchState {
    query: String,
    results: Vec<SearchResult>,
    selected: usize,
    folder: Option<FolderFrame>,
    folder_filter: String,
    folder_gen: u64,
    tool: Option<ToolFrame>,
    /// `results` が総入れ替えされるたびに進む世代（#699。旧 `view.rs` の
    /// `snapshot_generation` をここへ移した）。
    ///
    /// **この型の中で `self.results` へ代入・`clear` するメソッドは、必ずここを進める。**
    /// 全称の射程はこの型の内側に限る——外から `results` を差し替える経路は無い
    /// （フィールドは private で、`set_results` / `enter_tool` / `on_escape` / `reset` だけが触る）。
    ///
    /// 進めるのは**行の差し替え**だけであって選択の移動ではない。`move_selection` /
    /// `reset_selection` で進めると、消費側（#699 のクリック照合）が全クリックを捨てる。
    /// さらに #714 以降、世代交代フレームは results 窓のスクロールがアニメーションなし
    /// （瞬時）になる——選択移動で進めると ↑↓ のアニメーション維持という要件も壊れる。
    rows_generation: u64,
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
            tool: None,
            rows_generation: 0,
        }
    }

    /// `results` の世代（#699 / #632 Fix 3）。**行が差し替わったかの唯一の判定材料**である
    /// ——`selected` の値だけでは「打鍵で結果が丸ごと変わったが selected は偶然 0 のまま」を
    /// 検出できない。`RowsSnapshot.generation` と results 窓からのクリック照合が消費する。
    pub fn rows_generation(&self) -> u64 {
        self.rows_generation
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

    /// 結果を差し替える。選択は範囲内へクランプ（空なら 0）。世代を進める（#699）。
    pub fn set_results(&mut self, results: Vec<SearchResult>) {
        self.results = results;
        self.selected = clamp_selected(self.results.len(), self.selected);
        self.rows_generation += 1;
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    /// 選択を先頭へ戻す。driver が打鍵（changed エッジ）ごとに呼ぶ（SolidJS の毎打鍵
    /// setSelected(0) parity・#532 SU3 M3）。
    pub fn reset_selection(&mut self) {
        self.selected = 0;
    }

    /// 軸1: モーダルビュー頂点の射影（§18.5 優先度 tool > folder > results と一対一）。
    pub fn view_kind(&self) -> ViewKind {
        if self.tool.is_some() {
            ViewKind::Tool
        } else if self.folder.is_some() {
            ViewKind::Folder
        } else {
            ViewKind::Results
        }
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
    pub fn navigate_folder(&mut self, dir: String) -> u64 {
        if let Some(f) = self.folder.as_mut() {
            f.current_dir = dir;
        }
        self.folder_filter.clear();
        self.selected = 0;
        self.folder_gen += 1;
        self.folder_gen
    }

    /// launcher_controller.rs（driver）は現状 `parent_dir()` 越しに current_dir を使い、生の accessor は直接
    /// 呼ばない（folder 中の hint 文脈提示は §6 で任意扱い・#532 SU3 M2 Task 3 で見送り）。
    #[allow(dead_code)]
    pub fn folder_current_dir(&self) -> Option<&str> {
        self.folder.as_ref().map(|f| f.current_dir.as_str())
    }

    /// folder 中の親ディレクトリ（ルート終端で None）。
    pub fn parent_dir(&self) -> Option<String> {
        self.folder.as_ref().and_then(|f| compute_parent_dir(&f.current_dir))
    }

    /// driver は token を `enter_folder`/`navigate_folder` の戻り値から直接得るため、独立した
    /// getter としては未消費（#532 SU3 M2 Task 3）。
    #[allow(dead_code)]
    pub fn folder_gen(&self) -> u64 {
        self.folder_gen
    }

    /// 遅延到着したナビ結果を受理してよいか（tool 非表示 ∧ token 一致 ∧ folder 中）。
    /// tool 中は §18.5「検索結果が上書きされない」ため受理しない（driver は破棄でなく
    /// 保留せず捨てる——escape で folder へ戻れば次の打鍵/ナビが再ロードする）。
    pub fn accept_folder_result(&self, token: u64) -> bool {
        self.tool.is_none() && token == self.folder_gen && self.folder.is_some()
    }

    /// ツール選択へ突入（§18.4: 結果リストをツール一覧で置換）。現在ビュー（Results/Folder
    /// どちらでも）の表示状態を frame へ退避する。tools ≥ 2 の判定は driver 側
    /// （§18.3: ≤1 は通常 Enter と同一のため、そもそも呼ばれない）。driver からは
    /// `shift_activate` が呼ぶ（#532 SU3.5 Task 3）。
    pub fn enter_tool(&mut self, target_path: String, target_is_folder: bool, tools: Vec<OpenerTool>) {
        let rows: Vec<SearchResult> = tools
            .iter()
            .map(|t| SearchResult {
                name: t.name.clone(),
                path: t.exe.clone(),
                is_folder: false,
                is_error: false,
            })
            .collect();
        self.tool = Some(ToolFrame {
            restore_results: std::mem::take(&mut self.results),
            restore_selected: self.selected,
            target_path,
            target_is_folder,
            tools,
            launch_query: self.query.clone(),
            saved_folder_filter: self.folder_filter.clone(),
        });
        self.results = rows;
        self.selected = 0;
        self.rows_generation += 1; // ツール行への総入れ替え（#699）
    }

    /// driver（`shift_activate` / `execute_tool_selected`）が読む（#532 SU3.5 Task 3）。
    pub fn tool_frame(&self) -> Option<&ToolFrame> {
        self.tool.as_ref()
    }

    pub fn folder_filter(&self) -> &str {
        &self.folder_filter
    }

    pub fn set_folder_filter(&mut self, f: String) {
        self.folder_filter = f;
        self.selected = 0;
    }

    /// Escape ラダー（M2）: folder 中は展開前状態へ復帰、top-level は Hide。
    /// folder 離脱時は folder_gen を進めて遅延到着した旧ナビ結果を無効化する。
    pub fn on_escape(&mut self) -> EscapeOutcome {
        if let Some(t) = self.tool.take() {
            // query は復元しない（tool 中は入力無効で不変・ToolFrame doc 参照）
            self.results = t.restore_results;
            self.selected = clamp_selected(self.results.len(), t.restore_selected);
            self.folder_filter = t.saved_folder_filter;
            self.rows_generation += 1; // 退避行への総入れ替え（#699）
            return EscapeOutcome::RestoredFromTool;
        }
        if let Some(f) = self.folder.take() {
            self.query = f.restore_query;
            self.results = f.restore_results;
            self.selected = clamp_selected(self.results.len(), f.restore_selected);
            self.folder_filter.clear();
            self.folder_gen += 1; // 離脱経路でも失効
            self.rows_generation += 1; // 退避行への総入れ替え（#699）
            EscapeOutcome::RestoredSearch
        } else {
            // Hide 経路は results を触らないので世代も進めない
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
        self.tool = None;
        self.rows_generation += 1; // 空行への総入れ替え（#699）——hide を跨いだ stale クリックはこれで失効する
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

/// Enter 時の trailing flush 要否（#631 flush-on-Enter・SolidJS flushPendingRefresh 同型）。
/// armed になるのは Results∧Plain 経路のみ（folder=同期・instant/command=cancel 済み）だが、
/// 将来の armed 経路追加に対して条件を独立に固定する（誤発火の構造的防止・spec C 節）。
pub fn should_flush_on_enter(view_kind: ViewKind, is_plain: bool, armed: bool) -> bool {
    view_kind == ViewKind::Results && is_plain && armed
}

/// 親ディレクトリを返す。ルート（`C:\` / `\\server\share\`）で None。folderNav.computeParentDir 相当。
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

/// §4.7 表示ゲート（#633・SU6 spec 決定 3）: 再インデックス中は plain 結果のみ隠す。
/// SolidJS `shouldShowResults`（search.ts: `interpKind()==="instant" || !indexing()`）の鏡写しで、
/// instant/folder/tool は表示継続、データと選択は保持する（クリアしない・選択リセットしない）。
/// `instant_rows` は表示中行の来歴 snapshot（`instant_rows_query.is_some()`）——live interp でなく
/// 来歴で判定するのは prefix hot-change の stale 行対策（#637 finding 0）と同じ理由。
/// driver（view.rs）は Task 4 で表示分岐に組み込む（#532 SU6 Task 1）。
pub fn plain_results_hidden(view_kind: ViewKind, instant_rows: bool, indexing: bool) -> bool {
    indexing && matches!(view_kind, ViewKind::Results) && !instant_rows
}

/// #633 世代トリガ（SU6 spec 決定 3）: index build 完了で bump される世代が last-seen と
/// 異なれば再検索。bool エッジ検出と違い、started/complete の repaint が 1 フレームに合流して
/// パルスが見えなくても累積カウンタは差分が残るため取りこぼさない。
/// driver（launcher_controller.rs）は Task 4 で再検索トリガに組み込む（#532 SU6 Task 1）。
pub fn needs_index_refresh(last_seen: u64, current: u64) -> bool {
    last_seen != current
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_query_is_plain() {
        assert_eq!(interpret("firefox", "@", ViewKind::Results), QueryIntent::Plain);
    }

    #[test]
    fn flush_on_enter_only_for_armed_plain_results() {
        assert!(should_flush_on_enter(ViewKind::Results, true, true));
        assert!(!should_flush_on_enter(ViewKind::Results, true, false), "armed でなければ flush 不要");
        assert!(!should_flush_on_enter(ViewKind::Results, false, true), "instant/command では flush しない");
        assert!(!should_flush_on_enter(ViewKind::Folder, true, true), "folder は同期フィルタ");
        assert!(!should_flush_on_enter(ViewKind::Tool, true, true), "tool 中は検索自体が凍結");
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

    // ---- rows_generation（#699）----
    //
    // **両方向を固定する。** 進まないと stale クリックが誤った行を起動し、進めすぎると
    // 正当なクリックが全部落ちる。「行の差し替えで進む」「選択の移動では進まない」の
    // 対で初めて意味を持つ。

    #[test]
    fn rows_generation_advances_on_set_results() {
        let mut s = SearchState::new();
        let g0 = s.rows_generation();
        s.set_results(vec![res("a")]);
        assert_eq!(s.rows_generation(), g0 + 1);
    }

    /// #699 で当初の計画が落としていた経路（issue 本文も挙げていない）。
    #[test]
    fn rows_generation_advances_on_enter_tool() {
        let mut s = SearchState::new();
        s.set_results(vec![res("a"), res("b")]);
        let g = s.rows_generation();
        s.enter_tool("C:/t.txt".into(), false, make_tools());
        assert_eq!(s.rows_generation(), g + 1, "ツール行への総入れ替えで進むこと");
    }

    /// `on_escape` は **2 分岐とも** 行を差し替える。片方だけ直すと穴が残る。
    #[test]
    fn rows_generation_advances_on_escape_from_tool() {
        let mut s = SearchState::new();
        s.set_results(vec![res("a"), res("b"), res("c")]);
        s.enter_tool("C:/t.txt".into(), false, make_tools());
        let g = s.rows_generation();
        assert_eq!(s.on_escape(), EscapeOutcome::RestoredFromTool);
        assert_eq!(s.rows_generation(), g + 1, "退避行への復帰で進むこと");
    }

    #[test]
    fn rows_generation_advances_on_escape_from_folder() {
        let mut s = SearchState::new();
        s.set_results(vec![res("a"), res("b")]);
        s.enter_folder("C:/dir".into()); // 退避のみ（results は差し替えない）
        s.set_results(vec![res("in-dir")]); // folder の列挙結果
        let g = s.rows_generation();
        assert_eq!(s.on_escape(), EscapeOutcome::RestoredSearch);
        assert_eq!(s.rows_generation(), g + 1, "展開前の行への復帰で進むこと");
    }

    /// `enter_folder` は `results` を frame へ退避するだけで差し替えない——**進めないのが正しい**。
    /// 進めると、フォルダ展開の瞬間に飛んでいた正当なクリックまで落ちる。
    #[test]
    fn rows_generation_is_stable_on_enter_folder() {
        let mut s = SearchState::new();
        s.set_results(vec![res("a"), res("b")]);
        let g = s.rows_generation();
        s.enter_folder("C:/dir".into());
        assert_eq!(s.rows_generation(), g);
    }

    /// top-level の Escape（Hide）は `results` を触らない。
    #[test]
    fn rows_generation_is_stable_on_escape_to_hide() {
        let mut s = SearchState::new();
        s.set_results(vec![res("a")]);
        let g = s.rows_generation();
        assert_eq!(s.on_escape(), EscapeOutcome::Hide);
        assert_eq!(s.rows_generation(), g);
    }

    #[test]
    fn rows_generation_advances_on_reset() {
        let mut s = SearchState::new();
        s.set_results(vec![res("a")]);
        let g = s.rows_generation();
        s.reset();
        assert_eq!(s.rows_generation(), g + 1, "reset は hide を跨いだ stale クリックを失効させる");
    }

    /// **進めすぎの側**。選択の移動は行の差し替えではない——ここで進めると #699 の
    /// 照合が正当なクリックまで捨てる。
    #[test]
    fn rows_generation_is_stable_on_selection_change() {
        let mut s = SearchState::new();
        s.set_results(vec![res("a"), res("b"), res("c")]);
        let g = s.rows_generation();
        s.move_selection(2);
        s.reset_selection();
        assert_eq!(s.rows_generation(), g, "選択の移動では進まないこと");
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

    // ---- 階層移動と選択（#743・SPEC §6.1 の as-built）----
    //
    // **この 5 本が固定するのは状態核の合成である**——`parent_dir` → `navigate_folder` の連鎖と、
    // 行の差し替えが選択に与える影響。**キー割り当て（`view.rs` の読み）と driver の分岐
    // （`launcher_controller.rs` の `on_nav_keys`）は射程外**であり、`←` の分岐を丸ごと殺しても
    // この 5 本は緑のままである（実測）。分岐が正しく Folder 側へ落ちることの証拠は実機トレース
    // 計測であってこのテストではない——「緑だから ← の分岐も守られている」と読まないこと。
    //
    // なお `navigate_folder` は `folder` が `None` でも黙って通る。**各テストは必ず
    // `enter_folder` を先に通す**——突入を忘れたテストは何も検証しないまま緑になる。

    /// `←` の連続で 2 段上がる（#743 の報告が「起きない」と述べた挙動そのもの）。
    #[test]
    fn left_twice_climbs_two_levels() {
        let mut s = SearchState::new();
        let t0 = s.enter_folder("C:\\a\\b\\c".into());
        let p1 = s.parent_dir().expect("孫フォルダからは親が取れる");
        assert_eq!(p1, "C:\\a\\b");
        let t1 = s.navigate_folder(p1);
        assert_eq!(s.view_kind(), ViewKind::Folder, "上昇しても folder のままである");
        assert_eq!(s.folder_current_dir(), Some("C:\\a\\b"));
        let p2 = s.parent_dir().expect("さらに親が取れる");
        assert_eq!(p2, "C:\\a");
        let t2 = s.navigate_folder(p2);
        assert_eq!(s.folder_current_dir(), Some("C:\\a"));
        assert!(t0 < t1 && t1 < t2, "遷移ごとに token が進む（遅着の旧結果を失効させる）");
    }

    /// ルート終端では親が無い（§6.2）。driver の `if let Some(parent)` がここで無反応に落ちる。
    #[test]
    fn left_at_root_has_no_parent() {
        let mut drive = SearchState::new();
        drive.enter_folder("C:\\".into());
        assert_eq!(drive.parent_dir(), None, "ドライブルートは終端");
        let mut unc = SearchState::new();
        unc.enter_folder("\\\\srv\\share".into());
        assert_eq!(unc.parent_dir(), None, "UNC 共有ルートは終端");
    }

    /// 前進方向の遷移は選択を先頭へ戻す（§6.1 の 1 段目）。**非ゼロから始める**——0 から
    /// 始めると `enter_folder` の初期値と区別が付かず、何も実証しない。突入と上昇の両方を測る。
    #[test]
    fn folder_navigation_resets_selection_to_first_row() {
        let mut s = SearchState::new();
        s.set_results(vec![res("a"), res("b"), res("c")]);
        s.move_selection(2);
        assert_eq!(s.selected(), 2);
        s.enter_folder("C:\\a\\b".into());
        assert_eq!(s.selected(), 0, "通常検索からの突入で先頭へ");
        s.set_results(vec![res("x"), res("y"), res("z")]);
        s.move_selection(2);
        assert_eq!(s.selected(), 2);
        s.navigate_folder("C:\\a".into());
        assert_eq!(s.selected(), 0, "folder 中の遷移でも先頭へ");
    }

    /// 到着した行は選択を**先頭へ戻さず**、件数の範囲へ収めるだけである（§6.1 の 2 段目）。
    /// **数値は非退化のものを使う**——3 件 → 1 件では `2.min(0) == 0` となり、無条件リセットと
    /// 区別が付かないため何も証明しない。ロード中に上下カーソルキーで動かした選択が到着後も
    /// 残るのはこの性質による。
    #[test]
    fn arriving_rows_clamp_selection_without_resetting_it() {
        let mut s = SearchState::new();
        s.enter_folder("C:\\a".into());
        s.set_results(vec![res("a"), res("b"), res("c"), res("d"), res("e")]); // 置換前の候補
        s.navigate_folder("C:\\a\\b".into()); // ← / → の打鍵: 選択は 0 へ・行はまだ古いまま
        s.move_selection(3); // ロード窓で ↑↓ が入る
        assert_eq!(s.selected(), 3);
        s.set_results(vec![res("a"), res("b"), res("c"), res("d")]);
        assert_eq!(s.selected(), 3, "範囲内なら保たれる（先頭へは戻さない）");
        s.set_results(vec![res("a"), res("b")]);
        assert_eq!(s.selected(), 1, "範囲外は末尾へクランプされる（0 ではない）");
    }

    /// 0 件が到着したときは選択の指す行が無い（§6.1 の 2 段目の末尾）。**上の「先頭へ戻さない」
    /// とは別の 0 である**——方針としてのリセットではなく、有効な index が存在しないための 0。
    #[test]
    fn arriving_empty_rows_leaves_no_selectable_row() {
        let mut s = SearchState::new();
        s.enter_folder("C:\\empty".into());
        s.set_results(vec![res("a"), res("b"), res("c")]);
        s.move_selection(2);
        assert_eq!(s.selected(), 2);
        s.set_results(vec![]); // 空フォルダの列挙結果が到着
        assert_eq!(s.selected(), 0);
        assert!(s.results().is_empty(), "選択が 0 でも、それが指す行は無い");
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
    fn folder_load_pending_blocks_launch_only_before_cache_or_error() {
        // Folder 突入直後（cache も error も未着）は起動抑止の窓 = true。
        assert!(folder_load_pending(ViewKind::Folder, false, false));
        // 列挙成功（cache 到着）後は false → 通常どおり起動できる。
        assert!(!folder_load_pending(ViewKind::Folder, true, false));
        // 列挙失敗（error 行到着）後も false（error 行の非起動は activate の is_error ガードが担う）。
        assert!(!folder_load_pending(ViewKind::Folder, false, true));
        // Results モードでは folder cache/error に依らず常に false（前ビュー残存物問題は起きない）。
        assert!(!folder_load_pending(ViewKind::Results, false, false));
        assert!(!folder_load_pending(ViewKind::Results, true, false));
    }

    #[test]
    fn find_slash_command_exact_match_with_trim() {
        assert_eq!(find_slash_command("/r"), Some(SlashCmd::History));
        assert_eq!(find_slash_command(" /o "), Some(SlashCmd::OpenSettings)); // trim 後一致
        assert_eq!(find_slash_command("/s"), Some(SlashCmd::RebuildIndex));
        assert_eq!(find_slash_command("/q"), Some(SlashCmd::Quit));
    }

    #[test]
    fn find_slash_command_rejects_partial_case_and_args() {
        assert_eq!(find_slash_command("/"), None); // 部分入力
        assert_eq!(find_slash_command("/x"), None); // 未知コマンド
        assert_eq!(find_slash_command("/O"), None); // 大文字は不一致（findCommand === parity）
        assert_eq!(find_slash_command("/o extra"), None); // 引数付きは不一致（完全一致のみ）
        assert_eq!(find_slash_command(""), None);
    }

    #[test]
    fn reset_selection_returns_to_top() {
        // SolidJS parity: 毎打鍵 setSelected(0)（handlePlainQueryInput / instant fetch / slash とも）。
        // M1 は set_results の clamp のみで、打鍵後も旧 selected が残っていた（M3 で是正）。
        let mut s = SearchState::new();
        s.set_results(vec![res("a"), res("b"), res("c")]);
        s.move_selection(2);
        assert_eq!(s.selected(), 2);
        s.reset_selection();
        assert_eq!(s.selected(), 0);
    }

    fn make_tools() -> Vec<OpenerTool> {
        vec![
            OpenerTool { name: "VSCode".into(), exe: "Code.exe".into(), args: String::new() },
            OpenerTool { name: "Terminal".into(), exe: "wt.exe".into(), args: "-d {path}".into() },
        ]
    }

    #[test]
    fn enter_tool_from_results_saves_and_replaces_rows() {
        let mut s = SearchState::new();
        s.set_query("code".into());
        s.set_results(vec![SearchResult {
            name: "proj".into(), path: "C:\\proj".into(), is_folder: true, is_error: false,
        }]);
        s.enter_tool("C:\\proj".into(), true, make_tools());
        assert_eq!(s.view_kind(), ViewKind::Tool);
        // 行はツール一覧（name=表示名・path=exe・§18.4）
        assert_eq!(s.results().len(), 2);
        assert_eq!(s.results()[0].name, "VSCode");
        assert_eq!(s.results()[0].path, "Code.exe");
        assert!(!s.results()[0].is_folder);
        assert_eq!(s.selected(), 0);
        let f = s.tool_frame().expect("frame");
        assert_eq!(f.target_path, "C:\\proj");
        assert!(f.target_is_folder);
        assert_eq!(f.launch_query, "code");
    }

    #[test]
    fn escape_from_tool_restores_results_view() {
        let mut s = SearchState::new();
        s.set_query("code".into());
        s.set_results(vec![SearchResult {
            name: "proj".into(), path: "C:\\proj".into(), is_folder: true, is_error: false,
        }]);
        s.enter_tool("C:\\proj".into(), true, make_tools());
        assert_eq!(s.on_escape(), EscapeOutcome::RestoredFromTool);
        assert_eq!(s.view_kind(), ViewKind::Results);
        assert_eq!(s.results()[0].name, "proj");
        assert_eq!(s.query(), "code");
    }

    #[test]
    fn escape_ladder_tool_then_folder_then_hide() {
        // §18.4: tool → folder 復帰（filter 込み）→ results → hide の全段
        let mut s = SearchState::new();
        s.set_query("pre".into());
        s.set_results(vec![SearchResult {
            name: "dir".into(), path: "C:\\dir".into(), is_folder: true, is_error: false,
        }]);
        s.enter_folder("C:\\dir".into());
        s.set_folder_filter("fil".into());
        s.set_results(vec![SearchResult {
            name: "child".into(), path: "C:\\dir\\child".into(), is_folder: false, is_error: false,
        }]);
        s.enter_tool("C:\\dir\\child".into(), false, make_tools());
        assert_eq!(s.view_kind(), ViewKind::Tool);

        assert_eq!(s.on_escape(), EscapeOutcome::RestoredFromTool);
        assert_eq!(s.view_kind(), ViewKind::Folder); // folder が下に残っている（§18.5 直交）
        assert_eq!(s.folder_filter(), "fil"); // saved_folder_filter 復元
        assert_eq!(s.results()[0].name, "child"); // folder のフィルタ済みビュー復元

        assert_eq!(s.on_escape(), EscapeOutcome::RestoredSearch);
        assert_eq!(s.view_kind(), ViewKind::Results);
        assert_eq!(s.query(), "pre");

        assert_eq!(s.on_escape(), EscapeOutcome::Hide);
    }

    #[test]
    fn reset_clears_tool_slot() {
        let mut s = SearchState::new();
        s.set_results(vec![SearchResult {
            name: "f".into(), path: "C:\\f".into(), is_folder: false, is_error: false,
        }]);
        s.enter_tool("C:\\f".into(), false, make_tools());
        s.reset(); // §18.5: ホットキー再表示（resetForShow）でツール選択はリセット
        assert_eq!(s.view_kind(), ViewKind::Results);
        assert!(s.tool_frame().is_none());
    }

    #[test]
    fn interp_during_tool_is_plain() {
        // §18.5 入力無効の状態面: tool 中はどんな query でも Plain（instant/command 化しない）
        let mut s = SearchState::new();
        s.set_results(vec![SearchResult {
            name: "f".into(), path: "C:\\f".into(), is_folder: false, is_error: false,
        }]);
        s.enter_tool("C:\\f".into(), false, make_tools());
        s.set_query("@gh".into());
        assert_eq!(s.interp("@"), QueryIntent::Plain);
        s.set_query("/r".into());
        assert_eq!(s.interp("@"), QueryIntent::Plain);
    }

    #[test]
    fn folder_results_rejected_while_tool_is_open() {
        // §18.5「ツール選択中の検索結果が上書きされない」: tool 中に遅延到着した folder ナビ
        // 結果（dead/slow UNC）を drain が受理してツール一覧を潰さない。
        let mut s = SearchState::new();
        let tok = s.enter_folder("C:\\slow".into());
        s.enter_tool("C:\\slow\\x".into(), false, make_tools());
        assert!(!s.accept_folder_result(tok));
        s.on_escape(); // tool 解除 → folder 復帰
        assert!(s.accept_folder_result(tok)); // folder に戻れば同 token は再び有効（gen は進めていない）
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

    #[test]
    fn plain_results_hidden_only_for_plain_results_view() {
        // §4.7: indexing 中は plain results のみ隠す（SolidJS shouldShowResults 鏡写し）
        assert!(plain_results_hidden(ViewKind::Results, false, true));
        // instant 行は表示継続（§4.7 instant carve-out・SPEC §4.7:181）
        assert!(!plain_results_hidden(ViewKind::Results, true, true));
        // folder/tool は index 非依存ゆえ表示継続
        assert!(!plain_results_hidden(ViewKind::Folder, false, true));
        assert!(!plain_results_hidden(ViewKind::Tool, false, true));
        // 非 indexing は常に表示
        assert!(!plain_results_hidden(ViewKind::Results, false, false));
    }

    #[test]
    fn needs_index_refresh_only_on_generation_change() {
        assert!(!needs_index_refresh(0, 0));
        assert!(needs_index_refresh(0, 1));
        // 複数回 bump がまとまっても 1 回の比較で拾う（repaint 合流パルス耐性・spec 決定 3）
        assert!(needs_index_refresh(3, 7));
    }
}
