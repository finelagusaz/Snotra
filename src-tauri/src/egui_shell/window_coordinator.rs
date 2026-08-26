//! 窓の可視性・位置・サイズ・wake を駆動する 1 つの責務（#749 段 1）。
//!
//! 「撃つ主体」を集めた場所であって、「撃ってよいか」の判定は持たない——可視性の述語は
//! `layout::present_results`（純粋核・#752）、results の raw 操作の所有点は
//! `results_window::ResultsWindow`（#671 PR A′）である。
//!
//! **例外的にフレームへ閉じた値型を持つ**: [`FrameIndexing`]（#1077）と [`FrameVisibleRows`]
//! （#1106）。読む関数（`read_indexing` / `read_visible_rows`）がここに在り、その返り値型を
//! **同じ場所に置くことで構築子を読み点へ閉じる**——別の値をそのつもりで配る書き方が
//! コンパイル不能になる。判定は持たない（述語は `search_state::plain_results_hidden` と
//! `layout::results_area_collapsed`）という上の性格は変わらない。
//!
//! **wake は primitive として公開する**（#711）——「いつ起こすか」を本モジュールが決めた
//! 瞬間に「armed 期限は保持者が毎フレーム再要求する」契約が壊れる。期限の所有者
//! （`LauncherController`）が呼び、ここは実行するだけにする。
//!
//! **z-order は本モジュールに無い**——main は `commands/window.rs` が `set_always_on_top` を
//! 直接叩き、results は `ResultsWindow::set_topmost` が持つ（tao の差分適用が results を消す
//! ため層が違う・#646 PR2）。どちらも設定サイドカー監視のポーリングスレッドから来るため、
//! ここを通らない。
//!
//! **main 窓のサイズは 2 か所に分かれたままである**——show 経路の実高導出は `show_egui_main` の中、
//! すなわちここにあり、毎フレームの動的高さ（`layout::main_window_height` の適用）は `view.rs` に
//! ある。両者が同じ高さを導出する共有の実体の正本は `src-tauri/CLAUDE.md`「モジュール構成」の
//! `window_coordinator.rs` の項（#755 / #801）。分かれている理由は読み点だけで、ここは
//! 「フレームの外・reset-on-show 後の値」を、`view.rs` は「フレームの中・実際の値」を読む。
//!
//! listener の**登録**は `mod.rs` に残す（setup の順序制約を `main.rs` の 1 画面に残す設計・
//! `EguiShellHandles` の doc を参照）。ここにあるのは登録先の実体だけである。

use std::sync::atomic::Ordering;
use std::time::Instant;

use snotra_core::config::{AppearanceConfig, GeneralConfig};

use tauri::Manager;

use super::layout;
use super::{EguiShellState, ResultsWindow};

/// 実行中 config から Metrics を導出する(#646 決定 2)。毎フレーム/毎 show の live-read で
/// キャッシュしない。
///
/// **呼び出し元は `show_egui_main` だけである**（#749 で実測）。view は `read_visual` →
/// `visual::visual_snapshot` 経由で同じ `layout::Metrics::from_config` に到達するため、
/// **導出式とフォールバックの単一点は `Metrics::from_config` であって本関数ではない**
/// (/simplify: 独立実装 2 箇所でフォールバックが 52.0/43.0 に乖離していた)。
/// 本関数が `read_visual` と別に在るのは、show 経路が高さだけを要り色 parse を払わないため
/// （`mod.rs` の `read_visual` の doc と対）。
/// AppState 不在(setup 完了前の理論経路のみ)は `visual::default_visual()` から導出——
/// 理由は本体のコメントに置く。
pub(crate) fn read_metrics(app: &tauri::AppHandle) -> layout::Metrics {
    let (f, rp, bp) = super::read_config(
        app,
        |c| {
            let v = &c.visual;
            (v.font_size, v.row_padding, v.bar_padding)
        },
        || {
            // 既定 VisualConfig の正本は `visual::default_visual()`（`LazyLock` 静的）である——
            // ここで `VisualConfig::default()` を組むと String 6 本を毎回確保し、既定源も 2 つになる
            let v = super::visual::default_visual();
            (v.font_size, v.row_padding, v.bar_padding)
        },
    );
    layout::Metrics::from_config(f, rp, bp)
}

/// 窓の論理幅を config から読む点のうち、**窓生成後の 2 経路**（show 経路 `show_egui_main` と
/// フレーム内の `view.rs` の `window_width`）が共有する唯一の実装。
///
/// **窓生成は含まない**——`main.rs` が起動時 config から `window_width` を直読みし、
/// `create` へ渡して両窓の初期 `inner_size` にする（`mod.rs` の窓生成）。ゆえに幅が既定へ
/// 落ちる条件は 2 系統ある: ここは AppState 不在（下記）、窓生成側は config のロード失敗。
/// **ここに fallback / clamp / migration を足しても起動直後の初期サイズには効かない。**
///
/// **読みと落とし先を独立実装に分けない**——同じことを 2 箇所でやって乖離した実績が
/// このファイルにある（`read_metrics` の doc が記録する 52.0/43.0）。
///
/// **幅について OS の現在サイズは読まない**（#824 の 1）。show 経路は以前 `inner_size()` を読み、
/// 失敗するとリテラル 600 へ落ちていた——`window_width = 900` のユーザーで窓が縮む欠陥である。
/// 読み元ごと config へ寄せたのは、落とし先を直すだけでは hide を跨いだ設定変更が残るためで、
/// hidden 中は `update()` が走らず `inner_size()` が旧幅を返す（show 直後に幅がスナップする）。
/// これは `view.rs` が幅の `inner_size()` 読みを撤去したときと同じ判断である。
///
/// AppState 不在は setup 完了前の理論経路のみ（`.manage` は `.setup` より前・`read_metrics` の
/// doc と同じ）。既定へ落ちるのはそのときだけである。
pub(crate) fn read_window_width(app: &tauri::AppHandle) -> f64 {
    super::read_config(
        app,
        |c| f64::from(c.appearance.window_width),
        || f64::from(AppearanceConfig::default().window_width),
    )
}

/// `AppState.indexing` を**実際に読んだ**という証拠つきの値（#1077）。
///
/// **フィールドが private なので、このモジュールの外ではこの型を構築できない。** ゆえに
/// 「別の `bool` を `indexing` のつもりで渡す」書き方が**コンパイルを通らない**。素の `bool` で
/// 配っていた頃は、受け取る側の
/// [`crate::egui_shell::launcher_controller::LauncherController::on_enter`] が
/// `shift_held: bool` を先に取るため、2 引数を入れ替えてもコンパイルもテストも通った
/// （この型にはテスト席が無く、取り違えを区別できる観測が無い）。**newtype を被せるだけでは
/// 閉じない**——タプル構築子が公開されていれば呼び出し点で任意の `bool` を包めるので、
/// `/symmetric-check` の Step 2c が言う「起点が同型なら型は守っていない」のままだった。
///
/// **フレームに閉じた値である。** `AppState.indexing` は `AtomicBool` の live-read で同一
/// フレーム内でも変わりうるため、`view.rs` の `update()` は 1 回だけ読み、この型のまま
/// status 行・表示ゲート・起動判定へ配る。**`self.` へ保持してはならない**——フレームを跨いで
/// 持つと index build の開始・完了が反映されなくなる。
#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameIndexing(bool);

impl FrameIndexing {
    /// 述語（[`crate::egui_shell::plain_results_hidden`] など）へ渡すための取り出し。
    pub(crate) fn get(self) -> bool {
        self.0
    }
}

/// index 構築中か（show 経路が status 行の有無を導くために読む）。正本は `AppState.indexing`。
/// 毎フレーム側は `launcher_controller::LauncherController::indexing` がこの実装へ委譲する
/// （両者がバイト単位で同一実装を独立に持っていた重複の解消・レビュー是正 3）。
///
/// **[`FrameIndexing`] の値は必ずこの関数の返り値に由来する**（#1077）——型がその証拠を担う。
pub(super) fn read_indexing(app: &tauri::AppHandle) -> FrameIndexing {
    FrameIndexing(
        app.try_state::<crate::AppState>()
            .map(|s| s.indexing.load(Ordering::Relaxed))
            .unwrap_or(false),
    )
}

/// updater toast の行が出るか（show 経路が高さを導くために読む）。正本は `UpdaterUiState`。
/// **reset-on-show はこれを触らない**——ゆえに hide を跨いで残り、通常は show 後の最初の
/// フレームでも同じ値になる（`launcher_controller` の reset 消費のコメントが明記している）。
/// **ただし別スレッドからの更新は排除できない**——`spawn_update_check` の完了・
/// `spawn_install` の失敗腕（`mod.rs` / `launcher_controller.rs`）は非同期に `phase` を
/// 書き換えて `wake_main` するため、この読みと最初のフレームの間に値が変わりうる。
/// そのときも 1 フレームだけ高さがずれるにとどまり、`view.rs` の reset-on-show による
/// memo リセットが同じフレームの動的高さ算出で直す（固着しない）。
fn read_toast_present(app: &tauri::AppHandle) -> bool {
    app.try_state::<super::UpdaterUiState>()
        .map(|st| st.0.lock().unwrap().toast().is_some())
        .unwrap_or(false)
}

/// `Color32` を tao のネイティブ背景ブラシ色へ（spec 決定 4）。
///
/// **ブラシ側の alpha は 255 に固定する**——softbuffer の clear color が `0x00RRGGBB` で alpha を
/// 持てず、下地と定常の背景が食い違うと show の一瞬だけ色が変わって見えるためである。両者は
/// 同じ `Color32` から導くので**必ず一致する**。
///
/// **`#RRGGBBAA` の alpha は「無視」されない**——`Color32::from_hex` が RGB を alpha で
/// premultiply する。命題を測っているのは `visual.rs` の
/// `background_color_premultiplies_alpha_rather_than_ignoring_it` である（正本はそのテスト）。
/// ここへ来る時点で RGB は既に減衰済みで、この関数が落とすのは alpha 成分だけである。
///
/// **`visual.rs` ではなくここに居る理由**: `visual.rs` は「1 フレーム分のテーマ値と純粋な導出」を
/// 宣言する module だが、この変換の消費者は**すべてフレームの外**（窓生成・show・リサイズ）に居る。
/// `egui::Color32` → `tauri::window::Color` は窓の関心であってテーマ導出の関心ではない。
pub(crate) fn native_brush_color(color: egui::Color32) -> tauri::window::Color {
    tauri::window::Color(color.r(), color.g(), color.b(), 0xff)
}

/// 窓の下地（softbuffer の present 前に一瞬見えるネイティブブラシ）を config 色へ合わせる。
///
/// **両窓が同じ本体を通る。** main（`show_egui_main` とリサイズ）と results
/// （`ResultsWindow` が委譲する）で別実装にすると、一手増えたときに片方だけ直る——そして
/// 乖離の症状は「main と results で下地の色が食い違う」で、**この変更が消したはずのバグと同型**に
/// なる。文書の相互参照ではなくコードで結ぶ。
///
/// **撃つのは下地が露出しうる経路のうち show 遷移時とサイズ変更時である**（全経路ではない——
/// DPI 変化に伴う物理リサイズは論理サイズが不変ゆえ `size_delta_exceeds` を通らず、撃たれない。
/// 「main 可視のまま色を変更 → DPI の違うモニターへ移動」の一瞬だけ旧色が出うる・受容する残余）。
pub(crate) fn apply_native_background(window: &tauri::Window, color: egui::Color32) {
    let _ = window.set_background_color(Some(native_brush_color(color)));
}

/// show 経路が**背景色だけ**を読む（`read_metrics` と同方針——色 5 本の parse と font 比較を
/// 払わせない）。ネイティブ背景ブラシ用であり、フレーム内の描画は `read_visual` を使う。
///
/// **`read_visual` と統合しない**: こちらは show 経路（**フレーム外**——同じイベントループ
/// スレッドではあるが `update()` の中ではない）の
/// 読みで、**1 フレーム 1 読みの規律（#673 決定 4）が掛かる面には居ない**——同じ関数内の
/// `read_metrics` や `follow_cursor_monitor` / `ime_off_on_show` の読みと同じ層である。
///
/// **その層は #1076 で `engine.lock()` を持たなくなった。** show はフレームの外だが、
/// **窓が出るまでを止める**——検索 worker が `engine.search` で `Mutex<Engine>` を握っている
/// 間に hotkey が来ると、そこで待つのは表示そのものである（`src-tauri/CLAUDE.md`「モジュール構成」の #1032 条項）。
pub(crate) fn read_background(app: &tauri::AppHandle) -> egui::Color32 {
    let hex = super::read_config(
        app,
        |c| c.visual.background_color.clone(),
        || super::visual::default_visual().background_color.clone(),
    );
    super::visual::background_color(&hex)
}

/// Position the main window on the target monitor using saved relative coordinates.
///
/// Target monitor is determined by `follow_cursor_monitor` config:
/// - true: monitor containing the mouse cursor
/// - false: primary monitor
///
/// Saved relative coordinates (physical pixels from monitor work area origin)
/// are applied and clamped to the target work area. If no saved position exists,
/// the window is centered on the target monitor.
///
/// Moved here from `main.rs` alongside its counterpart `read_placement_relative` (#749):
/// keeping save and restore in separate modules would falsify the claim that placement is
/// owned by one responsibility. `show_egui_main` is the only caller.
///
/// **クランプの材料は引数で受け取る**（#878）——以前は `main.outer_size()` を読み戻しており、
/// 呼び出し側は「位置計算へ高さを伝える」ためだけに窓を `set_size(幅, バー高)` で畳んでいた。
/// **畳むことに目的は無く、値を渡す手段が OS の窓しか無かったことの帰結だった**
/// （#878 の継ぎ目 2・`ADR-show-path-derives-bar-rect`）。
#[cfg(windows)]
fn position_on_target_monitor(
    app_handle: &tauri::AppHandle,
    // &Window に一般化して egui 経路と共有（#532 SU2）。両経路とも同一の "main" 窓
    // （get_window/get_webview_window は同じ内部 Window を指す・manager/window.rs:106）。
    main: &tauri::Window,
    // 材料は `derive_bar_rect_phys` が導く——クランプ経路（`clamp_main_into_work_area`）と
    // **同じ合成**を通ることが、両者の基準が一致することの担保である（#877 と同型）。
    bar: BarRectPhys,
) {
    use snotra_core::window_data;

    // show のたびに読み直す（#1076 で engine lock から read_config へ・read_background と同じ層）。
    let follow_cursor = super::read_config(
        app_handle,
        |c| c.general.follow_cursor_monitor,
        || GeneralConfig::default().follow_cursor_monitor,
    );

    // Determine target monitor work area.
    let target_wa = if follow_cursor {
        crate::monitor::cursor_monitor_work_area()
    } else {
        crate::monitor::primary_monitor_work_area()
    };
    let Some(target_wa) = target_wa else { return };

    // Bar rect size (physical) for centering/clamping. **OS からは読まない**（#878）。
    let win_w = bar.width;
    let win_h = bar.height;

    // Load saved relative placement and convert to absolute on target monitor.
    let (abs_x, abs_y) = if let Some(placement) = window_data::load_search_placement() {
        // Saved coordinates are physical pixels relative to monitor work area origin.
        let x = target_wa.left + placement.x;
        let y = target_wa.top + placement.y;
        // Clamp to ensure the window stays within the target work area
        // (handles different-sized monitors).
        target_wa.clamp(x, y, win_w, win_h)
    } else {
        // No saved position — center on target monitor.
        target_wa.center(win_w, win_h)
    };

    let _ = main.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
        abs_x, abs_y,
    )));
}

/// egui 経路の show。共有するのは position_on_target_monitor のみ。全 hide は外部化ゆえ
/// runtime.visible は false にならず、show は Focused(true) に依存せず確実に描ける（codex #4）。
/// show 列は WebView2 の show_and_focus_main を egui 用に自前複製（WebView2 本体を触らないため）。
///
/// **`_el` はイベントループスレッド上であることの証人である**（`EventLoopProof`）。可視性を
/// **変える**操作を単一スレッドへ閉じるため、別スレッドからの呼び出しをコンパイル不能にする。
/// 本体は証人を使わない（`_` 始まり）が、シグネチャから外してはならない——外した瞬間に
/// hotkey スレッドや spawn した待機スレッドから直接撃てるようになり、判定と副作用のあいだへ
/// 逆操作が割り込む並びが**再び構築可能になる**。
pub(crate) fn show_egui_main(
    app: &tauri::AppHandle,
    _el: &snotra_egui_runtime::EventLoopProof,
    t0: Instant,
) {
    let Some(window) = app.get_window("main") else {
        crate::trace_main("egui_show:no_window", serde_json::json!({}));
        return;
    };
    // show のたびに view の emit dedup をリセット（Focused(true) 非依存・codex #8）。
    if let Some(sh) = app.try_state::<EguiShellState>() {
        sh.hide_pending.store(false, Ordering::SeqCst);
        sh.reset_pending.store(true, Ordering::SeqCst); // resetForShow を view に指示
    }
    // 位置 → サイズ → show の順（旧 WebView2 経路から引き継いだ順序制約）。
    // **位置とサイズは別々の高さで決まる**: 位置は**バー高**、サイズは**実高**（status / toast 込み）。
    // 実高で位置を決めると、作業領域の下端付近では常にその分だけ窓が上へ押し戻される——毎フレーム
    // 経路（`view.rs`）は `set_size` しか呼ばないため、toast が消えて窓が縮んでも位置は戻らず、
    // 次の hide が `read_placement_relative` でそのずれた位置を永続化する。**バーの位置は
    // ユーザーが決め、行の出没では動かさない**（人間裁定・2026-08-04）の帰結である。
    // **かつてこの非対称は「窓を 2 回 set_size する」形で表現されていた**（1 手目にバー高へ畳み、
    // `position_on_target_monitor` がそれを `outer_size()` で読み戻す）。**#878 で材料を引数へ移した**
    // ため、畳む必要は消えて `set_size` は 1 回になった——順序制約だけが残る。
    // `egui_show:done` の trace payload（下）が読む「show が適用した高さ」の受け皿。
    // 非 windows ビルドは本関数がサイズ/位置を一切設定しないため常に `None` のまま残る。
    #[cfg(not(windows))]
    let applied_height: Option<f64> = None;
    #[cfg(windows)]
    let applied_height: Option<f64> = {
        // 幅も config から当てる（#824 の 1）。**OS の現在サイズは読まない**——hidden 中は
        // update() が走らないので、hide を跨いで幅設定が変わると `inner_size()` は旧幅を返す。
        // それで show すると最初のフレームが新幅へ書き直して幅がスナップする（このブロック
        // 冒頭が高さについて断っている視覚スナップと同型）。config が幅の正本であることは
        // `view.rs` の `window_width` の doc が記録するとおりで、OS を経由する
        // read-modify-write を作らないのが元々の設計である。
        let width = read_window_width(app);
        // 畳む先は「そのフレームで実際に描かれる高さ」である(#755 / #801)。かつては
        // バー高固定で、最初のフレームが status / toast の分だけ書き直していた——その
        // 食い違いが、伸びる(#801)か固着する(#755)かのどちらかとして必ず現れた。
        // **両者は 1 回の show では排他であり、同じ食い違いの 2 分岐である**。
        let m = read_metrics(app);
        let indexing_now = read_indexing(app);
        let toast_now = read_toast_present(app);
        // **3 つのリテラルが reset-on-show への依存である。** 最初のフレームは reset 後の
        // 状態を描くので、`launching` は消えており、view は Results 段に戻っている。
        // **一時通知については「消えている」が唯一の前提ではない**——`view.rs` の
        // `consume_reset_pending` は通知をクリアするが、直後の `consume_external_pending`
        // が同じフレームで hotkey 登録失敗の pending 通知を新たに立てうる
        // （`launcher_controller.rs` の同関数 doc「hidden 中の失敗は次 show のこの消費で
        // 表示される」）。ここでは常に `false` を渡すため、その場合 1 フレームだけ高さが
        // 実際より低く畳まれるが、`view.rs` の reset-on-show が memo を 0 へ戻しているので
        // 同じフレームの動的高さ算出が直す（固着はしない・修正前より悪化もしない）。
        // 前提が変わったら `status_row_present` の呼び出し点を grep すればここへ来る。
        let status = crate::egui_shell::status_row_present(
            indexing_now.get(),
            /* results_view */ true,
            /* launching    */ false,
            /* has_notice   */ false,
        );
        let height = layout::main_window_height(
            m.bar_height,
            status.then_some(m.toast_height),
            toast_now.then_some(m.toast_height),
        );
        // 位置決めの材料は**導出して引数で渡す**（#878）。かつてはここで
        // `set_size(width, m.bar_height)` を撃ち、`position_on_target_monitor` が
        // `outer_size()` で読み戻していた——**畳むこと自体に目的は無く、値を渡す手段が
        // OS の窓しか無かった**（継ぎ目 2）。導出できなければ位置決めをしない（取得失敗時に
        // 何もしない側へ倒すのは、クランプ経路と同じ倒し方である）。
        let derived_bar = derive_bar_rect_phys(&window, width, m.bar_height);
        if let Some(bar) = &derived_bar {
            position_on_target_monitor(app, &window, *bar);
        }
        // 不変条件検出器（#878）の受け皿。**導けなかったときは 0 を書いて残骸を消す**
        // ——前回の show の値が残ると、次のフレームがそれと現在の矩形を突き合わせて
        // 誤って発火する。突き合わせは `check_show_bar_rect`。
        if let Some(sh) = app.try_state::<EguiShellState>() {
            let (w, h) = derived_bar.map_or((0, 0), |b| (b.width, b.height));
            sh.show_bar_width_phys.store(w, Ordering::SeqCst);
            sh.show_bar_height_phys.store(h, Ordering::SeqCst);
        }
        // サイズは実高で決める（#755 / #801 の修正が導く「そのフレームで実際に描かれる高さ」）。
        // **位置はバー高、サイズは実高**——この非対称は「バーの位置はユーザーが決め、行の出没では
        // 動かさない」（人間裁定・2026-08-04）の帰結である。`set_size` は `SWP_NOMOVE` を立てるので
        // （tao `util::set_inner_size_physical`）、ここで位置は動かない。
        let _ = window.set_size(tauri::LogicalSize::new(width, height));

        // 不変条件検出器（レビュー是正 4）: 仕様は「高さは『いま描く行』で決まり、高さの変化は
        // 行の出没と 1 対 1 で対応する」。ここで読んだ生の入力と適用した高さを残し、`view.rs` の
        // reset-on-show 消費フレームが「行は変わっていないのに高さが変わった」を突き合わせる。
        // **述語へ渡したリテラル（上の `launching`/`has_notice` の `false`）ではなく、読んだ値
        // そのものを残す**——将来 show 側が「読んだが渡さない」形へ退行しても拾えるようにする。
        if let Some(sh) = app.try_state::<EguiShellState>() {
            sh.show_read_indexing
                .store(indexing_now.get(), Ordering::SeqCst);
            sh.show_read_toast.store(toast_now, Ordering::SeqCst);
            sh.show_applied_height_bits
                .store(height.to_bits(), Ordering::SeqCst);
        }
        Some(height)
    };
    // 下地（softbuffer が present するまでの一瞬に見える）を config の背景色へ合わせる。
    // **show のたびに無条件で撃つ**（spec 決定 3）——エッジ検出は「変化の瞬間に居合わせる」
    // ことを要求するが、hidden 中は update() が走らないため居合わせられない。同値の再設定は
    // 安価であり、show は頻繁な操作でもない。
    apply_native_background(&window, read_background(app));
    let _ = window.show();
    // main_visible は show() の後に立てる（WebView2 の show_and_focus_main と同じ「順序不変」
    // 制約）。show 完了前に visible=true を読んだホットキートグルが hide するのを避ける。
    if let Some(state) = app.try_state::<crate::AppState>() {
        state.main_visible.store(true, Ordering::SeqCst);
    }
    let _ = window.set_focus();
    // **かつてここに `SendMessageTimeoutW(hwnd, WM_NULL, …)` のフォーカス同期待ちが在った。
    // イベントループへ移した時点で恒久的に no-op になったため撤去した**（#880 サイクル段 2）。
    //
    // 機構: `SendMessage` 系は、宛先窓が**呼び出しスレッド自身の所有**であるとき窓プロシージャを
    // サブルーチンとして直接呼んで即座に戻る——キューを 1 通も排出せず、タイムアウトも意味を
    // 持たない。main 窓は setup（イベントループスレッド）で生成され、`show_egui_main` は証人型に
    // より同スレッドでしか呼べないので、**宛先は常に自スレッド所有**である。`WM_NULL` は tao の
    // wndproc が扱わず（0.35.3 実測・ハンドラ皆無）`DefWindowProcW` が 0 を返すだけなので、
    // 撤去は**構造的に挙動を変えない**。
    //
    // 失われた保証: 旧経路（hotkey は platform スレッド上で `show_egui_main` を走らせていた）では
    // これは**スレッド間** `SendMessage` であり、「イベントループがメッセージ取得点へ到達した」
    // ことを待てた。**活性化の完了まで待てていたとは考えにくい**——`WM_NULL` は自分が投げた 1 通に
    // 過ぎず、それが処理されたことは他のメッセージの処理を含意しないためである。
    // （`WM_ACTIVATE` 自身もシステムが**送る** sent メッセージなので、「sent は posted より先に
    // 処理される」という一般則ではこれを**確定できない**。Win32 の一般則からの導出であって
    // **この経路では測っていない**。）ゆえに失われたのはおそらく「ポンプが 1 度回った」ことだけ
    // だが、**それも含めて未実測である**。
    //
    // **導出し直せない。** 待つ対象は**自分のキュー**であり、自スレッドのキューが進むのを
    // 待つにはポンプを回すしかないが、イベントループのコールバック内でポンプを進めることは
    // 禁じられている（`src-tauri/CLAUDE.md`「ウィンドウ生成の制約」）。`on_event_loop` でも
    // 遅延できない——イベントループスレッドから呼ぶとインライン実行へ倒れる。
    // **この構造では表現不能である**というのが結論であり、下 2 つの順序依存は現在
    // 「`set_focus()` を呼んだ後」でしかない（**未実測**——実機での確認は次段のカテゴリ C/D）。
    //
    // 残留 Alt 解除: focus 確定後かつ物理 Alt 解放後のみ（#558）。
    // **`send_alt_key_up` は内部で 5ms スリープし、その根拠は失効している**——いまイベント
    // ループ上なのでスリープ中ポンプが止まり、「窓がキー up を処理する時間を作る」という
    // 当初の目的を果たさない（show のたび・受容する残余）。**レイテンシだけの問題として
    // 読まないこと**——スリープ自体が不要でありうる。判断材料と次段への申し送りは
    // `main.rs` の `send_alt_key_up` のコメントが正本。
    if !crate::is_alt_pressed() {
        crate::send_alt_key_up();
    }
    // §12: 表示時 IME オフ（設定有効時・復元なし・SU6 spec 決定 4）。ime_off_on_show は実行中
    // config から都度読み（キャッシュしない・#576 同型——config_watcher の hot-reload が diff/event
    // 追加なしに届く）。**`set_focus()` より後に置く**——前だと IME オフが対象窓に効かない
    // （WebView2 apply_ime_control doc の警告条件）。**旧記述「focus 同期（SendMessageTimeoutW）
    // より後」は、その同期待ちが no-op 化して撤去された今は意味を持たない**（上のコメント）。
    // ここが依存できるのは `set_focus()` の呼び出し順だけである。
    //
    // なお `TurnOffIme` は **platform スレッドへの channel 送信**であり、`ImmSetOpenStatus` は
    // そちらで非同期に走る——順序として制御できるのは**送信の位置**までで、実行の時刻ではない
    // （これは本変更の前からそうである）。Win32 は PlatformBridge 経由（rule）。
    // TurnOffIme は生 HWND(usize) を取るため窓型非依存で &Window 一般化は不要。
    #[cfg(windows)]
    {
        let ime_control = super::read_config(
            app,
            |c| c.general.ime_off_on_show,
            || GeneralConfig::default().ime_off_on_show,
        );
        if ime_control
            && let Some(bridge) =
                app.try_state::<std::sync::Mutex<crate::platform::PlatformBridge>>()
            && let Ok(b) = bridge.lock()
            && let Ok(hwnd) = window.hwnd()
        {
            b.send_command(crate::platform::PlatformCommand::TurnOffIme(
                hwnd.0 as usize,
            ));
            crate::trace_main("egui_show:ime_control", serde_json::json!({}));
        }
    }
    crate::trace_main(
        "egui_show:done",
        // height: show の時点で toast/status 行が予算されたかの肯定的証拠（レビュー是正 4）。
        // `None`（非 windows ビルド）は json では `null` になる。
        serde_json::json!({ "ms": t0.elapsed().as_secs_f64() * 1000.0, "height": applied_height }),
    );
}

/// egui 経路の hide。**main の** hide の唯一の副作用所有点（codex #7）——位置保存・
/// main_visible=false・working set trim はここにしか無い。**世代 bump（hotkey_generation）
/// だけは 2 箇所ある**——ここは「保留中の alt 解放待ち show を無効化する」ため、
/// hotkey listener（main.rs）は「押下ごとに採番する」ため（用途が別）。
/// **results の hide はここを通らない経路がある**（同モジュールの `drive_results_window`）
/// ため、両窓を合わせた合流点ではない（#646 PR2 以降・全称主張の訂正は #671 サイクル PR A）。
/// 外部 window.hide() のみで runtime.visible を false にしない（空白窓回避・codex #4）。
///
/// **`el` はイベントループスレッド上であることの証人である**（理由は `show_egui_main` の doc）。
/// こちらは `_` を付けない——`ResultsWindow::hide` へそのまま渡すためである。
///
/// **本体をイベントループ上へ移したことの帰結**: この関数の実行中はメッセージポンプが停止する
/// （`src-tauri/CLAUDE.md`「ウィンドウ生成の制約」）。ゆえに重い処理・ディスク I/O を
/// 臨界区間（`main_visible` の store と 2 枚の `ShowWindow` が不可分であるべき区間）に
/// 置いてはならない。placement の**書き込み**と working set の trim は末尾へ出してある。
///
/// **ただし「臨界区間の外」は「イベントループの外」ではない。** 末尾へ出した 2 つも
/// この関数の中＝イベントループスレッド上で走り、その間ポンプは止まったままである
/// （**受容する残余**・詳細は当該箇所のコメント）。臨界区間から外したことで守られるのは
/// 「可視性の 3 操作が不可分であること」であって、ポンプの応答性ではない。
pub(crate) fn hide_egui_main(app: &tauri::AppHandle, el: &snotra_egui_runtime::EventLoopProof) {
    // 保留中の alt 解放待ち show を無効化（codex #5/(B)#2）: 世代を bump し、spawn 済み show
    // スレッドの gen 一致チェックを外す。
    if let Some(sh) = app.try_state::<EguiShellState>() {
        sh.hotkey_generation.fetch_add(1, Ordering::SeqCst);
    }
    // placement は「読み」だけを窓の hide より前に置く。**書き込みはこの下**——
    // ディスク I/O はポンプを止めた区間に置かない。
    // バー高は保存の**基準モニターを決めるため**に要る（#738。理由は
    // `read_placement_relative` の doc）。`read_metrics` は読みをクロージャ内で取り切って
    // 返すため、ここで保持は残らない（#1036 までは engine lock だった）。
    let bar_height = read_metrics(app).bar_height;
    let placement = app
        .get_window("main")
        .and_then(|w| read_placement_relative(&w, bar_height));
    if let Some(window) = app.get_window("main") {
        let _ = window.hide();
    }
    // main_visible は **results.hide() より前**に落とす（#671 PR A′ レビュー Important 1）。
    // これは `drive_results_window` の show ゲート（layout::present_results）が読む値である。
    // **証人型（`EventLoopProof`）を引数に要求する 5 関数はイベントループスレッドへ閉じているため**、
    // 「results.hide() 済み・main_visible=true」の隙間へ割り込むフレームはもはや構築できない
    // ——この順序を保つのは、show 側の「show() の後に true を立てる」（順序不変制約）との
    // 対称のためである。どちらも「main が可視でない期間に visible=true と読ませない」向きに
    // 倒している。
    if let Some(state) = app.try_state::<crate::AppState>() {
        state.main_visible.store(false, Ordering::SeqCst);
    }
    // #646 PR2: 従属窓も同時に隠す（決定 6）。show 側は main の update() が snapshot の
    // show 判定で駆動するため、`update()` の外から results を hide する経路はここだけ
    // （対称は main update 内の show）。同モジュールの `drive_results_window` は update **内**
    // で動く対の経路であり、直前の doc comment が言う「results の hide は 2 経路ある」は
    // この 2 つ（update 外＝ここ／update 内＝drive_results_window）を指す——矛盾ではない。
    if let Some(results) = app.try_state::<ResultsWindow>() {
        // 戻り値（遷移したか）を無視するのは意図的である——ここの trace は**要求レベル**で
        // あり、既に隠れていても出す（spec 決定 7・PR A の smoke は presence のみを assert）。
        results.hide(el);
        // 呼び出し側に置く（spec 決定 7）。results の hide は 2 経路あり
        // （ここと同モジュールの drive_results_window）、trace は要求レベルゆえ
        // 既に隠れていても出る——smoke は presence のみを assert する。
        crate::trace_main(
            "egui_results:hide",
            serde_json::json!({ "from": "hide_main" }),
        );
    }
    // ここから臨界区間の外。**順序に意味は無い**——
    // trim は hide 前後どちらで走っても無害（`src-tauri/CLAUDE.md`「working set の能動回収」）、
    // placement の書き込みは値を既に持っているので窓の状態に依存しない。
    //
    // **「臨界区間の外」は「イベントループの外」ではない——受容する残余である。** 下の 2 つは
    // 依然この関数の中、すなわちイベントループスレッド上で走り、その間メッセージポンプは
    // 止まる（本タスク以前は platform スレッド上でありループを塞がなかった）。ポンプ進行を
    // 要する操作ではないのでデッドロックはせず、窓を隠した**後**なので視覚的なジャンクにも
    // ならない。実測はしていない（ディスク書き込み + Toolhelp スナップショット + プロセスツリー
    // BFS の合計）。**別スレッドへ出すか受容するかは後段の判断に残す。**
    if let Some(p) = placement {
        snotra_core::window_data::save_search_placement(p);
    }
    // hide 後に working set を trim する（**main の** hide 経路の合流点＝ここが唯一の呼び出し元・
    // #532 SU6.5）。results 単独 hide（drive_results_window）では main が可視のままゆえ trim しないのが正しい。
    // EmptyWorkingSet はスレッド非依存ゆえこの context（イベントループ）から直呼び可
    // （src-tauri/CLAUDE.md「working set の能動回収」）。trim されたページは show 時に OS が透過
    // re-fault する（逆操作不要・trim が hide 前後どちらで走っても無害）。子孫 BFS は設定プロセス
    // （snotra-settings.exe・存命中のみ）も巻き込みうる——trim は best-effort ゆえ無害。
    crate::working_set::trim_idle_working_set(std::process::id());
    // **この trace は関数の最後の文である。** hidden 区間の開始をここで区切っており、
    // trace 不変条件（`scripts/lib/SnotraTraceInvariants.psm1` の H1）が区間の判定に使う。
    crate::trace_main("egui_hide:done", serde_json::json!({}));
}

/// 現在の物理位置を、ターゲットモニター作業領域原点からの相対座標へ換算する（**読みのみ**）。
///
/// **書き込みと分けてある**（ディスク I/O をイベントループの臨界区間から外すため——
/// `hide_egui_main` の中でポンプが止まる。`src-tauri/CLAUDE.md`「ウィンドウ生成の制約」）。
/// 読みは hide **より前**でなければ意味を持たないので、こちらだけが臨界区間に残る。
///
/// 算出は旧 WebView2 の save_relative_placement と同じ（#532 SU7 で唯一の保存経路）。
///
/// **基準モニターは `read_bar_anchor` が導く**（#738）——クランプと同じ 1 つの関数を通すことで
/// 「保存の原点」と「戻す先」が食い違わないようにしてある。窓全体の矩形から引くと、保存座標の
/// 原点が行の出没で変わり、次の show でバーがモニター 1 枚ぶん飛ぶ（理由の正本は
/// `monitor::point_monitor_work_area` の doc）。
pub(crate) fn read_placement_relative(
    window: &tauri::Window,
    bar_height: f64,
) -> Option<snotra_core::window_data::WindowPlacement> {
    use snotra_core::window_data::WindowPlacement;
    #[cfg(windows)]
    {
        let a = read_bar_anchor(window, bar_height)?;
        Some(WindowPlacement {
            x: a.pos.x - a.work_area.left,
            y: a.pos.y - a.work_area.top,
        })
    }
    #[cfg(not(windows))]
    {
        let _ = bar_height;
        let pos = window.outer_position().ok()?;
        Some(WindowPlacement { x: pos.x, y: pos.y })
    }
}

/// 可視中の main 窓を起こす（#671 PR D。旧実装＝窓ごとの Context clone を登録するスロットの後継）。
///
/// hidden 中は実効的な no-op である——抑止は wake 経路ではなく **tao/OS 層**にある
/// （2026-07-26 実測・#697: worker は `RequestRedraw` を送信するが、hidden な窓には
/// `RedrawRequested` が配送されない。spec §7 残余 2 は errata で解消済み）。旧実装
/// （Context の clone に `request_repaint()`）と同じ経路（`RepaintScheduler` → proxy →
/// `RequestRedraw`）を通るため、この性質は変わらない。
///
/// **`try_state` が返す `Option` は残る**（Tauri managed state の性質）。消えたのは
/// 「Context が登録済みか」という 2 段目の Option である。
pub(crate) fn wake_main(app: &tauri::AppHandle) {
    if let Some(sh) = app.try_state::<EguiShellState>() {
        sh.main_waker.wake();
    }
}

/// results 窓を起こす(#646 PR2)。snapshot 更新・config 変更を反映させる wake。呼び出しは
/// main の update() 内 2 箇所: snapshot 差分検知時（edge-triggered・変化フレームのみ）
/// と `drive_results_window`（可視時・毎フレーム・level-triggered。**削ると壊れる理由は
/// 呼び出し点のコメントを参照**——決定 5・#697）。hidden 中の results は
/// 描かれないため事前 wake は無意味(plan-review で冗長と判定)。クリック逆流の results→main は
/// `wake_main` を使う。
///
/// **`wake_main` と 1 関数に束ねない**——窓を引数で選ぶ形は、呼び出し側の「どちらの窓を
/// 起こすか」という判断を型から引数へ落とすだけで、配線の総量は減らない。
pub(crate) fn wake_results(app: &tauri::AppHandle) {
    if let Some(sh) = app.try_state::<EguiShellState>() {
        sh.results_waker.wake();
    }
}

/// main のバー矩形（物理）と、その中心が乗るモニターの作業領域。**Win32 の読みはここ 1 回**。
///
/// **クランプ（`clamp_main_into_work_area`）と hide 時の保存（`read_placement_relative`）は
/// 同じ基準でなければならない**——ずれると、保存したオフセットがクランプの想定と別モニターの
/// 原点に対する相対値になり、次の show でバーが 1 枚ぶん飛ぶ。その一致を doc の申し合わせでは
/// なく**この関数が 1 つであること**で担保する（同じ値を 2 か所で計算させると、次の利用者が
/// 写しを書く）。
///
/// 取得に 1 つでも失敗したら `None`——呼び出し側はいずれも「何もしない」側へ倒す。
#[cfg(windows)]
struct BarAnchor {
    pos: tauri::PhysicalPosition<i32>,
    width_phys: u32,
    /// **非クライアント分を足した後**の高さ（`layout::bar_rect_height_phys` の戻り値そのもの
    /// ではない）。`WorkArea::clamp` の `win_h` は物理 outer を要求するため、合成をここで
    /// 済ませて呼び出し側に手作業を残さない。
    outer_bar_height_phys: i32,
    work_area: crate::monitor::WorkArea,
}

/// 窓の frame 幾何——**OS しか知らない量だけ**を読む（#878）。
///
/// **窓の矩形から読んでよいのは、コードが持っていない量だけである**——非クライアント分と
/// scale がそれで、位置はユーザーが動かす。**コード自身が直前に書いた content 寸法を、
/// 渡す手段が無いという理由で読み戻してはならない**（#878 の裁定則。`show_egui_main` は
/// かつて「位置決めへ高さを伝える」ためだけに窓を畳んでいた）。
///
/// **Win32 の読みはここ 1 回である。** 消費者は 2 つ——[`read_bar_anchor`]（クランプと hide
/// 保存）と [`derive_bar_rect_phys`]（show の位置決め）。**非クライアント分の合成を
/// 呼び出し側 2 か所へ書き写さないために型で配る**。
///
/// 取得に 1 つでも失敗したら `None`——呼び出し側はいずれも「何もしない」側へ倒す。
#[cfg(windows)]
struct FrameGeom {
    /// 窓の現在の外形（物理）。**バー矩形の幅として使うのは [`read_bar_anchor`] だけである**
    /// ——show 側は config の幅から導く（OS の現在値は hide を跨いだ旧幅でありうるため）。
    outer: tauri::PhysicalSize<u32>,
    /// 非クライアント分の幅（`outer` − `inner`）。
    inset_w: i32,
    /// 非クライアント分の高さ（`outer` − `inner`）。**「将来の保険」ではなく今すでに効いている**
    /// ——`decorations: false` でも DWM の影が乗るため、実測で 10 物理 px あった（DPI 125% の
    /// 環境・#738 のカテゴリ D）。落とすとその分だけバーが作業領域からはみ出す。
    inset_h: i32,
    scale: layout::MainScale,
}

#[cfg(windows)]
impl FrameGeom {
    /// バー矩形の**物理**高さ（非クライアント分を足した後）。
    ///
    /// **合成をここ 1 か所に置く**——消費者は [`read_bar_anchor`] をはじめ複数あり、書き写すと
    /// 「片方だけが非クライアント分を落とす」形の欠陥が沈黙で入る（#738 の実例）。件数を
    /// 書かないのは、**足すたびに腐る数**だからである（呼び出し点は grep で数える）。
    ///
    /// **この合成そのものの誤り（`inset_h` を半分にする等）を捕まえる自動検査は無い。**
    /// 呼び出し点が 1 つに寄った以上、片方だけを壊す変異は
    /// [`check_show_bar_rect`] が捕まえるが、**ここを壊すと全消費者が同じだけずれる**ので
    /// 検出器も型検査もテストも沈黙する（実測: `+ self.inset_h / 2` で clippy / test /
    /// `smoke:egui` / 検出器のすべてが緑・#878）。**発見経路は目視だけである**
    /// （`docs/build-commands.md`「D. UI のスタイル・レイアウト・テキスト表示に影響する変更（A／B／C に追加）」）——#878 が
    /// 「6 例中 4 例がカテゴリ D でのみ見つかった」と集計した状態が、この 1 点については残る。
    ///
    /// **`dead_code` が拾うことを検査に数えない**——`+ self.inset_h` を丸ごと落とす変異は
    /// 今日は `field is never read` で赤くなるが、それは `inset_h` の読み手がここ 1 つだという
    /// 配置の副産物であり、誰かが別の場所で 1 回読んだ瞬間に音もなく消える。
    fn bar_height_phys(&self, bar_height_logical: f64) -> i32 {
        layout::bar_rect_height_phys(bar_height_logical, self.scale) + self.inset_h
    }
}

#[cfg(windows)]
fn read_frame_geom(window: &tauri::Window) -> Option<FrameGeom> {
    let (Ok(outer), Ok(inner), Ok(scale)) = (
        window.outer_size(),
        window.inner_size(),
        window.scale_factor(),
    ) else {
        return None;
    };
    Some(FrameGeom {
        outer,
        inset_w: outer.width as i32 - inner.width as i32,
        inset_h: outer.height as i32 - inner.height as i32,
        scale: layout::MainScale::new(scale),
    })
}

#[cfg(windows)]
fn read_bar_anchor(window: &tauri::Window, bar_height: f64) -> Option<BarAnchor> {
    let (Ok(pos), Some(geom)) = (window.outer_position(), read_frame_geom(window)) else {
        return None;
    };
    let outer_bar_height_phys = geom.bar_height_phys(bar_height);
    let (cx, cy) = layout::bar_rect_center(pos.x, pos.y, geom.outer.width, outer_bar_height_phys);
    let work_area = crate::monitor::point_monitor_work_area(cx, cy)?;
    Some(BarAnchor {
        pos,
        width_phys: geom.outer.width,
        outer_bar_height_phys,
        work_area,
    })
}

/// show がこれから当てるバー矩形の**物理**サイズ（#878）。
///
/// **`position_on_target_monitor` はこれを引数で受け取る。** かつては呼び出し側が
/// `set_size(幅, バー高)` で窓を物理的に畳み、あちらが `outer_size()` で**読み戻して**いた
/// ——`set_size` の目的は「値を渡すこと」だけで、畳むこと自体には意味が無かった
/// （`ADR-show-path-derives-drawn-height` 却下 2 の反転・#878）。
#[cfg(windows)]
#[derive(Clone, Copy)]
struct BarRectPhys {
    width: i32,
    height: i32,
}

/// [`BarRectPhys`] の唯一の構築点——型がその証拠を担う（`FrameVisibleRows` と同じ形）。
///
/// **クランプ経路（[`read_bar_anchor`]）と同じ材料・同じ合成を通る**。以前は show 側が
/// OS の読み戻しで、クランプ側が `bar_rect_height_phys` + 非クライアント分で、**同じ物理
/// バー高が 2 通りに導出されていた**（`ADR-main-window-clamp-on-pointer-release`
/// 「残っている代価」）。導出が 1 つになったので、上流の丸め規則への依存も
/// [`layout::logical_to_phys`] の doc 1 か所に集まる。
#[cfg(windows)]
fn derive_bar_rect_phys(
    window: &tauri::Window,
    width_logical: f64,
    bar_height_logical: f64,
) -> Option<BarRectPhys> {
    let geom = read_frame_geom(window)?;
    Some(BarRectPhys {
        width: layout::logical_to_phys(width_logical, geom.scale) + geom.inset_w,
        height: geom.bar_height_phys(bar_height_logical),
    })
}

/// 可視中の main を作業領域の内側へ戻す（#738）。**バー矩形だけを対象にする。**
///
/// **呼び出し条件はここに無い**——`view.rs` が「ポインタが押されていないフレーム」でのみ
/// 呼ぶ。ドラッグ中も毎フレーム戻すと、横並びモニター間の移動が**封鎖される**: 幅 600px の
/// 窓を A(`0..1920`) から右へ運ぶとき、左端が `1320..1620` の区間ではまだ A の重なりが優勢で
/// あり、毎回 `x=1320` へ引き戻されて B が優勢になる位置へ到達できない。ゆえに保証は
/// 「ドラッグ中も出られない」ではなく**「離したら戻る」**である（人間裁定・2026-08-04。
/// 前者には `WM_MOVING` のフック＝tao の wndproc サブクラス化が要り、却下した）。
///
/// **`was_reset_frame` を OR で足す backstop は実測で却下した。** 動機は正しい——egui が押下
/// フラグを落とすのは `Event::PointerButton{pressed:false}` のときだけで、`PointerGone` でも
/// フォーカス喪失でも落ちない。release が届かない経路が 1 つでもあれば `any_down()` は固着し、
/// クランプが黙って死ぬ。だが**実測ではドラッグ中も毎フレームクランプが走り、上の封鎖が
/// そのまま現実になった**（backstop 無し: ドラッグ中 top=1050 のまま／有り: 956 へ引き戻される）。
/// 固着は**受容残余**である（理由と再測の手順は `ADR-main-window-clamp-on-pointer-release`）。
///
/// **キーボードによる窓移動（`Alt+Space` → `M`）は、この保証の外にある**（#1173・実測
/// 2026-08-26）。OS のモーダル移動ループ中もフレームは回り、**ポインタが押されていないので
/// 回ったフレームでは必ず発火する**（ループ中に何枚回るかは測っていない）——マウスドラッグ
/// では除外される経路が、ここでは除外されない。
/// 対照実験（クランプ行を落としたローカルビルド）で分離した:
///
/// 条件: 2026-08-26・release・単一モニター 1920x1080（125%・作業領域の下端 1020 物理 px）・
/// 5 反復 × `↓` 200 回。窓の bottom（物理 px）:
///
/// | | 移動中に落ち着く値 | 一時的な逸脱 | 外に留まれるか |
/// |---|---|---|---|
/// | クランプ有効 | **1020**（＝作業領域の下端ちょうど） | 5 反復中 4 回、1132〜1133 まで出てから戻る | 留まれない（`Enter` の**前に**戻る） |
/// | クランプ無効 | 1134（カーソルが画面下端で止まる位置） | 無し | 留まる（確定後も 1106） |
///
/// **落ち着く値が作業領域の下端ちょうどであること**が、止めているのがカーソルの限界ではなく
/// `WorkArea::clamp` であることを示す。**「移動中は外へ出られない」ではない**——引き戻しは
/// モーダルループの `SetWindowPos` との競り合いであり、外に出た状態が数十回の打鍵にわたって
/// 観測される。**多モニターでの封鎖そのものは未測定である**——測ったのは機序であり、封鎖は
/// 上の作業例からの演繹にとどまる。
///
/// ⚠️ 対照側は確定後に 1134 → 1106 と 28 px 上がる（5/5）。**理由は特定していない**——
/// クランプ以外にも確定後の位置を動かす経路があることを意味するが、外側で落ち着く事実
/// （1106 > 1020）は変わらないので上の分離は保たれる。
///
/// **代替判定（`any_down()` に代わる「移動中である」）はまだ無い。** 候補は
/// `GetGUIThreadInfo` の `GUI_INMOVESIZE`（tao の wndproc サブクラス化を要さないので
/// `ADR-main-window-clamp-on-pointer-release` の却下 1 には当たらない見込み・**実呼び出しは
/// 未検証**）である。
///
/// **再測の手順**: release ビルドを使い捨てプロファイル（`SNOTRA_CONFIG_DIR`）で起動し、
/// `Alt+Space` → `M` → `↓` の反復 → `Enter` を `Send-SnotraKey` で注入して `GetWindowRect` の
/// `bottom` を刻む。**DPI awareness を先に確立する**（通さないと作業領域が論理値で返り、
/// `GetWindowRect` の物理座標と土俵が合わない）。**クランプ行を落とした対照が要る**——
/// 単独の系列では頭打ちがクランプかカーソル限界かを分離できない。
///
/// **`show_egui_main` の `position_on_target_monitor` とは基準モニターの決め方が違う。**
/// あちらは「これから出す窓をどこへ置くか」ゆえカーソル/プライマリを見るが、こちらは
/// 「いまある窓をどこへ戻すか」ゆえ**バー矩形の中心**が乗るモニターを見る（理由は
/// `monitor::point_monitor_work_area` の doc）。
///
/// 材料（バー矩形と基準モニター）は `read_bar_anchor` が導く——hide 時の保存と**同じ 1 つの
/// 関数**を通すことが、両者の基準が一致することの担保である。
///
/// 取得に失敗したら**クランプしない側へ倒す**（`position_on_target_monitor` と同じ）。
#[cfg(windows)]
pub(crate) fn clamp_main_into_work_area(app: &tauri::AppHandle, bar_height: f64) {
    let Some(main) = app.get_window("main") else {
        return;
    };
    let Some(a) = read_bar_anchor(&main, bar_height) else {
        return;
    };
    // 算術は `WorkArea::clamp`（ユニットテスト 7 件が固定する既存の純粋核）。**新しい算術を
    // 書かない**——show 経路と同じ導出を通すことが本修正の要点である（#877 と同型）。
    let (nx, ny) = a.work_area.clamp(
        a.pos.x,
        a.pos.y,
        a.width_phys as i32,
        a.outer_bar_height_phys,
    );
    // 同値なら撃たない。`set_position` は Win32 呼び出しであり、可視中は毎フレーム通る。
    if nx != a.pos.x || ny != a.pos.y {
        let _ = main.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
            nx, ny,
        )));
    }
}

#[cfg(not(windows))]
pub(crate) fn clamp_main_into_work_area(_app: &tauri::AppHandle, _bar_height: f64) {}

/// 不変条件検出器（#878）: **show が導いたバー矩形は、フレームが測るバー矩形と一致する。**
///
/// show は OS へ書かずに矩形を導くようになった（[`derive_bar_rect_phys`]）ので、**この PR が
/// 持ち込む退行は「導出の誤り」である**——非クライアント分の落とし、scale の取り違え、
/// 丸め規則の食い違い。ここはその導出を**毎回の show で実データに当てて検算する**。
///
/// **外から測る検査ではこの退行が見えない。** show の矩形が誤っていても、窓の実サイズは
/// 2 手目の `set_size` が決めるので DWM で測る幅・高さは正しいままであり、位置のずれも
/// 可視中のクランプが次のフレームで戻す（`egui_main:height_mismatch` と同じ「安全機構が
/// 外部の検出器を無力化する」形。理由の正本は
/// `.claude/rules/safety-nets.md`「検出器のカバー範囲は、欠落のパターンごとに検算する」）。
///
/// **「クランプが動いたか」を見る形は採らなかった。** `WorkArea::clamp` は矩形が境界を
/// 越えたときにしか座標を変えないため、窓が作業領域の内側にいる限り導出が壊れていても
/// 沈黙する（＝**守りたい退行の足を 1 本も捕まえない配置がある**）。却下の詳細は
/// `ADR-show-path-derives-bar-rect`。
///
/// # 何を突き合わせているのか——**2 軸の強さは同じではない**
///
/// **幅軸だけが導出を現実と突き合わせている。** show 側は config の幅から
/// `logical_to_phys(幅) + inset_w` を導き、こちらは `outer.width` を**実測**する。両者が
/// 一致することは、「`set_size(論理値)` の後に窓が占める物理幅は
/// `round(論理値 × scale) + 非クライアント分` である」という上流（tao / `dpi`）の振る舞いへの
/// 依存を、**毎回の show で検算している**ことにほかならない。
///
/// **高さ軸は 2 つの呼び出し点の A/B である。** show 側もここも同じ
/// [`FrameGeom::bar_height_phys`] を通るので、**共有した導出そのものの誤り**（`inset_h` の
/// 読み違い・`bar_rect_height_phys` の丸め）は両側が同じだけずれて沈黙する。捕まえるのは
/// 「**片方の呼び出し点だけが変わった**」形である。実測（#878）: show 側の呼び出しを
/// [`FrameGeom::bar_height_phys`] から素の [`layout::bar_rect_height_phys`] へ差し替える
/// （＝非クライアント分を落とす・#738 が名指す欠陥形）と、**幅が一致したまま高さ 10 px 差
/// だけで発火**した。**共有した合成そのものを壊す変異では沈黙する**——その射程は
/// [`FrameGeom::bar_height_phys`] の doc が持つ。
///
/// **ゆえに残余を数え上げない。** 沈黙するのは「show 側とフレーム側が同じ値を見る」経路
/// すべてであり、config 変更・DPI 変更が窓に挟まる場合（`egui_main:height_mismatch` に既に
/// 在る同種の残余）はその一例にすぎない。**scale もこの窓が今いるモニターのものであって、
/// show がこれから置く先のモニターのものではない**（旧経路も同じで退行ではないが、
/// 両者の DPI が違う配置ではどちらの軸も現実を測れない）。
#[cfg(windows)]
pub(crate) fn check_show_bar_rect(app: &tauri::AppHandle, bar_height: f64) {
    let Some(sh) = app.try_state::<EguiShellState>() else {
        return;
    };
    let (show_w, show_h) = (
        sh.show_bar_width_phys.load(Ordering::SeqCst),
        sh.show_bar_height_phys.load(Ordering::SeqCst),
    );
    // 0 は「show が導けなかった（`read_frame_geom` が `None`）」の番兵。実際の矩形は
    // 非クライアント分だけでも正なので、0 と衝突しない。
    if show_w == 0 || show_h == 0 {
        return;
    }
    let Some(main) = app.get_window("main") else {
        return;
    };
    // **`read_bar_anchor` は通さない。** あちらは基準モニターまで引くので
    // `point_monitor_work_area` が `None` を返す経路で検出器が黙る（要らない依存である
    // ——ここが要るのは矩形だけで、どのモニターに乗っているかは問わない）。
    let Some(geom) = read_frame_geom(&main) else {
        return;
    };
    let (frame_w, frame_h) = (geom.outer.width as i32, geom.bar_height_phys(bar_height));
    if show_w != frame_w || show_h != frame_h {
        crate::trace_main(
            "egui_main:bar_rect_mismatch",
            serde_json::json!({
                "show_w": show_w,
                "show_h": show_h,
                "frame_w": frame_w,
                "frame_h": frame_h,
            }),
        );
    }
}

#[cfg(not(windows))]
pub(crate) fn check_show_bar_rect(_app: &tauri::AppHandle, _bar_height: f64) {}

/// results を main の直下 + window_gap に配置する(#646 PR2 決定 6)。呼び出し元は
/// 2 つ——main の update()(通常の毎フレーム従属)と main の Moved リスナー
/// (ネイティブ移動ループ中の追従。ループ中は egui フレームが回らない可能性があるため
/// イベント駆動で直接動かす)。デルタガードは持たない(set_position は同値でも安価・
/// サイズ側のガードとは対称でない。理由は `ResultsWindow::set_size` の doc)。
/// 上端の算出式の正本は `layout::results_top_y`（純粋核・#752 C1）で、**Win32 を読んで
/// それを適用する場所はここだけ**である。呼び出し側で再計算すると `outer_position` /
/// `outer_size` / `window_gap` の 2 度読みになり、フレーム内で値が食い違いうる
/// （`AGENTS.md`「条件別チェック」の「重複した読み」）。
///
/// **上端 y を返さない。** #675 のクランプが唯一の消費者だったため、#835 の撤去で返す先が
/// 無くなった。**`Option` は型に `#[must_use]` を持たないので、返し続けても警告は出ない**
/// ——消費者のいない戻り値は、次の読者に「何に使うのか」を探させる。
///
/// # ここが `main.outer_size()` を読み戻すのは正当である（#878）
///
/// #878 の裁定則は「コード自身が直前に書いた content 寸法を読み戻してはならない」だが、
/// **書き手のフレーム文脈が読み手に届かない経路が呼び出し元に含まれるとき、読み戻しは
/// 正当である**——ここがその唯一の例外である（`Moved` リスナーは egui のフレームの外で走る）。
/// `outer_position()` はそもそもユーザーが動かした位置＝コードが持っていない量である。
///
/// **「main の直近の書き込み高さを共有 atomic へ残して渡す」案は却下した**（理由の詳細は
/// `ADR-show-path-derives-bar-rect`）: (1) `outer_position()` はどのみち読むので Win32 の
/// 呼び出しは減らない、(2) 物理 outer 高を残すには書き込み点で非クライアント分を読む必要が
/// あり、読み戻しが移動するだけである、(3) 書き手が 2 人（show と毎フレーム）の memo を
/// フレーム跨ぎで持つ形は #878 の継ぎ目 1 そのもので、results 側には補正フレームの
/// 相当物が無い。
pub(crate) fn position_results_below_main(app: &tauri::AppHandle) {
    let (Some(main), Some(results)) = (app.get_window("main"), app.try_state::<ResultsWindow>())
    else {
        return;
    };
    let gap = super::read_config(
        app,
        |c| c.visual.window_gap,
        || super::visual::default_visual().window_gap,
    );
    let (Ok(pos), Ok(size), Ok(scale)) = (
        main.outer_position(),
        main.outer_size(),
        main.scale_factor(),
    ) else {
        return;
    };
    // 算術は layout::results_top_y（純粋核・#752 C1）。Win32 の読みはここで 1 回だけ行う。
    // **`main` から読んだ scale をその場で `MainScale` へ包む**（`layout::MainScale` の doc）
    // ——results 窓の scale（`ResultsWindow::set_size` が読む）と型で分かれており、
    // 取り違えはコンパイルが通らない。
    let top = layout::results_top_y(pos.y, size.height, gap, layout::MainScale::new(scale));
    results.set_position(pos.x, top);
}

/// 表示ゲートの連言④（`SPEC.md`「4.5 最大列挙数」）の 1 フレーム分の読み（#1106）。
///
/// **フィールドが private なので、このモジュールの外ではこの型を構築できない**
/// （[`FrameIndexing`] と同じ形）。
///
/// **フレームに閉じた値である。** `view.rs` の `update()` が 1 回だけ読み、この型のまま
/// 表示側と起動側へ配る。**この 2 つの役割が同じ値を見ることが、この型の存在理由そのもの
/// である**（配り先の数ではなく、役割が 2 つあることを名指している）。**`self.` へ保持しては
/// ならない**——フレームを跨いで持つと `config.toml` の変更が反映されなくなる。
#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameVisibleRows(u32);

impl FrameVisibleRows {
    /// 述語（[`crate::egui_shell::layout::results_area_collapsed`] / `present_results`）へ
    /// 渡すための取り出し。
    pub(crate) fn get(self) -> u32 {
        self.0
    }
}

/// [`FrameVisibleRows`] の唯一の構築点（#1106）——型がその証拠を担う。
pub(super) fn read_visible_rows(app: &tauri::AppHandle) -> FrameVisibleRows {
    FrameVisibleRows(max_results(app))
}

/// 動的高さ算出用の max_results（§4.5/§4.7）。visible_rows は `Option<usize>` のため
/// effective_visible_rows() で既定補完する（config.rs:327）。
///
/// **呼び出し点は `read_visible_rows` ただ 1 つである**（#1106 で `drive_results_window` の
/// 内側からの直読みを撤去した）。#749 は「読み点の制約を持たない」ことを理由にここで読んで
/// いたが、**起動側のゲートが同じ値を見るようになった時点で制約が生まれた**——読みが 2 つ
/// あると、同一フレームで表示は隠し起動は通す並びが構築できる。
fn max_results(app: &tauri::AppHandle) -> u32 {
    super::read_config(
        app,
        |c| c.appearance.effective_visible_rows() as u32,
        || AppearanceConfig::default().effective_visible_rows() as u32,
    )
}

/// `AppState.main_visible` の live-read。`drive_results_window` が show の事前ゲート
/// （`layout::present_results` の連言①）として 1 回読む。
///
/// AppState 不在は false（results を出さない側へ倒す）。
fn read_main_visible(app: &tauri::AppHandle) -> bool {
    app.try_state::<crate::AppState>()
        .map(|s| s.main_visible.load(Ordering::SeqCst))
        .unwrap_or(false)
}

/// `drive_results_window` の 1 フレーム分の入力（#749 段 1）。
///
/// **`result_count` の読み点は呼び出し側の責務である**（#752 F2 / ADR-results-presentation-two-stage）。同一フレーム内で
/// `plain_hidden` はクリック逆流の消費**前**、`result_count` は消費**後**に読む。
/// **この構造体を作る式を `plain_hidden` の算出の隣へ動かしてはならない**——行クリック起動
/// フレームで古い行が 1 フレーム描かれる。`cargo test` では落ちない種類の回帰である。
///
/// `width` と `row_height` と `visible_rows` は**それぞれ別種の制約**を持つ（混同しないこと）。
/// `row_height` はフレーム冒頭の `VisualSnapshot` 由来でなければならず（#673 決定 4: テーマ値は
/// 1 フレーム 1 回）、`width` は view が main へ適用するのと**同一フレームの同一値**でなければ
/// ならない（両窓の唯一の size writer が main である前提）。ゆえに内側で読み直さない。
///
/// **`visible_rows` の理由は上の 2 つのどちらでもない**（#1106）——**起動側のゲートと同じ
/// 1 回の読みでなければならない**。`launcher_controller` の `activate_or_execute` /
/// `shift_activate` が同じ値から連言④を導くので、読みが 2 つあると「表示は隠し、起動は通す」
/// 並びが同一フレーム内に構築できる。それは #1106 が実機で測った症状そのものである。
/// 理由を書かずに制約だけ書くと、次に読む人が上の 2 種のどちらかへ誤って分類する。
pub(crate) struct DriveResultsInputs {
    pub(crate) plain_hidden: bool,
    pub(crate) result_count: usize,
    pub(crate) width: f64,
    pub(crate) row_height: f64,
    pub(crate) visible_rows: FrameVisibleRows,
    /// results の下地（ネイティブ背景ブラシ）に使う背景色。**フレーム冒頭の `VisualSnapshot`
    /// 由来でなければならない**——ここで別に読むと `row_height` と別の lock になり、同じ
    /// フレームで新旧が混ざる（#673 決定 4・`row_height` と同じ理由）。
    pub(crate) background: egui::Color32,
}

/// results 窓の可視性・サイズ・位置を main から駆動する(#646 PR2 決定 6)。
/// 位置 = main の直下 + window_gap(従属)。デルタガードで無変化フレームは no-op。
/// show は focusable(false) 窓ゆえフォーカスを奪わない(決定 4)。
///
/// **hidden 窓は `update()` が走らないため自分では show できない**(SU5 要石)——毎フレーム走る
/// main 側からのみ駆動できる。呼び出し元は `SearchWindowView::update()` の末尾 1 か所である。
///
/// **`el` はイベントループスレッド上であることの証人である**（理由は `show_egui_main` の doc）。
/// 呼び出し元はフレームの中なので `RuntimeFrame::event_loop()` から得る。
/// `drive_results_window` の区間別の所要（#1032 の調査足場）。
///
pub(crate) fn drive_results_window(
    app: &tauri::AppHandle,
    el: &snotra_egui_runtime::EventLoopProof,
    i: DriveResultsInputs,
) {
    let Some(results) = app.try_state::<ResultsWindow>() else {
        return;
    };
    // 連言②の材料（件数）は**クリック逆流の消費後**に読む。③ `plain_hidden` は消費**前**に
    // 読んだ値を受け取る（#752 F2）。**この非対称は意図である**——読み点を揃えて前へ寄せる
    // と、行クリック起動フレームで古い行が 1 フレーム描かれる（`cargo test` では落ちない）。
    // **どちらも呼び出し側が読む**（#749 で driver が view から出たため、読み点は
    // `DriveResultsInputs` を組み立てる式の位置が決める）。
    let count = i.result_count;
    // main が hidden の間は results を出さない（#671 PR A′ レビュー Important 1）。
    // main_visible は hide_egui_main が results.hide() の**前**に false へ落とすため、
    // **この読みより前に store が済んでいたフレーム**はここで hide 側へ倒れる。判定式と
    // 根拠は layout::present_results（純粋核・ユニットテスト対象）。
    // **証人型（`EventLoopProof`）を引数に要求する 5 関数はイベントループスレッドへ閉じている**
    // ため、この読みと `results.show()` のあいだへ hide が割り込む並びは構築できない
    // （読んだ後に store されるフレームを心配する必要が無い）。**ただし `main_visible` を
    // 更新しない main の hide はこの閉包の外にある**——`state.rs` の同フィールドの doc を見よ。
    let main_visible = read_main_visible(app);
    let desired_height = match layout::present_results(layout::ResultsInputs {
        main_visible,
        plain_hidden: i.plain_hidden,
        result_count: count,
        // **`view.rs` が 1 回だけ読んだ値である**（#1106）——起動側のゲートと同じ 1 回に載せる
        // ため、ここで `max_results(app)` を読み直さない（同フィールドの doc）。
        max_results: i.visible_rows.get(),
        row_height: i.row_height,
    }) {
        layout::ResultsPresentation::Hidden => {
            // 可視フラグは ResultsWindow が持つ（#671 PR A′ spec 決定 2）。hide() は遷移した
            // ときだけ true を返すため、trace は 1 回だけ出る（毎フレーム撃たない）。
            // trace を型の内側でなく呼び出し側に置く理由は spec 決定 7。
            if results.hide(el) {
                crate::trace_main("egui_results:hide", serde_json::json!({ "from": "drive" }));
            }
            return;
        }
        layout::ResultsPresentation::Visible { desired_height } => desired_height,
    };
    // 位置: main の外形直下 + gap(物理座標。gap は論理 px を scale で換算)。無ガードの
    // 単一点(position_results_below_main)へ委譲——Moved リスナーと共用する
    // ため、デルタガードはヘルパー側に持たない(#646 PR2 決定 10)。
    position_results_below_main(app);
    // 高さは `present_results` が導いた値をそのまま渡す。**作業領域の下端によるクランプは
    // #835 で撤去した**——窓の大きさは表示位置で変わらず、収まらない分は画面外へはみ出す
    // （`layout::results_window_height` の doc・`ADR-results-fixed-height`）。
    //
    // デルタガードは `ResultsWindow::set_size` が内蔵する（#749 で移設）。**照合対象が
    // `set_size` の実引数と同じであること**——素の値を覚えると、毎フレーム撃つか必要な
    // 再サイズを撃たないかのどちらかになる——は、memo を撃つ側の内側へ入れたことで
    // 構造的に保たれる（渡した値がそのまま memo になる）。
    results.set_size(i.width, desired_height, i.background);
    // フォーカスを奪わない表示（tauri show() は SW_SHOW で活性化する・#646 PR2）。
    // 置き場の理由は上の hide 側コメントと同じ（spec 決定 7）。
    if results.show(el, i.background) {
        crate::trace_main("egui_results:show", serde_json::json!({ "rows": count }));
    }
    // 決定 5（#673 spec・#697）: この無条件 wake を edge 化してはならない。results は
    // config 系イベントを一切 listen せず（register_config_wake_listeners は wake_main のみ）、
    // visual-only の config 変更では RowsSnapshot が不変ゆえ snapshot 差分 wake も発火しない。
    // results が新しい色・フォント・行高を描くことを**保証する**唯一の経路がこの
    // level-triggered wake である（入力起因の偶発 wake でも描かれるが、それに依れない）。
    wake_results(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ネイティブブラシは alpha を 255 へ固定する。softbuffer の clear color が alpha を
    /// 持てないため、**下地と定常の背景が食い違わないよう不透明側へ揃える**（spec 決定 4）。
    /// hex から入る側の premultiply は `visual.rs` の
    /// `background_color_premultiplies_alpha_rather_than_ignoring_it` が測る（そちらが正本）。
    #[test]
    fn native_brush_forces_opaque_alpha() {
        let c = native_brush_color(egui::Color32::from_rgba_premultiplied(
            0x12, 0x34, 0x56, 0x80,
        ));
        assert_eq!((c.0, c.1, c.2, c.3), (0x12, 0x34, 0x56, 0xff));
    }

    /// runtime のフォールバック（`set_clear_color` を呼ばなかったフレームの色）が config の
    /// 既定背景色と一致することを**機構で**固定する。両 crate に依存するのはこの crate だけなので、
    /// 突き合わせられる位置がここしか無い。**この名前を名指す散文が他にある**——改名・移動する
    /// ならテスト名で grep して数え上げる（名前の実在を見る検査は無い）。
    #[test]
    fn runtime_fallback_matches_config_default_background() {
        let d = snotra_core::config::VisualConfig::default().background_color;
        let c = egui::Color32::from_hex(&d).unwrap();
        let packed = ((c.r() as u32) << 16) | ((c.g() as u32) << 8) | c.b() as u32;
        assert_eq!(packed, snotra_egui_runtime::CLEAR_COLOR);
    }
}
