//! 検索セッション層（show を跨ぐ状態・結果・選択・起動・履歴・期限）の所有者（#666 段 3）。
//! クエリ/結果/選択・folder ナビ（async ロード + staleness）・tool 選択・instant/slash・起動
//! dispatch と in-flight 回収・一時通知の期限を単独で持ち、`view.rs` からは遷移メソッドと
//! 読み口（`&self`）としてだけ届く。**依存は一方向である**——ここから `view` を参照しない。
//!
//! ここに**無いもの**（`window_coordinator.rs` の様式に倣い、前提条件付きで明記する）:
//!
//! - **フレームを所有しない。** `update()` の文の実行順序を決めるのは `view.rs` であり、ここに
//!   あるのは呼ばれる側の遷移である。`egui::Context` もフィールドに持たず毎回引数で受け取る
//!   （Context の clone は repaint callback ごと複製し、その `RepaintScheduler` の `Arc` が窓の
//!   `Destroyed` を越えて worker の停止・join を止める・#671 PR D）
//! - **7 番目の外部イベント消費（`take_clicked_for`）はここに無い**（#699）。行クリックの逆流は
//!   `view.rs` が snapshot publish の**後**に消費し、照合を通った index だけが
//!   `activate_or_execute` としてここへ届く
//! - **`SearchState::reset()` の `rows_generation` bump は、`view.rs` 末尾のクリック照合と
//!   結ばれている**（#699）。`consume_reset_pending` がフレーム冒頭で進める世代が、同じフレームの
//!   `take_clicked_for` で stale クリックを棄却する根拠になる——ここだけを読んで「世代は検索の
//!   都合」と読まないこと
//! - **`drain_launch` の `notice.set` を撃つ 3 分岐は自前の `request_repaint` を持たない**
//!   （通知の期限を張る唯一の主体は `poll_async` の `notice.remaining()` ブロックであり、両者が
//!   **同じフレームに呼ばれる**ことが期限の成立条件である）。**`drain_launch` 全体が repaint を
//!   持たないという意味ではない**——Empty 腕の未 timeout 側は
//!   `request_repaint_after(LAUNCH_TIMEOUT - elapsed)` を張るが、これは起動タイムアウトの期限で
//!   あって通知の期限ではない（別の armed 期限・#711）
//! - **folder drain の前後関係**は両側とも他所にある: 前は `reset()` / `enter_folder` 等の
//!   `folder_gen` bump（`accept_folder_result` の stale 棄却はこれを根拠に成立する）、後ろは
//!   #699 の世代照合（drain が行を差し替えるのはクリック消費より前でなければならない）
//!
//! # 子モジュール
//!
//! [`LauncherController`] の型・構築・読み口（`&self`）だけをこのファイルが持ち、遷移は責務ごとの
//! 子モジュールが `impl` を分けて持つ。**子は private のままにし、`view.rs` へ届く名前は
//! `pub(in crate::egui_shell)` と本ファイルの re-export が決める**（`indexer.rs` の分割と同じ
//! 流儀）。子どうしで呼び合うものは `pub(super)`（＝このモジュール内）に留める。
//! 子モジュールの責務はそれぞれの `//!` が正本であり、ここでは数え上げない（下の `mod` 宣言が
//! 一覧である）。
//!
//! **`view.rs` から見た `pub(super)` はこのファイルのものだけである**——子の `pub(super)` は
//! `launcher_controller` までしか届かない。同じ綴りが親子で別のスコープを指すので、子を読む
//! ときは「`view.rs` へ届くのは `pub(in crate::egui_shell)` と書いてあるものだけ」と読むこと。
//!
//! **起動の入口は `launcher_controller/` の直下に置くこと**——ソーステキスト検査の母集団が
//! そのディレクトリであり、入口がどの子モジュールに在っても射程は付いていく（#1201）。
//! **このファイル自身は母集団の外である**（ディレクトリの外に在る）。残る死角の正本は
//! `activation.rs` の `//!`。

use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use snotra_core::engine::FolderListContext;
use snotra_core::ui_types::SearchResult;

use crate::egui_shell::{Debouncer, FrameIndexing, SearchState};

use activation::LaunchInFlight;
use folder_nav::FolderMsg;

mod activation;
mod folder_nav;
mod frame_stages;
mod hide_request;
mod search_flow;
mod updater_toast;

pub(super) use updater_toast::ToastAction;

pub(super) struct LauncherController {
    app_handle: tauri::AppHandle,
    /// blur 猶予の状態機械（#745）。**hide を跨いだ持ち越しは `consume_reset_pending` の
    /// `reset()` が塞ぐ**——旧 2 フィールド（`was_focused` / `unfocus_at`）はこの型へ畳んだ。
    blur_grace: crate::egui_shell::BlurGrace,
    state: SearchState,
    search_debounce: Debouncer,
    last_input_at: Instant,
    // query フィールドは SearchState.query へ移譲（削除）。
    // emit dedup は共有 EguiShellState.hide_pending（show がクリア・codex #8）。view-local には持たない。
    folder_tx: Sender<FolderMsg>,
    folder_rx: Receiver<FolderMsg>,
    /// ナビゲーションでロードした (ctx, 全ソート済み) キャッシュ。打鍵フィルタの源（#532 SU3 M2）。
    folder_cache: Option<(FolderListContext, Vec<SearchResult>)>,
    /// 列挙失敗時の単一エラー行（filter を無視して表示）。
    folder_error: Option<Vec<SearchResult>>,
    /// 表示中の results の来歴: instant 候補なら Some(instant_query)。Enter/クリック/← の判定は
    /// live config の prefix から interp を再導出せず、この snapshot を使う——prefix の hot-change
    /// 後に stale instant 行（path=description/display）が activate() へ流れて文字列をパスとして
    /// 起動・履歴汚染するのを防ぐ（/code-review #637 finding 0）。run_search が行と一体で更新する。
    instant_rows_query: Option<String>,
    /// in-flight 起動（single-flight の実体: Some の間は新規起動 dispatch を拒否）。
    launching: Option<LaunchInFlight>,
    /// #633: index build 完了世代の last-seen（AppState.index_generation と比較・SU6 spec 決定 3）。
    /// 差分で現クエリを再検索（SolidJS `indexing-complete`→runRefresh parity）。bool エッジ検出で
    /// ないのは started/complete の repaint が 1 フレームに合流するとパルスが見えないため。
    last_seen_index_generation: u64,
    /// 一時通知（起動失敗/結果不明）。時刻は notice_base からの経過で注入（純粋核）。
    notice: crate::egui_shell::NoticeSlot,
    /// notice の単調時刻基準（view 生成時に固定・Instant 差分を Duration で渡す）。
    notice_base: Instant,
    /// Plain 検索の要求を worker へ送る送信端（#1004 PR 2）。
    search_tx: Sender<crate::egui_shell::SearchRequest>,
    /// worker からの結果受信端。`drain_search` が毎フレーム吸い出す（#1004 PR 2）。
    search_rx: Receiver<crate::egui_shell::SearchMsg>,
}

impl LauncherController {
    pub(super) fn new(app_handle: tauri::AppHandle) -> Self {
        let (folder_tx, folder_rx) = channel();
        let (search_tx, search_rx) = crate::egui_shell::spawn_search_worker(app_handle.clone());
        Self {
            app_handle,
            blur_grace: crate::egui_shell::BlurGrace::default(),
            state: SearchState::new(),
            search_debounce: Self::new_search_debounce(),
            last_input_at: Instant::now(),
            folder_tx,
            folder_rx,
            folder_cache: None,
            folder_error: None,
            instant_rows_query: None,
            launching: None,
            last_seen_index_generation: 0,
            notice: crate::egui_shell::NoticeSlot::default(),
            notice_base: Instant::now(),
            search_tx,
            search_rx,
        }
    }

    /// 検索 debounce を建てる唯一の口（構築時と reset-on-show の 2 か所が共有する）。
    ///
    /// **窓の長さ（interval）の正本はここである**——[`crate::egui_shell::search_state`] の
    /// `is_unsettled` の doc が「`LauncherController` が `Debouncer::new` へ渡す値」と名指して
    /// いる先がこの関数である。分割で 2 か所がファイルをまたいだため、値を綴る点を 1 つに戻した。
    /// 第 2 引数は `leading`（バースト先頭で即発火するか）であって `armed` ではない——
    /// [`crate::egui_shell::Debouncer::new`] は常に `armed: false` で建てる。
    fn new_search_debounce() -> Debouncer {
        Debouncer::new(Duration::from_millis(50), true)
    }

    // ---- view へ公開する読み口（すべて `&self`）------------------------------------------
    // **mutator は `&mut self` を要するため、この 1 群を通して view から状態を変える経路は無い**
    // （設計 §3.7）。`app_handle` の clone を view 側にもう 1 本持たないための `app()` も含む。

    /// managed state を引くための `AppHandle`（唯一の所有者はここ・不変条件 13）。
    /// **view は `update()` / `setup()` の冒頭でこれを 1 回 clone してローカルに置く**——
    /// `tauri::State<'_, T>` は借用元に紐付くため、戻り値を保持したまま `&mut` の遷移メソッドを
    /// 呼ぶと E0502 になる（先例は `results_view.rs` の `let app_handle = self.app_handle.clone();`）。
    pub(super) fn app(&self) -> &tauri::AppHandle {
        &self.app_handle
    }

    /// 検索状態の読み口（`view_kind` / `results` / `selected` / `query` / `folder_filter` /
    /// `tool_frame` / `rows_generation`）。共有参照 1 本で読みを全て通す（設計 §3.7）。
    pub(super) fn state(&self) -> &SearchState {
        &self.state
    }

    /// 一時通知の本文（status 行が描く・段 24）。期限管理は `poll_async` が持つ。
    pub(super) fn notice_message(&self) -> Option<&str> {
        self.notice.message()
    }

    /// in-flight 起動があるか（入力欄の非対話化と「起動中…」表示・段 21/24）。
    pub(super) fn is_launching(&self) -> bool {
        self.launching.is_some()
    }

    /// 検索 debounce が armed か（snapshot の [`crate::egui_shell::results_view::RowsSnapshot::input_idle`] の材料・段 30）。連打中は results 側が icon worker を積まないためのゲートで、live 値を snapshot 経由で運ぶ。
    pub(super) fn is_search_armed(&self) -> bool {
        self.search_debounce.is_armed()
    }

    /// 表示中の行が instant 候補なら Some（表示ゲートの連言②・段 29）。来歴 snapshot の
    /// 意味は同名フィールドの doc が正本。**この読み口は `view.rs` の表示ゲート用である**——
    /// 起動側（`activate_or_execute` / `shift_activate`）は同じ述語へ `self.instant_rows_query` を
    /// 直接渡すので、ここを通らない（#1077）。
    pub(super) fn instant_rows_query(&self) -> Option<&str> {
        self.instant_rows_query.as_deref()
    }

    /// index 構築中か（AppState.indexing: AtomicBool・state.rs:14 で確認済み）。実装は
    /// `window_coordinator::read_indexing` へ委譲する——show 経路とバイト単位で同一の
    /// 独立実装を持っていた重複の解消（レビュー是正 3）。
    ///
    /// **返すのは [`FrameIndexing`] であって `bool` ではない**（#1077）——「実際に読んだ」
    /// 証拠を型で運び、別の `bool` を `indexing` のつもりで渡す書き方を構築不能にする。
    /// **`view.rs` はこれを 1 フレーム 1 回だけ呼ぶ**。ここが `pub(super)` のままなのは
    /// `run_search_with` が自分の時点で読むためで（用途が違う——行をクリアするか）、
    /// そこは意図的に live-read を残してある。
    pub(super) fn indexing(&self) -> FrameIndexing {
        super::window_coordinator::read_indexing(&self.app_handle)
    }

    /// UI 文言の言語（config general.language・起動時一回でなく都度読み——読み 1 回/フレームの
    /// 既存ヘルパー群と同型。SU6 の hot-reload 拡張時もこの読み口のまま動く）。
    pub(super) fn lang(&self) -> snotra_core::config::Language {
        crate::egui_shell::read_config(
            &self.app_handle,
            |c| c.general.language,
            // AppState 不在（setup 完了前の理論経路のみ——`.manage` は `.setup` より前）は OS
            // ロケールから導く（#824 の 2 で決定）。固定の `Ja` は `SPEC.md`「7.6 起動時の設定初期化」
            // の「`ja` で始まれば日本語、それ以外は英語」と食い違っており、到達すれば非 ja 環境で
            // 誤った文言を出す。
            // `GeneralConfig::default()` を経由するのは、`default_language()` を `pub` にすると
            // lib crate の公開面が増えて `dead_code` による到達性の検出を失うためである
            // （`docs/adr/ADR-config-default-fallback-references.md`）。
            || snotra_core::config::GeneralConfig::default().language,
        )
    }
}
