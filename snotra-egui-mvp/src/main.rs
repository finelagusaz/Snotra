#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use snotra_core::{
    config::{AutoUpdateMode, Config},
    engine::Engine,
    history::HistoryStore,
    indexer::AppEntry,
    ui_types::SearchResult,
};
use snotra_egui_runtime::{EguiRuntime, EguiView, RuntimeFrame};
use tauri::Manager;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_updater::UpdaterExt;

const SEARCH_ENTRY_COUNT: usize = 10_000;
const ALT_RELEASE_POLL_MS: u64 = 10;
const ALT_RELEASE_TIMEOUT_MS: u64 = 350;

#[derive(Clone, Debug)]
enum UpdaterState {
    Idle,
    Checking,
    UpToDate,
    Available { version: String, can_install: bool },
    Failed { message: String },
}

struct MvpView {
    app_handle: tauri::AppHandle,
    query: String,
    engine: Engine,
    results: Vec<SearchResult>,
    selected: usize,
    updater: Arc<Mutex<UpdaterState>>,
    updater_check_started: bool,
    focus_search: bool,
    was_focused: bool,
    last_action: String,
    last_search_us: u128,
    started_at: Instant,
    first_frame_logged: bool,
    updater_mode: AutoUpdateMode,
    updater_download: bool,
}

impl MvpView {
    fn new(app_handle: tauri::AppHandle, started_at: Instant) -> Self {
        let engine_started = Instant::now();
        let engine = build_verification_engine(SEARCH_ENTRY_COUNT);
        let results = engine.recent_history();
        eprintln!(
            "SNOTRA_EGUI_ENGINE_READY entries={} elapsed_ms={:.3}",
            engine.entries().len(),
            engine_started.elapsed().as_secs_f64() * 1000.0
        );
        Self {
            app_handle,
            query: String::new(),
            engine,
            results,
            selected: 0,
            updater: Arc::new(Mutex::new(UpdaterState::Idle)),
            updater_check_started: false,
            focus_search: true,
            was_focused: false,
            last_action: "Enterで選択結果を確認できます".to_owned(),
            last_search_us: 0,
            started_at,
            first_frame_logged: false,
            updater_mode: if env_flag("SNOTRA_EGUI_MVP_DISABLE_UPDATER") {
                AutoUpdateMode::Disabled
            } else {
                parse_update_mode(std::env::var("SNOTRA_EGUI_MVP_UPDATE_MODE").ok().as_deref())
            },
            updater_download: env_flag("SNOTRA_EGUI_MVP_UPDATER_DOWNLOAD"),
        }
    }

    fn refresh_results(&mut self) {
        let started = Instant::now();
        self.results = if self.query.is_empty() {
            self.engine.recent_history()
        } else {
            self.engine.search(&self.query)
        };
        self.last_search_us = started.elapsed().as_micros();
        self.selected = self.selected.min(self.results.len().saturating_sub(1));
        eprintln!(
            "SNOTRA_EGUI_SEARCH query_chars={} results={} elapsed_us={}",
            self.query.chars().count(),
            self.results.len(),
            self.last_search_us
        );
    }

    fn begin_updater_check(&self, context: egui::Context) {
        {
            let mut state = self.updater.lock().expect("updater state lock");
            if matches!(*state, UpdaterState::Checking) {
                return;
            }
            *state = UpdaterState::Checking;
        }

        let handle = self.app_handle.clone();
        let updater_state = Arc::clone(&self.updater);
        let mode = self.updater_mode;
        let download = self.updater_download;
        tauri::async_runtime::spawn(async move {
            let next_state = match handle.updater() {
                Ok(updater) => match updater.check().await {
                    Ok(Some(update)) => {
                        let version = update.version.clone();
                        if download {
                            match update.download(|_, _| {}, || {}).await {
                                Ok(bytes) => eprintln!(
                                    "SNOTRA_EGUI_UPDATER_DOWNLOAD_VERIFIED version={version} bytes={}",
                                    bytes.len()
                                ),
                                Err(error) => {
                                    *updater_state.lock().expect("updater state lock") =
                                        UpdaterState::Failed {
                                            message: error.to_string(),
                                        };
                                    context.request_repaint();
                                    return;
                                }
                            }
                        }
                        UpdaterState::Available {
                            version,
                            can_install: mode == AutoUpdateMode::Full,
                        }
                    }
                    Ok(None) => UpdaterState::UpToDate,
                    Err(error) => UpdaterState::Failed {
                        message: error.to_string(),
                    },
                },
                Err(error) => UpdaterState::Failed {
                    message: error.to_string(),
                },
            };
            eprintln!("SNOTRA_EGUI_MVP_UPDATER={next_state:?}");
            *updater_state.lock().expect("updater state lock") = next_state;
            context.request_repaint();
        });
    }
}

impl EguiView for MvpView {
    fn setup(&mut self, context: &egui::Context) {
        configure_japanese_font(context);
    }

    fn update(&mut self, ui: &mut egui::Ui, frame: &mut RuntimeFrame) {
        let context = ui.ctx().clone();
        if !self.first_frame_logged {
            self.first_frame_logged = true;
            eprintln!(
                "SNOTRA_EGUI_FIRST_FRAME elapsed_ms={:.3} pixels_per_point={:.3}",
                self.started_at.elapsed().as_secs_f64() * 1000.0,
                context.pixels_per_point()
            );
        }
        let focused = context.input(|input| input.focused);
        if focused && !self.was_focused {
            self.focus_search = true;
        }
        self.was_focused = focused;

        if !self.updater_check_started && self.updater_mode != AutoUpdateMode::Disabled {
            self.updater_check_started = true;
            self.begin_updater_check(context.clone());
        }
        if context.input(|input| input.key_pressed(egui::Key::Escape)) {
            frame.close_window();
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(egui::Color32::from_rgb(12, 14, 18)))
            .show(ui, |ui| {
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.heading("Snotra egui MVP");
                    ui.separator();
                    ui.label(
                        egui::RichText::new("Tauri native Window / WebViewなし")
                            .color(egui::Color32::from_rgb(95, 190, 130)),
                    );
                });
                ui.add_space(10.0);

                let search = ui.add_sized(
                    [ui.available_width(), 34.0],
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text("アプリ、フォルダーを検索")
                        .font(egui::TextStyle::Heading),
                );
                if search.changed() {
                    self.refresh_results();
                }
                if self.focus_search {
                    search.request_focus();
                    self.focus_search = false;
                }

                if self.results.is_empty() {
                    self.selected = 0;
                } else {
                    self.selected = self.selected.min(self.results.len() - 1);
                    if context.input(|input| input.key_pressed(egui::Key::ArrowDown)) {
                        self.selected = (self.selected + 1).min(self.results.len() - 1);
                    }
                    if context.input(|input| input.key_pressed(egui::Key::ArrowUp)) {
                        self.selected = self.selected.saturating_sub(1);
                    }
                    if context.input(|input| input.key_pressed(egui::Key::Enter)) {
                        self.last_action = format!("選択: {}", self.results[self.selected].name);
                    }
                }

                ui.add_space(8.0);
                egui::ScrollArea::vertical()
                    .max_height(230.0)
                    .show(ui, |ui| {
                        for (index, result) in self.results.iter().enumerate() {
                            let selected = index == self.selected;
                            let response = ui
                                .group(|ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        ui.label(if selected { "▶" } else { "  " });
                                        ui.vertical(|ui| {
                                            ui.label(egui::RichText::new(&result.name).strong());
                                            ui.label(
                                                egui::RichText::new(&result.path)
                                                    .small()
                                                    .color(egui::Color32::GRAY),
                                            );
                                        });
                                    });
                                })
                                .response
                                .interact(egui::Sense::click());
                            if response.clicked() {
                                self.selected = index;
                                self.last_action = format!("選択: {}", result.name);
                            }
                        }
                    });

                ui.separator();
                ui.horizontal(|ui| {
                    let updater_state = self.updater.lock().expect("updater state lock").clone();
                    if ui
                        .add_enabled(
                            self.updater_mode != AutoUpdateMode::Disabled
                                && !matches!(updater_state, UpdaterState::Checking),
                            egui::Button::new("Updaterを確認"),
                        )
                        .clicked()
                    {
                        self.begin_updater_check(context.clone());
                    }
                    match updater_state {
                        UpdaterState::Idle => {
                            ui.label("未確認");
                        }
                        UpdaterState::Checking => {
                            ui.spinner();
                            ui.label("確認中...");
                            context.request_repaint_after(std::time::Duration::from_millis(16));
                        }
                        UpdaterState::UpToDate => {
                            ui.label("最新版です");
                        }
                        UpdaterState::Available {
                            version,
                            can_install,
                        } => {
                            ui.label(format!("更新あり: {version} / install={can_install}"));
                        }
                        UpdaterState::Failed { message } => {
                            ui.colored_label(
                                egui::Color32::from_rgb(224, 96, 96),
                                format!("確認失敗: {message}"),
                            );
                        }
                    }
                    if self.updater_mode == AutoUpdateMode::Disabled {
                        ui.label("更新無効");
                    }
                });
                ui.label(&self.last_action);
                ui.small(format!(
                    "Engine: {} entries / last search: {} µs",
                    self.engine.entries().len(),
                    self.last_search_us
                ));
                ui.small("Alt+Q: 表示/非表示 / Esc: 終了 / ↑↓: 選択 / Enter: 決定");
            });
    }
}

fn build_verification_engine(entry_count: usize) -> Engine {
    let mut entries = vec![
        AppEntry {
            name: "Visual Studio Code".to_owned(),
            target_path: "C:/Program Files/Microsoft VS Code/Code.exe".to_owned(),
            is_folder: false,
        },
        AppEntry {
            name: "Windows Terminal".to_owned(),
            target_path: "C:/Program Files/WindowsApps/wt.exe".to_owned(),
            is_folder: false,
        },
        AppEntry {
            name: "ドキュメント".to_owned(),
            target_path: "C:/Users/User/Documents".to_owned(),
            is_folder: true,
        },
        AppEntry {
            name: "ダウンロード".to_owned(),
            target_path: "C:/Users/User/Downloads".to_owned(),
            is_folder: true,
        },
        AppEntry {
            name: "Snotra Settings".to_owned(),
            target_path: "snotra-settings.exe".to_owned(),
            is_folder: false,
        },
        AppEntry {
            name: "日本語入力テスト".to_owned(),
            target_path: "C:/Snotra/Verification/JapaneseInput.exe".to_owned(),
            is_folder: false,
        },
    ];
    for index in entries.len()..entry_count {
        entries.push(AppEntry {
            name: format!("Benchmark App {index:05}"),
            target_path: format!("C:/Snotra/Verification/App{index:05}.exe"),
            is_folder: false,
        });
    }

    let history_path = std::env::temp_dir().join(format!(
        "snotra-egui-mvp-history-{}-unused",
        std::process::id()
    ));
    let history = HistoryStore::load_in(&history_path);
    let mut engine = Engine::new(entries, history, Config::normalized_default());
    for path in [
        "C:/Program Files/Microsoft VS Code/Code.exe",
        "C:/Program Files/WindowsApps/wt.exe",
        "C:/Users/User/Documents",
        "C:/Users/User/Downloads",
        "snotra-settings.exe",
    ] {
        engine.record_launch(path, "");
    }
    engine
}

fn parse_update_mode(value: Option<&str>) -> AutoUpdateMode {
    match value {
        Some("check_only") => AutoUpdateMode::CheckOnly,
        Some("disabled") => AutoUpdateMode::Disabled,
        _ => AutoUpdateMode::Full,
    }
}

fn env_flag(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| value != "0")
}

fn env_usize(name: &str) -> Option<usize> {
    std::env::var(name).ok()?.parse().ok()
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

/// フォーカス移動後に残留Alt状態を解除する。ダミーvkE8を先行させるのは、
/// bare Alt-upとしてメニューやビープを発火させないためである。
#[cfg(windows)]
fn send_alt_key_up() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, SendInput, VIRTUAL_KEY, VK_LMENU, VK_MENU, VK_RMENU,
    };

    const VK_MASK: VIRTUAL_KEY = VIRTUAL_KEY(0xE8);
    let inputs = [
        make_key_input(VK_MASK, false),
        make_key_input(VK_MASK, true),
        make_key_input(VK_MENU, true),
        make_key_input(VK_LMENU, true),
        make_key_input(VK_RMENU, true),
    ];
    unsafe {
        let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
    std::thread::sleep(Duration::from_millis(5));
}

#[cfg(not(windows))]
fn send_alt_key_up() {}

fn show_and_focus(window: &tauri::Window, visible: &AtomicBool, hotkey_started: Instant) {
    let _ = window.show();
    visible.store(true, Ordering::SeqCst);
    let _ = window.set_focus();

    // SetForegroundWindowは部分的に非同期なので、WM_NULLで対象スレッドが
    // activationメッセージを処理し終えるまで待ってからAltを解放する。
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

    send_alt_key_up();
    eprintln!(
        "SNOTRA_EGUI_MVP_HOTKEY=shown elapsed_ms={:.3}",
        hotkey_started.elapsed().as_secs_f64() * 1000.0
    );
}

#[derive(Clone, Copy, Debug, Default)]
struct ProcessSnapshot {
    handles: u32,
    gdi_objects: u32,
    user_objects: u32,
    working_set: usize,
    private_bytes: usize,
}

#[cfg(windows)]
fn process_snapshot() -> ProcessSnapshot {
    use windows::Win32::System::{
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::{
            GR_GDIOBJECTS, GR_USEROBJECTS, GetCurrentProcess, GetGuiResources,
            GetProcessHandleCount,
        },
    };

    let process = unsafe { GetCurrentProcess() };
    let mut handles = 0;
    let mut memory = PROCESS_MEMORY_COUNTERS {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    unsafe {
        let _ = GetProcessHandleCount(process, &mut handles);
        let _ = GetProcessMemoryInfo(process, &mut memory, memory.cb);
    }
    ProcessSnapshot {
        handles,
        gdi_objects: unsafe { GetGuiResources(process, GR_GDIOBJECTS) },
        user_objects: unsafe { GetGuiResources(process, GR_USEROBJECTS) },
        working_set: memory.WorkingSetSize,
        private_bytes: memory.PagefileUsage,
    }
}

#[cfg(not(windows))]
fn process_snapshot() -> ProcessSnapshot {
    ProcessSnapshot::default()
}

#[cfg(windows)]
fn force_japanese_ime_for_verification(window: &tauri::Window) {
    use windows::{
        Win32::{
            Foundation::{LPARAM, WPARAM},
            UI::{
                Input::{
                    Ime::{ImmGetContext, ImmReleaseContext, ImmSetOpenStatus},
                    KeyboardAndMouse::{KLF_ACTIVATE, LoadKeyboardLayoutW},
                },
                WindowsAndMessaging::{SendMessageW, WM_INPUTLANGCHANGEREQUEST},
            },
        },
        core::w,
    };

    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    // 検証プロセスのUIスレッドだけを日本語配列へ切り替える。プロセス終了時に
    // 破棄されるため、ユーザーの既定入力方式は変更しない。
    let Ok(layout) = (unsafe { LoadKeyboardLayoutW(w!("00000411"), KLF_ACTIVATE) }) else {
        eprintln!("SNOTRA_EGUI_FORCE_IME layout=false");
        return;
    };
    unsafe {
        let _ = SendMessageW(
            hwnd,
            WM_INPUTLANGCHANGEREQUEST,
            Some(WPARAM(0)),
            Some(LPARAM(layout.0 as isize)),
        );
    }
    let context = unsafe { ImmGetContext(hwnd) };
    if context.0.is_null() {
        eprintln!("SNOTRA_EGUI_FORCE_IME context=false");
        return;
    }
    let opened = unsafe { ImmSetOpenStatus(context, true).as_bool() };
    unsafe {
        let _ = ImmReleaseContext(hwnd, context);
    }
    eprintln!("SNOTRA_EGUI_FORCE_IME context=true opened={opened}");
}

#[cfg(not(windows))]
fn force_japanese_ime_for_verification(_window: &tauri::Window) {}

fn run_hide_show_probe(window: tauri::Window, app_handle: tauri::AppHandle, cycles: usize) {
    std::thread::spawn(move || {
        // GPU・フォント・ショートカットの遅延初期化を測定区間から除外する。
        std::thread::sleep(Duration::from_secs(2));
        let before = process_snapshot();
        let started = Instant::now();
        for _ in 0..cycles {
            let _ = window.hide();
            std::thread::sleep(Duration::from_millis(5));
            let _ = window.show();
            let _ = window.set_focus();
            std::thread::sleep(Duration::from_millis(5));
        }
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        let after = process_snapshot();
        eprintln!(
            "SNOTRA_EGUI_HIDE_SHOW cycles={cycles} elapsed_ms={elapsed_ms:.3} avg_ms={:.3} handles={}->{} gdi={}->{} user={}->{} working_set={}->{} private={}->{}",
            elapsed_ms / cycles.max(1) as f64,
            before.handles,
            after.handles,
            before.gdi_objects,
            after.gdi_objects,
            before.user_objects,
            after.user_objects,
            before.working_set,
            after.working_set,
            before.private_bytes,
            after.private_bytes,
        );
        app_handle.exit(0);
    });
}

/// `jp_font` を登録し Proportional/Monospace の先頭（`insert(0, ...)`）へ差し込んだ
/// `FontDefinitions` を組む純粋部分。`push`（末尾）だと Latin=egui既定/CJK=Yu Gothic の
/// 2フォントに分かれ、softbuffer の被覆AA無しラスタが vertical metrics 差を整数pxへ
/// 丸めて混在行のベースラインをずらす（#399/#579）。呼び出し側は `ctx.set_fonts` を担う。
fn japanese_font_definitions(bytes: &'static [u8]) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    let mut font = egui::FontData::from_static(bytes);
    font.tweak = egui::FontTweak {
        scale: 1.0,
        y_offset_factor: 0.3,
        y_offset: 0.0,
        ..Default::default()
    };
    fonts.font_data.insert("jp_font".to_owned(), font.into());
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, "jp_font".to_owned());
    }
    fonts
}

static JP_FONT_BYTES: OnceLock<Box<[u8]>> = OnceLock::new();

fn configure_japanese_font(context: &egui::Context) {
    let candidates = [
        "C:/Windows/Fonts/YuGothM.ttc",
        "C:/Windows/Fonts/yugothic.ttf",
        "C:/Windows/Fonts/msgothic.ttc",
        "C:/Windows/Fonts/meiryo.ttc",
    ];

    if JP_FONT_BYTES.get().is_none() {
        for path in candidates {
            let Ok(bytes) = std::fs::read(path) else {
                continue;
            };
            let _ = JP_FONT_BYTES.set(bytes.into_boxed_slice());
            break;
        }
    }

    let Some(bytes) = JP_FONT_BYTES.get() else {
        eprintln!("warning: no Japanese font found; MVP Japanese text will use fallback glyphs");
        return;
    };
    context.set_fonts(japanese_font_definitions(bytes));
}

fn main() {
    let started_at = Instant::now();
    // SU1: 検証プローブは常に可視起動する（env 既定 false の罠を避ける。
    // 旧 SNOTRA_EGUI_MVP_START_VISIBLE 依存は撤去 — #532）。
    let start_visible = true;
    let hide_show_cycles = env_usize("SNOTRA_EGUI_MVP_HIDE_SHOW_CYCLES").unwrap_or(0);
    let runtime = EguiRuntime::new();
    let setup_runtime = runtime.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(move |app| {
            setup_runtime.install(app);
            let alt_q = Shortcut::new(Some(Modifiers::ALT), Code::KeyQ);
            let handler_alt_q = alt_q;
            let visible = Arc::new(AtomicBool::new(start_visible));
            let visible_for_handler = Arc::clone(&visible);
            let hotkey_generation = Arc::new(AtomicU64::new(0));
            let hotkey_generation_for_handler = Arc::clone(&hotkey_generation);
            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(move |app, shortcut, event| {
                        if shortcut != &handler_alt_q || event.state() != ShortcutState::Pressed {
                            return;
                        }
                        let Some(window) = app.get_window("egui-mvp") else {
                            return;
                        };
                        let current_generation =
                            hotkey_generation_for_handler.fetch_add(1, Ordering::SeqCst) + 1;
                        if visible_for_handler.load(Ordering::SeqCst) {
                            let _ = window.hide();
                            visible_for_handler.store(false, Ordering::SeqCst);
                            eprintln!("SNOTRA_EGUI_MVP_HOTKEY=hidden");
                        } else if is_alt_pressed() {
                            let visible_for_show = Arc::clone(&visible_for_handler);
                            let generation_for_wait = Arc::clone(&hotkey_generation_for_handler);
                            let hotkey_started = Instant::now();
                            std::thread::spawn(move || {
                                wait_alt_release_or_timeout();
                                eprintln!(
                                    "SNOTRA_EGUI_MVP_ALT_WAIT elapsed_ms={:.3}",
                                    hotkey_started.elapsed().as_secs_f64() * 1000.0
                                );
                                if generation_for_wait.load(Ordering::SeqCst)
                                    != current_generation
                                {
                                    return;
                                }
                                show_and_focus(&window, &visible_for_show, hotkey_started);
                            });
                        } else {
                            show_and_focus(&window, &visible_for_handler, Instant::now());
                        }
                    })
                    .build(),
            )?;
            app.global_shortcut().register(alt_q)?;
            let window = tauri::Window::builder(app, "egui-mvp")
                .title("Snotra egui MVP")
                .inner_size(720.0, 470.0)
                .min_inner_size(520.0, 360.0)
                .center()
                .visible(start_visible)
                .build()?;
            let scale_factor = window.scale_factor()?;
            let inner_size = window.inner_size()?;
            eprintln!(
                "SNOTRA_EGUI_WINDOW_READY scale_factor={scale_factor:.3} physical={}x{} elapsed_ms={:.3}",
                inner_size.width,
                inner_size.height,
                started_at.elapsed().as_secs_f64() * 1000.0
            );
            let probe_window = window.clone();
            setup_runtime.attach(window, MvpView::new(app.handle().clone(), started_at))?;
            if env_flag("SNOTRA_EGUI_MVP_FORCE_JAPANESE_IME") {
                force_japanese_ime_for_verification(&probe_window);
            }
            if hide_show_cycles > 0 {
                run_hide_show_probe(
                    probe_window,
                    app.handle().clone(),
                    hide_show_cycles,
                );
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run Snotra egui MVP");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verification_engine_searches_with_real_core_engine() {
        let mut engine = build_verification_engine(100);

        let english = engine.search("Visual Studio");
        assert_eq!(
            english.first().map(|result| result.name.as_str()),
            Some("Visual Studio Code")
        );
        let japanese = engine.search("ドキュメント");
        assert_eq!(
            japanese.first().map(|result| result.name.as_str()),
            Some("ドキュメント")
        );
        let ime_commit = engine.search("日本語");
        assert_eq!(
            ime_commit.first().map(|result| result.name.as_str()),
            Some("日本語入力テスト")
        );
    }

    #[test]
    fn jp_font_is_registered_at_index_zero_for_both_families() {
        // families への挿入は font bytes の妥当性に依らないため dummy で構造を検証。
        let dummy: &'static [u8] = &[0u8; 4];
        let fonts = japanese_font_definitions(dummy);
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            let list = fonts.families.get(&family).expect("family present");
            assert_eq!(
                list.first().map(String::as_str),
                Some("jp_font"),
                "jp_font must be index 0 for {family:?}（push=末尾だと #579 のベースラインずれが再発）"
            );
        }
    }

    #[test]
    fn updater_modes_preserve_install_capability_boundary() {
        assert_eq!(parse_update_mode(Some("full")), AutoUpdateMode::Full);
        assert_eq!(
            parse_update_mode(Some("check_only")),
            AutoUpdateMode::CheckOnly
        );
        assert_eq!(
            parse_update_mode(Some("disabled")),
            AutoUpdateMode::Disabled
        );
        assert_eq!(parse_update_mode(Some("unknown")), AutoUpdateMode::Full);
    }
}
