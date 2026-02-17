use std::cell::RefCell;

use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{
    InitCommonControls, NMHDR, TCIF_TEXT, TCITEMW, TCM_GETCURSEL, TCM_INSERTITEMW, TCN_SELCHANGE,
    WC_TABCONTROLW,
};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::config::{Config, ScanPath, SearchModeConfig, ThemePreset};

const IDC_TAB: i32 = 2000;
const IDC_SAVE: i32 = 2001;
const IDC_CANCEL: i32 = 2002;
const IDC_STATUS: i32 = 2003;

const IDC_HOTKEY_MODIFIER: i32 = 2100;
const IDC_HOTKEY_KEY: i32 = 2101;
const IDC_GENERAL_HOTKEY_TOGGLE: i32 = 2110;
const IDC_GENERAL_SHOW_ON_STARTUP: i32 = 2111;
const IDC_GENERAL_AUTO_HIDE: i32 = 2112;
const IDC_GENERAL_SHOW_TRAY: i32 = 2113;
const IDC_GENERAL_IME_OFF: i32 = 2114;
const IDC_GENERAL_TITLE_BAR: i32 = 2115;

const IDC_SEARCH_NORMAL_MODE: i32 = 2200;
const IDC_SEARCH_FOLDER_MODE: i32 = 2201;
const IDC_SEARCH_MAX_RESULTS: i32 = 2202;
const IDC_SEARCH_SHOW_HIDDEN: i32 = 2203;
const IDC_SEARCH_MAX_HISTORY: i32 = 2204;

const IDC_SCAN_LIST: i32 = 2300;
const IDC_SCAN_PATH: i32 = 2301;
const IDC_SCAN_EXT: i32 = 2302;
const IDC_SCAN_INCLUDE_FOLDERS: i32 = 2303;
const IDC_SCAN_ADD: i32 = 2304;
const IDC_SCAN_UPDATE: i32 = 2305;
const IDC_SCAN_DELETE: i32 = 2306;
const IDC_TOP_N_HISTORY: i32 = 2307;
const IDC_SHOW_ICONS: i32 = 2308;
const IDC_REBUILD: i32 = 2309;
const REBUILD_SPINNER_TIMER_ID: usize = 1;
const REBUILD_SPINNER_INTERVAL_MS: u32 = 120;
const SPINNER_FRAMES: [char; 4] = ['|', '/', '-', '\\'];

const IDC_VISUAL_PRESET: i32 = 2400;
const IDC_VISUAL_BG: i32 = 2401;
const IDC_VISUAL_INPUT_BG: i32 = 2402;
const IDC_VISUAL_TEXT: i32 = 2403;
const IDC_VISUAL_SELECTED: i32 = 2404;
const IDC_VISUAL_HINT: i32 = 2405;
const IDC_VISUAL_FONT_FAMILY: i32 = 2406;
const IDC_VISUAL_FONT_SIZE: i32 = 2407;

const IDC_LABEL_GENERAL_MODIFIER: i32 = 2500;
const IDC_LABEL_GENERAL_KEY: i32 = 2501;
const IDC_LABEL_SEARCH_NORMAL: i32 = 2510;
const IDC_LABEL_SEARCH_FOLDER: i32 = 2511;
const IDC_LABEL_SEARCH_MAX_RESULTS: i32 = 2512;
const IDC_LABEL_SEARCH_MAX_HISTORY: i32 = 2513;
const IDC_LABEL_INDEX_LIST: i32 = 2520;
const IDC_LABEL_INDEX_PATH: i32 = 2521;
const IDC_LABEL_INDEX_EXT: i32 = 2522;
const IDC_LABEL_INDEX_TOP_N: i32 = 2523;
const IDC_LABEL_VISUAL_PRESET: i32 = 2530;
const IDC_LABEL_VISUAL_BG: i32 = 2531;
const IDC_LABEL_VISUAL_INPUT_BG: i32 = 2532;
const IDC_LABEL_VISUAL_TEXT: i32 = 2533;
const IDC_LABEL_VISUAL_SELECTED: i32 = 2534;
const IDC_LABEL_VISUAL_HINT: i32 = 2535;
const IDC_LABEL_VISUAL_FONT_FAMILY: i32 = 2536;
const IDC_LABEL_VISUAL_FONT_SIZE: i32 = 2537;

thread_local! {
    static SETTINGS_STATE: RefCell<Option<SettingsState>> = const { RefCell::new(None) };
    static PENDING_OPEN: RefCell<Option<PendingOpen>> = const { RefCell::new(None) };
}

pub struct ApplyResult {
    pub applied: Config,
    pub hotkey_ok: bool,
}

pub struct SettingsHooks {
    pub on_apply: Box<dyn Fn(Config) -> ApplyResult>,
    pub on_rebuild: Box<dyn Fn(Config) -> bool>,
}

struct PendingOpen {
    config: Config,
    hooks: SettingsHooks,
}

struct SettingsState {
    hwnd: HWND,
    config: Config,
    initial_config: Config,
    hooks: SettingsHooks,
    rebuild_in_progress: bool,
    spinner_index: usize,
}

pub fn open_or_focus(config: Config, hooks: SettingsHooks) {
    if let Some(hwnd) = existing_window() {
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
        }
        return;
    }

    PENDING_OPEN.with(|p| {
        *p.borrow_mut() = Some(PendingOpen { config, hooks });
    });

    unsafe {
        InitCommonControls();

        let instance = match GetModuleHandleW(None) {
            Ok(v) => v,
            Err(_) => return,
        };
        let class_name = w!("SnotraSettingsWindow");

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(settings_wnd_proc),
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: HBRUSH::default(),
            lpszClassName: class_name,
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);
        let placement = crate::window_data::load_settings_placement();

        let hwnd = CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            class_name,
            w!("Snotra 設定"),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            placement.map(|p| p.x).unwrap_or(CW_USEDEFAULT),
            placement.map(|p| p.y).unwrap_or(CW_USEDEFAULT),
            760,
            560,
            HWND::default(),
            None,
            instance,
            None,
        );
        if let Ok(hwnd) = hwnd {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
    }
}

fn existing_window() -> Option<HWND> {
    SETTINGS_STATE.with(|s| {
        s.borrow()
            .as_ref()
            .and_then(|state| unsafe { IsWindow(state.hwnd).as_bool().then_some(state.hwnd) })
    })
}

unsafe extern "system" fn settings_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let pending = PENDING_OPEN.with(|p| p.borrow_mut().take());
            let Some(pending) = pending else {
                return LRESULT(0);
            };

            create_controls(hwnd);
            let mut state = SettingsState {
                hwnd,
                config: pending.config.clone(),
                initial_config: pending.config,
                hooks: pending.hooks,
                rebuild_in_progress: false,
                spinner_index: 0,
            };
            fill_controls_from_config(&mut state);
            show_tab(hwnd, 0);
            SETTINGS_STATE.with(|s| *s.borrow_mut() = Some(state));
            LRESULT(0)
        }
        WM_NOTIFY => {
            let hdr = lparam.0 as *const NMHDR;
            if !hdr.is_null() {
                let hdr = &*hdr;
                if hdr.idFrom as i32 == IDC_TAB && hdr.code == TCN_SELCHANGE {
                    let tab = GetDlgItem(hwnd, IDC_TAB).unwrap_or_default();
                    let idx = SendMessageW(tab, TCM_GETCURSEL, WPARAM(0), LPARAM(0)).0 as i32;
                    show_tab(hwnd, idx.max(0));
                }
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as i32;
            let notify = ((wparam.0 >> 16) & 0xFFFF) as u32;
            handle_command(hwnd, id, notify);
            LRESULT(0)
        }
        WM_TIMER => {
            let timer_id = wparam.0;
            if timer_id == REBUILD_SPINNER_TIMER_ID {
                tick_rebuild_spinner(hwnd);
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            persist_settings_placement(hwnd);
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            persist_settings_placement(hwnd);
            let _ = KillTimer(hwnd, REBUILD_SPINNER_TIMER_ID);
            SETTINGS_STATE.with(|s| *s.borrow_mut() = None);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn persist_settings_placement(hwnd: HWND) {
    unsafe {
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_ok() {
            crate::window_data::save_settings_placement(crate::window_data::WindowPlacement {
                x: rect.left,
                y: rect.top,
            });
        }
    }
}

fn create_controls(hwnd: HWND) {
    unsafe {
        let instance = match GetModuleHandleW(None) {
            Ok(v) => v,
            Err(_) => return,
        };

        let tab = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            WC_TABCONTROLW,
            w!(""),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            12,
            12,
            720,
            450,
            hwnd,
            HMENU(IDC_TAB as *mut _),
            instance,
            None,
        )
        .unwrap_or_default();

        add_tab(tab, 0, "全般");
        add_tab(tab, 1, "検索");
        add_tab(tab, 2, "インデックス");
        add_tab(tab, 3, "ビジュアル");

        create_static(
            hwnd,
            "ホットキー修飾キー:",
            30,
            60,
            150,
            20,
            IDC_LABEL_GENERAL_MODIFIER,
        );
        create_edit(hwnd, "", 190, 58, 180, 24, IDC_HOTKEY_MODIFIER);
        create_static(
            hwnd,
            "ホットキーキー:",
            390,
            60,
            110,
            20,
            IDC_LABEL_GENERAL_KEY,
        );
        create_edit(hwnd, "", 500, 58, 120, 24, IDC_HOTKEY_KEY);

        create_checkbox(
            hwnd,
            "呼び出しキーで表示/非表示トグル",
            30,
            100,
            360,
            22,
            IDC_GENERAL_HOTKEY_TOGGLE,
            true,
        );
        create_checkbox(
            hwnd,
            "起動時にウィンドウ表示",
            30,
            126,
            300,
            22,
            IDC_GENERAL_SHOW_ON_STARTUP,
            true,
        );
        create_checkbox(
            hwnd,
            "フォーカス喪失時の自動非表示",
            30,
            152,
            340,
            22,
            IDC_GENERAL_AUTO_HIDE,
            true,
        );
        create_checkbox(
            hwnd,
            "タスクトレイアイコン表示",
            30,
            178,
            340,
            22,
            IDC_GENERAL_SHOW_TRAY,
            true,
        );
        create_checkbox(
            hwnd,
            "IME をオフにする",
            30,
            204,
            280,
            22,
            IDC_GENERAL_IME_OFF,
            true,
        );
        create_checkbox(
            hwnd,
            "タイトルバー表示",
            30,
            230,
            280,
            22,
            IDC_GENERAL_TITLE_BAR,
            true,
        );

        create_static(
            hwnd,
            "通常時検索方式:",
            30,
            60,
            130,
            20,
            IDC_LABEL_SEARCH_NORMAL,
        );
        create_combo(hwnd, 190, 58, 180, 200, IDC_SEARCH_NORMAL_MODE);
        fill_search_mode_combo(hwnd, IDC_SEARCH_NORMAL_MODE);
        create_static(
            hwnd,
            "フォルダ展開時検索方式:",
            390,
            60,
            170,
            20,
            IDC_LABEL_SEARCH_FOLDER,
        );
        create_combo(hwnd, 560, 58, 150, 200, IDC_SEARCH_FOLDER_MODE);
        fill_search_mode_combo(hwnd, IDC_SEARCH_FOLDER_MODE);

        create_static(
            hwnd,
            "最大表示件数:",
            30,
            100,
            120,
            20,
            IDC_LABEL_SEARCH_MAX_RESULTS,
        );
        create_edit(hwnd, "", 190, 98, 80, 24, IDC_SEARCH_MAX_RESULTS);
        create_static(
            hwnd,
            "履歴表示最大件数:",
            300,
            100,
            140,
            20,
            IDC_LABEL_SEARCH_MAX_HISTORY,
        );
        create_edit(hwnd, "", 450, 98, 80, 24, IDC_SEARCH_MAX_HISTORY);
        create_checkbox(
            hwnd,
            "隠し/システム項目を表示",
            30,
            132,
            220,
            22,
            IDC_SEARCH_SHOW_HIDDEN,
            true,
        );

        create_static(
            hwnd,
            "スキャン条件一覧 (path | ext1,ext2 | folder=0/1)",
            30,
            60,
            360,
            20,
            IDC_LABEL_INDEX_LIST,
        );
        create_listbox(hwnd, 30, 82, 680, 140, IDC_SCAN_LIST);
        create_static(hwnd, "パス:", 30, 236, 50, 20, IDC_LABEL_INDEX_PATH);
        create_edit(hwnd, "", 80, 234, 480, 24, IDC_SCAN_PATH);
        create_static(
            hwnd,
            "拡張子(,区切り):",
            30,
            266,
            110,
            20,
            IDC_LABEL_INDEX_EXT,
        );
        create_edit(hwnd, "", 150, 264, 410, 24, IDC_SCAN_EXT);
        create_checkbox(
            hwnd,
            "フォルダ含む",
            580,
            264,
            120,
            24,
            IDC_SCAN_INCLUDE_FOLDERS,
            true,
        );
        create_button(hwnd, "追加", 580, 232, 60, 24, IDC_SCAN_ADD);
        create_button(hwnd, "更新", 650, 232, 60, 24, IDC_SCAN_UPDATE);
        create_button(hwnd, "削除", 650, 264, 60, 24, IDC_SCAN_DELETE);
        create_static(
            hwnd,
            "履歴保存上位 N:",
            30,
            302,
            110,
            20,
            IDC_LABEL_INDEX_TOP_N,
        );
        create_edit(hwnd, "", 150, 300, 80, 24, IDC_TOP_N_HISTORY);
        create_checkbox(
            hwnd,
            "アイコン表示",
            260,
            300,
            120,
            24,
            IDC_SHOW_ICONS,
            true,
        );
        create_button(hwnd, "再構築", 580, 300, 130, 28, IDC_REBUILD);

        create_static(
            hwnd,
            "プリセット:",
            30,
            60,
            100,
            20,
            IDC_LABEL_VISUAL_PRESET,
        );
        create_combo(hwnd, 150, 58, 180, 200, IDC_VISUAL_PRESET);
        create_static(
            hwnd,
            "背景色 (#RRGGBB):",
            30,
            96,
            120,
            20,
            IDC_LABEL_VISUAL_BG,
        );
        create_edit(hwnd, "", 150, 94, 120, 24, IDC_VISUAL_BG);
        create_static(
            hwnd,
            "入力背景色:",
            290,
            96,
            90,
            20,
            IDC_LABEL_VISUAL_INPUT_BG,
        );
        create_edit(hwnd, "", 390, 94, 120, 24, IDC_VISUAL_INPUT_BG);
        create_static(
            hwnd,
            "文字色:",
            30,
            126,
            120,
            20,
            IDC_LABEL_VISUAL_TEXT,
        );
        create_edit(hwnd, "", 150, 124, 120, 24, IDC_VISUAL_TEXT);
        create_static(
            hwnd,
            "選択行色:",
            290,
            126,
            90,
            20,
            IDC_LABEL_VISUAL_SELECTED,
        );
        create_edit(hwnd, "", 390, 124, 120, 24, IDC_VISUAL_SELECTED);
        create_static(
            hwnd,
            "ヒント文字色:",
            30,
            156,
            120,
            20,
            IDC_LABEL_VISUAL_HINT,
        );
        create_edit(hwnd, "", 150, 154, 120, 24, IDC_VISUAL_HINT);
        create_static(
            hwnd,
            "フォント:",
            30,
            190,
            120,
            20,
            IDC_LABEL_VISUAL_FONT_FAMILY,
        );
        create_edit(hwnd, "", 150, 188, 220, 24, IDC_VISUAL_FONT_FAMILY);
        create_static(
            hwnd,
            "サイズ:",
            390,
            190,
            60,
            20,
            IDC_LABEL_VISUAL_FONT_SIZE,
        );
        create_edit(hwnd, "", 450, 188, 60, 24, IDC_VISUAL_FONT_SIZE);

        create_button(hwnd, "保存", 500, 474, 100, 30, IDC_SAVE);
        create_button(hwnd, "閉じる", 610, 474, 100, 30, IDC_CANCEL);
        create_static(hwnd, "", 20, 478, 460, 24, IDC_STATUS);

        fill_preset_combo(hwnd);
    }
}

fn add_tab(tab: HWND, index: usize, label: &str) {
    let mut wide = to_wide(label);
    let mut item = TCITEMW {
        mask: TCIF_TEXT,
        pszText: PWSTR(wide.as_mut_ptr()),
        ..Default::default()
    };
    unsafe {
        let _ = SendMessageW(
            tab,
            TCM_INSERTITEMW,
            WPARAM(index),
            LPARAM((&mut item as *mut TCITEMW) as isize),
        );
    }
}

fn create_static(hwnd: HWND, text: &str, x: i32, y: i32, w: i32, h: i32, id: i32) {
    let wide = to_wide(text);
    unsafe {
        let instance = GetModuleHandleW(None).ok().unwrap_or_default();
        let _ = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            PCWSTR(wide.as_ptr()),
            WS_CHILD | WS_VISIBLE,
            x,
            y,
            w,
            h,
            hwnd,
            HMENU(id as *mut _),
            instance,
            None,
        );
    }
}

fn create_edit(hwnd: HWND, text: &str, x: i32, y: i32, w: i32, h: i32, id: i32) {
    let wide = to_wide(text);
    unsafe {
        let instance = GetModuleHandleW(None).ok().unwrap_or_default();
        let _ = CreateWindowExW(
            WS_EX_CLIENTEDGE,
            w!("EDIT"),
            PCWSTR(wide.as_ptr()),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
            x,
            y,
            w,
            h,
            hwnd,
            HMENU(id as *mut _),
            instance,
            None,
        );
    }
}

fn create_combo(hwnd: HWND, x: i32, y: i32, w: i32, h: i32, id: i32) {
    unsafe {
        let instance = GetModuleHandleW(None).ok().unwrap_or_default();
        let combo = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("COMBOBOX"),
            w!(""),
            WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | WINDOW_STYLE((CBS_DROPDOWNLIST | WS_VSCROLL.0 as i32) as u32),
            x,
            y,
            w,
            h,
            hwnd,
            HMENU(id as *mut _),
            instance,
            None,
        )
        .unwrap_or_default();
        let _ = combo;
    }
}

fn create_checkbox(hwnd: HWND, text: &str, x: i32, y: i32, w: i32, h: i32, id: i32, enabled: bool) {
    let wide = to_wide(text);
    unsafe {
        let instance = GetModuleHandleW(None).ok().unwrap_or_default();
        let ctrl = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("BUTTON"),
            PCWSTR(wide.as_ptr()),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            x,
            y,
            w,
            h,
            hwnd,
            HMENU(id as *mut _),
            instance,
            None,
        )
        .unwrap_or_default();
        let _ = SendMessageW(
            ctrl,
            WM_ENABLE,
            WPARAM(if enabled { 1 } else { 0 }),
            LPARAM(0),
        );
    }
}

fn create_button(hwnd: HWND, text: &str, x: i32, y: i32, w: i32, h: i32, id: i32) {
    let wide = to_wide(text);
    unsafe {
        let instance = GetModuleHandleW(None).ok().unwrap_or_default();
        let _ = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("BUTTON"),
            PCWSTR(wide.as_ptr()),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            x,
            y,
            w,
            h,
            hwnd,
            HMENU(id as *mut _),
            instance,
            None,
        );
    }
}

fn create_listbox(hwnd: HWND, x: i32, y: i32, w: i32, h: i32, id: i32) {
    unsafe {
        let instance = GetModuleHandleW(None).ok().unwrap_or_default();
        let _ = CreateWindowExW(
            WS_EX_CLIENTEDGE,
            w!("LISTBOX"),
            w!(""),
            WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | WINDOW_STYLE((LBS_NOTIFY | WS_VSCROLL.0 as i32) as u32),
            x,
            y,
            w,
            h,
            hwnd,
            HMENU(id as *mut _),
            instance,
            None,
        );
    }
}

fn set_control_enabled(hwnd: HWND, id: i32, enabled: bool) {
    unsafe {
        let ctrl = GetDlgItem(hwnd, id).unwrap_or_default();
        let _ = SendMessageW(ctrl, WM_ENABLE, WPARAM(if enabled { 1 } else { 0 }), LPARAM(0));
    }
}

fn set_rebuild_controls_enabled(hwnd: HWND, enabled: bool) {
    set_control_enabled(hwnd, IDC_SAVE, enabled);
    set_control_enabled(hwnd, IDC_REBUILD, enabled);
    set_control_enabled(hwnd, IDC_SCAN_ADD, enabled);
    set_control_enabled(hwnd, IDC_SCAN_UPDATE, enabled);
    set_control_enabled(hwnd, IDC_SCAN_DELETE, enabled);
}

fn begin_rebuild_ui_state(state: &mut SettingsState) {
    if state.rebuild_in_progress {
        return;
    }
    state.rebuild_in_progress = true;
    state.spinner_index = 0;
    let hwnd = state.hwnd;
    set_rebuild_controls_enabled(hwnd, false);
    unsafe {
        let _ = SetTimer(
            hwnd,
            REBUILD_SPINNER_TIMER_ID,
            REBUILD_SPINNER_INTERVAL_MS,
            None,
        );
    }
    set_control_text(hwnd, IDC_STATUS, "インデックス再構築中... |");
}

fn tick_rebuild_spinner(hwnd: HWND) {
    SETTINGS_STATE.with(|cell| {
        let mut binding = cell.borrow_mut();
        let Some(state) = binding.as_mut() else {
            return;
        };
        if !state.rebuild_in_progress {
            return;
        }
        state.spinner_index = (state.spinner_index + 1) % SPINNER_FRAMES.len();
        let frame = SPINNER_FRAMES[state.spinner_index];
        let text = format!("インデックス再構築中... {}", frame);
        set_control_text(hwnd, IDC_STATUS, &text);
    });
}

fn end_rebuild_ui_state(state: &mut SettingsState, text: &str) {
    state.rebuild_in_progress = false;
    state.spinner_index = 0;
    let hwnd = state.hwnd;
    unsafe {
        let _ = KillTimer(hwnd, REBUILD_SPINNER_TIMER_ID);
    }
    set_rebuild_controls_enabled(hwnd, true);
    set_control_text(hwnd, IDC_STATUS, text);
}

fn fill_controls_from_config(state: &mut SettingsState) {
    set_control_text(
        state.hwnd,
        IDC_HOTKEY_MODIFIER,
        &state.config.hotkey.modifier,
    );
    set_control_text(state.hwnd, IDC_HOTKEY_KEY, &state.config.hotkey.key);
    set_checkbox(
        state.hwnd,
        IDC_GENERAL_HOTKEY_TOGGLE,
        state.config.general.hotkey_toggle,
    );
    set_checkbox(
        state.hwnd,
        IDC_GENERAL_SHOW_ON_STARTUP,
        state.config.general.show_on_startup,
    );
    set_checkbox(
        state.hwnd,
        IDC_GENERAL_AUTO_HIDE,
        state.config.general.auto_hide_on_focus_lost,
    );
    set_checkbox(
        state.hwnd,
        IDC_GENERAL_SHOW_TRAY,
        state.config.general.show_tray_icon,
    );
    set_checkbox(
        state.hwnd,
        IDC_GENERAL_IME_OFF,
        state.config.general.ime_off_on_show,
    );
    set_checkbox(
        state.hwnd,
        IDC_GENERAL_TITLE_BAR,
        state.config.general.show_title_bar,
    );

    set_mode_combo(
        state.hwnd,
        IDC_SEARCH_NORMAL_MODE,
        state.config.search.normal_mode,
    );
    set_mode_combo(
        state.hwnd,
        IDC_SEARCH_FOLDER_MODE,
        state.config.search.folder_mode,
    );
    set_control_text(
        state.hwnd,
        IDC_SEARCH_MAX_RESULTS,
        &state.config.appearance.max_results.to_string(),
    );
    set_control_text(
        state.hwnd,
        IDC_SEARCH_MAX_HISTORY,
        &state.config.appearance.max_history_display.to_string(),
    );
    set_checkbox(
        state.hwnd,
        IDC_SEARCH_SHOW_HIDDEN,
        state.config.search.show_hidden_system,
    );

    set_control_text(
        state.hwnd,
        IDC_TOP_N_HISTORY,
        &state.config.appearance.top_n_history.to_string(),
    );
    set_checkbox(
        state.hwnd,
        IDC_SHOW_ICONS,
        state.config.appearance.show_icons,
    );

    set_theme_preset_combo(state.hwnd, state.config.visual.preset);
    set_control_text(
        state.hwnd,
        IDC_VISUAL_BG,
        &state.config.visual.background_color,
    );
    set_control_text(
        state.hwnd,
        IDC_VISUAL_INPUT_BG,
        &state.config.visual.input_background_color,
    );
    set_control_text(
        state.hwnd,
        IDC_VISUAL_TEXT,
        &state.config.visual.text_color,
    );
    set_control_text(
        state.hwnd,
        IDC_VISUAL_SELECTED,
        &state.config.visual.selected_row_color,
    );
    set_control_text(
        state.hwnd,
        IDC_VISUAL_HINT,
        &state.config.visual.hint_text_color,
    );
    set_control_text(
        state.hwnd,
        IDC_VISUAL_FONT_FAMILY,
        &state.config.visual.font_family,
    );
    set_control_text(
        state.hwnd,
        IDC_VISUAL_FONT_SIZE,
        &state.config.visual.font_size.to_string(),
    );

    refresh_scan_list(state.hwnd, &state.config.paths.scan);
}

fn show_tab(hwnd: HWND, tab: i32) {
    const GENERAL_IDS: &[i32] = &[
        IDC_LABEL_GENERAL_MODIFIER,
        IDC_LABEL_GENERAL_KEY,
        IDC_HOTKEY_MODIFIER,
        IDC_HOTKEY_KEY,
        IDC_GENERAL_HOTKEY_TOGGLE,
        IDC_GENERAL_SHOW_ON_STARTUP,
        IDC_GENERAL_AUTO_HIDE,
        IDC_GENERAL_SHOW_TRAY,
        IDC_GENERAL_IME_OFF,
        IDC_GENERAL_TITLE_BAR,
    ];
    const SEARCH_IDS: &[i32] = &[
        IDC_LABEL_SEARCH_NORMAL,
        IDC_LABEL_SEARCH_FOLDER,
        IDC_LABEL_SEARCH_MAX_RESULTS,
        IDC_LABEL_SEARCH_MAX_HISTORY,
        IDC_SEARCH_NORMAL_MODE,
        IDC_SEARCH_FOLDER_MODE,
        IDC_SEARCH_MAX_RESULTS,
        IDC_SEARCH_SHOW_HIDDEN,
        IDC_SEARCH_MAX_HISTORY,
    ];
    const INDEX_IDS: &[i32] = &[
        IDC_LABEL_INDEX_LIST,
        IDC_LABEL_INDEX_PATH,
        IDC_LABEL_INDEX_EXT,
        IDC_LABEL_INDEX_TOP_N,
        IDC_SCAN_LIST,
        IDC_SCAN_PATH,
        IDC_SCAN_EXT,
        IDC_SCAN_INCLUDE_FOLDERS,
        IDC_SCAN_ADD,
        IDC_SCAN_UPDATE,
        IDC_SCAN_DELETE,
        IDC_TOP_N_HISTORY,
        IDC_SHOW_ICONS,
        IDC_REBUILD,
    ];
    const VISUAL_IDS: &[i32] = &[
        IDC_LABEL_VISUAL_PRESET,
        IDC_LABEL_VISUAL_BG,
        IDC_LABEL_VISUAL_INPUT_BG,
        IDC_LABEL_VISUAL_TEXT,
        IDC_LABEL_VISUAL_SELECTED,
        IDC_LABEL_VISUAL_HINT,
        IDC_LABEL_VISUAL_FONT_FAMILY,
        IDC_LABEL_VISUAL_FONT_SIZE,
        IDC_VISUAL_PRESET,
        IDC_VISUAL_BG,
        IDC_VISUAL_INPUT_BG,
        IDC_VISUAL_TEXT,
        IDC_VISUAL_SELECTED,
        IDC_VISUAL_HINT,
        IDC_VISUAL_FONT_FAMILY,
        IDC_VISUAL_FONT_SIZE,
    ];

    for id in GENERAL_IDS {
        show_control(hwnd, *id, tab == 0);
    }
    for id in SEARCH_IDS {
        show_control(hwnd, *id, tab == 1);
    }
    for id in INDEX_IDS {
        show_control(hwnd, *id, tab == 2);
    }
    for id in VISUAL_IDS {
        show_control(hwnd, *id, tab == 3);
    }
}

fn show_control(hwnd: HWND, id: i32, show: bool) {
    unsafe {
        let ctrl = GetDlgItem(hwnd, id).unwrap_or_default();
        if ctrl.is_invalid() {
            return;
        }
        let _ = ShowWindow(ctrl, if show { SW_SHOW } else { SW_HIDE });
    }
}

fn handle_command(hwnd: HWND, id: i32, notify: u32) {
    if id == IDC_CANCEL {
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        return;
    }

    if id == IDC_SAVE {
        save_from_ui(hwnd, false);
        return;
    }

    if id == IDC_REBUILD {
        rebuild_from_ui(hwnd);
        return;
    }

    if id == IDC_SCAN_ADD {
        scan_add(hwnd);
        return;
    }

    if id == IDC_SCAN_UPDATE {
        scan_update(hwnd);
        return;
    }

    if id == IDC_SCAN_DELETE {
        scan_delete(hwnd);
        return;
    }

    if id == IDC_SCAN_LIST && notify == LBN_SELCHANGE as u32 {
        scan_load_selected(hwnd);
        return;
    }

    if id == IDC_VISUAL_PRESET && notify == CBN_SELCHANGE as u32 {
        apply_visual_preset_to_controls(hwnd, get_theme_preset_combo(hwnd));
    }
}

fn save_from_ui(hwnd: HWND, close_after_save: bool) {
    SETTINGS_STATE.with(|cell| {
        let mut binding = cell.borrow_mut();
        let Some(state) = binding.as_mut() else {
            return;
        };

        let baseline = state.initial_config.clone();
        let requested = read_config_from_controls(hwnd, &state.config);
        let apply = (state.hooks.on_apply)(requested);
        let applied = apply.applied;
        let rebuild_needed = needs_rebuild(&baseline, &applied);
        state.config = applied.clone();
        state.initial_config = applied.clone();
        fill_controls_from_config(state);

        if !apply.hotkey_ok {
            info_box(
                hwnd,
                "ホットキーの再登録に失敗したため、旧設定を維持しました。",
            );
        }

        if rebuild_needed && ask_rebuild(hwnd) {
            begin_rebuild_ui_state(state);
            if !(state.hooks.on_rebuild)(applied.clone()) {
                end_rebuild_ui_state(state, "再構築開始に失敗しました");
            }
        } else {
            set_control_text(hwnd, IDC_STATUS, "保存しました");
        }

        if close_after_save {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
        }
    });
}

fn rebuild_from_ui(hwnd: HWND) {
    SETTINGS_STATE.with(|cell| {
        let mut binding = cell.borrow_mut();
        let Some(state) = binding.as_mut() else {
            return;
        };

        let requested = read_config_from_controls(hwnd, &state.config);
        let apply = (state.hooks.on_apply)(requested);
        state.config = apply.applied.clone();
        state.initial_config = state.config.clone();
        fill_controls_from_config(state);

        if !apply.hotkey_ok {
            info_box(
                hwnd,
                "ホットキーの再登録に失敗したため、旧設定を維持しました。",
            );
        }

        if ask_rebuild(hwnd) {
            begin_rebuild_ui_state(state);
            if !(state.hooks.on_rebuild)(state.config.clone()) {
                end_rebuild_ui_state(state, "再構築開始に失敗しました");
            }
        }
    });
}

pub fn notify_rebuild_finished(success: bool) {
    SETTINGS_STATE.with(|s| {
        let mut binding = s.borrow_mut();
        let Some(state) = binding.as_mut() else {
            return;
        };
        let text = if success {
            "インデックス再構築が完了しました"
        } else {
            "インデックス再構築に失敗しました"
        };
        end_rebuild_ui_state(state, text);
    });
}

fn ask_rebuild(hwnd: HWND) -> bool {
    let text = to_wide("設定変更によりインデックス再構築が必要です。再構築を開始しますか？");
    let caption = to_wide("Snotra");
    unsafe {
        MessageBoxW(
            hwnd,
            PCWSTR(text.as_ptr()),
            PCWSTR(caption.as_ptr()),
            MB_YESNO | MB_ICONQUESTION,
        ) == IDYES
    }
}

fn info_box(hwnd: HWND, text: &str) {
    let w_text = to_wide(text);
    let caption = to_wide("Snotra");
    unsafe {
        let _ = MessageBoxW(
            hwnd,
            PCWSTR(w_text.as_ptr()),
            PCWSTR(caption.as_ptr()),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

fn needs_rebuild(old: &Config, new: &Config) -> bool {
    old.paths.scan != new.paths.scan
        || old.search.show_hidden_system != new.search.show_hidden_system
        || old.appearance.show_icons != new.appearance.show_icons
}

fn read_config_from_controls(hwnd: HWND, base: &Config) -> Config {
    let mut cfg = base.clone();

    cfg.hotkey.modifier = get_control_text(hwnd, IDC_HOTKEY_MODIFIER);
    cfg.hotkey.key = get_control_text(hwnd, IDC_HOTKEY_KEY);
    cfg.general.hotkey_toggle = get_checkbox(hwnd, IDC_GENERAL_HOTKEY_TOGGLE);
    cfg.general.show_on_startup = get_checkbox(hwnd, IDC_GENERAL_SHOW_ON_STARTUP);
    cfg.general.auto_hide_on_focus_lost = get_checkbox(hwnd, IDC_GENERAL_AUTO_HIDE);
    cfg.general.show_tray_icon = get_checkbox(hwnd, IDC_GENERAL_SHOW_TRAY);
    cfg.general.ime_off_on_show = get_checkbox(hwnd, IDC_GENERAL_IME_OFF);
    cfg.general.show_title_bar = get_checkbox(hwnd, IDC_GENERAL_TITLE_BAR);

    cfg.search.normal_mode = get_mode_combo(hwnd, IDC_SEARCH_NORMAL_MODE);
    cfg.search.folder_mode = get_mode_combo(hwnd, IDC_SEARCH_FOLDER_MODE);
    cfg.search.show_hidden_system = get_checkbox(hwnd, IDC_SEARCH_SHOW_HIDDEN);
    cfg.appearance.max_results = parse_usize(
        &get_control_text(hwnd, IDC_SEARCH_MAX_RESULTS),
        cfg.appearance.max_results,
        1,
        50,
    );
    cfg.appearance.max_history_display = parse_usize(
        &get_control_text(hwnd, IDC_SEARCH_MAX_HISTORY),
        cfg.appearance.max_history_display,
        1,
        50,
    );
    cfg.appearance.max_history_display = cfg
        .appearance
        .max_history_display
        .min(cfg.appearance.max_results);

    cfg.appearance.top_n_history = parse_usize(
        &get_control_text(hwnd, IDC_TOP_N_HISTORY),
        cfg.appearance.top_n_history,
        10,
        5000,
    );
    cfg.appearance.show_icons = get_checkbox(hwnd, IDC_SHOW_ICONS);

    cfg.visual.preset = get_theme_preset_combo(hwnd);
    cfg.visual.background_color = normalize_hex_color(
        &get_control_text(hwnd, IDC_VISUAL_BG),
        &cfg.visual.background_color,
    );
    cfg.visual.input_background_color = normalize_hex_color(
        &get_control_text(hwnd, IDC_VISUAL_INPUT_BG),
        &cfg.visual.input_background_color,
    );
    cfg.visual.text_color = normalize_hex_color(
        &get_control_text(hwnd, IDC_VISUAL_TEXT),
        &cfg.visual.text_color,
    );
    cfg.visual.selected_row_color = normalize_hex_color(
        &get_control_text(hwnd, IDC_VISUAL_SELECTED),
        &cfg.visual.selected_row_color,
    );
    cfg.visual.hint_text_color = normalize_hex_color(
        &get_control_text(hwnd, IDC_VISUAL_HINT),
        &cfg.visual.hint_text_color,
    );
    let family = get_control_text(hwnd, IDC_VISUAL_FONT_FAMILY);
    cfg.visual.font_family = if family.trim().is_empty() {
        cfg.visual.font_family
    } else {
        family.trim().to_string()
    };
    cfg.visual.font_size = parse_u32(
        &get_control_text(hwnd, IDC_VISUAL_FONT_SIZE),
        cfg.visual.font_size,
        8,
        48,
    );

    cfg.paths.scan = read_scan_entries(hwnd, &cfg.paths.scan);
    cfg
}

fn parse_usize(input: &str, fallback: usize, min: usize, max: usize) -> usize {
    input
        .trim()
        .parse::<usize>()
        .ok()
        .map(|v| v.clamp(min, max))
        .unwrap_or(fallback)
}

fn parse_u32(input: &str, fallback: u32, min: u32, max: u32) -> u32 {
    input
        .trim()
        .parse::<u32>()
        .ok()
        .map(|v| v.clamp(min, max))
        .unwrap_or(fallback)
}

fn set_control_text(hwnd: HWND, id: i32, text: &str) {
    let wide = to_wide(text);
    unsafe {
        let ctrl = GetDlgItem(hwnd, id).unwrap_or_default();
        let _ = SetWindowTextW(ctrl, PCWSTR(wide.as_ptr()));
    }
}

fn get_control_text(hwnd: HWND, id: i32) -> String {
    unsafe {
        let ctrl = GetDlgItem(hwnd, id).unwrap_or_default();
        let len = GetWindowTextLengthW(ctrl) as usize;
        if len == 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len + 1];
        let _ = GetWindowTextW(ctrl, &mut buf);
        String::from_utf16_lossy(&buf[..len])
    }
}

fn set_checkbox(hwnd: HWND, id: i32, checked: bool) {
    unsafe {
        let ctrl = GetDlgItem(hwnd, id).unwrap_or_default();
        let _ = SendMessageW(
            ctrl,
            BM_SETCHECK,
            WPARAM(if checked { 1 } else { 0 }),
            LPARAM(0),
        );
    }
}

fn get_checkbox(hwnd: HWND, id: i32) -> bool {
    unsafe {
        let ctrl = GetDlgItem(hwnd, id).unwrap_or_default();
        SendMessageW(ctrl, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == 1
    }
}

fn combo_add(combo: HWND, text: &str) {
    let wide = to_wide(text);
    unsafe {
        let _ = SendMessageW(
            combo,
            CB_ADDSTRING,
            WPARAM(0),
            LPARAM(wide.as_ptr() as isize),
        );
    }
}

fn fill_search_mode_combo(hwnd: HWND, id: i32) {
    unsafe {
        let combo = GetDlgItem(hwnd, id).unwrap_or_default();
        let _ = SendMessageW(combo, CB_RESETCONTENT, WPARAM(0), LPARAM(0));
        combo_add(combo, "prefix");
        combo_add(combo, "substring");
        combo_add(combo, "fuzzy");
    }
}

fn fill_preset_combo(hwnd: HWND) {
    unsafe {
        let combo = GetDlgItem(hwnd, IDC_VISUAL_PRESET).unwrap_or_default();
        let _ = SendMessageW(combo, CB_RESETCONTENT, WPARAM(0), LPARAM(0));
        combo_add(combo, "obsidian");
        combo_add(combo, "paper");
        combo_add(combo, "solarized");
    }
}

fn set_mode_combo(hwnd: HWND, id: i32, mode: SearchModeConfig) {
    let idx = match mode {
        SearchModeConfig::Prefix => 0,
        SearchModeConfig::Substring => 1,
        SearchModeConfig::Fuzzy => 2,
    };
    unsafe {
        let ctrl = GetDlgItem(hwnd, id).unwrap_or_default();
        let _ = SendMessageW(ctrl, CB_SETCURSEL, WPARAM(idx), LPARAM(0));
    }
}

fn get_mode_combo(hwnd: HWND, id: i32) -> SearchModeConfig {
    unsafe {
        let ctrl = GetDlgItem(hwnd, id).unwrap_or_default();
        let idx = SendMessageW(ctrl, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0;
        match idx {
            0 => SearchModeConfig::Prefix,
            1 => SearchModeConfig::Substring,
            _ => SearchModeConfig::Fuzzy,
        }
    }
}

fn set_theme_preset_combo(hwnd: HWND, preset: ThemePreset) {
    let idx = match preset {
        ThemePreset::Obsidian => 0,
        ThemePreset::Paper => 1,
        ThemePreset::Solarized => 2,
    };
    unsafe {
        let ctrl = GetDlgItem(hwnd, IDC_VISUAL_PRESET).unwrap_or_default();
        let _ = SendMessageW(ctrl, CB_SETCURSEL, WPARAM(idx), LPARAM(0));
    }
}

fn get_theme_preset_combo(hwnd: HWND) -> ThemePreset {
    unsafe {
        let ctrl = GetDlgItem(hwnd, IDC_VISUAL_PRESET).unwrap_or_default();
        let idx = SendMessageW(ctrl, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0;
        match idx {
            1 => ThemePreset::Paper,
            2 => ThemePreset::Solarized,
            _ => ThemePreset::Obsidian,
        }
    }
}

fn refresh_scan_list(hwnd: HWND, scan: &[ScanPath]) {
    unsafe {
        let list = GetDlgItem(hwnd, IDC_SCAN_LIST).unwrap_or_default();
        let _ = SendMessageW(list, LB_RESETCONTENT, WPARAM(0), LPARAM(0));
        for sp in scan {
            let line = format!(
                "{} | {} | folder={}",
                sp.path,
                sp.extensions.join(","),
                if sp.include_folders { 1 } else { 0 }
            );
            let wide = to_wide(&line);
            let _ = SendMessageW(
                list,
                LB_ADDSTRING,
                WPARAM(0),
                LPARAM(wide.as_ptr() as isize),
            );
        }
    }
}

fn scan_load_selected(hwnd: HWND) {
    SETTINGS_STATE.with(|cell| {
        let binding = cell.borrow();
        let Some(state) = binding.as_ref() else {
            return;
        };
        unsafe {
            let list = GetDlgItem(hwnd, IDC_SCAN_LIST).unwrap_or_default();
            let idx = SendMessageW(list, LB_GETCURSEL, WPARAM(0), LPARAM(0)).0 as i32;
            if idx < 0 {
                return;
            }
            let idx = idx as usize;
            if idx >= state.config.paths.scan.len() {
                return;
            }
            let sp = &state.config.paths.scan[idx];
            set_control_text(hwnd, IDC_SCAN_PATH, &sp.path);
            set_control_text(hwnd, IDC_SCAN_EXT, &sp.extensions.join(","));
            set_checkbox(hwnd, IDC_SCAN_INCLUDE_FOLDERS, sp.include_folders);
        }
    });
}

fn parse_extensions(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| {
            if s.starts_with('.') {
                s.to_lowercase()
            } else {
                format!(".{}", s.to_lowercase())
            }
        })
        .collect()
}

fn scan_add(hwnd: HWND) {
    SETTINGS_STATE.with(|cell| {
        let mut binding = cell.borrow_mut();
        let Some(state) = binding.as_mut() else {
            return;
        };
        let path = get_control_text(hwnd, IDC_SCAN_PATH);
        if path.trim().is_empty() {
            set_control_text(hwnd, IDC_STATUS, "パスを入力してください");
            return;
        }
        let extensions = parse_extensions(&get_control_text(hwnd, IDC_SCAN_EXT));
        if extensions.is_empty() {
            set_control_text(hwnd, IDC_STATUS, "拡張子を1つ以上入力してください");
            return;
        }
        state.config.paths.scan.push(ScanPath {
            path: path.trim().to_string(),
            extensions,
            include_folders: get_checkbox(hwnd, IDC_SCAN_INCLUDE_FOLDERS),
        });
        refresh_scan_list(hwnd, &state.config.paths.scan);
        set_control_text(hwnd, IDC_STATUS, "スキャン条件を追加しました");
    });
}

fn scan_update(hwnd: HWND) {
    SETTINGS_STATE.with(|cell| {
        let mut binding = cell.borrow_mut();
        let Some(state) = binding.as_mut() else {
            return;
        };
        unsafe {
            let list = GetDlgItem(hwnd, IDC_SCAN_LIST).unwrap_or_default();
            let idx = SendMessageW(list, LB_GETCURSEL, WPARAM(0), LPARAM(0)).0 as i32;
            if idx < 0 {
                set_control_text(hwnd, IDC_STATUS, "更新対象を選択してください");
                return;
            }
            let idx = idx as usize;
            if idx >= state.config.paths.scan.len() {
                return;
            }

            let path = get_control_text(hwnd, IDC_SCAN_PATH);
            let extensions = parse_extensions(&get_control_text(hwnd, IDC_SCAN_EXT));
            if path.trim().is_empty() || extensions.is_empty() {
                set_control_text(hwnd, IDC_STATUS, "パスと拡張子を入力してください");
                return;
            }

            state.config.paths.scan[idx] = ScanPath {
                path: path.trim().to_string(),
                extensions,
                include_folders: get_checkbox(hwnd, IDC_SCAN_INCLUDE_FOLDERS),
            };
            refresh_scan_list(hwnd, &state.config.paths.scan);
            let _ = SendMessageW(list, LB_SETCURSEL, WPARAM(idx), LPARAM(0));
            set_control_text(hwnd, IDC_STATUS, "スキャン条件を更新しました");
        }
    });
}

fn scan_delete(hwnd: HWND) {
    SETTINGS_STATE.with(|cell| {
        let mut binding = cell.borrow_mut();
        let Some(state) = binding.as_mut() else {
            return;
        };
        unsafe {
            let list = GetDlgItem(hwnd, IDC_SCAN_LIST).unwrap_or_default();
            let idx = SendMessageW(list, LB_GETCURSEL, WPARAM(0), LPARAM(0)).0 as i32;
            if idx < 0 {
                set_control_text(hwnd, IDC_STATUS, "削除対象を選択してください");
                return;
            }
            let idx = idx as usize;
            if idx < state.config.paths.scan.len() {
                state.config.paths.scan.remove(idx);
                refresh_scan_list(hwnd, &state.config.paths.scan);
                set_control_text(hwnd, IDC_STATUS, "スキャン条件を削除しました");
            }
        }
    });
}

fn read_scan_entries(_hwnd: HWND, current: &[ScanPath]) -> Vec<ScanPath> {
    current.to_vec()
}

fn apply_visual_preset_to_controls(hwnd: HWND, preset: ThemePreset) {
    let (bg, input_bg, text, selected, hint, family, size) = match preset {
        ThemePreset::Obsidian => (
            "#282828",
            "#383838",
            "#E0E0E0",
            "#505050",
            "#808080",
            "Segoe UI",
            "15",
        ),
        ThemePreset::Paper => (
            "#FFFFFF",
            "#F2F2F2",
            "#141414",
            "#DADADA",
            "#707070",
            "Segoe UI",
            "15",
        ),
        ThemePreset::Solarized => (
            "#002B36",
            "#073642",
            "#839496",
            "#586E75",
            "#93A1A1",
            "Consolas",
            "15",
        ),
    };
    set_control_text(hwnd, IDC_VISUAL_BG, bg);
    set_control_text(hwnd, IDC_VISUAL_INPUT_BG, input_bg);
    set_control_text(hwnd, IDC_VISUAL_TEXT, text);
    set_control_text(hwnd, IDC_VISUAL_SELECTED, selected);
    set_control_text(hwnd, IDC_VISUAL_HINT, hint);
    set_control_text(hwnd, IDC_VISUAL_FONT_FAMILY, family);
    set_control_text(hwnd, IDC_VISUAL_FONT_SIZE, size);
}

fn normalize_hex_color(input: &str, fallback: &str) -> String {
    let trimmed = input.trim();
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return fallback.to_string();
    }
    format!("#{}", hex.to_uppercase())
}

fn to_wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
