//! egui/softbuffer メインウィンドウの外殻（#532 SU2〜SU7・唯一の UI 経路）。
//! window 生成・show/hide・blur 自動非表示・位置永続。
mod icon_textures;
mod lifecycle;
mod search_state;
mod layout;
mod notify;
// view.rs（driver）が起動 worker の in-flight 追跡・一時通知で消費する（#532 SU5 Task 4）。
pub(crate) use notify::{LAUNCH_TIMEOUT, NOTICE_LAUNCH, NoticeSlot};
// view.rs（driver）が overlay 優先ラダー・hotkey 失敗通知の duration で消費する（#532 SU6 Task 7）。
pub(crate) use notify::{NOTICE_HOTKEY, OverlayKind, overlay_kind};
// mod.rs の spawn_update_check が phase 書き込みで、UpdaterUiState が Default で消費する
// （#532 SU5 Task 6）。toast 描画は view.rs が Task 7 で消費する。
pub(crate) use notify::{ToastKind, UpdaterPhase, UpdaterUi};
pub(crate) mod strings;
mod results_view;
mod view;

// mod.rs（窓生成・managed state）が消費する。RowsSnapshot は view.rs（main の snapshot 発行）・
// results_view.rs（update() 描画）が消費する（#646 PR2 Task 4）。
pub(crate) use results_view::ResultsShared;
pub(crate) use results_view::RowsSnapshot;

// view.rs の icon texture driver（worker spawn / load_texture 適用）が消費する（#532 SU4 Task 5）。
pub(crate) use icon_textures::{IconMsg, needs_extraction, png_to_color_image, retain_visible};
pub(crate) use lifecycle::{HotkeyPlan, blur_should_hide, plan_hotkey};
// view.rs（driver）が folder 展開（#532 SU3 M2）で消費する。
pub(crate) use search_state::{
    EscapeOutcome, QueryIntent, SearchState, ViewKind, compute_parent_dir, folder_load_pending,
    should_flush_on_enter,
};
// SlashCmd/find_slash_command は driver（view.rs）が command 分岐・slash 実行で消費する（#532 SU3 M3 Task 2）。
pub(crate) use search_state::{SlashCmd, find_slash_command};
// driver（view.rs）は Task 4 で表示ゲート・再検索トリガとして消費する（#532 SU6 Task 1）。
pub(crate) use search_state::{needs_index_refresh, plain_results_hidden};
pub(crate) use layout::Debouncer;
// view.rs が UI 文言（hint/overlay/toast）で消費する（#532 SU5・言語は lang() が毎フレーム live-read）。
pub(crate) use strings as ui_strings;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use snotra_egui_runtime::EguiRuntime;
use tauri::{Listener, Manager};

use crate::egui_shell::view::SearchWindowView;

/// hotkey 登録失敗の種別（#652）。文言キーが別（i18n.ts `notice.hotkey.*`）ゆえ
/// pending に載せて view 側で整形を分岐する。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HotkeyFailureKind {
    /// 起動時の初回登録失敗（`platform-event` / `initial-hotkey-failed`）。
    /// SPEC §10 のとおり窓を能動表示してから通知する
    /// （listener は `register_platform_event_listener`・#652 Task 4）。
    Initial,
    /// 設定変更による再登録失敗（`hotkey-registration-failed`）。旧ホットキーを維持。
    Change,
}

/// egui 経路の show/hide を跨ぐ共有状態（managed state）。
/// - hotkey_generation: alt 解放待ち show の世代。hide が bump して保留 show を無効化する（codex #5/(B)#2）。
/// - hide_pending: view の emit dedup。show がクリアして「hide 後に Focused(true) が来ず抑止が残る」を断つ（codex #8）。
/// - reset_pending: show が立て、view が消費して state.reset()（resetForShow 相当・SU3 M1 Task 9）。
#[derive(Default)]
pub(crate) struct EguiShellState {
    pub(crate) hotkey_generation: AtomicU64,
    pub(crate) hide_pending: AtomicBool,
    pub(crate) reset_pending: AtomicBool,
    /// updater check 完了時に可視中の view を起こすための egui Context（view.setup が登録）。
    /// hidden 中は次 show のフレームで toast が読まれるため repaint は可視中のみ意味を持つ
    /// （codex レビュー: 「hidden は次 show でよい」と「visible は repaint が要る」は別条件）。
    pub(crate) egui_ctx: Mutex<Option<egui::Context>>,
    /// results 窓の egui Context（外部 wake 用・egui_ctx と同型・#646 PR2）。
    pub(crate) results_ctx: Mutex<Option<egui::Context>>,
    /// hotkey 登録失敗の pending payload（SU6 spec 追補 2 + #652）。種別ごとに文言が違う
    /// ため `(kind, hotkey)` を保持し、view が消費時に lang() live-read で整形する。
    /// **wake の有無は経路で異なる**——Change は wake しない（wake を config-applied に
    /// 委ね、言語同時変更で旧言語整形になる競合窓を閉じる）。この競合窓が閉じる根拠は
    /// `apply_config_change` が engine への `update_config` 適用**後**に `config-applied` を
    /// emit する順序——wake 時の lang() live-read は必ず新言語を読む（旧 `language-changed`
    /// 先行発火の不変条件は #532 SU7 の emit 削除で消滅し、この順序が後継の根拠）。
    /// Initial は wake する（config 変更が随伴せず config-applied が来ないため・#652・SU6.5 決定 2）。
    pub(crate) pending_hotkey_failure: Mutex<Option<(HotkeyFailureKind, String)>>,
}

/// updater toast の managed 状態（#532 SU5）。view が毎フレーム読む level-triggered
/// （hidden に頑健・launching の channel edge-trigger との構造的対比は spec C 節）。
/// dismissed は view-local に置かない——reset-on-show が view-local を一掃した際に
/// `[閉じる]` 済み toast が復活するため（状態機械レビュー・spec A 節）。
pub(crate) struct UpdaterUiState(pub(crate) Mutex<crate::egui_shell::UpdaterUi<Box<tauri_plugin_updater::Update>>>);

/// 起動時 updater check（§20.2・spec B 節）。`auto_update != disabled` で一回だけ呼ぶ。
/// `on_before_exit` に終了保存を登録した builder で check する——ここで得た `Update` の
/// install は「download → 保存 → installer 起動 → exit(0)」となり、保存が構造的に保証される
/// （Windows では downloadAndInstall が復帰しない・updater.rs:865・spec「決着済み: 保存順序」）。
pub(crate) fn spawn_update_check(app: &tauri::AppHandle) {
    use snotra_core::config::AutoUpdateMode;
    use tauri_plugin_updater::UpdaterExt;
    // 視覚スモーク専用の env エスケープハッチ:
    // 実 release への依存なしに toast を表示する。install 実体は無い（update: None）。
    if crate::trace::env_flag("SNOTRA_EGUI_FAKE_UPDATE") {
        if let Some(st) = app.try_state::<UpdaterUiState>() {
            st.0.lock().unwrap().phase = crate::egui_shell::UpdaterPhase::Available {
                version: "9.9.9".into(),
                can_install: true,
                update: None,
            };
        }
        return;
    }
    let mode = app
        .try_state::<crate::AppState>()
        .map(|s| s.engine.lock().unwrap().config().general.auto_update)
        // AppState は setup 前に managed されるため実運用では到達不能。到達したら
        // 「設定を読めていない」状態なので、勝手に更新を始めない Disabled へ倒す（#648 F）。
        .unwrap_or(AutoUpdateMode::Disabled);
    if mode == AutoUpdateMode::Disabled {
        return;
    }
    let can_install = mode == AutoUpdateMode::Full;
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Some(st) = handle.try_state::<UpdaterUiState>() {
            st.0.lock().unwrap().phase = crate::egui_shell::UpdaterPhase::Checking;
        }
        let flush_handle = handle.clone();
        let updater = handle
            .updater_builder()
            .on_before_exit(move || crate::flush_persistent_state(&flush_handle))
            .build();
        let next = match updater {
            Ok(u) => match u.check().await {
                Ok(Some(update)) => crate::egui_shell::UpdaterPhase::Available {
                    version: update.version.clone(),
                    can_install,
                    update: Some(Box::new(update)),
                },
                Ok(None) => crate::egui_shell::UpdaterPhase::UpToDate,
                Err(e) => {
                    // check 失敗は無音（console.warn parity・trace のみ）。
                    crate::trace_main("egui_update_check_failed", serde_json::json!({ "error": e.to_string() }));
                    crate::egui_shell::UpdaterPhase::Idle
                }
            },
            Err(e) => {
                crate::trace_main("egui_update_check_failed", serde_json::json!({ "error": e.to_string() }));
                crate::egui_shell::UpdaterPhase::Idle
            }
        };
        if let Some(st) = handle.try_state::<UpdaterUiState>() {
            st.0.lock().unwrap().phase = next;
        }
        // 可視中に check が完了した場合の wake-up(スパイクの request_repaint と同じ・codex レビュー)。
        wake_view(&handle);
    });
}

/// フラグ ON の窓生成。EguiRuntime を install し webview 無しの "main" 窓を生成して attach。setup 限定。
/// 宣言窓の全プロパティ（52px 高は初期値〔SU3 で show 前折り畳み + 結果表示時に動的リサイズ・view.rs〕・width は config の window_width・skipTaskbar・
/// alwaysOnTop・decorations:false・resizable:false・visible:false）を再現する（codex #11・(B)#1）。
/// `background_color_hex`: config `visual.background_color`（`#RRGGBB`）。過渡/リサイズ下地の
/// SU2 ハードコード 0x282828 を config へ差し替える（§11・#532 SU4 Task 2）。パース失敗時は
/// 従来の 0x282828 へ fallback（`config_watcher::parse_hex_color` を再利用・二重実装回避）。
pub(crate) fn create(
    app: &mut tauri::App,
    window_width: f64,
    background_color_hex: &str,
) -> Result<(), snotra_egui_runtime::RuntimeError> {
    let runtime = EguiRuntime::new();
    runtime.install(app); // install(&self, &mut App<Wry>)（runtime.rs:77）
    let app_handle = app.handle().clone();
    let bg_color = crate::config_watcher::parse_hex_color(background_color_hex)
        .unwrap_or(tauri::window::Color(0x28, 0x28, 0x28, 0xff));
    let window = tauri::Window::builder(app, "main")
        .title("Snotra")
        .inner_size(window_width, 52.0) // 保存幅を尊重（codex #11）。高さは初期値。実高は show 時に Metrics で再設定(#646)
        .decorations(false)
        .resizable(false)
        .skip_taskbar(true) // 宣言窓 skipTaskbar:true の再現（(B)#1）
        .always_on_top(true) // 宣言窓 alwaysOnTop:true の再現（(B)#1）
        // 白フラッシュ回避: show 時、最初の softbuffer present 前にネイティブ背景ブラシが一瞬見える。
        // softbuffer の CLEAR_COLOR（renderer.rs=0x282828）に合わせて config テーマ色にし、
        // 白→暗の点滅を消す（既定は従来どおり 0x282828）。
        .background_color(bg_color)
        .visible(false)
        .build()?; // tauri::Error → RuntimeError（#[from]・runtime.rs:46）

    // #646 PR2: 結果リスト窓。focusable(false) で tao が WS_EX_NOACTIVATE を自動適用し
    // (tao window_state.rs: !FOCUSABLE → style_ex |= WS_EX_NOACTIVATE)、クリックしても
    // フォーカスはメインの入力欄から動かない（決定 4）。可視性・サイズ・位置は main の
    // update() が駆動する（hidden 窓は update() が走らないため自分では show できない）。
    let results = tauri::Window::builder(app, "results")
        .title("Snotra Results")
        .inner_size(window_width, 100.0) // 初期値。実高は main が実件数フィットで設定
        .decorations(false)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .focusable(false)
        .background_color(bg_color)
        .visible(false)
        .build()?;
    #[cfg(windows)]
    {
        apply_rounded_corners(&window); // main にも適用（輪郭言語を揃える・決定 4）
        apply_rounded_corners(&results);
    }
    runtime.attach(results, results_view::ResultsView::new(app_handle.clone()))?;
    // #646 PR2 決定 10: ドラッグ移動中の追従。ネイティブ移動ループ中は egui フレームが
    // 回る保証が無いため、tao の Moved イベント(tauri Window リスナー経由)で直接
    // results を追従させる。通常時の従属は main update() の drive が担う(二重呼びは
    // set_position の同値上書きで無害)。attach で window が move される前に登録する。
    {
        let handle = app_handle.clone();
        window.on_window_event(move |event| {
            if matches!(event, tauri::WindowEvent::Moved(_)) {
                position_results_below_main(&handle);
            }
        });
    }
    runtime.attach(window, SearchWindowView::new(app_handle))
}

/// DWM に窓の角丸を依頼する（#646 PR2 決定 4）。Windows 11（build 22000+）のみ有効で、
/// Windows 10 ではエラーを黙って握りつぶす（装飾なしで受容・best-effort）。
/// softbuffer は AA を持たず自前角丸は品質が出ないため OS 機構に委ねる。
#[cfg(windows)]
fn apply_rounded_corners(window: &tauri::Window) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DWM_WINDOW_CORNER_PREFERENCE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
        DwmSetWindowAttribute,
    };
    let Ok(hwnd) = window.hwnd() else { return };
    let pref: DWM_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND;
    unsafe {
        let _ = DwmSetWindowAttribute(
            HWND(hwnd.0),
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &pref as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        );
    }
}

/// 実行中 config から Metrics を導出する(#646 決定 2)。毎フレーム/毎 show の live-read で
/// キャッシュしない。view(update)と show 経路の両方がここを通ることで、導出式とフォールバック
/// を単一点に保つ(/simplify: 独立実装 2 箇所でフォールバックが 52.0/43.0 に乖離していた)。
/// AppState 不在(setup 完了前の理論経路のみ)は `VisualConfig::default()` から導出——
/// 既定値の正本(config.rs の default_*)に追従し、リテラル再手打ちを持たない。
pub(crate) fn read_metrics(app: &tauri::AppHandle) -> layout::Metrics {
    let (f, rp, bp) = app
        .try_state::<crate::AppState>()
        .map(|s| {
            let engine = s.engine.lock().unwrap();
            let v = &engine.config().visual;
            (v.font_size, v.row_padding, v.bar_padding)
        })
        .unwrap_or_else(|| {
            let v = snotra_core::config::VisualConfig::default();
            (v.font_size, v.row_padding, v.bar_padding)
        });
    layout::Metrics::from_config(f, rp, bp)
}

/// egui 経路の show。共有するのは position_on_target_monitor のみ。全 hide は外部化ゆえ
/// runtime.visible は false にならず、show は Focused(true) に依存せず確実に描ける（codex #4）。
/// show 列は WebView2 の show_and_focus_main を egui 用に自前複製（WebView2 本体を触らないため）。
pub(crate) fn show_egui_main(app: &tauri::AppHandle, t0: Instant) {
    let Some(window) = app.get_window("main") else {
        crate::trace_main("egui_show:no_window", serde_json::json!({}));
        return;
    };
    // show のたびに view の emit dedup をリセット（Focused(true) 非依存・codex #8）。
    if let Some(sh) = app.try_state::<EguiShellState>() {
        sh.hide_pending.store(false, Ordering::SeqCst);
        sh.reset_pending.store(true, Ordering::SeqCst); // resetForShow を view に指示
    }
    // 高さリセット → 位置 → show の順（SU2 の show_main_and_emit と同じ制約）。
    // reset-on-show でクエリは空 = 結果なし = bar_height（既定 43px）。前回 hide 時に展開高
    // （例 300px）のままだと position クランプが 300px で効き、show 後に view が bar_height へ
    // collapse して視覚スナップ + 位置ずれになる。position の前に bar_height へ collapse して
    // これを断つ（SU3 で高さが動的化したため、旧「52px は create で固定・位置のみ復元」前提は
    // 崩れている）。
    #[cfg(windows)]
    {
        let width = window
            .inner_size()
            .ok()
            .map(|s| s.to_logical::<f64>(window.scale_factor().unwrap_or(1.0)).width)
            .unwrap_or(600.0);
        // 折りたたみ高 = bar_height(#646 決定 2)。52 固定だと font 連動後の実バー高と
        // ずれ、position クランプが誤った高さで効く(このブロック冒頭の reset-on-show
        // コメントの機構と同じ理由。行番号参照は挿入でずれるため名前で指す)。
        let bar_h = read_metrics(app).bar_height;
        let _ = window.set_size(tauri::LogicalSize::new(width, bar_h));
    }
    #[cfg(windows)]
    crate::position_on_target_monitor(app, &window);
    let _ = window.show();
    // main_visible は show() の後に立てる（WebView2 の show_and_focus_main と同じ「順序不変」
    // 制約）。show 完了前に visible=true を読んだホットキートグルが hide するのを避ける。
    if let Some(state) = app.try_state::<crate::AppState>() {
        state.main_visible.store(true, Ordering::SeqCst);
    }
    let _ = window.set_focus();
    // フォーカス移行の同期待ち（SetForegroundWindow は部分的に非同期・Raymond Chen）。
    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{SMTO_NORMAL, SendMessageTimeoutW, WM_NULL};
        let hwnd = HWND(hwnd.0);
        let mut result = 0usize;
        unsafe {
            let _ = SendMessageTimeoutW(
                hwnd,
                WM_NULL,
                WPARAM(0),
                LPARAM(0),
                SMTO_NORMAL,
                100,
                Some(&mut result),
            );
        }
    }
    // 残留 Alt 解除: focus 確定後かつ物理 Alt 解放後のみ（#558）。
    if !crate::is_alt_pressed() {
        crate::send_alt_key_up();
    }
    // §12: 表示時 IME オフ（設定有効時・復元なし・SU6 spec 決定 4）。ime_off_on_show は実行中
    // config から都度読み（キャッシュしない・#576 同型——config_watcher の hot-reload が diff/event
    // 追加なしに届く）。**focus 同期（上の SendMessageTimeoutW）より後に置く**——前だと IME オフが
    // 対象窓に効かない（WebView2 apply_ime_control doc の警告条件）。Win32 は PlatformBridge 経由
    // （rule）。TurnOffIme は生 HWND(usize) を取るため窓型非依存で &Window 一般化は不要。
    #[cfg(windows)]
    {
        let ime_control = app
            .try_state::<crate::AppState>()
            .map(|s| s.engine.lock().unwrap().config().general.ime_off_on_show)
            .unwrap_or(false); // config.rs の既定値と一致
        if ime_control
            && let Some(bridge) = app.try_state::<std::sync::Mutex<crate::platform::PlatformBridge>>()
            && let Ok(b) = bridge.lock()
            && let Ok(hwnd) = window.hwnd()
        {
            b.send_command(crate::platform::PlatformCommand::TurnOffIme(hwnd.0 as usize));
            crate::trace_main("egui_show:ime_control", serde_json::json!({}));
        }
    }
    crate::trace_main(
        "egui_show:done",
        serde_json::json!({ "ms": t0.elapsed().as_secs_f64() * 1000.0 }),
    );
}

/// egui 経路の hide。全 hide の唯一の副作用所有点（codex #7）。外部 window.hide() のみで
/// runtime.visible を false にしない（空白窓回避・codex #4）。
pub(crate) fn hide_egui_main(app: &tauri::AppHandle) {
    // 保留中の alt 解放待ち show を無効化（codex #5/(B)#2）: 世代を bump し、spawn 済み show
    // スレッドの gen 一致チェックを外す。
    if let Some(sh) = app.try_state::<EguiShellState>() {
        sh.hotkey_generation.fetch_add(1, Ordering::SeqCst);
    }
    if let Some(window) = app.get_window("main") {
        save_placement_relative(&window); // save-on-hide
        let _ = window.hide();
    }
    // #646 PR2: 従属窓も同時に隠す（決定 6）。show 側は main の update() が snapshot の
    // show 判定で駆動するため、ここが唯一の外部 hide 経路（対称は main update 内の show）。
    if let Some(results) = app.get_window("results") {
        let _ = results.hide();
    }
    if let Some(state) = app.try_state::<crate::AppState>() {
        state.main_visible.store(false, Ordering::SeqCst);
    }
    // hide 後に working set を trim する（全 hide 経路の合流点＝ここが唯一の呼び出し元・#532 SU6.5）。
    // EmptyWorkingSet はスレッド非依存ゆえこの context（イベントループ / listener）から直呼び可
    // （src-tauri/CLAUDE.md「working set の能動回収」）。trim されたページは show 時に OS が透過
    // re-fault する（逆操作不要・trim が hide 前後どちらで走っても無害）。子孫 BFS は設定プロセス
    // （snotra-settings.exe・存命中のみ）も巻き込みうる——trim は best-effort ゆえ無害。
    crate::working_set::trim_idle_working_set(std::process::id());
    crate::trace_main("egui_hide:done", serde_json::json!({}));
}

/// 現在の物理位置をターゲットモニター作業領域原点からの相対座標で window.bin に保存
/// （旧 WebView2 の save_relative_placement と同じ算出・#532 SU7 で唯一の保存経路）。
pub(crate) fn save_placement_relative(window: &tauri::Window) {
    let Ok(pos) = window.outer_position() else {
        return;
    };
    #[cfg(windows)]
    {
        use snotra_core::window_data::{self, WindowPlacement};
        let Ok(hwnd) = window.hwnd() else { return };
        let Some(wa) = crate::monitor::window_monitor_work_area(hwnd.0 as isize) else {
            return;
        };
        window_data::save_search_placement(WindowPlacement {
            x: pos.x - wa.left,
            y: pos.y - wa.top,
        });
    }
    #[cfg(not(windows))]
    {
        use snotra_core::window_data::{self, WindowPlacement};
        window_data::save_search_placement(WindowPlacement { x: pos.x, y: pos.y });
    }
}

/// view からの `egui-hide-requested` を受け、hide_egui_main を実行する（全 hide の合流点・codex #7）。
/// view（イベントループスレッド）→ emit → この listener で hide を 1 経路に集約する。
pub(crate) fn register_hide_listener(app: &tauri::AppHandle) {
    let handle = app.clone();
    app.listen("egui-hide-requested", move |_| {
        hide_egui_main(&handle);
    });
}

/// 可視中の view を起こす（egui_ctx 未登録＝setup〜初フレーム、hidden 中は無害な no-op）。
/// WebView2 経路（flag OFF）では EguiShellState が manage されておらず自然に no-op。
pub(crate) fn wake_view(app: &tauri::AppHandle) {
    if let Some(sh) = app.try_state::<EguiShellState>()
        && let Ok(guard) = sh.egui_ctx.lock()
        && let Some(ctx) = guard.as_ref()
    {
        ctx.request_repaint();
    }
}

/// results 窓を起こす(#646 PR2)。snapshot 更新・config 変更を反映させる wake。呼び出しは
/// main の update()（Task 4）内 2 箇所: snapshot 差分検知時（edge-triggered・変化フレームのみ）
/// と `drive_results_window`（可視時・毎フレーム・level-triggered）。hidden 中の results は
/// 描かれないため事前 wake は無意味(plan-review で冗長と判定)。クリック逆流の results→main は
/// 既存 `wake_view` を使う。
pub(crate) fn wake_results(app: &tauri::AppHandle) {
    if let Some(sh) = app.try_state::<EguiShellState>()
        && let Ok(guard) = sh.results_ctx.lock()
        && let Some(ctx) = guard.as_ref()
    {
        ctx.request_repaint();
    }
}

/// results を main の直下 + window_gap に配置する(#646 PR2 決定 6)。呼び出し元は
/// 2 つ——main の update()(通常の毎フレーム従属)と main の Moved リスナー
/// (ネイティブ移動ループ中の追従。ループ中は egui フレームが回らない可能性があるため
/// イベント駆動で直接動かす)。デルタガードは持たない(set_position は同値でも安価・
/// ガードは update 側の責務)。
pub(crate) fn position_results_below_main(app: &tauri::AppHandle) {
    let (Some(main), Some(results)) = (app.get_window("main"), app.get_window("results")) else {
        return;
    };
    let gap = app
        .try_state::<crate::AppState>()
        .map(|s| s.engine.lock().unwrap().config().visual.window_gap)
        .unwrap_or(4) as f64;
    if let (Ok(pos), Ok(size), Ok(scale)) =
        (main.outer_position(), main.outer_size(), main.scale_factor())
    {
        let _ = results.set_position(tauri::PhysicalPosition::new(
            pos.x,
            pos.y + size.height as i32 + (gap * scale).round() as i32,
        ));
    }
}

/// config 変更・index 状態変化の wake 合図（#532 SU6 spec 決定 1）。値は運ばず request_repaint
/// のみ——次フレームの live-read が最新値を拾う。空振りは benign（初 show フレームの live-read が
/// 最新を描く）。**「値を運ばない」はこの benign 性の load-bearing 前提**——将来イベントに値を
/// 載せる変更はこの前提を壊す（spec 決定 1）。
pub(crate) fn register_config_wake_listeners(app: &tauri::AppHandle) {
    for event in ["config-applied", "indexing-started", "indexing-complete"] {
        let handle = app.clone();
        app.listen(event, move |_| {
            wake_view(&handle);
        });
    }
}

/// hotkey 登録失敗の payload 受け口（spec 追補 2・wake は config-applied に委ねる）。
pub(crate) fn register_hotkey_failure_listener(app: &tauri::AppHandle) {
    let handle = app.clone();
    app.listen("hotkey-registration-failed", move |event| {
        // emit 側は String を渡すため payload は JSON 文字列（引用符付き）。
        let hotkey: String = serde_json::from_str(event.payload()).unwrap_or_default();
        if let Some(sh) = handle.try_state::<EguiShellState>() {
            *sh.pending_hotkey_failure.lock().unwrap() = Some((HotkeyFailureKind::Change, hotkey));
        }
    });
}

/// 起動時 hotkey 登録失敗の受け口（#652・SU6.5 決定 2）。`platform-event` の
/// `initial-hotkey-failed` を判別し、**格納 → show → wake** の順で処理する。
///
/// - **格納が先**: show が起こすフレームは reset-on-show の `notice.clear()` を通ってから
///   pending を消費する（view の順序不変条件）。逆順にすると clear と store の間にフレームが
///   挟まりうるため、通知が消えたまま二度と出ない。
/// - **show する**: ホットキーが登録できていない＝ユーザーが窓を開く手段がトレイしか無い。
///   SPEC §10「初回ホットキー登録失敗時は操作不能回避のため検索 UI を表示し」の実装で、
///   WebView2 では `MainApp.tsx` の `win.show()` が担っている経路の egui 版。
/// - **wake する**: `show_on_startup=true` で既に可視なら `show()` は再描画を生まない。
///   `register_hotkey_failure_listener`（変更失敗）が wake しないのと**意図的に逆**——
///   あちらは必ず `config-applied` が随伴するが、起動時失敗には config 変更が無く
///   `config-applied` は来ない。ここで起こさないと「hidden 中は update() が走らない」
///   不変条件（SU5）により通知が永遠に描かれない。
pub(crate) fn register_platform_event_listener(app: &tauri::AppHandle) {
    let handle = app.clone();
    app.listen("platform-event", move |event| {
        let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) else {
            return;
        };
        if payload.get("event").and_then(|v| v.as_str()) != Some("initial-hotkey-failed") {
            return;
        }
        let hotkey = payload
            .get("hotkey")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        if let Some(sh) = handle.try_state::<EguiShellState>() {
            *sh.pending_hotkey_failure.lock().unwrap() = Some((HotkeyFailureKind::Initial, hotkey));
        }
        show_egui_main(&handle, Instant::now());
        wake_view(&handle);
    });
}
