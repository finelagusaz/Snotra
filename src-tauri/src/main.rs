//! アプリケーションのエントリポイント。Tauri のセットアップ・イベントリスナー登録・
//! トレイ/ホットキー起動を行う。
//!
//! 起動時の背景再スキャンは撤去済み（#1001 `docs/adr/ADR-rescan-explicit-only.md`）。
//! 索引の更新は `start_index_build` の kick だけを経路とする。起動が kick するのは、索引が
//! 無いとき（初回起動・`index.bin` を読めなかった起動）だけである（`InitialIndex`・#1448）。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod config_watcher;
mod egui_shell;
mod events;
mod heap_trace;
mod icon;
mod ime;
mod indexing;
mod monitor;
mod platform;
mod scan_log;
mod startup;
mod state;
mod trace;
mod working_set;

use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::time::Instant;

use serde_json::json;
use snotra_core::config::{Config, GeneralConfig, LoadOutcome};
use snotra_core::engine::Engine;
use snotra_core::history::HistoryStore;
use snotra_core::indexer;
use tauri::{AppHandle, Listener, Manager};

use crate::commands::SettingsProcessState;
use crate::icon::IconCacheState;

use crate::platform::{BridgeError, PlatformBridge, PlatformBridgePending, PlatformCommand};
use crate::state::AppState;

/// Alt の解放を見る間隔。Alt を離してから表示が始まるまでの遅れは、この間隔の 0〜1 倍である。
/// 待つ間だけ動く専用のスレッド（`ShowAfterAltRelease`）なので、細かく見ても他を塞がない
/// （#1446 で 10 → 1 ms。`thread::sleep` が実際に 1 ms で起きるかは確かめていない）。
const ALT_RELEASE_POLL_MS: u64 = 1;
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

/// MenuMaskKey 技法のマスクキー（vkE8・未割り当て）。Alt の押下と解放のあいだに挟むと、
/// 受け手はその Alt を単独の Alt と扱わず、メニューを起動しない。
#[cfg(windows)]
const VK_MASK: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY =
    windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(0xE8);

/// main ウィンドウを前面にする。tao の `set_focus` の代わりに使う（#1470）。
///
/// `SetForegroundWindow` が拒まれたら、Alt を注入して 1 回だけ再試行する（Alt の押下で
/// 前面化が許可される）。**その Alt は vkE8 で包む。** tao は包まない Alt を注入するので、
/// まだ前面にいる元のアプリがそれを単独の Alt として受け、メニューを起動する。
///
/// 修飾キーが押されていても、注入は省かない。省くと、最前面に見えるのに前面でないウィンドウが
/// 残り、打鍵が元のアプリへ流れ続ける。注入した Alt up は押されている Alt を論理的に離すが、
/// tao の注入も同じ Alt up を送っていた。
///
/// `PlatformBridge` を通さず直接呼ぶ。呼び出し元（`show_egui_main`）は、イベントループ上で
/// 同期に走る `send_alt_key_up` より前に前面化を済ませる必要がある。channel 越しの非同期では、
/// この順序を保てない。
///
/// 結果は trace `egui_show:foreground` に残す。`path` は `already`（既に前面）・`direct`
/// （1 回で通った）・`masked_alt`（注入して再試行した）。`sent` は注入した回だけ値を持ち、
/// `SendInput` が全件を受け付けたかを表す（UIPI に阻まれたときに偽になるかは未実測）。
#[cfg(windows)]
pub(crate) fn bring_to_foreground(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{INPUT, SendInput, VK_MENU};
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};

    let trace = |path: &str, ok: bool, sent: Option<bool>| {
        trace_main(
            "egui_show:foreground",
            json!({ "path": path, "ok": ok, "sent": sent }),
        );
    };
    unsafe {
        if GetForegroundWindow() == hwnd {
            return trace("already", true, None);
        }
        if SetForegroundWindow(hwnd).as_bool() {
            return trace("direct", true, None);
        }
        let inputs = [
            make_key_input(VK_MENU, false),
            make_key_input(VK_MASK, false),
            make_key_input(VK_MASK, true),
            make_key_input(VK_MENU, true),
        ];
        let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) as usize == inputs.len();
        trace(
            "masked_alt",
            SetForegroundWindow(hwnd).as_bool(),
            Some(sent),
        );
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
        INPUT, SendInput, VK_LMENU, VK_MENU, VK_RMENU,
    };

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
    // している focused window は**自スレッドが所有する main ウィンドウ**になった。スリープ中この
    // スレッドはポンプを回さないので、**フォアグラウンドが既に main ウィンドウへ移っていれば、
    // 待っているあいだにキー up が処理されることはもう無い**——その前提の下では
    // スリープの目的は自スレッド上では達成できず、純粋なコストになる
    // （`show_egui_main` の `SendMessageTimeoutW` 撤去と**同じ型の失効**。あちらの撤去跡の
    // コメントを参照）。**前提を落として「常に純粋なコスト」とは書けない**——直前の
    // 前面化（`bring_to_foreground`）は活性化の処理の完了を待たない。再試行でも通らなければ、
    // 旧ウィンドウが前景に残る。どちらの場合も、合成キー up は旧ウィンドウのスレッドで
    // 処理されうる（それこそがバリア撤去跡が「未実測」と断っている当の点である）。**hotkey に Alt を含む設定では実質すべての show がここを通る**
    // （`ShowAfterAltRelease` の再入時に `!is_alt_pressed()` が真になるため）。
    //
    // **撤去は次段の判断に残す。** 見込みとしては「別スレッドへ出す」ではなく**削除**が正しい
    // ——目的は「ウィンドウがキー up を処理できるようにする」ことであり、それを可能にするのは
    // 待つことではなく**早く返してポンプへ戻ること**だからである（**ただし上の前提が要る**
    // ——旧ウィンドウが前景に残る場合まで含めて削除してよいかは、測ってからでないと言えない）。加えて注入順は
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
    // 別に測る——測らないと、そこに住む遅延が計測区間の外へ落ちる。
    startup::begin();

    let is_first_run = Config::is_first_run();
    let (config, load_outcome) = Config::load_reporting();
    startup::mark(startup::Phase::ConfigLoad);
    // フォントの解決を先読みし、索引のロード以降と重ねる（#1452・`font_stack::prefetch` の doc）。
    egui_shell::prefetch_fonts(config.visual.font_family.clone());

    let (mut material, initial, cache_in_sync) = if is_first_run {
        (
            indexer::IndexMaterial::from_tree(snotra_core::index_tree::IndexTree::empty()),
            InitialIndex::FirstRun,
            false,
        )
    } else {
        // **起動の中では走査しない**（#1448）。読めなければ空の索引で起動し、setup が背景の
        // 構築を起こす（`start_initial_build`）。
        let result =
            indexer::load_cache_with_stats(&config.paths.scan, config.search.show_hidden_system);
        #[cfg(debug_assertions)]
        {
            let s = &result.stats;
            eprintln!(
                "[index-load] cache_hit={} total={}ms hash={}ms cache_load={}ms scan={}ms sort={}ms cache_save={}ms",
                s.cache_hit,
                s.total.as_millis(),
                s.hash.as_millis(),
                s.cache_load.as_millis(),
                s.scan.as_millis(),
                s.sort.as_millis(),
                s.cache_save.as_millis(),
            );
        }
        // 内側の内訳との差が `index_load_unattributed_ms` になる——`load_cache_with_stats`
        // の中にある未命名の処理を、外側の計測コードが検出するための項目である。
        startup::set_index_load_stats_total(result.stats.total);
        // **分岐は `LoadOrScanStats` から取る。** cache-miss の起動は `first_run: false` /
        // `cache_hit: false` で、`index_load` も刻む——`index_load` が null でよいのは first-run
        // だけである（起動の計測の契約）。
        startup::set_branch(startup::Branch {
            first_run: false,
            cache_hit: result.stats.cache_hit,
            include_path_env: config.search.include_path_env,
        });
        startup::mark(startup::Phase::IndexLoad);
        (
            result.material,
            InitialIndex::of(false, result.stats.cache_hit),
            result.cache_in_sync,
        )
    };
    let initial_indexing = initial.initial_indexing();

    // PATH エントリのスキャン + マージ（`indexing::build_index_from_material` と同じ手順の別実装——規則を変えるなら両方）。**ツリーとマスクは組のまま持つ**ので、片方だけ伸ばす形はここでは書けない（正本は `IndexMaterial` の doc）。**空の場合のガードは持たない**——追記もツリーへの追加も、空なら何もしない。
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

    let mut engine = Engine::from_material(material, history, config);
    // `index.bin` がこの索引と同じ走査の結果か。偽なら、設定変更の再構築でも `index.bin` を読まずに
    // 走査する（#1456）。
    engine.set_cache_in_sync(cache_in_sync);
    startup::mark(startup::Phase::EngineBuild);

    let app_state = AppState::new(engine, initial_indexing);

    // 宣言ウィンドウなし（tauri.conf.json の windows は空・#532 SU7 flip）。メインウィンドウは
    // setup フェーズで egui_shell::create が生成する。
    let mut app_context = tauri::generate_context!();
    // 分離プロファイルでは、単一インスタンス制御を普段使いの Snotra と分ける（SPEC.md §13・#1306）。
    // single-instance plugin は setup で identifier から mutex 名を作るので、ここで書き換えれば
    // 足りる。鍵は 16 進だけからなる（`\` が混じると mutex が黙って排他を失う）。
    if let Some(key) = Config::isolated_profile_key() {
        let identifier = format!("{}.profile-{key}", app_context.config().identifier);
        app_context.config_mut().identifier = identifier;
    }

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
            // ここまでが「tauri の初期化」であり、以降がウィンドウとリスナーである。
            startup::mark(startup::Phase::TauriInit);
            let app_handle = app.handle().clone();

            // Spawn the platform thread before window creation and wait for its Win32 init
            // only after windows are created, so the two run in parallel (SPEC §8.5・#1457).
            // Tray is NOT created here; SetTrayVisible is sent after full setup (SPEC §7.5).
            let platform =
                PlatformBridge::begin(app_handle.clone(), hotkey_config, initial_language);

            // ウィンドウ生成（egui・platform thread spawn 後・SPEC §8.5 で Win32 初期化と並列化）。
            // 幅の復元は create が window_width で行う（#532 SU7 flip で唯一の経路）。
            // **setup ブロック唯一の早期 return である。** ここで抜けると
            // `RegisterInitialHotkey` は送られないので、終端を出さないとハーネスには
            // 「タイムアウト」としか見えない。
            let handles = match egui_shell::create(app, window_width as f64, &bg_color) {
                Ok(h) => h,
                Err(e) => {
                    startup::finish(Err(startup::StartupFailure::WindowCreation));
                    return Err(Box::new(e));
                }
            };
            // **フォント解決を含む区間である**（`font_stack.rs`）。解決は `main()` で先読みを始めて
            // あるので（#1452）、ここに乗るのは先読みが終わっていなかった残りだけである。ウィンドウを
            // 一度も出していない時点で常駐に効くことが実測されており、表示より前に走る。
            startup::mark(startup::Phase::WindowsCreate);
            // platform スレッドの初期化を待って bridge を manage する。**bridge を使う listener の
            // 登録（下の `register_*` と `setup_hotkey_listener`）より前に置く**——`egui_shell::create`
            // と 2 つの view の `setup` は bridge に触れないので、ここまで待たなくてよい。
            finish_platform_thread(&app_handle, platform);
            // #671 PR D（spec 決定 8 の終端形）: show/hide を跨ぐ共有状態（世代・emit dedup）と
            // 両ウィンドウの wake handle。**`create()` の後**に manage する——handle は attach の戻り値
            // ゆえウィンドウの生成より前には存在しない。各 view の `setup()` はもう `EguiShellState` を
            // 読まないので（PR D で `register_ctx` を撤去した）、この順序で問題が無い。
            //
            // この順序が安全である根拠は 2 つある（`EguiShellState` の読み手はすべて
            // `if let Some(..)` ゆえ、manage 前にフレームが走ると消費が**沈黙して skip される**）:
            //
            // 1. **この setup ブロック自体がイベントループの 1 イテレーション内で走る。**
            //    tauri は setup フックを `RuntimeRunEvent::Ready` の処理中に呼ぶ
            //    （tauri 2.12.1 `src/app.rs` の `make_run_event_loop_callback`）。この間
            //    メッセージポンプは停止しており（「ウィンドウ生成の制約」と同じ機構）、
            //    wry plugin の `on_event`＝`attach_pending_windows` は setup の復帰後にしか
            //    走らない。**「setup はイベントループより前」ではない**——同じ 1 イベントの
            //    処理中である。
            // 2. **仮にフレームが走っても、この時点で pending なものは無い。**
            //    reset_pending は `show_egui_main` が、pending_hotkey_failure は下の 2 つの
            //    listener が、hotkey_generation は hide と hotkey listener が立てる——
            //    setter はすべてこの manage より後にしか動かない。
            app.manage(egui_shell::EguiShellState::new(&handles));
            // #671 PR A′: results ウィンドウの所有型を managed state へ。**listener 登録より前**に置く
            // ——hide_egui_main が try_state で引くため、hide が起こりうる時点より前に manage
            // されている必要がある。
            app.manage(handles.results_window);
            // view→emit→listener の合流点。**main の** hide を hide_egui_main の 1 経路に集約（codex #7）。
            egui_shell::register_hide_listener(&app_handle);
            // config 変更・indexing 状態変化の wake（SU6 spec 決定 1）。config_watcher 起動
            // （下の setup_config_watcher）と setup_startup_display より前に登録し、可視ウィンドウが
            // 合図を取りこぼすウィンドウを作らない（位置は spec が pin・並行性レビュー）。
            egui_shell::register_config_wake_listeners(&app_handle);
            // hotkey 登録失敗の pending 格納（spec 追補 2）。wake しない listener——
            // wake は config-applied（言語変更同時発生時の競合ウィンドウを閉じる）に委ねる。
            egui_shell::register_hotkey_failure_listener(&app_handle);
            // 起動時 hotkey 登録失敗の受け口（#652）。RegisterInitialHotkey を送る
            // setup_hotkey_listener より前に登録される位置なので emit を取りこぼさない
            //（この egui ブロック自体が finish_platform_thread の直後・hotkey listener の前）。
            egui_shell::register_initial_hotkey_failure_listener(&app_handle);
            app.manage(egui_shell::UpdaterUiState(std::sync::Mutex::new(
                Default::default(),
            )));
            // main と results ウィンドウが共有する一方向フローの入れ物（#646 PR2）。
            app.manage(egui_shell::ResultsShared::default());
            egui_shell::spawn_update_check(&app_handle);

            // 初回起動なら設定アプリを、cache-miss なら背景の構築を起こす（#1448）。この位置は
            // `PlatformBridge` を manage した後（構築中の表示がトレイへ届く）・hotkey listener の前。
            start_initial_build(&app_handle, initial);

            // Listen for hotkey-pressed events, then activate the hotkey on the
            // platform thread. Registering the listener before activating the
            // hotkey ensures no event is emitted before there is a receiver to
            // handle it — this order must not change (`docs/architecture.md`「起動シーケンスと初期化順序」).
            //
            // **`SetupRest` はこの呼び出しの中（送信の直前）で刻まれる。** platform
            // スレッドは送信の直後から並行に走り、登録を終えると終端を出す——そのとき
            // マークが済んでいないと `setup_rest` が `null` のまま出力される。
            setup_hotkey_listener(&app_handle);

            // Listen for open-settings event from tray
            setup_open_settings_listener(&app_handle);

            // Listen for exit request from tray
            setup_exit_listener(&app_handle);

            // Start config.toml file watcher for external changes (snotra-settings)
            setup_config_watcher(&app_handle);

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

/// Wait for the platform thread's Win32 init and manage the resulting `PlatformBridge`.
/// The spawn (`PlatformBridge::begin`) is at the setup call site, before window creation;
/// call this after window creation and before any listener that uses the bridge.
fn finish_platform_thread(
    app_handle: &AppHandle,
    pending: Result<PlatformBridgePending, BridgeError>,
) {
    match pending.and_then(PlatformBridgePending::wait) {
        Ok(bridge) => {
            app_handle.manage(Mutex::new(bridge));
        }
        // **起動はここで続行するが、終端は出す。** 出さないと `RegisterInitialHotkey` の
        // arm が走らないまま起動が終わり、ハーネスには「タイムアウト」としか見えない
        // （正本は `crate::startup` の `//!`）。spawn の失敗もここで出す（`begin` は結果を
        // 運ぶだけにして、終端を出す場所を 1 つに保つ）。
        Err(e) => startup::finish(Err(startup::StartupFailure::from(e))),
    }
}

/// 起動したときの索引の状態（#1448）。`main` が 1 回だけ導き、構築を起こすかを決める。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InitialIndex {
    /// `index.bin` を読めた。構築は起こさない。
    Loaded,
    /// 初回起動（`config.toml` が無い）。設定アプリで走査対象を確かめてから構築する。
    FirstRun,
    /// `index.bin` を読めなかった（版上げ・破損・止めている間の走査対象の変更）。空の索引で起動し、
    /// 背景で構築する。設定アプリは開かない。
    Deferred,
}

impl InitialIndex {
    fn of(is_first_run: bool, cache_hit: bool) -> Self {
        match (is_first_run, cache_hit) {
            (true, _) => Self::FirstRun,
            (false, true) => Self::Loaded,
            (false, false) => Self::Deferred,
        }
    }

    /// 起動の時点で構築中として扱うか（UI の構築中の表示とガード）。**構築を起こす状態のことである**
    /// （[`Self::initial_build`] から導く）——構築中として起動して構築を起こさないと、構築中の旗が
    /// 戻らず、トレイの「終了」もグレーアウトしたままになる。
    fn initial_indexing(self) -> bool {
        self.initial_build() != InitialBuild::None
    }

    /// setup が起こす構築。
    fn initial_build(self) -> InitialBuild {
        match self {
            Self::Loaded => InitialBuild::None,
            Self::FirstRun => InitialBuild::SettingsElseRescan,
            Self::Deferred => InitialBuild::Rescan,
        }
    }
}

/// [`InitialIndex::initial_build`] が決める、setup の構築の起こし方。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InitialBuild {
    None,
    /// 設定アプリを `--first-run` で起動する。起動できなければ構築する。
    SettingsElseRescan,
    /// 背景で構築する。
    Rescan,
}

/// 起動時の索引の状態に応じて構築を起こす（#1448）。
///
/// - `FirstRun`: 設定アプリを `--first-run` で起動する（索引タブでスキャン対象を確かめさせる）。
///   `initial_indexing` が真なので、`open_settings` の構築中のガードを迂回して直接起動する。
///   起動できなかったら（exe が無い・spawn の失敗）、既定の走査対象で構築する——構築中の旗が
///   いずれ戻り、構築の後に `open_settings` から設定を開ける
/// - `Deferred`: 背景で構築する。設定アプリは開かない
/// - `Loaded`: 何もしない
fn start_initial_build(app_handle: &AppHandle, initial: InitialIndex) {
    match initial.initial_build() {
        InitialBuild::SettingsElseRescan => {
            if commands::launch_settings_process(app_handle, &["--first-run"]).is_err() {
                indexing::start_index_build(app_handle, indexing::BuildRequest::Rescan);
            }
        }
        InitialBuild::Rescan => {
            indexing::start_index_build(app_handle, indexing::BuildRequest::Rescan);
        }
        InitialBuild::None => {}
    }
}

/// Register the `hotkey-pressed` listener, then activate the hotkey on the
/// platform thread. **Order must not change**: registering the listener
/// before sending `RegisterInitialHotkey` ensures no event is emitted before
/// there is a receiver to handle it (`docs/architecture.md`「起動シーケンスと初期化順序」).
fn setup_hotkey_listener(app_handle: &AppHandle) {
    let handle_for_hotkey = app_handle.clone();
    app_handle.listen(crate::events::HOTKEY_PRESSED, move |_| {
        let t0 = Instant::now();
        trace_main("hotkey:listener_enter", json!({}));
        // 設定画面の起動中はホットキーを無視する（ユーザーが新しい組み合わせを設定するために
        // 現在の組み合わせを押している可能性がある）。**ウィンドウに触らない読みなのでタスクの外に置く**
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
            let shell = app.try_state::<egui_shell::EguiShellState>();
            let current_gen = shell
                .as_ref()
                .map(|sh| sh.hotkey_generation.fetch_add(1, Ordering::SeqCst) + 1)
                .unwrap_or(0);
            let visible = egui_shell::read_main_visible(app, el);
            // hotkey_toggle は可視時の hide 判定にしか使わない（plan_hotkey）。`visible &&` で
            // 短絡し、非表示＝show 経路（最も遅延に敏感）では読みそのものを省く。
            // 表示中でも hotkey_toggle=false なら hide せず show 側（再フォーカス/再配置）へ回る。
            //
            // **読みは engine ロックを経ない**（#1032）。この短絡はかつて「show 経路で engine
            // ロックを取らない」ための対策でもあったが、`read_config` が engine の錠を避ける
            // ようになったので、いまは無駄な読みを省くだけの意味になった。
            let hotkey_toggle = visible
                && egui_shell::read_config(
                    app,
                    |c| c.general.hotkey_toggle,
                    || GeneralConfig::default().hotkey_toggle,
                );
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
    // **マークは送信より前でなければならない。** 送信した瞬間から platform スレッドが
    // 並行に走り、登録を終えると終端（`startup:finish`）を出す。マークが後ろにあると、
    // platform 側が先に着いた回だけ `setup_rest` が `null` のまま出力され、ハーネスが
    // 「説明されない null」で落ちる（/code-review が指摘。**実際に一度そう書いた**
    // ——setup ブロックの末尾へ移した版がそれで、送信より 5 つの setup ぶん後ろだった）。
    //
    // **代償は受け入れる。** この関数は setup ブロックの途中で呼ばれるので、以降の
    // リスナー登録・config watcher・トレイ生成は `hotkey_register` の区間へ含まれる
    // ——`hotkey_register` は「純粋な登録待ち」ではなく「送信後に main スレッドがやった
    // 残り + 登録待ち」である。**並行に走るものを 1 本の時間軸で刻む以上、どちらかへ
    // 寄せるしかない**（両方を正しく分けるには platform 側にも独立した anchor が要る）。
    startup::mark(startup::Phase::SetupRest);

    // **送信できなかった経路にも終端を置く。** bridge state 不在・`Mutex` の poison・
    // channel 切断のいずれでも `RegisterInitialHotkey` の arm は走らないので、ここで
    // `startup:failed` を出さないとハーネスには「タイムアウト」としか見えない。
    let sent = match app_handle.try_state::<Mutex<PlatformBridge>>() {
        Some(bridge) => match bridge.lock() {
            // **マッピングは 1 か所に集約してある**（`startup::StartupFailure::from`）。
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
        // setup フック自身がイベントループの中で走る（`src-tauri/CLAUDE.md`「ウィンドウ生成の制約」）ため `on_event_loop` はインライン実行にフォールバックするが、**証人を作れるのは
        // `on_event_loop` の中だけ**なので包む形は必要である。
        snotra_egui_runtime::on_event_loop(app_handle, |app, el| {
            egui_shell::show_egui_main(app, el, Instant::now());
        });
    }
}

#[cfg(test)]
mod tests {
    use super::InitialIndex;

    /// 起動時の索引の状態の導出（#1448）。cache-miss は first-run ではなく `Deferred` になり、
    /// 構築中として起動する。
    #[test]
    fn initial_index_is_derived_from_first_run_and_cache_hit() {
        assert_eq!(InitialIndex::of(true, false), InitialIndex::FirstRun);
        assert_eq!(InitialIndex::of(true, true), InitialIndex::FirstRun);
        assert_eq!(InitialIndex::of(false, true), InitialIndex::Loaded);
        assert_eq!(InitialIndex::of(false, false), InitialIndex::Deferred);

        assert!(InitialIndex::FirstRun.initial_indexing());
        assert!(
            InitialIndex::Deferred.initial_indexing(),
            "cache-miss の起動は構築中として起動する"
        );
        assert!(!InitialIndex::Loaded.initial_indexing());
    }

    /// 起動時の索引の状態ごとの構築の起こし方（#1448）。`initial_indexing` はこれから導く。
    #[test]
    fn initial_build_is_decided_by_the_initial_index() {
        use super::InitialBuild;
        assert_eq!(InitialIndex::Loaded.initial_build(), InitialBuild::None);
        assert_eq!(
            InitialIndex::FirstRun.initial_build(),
            InitialBuild::SettingsElseRescan
        );
        assert_eq!(
            InitialIndex::Deferred.initial_build(),
            InitialBuild::Rescan,
            "cache-miss の起動は背景で構築する（設定アプリは開かない）"
        );
    }
}
