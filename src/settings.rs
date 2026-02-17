use std::cell::RefCell;

use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{
    InitCommonControls, NMHDR, TCIF_TEXT, TCITEMW, TCM_GETCURSEL, TCM_INSERTITEMW, TCN_SELCHANGE,
    WC_TABCONTROLW,
};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::config::{Config, ScanPath, SearchModeConfig};

const IDC_TAB: i32 = 2000;
const IDC_SAVE: i32 = 2001;
const IDC_CANCEL: i32 = 2002;
const IDC_STATUS: i32 = 2003;

const IDC_HOTKEY_MODIFIER: i32 = 2100;
const IDC_HOTKEY_KEY: i32 = 2101;
const IDC_UNSUPPORTED_GENERAL_1: i32 = 2110;
const IDC_UNSUPPORTED_GENERAL_2: i32 = 2111;
const IDC_UNSUPPORTED_GENERAL_3: i32 = 2112;
const IDC_UNSUPPORTED_GENERAL_4: i32 = 2113;
const IDC_UNSUPPORTED_GENERAL_5: i32 = 2114;
const IDC_UNSUPPORTED_GENERAL_6: i32 = 2115;

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

const IDC_VISUAL_NOTE: i32 = 2400;
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
    hooks: SettingsHooks,
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

        let hwnd = CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            class_name,
            w!("Snotra 設定"),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
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

pub fn set_status_text(text: &str) {
    SETTINGS_STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            set_control_text(state.hwnd, IDC_STATUS, text);
        }
    });
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
                config: pending.config,
                hooks: pending.hooks,
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
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            SETTINGS_STATE.with(|s| *s.borrow_mut() = None);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
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
            "呼び出しキーで表示/非表示トグル (Phase 6)",
            30,
            100,
            360,
            22,
            IDC_UNSUPPORTED_GENERAL_1,
            false,
        );
        create_checkbox(
            hwnd,
            "起動時にウィンドウ表示 (Phase 6)",
            30,
            126,
            300,
            22,
            IDC_UNSUPPORTED_GENERAL_2,
            false,
        );
        create_checkbox(
            hwnd,
            "フォーカス喪失時の自動非表示 (Phase 6)",
            30,
            152,
            340,
            22,
            IDC_UNSUPPORTED_GENERAL_3,
            false,
        );
        create_checkbox(
            hwnd,
            "タスクトレイアイコン表示切替 (Phase 6)",
            30,
            178,
            340,
            22,
            IDC_UNSUPPORTED_GENERAL_4,
            false,
        );
        create_checkbox(
            hwnd,
            "IME をオフにする (Phase 6)",
            30,
            204,
            280,
            22,
            IDC_UNSUPPORTED_GENERAL_5,
            false,
        );
        create_checkbox(
            hwnd,
            "タイトルバー表示切替 (Phase 6)",
            30,
            230,
            280,
            22,
            IDC_UNSUPPORTED_GENERAL_6,
            false,
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
            "ビジュアル設定は Phase 6 で有効化予定です。",
            30,
            60,
            360,
            24,
            IDC_VISUAL_NOTE,
        );

        create_button(hwnd, "保存", 500, 474, 100, 30, IDC_SAVE);
        create_button(hwnd, "閉じる", 610, 474, 100, 30, IDC_CANCEL);
        create_static(hwnd, "", 20, 478, 460, 24, IDC_STATUS);

        disable_control(hwnd, IDC_UNSUPPORTED_GENERAL_1);
        disable_control(hwnd, IDC_UNSUPPORTED_GENERAL_2);
        disable_control(hwnd, IDC_UNSUPPORTED_GENERAL_3);
        disable_control(hwnd, IDC_UNSUPPORTED_GENERAL_4);
        disable_control(hwnd, IDC_UNSUPPORTED_GENERAL_5);
        disable_control(hwnd, IDC_UNSUPPORTED_GENERAL_6);
        disable_control(hwnd, IDC_VISUAL_NOTE);
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
        combo_add(combo, "prefix");
        combo_add(combo, "substring");
        combo_add(combo, "fuzzy");
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

fn disable_control(hwnd: HWND, id: i32) {
    unsafe {
        let ctrl = GetDlgItem(hwnd, id).unwrap_or_default();
        let _ = SendMessageW(ctrl, WM_ENABLE, WPARAM(0), LPARAM(0));
    }
}

fn fill_controls_from_config(state: &mut SettingsState) {
    set_control_text(
        state.hwnd,
        IDC_HOTKEY_MODIFIER,
        &state.config.hotkey.modifier,
    );
    set_control_text(state.hwnd, IDC_HOTKEY_KEY, &state.config.hotkey.key);

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

    refresh_scan_list(state.hwnd, &state.config.paths.scan);
}

fn show_tab(hwnd: HWND, tab: i32) {
    const GENERAL_IDS: &[i32] = &[
        IDC_LABEL_GENERAL_MODIFIER,
        IDC_LABEL_GENERAL_KEY,
        IDC_HOTKEY_MODIFIER,
        IDC_HOTKEY_KEY,
        IDC_UNSUPPORTED_GENERAL_1,
        IDC_UNSUPPORTED_GENERAL_2,
        IDC_UNSUPPORTED_GENERAL_3,
        IDC_UNSUPPORTED_GENERAL_4,
        IDC_UNSUPPORTED_GENERAL_5,
        IDC_UNSUPPORTED_GENERAL_6,
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
    const VISUAL_IDS: &[i32] = &[IDC_VISUAL_NOTE];

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
    }
}

fn save_from_ui(hwnd: HWND, close_after_save: bool) {
    SETTINGS_STATE.with(|cell| {
        let mut binding = cell.borrow_mut();
        let Some(state) = binding.as_mut() else {
            return;
        };

        let old_config = state.config.clone();
        let requested = read_config_from_controls(hwnd, &old_config);
        let apply = (state.hooks.on_apply)(requested);
        let applied = apply.applied;
        let rebuild_needed = needs_rebuild(&old_config, &applied);
        state.config = applied.clone();
        fill_controls_from_config(state);

        if !apply.hotkey_ok {
            info_box(
                hwnd,
                "ホットキーの再登録に失敗したため、旧設定を維持しました。",
            );
        }

        if rebuild_needed && ask_rebuild(hwnd) {
            set_control_text(hwnd, IDC_STATUS, "インデックス再構築中...");
            if !(state.hooks.on_rebuild)(applied.clone()) {
                set_control_text(hwnd, IDC_STATUS, "再構築開始に失敗しました");
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
        fill_controls_from_config(state);

        if !apply.hotkey_ok {
            info_box(
                hwnd,
                "ホットキーの再登録に失敗したため、旧設定を維持しました。",
            );
        }

        if ask_rebuild(hwnd) {
            set_control_text(hwnd, IDC_STATUS, "インデックス再構築中...");
            if !(state.hooks.on_rebuild)(state.config.clone()) {
                set_control_text(hwnd, IDC_STATUS, "再構築開始に失敗しました");
            }
        }
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

fn to_wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
