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
mod events;
mod icon;
mod ime;
mod indexing;
mod monitor;
mod platform;
mod startup;
mod state;
mod trace;
mod working_set;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use serde_json::json;
use snotra_core::config::{Config, GeneralConfig, HotkeyConfig, Language, LoadOutcome};
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
        INPUT, SendInput, VIRTUAL_KEY, VK_LMENU, VK_MENU, VK_RMENU,
    };

    const VK_MASK: VIRTUAL_KEY = VIRTUAL_KEY(0xE8); // unassigned — safe dummy key

    let inputs = [
        make_key_input(VK_MASK, false), // mask key down
        make_key_input(VK_MASK, true),  // mask key up
        make_key_input(VK_MENU, true),  // Alt (generic) up
        make_key_input(VK_LMENU, true), // Left Alt up
        make_key_input(VK_RMENU, true), // Right Alt up
    ];
    unsafe {
        let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
    // Brief pause so the focused window processes the synthetic key-ups
    // before receiving actual user keystrokes.
    //
    // **この根拠は #880 サイクル段 2 で失効した（受容・未実測）。** 呼び出し元
    // （`egui_shell::show_egui_main`）がイベントループスレッドへ移った結果、ここで対象と
    // している focused window は**自スレッドが所有する main 窓**になった。スリープ中この
    // スレッドはポンプを回さないので、**フォアグラウンドが既に main 窓へ移っていれば、
    // 待っているあいだにキー up が処理されることはもう無い**——その前提の下では
    // スリープの目的は自スレッド上では達成できず、純粋なコストになる
    // （`show_egui_main` の `SendMessageTimeoutW` 撤去と**同じ型の失効**。あちらの撤去跡の
    // コメントを参照）。**前提を落として「常に純粋なコスト」とは書けない**——直前の
    // `set_focus()` はフォアグラウンド移行を同期しないため、まだ旧窓が前景なら合成キー up は
    // そちらのスレッドで処理されうる（それこそがバリア撤去跡が「未実測」と断っている当の点で
    // ある）。**hotkey に Alt を含む設定では実質すべての show がここを通る**
    // （`ShowAfterAltRelease` の再入時に `!is_alt_pressed()` が真になるため）。
    //
    // **撤去は次段の判断に残す。** 見込みとしては「別スレッドへ出す」ではなく**削除**が正しい
    // ——目的は「窓がキー up を処理できるようにする」ことであり、それを可能にするのは
    // 待つことではなく**早く返してポンプへ戻ること**だからである（**ただし上の前提が要る**
    // ——旧窓が前景に残る場合まで含めて削除してよいかは、測ってからでないと言えない）。加えて注入順は
    // `SendInput` がシステム入力キューへ積んだ時点で決まり、後から打たれた実キーはその後ろに
    // 並ぶ。ただし**この経路は生 Win32 で `cargo test` の視界の外にあり、実機で測っていない**
    // ——削除はカテゴリ C/D の検出器を回せる段で行うこと。
    std::thread::sleep(std::time::Duration::from_millis(5));
}

#[cfg(windows)]
fn make_key_input(
    vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY,
    is_up: bool,
) -> windows::Win32::UI::Input::KeyboardAndMouse::INPUT {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP,
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

fn main() {
    // **anchor はここである。** これより前（DLL ロード・CRT 初期化）は `pre_main` として
    // 別に測る——測らないと、そこに住む遅延が計測区間の外へ落ちる（`crate::startup` の `//!`）。
    startup::begin();

    let is_first_run = Config::is_first_run();
    let (config, load_outcome) = Config::load_reporting();
    startup::mark(startup::Phase::ConfigLoad);

    let (mut material, initial_indexing, rescan_task) = if is_first_run {
        (
            indexer::IndexMaterial::from_tree(snotra_core::index_tree::IndexTree::empty()),
            true,
            None,
        )
    } else {
        let result =
            indexer::load_or_scan_with_stats(&config.paths.scan, config.search.show_hidden_system);
        #[cfg(debug_assertions)]
        {
            let s = &result.stats;
            eprintln!(
                "[index-load] cache_hit={} total={}ms hash={}ms cache_load={}ms digest={}ms scan={}ms sort={}ms cache_save={}ms",
                s.cache_hit,
                s.total_ms,
                s.hash_ms,
                s.cache_load_ms,
                s.digest_ms,
                s.scan_ms,
                s.sort_ms,
                s.cache_save_ms,
            );
        }
        // 内側の内訳との差が `index_load_unattributed_ms` になる——`load_or_scan_with_stats`
        // の中にある未命名の処理を、外側の計器が捕まえるための項目である。
        startup::set_index_load_stats_ms(result.stats.total_ms as u64);
        // **枝は `LoadOrScanStats` から取る。** `initial_indexing` は first-run でしか
        // 立たないので、そこから導くと非 first-run の cache-miss が cache_hit=true に化ける。
        startup::set_branch(startup::Branch {
            first_run: false,
            cache_hit: result.stats.cache_hit,
            include_path_env: config.search.include_path_env,
        });
        startup::mark(startup::Phase::IndexLoad);
        (result.material, false, result.rescan_task)
    };

    // PATH エントリのスキャン + マージ。**木とマスクは組のまま持つ**ので、片方だけ伸ばす形はここでは書けない（正本は `IndexMaterial` の doc）。**空の場合のガードは持たない**——追記も木への追加も、空なら何もしない。
    if config.search.include_path_env {
        let path_entries =
            indexer::scan_path_env(material.tree(), config.search.show_hidden_system);
        material.extend_with_path_entries(path_entries);
        // **既定（`include_path_env = false`）ではこのマークを通らない**——出力は `null` に
        // なる。0 と書いてはならない（「実行して 1 ms 未満」と区別できなくなる）。
        startup::mark(startup::Phase::PathMerge);
    }
    if is_first_run {
        // first-run は索引ロードを通らない（`index_load` は `null` になる）。
        startup::set_branch(startup::Branch {
            first_run: true,
            cache_hit: false,
            include_path_env: config.search.include_path_env,
        });
    }

    // Lazy-load icon cache on first icon request to keep startup path short.
    let icon_cache_state: IconCacheState = Mutex::new(None);

    let history = HistoryStore::load();
    startup::mark(startup::Phase::HistoryLoad);

    let show_on_startup = config.general.show_on_startup;
    let show_tray = config.general.show_tray_icon;
    let hotkey_config = config.hotkey.clone();
    let initial_language = config.general.language;
    let window_width = config.appearance.window_width;
    let bg_color = config.visual.background_color.clone();

    let engine = Engine::from_material(material, history, config);
    startup::mark(startup::Phase::EngineBuild);

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
        .plugin(tauri_plugin_single_instance::init(
            move |app, _args, _cwd| {
                // When a second instance tries to start, show the main (egui) window.
                // **証人を作れるのは `on_event_loop` の中だけ**ゆえ marshalling する
                // （このコールバックがどのスレッドから来るかに依らず正しい形になる）。
                snotra_egui_runtime::on_event_loop(app, |app, el| {
                    egui_shell::show_egui_main(app, el, Instant::now());
                });
            },
        ))
        .manage(app_state)
        .manage(icon_cache_state)
        .manage(SettingsProcessState::default())
        // invoke_handler は無い（#532 SU7 PR3・フロント撤去で IPC は消滅。egui は
        // commands/ の _core 関数・engine を直呼びする）
        .setup(move |app| {
            // **setup はイベントループより前ではない**（同じ 1 イベントの処理中）。ゆえに
            // ここまでが「tauri の初期化」であり、以降が窓とリスナーである。
            startup::mark(startup::Phase::TauriInit);
            let app_handle = app.handle().clone();

            // Spawn platform thread early to parallelize Win32 init with window creation.
            // Tray is NOT created here; SetTrayVisible is sent after full setup (SPEC §7.5).
            // Must precede setup_hotkey_listener below, which sends RegisterInitialHotkey
            // through the platform bridge managed here.
            setup_platform_thread(&app_handle, hotkey_config, initial_language);

            // 窓生成（egui・platform thread spawn 後・SPEC §8.5 で Win32 初期化と並列化）。
            // 幅の復元は create が window_width で行う（#532 SU7 flip で唯一の経路）。
            // **setup ブロック唯一の早期 return である。** ここで抜けると
            // `RegisterInitialHotkey` は送られないので、終端を出さないとハーネスには
            // 「タイムアウト」としか見えない（`crate::startup` の `//!`）。
            let handles = match egui_shell::create(app, window_width as f64, &bg_color) {
                Ok(h) => h,
                Err(e) => {
                    startup::finish(Err(startup::StartupFailure::WindowCreation));
                    return Err(Box::new(e));
                }
            };
            // **フォント解決を含む区間である**（`font_stack.rs`）。窓を一度も出していない
            // 時点で常駐に効くことが実測されており、表示より前に走る。
            startup::mark(startup::Phase::WindowsCreate);
            // #671 PR D（spec 決定 8 の終端形）: show/hide を跨ぐ共有状態（世代・emit dedup）と
            // 両窓の wake handle。**`create()` の後**に manage する——handle は attach の戻り値
            // ゆえ窓の生成より前には存在しない。各 view の `setup()` はもう `EguiShellState` を
            // 読まないので（PR D で `register_ctx` を撤去した）、この順序で問題が無い。
            //
            // この順序が安全である根拠は 2 つある（`EguiShellState` の読み手はすべて
            // `if let Some(..)` ゆえ、manage 前にフレームが走ると消費が**沈黙して skip される**）:
            //
            // 1. **この setup ブロック自体がイベントループの 1 イテレーション内で走る。**
            //    tauri は setup フックを `RuntimeRunEvent::Ready` の処理中に呼ぶ
            //    （tauri 2.11.4 `src/app.rs` の `make_run_event_loop_callback`）。この間
            //    メッセージポンプは停止しており（「ウィンドウ生成の制約」と同じ機構）、
            //    wry plugin の `on_event`＝`attach_pending_windows` は setup の復帰後にしか
            //    走らない。**「setup はイベントループより前」ではない**——同じ 1 イベントの
            //    処理中である。
            // 2. **仮にフレームが走っても、この時点で pending なものは無い。**
            //    reset_pending は `show_egui_main` が、pending_hotkey_failure は下の 2 つの
            //    listener が、hotkey_generation は hide と hotkey listener が立てる——
            //    setter はすべてこの manage より後にしか動かない。
            app.manage(egui_shell::EguiShellState::new(&handles));
            // #671 PR A′: results 窓の所有型を managed state へ。**listener 登録より前**に置く
            // ——hide_egui_main が try_state で引くため、hide が起こりうる時点より前に manage
            // されている必要がある。
            app.manage(handles.results_window);
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
            egui_shell::register_initial_hotkey_failure_listener(&app_handle);
            app.manage(egui_shell::UpdaterUiState(std::sync::Mutex::new(
                Default::default(),
            )));
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
fn setup_platform_thread(
    app_handle: &AppHandle,
    hotkey_config: HotkeyConfig,
    initial_language: Language,
) {
    // Win32 init finishes in a few ms; by the time windows are created it is already done.
    match PlatformBridge::begin(app_handle.clone(), hotkey_config, initial_language)
        .and_then(PlatformBridgePending::wait)
    {
        Ok(bridge) => {
            app_handle.manage(Mutex::new(bridge));
        }
        // **起動はここで続行するが、終端は出す。** 出さないと `RegisterInitialHotkey` の
        // arm が走らないまま起動が終わり、ハーネスには「タイムアウト」としか見えない
        // （正本は `crate::startup` の `//!`）。
        Err(e) => startup::finish(Err(startup::StartupFailure::from(e))),
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
    app_handle.listen(crate::events::HOTKEY_PRESSED, move |_| {
        let t0 = Instant::now();
        trace_main("hotkey:listener_enter", json!({}));
        // 設定画面の起動中はホットキーを無視する（ユーザーが新しい組み合わせを設定するために
        // 現在の組み合わせを押している可能性がある）。**窓に触らない読みなのでタスクの外に置く**
        // ——無駄なタスク post を避ける。
        if let Some(proc_state) = handle_for_hotkey.try_state::<SettingsProcessState>()
            && proc_state.lock().unwrap().is_some()
        {
            return;
        }
        // **判定ごとイベントループへ移す。** 世代の採番・可視の読み・分岐・副作用が
        // ひとまとまりで逐次化される——効果だけを移すと連打で stale を読む（各アームを
        // 個別に包むと、判定がこの producer スレッドに残り、タスク実行前に届いた 2 回目の
        // 押下が**同じ stale 値**を読んで両方 Hide / 両方 Show になる）。今日この問題が
        // 無いのは判定も副作用も同じ platform スレッド上で逐次化されているからで、
        // **効果だけを marshalling するとその逐次化が失われる**。
        //
        // `t0` は**post する前**に取ってある（marshalling の hop をレイテンシ計測に含める）。
        snotra_egui_runtime::on_event_loop(&handle_for_hotkey, move |app, el| {
            // 共有 EguiShellState.hotkey_generation を使い（hide が bump して
            // 保留 show を無効化・codex #5/(B)#2）、純粋核 plan_hotkey で分岐する。
            let current_gen = app
                .try_state::<egui_shell::EguiShellState>()
                .map(|sh| sh.hotkey_generation.fetch_add(1, Ordering::SeqCst) + 1)
                .unwrap_or(0);
            let app_state = app.try_state::<AppState>();
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
                    .unwrap_or_else(|| GeneralConfig::default().hotkey_toggle);
            match egui_shell::plan_hotkey(visible, is_alt_pressed(), hotkey_toggle) {
                egui_shell::HotkeyPlan::HideNow => {
                    egui_shell::hide_egui_main(app, el);
                }
                egui_shell::HotkeyPlan::ShowNow => {
                    egui_shell::show_egui_main(app, el, t0);
                }
                egui_shell::HotkeyPlan::ShowAfterAltRelease => {
                    // 待機はイベントループを塞げないので別スレッドで行い、**再入するときに
                    // もう一度 marshalling する**。世代の照合もイベントループ上で行う——
                    // 照合と show のあいだに別の押下が割り込まないため。
                    let h = app.clone();
                    std::thread::spawn(move || {
                        wait_alt_release_or_timeout();
                        snotra_egui_runtime::on_event_loop(&h, move |app, el| {
                            // 共有世代が変わっていたら（別 press や hide が bump）show を諦める。
                            let gen_now = app
                                .try_state::<egui_shell::EguiShellState>()
                                .map(|sh| sh.hotkey_generation.load(Ordering::SeqCst))
                                .unwrap_or(0);
                            if gen_now != current_gen {
                                return;
                            }
                            egui_shell::show_egui_main(app, el, Instant::now());
                        });
                    });
                }
            }
        });
    });

    // hotkey-pressed listener is now registered; activate hotkey on platform thread.
    // Registering the hotkey only after the listener is ready ensures no event
    // is emitted before there is a receiver to handle it.
    // **ここまでが「送信の直前」である。** 以降 `hotkey_register` の区間は、platform
    // スレッドが登録を終えるまでを測る。
    startup::mark(startup::Phase::SetupRest);

    // **送信できなかった経路にも終端を置く。** bridge state 不在・`Mutex` の poison・
    // channel 切断のいずれでも `RegisterInitialHotkey` の arm は走らないので、ここで
    // `startup:failed` を出さないとハーネスには「タイムアウト」としか見えない。
    let sent = match app_handle.try_state::<Mutex<PlatformBridge>>() {
        Some(bridge) => match bridge.lock() {
            // **写像は 1 か所に集約してある**（`startup::StartupFailure::from`）。
            // ここでワイルドカードを書くと、`BridgeError` に variant を足したとき
            // 黙って既存の `reason` へ潰れる——`reason` はハーネスの契約である。
            Ok(b) => b
                .send_initial_hotkey_registration()
                .map_err(startup::StartupFailure::from),
            Err(_) => Err(startup::StartupFailure::PlatformBridgeUnavailable),
        },
        None => Err(startup::StartupFailure::PlatformBridgeUnavailable),
    };
    if let Err(failure) = sent {
        startup::finish(Err(failure));
    }
}

/// Listen for the `open-settings` event emitted by the tray menu.
fn setup_open_settings_listener(app_handle: &AppHandle) {
    let handle_for_settings = app_handle.clone();
    app_handle.listen(crate::events::OPEN_SETTINGS, move |_| {
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
    app_handle.listen(crate::events::EXIT_REQUESTED, move |_| {
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
fn setup_background_rescan(
    app_handle: &AppHandle,
    rescan_task: Option<indexer::BackgroundRescanTask>,
) {
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
        // setup フック自身がイベントループの中で走る（`src-tauri/CLAUDE.md`「ウィンドウ生成の
        // 制約」）ため `on_event_loop` はインライン実行へ倒れるが、**証人を作れるのは
        // `on_event_loop` の中だけ**なので包む形は必要である。
        snotra_egui_runtime::on_event_loop(app_handle, |app, el| {
            egui_shell::show_egui_main(app, el, Instant::now());
        });
    }
}
