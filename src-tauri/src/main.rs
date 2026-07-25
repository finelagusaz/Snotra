//! アプリケーションのエントリポイント。Tauri のセットアップ・イベントリスナー登録・
//! トレイ/ホットキー起動を行う。
//!
//! 起動時の背景再スキャン（`indexer::load_or_scan_with_stats` が返す `BackgroundRescanTask`）を
//! setup フェーズで低優先度スレッドに spawn し、`RescanOutcome::Changed` なら
//! `icon::invalidate_icon_cache` を呼ぶ。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod config_watcher;
mod egui_shell;
mod icon;
mod ime;
mod indexing;
mod monitor;
mod platform;
mod state;
mod trace;
mod working_set;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use serde_json::json;
use snotra_core::config::{Config, HotkeyConfig, Language, LoadOutcome};
use snotra_core::engine::Engine;
use snotra_core::history::HistoryStore;
use snotra_core::indexer;
use tauri::{AppHandle, Listener, Manager};

use crate::commands::SettingsProcessState;
use crate::icon::IconCacheState;

use crate::platform::{PlatformBridge, PlatformBridgePending, PlatformCommand};
use crate::state::AppState;

const ALT_RELEASE_POLL_MS: u64 = 10;
const ALT_RELEASE_TIMEOUT_MS: u64 = 350;

/// Thin wrapper kept so call sites read `trace_main(...)`; logic lives in the
/// shared `crate::trace` module (deduped with `commands::trace_command`, #433).
fn trace_main(event: &str, data: serde_json::Value) {
    trace::trace(event, data);
}

#[cfg(windows)]
fn is_alt_pressed() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_LMENU, VK_MENU, VK_RMENU,
    };
    unsafe {
        GetAsyncKeyState(VK_MENU.0 as i32) < 0
            || GetAsyncKeyState(VK_LMENU.0 as i32) < 0
            || GetAsyncKeyState(VK_RMENU.0 as i32) < 0
    }
}

#[cfg(not(windows))]
fn is_alt_pressed() -> bool {
    false
}

fn wait_alt_release_or_timeout() {
    use std::time::{Duration, Instant};

    if !is_alt_pressed() {
        return;
    }

    let started = Instant::now();
    let timeout = Duration::from_millis(ALT_RELEASE_TIMEOUT_MS);
    let poll = Duration::from_millis(ALT_RELEASE_POLL_MS);

    while started.elapsed() < timeout {
        if !is_alt_pressed() {
            return;
        }
        std::thread::sleep(poll);
    }
}

/// Clear lingering Alt modifier state via `SendInput` before showing the
/// search window.  Uses the AutoHotkey "MenuMaskKey" technique: a dummy
/// key-down/up (vkE8, unassigned) is injected *before* the Alt key-up so
/// that Windows does not treat the Alt release as a bare Alt-up, which
/// would activate the menu bar or trigger a system beep.
#[cfg(windows)]
fn send_alt_key_up() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, SendInput, VK_LMENU, VK_MENU, VK_RMENU, VIRTUAL_KEY,
    };

    const VK_MASK: VIRTUAL_KEY = VIRTUAL_KEY(0xE8); // unassigned — safe dummy key

    let inputs = [
        make_key_input(VK_MASK, false),  // mask key down
        make_key_input(VK_MASK, true),   // mask key up
        make_key_input(VK_MENU, true),   // Alt (generic) up
        make_key_input(VK_LMENU, true),  // Left Alt up
        make_key_input(VK_RMENU, true),  // Right Alt up
    ];
    unsafe {
        let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
    // Brief pause so the focused window processes the synthetic key-ups
    // before receiving actual user keystrokes.
    std::thread::sleep(std::time::Duration::from_millis(5));
}

#[cfg(windows)]
fn make_key_input(
    vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY,
    is_up: bool,
) -> windows::Win32::UI::Input::KeyboardAndMouse::INPUT {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYBD_EVENT_FLAGS,
    };
    let mut input = INPUT {
        r#type: INPUT_KEYBOARD,
        ..Default::default()
    };
    input.Anonymous.ki = KEYBDINPUT {
        wVk: vk,
        dwFlags: if is_up {
            KEYEVENTF_KEYUP
        } else {
            KEYBD_EVENT_FLAGS::default()
        },
        ..Default::default()
    };
    input
}

#[cfg(not(windows))]
fn send_alt_key_up() {}

/// Position the main window on the target monitor using saved relative coordinates.
///
/// Target monitor is determined by `follow_cursor_monitor` config:
/// - true: monitor containing the mouse cursor
/// - false: primary monitor
///
/// Saved relative coordinates (physical pixels from monitor work area origin)
/// are applied and clamped to the target work area. If no saved position exists,
/// the window is centered on the target monitor.
#[cfg(windows)]
fn position_on_target_monitor(
    app_handle: &AppHandle,
    // &Window に一般化して egui 経路と共有（#532 SU2）。両経路とも同一の "main" 窓
    // （get_window/get_webview_window は同じ内部 Window を指す・manager/window.rs:106）。
    main: &tauri::Window,
) {
    use snotra_core::window_data;

    // Read follow_cursor_monitor from Engine config (refreshed on every show).
    let follow_cursor = app_handle
        .try_state::<AppState>()
        .map(|s| s.engine.lock().unwrap().config().general.follow_cursor_monitor)
        .unwrap_or(true);

    // Determine target monitor work area.
    let target_wa = if follow_cursor {
        monitor::cursor_monitor_work_area()
    } else {
        monitor::primary_monitor_work_area()
    };
    let Some(target_wa) = target_wa else { return };

    // Get current window size (physical) for centering/clamping.
    let Ok(win_size) = main.outer_size() else { return };
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

fn main() {
    let is_first_run = Config::is_first_run();
    let (config, load_outcome) = Config::load_reporting();

    let (entries, initial_indexing, cached_masks, rescan_task) = if is_first_run {
        (Vec::new(), true, None, None)
    } else {
        let result =
            indexer::load_or_scan_with_stats(&config.paths.scan, config.search.show_hidden_system);
        #[cfg(debug_assertions)]
        {
            let s = &result.stats;
            eprintln!(
                "[index-load] cache_hit={} total={}ms hash={}ms cache_load={}ms scan={}ms sort={}ms cache_save={}ms",
                s.cache_hit,
                s.total_ms,
                s.hash_ms,
                s.cache_load_ms,
                s.scan_ms,
                s.sort_ms,
                s.cache_save_ms,
            );
        }
        (result.entries, false, result.cached_masks, result.rescan_task)
    };

    // PATH エントリのスキャン + マージ
    let (mut entries, mut cached_masks) = (entries, cached_masks);
    if config.search.include_path_env {
        let path_entries = indexer::scan_path_env(&entries, config.search.show_hidden_system);
        if !path_entries.is_empty() {
            if let Some(ref mut masks) = cached_masks {
                indexer::extend_cached_masks(masks, &path_entries);
            }
            entries.extend(path_entries);
        }
    }

    // Lazy-load icon cache on first icon request to keep startup path short.
    let icon_cache_state: IconCacheState = Mutex::new(None);

    let history = HistoryStore::load();

    let show_on_startup = config.general.show_on_startup;
    let show_tray = config.general.show_tray_icon;
    let hotkey_config = config.hotkey.clone();
    let initial_language = config.general.language;
    let window_width = config.appearance.window_width;
    let bg_color = config.visual.background_color.clone();

    let engine = if let Some(masks) = cached_masks {
        Engine::new_from_cache(entries, masks, history, config)
    } else {
        Engine::new(entries, history, config)
    };

    let app_state = AppState {
        engine: Mutex::new(engine),
        indexing: AtomicBool::new(initial_indexing),
        index_build_started: AtomicBool::new(false),
        main_visible: AtomicBool::new(false),
        index_generation: AtomicU64::new(0),
    };

    // 宣言窓なし（tauri.conf.json の windows は空・#532 SU7 flip）。メイン窓は
    // setup フェーズで egui_shell::create が生成する。
    let app_context = tauri::generate_context!();

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(move |app, _args, _cwd| {
            // When a second instance tries to start, show the main (egui) window.
            egui_shell::show_egui_main(app, Instant::now());
        }))
        .manage(app_state)
        .manage(icon_cache_state)
        .manage(SettingsProcessState::default())
        // invoke_handler は無い（#532 SU7 PR3・フロント撤去で IPC は消滅。egui は
        // commands/ の _core 関数・engine を直呼びする）
        .setup(move |app| {
            let app_handle = app.handle().clone();

            // Spawn platform thread early to parallelize Win32 init with window creation.
            // Tray is NOT created here; SetTrayVisible is sent after full setup (SPEC §7.5).
            // Must precede setup_hotkey_listener below, which sends RegisterInitialHotkey
            // through the platform bridge managed here.
            setup_platform_thread(&app_handle, hotkey_config, initial_language);

            // 窓生成（egui・platform thread spawn 後・SPEC §8.5 で Win32 初期化と並列化）。
            // 幅の復元は create が window_width で行う（#532 SU7 flip で唯一の経路）。
            // show/hide を跨ぐ共有状態（世代・emit dedup）。view/hotkey/hide が参照するので窓生成前に管理下へ。
            app.manage(egui_shell::EguiShellState::default());
            let results_window = egui_shell::create(app, window_width as f64, &bg_color)?;
            // #671 PR A′: results 窓の所有型を managed state へ（spec 決定 8「A′ の中間形」）。
            // **listener 登録より前**に置く——hide_egui_main が try_state で引くため、hide が
            // 起こりうる時点より前に manage されている必要がある。`EguiShellState` の manage
            // 位置は動かさない（create() の後へ移すのは PR D の担当。register_ctx の撤去と
            // セットでなければ Option スロットが残る）。
            app.manage(results_window);
            // view→emit→listener の合流点。**main の** hide を hide_egui_main の 1 経路に集約（codex #7）。
            egui_shell::register_hide_listener(&app_handle);
            // config 変更・indexing 状態変化の wake（SU6 spec 決定 1）。config_watcher 起動
            // （下の setup_config_watcher）と setup_startup_display より前に登録し、可視窓が
            // 合図を取りこぼす窓を作らない（位置は spec が pin・並行性レビュー）。
            egui_shell::register_config_wake_listeners(&app_handle);
            // hotkey 登録失敗の pending 格納（spec 追補 2）。wake しない listener——
            // wake は config-applied（言語変更同時発生時の競合窓を閉じる）に委ねる。
            egui_shell::register_hotkey_failure_listener(&app_handle);
            // 起動時 hotkey 登録失敗の受け口（#652）。RegisterInitialHotkey を送る
            // setup_hotkey_listener より前に登録される位置なので emit を取りこぼさない
            //（この egui ブロック自体が setup_platform_thread の直後・hotkey listener の前）。
            egui_shell::register_platform_event_listener(&app_handle);
            app.manage(egui_shell::UpdaterUiState(std::sync::Mutex::new(Default::default())));
            // main と results 窓が共有する一方向フローの入れ物（#646 PR2）。
            app.manage(egui_shell::ResultsShared::default());
            egui_shell::spawn_update_check(&app_handle);

            // First-run: launch snotra-settings directly (bypassing the indexing guard
            // in open_settings, since initial_indexing=true during first run).
            setup_first_run(&app_handle, is_first_run);

            // Listen for hotkey-pressed events, then activate the hotkey on the
            // platform thread. Registering the listener before activating the
            // hotkey ensures no event is emitted before there is a receiver to
            // handle it — this order must not change (src-tauri/CLAUDE.md).
            setup_hotkey_listener(&app_handle);

            // Listen for open-settings event from tray
            setup_open_settings_listener(&app_handle);

            // Listen for exit request from tray
            setup_exit_listener(&app_handle);

            // Start config.toml file watcher for external changes (snotra-settings)
            setup_config_watcher(&app_handle);

            // 背景再スキャン（SPEC §3.3 ハイブリッド方式）。キャッシュヒット時のみ。
            // ロジックは snotra-core、spawn と結果の後始末（アイコン無効化）は src-tauri。
            setup_background_rescan(&app_handle, rescan_task);

            // All windows pre-created and all listeners registered; now safe to show tray.
            // Showing tray before this point would allow right-click menu actions before
            // the windows and listeners are ready (SPEC §7.5 / §9). Must run after every
            // setup_*_listener call above.
            setup_tray(&app_handle, show_tray, load_outcome);

            // Show window on startup if configured. Must run last: relies on the
            // platform bridge (IME control) and listeners registered above.
            setup_startup_display(&app_handle, show_on_startup);

            Ok(())
        })
        .run(app_context)
        .expect("error while running tauri application");
}

/// Spawn the Win32 platform thread early to parallelize its init with WebView
/// creation, and manage the resulting `PlatformBridge` once ready. The tray
/// is NOT created here; `setup_tray` sends `SetTrayVisible` later, after all
/// windows and listeners are ready (SPEC §7.5).
fn setup_platform_thread(app_handle: &AppHandle, hotkey_config: HotkeyConfig, initial_language: Language) {
    let platform_pending = PlatformBridge::begin(app_handle.clone(), hotkey_config, initial_language);

    // Win32 init finishes in a few ms; by the time windows are created it is already done.
    if let Some(bridge) = platform_pending.and_then(PlatformBridgePending::wait) {
        app_handle.manage(Mutex::new(bridge));
    }
}

/// First-run: launch snotra-settings directly (bypassing the indexing guard
/// in open_settings, since initial_indexing=true during first run).
/// Pass --first-run so SettingsApp opens on the Index tab for onboarding.
/// On failure (exe not found / spawn error), fall back to building the index
/// with default paths so the indexing flag eventually clears and the user
/// can open settings via open_settings once the build finishes.
fn setup_first_run(app_handle: &AppHandle, is_first_run: bool) {
    if is_first_run && commands::launch_settings_process(app_handle, &["--first-run"]).is_err() {
        indexing::start_index_build(app_handle);
    }
}

/// Register the `hotkey-pressed` listener, then activate the hotkey on the
/// platform thread. **Order must not change**: registering the listener
/// before sending `RegisterInitialHotkey` ensures no event is emitted before
/// there is a receiver to handle it (src-tauri/CLAUDE.md).
fn setup_hotkey_listener(app_handle: &AppHandle) {
    let handle_for_hotkey = app_handle.clone();
    app_handle.listen("hotkey-pressed", move |_| {
        let t0 = Instant::now();
        trace_main("hotkey:listener_enter", json!({}));
        // Ignore the hotkey while snotra-settings is running: the user may be
        // pressing the current hotkey combination to configure a new one.
        if let Some(proc_state) = handle_for_hotkey.try_state::<SettingsProcessState>()
            && proc_state.lock().unwrap().is_some()
        {
            return;
        }
        // 共有 EguiShellState.hotkey_generation を使い（hide が bump して
        // 保留 show を無効化・codex #5/(B)#2）、純粋核 plan_hotkey で分岐する。
        let current_gen = handle_for_hotkey
            .try_state::<egui_shell::EguiShellState>()
            .map(|sh| sh.hotkey_generation.fetch_add(1, Ordering::SeqCst) + 1)
            .unwrap_or(0);
        let app_state = handle_for_hotkey.try_state::<AppState>();
        let visible = app_state
            .as_ref()
            .map(|s| s.main_visible.load(Ordering::SeqCst))
            .unwrap_or(false);
        // hotkey_toggle は可視時の hide 判定にしか使わない（plan_hotkey）。`visible &&` で
        // 短絡し、非表示＝show 経路（最も遅延に敏感）では engine ロックを取らない。
        // 表示中でも hotkey_toggle=false なら hide せず show 側（再フォーカス/再配置）へ回る。
        let hotkey_toggle = visible
            && app_state
                .as_ref()
                .map(|s| s.engine.lock().unwrap().config().general.hotkey_toggle)
                .unwrap_or(true); // config.rs 既定と一致
        match egui_shell::plan_hotkey(visible, is_alt_pressed(), hotkey_toggle) {
            egui_shell::HotkeyPlan::HideNow => {
                egui_shell::hide_egui_main(&handle_for_hotkey);
            }
            egui_shell::HotkeyPlan::ShowNow => {
                egui_shell::show_egui_main(&handle_for_hotkey, t0);
            }
            egui_shell::HotkeyPlan::ShowAfterAltRelease => {
                let h = handle_for_hotkey.clone();
                std::thread::spawn(move || {
                    wait_alt_release_or_timeout();
                    // 共有世代が変わっていたら（別 press や hide が bump）show を諦める。
                    let gen_now = h
                        .try_state::<egui_shell::EguiShellState>()
                        .map(|sh| sh.hotkey_generation.load(Ordering::SeqCst))
                        .unwrap_or(0);
                    if gen_now != current_gen {
                        return;
                    }
                    egui_shell::show_egui_main(&h, Instant::now());
                });
            }
        }
    });

    // hotkey-pressed listener is now registered; activate hotkey on platform thread.
    // Registering the hotkey only after the listener is ready ensures no event
    // is emitted before there is a receiver to handle it.
    if let Some(bridge) = app_handle.try_state::<Mutex<PlatformBridge>>()
        && let Ok(b) = bridge.lock()
    {
        b.send_command(PlatformCommand::RegisterInitialHotkey);
    }
}

/// Listen for the `open-settings` event emitted by the tray menu.
fn setup_open_settings_listener(app_handle: &AppHandle) {
    let handle_for_settings = app_handle.clone();
    app_handle.listen("open-settings", move |_| {
        let _ = commands::open_settings(
            handle_for_settings.state::<AppState>(),
            handle_for_settings.clone(),
        );
    });
}

/// 終了時と updater install 前（`on_before_exit`）が共有する保存専用ルーチン（#532 SU5）。
/// exit-requested の flush 列は保存 + exit(0) の不可分列だったため、保存だけを再利用可能に
/// 切り出した（spec「決着済み: 保存順序」）。二重 flush（install 前 + 通常終了）は
/// `NEXT_SAVE_SEQUENCE` の単調ガードで安全（最新 seq 勝ち・並行性レビュー実測）。
pub(crate) fn flush_persistent_state(app_handle: &AppHandle) {
    // Capture a consistent final snapshot under the Engine lock, then flush
    // it without holding the lock through filesystem I/O.
    let history_save = {
        let app_state = app_handle.state::<AppState>();
        let mut engine = app_state.engine.lock().unwrap();
        engine.prepare_history_flush()
    };
    if let Some(save) = history_save {
        let _ = save.save();
    }
    {
        let icon_state = app_handle.state::<IconCacheState>();
        let mut cache = icon_state.lock().unwrap();
        if let Some(c) = cache.as_mut() {
            c.save_if_dirty();
        }
    }
}

/// Listen for the `exit-requested` event emitted by the tray menu: flush
/// unsaved data, kill the snotra-settings child process if running, and exit.
fn setup_exit_listener(app_handle: &AppHandle) {
    let handle_for_exit = app_handle.clone();
    app_handle.listen("exit-requested", move |_| {
        // Flush any unsaved data before exit
        flush_persistent_state(&handle_for_exit);
        // Kill snotra-settings child process if running.
        if let Some(proc_state) = handle_for_exit.try_state::<SettingsProcessState>()
            && let Ok(mut guard) = proc_state.lock()
            && let Some(mut child) = guard.take()
        {
            let _ = child.kill();
        }
        if let Some(bridge) = handle_for_exit.try_state::<Mutex<PlatformBridge>>()
            && let Ok(b) = bridge.lock()
        {
            b.send_command(PlatformCommand::Exit);
        }
        handle_for_exit.exit(0);
    });
}

/// Start the `config.toml` file watcher for external changes (snotra-settings).
fn setup_config_watcher(app_handle: &AppHandle) {
    if let Some(watcher) = config_watcher::start(app_handle) {
        app_handle.manage(Mutex::new(watcher));
    }
}

/// キャッシュヒット時のみ `Some` で渡される背景再スキャンタスクを低優先度スレッド
/// で実行する（SPEC §3.3 ハイブリッド方式）。ロジックは snotra-core、spawn と結果の
/// 後始末（アイコン無効化）は src-tauri の責務。
fn setup_background_rescan(app_handle: &AppHandle, rescan_task: Option<indexer::BackgroundRescanTask>) {
    if let Some(task) = rescan_task {
        let handle_for_rescan = app_handle.clone();
        let _ = std::thread::Builder::new()
            .name("snotra-index-rescan".to_string())
            .spawn(move || {
                indexer::lower_current_thread_priority();
                if task.run() == indexer::RescanOutcome::Changed
                    && let Some(icons) = handle_for_rescan.try_state::<IconCacheState>()
                {
                    icon::invalidate_icon_cache(&icons);
                }
            });
    }
}

/// Show the tray icon now that all windows are pre-created and all event
/// listeners are registered. Showing tray before this point would allow
/// right-click menu actions before the windows/listeners are ready (SPEC
/// §7.5 / §9).
fn setup_tray(app_handle: &AppHandle, show_tray: bool, load_outcome: LoadOutcome) {
    if show_tray
        && let Some(bridge) = app_handle.try_state::<Mutex<PlatformBridge>>()
        && let Ok(b) = bridge.lock()
    {
        b.send_command(PlatformCommand::SetTrayVisible(true));
        // config が壊れて既定値に復旧した場合、トレイ生成直後に通知する。
        // 復旧時は config=default=show_tray ON のためこの分岐に入る。
        // SetTrayVisible→ShowConfigRecoveryBalloon を同一チャネルに順に積むので、
        // process_commands はトレイ生成後にバルーンを処理する。
        if load_outcome == LoadOutcome::RecoveredFromCorrupt {
            b.send_command(PlatformCommand::ShowConfigRecoveryBalloon);
        }
    }
}

/// Show the main window on startup if configured. Must run after tray/listener
/// setup: `show_egui_main` depends on the platform bridge (IME control).
fn setup_startup_display(app_handle: &AppHandle, show_on_startup: bool) {
    if show_on_startup {
        egui_shell::show_egui_main(app_handle, Instant::now());
    }
}
