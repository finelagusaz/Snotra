use std::cell::RefCell;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontIndirectW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect,
    InvalidateRect, SelectObject, SetBkMode, SetTextColor, DT_END_ELLIPSIS,
    DT_LEFT, DT_SINGLELINE, FONT_CHARSET, HBRUSH, LOGFONTW, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

const EDIT_ID: i32 = 100;
const ITEM_HEIGHT: i32 = 36;
const INPUT_HEIGHT: i32 = 40;
const PADDING: i32 = 8;

// Colors (BGR format for COLORREF)
const BG_COLOR: u32 = 0x00282828;
const INPUT_BG_COLOR: u32 = 0x00383838;
const TEXT_COLOR: u32 = 0x00E0E0E0;
const SELECTED_BG: u32 = 0x00505050;
const HINT_COLOR: u32 = 0x00808080;

pub struct SearchResult {
    pub name: String,
    pub path: String,
}

pub struct WindowState {
    pub results: Vec<SearchResult>,
    pub selected: usize,
    pub on_query_changed: Option<Box<dyn Fn(&str) -> Vec<SearchResult>>>,
    pub on_launch: Option<Box<dyn Fn(&SearchResult)>>,
    pub edit_hwnd: HWND,
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

pub fn create_search_window(width: u32, max_results: usize) -> Option<HWND> {
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

        // Center on primary monitor
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let x = (screen_w - width as i32) / 2;
        let y = screen_h / 4;

        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class_name,
            w!("Snotra"),
            WS_POPUP,
            x,
            y,
            width as i32,
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
        let font = create_font(18);
        if !font.is_invalid() {
            SendMessageW(edit_hwnd, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
        }

        set_window_state(WindowState {
            results: Vec::new(),
            selected: 0,
            on_query_changed: None,
            on_launch: None,
            edit_hwnd,
        });

        Some(hwnd)
    }
}

fn create_font(size: i32) -> windows::Win32::Graphics::Gdi::HFONT {
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
            state.edit_hwnd
        })
        .unwrap_or_default();
        let _ = SetWindowTextW(edit_hwnd, w!(""));

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(edit_hwnd);
        let _ = InvalidateRect(hwnd, None, true);
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

fn handle_query_changed(hwnd: HWND) {
    with_state(|state| {
        let len = unsafe { GetWindowTextLengthW(state.edit_hwnd) } as usize;
        if len == 0 {
            state.results.clear();
            state.selected = 0;
        } else {
            let mut buf = vec![0u16; len + 1];
            unsafe { GetWindowTextW(state.edit_hwnd, &mut buf) };
            let query = String::from_utf16_lossy(&buf[..len]);
            if let Some(ref on_query) = state.on_query_changed {
                state.results = on_query(&query);
                state.selected = 0;
            }
        }
    });
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
        let bg_brush = CreateSolidBrush(COLORREF(BG_COLOR));
        FillRect(hdc, &rect, bg_brush);
        let _ = DeleteObject(bg_brush);

        // Fill input area background
        let input_rect = RECT {
            left: PADDING,
            top: PADDING,
            right: rect.right - PADDING,
            bottom: INPUT_HEIGHT,
        };
        let input_brush = CreateSolidBrush(COLORREF(INPUT_BG_COLOR));
        FillRect(hdc, &input_rect, input_brush);
        let _ = DeleteObject(input_brush);

        // Draw results
        let font = create_font(15);
        let old_font = SelectObject(hdc, font);
        let _ = SetBkMode(hdc, TRANSPARENT);

        with_state(|state| {
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
                    let sel_brush = CreateSolidBrush(COLORREF(SELECTED_BG));
                    FillRect(hdc, &item_rect, sel_brush);
                    let _ = DeleteObject(sel_brush);
                }

                // Draw name
                SetTextColor(hdc, COLORREF(TEXT_COLOR));
                let mut name_wide: Vec<u16> = result.name.encode_utf16().collect();
                let mut text_rect = RECT {
                    left: item_rect.left + PADDING,
                    top: y + 2,
                    right: item_rect.right - PADDING,
                    bottom: y + ITEM_HEIGHT / 2 + 4,
                };
                let fmt = DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS;
                DrawTextW(hdc, &mut name_wide, &mut text_rect, fmt);

                // Draw path (dimmed)
                SetTextColor(hdc, COLORREF(HINT_COLOR));
                let mut path_wide: Vec<u16> = result.path.encode_utf16().collect();
                let mut path_rect = RECT {
                    left: item_rect.left + PADDING,
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
            // Escape
            hide_window(hwnd);
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
        0x0D => {
            // Enter - launch selected
            with_state(|state| {
                if let Some(result) = state.results.get(state.selected) {
                    if let Some(ref on_launch) = state.on_launch {
                        on_launch(result);
                    }
                }
            });
            hide_window(hwnd);
            true
        }
        _ => false,
    }
}
