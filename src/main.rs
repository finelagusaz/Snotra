#![windows_subsystem = "windows"]

mod binfmt;
mod config;
mod folder;
mod history;
mod hotkey;
mod icon;
mod indexer;
mod launcher;
mod query;
mod search;
mod settings;
mod tray;
mod window;
mod window_data;

use std::cell::RefCell;
use std::rc::Rc;

use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;

use config::{Config, HotkeyConfig, SearchModeConfig};
use search::{SearchEngine, SearchMode};
use tray::{handle_tray_message, IDM_EXIT, IDM_SETTINGS, WM_TRAY_ICON};

const WM_REBUILD_DONE: u32 = WM_APP + 2;
const WM_REBUILD_FAILED: u32 = WM_APP + 3;

#[derive(Clone, Copy)]
struct RuntimeSettings {
    max_results: usize,
    max_history_display: usize,
    normal_mode: SearchMode,
    folder_mode: SearchMode,
    show_hidden_system: bool,
}

fn main() {
    if is_already_running() {
        return;
    }

    let mut config = Config::load();
    config.appearance.max_history_display = config
        .appearance
        .max_history_display
        .min(config.appearance.max_results);
    let config_state = Rc::new(RefCell::new(config.clone()));

    let (entries, rescanned) = indexer::load_or_scan(
        &config.paths.additional,
        &config.paths.scan,
        config.search.show_hidden_system,
    );

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
    let icon_cache_state = Rc::new(RefCell::new(icon_cache.clone()));

    let engine = Rc::new(RefCell::new(SearchEngine::new(entries)));
    let history = Rc::new(RefCell::new(history::HistoryStore::load(
        config.appearance.top_n_history,
        config.appearance.max_history_display,
    )));
    let runtime = Rc::new(RefCell::new(RuntimeSettings {
        max_results: config.appearance.max_results,
        max_history_display: config.appearance.max_history_display,
        normal_mode: to_search_mode(config.search.normal_mode),
        folder_mode: to_search_mode(config.search.folder_mode),
        show_hidden_system: config.search.show_hidden_system,
    }));

    let search_hwnd = window::create_search_window(
        config.appearance.window_width,
        config.appearance.max_results,
    );
    let Some(search_hwnd) = search_hwnd else {
        return;
    };

    let msg_hwnd = create_message_window();
    let Some(msg_hwnd) = msg_hwnd else {
        return;
    };

    let tray = tray::Tray::create(msg_hwnd);

    let open_settings_action: Rc<dyn Fn()> = {
        let config_state = config_state.clone();
        let runtime = runtime.clone();
        let engine = engine.clone();
        let history = history.clone();
        let icon_cache_state = icon_cache_state.clone();
        let msg_hwnd_for_rebuild = msg_hwnd;
        Rc::new(move || {
            let current_config = config_state.borrow().clone();

            let on_apply = {
                let config_state = config_state.clone();
                let runtime = runtime.clone();
                let engine = engine.clone();
                let history = history.clone();
                let icon_cache_state = icon_cache_state.clone();
                move |mut next: Config| -> settings::ApplyResult {
                    next.appearance.max_history_display = next
                        .appearance
                        .max_history_display
                        .min(next.appearance.max_results);

                    let old = config_state.borrow().clone();
                    let mut hotkey_ok = true;

                    if old.hotkey != next.hotkey {
                        hotkey::unregister();
                        if !hotkey::register(&next.hotkey) {
                            hotkey_ok = false;
                            let _ = hotkey::register(&old.hotkey);
                            next.hotkey = old.hotkey.clone();
                        }
                    }

                    next.save();
                    *config_state.borrow_mut() = next.clone();

                    {
                        let mut rt = runtime.borrow_mut();
                        rt.max_results = next.appearance.max_results;
                        rt.max_history_display = next.appearance.max_history_display;
                        rt.normal_mode = to_search_mode(next.search.normal_mode);
                        rt.folder_mode = to_search_mode(next.search.folder_mode);
                        rt.show_hidden_system = next.search.show_hidden_system;
                    }

                    *history.borrow_mut() = history::HistoryStore::load(
                        next.appearance.top_n_history,
                        next.appearance.max_history_display,
                    );

                    window::update_max_results_layout(search_hwnd, next.appearance.max_results);

                    if next.appearance.show_icons {
                        let cache = icon::IconCache::load().unwrap_or_else(|| {
                            let entries = engine.borrow().entries().to_vec();
                            let cache = icon::IconCache::build(&entries);
                            cache.save();
                            cache
                        });
                        let cache = Rc::new(cache);
                        *icon_cache_state.borrow_mut() = Some(cache.clone());
                        window::update_icon_cache(Some(cache));
                    } else {
                        *icon_cache_state.borrow_mut() = None;
                        window::update_icon_cache(None);
                    }

                    settings::ApplyResult {
                        applied: next,
                        hotkey_ok,
                    }
                }
            };

            let on_rebuild = move |cfg: Config| -> bool {
                let additional = cfg.paths.additional.clone();
                let scan = cfg.paths.scan.clone();
                let show_hidden = cfg.search.show_hidden_system;
                let show_icons = cfg.appearance.show_icons;
                let target_hwnd = msg_hwnd_for_rebuild.0 as isize;

                std::thread::Builder::new()
                    .name("snotra-manual-rebuild".to_string())
                    .spawn(move || {
                        let entries = indexer::rebuild_and_save(&additional, &scan, show_hidden);
                        let hwnd = HWND(target_hwnd as *mut core::ffi::c_void);
                        let ptr = Box::into_raw(Box::new(entries));
                        unsafe {
                            if PostMessageW(
                                hwnd,
                                WM_REBUILD_DONE,
                                WPARAM(if show_icons { 1 } else { 0 }),
                                LPARAM(ptr as isize),
                            )
                            .is_err()
                            {
                                let _ = Box::from_raw(ptr);
                                let _ = PostMessageW(
                                    hwnd,
                                    WM_REBUILD_FAILED,
                                    WPARAM(0),
                                    LPARAM(0),
                                );
                            }
                        }
                    })
                    .is_ok()
            };

            settings::open_or_focus(
                current_config,
                settings::SettingsHooks {
                    on_apply: Box::new(on_apply),
                    on_rebuild: Box::new(on_rebuild),
                },
            );
        })
    };

    let engine_for_search = engine.clone();
    let history_for_search = history.clone();
    let history_for_launch = history.clone();
    let history_for_expand = history.clone();
    let history_for_navigate = history.clone();
    let history_for_filter = history.clone();
    let runtime_for_search = runtime.clone();
    let runtime_for_folder_expand = runtime.clone();
    let runtime_for_folder_nav = runtime.clone();
    let runtime_for_folder_filter = runtime.clone();
    let open_settings_for_command = open_settings_action.clone();

    window::set_window_state(window::WindowState {
        results: Vec::new(),
        selected: 0,
        on_query_changed: Some(Box::new(move |query| {
            let rt = *runtime_for_search.borrow();
            let hist = history_for_search.borrow();
            if query.is_empty() {
                engine_for_search
                    .borrow()
                    .recent_history(&hist, rt.max_history_display)
            } else {
                engine_for_search
                    .borrow()
                    .search(query, rt.max_results, &hist, rt.normal_mode)
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
        on_command: Some(Box::new(move |query| {
            if crate::query::normalize_query(query) == "/o" {
                open_settings_for_command();
                true
            } else {
                false
            }
        })),
        edit_hwnd: get_edit_hwnd(search_hwnd),
        folder_state: None,
        on_folder_expand: Some(Box::new(move |folder_path| {
            history_for_expand
                .borrow_mut()
                .record_folder_expansion(folder_path);
            let rt = *runtime_for_folder_expand.borrow();
            let hist = history_for_expand.borrow();
            folder::list_folder(
                std::path::Path::new(folder_path),
                "",
                rt.folder_mode,
                rt.show_hidden_system,
                &hist,
                rt.max_results,
            )
        })),
        on_folder_navigate: Some(Box::new(move |folder_path| {
            let rt = *runtime_for_folder_nav.borrow();
            let hist = history_for_navigate.borrow();
            folder::list_folder(
                std::path::Path::new(folder_path),
                "",
                rt.folder_mode,
                rt.show_hidden_system,
                &hist,
                rt.max_results,
            )
        })),
        on_folder_filter: Some(Box::new(move |folder_path, query| {
            let rt = *runtime_for_folder_filter.borrow();
            let hist = history_for_filter.borrow();
            folder::list_folder(
                std::path::Path::new(folder_path),
                query,
                rt.folder_mode,
                rt.show_hidden_system,
                &hist,
                rt.max_results,
            )
        })),
        icon_cache,
    });

    if !hotkey::register(&config.hotkey) {
        let fallback = HotkeyConfig {
            modifier: "Alt".to_string(),
            key: "Q".to_string(),
        };
        if hotkey::register(&fallback) {
            config.hotkey = fallback.clone();
            config.save();
            config_state.borrow_mut().hotkey = fallback;
        }
        window::show_window(search_hwnd);
    }

    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_HOTKEY {
                window::toggle_window(search_hwnd);
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
                continue;
            }

            if msg.hwnd == msg_hwnd && msg.message == WM_TRAY_ICON {
                handle_tray_message(&tray, msg.lParam, search_hwnd);
                continue;
            }

            if msg.hwnd == msg_hwnd && msg.message == WM_REBUILD_DONE {
                let ptr = msg.lParam.0 as *mut Vec<indexer::AppEntry>;
                if !ptr.is_null() {
                    let entries = *Box::from_raw(ptr);
                    *engine.borrow_mut() = SearchEngine::new(entries.clone());

                    if msg.wParam.0 != 0 {
                        let cache = icon::IconCache::build(&entries);
                        cache.save();
                        let cache = Rc::new(cache);
                        *icon_cache_state.borrow_mut() = Some(cache.clone());
                        window::update_icon_cache(Some(cache));
                    } else {
                        *icon_cache_state.borrow_mut() = None;
                        window::update_icon_cache(None);
                    }
                    settings::set_status_text("インデックス再構築が完了しました");
                } else {
                    settings::set_status_text("インデックス再構築に失敗しました");
                }
                continue;
            }

            if msg.hwnd == msg_hwnd && msg.message == WM_REBUILD_FAILED {
                settings::set_status_text("インデックス再構築に失敗しました");
                continue;
            }

            if msg.hwnd == msg_hwnd && msg.message == WM_COMMAND {
                let id = (msg.wParam.0 & 0xFFFF) as u16;
                if id == IDM_SETTINGS {
                    open_settings_action();
                    continue;
                }
                if id == IDM_EXIT {
                    break;
                }
            }

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

fn to_search_mode(mode: SearchModeConfig) -> SearchMode {
    match mode {
        SearchModeConfig::Prefix => SearchMode::Prefix,
        SearchModeConfig::Substring => SearchMode::Substring,
        SearchModeConfig::Fuzzy => SearchMode::Fuzzy,
    }
}

unsafe extern "system" fn msg_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}
