#![windows_subsystem = "windows"]

mod config;
mod folder;
mod history;
mod hotkey;
mod icon;
mod indexer;
mod launcher;
mod search;
mod tray;
mod window;

use std::cell::RefCell;
use std::rc::Rc;

use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

use search::SearchEngine;
use tray::{handle_tray_message, IDM_EXIT, WM_TRAY_ICON};

fn main() {
    // Prevent duplicate instances
    if is_already_running() {
        return;
    }

    let mut config = config::Config::load();
    config.appearance.max_history_display =
        config.appearance.max_history_display.min(config.appearance.max_results);

    // Index applications
    let (entries, rescanned) = indexer::load_or_scan(&config.paths.additional, &config.paths.scan);

    // Build or load icon cache
    let icon_cache = if config.appearance.show_icons {
        if rescanned {
            let cache = icon::IconCache::build(&entries);
            cache.save();
            Some(Rc::new(cache))
        } else {
            Some(Rc::new(icon::IconCache::load().unwrap_or_else(|| {
                let cache = icon::IconCache::build(&entries);
                cache.save();
                cache
            })))
        }
    } else {
        None
    };

    let engine = Rc::new(SearchEngine::new(entries));
    let max_results = config.appearance.max_results;
    let max_history_display = config.appearance.max_history_display;

    // Load history
    let history = Rc::new(RefCell::new(history::HistoryStore::load(
        config.appearance.top_n_history,
        config.appearance.max_history_display,
    )));

    // Create search window
    let search_hwnd = window::create_search_window(
        config.appearance.window_width,
        config.appearance.max_results,
    );
    let Some(search_hwnd) = search_hwnd else {
        return;
    };

    // Set up callbacks
    let engine_for_search = engine.clone();
    let history_for_search = history.clone();
    let history_for_launch = history.clone();
    let history_for_expand = history.clone();
    let history_for_navigate = history.clone();
    let history_for_filter = history.clone();
    window::set_window_state(window::WindowState {
        results: Vec::new(),
        selected: 0,
        on_query_changed: Some(Box::new(move |query| {
            let hist = history_for_search.borrow();
            if query.is_empty() {
                engine_for_search.recent_history(&hist, max_history_display)
            } else {
                engine_for_search.search(query, max_results, &hist)
            }
        })),
        on_launch: Some(Box::new(move |result, query| {
            launcher::launch(&result.path);
            if !result.is_folder {
                history_for_launch
                    .borrow_mut()
                    .record_launch(&result.path, query);
            }
        })),
        edit_hwnd: get_edit_hwnd(search_hwnd),
        folder_state: None,
        on_folder_expand: Some(Box::new(move |folder_path| {
            history_for_expand
                .borrow_mut()
                .record_folder_expansion(folder_path);
            let hist = history_for_expand.borrow();
            folder::list_folder(
                std::path::Path::new(folder_path),
                "",
                &hist,
                max_results,
            )
        })),
        on_folder_navigate: Some(Box::new(move |folder_path| {
            let hist = history_for_navigate.borrow();
            folder::list_folder(
                std::path::Path::new(folder_path),
                "",
                &hist,
                max_results,
            )
        })),
        on_folder_filter: Some(Box::new(move |folder_path, query| {
            let hist = history_for_filter.borrow();
            folder::list_folder(
                std::path::Path::new(folder_path),
                query,
                &hist,
                max_results,
            )
        })),
        icon_cache,
    });

    // Create hidden message-only window for tray
    let msg_hwnd = create_message_window();
    let Some(msg_hwnd) = msg_hwnd else {
        return;
    };

    // System tray
    let tray = tray::Tray::create(msg_hwnd);

    // Register hotkey. Keep running even when registration fails.
    // Some combinations (e.g. Alt+Space) are reserved by the OS.
    if !hotkey::register(&config.hotkey) {
        let fallback = config::HotkeyConfig {
            modifier: "Alt".to_string(),
            key: "Q".to_string(),
        };
        if hotkey::register(&fallback) {
            config.hotkey = fallback;
            config.save();
        }
        window::show_window(search_hwnd);
    }

    // Message loop
    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_HOTKEY {
                window::toggle_window(search_hwnd);
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
                continue;
            }

            // Handle tray messages
            if msg.hwnd == msg_hwnd && msg.message == WM_TRAY_ICON {
                handle_tray_message(&tray, msg.lParam, search_hwnd);
                continue;
            }

            // Handle tray menu commands
            if msg.hwnd == msg_hwnd && msg.message == WM_COMMAND {
                let id = (msg.wParam.0 & 0xFFFF) as u16;
                if id == IDM_EXIT {
                    break;
                }
            }

            // Intercept keydown in edit control for arrow keys, Enter, Escape
            if msg.message == WM_KEYDOWN {
                if window::handle_edit_keydown(search_hwnd, msg.wParam.0 as u32) {
                    continue;
                }
            }

            if !IsDialogMessageW(search_hwnd, &msg).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    hotkey::unregister();
    drop(tray);
}

fn is_already_running() -> bool {
    unsafe { FindWindowW(w!("SnotraMessageWindow"), None).is_ok() }
}

fn get_edit_hwnd(parent: HWND) -> HWND {
    unsafe { GetDlgItem(parent, 100).unwrap_or_default() }
}

fn create_message_window() -> Option<HWND> {
    unsafe {
        let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None).ok()?;
        let class_name = w!("SnotraMessageWindow");

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(msg_wnd_proc),
            hInstance: instance.into(),
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassExW(&wc);

        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!(""),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            None,
            instance,
            None,
        )
        .ok()
    }
}

unsafe extern "system" fn msg_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}
