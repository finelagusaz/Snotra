//! 窓の可視性・位置・サイズ・wake を駆動する 1 つの責務（#749 段 1）。
//!
//! 「撃つ主体」を集めた場所であって、「撃ってよいか」の判定は持たない——可視性の述語は
//! `layout::present_results`（純粋核・#752）、results の raw 操作の所有点は
//! `results_window::ResultsWindow`（#671 PR A′）である。
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
    let (f, rp, bp) = app
        .try_state::<crate::AppState>()
        .map(|s| {
            let engine = s.engine.lock().unwrap();
            let v = &engine.config().visual;
            (v.font_size, v.row_padding, v.bar_padding)
        })
        .unwrap_or_else(|| {
            // 既定 VisualConfig の正本は `visual::default_visual()`（`LazyLock` 静的）である——
            // ここで `VisualConfig::default()` を組むと String 6 本を毎回確保し、既定源も 2 つになる
            let v = super::visual::default_visual();
            (v.font_size, v.row_padding, v.bar_padding)
        });
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
    app.try_state::<crate::AppState>()
        .map(|s| f64::from(s.engine.lock().unwrap().config().appearance.window_width))
        .unwrap_or_else(|| f64::from(AppearanceConfig::default().window_width))
}

/// index 構築中か（show 経路が status 行の有無を導くために読む）。正本は `AppState.indexing`。
/// 毎フレーム側は `launcher_controller::LauncherController::indexing` がこの実装へ委譲する
/// （両者がバイト単位で同一実装を独立に持っていた重複の解消・レビュー是正 3）。
pub(super) fn read_indexing(app: &tauri::AppHandle) -> bool {
    app.try_state::<crate::AppState>()
        .map(|s| s.indexing.load(Ordering::Relaxed))
        .unwrap_or(false)
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
/// 読みで、**1 フレーム 1 lock の規律（#673 決定 4）が掛かる面には居ない**——同じ関数内の
/// `read_metrics` や `follow_cursor_monitor` / `ime_off_on_show` の読みと同じ層である。
pub(crate) fn read_background(app: &tauri::AppHandle) -> egui::Color32 {
    let hex = app
        .try_state::<crate::AppState>()
        .map(|s| {
            s.engine
                .lock()
                .unwrap()
                .config()
                .visual
                .background_color
                .clone()
        })
        .unwrap_or_else(|| super::visual::default_visual().background_color.clone());
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
#[cfg(windows)]
fn position_on_target_monitor(
    app_handle: &tauri::AppHandle,
    // &Window に一般化して egui 経路と共有（#532 SU2）。両経路とも同一の "main" 窓
    // （get_window/get_webview_window は同じ内部 Window を指す・manager/window.rs:106）。
    main: &tauri::Window,
) {
    use snotra_core::window_data;

    // Read follow_cursor_monitor from Engine config (refreshed on every show).
    let follow_cursor = app_handle
        .try_state::<crate::AppState>()
        .map(|s| {
            s.engine
                .lock()
                .unwrap()
                .config()
                .general
                .follow_cursor_monitor
        })
        .unwrap_or_else(|| GeneralConfig::default().follow_cursor_monitor);

    // Determine target monitor work area.
    let target_wa = if follow_cursor {
        crate::monitor::cursor_monitor_work_area()
    } else {
        crate::monitor::primary_monitor_work_area()
    };
    let Some(target_wa) = target_wa else { return };

    // Get current window size (physical) for centering/clamping.
    let Ok(win_size) = main.outer_size() else {
        return;
    };
    let win_w = win_size.width as i32;
    let win_h = win_size.height as i32;

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
    // 高さ決定 → 位置 → show の順（旧 WebView2 経路から引き継いだ順序制約。#755 / #801 の
    // 修正後もこの順序は不変——畳む先の値だけが変わった）。
    // **「高さ決定」は 1 回では済まない**（レビュー是正 1）: position のクランプ
    // （`monitor.rs` の `WorkArea::clamp`・`max_y = bottom - win_h`）は
    // `position_on_target_monitor` が `outer_size()` で読み戻す「その時点の窓サイズ」に対して
    // 効く。status / toast 込みの実高をそのまま渡すと、作業領域の下端付近では常にその分だけ
    // 窓が上へ押し戻される——毎フレーム経路（`view.rs`）は `set_size` しか呼ばないため、
    // toast が消えて窓が縮んでも位置は戻らず、次の hide が `read_placement_relative` でその
    // ずれた位置を永続化する。**バーの位置はユーザーが決め、行の出没では動かさない**
    // （人間裁定・2026-08-04）という仕様の帰結として、位置はバー高で決め、サイズは実高で
    // 決める——ここでは 2 回に分けて set_size する。
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
            indexing_now,
            /* results_view */ true,
            /* launching    */ false,
            /* has_notice   */ false,
        );
        let height = layout::main_window_height(
            m.bar_height,
            status.then_some(m.toast_height),
            toast_now.then_some(m.toast_height),
        );
        // 1 手目: バー高だけで position のクランプ材料を確定する。
        let _ = window.set_size(tauri::LogicalSize::new(width, m.bar_height));
        position_on_target_monitor(app, &window);
        // 2 手目: 実高（#755 / #801 の修正が導く「そのフレームで実際に描かれる高さ」）へ
        // 書き直す。2 手の間はフレームが 1 枚も描かれない（窓はまだ hidden）ため、
        // 視覚的な影響は無い。
        let _ = window.set_size(tauri::LogicalSize::new(width, height));

        // 不変条件検出器（レビュー是正 4）: 仕様は「高さは『いま描く行』で決まり、高さの変化は
        // 行の出没と 1 対 1 で対応する」。ここで読んだ生の入力と適用した高さを残し、`view.rs` の
        // reset-on-show 消費フレームが「行は変わっていないのに高さが変わった」を突き合わせる。
        // **述語へ渡したリテラル（上の `launching`/`has_notice` の `false`）ではなく、読んだ値
        // そのものを残す**——将来 show 側が「読んだが渡さない」形へ退行しても拾えるようにする。
        if let Some(sh) = app.try_state::<EguiShellState>() {
            sh.show_read_indexing.store(indexing_now, Ordering::SeqCst);
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
        let ime_control = app
            .try_state::<crate::AppState>()
            .map(|s| s.engine.lock().unwrap().config().general.ime_off_on_show)
            .unwrap_or_else(|| GeneralConfig::default().ime_off_on_show);
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
    // `read_placement_relative` の doc）。`read_metrics` は engine lock をクロージャ内で
    // 取り切って返すため、ここで保持は残らない。
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
/// **基準モニターは `clamp_main_into_work_area` と同じ「バー矩形の中心」から引く**（#738）。
/// `MonitorFromWindow`（窓全体の矩形）だと、status 行・toast 行で伸びた窓の重なりによって
/// 隣モニターが選ばれ、**保存座標の原点が行の出没で変わる**——次の show がその相対座標を
/// 別モニターの原点へ当てるため、**バーがモニター 1 枚ぶん飛ぶ**。しかも飛ぶかどうかが
/// 「hide の瞬間に行が出ていたか」で決まる。`SPEC.md` §4.7「バーの位置は行の出没で
/// 動かさない」を破る経路であり、クランプ側と同じ基準に揃えて閉じてある。
pub(crate) fn read_placement_relative(
    window: &tauri::Window,
    bar_height: f64,
) -> Option<snotra_core::window_data::WindowPlacement> {
    let pos = window.outer_position().ok()?;
    #[cfg(windows)]
    {
        use snotra_core::window_data::WindowPlacement;
        let (Ok(outer), Ok(inner), Ok(scale)) = (
            window.outer_size(),
            window.inner_size(),
            window.scale_factor(),
        ) else {
            return None;
        };
        let bar_h = layout::bar_rect_height_phys(bar_height, scale)
            + (outer.height as i32 - inner.height as i32);
        let (cx, cy) = layout::bar_rect_center(pos.x, pos.y, outer.width, bar_h);
        let wa = crate::monitor::point_monitor_work_area(cx, cy)?;
        Some(WindowPlacement {
            x: pos.x - wa.left,
            y: pos.y - wa.top,
        })
    }
    #[cfg(not(windows))]
    {
        use snotra_core::window_data::WindowPlacement;
        let _ = bar_height;
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

/// 可視中の main を作業領域の内側へ戻す（#738）。**バー矩形だけを対象にする。**
///
/// **呼び出し条件はここに無い**——`view.rs` が「ポインタが押されていないフレーム」でのみ
/// 呼ぶ。ドラッグ中も毎フレーム戻すと、横並びモニター間の移動が**封鎖される**: 幅 600px の
/// 窓を A(`0..1920`) から右へ運ぶとき、左端が `1320..1620` の区間ではまだ A の重なりが優勢で
/// あり、毎回 `x=1320` へ引き戻されて B が優勢になる位置へ到達できない。ゆえに保証は
/// 「ドラッグ中も出られない」ではなく**「離したら戻る」**である（人間裁定・2026-08-04。
/// 前者には `WM_MOVING` のフック＝tao の wndproc サブクラス化が要り、却下した）。
///
/// **`show_egui_main` の `position_on_target_monitor` とは基準モニターの決め方が違う。**
/// あちらは「これから出す窓をどこへ置くか」ゆえカーソル/プライマリを見るが、こちらは
/// 「いまある窓をどこへ戻すか」ゆえ**バー矩形の中心**が乗るモニターを見る（理由は
/// `monitor::point_monitor_work_area` の doc）。
///
/// 高さは `layout::bar_rect_height_phys`（content）＋ `outer - inner`（非クライアント）で
/// 組む。`position_on_target_monitor` が `outer_size()` の読み戻しで OS から受け取っている
/// 分を、読み戻しの無いこの経路では自分で足す必要がある。
///
/// 取得に 1 つでも失敗したら**クランプしない側へ倒す**（`position_on_target_monitor` と同じ）。
#[cfg(windows)]
pub(crate) fn clamp_main_into_work_area(app: &tauri::AppHandle, bar_height: f64) {
    let Some(main) = app.get_window("main") else {
        return;
    };
    let (Ok(pos), Ok(outer), Ok(inner), Ok(scale)) = (
        main.outer_position(),
        main.outer_size(),
        main.inner_size(),
        main.scale_factor(),
    ) else {
        return;
    };
    // 非クライアント分は OS から取る。**この項は「将来の保険」ではなく今すでに効いている**
    // ——`decorations: false` でも DWM の影が乗るため、実測で 10 物理 px あった
    // （DPI 125% の環境で `outer - inner = 10`・#738 のカテゴリ D）。落とすとその分だけ
    // バーが作業領域からはみ出す。
    let chrome_h = outer.height as i32 - inner.height as i32;
    let bar_h = layout::bar_rect_height_phys(bar_height, scale) + chrome_h;
    let (cx, cy) = layout::bar_rect_center(pos.x, pos.y, outer.width, bar_h);
    let Some(area) = crate::monitor::point_monitor_work_area(cx, cy) else {
        return;
    };
    // 算術は `WorkArea::clamp`（ユニットテスト 7 件が固定する既存の純粋核）。**新しい算術を
    // 書かない**——show 経路と同じ導出を通すことが本修正の要点である（#877 と同型）。
    let (nx, ny) = area.clamp(pos.x, pos.y, outer.width as i32, bar_h);
    // 同値なら撃たない。`set_position` は Win32 呼び出しであり、可視中は毎フレーム通る。
    if nx != pos.x || ny != pos.y {
        let _ = main.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
            nx, ny,
        )));
    }
}

#[cfg(not(windows))]
pub(crate) fn clamp_main_into_work_area(_app: &tauri::AppHandle, _bar_height: f64) {}

/// results を main の直下 + window_gap に配置する(#646 PR2 決定 6)。呼び出し元は
/// 2 つ——main の update()(通常の毎フレーム従属)と main の Moved リスナー
/// (ネイティブ移動ループ中の追従。ループ中は egui フレームが回らない可能性があるため
/// イベント駆動で直接動かす)。デルタガードは持たない(set_position は同値でも安価・
/// サイズ側のガードとは対称でない。理由は `ResultsWindow::set_size` の doc)。
/// **設定した results 上端の物理 y を返す**（#675）。高さのクランプに上端が要るが、上端の
/// 算出式の正本は `layout::results_top_y`（純粋核・#752 C1）で、**Win32 を読んでそれを適用
/// する場所はここだけ**である。呼び出し側で再計算すると `outer_position` / `outer_size` /
/// `window_gap` の 2 度読みになり、フレーム内で値が食い違いうる（`AGENTS.md`「条件別チェック」
/// の「重複した読み」）。
/// **計算した値を捨てる関数は、次の利用者に写しを書かせる。**
pub(crate) fn position_results_below_main(app: &tauri::AppHandle) -> Option<i32> {
    let (Some(main), Some(results)) = (app.get_window("main"), app.try_state::<ResultsWindow>())
    else {
        return None;
    };
    let gap = app
        .try_state::<crate::AppState>()
        .map(|s| s.engine.lock().unwrap().config().visual.window_gap)
        .unwrap_or_else(|| super::visual::default_visual().window_gap);
    let (Ok(pos), Ok(size), Ok(scale)) = (
        main.outer_position(),
        main.outer_size(),
        main.scale_factor(),
    ) else {
        return None;
    };
    // 算術は layout::results_top_y（純粋核・#752 C1）。Win32 の読みはここで 1 回だけ行う。
    let top = layout::results_top_y(pos.y, size.height, gap, scale);
    results.set_position(pos.x, top);
    Some(top)
}

/// results 上端から作業領域の下端までの高さ（**論理 px**・#675）。取得できなければ `None`
/// （呼び出し側がクランプしない側へ倒す）。
///
/// 作業領域は **main の HWND** から引く——results は既に誤った位置へ置かれている可能性があり、
/// そこから引くと別モニターの作業領域を掴みうる。
///
/// **ここは `MonitorFromWindow`（窓全体の矩形）のままでよい**（#738 でクランプと hide 保存を
/// バー矩形中心へ揃えたが、この 3 件目は揃えない）。results は main の**下**に置かれるため、
/// main が下側モニターへ伸びている状況では、伸びた先のモニターの `bottom` で高さを切るほうが
/// 意図に合う。基準を揃えるべきなのは「バーの位置を決める／記録する」経路だけである。
///
/// 換算に使うのは **results 窓の scale factor** である。tao は `set_inner_size` に渡した
/// `LogicalSize` を**その窓の** `scale_factor()` で物理へ戻すため、main の scale を流用すると
/// 混在 DPI 環境で高さが食い違う。**受容する残余**: `set_position` 直後は tao 側の scale が
/// まだ旧モニターのものでありうる（Windows は移動後に `WM_DPICHANGED` を送る）。実害は
/// モニター跨ぎの 1 フレームに限られる見込みで、是正しない。
#[cfg(windows)]
fn results_available_height(app: &tauri::AppHandle, top_y: i32) -> Option<f64> {
    let main = app.get_window("main")?;
    let hwnd = main.hwnd().ok()?;
    let area = crate::monitor::window_monitor_work_area(hwnd.0 as isize)?;
    let scale = app.try_state::<ResultsWindow>()?.scale_factor()?;
    // 算術は layout::available_below（純粋核・cfg の外・#752 C1）。
    Some(layout::available_below(area.bottom, top_y, scale))
}

#[cfg(not(windows))]
fn results_available_height(_app: &tauri::AppHandle, _top_y: i32) -> Option<f64> {
    None
}

/// 動的高さ算出用の max_results（§4.5/§4.7）。visible_rows は `Option<usize>` のため
/// effective_visible_rows() で既定補完する（config.rs:327）。
///
/// **読み点の制約を持たない**ため `DriveResultsInputs` へは載せず、driver の内側で読む
/// （#749）——引数を増やすほど、呼び出し側で読み点の違う値を並べて書きたくなる。
fn max_results(app: &tauri::AppHandle) -> u32 {
    app.try_state::<crate::AppState>()
        .map(|s| {
            s.engine
                .lock()
                .unwrap()
                .config()
                .appearance
                .effective_visible_rows() as u32
        })
        .unwrap_or_else(|| AppearanceConfig::default().effective_visible_rows() as u32)
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
/// `width` と `row_height` は**別種の制約**を持つ（混同しないこと）。`row_height` はフレーム
/// 冒頭の `VisualSnapshot` 由来でなければならず（#673 決定 4: テーマ値は 1 フレーム 1 回）、
/// `width` は view が main へ適用するのと**同一フレームの同一値**でなければならない
/// （両窓の唯一の size writer が main である前提）。ゆえに内側で読み直さない。
pub(crate) struct DriveResultsInputs {
    pub(crate) plain_hidden: bool,
    pub(crate) result_count: usize,
    pub(crate) width: f64,
    pub(crate) row_height: f64,
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
        max_results: max_results(app),
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
    let top_y = position_results_below_main(app);
    // 作業領域の下端でクランプする（#675）。あふれた行は既存の ScrollArea が拾う。
    //
    // **可視判定（上の `present_results`）には `desired_height`（クランプ前）を使う。**
    // クランプには上端が要り、上端は直上の位置決めが決めるが、位置決めは可視判定の
    // **後**にある（不可視なら早期 return する）。判定にクランプ後の値を使おうとすると
    // 位置決めを判定より前へ動かすことになり、不可視フレームでも `SetWindowPos` を
    // 撃つ——#646 PR2 決定 10 の設計を変えてしまう。クランプは `set_size` に渡す値だけに
    // 効かせ、「0 件 ⇔ 高さ 0 ⇔ hide」の契約を判定側で無傷に保つ。
    //
    // **`desired_height` と `applied_height` は別名にしてある**（#752 F5）。旧実装は同一
    // 関数内で `res_h` を 2 度束縛しており、デルタガードがどちらを覚えるべきかが名前から
    // 読めなかった。覚えるのは**クランプ後**である。
    let applied_height = layout::clamp_results_height(
        desired_height,
        top_y.and_then(|y| results_available_height(app, y)),
        i.row_height,
    );
    // デルタガードは `ResultsWindow::set_size` が内蔵する（#749 で移設）。**照合対象が
    // `set_size` の実引数と同じであること**——素の値を覚えると、毎フレーム撃つか必要な
    // 再サイズを撃たないかのどちらかになる——は、memo を撃つ側の内側へ入れたことで
    // 構造的に保たれる（渡した値がそのまま memo になる）。
    results.set_size(i.width, applied_height, i.background);
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
