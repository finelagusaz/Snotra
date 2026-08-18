//! egui 検索ウィンドウの純粋状態核（#532 SU3）。query/選択/結果と 2 軸モード導出・遷移を
//! egui/Win32 非依存で持ち、driver（launcher_controller.rs）から駆動される。ユニットテスト対象。

use std::time::Instant;

use snotra_core::config::OpenerTool;
use snotra_core::ui_types::{IconSource, SearchResult};

use crate::egui_shell::search_dispatch::{SearchDispatch, Settled};

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
pub fn folder_load_pending(
    view_kind: ViewKind,
    has_folder_cache: bool,
    has_folder_error: bool,
) -> bool {
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
    /// 突入した時点で [`SearchState::is_unsettled`] が真だったか（#1079）。**復帰させる行が
    /// 復元 query の最終結果かどうかを、常に観測できる点は突入時だけである**——`enter_folder` は
    /// 行を差し替えないので in-flight が残るが、`on_escape` の `put_rows` がそれを無条件に落として
    /// しまうため、Escape の時点では材料がもう無い。ゆえにここへ控える。（folder の列挙結果が届く
    /// までの間も観測できるが、届いたかどうかが Escape の時点で判らない以上、**常に**有効な点ではない。）
    ///
    /// **控えた `armed` は減衰しない**（受容する残余）。`armed` はそれ自体が過剰近似で、
    /// [`crate::egui_shell::layout::Debouncer::poll`] の trailing 発火で**最後の打鍵から** debounce
    /// interval のうちに落ちる——ところがここへ写した値は folder に居る間に落ちる契機を持たない。
    /// ゆえに「打鍵 → interval 内に結果が届いて行は最新 → まだ armed のまま `→`」で突入すると、
    /// 復帰後の Enter に不要な同期 `engine.search` が 1 回乗る。**受容する。** 余分な費用はその 1 回
    /// だけであり、**過剰近似の向きは安全側である**——[`should_flush_on_enter`] は `unsettled` が
    /// 真のときに flush を**足す**だけなので、**古い行のまま起動する side へは倒れない**（偽陽性が
    /// 何をするかは flush の枝で分かれる: 通常は行を作り直し、空クエリ・`indexing()` 中は
    /// `set_results(Vec::new())` で起動そのものが止まる——後者は flush の既定の扱いである）。
    /// **窓の長さをここに書かない**（interval の正本は `LauncherController` が `Debouncer::new` へ
    /// 渡す値であって、この doc ではない）。
    pub unsettled_at_entry: bool,
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
///
/// **`#[must_use]` は関数ではなく型に付けてある**（#934）——危ういのは「この値を落とすこと」で
/// あって特定の関数ではないため、`EscapeOutcome` を返す将来の関数も覆う。配置の規約と受容残余の
/// 正本は `src-tauri/CLAUDE.md`「処置を返す純粋核の強制」。
#[must_use = "driver が処置を実行しないと hide 要求・repaint 予約・ビュー復元が黙って消える（#934）"]
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
    /// **ここを進めるのは [`SearchState::put_rows`] だけであり、そこが in-flight の失効も同時に行う**
    /// （#1039）。収束の射程と、`enter_tool` の `std::mem::take` がなぜその外にあってよいかは
    /// [`SearchState::put_rows`] の doc が正本である（ここには写しを置かない）。
    ///
    /// 進めるのは**行の差し替え**だけであって選択の移動ではない。`move_selection` /
    /// `reset_selection` で進めると、消費側（#699 のクリック照合）が全クリックを捨てる。
    /// さらに #714 以降、世代交代フレームは results 窓のスクロールがアニメーションなし
    /// （瞬時）になる——選択移動で進めると ↑↓ のアニメーション維持という要件も壊れる。
    rows_generation: u64,
    /// 検索要求の同一性（#1004）。**行の差し替えと同じ型に住まわせる**（#1039）——
    /// 同じ出来事に対する 2 つの義務（世代の前進と in-flight の失効）が
    /// [`SearchState::put_rows`] の内側で合流する。
    dispatch: SearchDispatch,
    /// 現在の `results` が、frame から**復帰した行**であって現 `query` の最終結果ではないこと
    /// （#1079）。[`SearchState::is_unsettled`] の第 3 の disjunct であり、folder の往復を
    /// 跨いで未反映を運ぶ唯一の材料である。
    ///
    /// **立てるのは [`SearchState::on_escape`] の folder 枝だけ、落とすのは
    /// [`SearchState::put_rows`] だけである。** 後者に置いたのは #1039 の設計を踏襲するため
    /// ——行が実際に差し替われば、それが何であれ「復帰した行」ではなくなる。**show を跨ぐ
    /// クリアが構造で付いてくるのもここに置いた帰結である**（[`SearchState::reset`] が
    /// `put_rows` を通るため、`consume_reset_pending` の一覧へ入れ忘れる形の残余を作らない）。
    ///
    /// **[`ToolFrame`] に対称のフィールドを置かないのは、`enter_tool` が
    /// [`crate::egui_shell::launcher_controller::LauncherController::on_enter`] の flush の**下流に
    /// しか無い**からである**——`enter_tool` の production 呼び出しは `shift_activate` の 1 つ、その
    /// `shift_activate` の呼び出しは `on_enter` の 1 つで、`on_enter` は flush を先に済ませる
    /// （クリック経路は `activate_or_execute` を直呼びし `enter_tool` へ届かない・2026-08-14 実測）。
    /// folder の上に積まれた tool から Escape 2 回で戻る並びは、2 回目の folder 枝が控えを立て直す。
    ///
    /// **却下: 「folder から復帰したら無条件に立てる」。** [`FolderFrame::unsettled_at_entry`] を
    /// 持たずに済み `enter_folder` の signature も変えずに済むが、**未反映が無いまま往復した場合
    /// まで真になる**——[`SearchState::is_unsettled`] が今度は逆向きに自分の doc と食い違い、
    /// #1079 が主題にしたのと同じ種類の不一致を良性の向きで作り直すことになる（費用も、**未反映が
    /// 無いまま往復したときの最初の Enter へ**同期 `engine.search` を 1 回乗せる形で実在する——
    /// 未反映が在る往復では採用案も同じ flush を撃つので差が出ない）。
    restored_rows_stale: bool,
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
            dispatch: SearchDispatch::default(),
            restored_rows_stale: false,
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

    /// **行の差し替えの単一チョークポイント**（#1039）。`results` の代入・`selected` のクランプ・
    /// 世代の前進（#699）・in-flight の失効・[`SearchState::restored_rows_stale`] のクリア（#1079）を
    /// 必ず同時に行う。**最後の 1 つをここへ置いたのは、`reset` がこのメソッドを通るからである**
    /// ——show を跨ぐクリアが構造で付いてくるので、show を跨ぐ状態を足しておいて
    /// `consume_reset_pending` の一覧へ入れ忘れる形の残余を作らずに済む。
    ///
    /// **同じ出来事に対する 2 つの義務をここへ合流させたのが #1039 である**——同期で行を差し替えた
    /// 以上、飛んでいる worker の結果は必ず古い。呼び出し側が [`SearchDispatch::invalidate`] を
    /// 書く規約だった頃は 11 箇所の手書きと 3 箇所の漏れがあった。
    ///
    /// **ここで `view_kind()` を読んではならない**——読ませると経路ごとに意味が変わる。呼ぶ側は
    /// 既に自分がどの遷移の途中かを知っている（`enter_tool` は `self.tool` を `Some` にした後で
    /// 呼ぶ）。
    ///
    /// **`results` を差し替える経路はこのメソッドに収束する。** `enter_tool` の
    /// `std::mem::take(&mut self.results)` だけは外にあるが、それは frame への**退避**であって
    /// 差し替えではなく、直後に必ずここを通る（take が空 Vec を残すので代入と衝突しない）。
    ///
    /// 却下した代替案（公開 `apply_rows(RowOrigin, ..)` など）は
    /// `docs/adr/ADR-row-replacement-choke-point.md` が正本。
    fn put_rows(&mut self, rows: Vec<SearchResult>, next_selected: usize) {
        self.results = rows;
        self.selected = clamp_selected(self.results.len(), next_selected);
        self.rows_generation += 1;
        self.dispatch.invalidate();
        // 行が差し替わった以上、いま在るのは「復帰した行」ではない（#1079）。**立てる側は
        // `on_escape` の folder 枝だけで、そこは必ずこの後に立て直す**（順序に意味がある）。
        self.restored_rows_stale = false;
    }

    /// 結果を差し替える。選択は範囲内へクランプ（空なら 0）。世代を進め、in-flight を失効させる
    /// （どちらも [`SearchState::put_rows`] が行う）。
    pub fn set_results(&mut self, results: Vec<SearchResult>) {
        let keep = self.selected;
        self.put_rows(results, keep);
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
    ///
    /// **返るのは folder token であって dispatch seq ではない**——照合先は
    /// [`SearchState::accept_folder_result`] であり、[`SearchState::issue_search`] が返す seq
    /// （照合先は [`SearchState::accept_worker_rows`]）とは別の量である。**取り違えても型・テスト・
    /// smoke は通る**（どちらも `u64`）。newtype を採らないのは、取り違えが「folder の行が永久に
    /// 出ない」という目に見える形で現れるためである（受容する残余・#1039）。
    ///
    /// **行は差し替えない**（frame へ退避するだけ）ので [`SearchState::put_rows`] を通らず、
    /// 世代も in-flight も動かさない。**Folder ビュー中の遅着 worker 結果を落とすのは
    /// [`SearchState::accept_worker_rows`] の view ガードである。**
    ///
    /// **`armed` を受けて [`SearchState::is_unsettled`] の合成をこの型の内側で行う**（#1079）。
    /// 呼び出し側に `is_unsettled(armed)` を書かせる形は採らない——誤って `armed` だけを渡しても
    /// 型は通り、`SearchState` のユニットテストは値を直接渡すので**その誤配線を検知できない**。
    /// `armed` だけを受ければ、呼び出し側の義務は `is_unsettled` の既存の呼び出し
    /// （`LauncherController::on_enter`）と同じ形のままである。**なぜ突入時点で控えるかは
    /// [`FolderFrame::unsettled_at_entry`] の doc が正本。**
    pub fn enter_folder(&mut self, dir: String, armed: bool) -> u64 {
        // frame の構築より前に撃つ——`&mut self` の中で `&self` メソッドを呼ぶため。
        let unsettled_at_entry = self.is_unsettled(armed);
        self.folder = Some(FolderFrame {
            restore_query: self.query.clone(),
            restore_results: self.results.clone(),
            restore_selected: self.selected,
            current_dir: dir,
            unsettled_at_entry,
        });
        self.folder_filter.clear();
        self.selected = 0;
        self.folder_gen += 1;
        self.folder_gen
    }

    /// folder 内で親/子へ遷移（frame の current_dir を書き換え・push しない）。token を返す。
    ///
    /// **これも folder token であって dispatch seq ではない**（区別の理由は
    /// [`SearchState::enter_folder`] の doc が正本）。
    pub fn navigate_folder(&mut self, dir: String) -> u64 {
        if let Some(f) = self.folder.as_mut() {
            f.current_dir = dir;
        }
        self.folder_filter.clear();
        self.selected = 0;
        self.folder_gen += 1;
        self.folder_gen
    }

    /// folder 中の現在ディレクトリ（フルパス）。消費者は `view.rs` の入力欄プレースホルダで、
    /// フォルダ展開中の現在地を示す（#836・`SPEC.md`「6.7 フォルダ展開中の現在地表示」）。
    /// driver（`launcher_controller.rs`）は別途 `parent_dir()` 越しにも current_dir を使う。
    ///
    /// **「消費者はこれ 1 つ」とは書かない。** `#[allow(dead_code)]` を外したことでコンパイラが
    /// 証明するのは**消費者が 1 つ以上ある**ことだけで、2 つ目が増えても何も落ちない。
    ///
    /// **返るのは「遷移先」であって「列挙が完了した」ディレクトリではない。** `enter_folder` /
    /// `navigate_folder` がこの値を**同期で**書き、フォルダ列挙は後から非同期に届く。ゆえに列挙が
    /// 未着の間も、列挙に失敗した場合（`SPEC.md`「6.6 列挙失敗時」）も、0 件のときも、ここは
    /// 遷移先を返し続ける。**これは意図した挙動である**——「どこへ移ったか」を行の入れ替わりより
    /// 先に示すことが #836 の目的そのもの（#743 の誤読はこの窓で起きた）。
    ///
    /// **`view_kind() == ViewKind::Folder` なら必ず `Some` である。逆は成り立たない**——tool が
    /// folder の上に積まれた状態（`enter_tool` は folder frame を残す）では `Some` を返しつつ
    /// `view_kind()` は `Tool` になる。呼び出し側が「`Some` なら folder ビュー」と読むと、
    /// ツール選択中に folder の文言を描く。
    pub fn folder_current_dir(&self) -> Option<&str> {
        self.folder.as_ref().map(|f| f.current_dir.as_str())
    }

    /// folder 中の親ディレクトリ（ルート終端で None）。
    pub fn parent_dir(&self) -> Option<String> {
        self.folder
            .as_ref()
            .and_then(|f| compute_parent_dir(&f.current_dir))
    }

    /// driver は token を `enter_folder`/`navigate_folder` の戻り値から直接得るため、独立した
    /// getter としては未消費（#532 SU3 M2 Task 3）。
    #[allow(dead_code)]
    pub fn folder_gen(&self) -> u64 {
        self.folder_gen
    }

    /// 遅延到着した**フォルダ列挙**の結果を受理してよいか（tool 非表示 ∧ token 一致 ∧ folder 中）。
    /// **worker の検索結果は [`SearchState::accept_worker_rows`] が別に裁く**——`token` と `seq` は
    /// 別の量である（[`SearchState::enter_folder`] の doc）。
    /// tool 中は §18.5「検索結果が上書きされない」ため受理しない（driver は破棄でなく
    /// 保留せず捨てる——escape で folder へ戻れば次の打鍵/ナビが再ロードする）。
    pub fn accept_folder_result(&self, token: u64) -> bool {
        self.tool.is_none() && token == self.folder_gen && self.folder.is_some()
    }

    /// ツール選択へ突入（§18.4: 結果リストをツール一覧で置換）。現在ビュー（Results/Folder
    /// どちらでも）の表示状態を frame へ退避する。tools ≥ 2 の判定は driver 側
    /// （§18.3: ≤1 は通常 Enter と同一のため、そもそも呼ばれない）。driver からは
    /// `shift_activate` が呼ぶ（#532 SU3.5 Task 3）。
    pub fn enter_tool(
        &mut self,
        target_path: String,
        target_is_folder: bool,
        tools: Vec<OpenerTool>,
    ) {
        let rows: Vec<SearchResult> = tools
            .iter()
            .map(|t| SearchResult {
                name: t.name.clone(),
                path: t.exe.clone(),
                is_folder: false,
                is_error: false,
                icon: IconSource::FromPath,
            })
            .collect();
        self.tool = Some(ToolFrame {
            // 退避（`std::mem::take`）は `put_rows` より前に来る——take が空 Vec を残すので
            // 下の代入と衝突しない。
            restore_results: std::mem::take(&mut self.results),
            restore_selected: self.selected,
            target_path,
            target_is_folder,
            tools,
            launch_query: self.query.clone(),
            saved_folder_filter: self.folder_filter.clone(),
        });
        // ツール行への総入れ替え（#699）。`self.tool` を `Some` にした**後**に呼ぶ——
        // 遅着した worker 結果はこの失効で落ちる（#1039 が閉じた漏れの 1 つ）。
        self.put_rows(rows, 0);
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
    ///
    /// **folder 枝は復帰させた行の未反映を [`SearchState::restored_rows_stale`] へ立て直す**（#1079）。
    /// `→` / `←` で folder へ入る経路（`on_nav_keys` の 2 箇所）は `on_enter` を通らないので #631 の
    /// flush が走らず、[`SearchState::enter_folder`] は行を差し替えない（[`SearchState::put_rows`] を
    /// 通らない）ので in-flight が残る。ところが復帰でその `put_rows` を通った瞬間、`invalidate` が
    /// in-flight を**無条件に**落とす——復帰した行は**展開前の行**、すなわち復元した query の最終結果
    /// ではないのに、`armed == false ∧ pending == 0` が成立してしまう。ゆえに突入時点の値を frame から
    /// 戻す（**なぜ突入時が唯一の常に有効な観測点なのかは [`FolderFrame::unsettled_at_entry`] の
    /// doc が正本**）。
    ///
    /// **worker の結果が folder 中に届くことはこの並びの要件ではない。** #1079 の issue 本文と、
    /// #1039 当時のこの doc は [`SearchState::accept_worker_rows`] の view ガードが捨てる段を並びへ
    /// 含めていたが、`invalidate` は届いたかを見ずに pending を落とすので**遅着の有無によらず成立する**
    /// （2026-08-14 に検知器で実測）。当時「稀」を受容の根拠の一つに置けたのは、この過剰仕様のためである。
    ///
    /// **却下した修正案は 2 つある。** どちらも controller 側に処置を置くもので、
    /// **2026-08-14 時点で** `launcher_controller.rs` には `#[cfg(test)]` が 1 つも無く（`tests/` も
    /// tauri の test feature も無い）、**修正が効いたことを測る検知器を置けない**のが却下の理由である:
    /// (A) ここで `run_search()` を撃つ、(B) `on_nav_keys` の 2 箇所で flush を挟む。
    /// **#1079 の issue 本文が (A) の費用として書く「Escape のたびに同期 `engine.search` をフレームへ
    /// 乗せる」は現在のコードに当たらない**——`run_search_with` の Plain 腕は #1004 の worker 化以降
    /// **同期 `engine.search` を含まない**（`search_tx.send` か、空クエリ・`indexing()`・送信失敗での
    /// `set_results` のいずれかである）。同期 `engine.search` が残るのは `on_enter` の flush だけである。
    /// 実際の費用は Plain 腕が `indexing()` 中に復帰行を空にすることであって、doc が名指して
    /// いたものではない。**`run_search` 入口の `instant_prefix` が `engine.lock()` を取ることも
    /// 費用に数えていたが、#1076 でその読みが `read_config` へ移り消えた。**
    pub fn on_escape(&mut self) -> EscapeOutcome {
        if let Some(t) = self.tool.take() {
            // query は復元しない（tool 中は入力無効で不変・ToolFrame doc 参照）
            self.folder_filter = t.saved_folder_filter;
            // 退避行への総入れ替え（#699）＋ in-flight の失効（#1039）
            self.put_rows(t.restore_results, t.restore_selected);
            return EscapeOutcome::RestoredFromTool;
        }
        if let Some(f) = self.folder.take() {
            self.query = f.restore_query;
            self.folder_filter.clear();
            self.folder_gen += 1; // 離脱経路でも失効
            // 退避行への総入れ替え（#699）＋ in-flight の失効（#1039）
            self.put_rows(f.restore_results, f.restore_selected);
            // **`put_rows` の後に立てる**（#1079）——`put_rows` が無条件に false へ戻すため、
            // 逆順にすると立てた瞬間に消える。復帰させた行が復元 query の最終結果かどうかを
            // 常に観測できる点は突入時だけなので、frame に控えた値をそのまま戻す。
            self.restored_rows_stale = f.unsettled_at_entry;
            EscapeOutcome::RestoredSearch
        } else {
            // Hide 経路は results を触らないので世代も進めない
            EscapeOutcome::Hide
        }
    }

    /// resetForShow 相当。show のたびに driver が呼ぶ。folder モードも解除し gen を進める
    /// （hide 前の in-flight ナビ結果を再表示後に轢かせない）。
    ///
    /// **instant / command モードに対応するフィールドは無い**——どちらも `interpret` が
    /// query と prefix から毎回導出する値ゆえ、`query.clear()` の帰結として落ちる
    /// （§19.7「ホットキーによる再表示でインスタントコマンドモードはリセットされる」の実体）。
    pub fn reset(&mut self) {
        self.query.clear();
        self.folder = None;
        self.folder_filter.clear();
        self.folder_gen += 1;
        self.tool = None;
        // 空行への総入れ替え（#699）——hide を跨いだ stale クリックはこれで失効する。
        // hide を跨いだ in-flight もここで失効する（#1039。以前は呼び出し側の
        // `consume_reset_pending` が別に撃っていた）。
        // 旧実装の `results.clear()` と違い確保を解放するが、**その確保には受益者が最初から
        // 居なかった**——次に行が入るのは `put_rows` の `self.results = rows` で、Vec ごと
        // 置き換えるため保っていた容量はそこで必ず捨てられる。挙動差は無い。
        self.put_rows(Vec::new(), 0);
    }

    /// 新しい検索要求へ seq を振る（controller が worker へ送る値）。
    ///
    /// **これは dispatch seq であって folder token ではない**——[`SearchState::enter_folder`] /
    /// [`SearchState::navigate_folder`] が返す `u64` は別の量で、消費者も
    /// [`SearchState::accept_folder_result`] と別である。取り違えても型は通る（受容する残余）。
    pub fn issue_search(&mut self, key_at: Instant, now: Instant) -> u64 {
        self.dispatch.issue(key_at, now)
    }

    /// worker の結果を採り込む。**seq が現 pending と一致し、かつ Results ビューにいるときだけ**
    /// 行を差し替える（#1004 / #1039）。
    ///
    /// **順序は `accept` が先である。** view ガードを前に置くと、Folder ビューで届いた結果が
    /// pending を残したまま捨てられ、生成 1 : 破棄 0 の終端ができる。seq 不一致のときだけ pending を
    /// 保つ——より新しい要求がまだ飛んでいるからである。
    ///
    /// **`None` の理由は 2 つあり、ここでは区別しない**（返り値は同じ）。呼び出し側が診断の
    /// ために区別したいなら、呼ぶ**前**に [`SearchState::pending_seq`] を読んで
    /// `pending_seq() == seq` かを控えておく——真なら view ガード、偽なら追い越しか同期差し替えである。
    ///
    /// **view ガードが production で発火するのは Folder ビューだけである。** Tool 側は
    /// [`SearchState::enter_tool`] が [`SearchState::put_rows`] を通って in-flight を失効させ、
    /// Tool ビュー中は新しい要求も出ない（`run_search_with` の `ViewKind::Tool` 腕が no-op）ため、
    /// `accept` が成功したうえで `view_kind() == Tool` になる並びが存在しない。**Tool 枝は
    /// 防御として残してあるだけである**——`enter_tool` が `put_rows` を通らなくなれば生きる。
    #[must_use = "落とすと計時 trace（egui_search:settled）の since_key_us / since_dispatch_us を導く材料が消え、打鍵起点・dispatch 起点の経過が観測できなくなる（#1004）"]
    pub fn accept_worker_rows(
        &mut self,
        seq: u64,
        rows: Vec<SearchResult>,
        now: Instant,
    ) -> Option<Settled> {
        let settled = self.dispatch.accept(seq, now)?;
        if self.view_kind() != ViewKind::Results {
            // 遷移前の要求である。`accept` が take 済みゆえ、ここで既に失効している。
            return None;
        }
        let keep = self.selected;
        // ここでの `invalidate`（`put_rows` の内側）は no-op である——`accept` が take 済み。
        self.put_rows(rows, keep);
        Some(settled)
    }

    /// 現在 in-flight の検索要求の seq（無ければ 0）。**消費者は trace の payload と、その
    /// payload の理由づけ（`None` の 2 種の切り分け）だけである。**
    pub fn pending_seq(&self) -> u64 {
        self.dispatch.pending_seq()
    }

    /// **最終クエリの結果がまだ行へ反映されていないか**（#1038。#1039 で `search_dispatch.rs` の
    /// 自由関数からここへ移設）。[`should_flush_on_enter`] の第 3 引数がこれである。
    ///
    /// **`armed` だけでは足りない。** 同期実装の頃は `armed == false` が「反映済み」を含意して
    /// いたが、worker 化（#1004）で trailing 発火の直後は必ず「`armed == false` かつ in-flight
    /// あり」になり、含意が壊れた。
    ///
    /// **`armed` を残すのは、最新クエリの要求がまだ出ていない間を覆うためである。**
    /// `pending_seq == 0` だが未反映、という状態はバースト継続中に直前の seq が `accept` 済みの
    /// フレーム（[`crate::egui_shell::layout::Debouncer::on_input`] は armed を立てるだけで発行せず、
    /// **打鍵のフレームで**要求を出すのは leading だけである——trailing は
    /// [`crate::egui_shell::layout::Debouncer::poll`] 経由で別に出す）と、空クエリ・`indexing`・
    /// 送信失敗が行を同期で差し替えた（＝ [`SearchState::put_rows`] が in-flight を失効させた）後に
    /// 打鍵が armed だけ残した状態で生じる。**これらを名指すのは、「打鍵したフレームの一瞬」と
    /// 誤読されるのを防ぐためである**——どちらも複数フレームにわたって続き、解けるのは trailing が
    /// 発火するか（[`crate::egui_shell::layout::Debouncer::poll`]・**最後の**打鍵から interval）、
    /// `cancel` か行の差し替えが起きたときである。**打鍵が続く限り armed は立ったままなので、
    /// 持続はバーストの長さで決まる。**
    ///
    /// **`armed` は引数で受ける。** [`crate::egui_shell::layout::Debouncer`] は
    /// [`crate::egui_shell::launcher_controller::LauncherController`] のフィールドであり、純粋核の
    /// この型からは見えない。**ゆえにこの合成は機構ではなく規約のままである**——正しい `armed` を
    /// 渡すのは呼び出し側の責務で、型が構造的に所有しているのは in-flight の側（`pending_seq`）だけ
    /// である。#1039 が規約から機構へ移したのも in-flight の失効に限る。
    ///
    /// **armed が下りる経路はメソッドだけではない。**
    /// [`crate::egui_shell::launcher_controller::LauncherController::consume_reset_pending`] は show の
    /// たびに [`crate::egui_shell::layout::Debouncer`] を丸ごと作り直すため、`poll` も `cancel` も
    /// 通らずに armed が落ちる（[`crate::egui_shell::layout::Debouncer::new`] は `armed: false` で
    /// 作る）。**`armed` 側の状態をこの型へ移すなら、reset 経路で落とす責務も一緒に移ること**——
    /// いま取り残しが起きないのは `Debouncer` ごと捨てているからであり、フィールドだけを移した瞬間に
    /// その救いは消える。対で動く `last_input_at`（同じ `impl` のフィールド）は
    /// `consume_reset_pending` が触らないので show を跨いで古いまま残っており、**今それが無害なのは
    /// `armed` が false だからである**——`armed` を移して reset を忘れると、show 直後の最初の
    /// フレームで `poll` が「隠れていた時間」を経過として読み、その場で trailing を撃つ。**同型の
    /// 残余（show を跨ぐ状態を足して `consume_reset_pending` の一覧へ入れ忘れる形。検知手段が無いこと
    /// まで）と、その解き方の先例は [`crate::egui_shell::lifecycle::BlurGrace::reset`] の doc が正本で
    /// ある**（#745。`*self = Self::default()` を `consume_reset_pending` から呼ぶ形で、フィールド代入
    /// ではなくメソッドで丸ごと畳む）。
    ///
    /// **第 3 の disjunct `restored_rows_stale` は folder の往復を跨いで未反映を運ぶ**（#1079）。
    /// `armed` と `pending_seq` はどちらも folder 突入から Escape までの間に落ちうる——前者は
    /// folder 中の trailing poll で、後者は `on_escape` 自身の [`SearchState::put_rows`] が呼ぶ
    /// `invalidate` で——ため、突入時点の値を [`FolderFrame::unsettled_at_entry`] へ控えて
    /// 復帰時に立て直す。**この disjunct だけは `armed` と違い機構である**——立てる場所も落とす
    /// 場所もこの型の内側にあり、[`SearchState::reset`] が `put_rows` を通ることで show を跨ぐ
    /// クリアまで構造で付いてくる（上段が `armed` について警告している懸念は当たらない）。
    ///
    /// **否定形で置いたのは呼び出し点に `!` を出さないためである**（#1039 の issue 本文が想定する
    /// 肯定形 `is_settled()` とは極性が逆。極性を反転すると
    /// [`crate::egui_shell::results_view::RowsSnapshot::input_idle`] の doc が持つ導出——
    /// 食い違うのは `armed == false` かつ残る 2 つの disjunct のいずれかが立つとき——を
    /// 式ごと書き直す必要が生じる）。
    pub fn is_unsettled(&self, armed: bool) -> bool {
        armed || self.dispatch.pending_seq() != 0 || self.restored_rows_stale
    }
}

impl Default for SearchState {
    fn default() -> Self {
        Self::new()
    }
}

/// 選択 index を [0, len) へクランプ（len==0 は 0）。folderNav.clampSelectedIndex 相当。
pub(crate) fn clamp_selected(len: usize, idx: usize) -> usize {
    if len == 0 { 0 } else { idx.min(len - 1) }
}

/// Enter 時の flush 要否（#631 flush-on-Enter・SolidJS flushPendingRefresh 同型）。
///
/// **`unsettled` は「最終クエリの結果がまだ行へ反映されていないか」である**（#1038）。その中身と経緯は [`SearchState::is_unsettled`] の doc が正本である（#1039 で `search_dispatch.rs` の自由関数から移設）。
///
/// unsettled になるのは Results∧Plain 経路のみ（folder=同期・instant/command=cancel + 同期で行を差し替え済み＝[`SearchState::put_rows`] が in-flight を失効させている）だが、将来の経路追加に対して条件を独立に固定する（誤発火の構造的防止・spec C 節）。
pub fn should_flush_on_enter(view_kind: ViewKind, is_plain: bool, unsettled: bool) -> bool {
    view_kind == ViewKind::Results && is_plain && unsettled
}

/// 親ディレクトリを返す。ルート（`C:\` / `\\server\share\`）で None。folderNav.computeParentDir 相当。
/// 入力ではスラッシュとバックスラッシュの混在を許容し、返り値はバックスラッシュへ統一する。
pub(crate) fn compute_parent_dir(current_dir: &str) -> Option<String> {
    // config の scan path は原表記を保持するため、OS が返す区切りとの混在をここで正規化する。
    let canonical = current_dir.replace('/', "\\");
    // 末尾 `\` を剥がす（ただしドライブルート "X:\" は保持しない — 後段で判定）。
    let normalized = if canonical.len() > 3 && canonical.ends_with('\\') {
        &canonical[..canonical.len() - 1]
    } else {
        &canonical
    };
    // UNC ルート判定: \\server\share（2 セグメント以下）は終端。
    if let Some(rest) = normalized.strip_prefix("\\\\") {
        let parts: Vec<&str> = rest
            .trim_end_matches('\\')
            .split('\\')
            .filter(|p| !p.is_empty())
            .collect();
        if parts.len() <= 2 {
            return None;
        }
    }
    // 最後の `\segment` を削る。
    let cut = normalized.rfind('\\')?;
    let mut parent = normalized[..cut].to_string();
    // "X:" → "X:\"（ドライブルートは末尾 `\` 必須）。
    if parent.len() == 2
        && parent.as_bytes()[1] == b':'
        && parent.as_bytes()[0].is_ascii_alphabetic()
    {
        parent.push('\\');
    }
    if parent.is_empty() || parent == normalized {
        return None;
    }
    // \\server（share 未満）は終端。
    if let Some(rest) = parent.strip_prefix("\\\\") {
        let parts: Vec<&str> = rest
            .trim_end_matches('\\')
            .split('\\')
            .filter(|p| !p.is_empty())
            .collect();
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
///
/// **表示だけでなく起動もこの述語で決める**（#1077）。`view.rs` の表示ゲート（`present_results` の
/// 連言③）と、`launcher_controller.rs` の `activate_or_execute` / `shift_activate` の起動ガードが
/// **同じ述語を共有する**——別式を書けば「画面に出ていない行を Enter が起動する」が再び生まれる
/// （2026-08-16 に実機再現済み）。**行を消すのはこの述語の仕事ではない**: §4.7 は
/// 「データと選択は保持——クリアしない」と定めており、起動側は不可逆な起動だけを止める。
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
    use std::time::Duration;

    use super::*;

    #[test]
    fn plain_query_is_plain() {
        assert_eq!(
            interpret("firefox", "@", ViewKind::Results),
            QueryIntent::Plain
        );
    }

    #[test]
    fn flush_on_enter_only_for_unsettled_plain_results() {
        assert!(should_flush_on_enter(ViewKind::Results, true, true));
        assert!(
            !should_flush_on_enter(ViewKind::Results, true, false),
            "反映済みなら flush 不要"
        );
        assert!(
            !should_flush_on_enter(ViewKind::Results, false, true),
            "instant/command では flush しない"
        );
        assert!(
            !should_flush_on_enter(ViewKind::Folder, true, true),
            "folder は同期フィルタ"
        );
        assert!(
            !should_flush_on_enter(ViewKind::Tool, true, true),
            "tool 中は検索自体が凍結"
        );
    }

    #[test]
    fn slash_prefix_is_command() {
        assert_eq!(
            interpret("/r", "@", ViewKind::Results),
            QueryIntent::Command
        );
    }

    #[test]
    fn instant_prefix_without_space() {
        assert_eq!(
            interpret("@goog", "@", ViewKind::Results),
            QueryIntent::Instant {
                filter_name: "goog".into(),
                instant_query: String::new()
            }
        );
    }

    #[test]
    fn instant_prefix_with_space_splits_filter_and_query() {
        assert_eq!(
            interpret("@google rust egui", "@", ViewKind::Results),
            QueryIntent::Instant {
                filter_name: "google".into(),
                instant_query: "rust egui".into()
            }
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
            QueryIntent::Instant {
                filter_name: "a".into(),
                instant_query: String::new()
            }
        );
    }

    fn res(name: &str) -> SearchResult {
        SearchResult {
            name: name.into(),
            path: format!("C:/{name}.exe"),
            is_folder: false,
            is_error: false,
            icon: IconSource::FromPath,
        }
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
        assert_eq!(
            s.rows_generation(),
            g + 1,
            "ツール行への総入れ替えで進むこと"
        );
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
        s.enter_folder("C:/dir".into(), false); // 退避のみ（results は差し替えない）
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
        s.enter_folder("C:/dir".into(), false);
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
        assert_eq!(
            s.rows_generation(),
            g + 1,
            "reset は hide を跨いだ stale クリックを失効させる"
        );
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

    // ---- 行の差し替えと in-flight の失効（#1039）----
    //
    // **どのテストも「`None` を返すこと」と「行が変わっていないこと」を対で assert する。**
    // 前者だけでは、seq が不一致でも `rows` を代入してしまう実装ミスが素通りする
    // （`/plan-review` Step 2b の U-5）。

    /// 遅着した worker 結果の材料。`accept_worker_rows` へ渡す行は必ずこれを使い、
    /// 「上書きされなかったこと」を行の中身で見分けられるようにする。
    fn late_rows() -> Vec<SearchResult> {
        vec![res("late-from-worker")]
    }

    fn names(s: &SearchState) -> Vec<String> {
        s.results().iter().map(|r| r.name.clone()).collect()
    }

    #[test]
    fn late_worker_rows_do_not_overwrite_tool_rows() {
        let base = Instant::now();
        let mut s = SearchState::new();
        s.set_results(vec![res("a")]);
        let seq = s.issue_search(base, base);
        s.enter_tool("C:/t.txt".into(), false, make_tools());
        let before = names(&s);
        assert!(
            s.accept_worker_rows(seq, late_rows(), base).is_none(),
            "tool 突入で in-flight は失効している（#1039 が閉じた漏れ）"
        );
        assert_eq!(names(&s), before, "ツール行が上書きされていないこと");
    }

    #[test]
    fn late_worker_rows_do_not_overwrite_restored_rows_after_escape() {
        let base = Instant::now();
        // (1) tool 復帰の枝
        let mut s = SearchState::new();
        s.set_results(vec![res("a"), res("b")]);
        s.enter_tool("C:/t.txt".into(), false, make_tools());
        let seq = s.issue_search(base, base);
        assert_eq!(s.on_escape(), EscapeOutcome::RestoredFromTool);
        let before = names(&s);
        assert!(
            s.accept_worker_rows(seq, late_rows(), base).is_none(),
            "tool からの復帰で in-flight は失効している"
        );
        assert_eq!(names(&s), before, "復帰した行が上書きされていないこと");

        // (2) folder 復帰の枝。片方だけ直すと穴が残る。
        let mut s = SearchState::new();
        s.set_results(vec![res("a"), res("b")]);
        s.enter_folder("C:/dir".into(), false);
        s.set_results(vec![res("in-dir")]);
        let seq = s.issue_search(base, base);
        assert_eq!(s.on_escape(), EscapeOutcome::RestoredSearch);
        let before = names(&s);
        assert!(
            s.accept_worker_rows(seq, late_rows(), base).is_none(),
            "folder からの復帰で in-flight は失効している"
        );
        assert_eq!(names(&s), before, "展開前の行が上書きされていないこと");
    }

    /// view 種別ガード（#1039 決定 3）。`enter_folder` は行を差し替えないので `put_rows` を
    /// 通らない——**失効させるのは採り込み側のガードである**。
    #[test]
    fn late_worker_rows_are_dropped_in_folder_view() {
        let base = Instant::now();
        let mut s = SearchState::new();
        s.set_results(vec![res("a")]);
        let seq = s.issue_search(base, base);
        s.enter_folder("C:/dir".into(), false);
        let before = names(&s);
        assert!(
            s.accept_worker_rows(seq, late_rows(), base).is_none(),
            "Folder ビュー中は遅着 worker 結果を採らない"
        );
        assert_eq!(names(&s), before, "folder の行が上書きされていないこと");
    }

    /// `navigate_folder` の窓も同じガードが閉じる（`enter_folder` と別の呼び出し点である）。
    #[test]
    fn late_worker_rows_are_dropped_after_navigate_folder() {
        let base = Instant::now();
        let mut s = SearchState::new();
        s.enter_folder("C:/dir".into(), false);
        s.set_results(vec![res("in-dir")]);
        let seq = s.issue_search(base, base);
        s.navigate_folder("C:/dir/sub".into());
        let before = names(&s);
        assert!(
            s.accept_worker_rows(seq, late_rows(), base).is_none(),
            "ナビ後も Folder ビューゆえ採らない"
        );
        assert_eq!(names(&s), before, "行が上書きされていないこと");
    }

    #[test]
    fn late_worker_rows_do_not_survive_reset() {
        let base = Instant::now();
        let mut s = SearchState::new();
        s.set_results(vec![res("a")]);
        let seq = s.issue_search(base, base);
        s.reset();
        assert!(
            s.accept_worker_rows(seq, late_rows(), base).is_none(),
            "hide を跨いだ in-flight は show 後の行を汚さない"
        );
        assert!(
            s.results().is_empty(),
            "reset 後の空行が上書きされていないこと"
        );
    }

    /// `stale_result_is_dropped_after_synchronous_replacement`（`search_dispatch.rs`）の
    /// `SearchState` 版。**同期で差し替えた以上、飛んでいる結果は必ず古い。**
    #[test]
    fn sync_replacement_invalidates_in_flight() {
        let base = Instant::now();
        let mut s = SearchState::new();
        // `c:\u` を打って worker が走り出した
        let seq = s.issue_search(base, base);
        // クエリを空にした → 同期でクリアする出所が `put_rows` を通る
        s.set_results(Vec::new());
        let before = names(&s);
        assert!(
            s.accept_worker_rows(seq, late_rows(), base + Duration::from_millis(20))
                .is_none(),
            "空クエリの下に古い行が生え直してはならない"
        );
        assert_eq!(names(&s), before, "空行が上書きされていないこと");
    }

    /// Results ビューで seq が一致する結果は**採る**。上の 6 件（採らない側）と対で意味を持つ
    /// ——ガードを広げすぎて全部落とす実装が素通りしないようにする。
    #[test]
    fn matching_worker_rows_are_accepted_in_results_view() {
        let base = Instant::now();
        let mut s = SearchState::new();
        s.set_results(vec![res("old")]);
        let g = s.rows_generation();
        let seq = s.issue_search(base, base);
        assert!(
            s.accept_worker_rows(seq, late_rows(), base).is_some(),
            "Results ビューで seq が一致すれば採る"
        );
        assert_eq!(names(&s), vec!["late-from-worker".to_string()]);
        assert_eq!(s.rows_generation(), g + 1, "採り込みも行の差し替えである");
    }

    /// 移設（`search_dispatch.rs` → ここ・#1039）。真理値表 4 件 + `should_flush_on_enter`
    /// との合成を固定する。**`armed` の disjunct を落とす変異でここが落ちる**（#1038 の明示要求）。
    #[test]
    fn unsettled_covers_in_flight_after_trailing_fired() {
        let base = Instant::now();
        // 予約も in-flight も無い
        let mut s = SearchState::new();
        assert!(
            !s.is_unsettled(false),
            "予約も in-flight も無ければ反映済み"
        );
        assert!(
            s.is_unsettled(true),
            "最新クエリの要求がまだ出ていない（バースト継続中・同期差し替え直後の打鍵）"
        );
        // in-flight あり。**`SearchState` を実際に遷移させて作る**（リテラルの seq を渡さない）。
        let _ = s.issue_search(base, base);
        assert!(
            s.is_unsettled(false),
            "trailing 発火の直後（armed == false かつ in-flight あり）が #1038 の欠陥そのものである"
        );
        assert!(s.is_unsettled(true), "両方立っていても未反映");
        // #1038 の受け入れ 1 を逐語で写す（`should_flush_on_enter` との合成まで固定する）。
        assert!(should_flush_on_enter(
            ViewKind::Results,
            true,
            s.is_unsettled(false),
        ));
    }

    /// 移設（`unsettled_is_grounded_on_real_dispatch`・#1039）。sentinel（0 = in-flight なし）を
    /// リテラルで書かず状態遷移から作る——判定の入力が出力側へすり替わる不動点化を避けるため、
    /// `armed` は false 固定で渡す。
    #[test]
    fn unsettled_is_grounded_on_real_dispatch() {
        let base = Instant::now();
        let mut s = SearchState::new();
        assert!(!s.is_unsettled(false), "発行前は反映済み");
        let seq = s.issue_search(base, base);
        assert!(s.is_unsettled(false), "worker へ出した直後は未反映");
        assert!(s.accept_worker_rows(seq, late_rows(), base).is_some());
        assert!(!s.is_unsettled(false), "採り込めば反映済みへ戻る");
        let _ = s.issue_search(base, base);
        s.set_results(Vec::new());
        assert!(!s.is_unsettled(false), "同期で差し替えた後も反映済みである");
    }

    /// #1079 の検知器。**folder を往復すると述語が自分の意味に反して偽を返す**——`enter_folder` は
    /// 行を差し替えない（[`SearchState::put_rows`] を通らない）ので in-flight が残り、`on_escape` の
    /// folder 枝が復帰させる行は**展開前の行**、すなわち復元した query の最終結果ではない。それなのに
    /// `put_rows` の `invalidate` が in-flight を**無条件に**落とすため `pending_seq == 0` になる。
    ///
    /// **worker の結果が folder 中に届くことは要件ではない。** #1079 の issue 本文と `on_escape` の
    /// doc はその段を並びに含めるが、`invalidate` は届いたかを見ずに `pending = None` を代入する
    /// （`search_dispatch.rs`）ため、遅着の有無によらずこの 3 段で成立する。
    #[test]
    fn unsettled_survives_a_folder_round_trip() {
        let base = Instant::now();
        let mut s = SearchState::new();
        s.set_results(vec![res("stale-row")]);
        // 打鍵 → worker へ発行。行はまだこの要求より前のものである。
        let _ = s.issue_search(base, base);
        // `→` / `←` で folder 突入（`on_enter` を通らないので flush が走らない）。
        s.enter_folder("C:/dir".into(), false);
        // Escape で展開前の行へ復帰する。
        assert_eq!(s.on_escape(), EscapeOutcome::RestoredSearch);
        assert_eq!(
            names(&s),
            vec!["stale-row".to_string()],
            "復帰したのは展開前の行であり、復元した query の最終結果ではない"
        );
        assert!(
            s.is_unsettled(false),
            "復帰行が最終クエリの結果でない以上、述語は真でなければならない（#1079）"
        );
    }

    /// #1079: **突入時に捕まえるのは `pending_seq` だけではない。** [`SearchState::enter_folder`] が
    /// [`SearchState::is_unsettled`] を丸ごと撃つ以上、debounce が予約を持ったまま（trailing 未発火で
    /// in-flight がまだ無い）突入した場合も捕まる。**`armed` の項を落とす変異——`enter_folder` の合成を
    /// `self.dispatch.pending_seq() != 0` に書き換える——でここが落ちる**（それ以外の #1079 のテストは
    /// 全部緑のまま通ることを 2026-08-14 に実測した。`is_unsettled` 自身の `armed` を守る
    /// `unsettled_covers_in_flight_after_trailing_fired` は、合成の呼び出し側であるここを覆わない）。
    #[test]
    fn folder_entry_captures_armed_not_only_in_flight() {
        let mut s = SearchState::new();
        s.set_results(vec![res("stale-row")]);
        assert!(!s.is_unsettled(false), "in-flight はまだ無い");
        // trailing 未発火のまま `→` を押す（`armed` だけが立っている）。
        s.enter_folder("C:/dir".into(), true);
        assert_eq!(s.on_escape(), EscapeOutcome::RestoredSearch);
        assert!(
            s.is_unsettled(false),
            "予約だけの未反映も folder の往復を跨いで運ばれる"
        );
    }

    /// #1079 の境界 #2: **過剰近似しない。** in-flight も予約も無いまま folder を往復したなら、
    /// 復帰する行は復元 query の最終結果そのものである。ここで真を返す実装（「folder から
    /// 戻ったら無条件に立てる」）は、述語を逆向きに doc と食い違わせる。
    #[test]
    fn folder_round_trip_without_in_flight_stays_settled() {
        let mut s = SearchState::new();
        s.set_results(vec![res("fresh-row")]);
        s.enter_folder("C:/dir".into(), false);
        assert_eq!(s.on_escape(), EscapeOutcome::RestoredSearch);
        assert!(
            !s.is_unsettled(false),
            "未反映が無いまま往復したなら復帰行は最終結果である"
        );
    }

    /// #1079 の境界 #3 / #4: **行が実際に差し替われば落ちる。** clear を
    /// [`SearchState::put_rows`] へ置いた帰結であり、`reset` が覆われるのはそれが `put_rows` を
    /// 通るからである（`consume_reset_pending` の一覧へ入れ忘れる形の残余を作らない）。
    #[test]
    fn restored_stale_clears_on_any_row_replacement() {
        let base = Instant::now();
        // (3) 同期の差し替えで落ちる
        let mut s = SearchState::new();
        let _ = s.issue_search(base, base);
        s.enter_folder("C:/dir".into(), false);
        let _ = s.on_escape();
        assert!(s.is_unsettled(false), "復帰直後は未反映");
        s.set_results(vec![res("fresh")]);
        assert!(!s.is_unsettled(false), "行を差し替えたら反映済みへ戻る");

        // (4) show を跨ぐクリア（`reset` は `put_rows` を通る）
        let mut s = SearchState::new();
        let _ = s.issue_search(base, base);
        s.enter_folder("C:/dir".into(), false);
        let _ = s.on_escape();
        assert!(s.is_unsettled(false));
        s.reset();
        assert!(!s.is_unsettled(false), "reset-on-show を跨いで残らない");
    }

    /// #1079 の境界 #5: [`SearchState::navigate_folder`] は frame を作り直さず `current_dir` を
    /// 書き換えるだけなので、突入時に控えた値は folder 内の深掘り・親移動を跨いで生き続ける。
    #[test]
    fn navigate_folder_preserves_the_captured_unsettled() {
        let base = Instant::now();
        let mut s = SearchState::new();
        let _ = s.issue_search(base, base);
        s.enter_folder("C:/dir".into(), false);
        s.navigate_folder("C:/dir/child".into());
        s.navigate_folder("C:/dir".into());
        let _ = s.on_escape();
        assert!(
            s.is_unsettled(false),
            "folder 内を動き回っても突入時点の未反映は失われない"
        );
    }

    /// #1079 の境界 #6: 復帰後に打鍵して worker へ出した場合、**フラグは残る**——
    /// `run_search_with` の Plain 腕は送信できたら行を保つ（`set_results` を呼ばない）ので
    /// [`SearchState::put_rows`] を通らず、そして行はまだ古いのだからそれが正しい。落ちるのは
    /// 結果が届いて行が実際に差し替わる瞬間である。
    #[test]
    fn restored_stale_survives_dispatch_and_clears_on_accept() {
        let base = Instant::now();
        let mut s = SearchState::new();
        let _ = s.issue_search(base, base);
        s.enter_folder("C:/dir".into(), false);
        let _ = s.on_escape();
        let seq = s.issue_search(base, base);
        assert!(
            s.is_unsettled(false),
            "dispatch しても行はまだ古い——フラグは残る"
        );
        assert!(s.accept_worker_rows(seq, late_rows(), base).is_some());
        assert!(
            !s.is_unsettled(false),
            "結果が届いて行が差し替わった瞬間に落ちる"
        );
    }

    /// #1079 × §18.5 の直交: **tool は folder の上に積まれる。** その状態の Escape は 2 回に
    /// 分かれ、1 回目（tool 枝）が [`SearchState::put_rows`] でフラグを落としても、2 回目
    /// （folder 枝）が frame から立て直す。**tool 枝は `self.tool.take()` で早期 return し
    /// `self.folder` に触らない**ので、控えた値は tool の出入りとは独立に生き残る。
    #[test]
    fn tool_stacked_on_folder_still_restores_the_captured_unsettled() {
        let base = Instant::now();
        let mut s = SearchState::new();
        let _ = s.issue_search(base, base);
        s.enter_folder("C:/dir".into(), false);
        s.enter_tool("C:/dir/t.txt".into(), false, make_tools());
        assert_eq!(
            s.on_escape(),
            EscapeOutcome::RestoredFromTool,
            "1 回目は tool から抜ける"
        );
        assert!(
            !s.is_unsettled(false),
            "tool 復帰は folder の中へ戻るだけなので、まだ立たない"
        );
        assert_eq!(s.on_escape(), EscapeOutcome::RestoredSearch);
        assert!(
            s.is_unsettled(false),
            "folder 枝が frame の控えを立て直す（tool の出入りで失われない）"
        );
    }

    /// 計時 trace（`egui_search:settled` の `since_key_us` / `since_dispatch_us`）の材料が
    /// 移設で変わっていないこと。`search_dispatch.rs` の `accepts_only_the_latest_seq` と
    /// 同じ値を返す（**移設は計器を壊してはならない**）。
    #[test]
    fn settled_timing_survives_the_move() {
        let base = Instant::now();
        let mut s = SearchState::new();
        let first = s.issue_search(base, base + Duration::from_millis(50));
        let second = s.issue_search(
            base + Duration::from_millis(60),
            base + Duration::from_millis(110),
        );
        assert!(second > first, "seq は単調に増える");
        assert!(
            s.accept_worker_rows(first, late_rows(), base + Duration::from_millis(120))
                .is_none(),
            "追い越された結果は採らない"
        );
        let settled = s
            .accept_worker_rows(second, late_rows(), base + Duration::from_millis(130))
            .expect("最新の seq は採る");
        assert_eq!(settled.since_key, Duration::from_millis(70), "打鍵起点");
        assert_eq!(
            settled.since_dispatch,
            Duration::from_millis(20),
            "dispatch 起点"
        );
    }

    /// `None` の 2 種（追い越し / view 不一致）を呼び出し側が切り分ける手順を固定する
    /// ——`drain_search` の `egui_search:dropped` が `"reason"` をこれで導く。**`accept_worker_rows`
    /// を呼ぶ前**に読むことが条件である（採り込みが pending を take するため）。
    #[test]
    fn pending_seq_separates_the_two_drop_reasons() {
        let base = Instant::now();
        // (1) view 不一致: 呼ぶ前の pending は当の seq と一致する
        let mut s = SearchState::new();
        let seq = s.issue_search(base, base);
        s.enter_folder("C:/dir".into(), false);
        assert_eq!(s.pending_seq(), seq, "view 不一致では pending が生きている");
        assert!(s.accept_worker_rows(seq, late_rows(), base).is_none());

        // (2) 追い越し: 新しい要求が pending を上書きしている
        let mut s = SearchState::new();
        let old = s.issue_search(base, base);
        let new = s.issue_search(base, base);
        assert_ne!(s.pending_seq(), old, "追い越された seq は pending でない");
        assert_eq!(s.pending_seq(), new);
        assert!(s.accept_worker_rows(old, late_rows(), base).is_none());

        // (3) 同期差し替えによる失効も「追い越し」側（pending == 0）へ落ちる
        let mut s = SearchState::new();
        let seq = s.issue_search(base, base);
        s.set_results(Vec::new());
        assert_eq!(s.pending_seq(), 0, "同期差し替えで失効している");
        assert!(s.accept_worker_rows(seq, late_rows(), base).is_none());
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
        assert_eq!(
            s.interp("@"),
            QueryIntent::Instant {
                filter_name: "g".into(),
                instant_query: String::new()
            }
        );
    }

    #[test]
    fn parent_dir_drive_and_unc_roots() {
        assert_eq!(compute_parent_dir("C:\\a\\b"), Some("C:\\a".to_string()));
        assert_eq!(compute_parent_dir("C:\\a"), Some("C:\\".to_string()));
        assert_eq!(compute_parent_dir("C:\\"), None); // ドライブルート終端
        assert_eq!(
            compute_parent_dir("\\\\srv\\share\\x"),
            Some("\\\\srv\\share".to_string())
        );
        assert_eq!(compute_parent_dir("\\\\srv\\share"), None); // UNC 共有ルート終端
        assert_eq!(compute_parent_dir("\\\\srv"), None); // UNC 不完全
    }

    #[test]
    fn parent_dir_normalizes_forward_and_mixed_separators() {
        assert_eq!(compute_parent_dir("C:/a/b"), Some("C:\\a".to_string()));
        assert_eq!(compute_parent_dir("C:/a\\b"), Some("C:\\a".to_string()));
        assert_eq!(compute_parent_dir("C:/a"), Some("C:\\".to_string()));
        assert_eq!(compute_parent_dir("C:/"), None); // `/` 表記のドライブルートも終端
        assert_eq!(
            compute_parent_dir("//srv/share/x"),
            Some("\\\\srv\\share".to_string())
        );
        assert_eq!(
            compute_parent_dir("\\\\srv/share\\x"),
            Some("\\\\srv\\share".to_string())
        );
        assert_eq!(compute_parent_dir("//srv/share"), None); // `/` 表記の UNC 共有ルートも終端
    }

    #[test]
    fn enter_folder_saves_view_and_switches_kind() {
        let mut s = SearchState::new();
        s.set_query("fire".into());
        s.set_results(vec![res("a"), res("b")]);
        s.move_selection(1); // selected=1
        assert_eq!(s.view_kind(), ViewKind::Results);
        let tok = s.enter_folder("C:\\proj".into(), false);
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
        let t1 = s.enter_folder("C:\\a".into(), false);
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
        let t0 = s.enter_folder("C:\\a\\b\\c".into(), false);
        let p1 = s.parent_dir().expect("孫フォルダからは親が取れる");
        assert_eq!(p1, "C:\\a\\b");
        let t1 = s.navigate_folder(p1);
        assert_eq!(
            s.view_kind(),
            ViewKind::Folder,
            "上昇しても folder のままである"
        );
        assert_eq!(s.folder_current_dir(), Some("C:\\a\\b"));
        let p2 = s.parent_dir().expect("さらに親が取れる");
        assert_eq!(p2, "C:\\a");
        let t2 = s.navigate_folder(p2);
        assert_eq!(s.folder_current_dir(), Some("C:\\a"));
        assert!(
            t0 < t1 && t1 < t2,
            "遷移ごとに token が進む（遅着の旧結果を失効させる）"
        );
    }

    /// ルート終端では親が無い（§6.2）。driver の `if let Some(parent)` がここで無反応に落ちる。
    #[test]
    fn left_at_root_has_no_parent() {
        let mut drive = SearchState::new();
        drive.enter_folder("C:\\".into(), false);
        assert_eq!(drive.parent_dir(), None, "ドライブルートは終端");
        let mut unc = SearchState::new();
        unc.enter_folder("\\\\srv\\share".into(), false);
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
        s.enter_folder("C:\\a\\b".into(), false);
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
        s.enter_folder("C:\\a".into(), false);
        s.set_results(vec![res("a"), res("b"), res("c"), res("d"), res("e")]); // 置換前の候補
        s.navigate_folder("C:\\a\\b".into()); // ← / → の打鍵: 選択は 0 へ・行はまだ古いまま
        s.move_selection(3); // ロード窓で ↑↓ が入る
        assert_eq!(s.selected(), 3);
        s.set_results(vec![res("a"), res("b"), res("c"), res("d")]);
        assert_eq!(s.selected(), 3, "範囲内なら保たれる（先頭へは戻さない）");
        s.set_results(vec![res("a"), res("b")]);
        assert_eq!(
            s.selected(),
            1,
            "範囲外は末尾へクランプされる（0 ではない）"
        );
    }

    /// 0 件が到着したときは選択の指す行が無い（§6.1 の 2 段目の末尾）。**上の「先頭へ戻さない」
    /// とは別の 0 である**——方針としてのリセットではなく、有効な index が存在しないための 0。
    #[test]
    fn arriving_empty_rows_leaves_no_selectable_row() {
        let mut s = SearchState::new();
        s.enter_folder("C:\\empty".into(), false);
        s.set_results(vec![res("a"), res("b"), res("c")]);
        s.move_selection(2);
        assert_eq!(s.selected(), 2);
        s.set_results(vec![]); // 空フォルダの列挙結果が到着
        assert_eq!(s.selected(), 0);
        assert!(s.results().is_empty(), "選択が 0 でも、それが指す行は無い");
    }

    // ---- フォルダ内の絞り込みと選択（`SPEC.md`「4.9 入力と選択」の as-built・#838 / #921）----
    //
    // **上の #743 ブロック（左右カーソルキーによる階層移動）の外に置く**——主題が違ううえ、
    // あちらの冒頭は本数を名指ししており、内側へ足すとその記述が stale になる。
    //
    // **射程は状態核だけである**——打鍵から `set_folder_filter` への配線（`view.rs` の
    // `changed()` エッジと `launcher_controller.rs` の `on_input_changed`）は射程外で、
    // その呼び出しを消してもこのテストは緑のままである（#743 ブロックの冒頭と同じ性質の限界）。

    /// フォルダ内の絞り込みの打鍵は選択を 1 行目へ戻す。正本は `SPEC.md`「4.9 入力と選択」で、
    /// 通常検索側も含めてそこが 1 か所で持つ（ここが測るのは folder 側の半分だけである）。
    /// **非ゼロから始める**——0 から始めると `enter_folder` の初期値と区別が付かない。
    /// **`enter_folder` を先に通す**——`set_folder_filter` は `folder` が `None` でも黙って
    /// `selected = 0` を撃つため、突入を忘れると folder 中の挙動を何も実証しない。
    ///
    /// **`SPEC.md`「6.3 フォルダ展開中の検索」の as-built（列挙失敗のエラー行が絞り込みの
    /// 対象外であること）は、ここでは測れない**——判定は `LauncherController::folder_error`
    /// を読む driver 側にあり `AppHandle` を要する（機構はそのフィールドの doc が持つ・
    /// #838 で受容した残余）。
    #[test]
    fn folder_filter_typing_resets_selection_to_first_row() {
        let mut s = SearchState::new();
        s.enter_folder("C:\\a".into(), false);
        s.set_results(vec![res("a"), res("b"), res("c")]);
        s.move_selection(2);
        assert_eq!(s.selected(), 2, "前提: 打鍵の前に選択は非ゼロである");
        s.set_folder_filter("b".into());
        assert_eq!(s.selected(), 0, "絞り込みの打鍵で選択は 1 行目へ戻る");
        assert_eq!(
            s.view_kind(),
            ViewKind::Folder,
            "folder 中の挙動を測っている（突入を忘れたテストは何も実証しない）"
        );
    }

    #[test]
    fn escape_folder_restores_then_hides() {
        let mut s = SearchState::new();
        s.set_query("fire".into());
        s.set_results(vec![res("a"), res("b"), res("c")]);
        s.move_selection(2); // selected=2
        s.enter_folder("C:\\proj".into(), false);
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
        let t1 = s.enter_folder("C:\\a".into(), false);
        let t2 = s.navigate_folder("C:\\a\\b".into());
        assert!(!s.accept_folder_result(t1)); // 旧 token は破棄
        assert!(s.accept_folder_result(t2)); // 最新 token は受理
    }

    #[test]
    fn escape_invalidates_gen_so_late_nav_result_is_dropped() {
        let mut s = SearchState::new();
        s.set_results(vec![res("a")]);
        let tok = s.enter_folder("C:\\a".into(), false);
        assert_eq!(s.on_escape(), EscapeOutcome::RestoredSearch); // folder 離脱 → gen 失効 + folder None
        assert!(!s.accept_folder_result(tok)); // 離脱後は旧ナビ結果を受理しない
        assert_eq!(s.view_kind(), ViewKind::Results);
    }

    #[test]
    fn reset_invalidates_folder_gen_and_clears_mode() {
        let mut s = SearchState::new();
        let tok = s.enter_folder("C:\\a".into(), false);
        s.reset(); // show 時 reset
        assert!(!s.accept_folder_result(tok));
        assert_eq!(s.view_kind(), ViewKind::Results);
        assert_eq!(s.folder_filter(), "");
    }

    #[test]
    fn folder_mode_interp_is_plain_even_with_prefix() {
        let mut s = SearchState::new();
        s.enter_folder("C:\\a".into(), false);
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
            OpenerTool {
                name: "VSCode".into(),
                exe: "Code.exe".into(),
                args: String::new(),
            },
            OpenerTool {
                name: "Terminal".into(),
                exe: "wt.exe".into(),
                args: "-d {path}".into(),
            },
        ]
    }

    #[test]
    fn enter_tool_from_results_saves_and_replaces_rows() {
        let mut s = SearchState::new();
        s.set_query("code".into());
        s.set_results(vec![SearchResult {
            name: "proj".into(),
            path: "C:\\proj".into(),
            is_folder: true,
            is_error: false,
            icon: IconSource::FromPath,
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
            name: "proj".into(),
            path: "C:\\proj".into(),
            is_folder: true,
            is_error: false,
            icon: IconSource::FromPath,
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
            name: "dir".into(),
            path: "C:\\dir".into(),
            is_folder: true,
            is_error: false,
            icon: IconSource::FromPath,
        }]);
        s.enter_folder("C:\\dir".into(), false);
        s.set_folder_filter("fil".into());
        s.set_results(vec![SearchResult {
            name: "child".into(),
            path: "C:\\dir\\child".into(),
            is_folder: false,
            is_error: false,
            icon: IconSource::FromPath,
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
            name: "f".into(),
            path: "C:\\f".into(),
            is_folder: false,
            is_error: false,
            icon: IconSource::FromPath,
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
            name: "f".into(),
            path: "C:\\f".into(),
            is_folder: false,
            is_error: false,
            icon: IconSource::FromPath,
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
        let tok = s.enter_folder("C:\\slow".into(), false);
        s.enter_tool("C:\\slow\\x".into(), false, make_tools());
        assert!(!s.accept_folder_result(tok));
        assert_eq!(s.on_escape(), EscapeOutcome::RestoredFromTool); // tool 解除 → folder 復帰
        assert!(s.accept_folder_result(tok)); // folder に戻れば同 token は再び有効（gen は進めていない）
    }

    #[test]
    fn stale_token_rejected_after_escape_and_reenter_while_folder_is_some() {
        // enter/escape/re-enter は folder_gen を進めるので、離脱前の token は
        // 再突入で folder.is_some()==true に戻っても受理されない（staleness の
        // defense-in-depth: is_some ガードと gen bump の両方が効いていることを固定）。
        let mut s = SearchState::new();
        s.set_results(vec![res("a")]);
        let t1 = s.enter_folder("C:\\a".into(), false);
        assert_eq!(s.on_escape(), EscapeOutcome::RestoredSearch); // folder=None・gen 進む
        let t2 = s.enter_folder("C:\\a".into(), false); // 再突入・folder=Some・gen 進む
        assert!(!s.accept_folder_result(t1)); // 旧 token は folder Some でも拒否
        assert!(s.accept_folder_result(t2)); // 最新 token は受理
        assert_eq!(s.view_kind(), ViewKind::Folder);
    }

    /// **この表は表示と起動の両方を決める**（#1077）。`view.rs` の表示ゲートに加えて
    /// `launcher_controller.rs` の `activate_or_execute` / `shift_activate` が同じ述語を呼ぶため、
    /// ここで偽になる組（instant 行・folder・tool・非 indexing）は**起動できることの固定**でもある。
    /// 呼び出し点が在ることは `launcher_controller.rs` の
    /// `activation_entry_points_consult_the_display_gate` が別に測る——この表だけでは測れない。
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
