use std::cell::RefCell;
use std::rc::Rc;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontIndirectW,
    CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect, InvalidateRect,
    SelectObject, SetBkMode, SetTextColor, DT_END_ELLIPSIS, DT_LEFT, DT_SINGLELINE, FONT_CHARSET,
    HBRUSH, HFONT, LOGFONTW, PAINTSTRUCT, SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

const EDIT_ID: i32 = 100;
const ITEM_HEIGHT: i32 = 36;
const INPUT_HEIGHT: i32 = 40;
const PADDING: i32 = 8;
const ICON_AREA: i32 = 24; // 16px icon + 8px gap

// Colors (BGR format for COLORREF)
const BG_COLOR: u32 = 0x00282828;
const INPUT_BG_COLOR: u32 = 0x00383838;
const TEXT_COLOR: u32 = 0x00E0E0E0;
const SELECTED_BG: u32 = 0x00505050;
const HINT_COLOR: u32 = 0x00808080;

pub struct SearchResult {
    pub name: String,
    pub path: String,
    pub is_folder: bool,
}

pub struct FolderExpansionState {
    pub current_dir: String,
    pub saved_results: Vec<SearchResult>,
    pub saved_selected: usize,
    pub saved_query: String,
}

/// GDI objects and layout values created during window initialization.
/// Returned from `create_search_window` so `main.rs` can build the full `WindowState`.
pub struct WindowStateInit {
    pub edit_hwnd: HWND,
    pub bg_brush: HBRUSH,
    pub input_bg_brush: HBRUSH,
    pub selected_bg_brush: HBRUSH,
    pub edit_font: HFONT,
    pub result_font: HFONT,
    pub item_height: i32,
    pub input_height: i32,
    pub padding: i32,
    pub icon_area: i32,
}

pub struct WindowState {
    pub results: Vec<SearchResult>,
    pub selected: usize,
    pub on_query_changed: Option<Box<dyn Fn(&str) -> Vec<SearchResult>>>,
    pub on_launch: Option<Box<dyn Fn(&SearchResult, &str)>>,
    pub edit_hwnd: HWND,
    pub folder_state: Option<FolderExpansionState>,
    pub on_folder_expand: Option<Box<dyn Fn(&str) -> Vec<SearchResult>>>,
    pub on_folder_navigate: Option<Box<dyn Fn(&str) -> Vec<SearchResult>>>,
    pub on_folder_filter: Option<Box<dyn Fn(&str, &str) -> Vec<SearchResult>>>,
    pub icon_cache: Option<Rc<crate::icon::IconCache>>,
    // Cached GDI objects
    pub bg_brush: HBRUSH,
    pub input_bg_brush: HBRUSH,
    pub selected_bg_brush: HBRUSH,
    pub edit_font: HFONT,
    pub result_font: HFONT,
    // DPI-scaled layout
    pub item_height: i32,
    pub input_height: i32,
    pub padding: i32,
    pub icon_area: i32,
    // Repaint suppression flag
    pub suppress_repaint: bool,
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

pub fn create_search_window(width: u32, max_results: usize) -> Option<(HWND, WindowStateInit)> {
    unsafe {
        let instance = GetModuleHandleW(None).ok()?;
        let class_name = w!("SnotraSearchWindow");

        // DPI scaling
        let dpi = windows::Win32::UI::HiDpi::GetDpiForSystem();
        let scale = |val: i32| -> i32 { val * dpi as i32 / 96 };

        let item_height = scale(ITEM_HEIGHT);
        let input_height = scale(INPUT_HEIGHT);
        let padding = scale(PADDING);
        let icon_area = scale(ICON_AREA);
        let scaled_width = scale(width as i32);

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

        let height = input_height + (item_height * max_results as i32) + padding * 2;

        // Center on primary monitor
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let x = (screen_w - scaled_width) / 2;
        let y = screen_h / 4;

        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class_name,
            w!("Snotra"),
            WS_POPUP,
            x,
            y,
            scaled_width,
            height,
            HWND::default(),
            None,
            instance,
            None,
        ).ok()?;

        // Create Edit control for text input
        let edit_hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("EDIT"),
            w!(""),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
            padding,
            padding,
            scaled_width - padding * 2,
            input_height - padding,
            hwnd,
            HMENU(EDIT_ID as *mut _),
            instance,
            None,
        )
        .ok()?;

        // Create cached GDI objects with DPI-scaled font sizes
        let edit_font = create_font(scale(18));
        let result_font = create_font(scale(15));
        let bg_brush = CreateSolidBrush(COLORREF(BG_COLOR));
        let input_bg_brush = CreateSolidBrush(COLORREF(INPUT_BG_COLOR));
        let selected_bg_brush = CreateSolidBrush(COLORREF(SELECTED_BG));

        // Set font for edit control
        if !edit_font.is_invalid() {
            SendMessageW(edit_hwnd, WM_SETFONT, WPARAM(edit_font.0 as usize), LPARAM(1));
        }

        let init = WindowStateInit {
            edit_hwnd,
            bg_brush,
            input_bg_brush,
            selected_bg_brush,
            edit_font,
            result_font,
            item_height,
            input_height,
            padding,
            icon_area,
        };

        Some((hwnd, init))
    }
}

fn create_font(size: i32) -> HFONT {
    let mut lf = LOGFONTW::default();
    lf.lfHeight = -size;
    lf.lfWeight = 400;
    lf.lfCharSet = FONT_CHARSET(0); // DEFAULT_CHARSET
    let face: Vec<u16> = "Segoe UI".encode_utf16().collect();
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
            state.suppress_repaint = true;
            state.edit_hwnd
        })
        .unwrap_or_default();
        let _ = SetWindowTextW(edit_hwnd, w!(""));
        with_state(|state| { state.suppress_repaint = false; });

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(edit_hwnd);
        let _ = InvalidateRect(hwnd, None, false);
    }
}

pub fn hide_window(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
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
            if active == 0 {
                // WA_INACTIVE - hide when losing focus
                hide_window(hwnd);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_DESTROY => {
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
    let (edit_hwnd, in_folder) = with_state(|state| (state.edit_hwnd, state.folder_state.is_some()))
        .unwrap_or_default();
    let query = get_edit_text(edit_hwnd);

    if in_folder {
        with_state(|state| {
            let current_dir = state
                .folder_state
                .as_ref()
                .map(|fs| fs.current_dir.clone());
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
    let suppress = with_state(|state| state.suppress_repaint).unwrap_or(false);
    if !suppress {
        unsafe {
            let _ = InvalidateRect(hwnd, None, false);
        }
    }
}

fn paint_results(hwnd: HWND) {
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);

        let mut rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut rect);

        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;

        if width <= 0 || height <= 0 {
            let _ = EndPaint(hwnd, &ps);
            return;
        }

        // Double buffering: create off-screen DC
        let mem_dc = CreateCompatibleDC(hdc);
        let mem_bmp = CreateCompatibleBitmap(hdc, width, height);
        let old_bmp = SelectObject(mem_dc, mem_bmp);

        with_state(|state| {
            // Fill background
            FillRect(mem_dc, &rect, state.bg_brush);

            // Fill input area background
            let input_rect = RECT {
                left: state.padding,
                top: state.padding,
                right: rect.right - state.padding,
                bottom: state.input_height,
            };
            FillRect(mem_dc, &input_rect, state.input_bg_brush);

            // Draw results
            let old_font = SelectObject(mem_dc, state.result_font);
            let _ = SetBkMode(mem_dc, TRANSPARENT);

            let has_icons = state.icon_cache.is_some();
            let text_left_offset = if has_icons { state.padding + state.icon_area } else { state.padding };

            for (i, result) in state.results.iter().enumerate() {
                let y = state.input_height + state.padding + (i as i32 * state.item_height);
                let item_rect = RECT {
                    left: state.padding,
                    top: y,
                    right: rect.right - state.padding,
                    bottom: y + state.item_height,
                };

                // Highlight selected
                if i == state.selected {
                    FillRect(mem_dc, &item_rect, state.selected_bg_brush);
                }

                // Draw icon
                if let Some(ref icon_cache) = state.icon_cache {
                    let icon_y = y + (state.item_height - 16) / 2;
                    icon_cache.draw(&result.path, mem_dc, item_rect.left + state.padding, icon_y);
                }

                // Draw name
                SetTextColor(mem_dc, COLORREF(TEXT_COLOR));
                let mut name_wide: Vec<u16> = result.name.encode_utf16().collect();
                let mut text_rect = RECT {
                    left: item_rect.left + text_left_offset,
                    top: y + 2,
                    right: item_rect.right - state.padding,
                    bottom: y + state.item_height / 2 + 4,
                };
                let fmt = DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS;
                DrawTextW(mem_dc, &mut name_wide, &mut text_rect, fmt);

                // Draw path (dimmed)
                SetTextColor(mem_dc, COLORREF(HINT_COLOR));
                let display_path = if result.is_folder {
                    format!("[DIR]  {}", result.path)
                } else {
                    result.path.clone()
                };
                let mut path_wide: Vec<u16> = display_path.encode_utf16().collect();
                let mut path_rect = RECT {
                    left: item_rect.left + text_left_offset,
                    top: y + state.item_height / 2,
                    right: item_rect.right - state.padding,
                    bottom: y + state.item_height - 2,
                };
                DrawTextW(mem_dc, &mut path_wide, &mut path_rect, fmt);
            }

            SelectObject(mem_dc, old_font);
        });

        // Blit to screen
        let _ = BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY);

        // Cleanup off-screen buffer
        SelectObject(mem_dc, old_bmp);
        let _ = DeleteObject(mem_bmp);
        let _ = DeleteDC(mem_dc);

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
                let _ = InvalidateRect(hwnd, None, false);
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
                let _ = InvalidateRect(hwnd, None, false);
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
            with_state(|state| {
                if let Some(result) = state.results.get(state.selected) {
                    if let Some(ref on_launch) = state.on_launch {
                        let query = get_edit_text(state.edit_hwnd);
                        on_launch(result, &query);
                    }
                }
            });
            hide_window(hwnd);
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
        // Suppress the EN_CHANGE repaint from SetWindowTextW
        with_state(|state| { state.suppress_repaint = true; });
        unsafe {
            let _ = SetWindowTextW(edit_hwnd, w!(""));
        }
        with_state(|state| { state.suppress_repaint = false; });
        unsafe {
            let _ = InvalidateRect(hwnd, None, false);
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
        let current = std::path::Path::new(&fs.current_dir);
        let Some(parent) = current.parent() else {
            return; // At drive root
        };
        // On Windows, parent of "C:\" is "C:\" — so compare to detect root
        if parent.to_string_lossy() == fs.current_dir {
            return;
        }
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
        let _ = InvalidateRect(hwnd, None, false);
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
            let _ = InvalidateRect(hwnd, None, false);
        }
        true
    } else {
        false
    }
}
