use std::cell::RefCell;
use std::rc::Rc;
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontIndirectW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect,
    InvalidateRect, SelectObject, SetBkMode, SetTextColor, DT_END_ELLIPSIS, DT_LEFT, DT_SINGLELINE,
    FONT_CHARSET, HBRUSH, HFONT, LOGFONTW, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::*;

const EDIT_ID: i32 = 100;
const ITEM_HEIGHT: i32 = 36;
const INPUT_HEIGHT: i32 = 40;
const PADDING: i32 = 8;
const ICON_AREA: i32 = 24; // 16px icon + 8px gap

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowTheme {
    pub bg_color: u32,
    pub input_bg_color: u32,
    pub text_color: u32,
    pub selected_bg_color: u32,
    pub hint_color: u32,
    pub font_family: String,
    pub font_size: i32,
}

impl Default for WindowTheme {
    fn default() -> Self {
        Self {
            bg_color: 0x00282828,
            input_bg_color: 0x00383838,
            text_color: 0x00E0E0E0,
            selected_bg_color: 0x00505050,
            hint_color: 0x00808080,
            font_family: "Segoe UI".to_string(),
            font_size: 15,
        }
    }
}

#[derive(Clone)]
pub struct SearchResult {
    pub name: String,
    pub path: String,
    pub is_folder: bool,
    pub is_error: bool,
}

pub struct FolderExpansionState {
    pub current_dir: String,
    pub saved_results: Vec<SearchResult>,
    pub saved_selected: usize,
    pub saved_query: String,
}

pub struct WindowState {
    pub results: Vec<SearchResult>,
    pub selected: usize,
    pub on_query_changed: Option<Box<dyn Fn(&str) -> Vec<SearchResult>>>,
    pub on_launch: Option<Box<dyn Fn(&SearchResult, &str)>>,
    pub on_command: Option<Box<dyn Fn(&str) -> bool>>,
    pub edit_hwnd: HWND,
    pub folder_state: Option<FolderExpansionState>,
    pub on_folder_expand: Option<Box<dyn Fn(&str) -> Vec<SearchResult>>>,
    pub on_folder_navigate: Option<Box<dyn Fn(&str) -> Vec<SearchResult>>>,
    pub on_folder_filter: Option<Box<dyn Fn(&str, &str) -> Vec<SearchResult>>>,
    pub icon_cache: Option<Rc<crate::icon::IconCache>>,
    pub theme: WindowTheme,
    pub edit_font: Option<HFONT>,
    pub auto_hide_on_focus_lost: bool,
    pub ime_off_on_show: bool,
    pub in_size_move: bool,
}

thread_local! {
    static WINDOW_STATE: RefCell<Option<WindowState>> = const { RefCell::new(None) };
}

pub fn set_window_state(state: WindowState) {
    WINDOW_STATE.with(|s| *s.borrow_mut() = Some(state));
}

fn with_state<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut WindowState) -> R,
{
    WINDOW_STATE.with(|s| s.borrow_mut().as_mut().map(f))
}

pub fn create_search_window(width: u32, max_results: usize, show_title_bar: bool) -> Option<HWND> {
    unsafe {
        let instance = GetModuleHandleW(None).ok()?;
        let class_name = w!("SnotraSearchWindow");

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: HBRUSH(std::ptr::null_mut()),
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let height = INPUT_HEIGHT + (ITEM_HEIGHT * max_results as i32) + PADDING * 2;

        // Restore previous placement if available; otherwise center on primary monitor
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let default_x = (screen_w - width as i32) / 2;
        let default_y = screen_h / 4;
        let (x, y) = crate::window_data::load_search_placement()
            .map(|p| (p.x, p.y))
            .unwrap_or((default_x, default_y));

        let style = if show_title_bar {
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX
        } else {
            WS_POPUP
        };
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class_name,
            w!("Snotra"),
            style,
            x,
            y,
            width as i32,
            height,
            HWND::default(),
            None,
            instance,
            None,
        )
        .ok()?;

        // Create Edit control for text input
        let edit_hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("EDIT"),
            w!(""),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
            PADDING,
            PADDING,
            width as i32 - PADDING * 2,
            INPUT_HEIGHT - PADDING,
            hwnd,
            HMENU(EDIT_ID as *mut _),
            instance,
            None,
        )
        .ok()?;

        // Set font for edit control
        let theme = WindowTheme::default();
        let font = create_font(theme.font_size + 3, &theme.font_family);
        let edit_font = if !font.is_invalid() {
            SendMessageW(edit_hwnd, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
            Some(font)
        } else {
            None
        };

        set_window_state(WindowState {
            results: Vec::new(),
            selected: 0,
            on_query_changed: None,
            on_launch: None,
            on_command: None,
            edit_hwnd,
            folder_state: None,
            on_folder_expand: None,
            on_folder_navigate: None,
            on_folder_filter: None,
            icon_cache: None,
            theme,
            edit_font,
            auto_hide_on_focus_lost: true,
            ime_off_on_show: false,
            in_size_move: false,
        });

        Some(hwnd)
    }
}

fn create_font(size: i32, family: &str) -> windows::Win32::Graphics::Gdi::HFONT {
    let mut lf = LOGFONTW::default();
    lf.lfHeight = -size;
    lf.lfWeight = 400;
    lf.lfCharSet = FONT_CHARSET(0); // DEFAULT_CHARSET
    let face: Vec<u16> = family.encode_utf16().collect();
    let len = face.len().min(lf.lfFaceName.len() - 1);
    lf.lfFaceName[..len].copy_from_slice(&face[..len]);
    unsafe { CreateFontIndirectW(&lf) }
}

pub fn toggle_window(hwnd: HWND) {
    unsafe {
        if IsWindowVisible(hwnd).as_bool() {
            hide_window(hwnd);
        } else {
            show_window(hwnd);
        }
    }
}

pub fn show_window(hwnd: HWND) {
    unsafe {
        // Clear state first, then update the edit control outside the borrow.
        // SetWindowTextW sends EN_CHANGE synchronously and can re-enter our code.
        let edit_hwnd = with_state(|state| {
            state.results.clear();
            state.selected = 0;
            state.folder_state = None;
            state.edit_hwnd
        })
        .unwrap_or_default();
        let ime_off = with_state(|state| state.ime_off_on_show).unwrap_or(false);
        let _ = SetWindowTextW(edit_hwnd, w!(""));

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(edit_hwnd);
        if ime_off {
            crate::ime::turn_off_ime(edit_hwnd);
        }
        let _ = InvalidateRect(hwnd, None, true);
    }
}

pub fn hide_window(hwnd: HWND) {
    unsafe {
        persist_window_placement(hwnd);
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}

pub fn update_icon_cache(icon_cache: Option<Rc<crate::icon::IconCache>>) {
    with_state(|state| {
        state.icon_cache = icon_cache;
    });
}

pub fn set_theme(hwnd: HWND, theme: WindowTheme) {
    let mut old_font = None;
    let edit_hwnd = with_state(|state| {
        state.theme = theme.clone();
        if let Some(font) = state.edit_font {
            old_font = Some(font);
        }
        state.edit_hwnd
    })
    .unwrap_or_default();
    let font = create_font(theme.font_size + 3, &theme.font_family);
    unsafe {
        if !font.is_invalid() {
            let _ = SendMessageW(edit_hwnd, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
            with_state(|state| {
                state.edit_font = Some(font);
            });
            if let Some(old) = old_font {
                if !old.is_invalid() && old != font {
                    let _ = DeleteObject(old);
                }
            }
        }
        let _ = InvalidateRect(hwnd, None, true);
    }
}

pub fn set_auto_hide_on_focus_lost(enabled: bool) {
    with_state(|state| {
        state.auto_hide_on_focus_lost = enabled;
    });
}

pub fn set_ime_off_on_show(enabled: bool) {
    with_state(|state| {
        state.ime_off_on_show = enabled;
    });
}

pub fn set_title_bar_mode(hwnd: HWND, enabled: bool) {
    unsafe {
        let mut style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        if enabled {
            style &= !WS_POPUP.0;
            style |= (WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX).0;
        } else {
            style &= !(WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX).0;
            style |= WS_POPUP.0;
        }
        let _ = SetWindowLongPtrW(hwnd, GWL_STYLE, style as isize);
        let _ = SetWindowPos(
            hwnd,
            HWND::default(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED | SWP_NOACTIVATE,
        );
    }
}

pub fn update_max_results_layout(hwnd: HWND, max_results: usize) {
    unsafe {
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_ok() {
            let width = rect.right - rect.left;
            let height = INPUT_HEIGHT + (ITEM_HEIGHT * max_results as i32) + PADDING * 2;
            let _ = SetWindowPos(
                hwnd,
                HWND::default(),
                rect.left,
                rect.top,
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            let _ = InvalidateRect(hwnd, None, true);
        }
    }
}

fn persist_window_placement(hwnd: HWND) {
    unsafe {
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_ok() {
            crate::window_data::save_search_placement(crate::window_data::WindowPlacement {
                x: rect.left,
                y: rect.top,
            });
        }
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let notification = ((wparam.0 >> 16) & 0xFFFF) as u32;
            let control_id = (wparam.0 & 0xFFFF) as i32;
            if control_id == EDIT_ID && notification == EN_CHANGE {
                handle_query_changed(hwnd);
            }
            LRESULT(0)
        }
        WM_PAINT => {
            paint_results(hwnd);
            LRESULT(0)
        }
        WM_ACTIVATE => {
            let active = (wparam.0 & 0xFFFF) as u32;
            let should_hide = with_state(|state| {
                state.auto_hide_on_focus_lost && !state.in_size_move
            })
            .unwrap_or(true);
            if active == 0 && should_hide {
                // WA_INACTIVE - hide when losing focus
                hide_window(hwnd);
            }
            LRESULT(0)
        }
        WM_ENTERSIZEMOVE => {
            with_state(|state| {
                state.in_size_move = true;
            });
            LRESULT(0)
        }
        WM_EXITSIZEMOVE => {
            with_state(|state| {
                state.in_size_move = false;
            });
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_DESTROY => {
            with_state(|state| {
                if let Some(font) = state.edit_font.take() {
                    if !font.is_invalid() {
                        let _ = DeleteObject(font);
                    }
                }
            });
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn get_edit_text(edit_hwnd: HWND) -> String {
    let len = unsafe { GetWindowTextLengthW(edit_hwnd) } as usize;
    if len > 0 {
        let mut buf = vec![0u16; len + 1];
        unsafe { GetWindowTextW(edit_hwnd, &mut buf) };
        String::from_utf16_lossy(&buf[..len])
    } else {
        String::new()
    }
}

fn set_edit_text(edit_hwnd: HWND, text: &str) {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let _ = SetWindowTextW(edit_hwnd, windows::core::PCWSTR(wide.as_ptr()));
    }
}

fn handle_query_changed(hwnd: HWND) {
    // Read edit text and folder state outside with_state to avoid re-entrancy
    let (edit_hwnd, in_folder) =
        with_state(|state| (state.edit_hwnd, state.folder_state.is_some())).unwrap_or_default();
    let query = get_edit_text(edit_hwnd);

    if in_folder {
        with_state(|state| {
            let current_dir = state.folder_state.as_ref().map(|fs| fs.current_dir.clone());
            if let (Some(dir), Some(ref on_filter)) = (current_dir, &state.on_folder_filter) {
                state.results = on_filter(&dir, &query);
                state.selected = 0;
            }
        });
    } else {
        with_state(|state| {
            if let Some(ref on_query) = state.on_query_changed {
                state.results = on_query(&query);
                state.selected = 0;
            }
        });
    }
    unsafe {
        let _ = InvalidateRect(hwnd, None, true);
    }
}

fn paint_results(hwnd: HWND) {
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);

        let mut rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut rect);

        // Fill background
        let theme = with_state(|state| state.theme.clone()).unwrap_or_default();

        let bg_brush = CreateSolidBrush(COLORREF(theme.bg_color));
        FillRect(hdc, &rect, bg_brush);
        let _ = DeleteObject(bg_brush);

        // Fill input area background
        let input_rect = RECT {
            left: PADDING,
            top: PADDING,
            right: rect.right - PADDING,
            bottom: INPUT_HEIGHT,
        };
        let input_brush = CreateSolidBrush(COLORREF(theme.input_bg_color));
        FillRect(hdc, &input_rect, input_brush);
        let _ = DeleteObject(input_brush);

        // Draw results
        let font = create_font(theme.font_size, &theme.font_family);
        let old_font = SelectObject(hdc, font);
        let _ = SetBkMode(hdc, TRANSPARENT);

        with_state(|state| {
            let has_icons = state.icon_cache.is_some();
            let text_left_offset = if has_icons {
                PADDING + ICON_AREA
            } else {
                PADDING
            };

            for (i, result) in state.results.iter().enumerate() {
                let y = INPUT_HEIGHT + PADDING + (i as i32 * ITEM_HEIGHT);
                let item_rect = RECT {
                    left: PADDING,
                    top: y,
                    right: rect.right - PADDING,
                    bottom: y + ITEM_HEIGHT,
                };

                // Highlight selected
                if i == state.selected {
                    let sel_brush = CreateSolidBrush(COLORREF(theme.selected_bg_color));
                    FillRect(hdc, &item_rect, sel_brush);
                    let _ = DeleteObject(sel_brush);
                }

                // Draw icon
                if let Some(ref icon_cache) = state.icon_cache {
                    let icon_y = y + (ITEM_HEIGHT - 16) / 2;
                    icon_cache.draw(&result.path, hdc, item_rect.left + PADDING, icon_y);
                }

                // Draw name
                SetTextColor(hdc, COLORREF(theme.text_color));
                let mut name_wide: Vec<u16> = result.name.encode_utf16().collect();
                let mut text_rect = RECT {
                    left: item_rect.left + text_left_offset,
                    top: y + 2,
                    right: item_rect.right - PADDING,
                    bottom: y + ITEM_HEIGHT / 2 + 4,
                };
                let fmt = DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS;
                DrawTextW(hdc, &mut name_wide, &mut text_rect, fmt);

                // Draw path (dimmed)
                SetTextColor(hdc, COLORREF(theme.hint_color));
                let display_path = if result.is_folder {
                    format!("[DIR]  {}", result.path)
                } else {
                    result.path.clone()
                };
                let mut path_wide: Vec<u16> = display_path.encode_utf16().collect();
                let mut path_rect = RECT {
                    left: item_rect.left + text_left_offset,
                    top: y + ITEM_HEIGHT / 2,
                    right: item_rect.right - PADDING,
                    bottom: y + ITEM_HEIGHT - 2,
                };
                DrawTextW(hdc, &mut path_wide, &mut path_rect, fmt);
            }
        });

        SelectObject(hdc, old_font);
        let _ = DeleteObject(font);
        let _ = EndPaint(hwnd, &ps);
    }
}

/// Process keyboard input from the edit control (called from message loop)
pub fn handle_edit_keydown(hwnd: HWND, vk: u32) -> bool {
    match vk {
        0x1B => {
            // Escape — exit folder mode first, hide only if not in folder mode
            let exited = exit_folder_expansion(hwnd);
            if !exited {
                hide_window(hwnd);
            }
            true
        }
        0x26 => {
            // Up arrow
            with_state(|state| {
                if state.selected > 0 {
                    state.selected -= 1;
                }
            });
            unsafe {
                let _ = InvalidateRect(hwnd, None, true);
            }
            true
        }
        0x28 => {
            // Down arrow
            with_state(|state| {
                if !state.results.is_empty() && state.selected < state.results.len() - 1 {
                    state.selected += 1;
                }
            });
            unsafe {
                let _ = InvalidateRect(hwnd, None, true);
            }
            true
        }
        0x27 => {
            // Right arrow — expand folder if selected item is a folder
            let info = with_state(|state| {
                state
                    .results
                    .get(state.selected)
                    .map(|r| (r.is_folder, r.path.clone()))
            })
            .flatten();

            if let Some((true, folder_path)) = info {
                enter_folder_expansion(hwnd, &folder_path);
                true
            } else {
                false // Let edit control handle cursor movement
            }
        }
        0x25 => {
            // Left arrow — navigate to parent folder if in folder mode
            let in_folder = with_state(|state| state.folder_state.is_some()).unwrap_or(false);
            if in_folder {
                navigate_folder_up(hwnd);
                true
            } else {
                false // Let edit control handle cursor movement
            }
        }
        0x0D => {
            // Enter - launch selected
            let (query, mut on_command) = with_state(|state| {
                let q = get_edit_text(state.edit_hwnd);
                let cmd = state.on_command.take();
                (q, cmd)
            })
            .unwrap_or((String::new(), None));
            let command_handled = on_command
                .as_ref()
                .map(|on_command| on_command(&query))
                .unwrap_or(false);
            with_state(|state| {
                if state.on_command.is_none() {
                    state.on_command = on_command.take();
                }
            });
            if command_handled {
                hide_window(hwnd);
                return true;
            }

            let (selected, mut on_launch) = with_state(|state| {
                let selected = state.results.get(state.selected).cloned();
                let on_launch = state.on_launch.take();
                (selected, on_launch)
            })
            .unwrap_or((None, None));
            let should_hide = selected.as_ref().map(|r| !r.is_error).unwrap_or(false);
            if let (Some(result), Some(on_launch)) = (selected.as_ref(), on_launch.as_ref()) {
                if !result.is_error {
                    on_launch(result, &query);
                }
            }
            with_state(|state| {
                if state.on_launch.is_none() {
                    state.on_launch = on_launch.take();
                }
            });
            if should_hide {
                hide_window(hwnd);
            }
            true
        }
        _ => false,
    }
}

fn enter_folder_expansion(hwnd: HWND, folder_path: &str) {
    // Read current query and extract edit_hwnd before mutating state
    let edit_hwnd = with_state(|state| state.edit_hwnd).unwrap_or_default();
    let current_query = get_edit_text(edit_hwnd);

    // Save current state and expand folder
    let expanded = with_state(|state| {
        if let Some(ref on_expand) = state.on_folder_expand {
            let new_results = on_expand(folder_path);
            if let Some(ref mut fs) = state.folder_state {
                // Already in folder mode — just update current_dir, keep original snapshot
                fs.current_dir = folder_path.to_string();
            } else {
                // First entry — save current search state
                state.folder_state = Some(FolderExpansionState {
                    current_dir: folder_path.to_string(),
                    saved_results: std::mem::take(&mut state.results),
                    saved_selected: state.selected,
                    saved_query: current_query,
                });
            }
            state.results = new_results;
            state.selected = 0;
            true
        } else {
            false
        }
    })
    .unwrap_or(false);

    if expanded {
        // Clear edit text — EN_CHANGE fires here but folder_state is already set
        unsafe {
            let _ = SetWindowTextW(edit_hwnd, w!(""));
            let _ = InvalidateRect(hwnd, None, true);
        }
    }
}

fn navigate_folder_up(hwnd: HWND) {
    let edit_hwnd = with_state(|state| state.edit_hwnd).unwrap_or_default();
    let current_filter = get_edit_text(edit_hwnd);

    with_state(|state| {
        let Some(ref mut fs) = state.folder_state else {
            return;
        };
        let Some(parent) = crate::folder::parent_for_navigation(&fs.current_dir) else {
            return; // At drive root
        };
        let parent_str = parent.to_string_lossy().to_string();
        fs.current_dir = parent_str.clone();
        if let Some(ref on_navigate) = state.on_folder_navigate {
            state.results = on_navigate(&parent_str);
            state.selected = 0;
        }
    });

    // If there was filter text, clear it for the new folder
    if !current_filter.is_empty() {
        unsafe {
            let _ = SetWindowTextW(edit_hwnd, w!(""));
        }
    }
    unsafe {
        let _ = InvalidateRect(hwnd, None, true);
    }
}

fn exit_folder_expansion(hwnd: HWND) -> bool {
    let saved = with_state(|state| {
        state.folder_state.take().map(|fs| {
            state.results = fs.saved_results;
            state.selected = fs.saved_selected;
            (fs.saved_query, state.edit_hwnd)
        })
    })
    .flatten();

    if let Some((query, edit_hwnd)) = saved {
        // Restore query text — folder_state is already None so EN_CHANGE runs normal search
        set_edit_text(edit_hwnd, &query);
        unsafe {
            let _ = InvalidateRect(hwnd, None, true);
        }
        true
    } else {
        false
    }
}
