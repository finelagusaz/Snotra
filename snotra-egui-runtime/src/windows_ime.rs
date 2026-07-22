use std::{
    cell::Cell,
    ffi::c_void,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::mpsc::{self, Receiver, Sender},
};

use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::InvalidateRect,
    UI::{
        Input::Ime::{
            CANDIDATEFORM, CFS_EXCLUDE, CFS_POINT, COMPOSITIONFORM, CPS_CANCEL, GCS_COMPATTR,
            GCS_COMPSTR, GCS_CURSORPOS, GCS_RESULTSTR, HIMC, ImmGetCompositionStringW,
            ImmGetContext, ImmNotifyIME, ImmReleaseContext, ImmSetCandidateWindow,
            ImmSetCompositionWindow, NI_COMPOSITIONSTR,
        },
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION, WM_IME_STARTCOMPOSITION,
        },
    },
};

use super::{active_range_chars, logical_ime_rect_to_physical};
use crate::RuntimeError;

const IME_SUBCLASS_ID: usize = 0x534E_4F54_5241_494D;

struct CallbackState {
    sender: Sender<egui::ImeEvent>,
}

pub(crate) struct PlatformIme {
    // windows::HWND intentionally is not Send. The runtime plugin is required
    // to be Send even though Tauri invokes it on the main thread, so retain the
    // stable handle value and reconstruct the typed wrapper only at API calls.
    hwnd_value: isize,
    receiver: Receiver<egui::ImeEvent>,
    // SetWindowSubclass stores this allocation's address in dwRefData. It must
    // outlive the subclass and is released only after RemoveWindowSubclass.
    _callback_state: Box<CallbackState>,
    last_candidate_rect: Cell<Option<([i32; 2], [i32; 4])>>,
}

impl PlatformIme {
    pub(crate) fn new(window: &tauri::Window) -> Result<Self, RuntimeError> {
        let hwnd = window.hwnd()?;
        let (sender, receiver) = mpsc::channel();
        let callback_state = Box::new(CallbackState { sender });
        let callback_ptr = (&*callback_state) as *const CallbackState as usize;

        // SAFETY: hwnd is owned by the attached Tauri Window. callback_ptr stays
        // valid until Drop removes this exact callback/id pair.
        let installed = unsafe {
            SetWindowSubclass(hwnd, Some(ime_subclass_proc), IME_SUBCLASS_ID, callback_ptr)
        };
        if !installed.as_bool() {
            return Err(RuntimeError::ImeInitialization(
                "SetWindowSubclass returned FALSE".to_owned(),
            ));
        }

        Ok(Self {
            hwnd_value: hwnd.0 as isize,
            receiver,
            _callback_state: callback_state,
            last_candidate_rect: Cell::new(None),
        })
    }

    pub(crate) fn drain(&self) -> Vec<egui::ImeEvent> {
        self.receiver.try_iter().collect()
    }

    pub(crate) fn update(&self, output: Option<egui::output::IMEOutput>, scale_factor: f32) {
        let Some(output) = output else {
            return;
        };
        let hwnd = HWND(self.hwnd_value as *mut c_void);
        let Some(context) = ImeContext::acquire(hwnd) else {
            return;
        };

        if output.should_interrupt_composition {
            // SAFETY: context is the active HIMC acquired for this live hwnd.
            unsafe {
                let _ = ImmNotifyIME(context.himc, NI_COMPOSITIONSTR, CPS_CANCEL, 0);
            }
        }

        let (spot, area) = logical_ime_rect_to_physical(output.cursor_rect, scale_factor);
        if std::env::var_os("SNOTRA_EGUI_IME_TRACE").is_some()
            && self.last_candidate_rect.replace(Some((spot, area))) != Some((spot, area))
        {
            eprintln!(
                "SNOTRA_EGUI_IME_RECT scale_factor={scale_factor:.3} spot={},{} area={},{},{},{}",
                spot[0], spot[1], area[0], area[1], area[2], area[3]
            );
        }
        let rect = RECT {
            left: area[0],
            top: area[1],
            right: area[2],
            bottom: area[3],
        };
        let composition = COMPOSITIONFORM {
            dwStyle: CFS_POINT,
            ptCurrentPos: POINT {
                x: spot[0],
                y: spot[1],
            },
            rcArea: rect,
        };
        let candidate = CANDIDATEFORM {
            dwIndex: 0,
            dwStyle: CFS_EXCLUDE,
            ptCurrentPos: POINT {
                x: spot[0],
                y: spot[1],
            },
            rcArea: rect,
        };

        // SAFETY: both forms are initialized physical client coordinates and
        // context is released by ImeContext::drop after these synchronous calls.
        unsafe {
            let _ = ImmSetCompositionWindow(context.himc, &composition);
            let _ = ImmSetCandidateWindow(context.himc, &candidate);
        }
    }
}

impl Drop for PlatformIme {
    fn drop(&mut self) {
        // SAFETY: this mirrors the successful SetWindowSubclass call in new.
        // A destroyed hwnd simply makes the API return FALSE; no callback can
        // run after the native window has ceased to exist.
        unsafe {
            let hwnd = HWND(self.hwnd_value as *mut c_void);
            let _ = RemoveWindowSubclass(hwnd, Some(ime_subclass_proc), IME_SUBCLASS_ID);
        }
    }
}

/// IME サブクラスメッセージのネイティブ描画方針。egui が preedit を自前描画するため
/// 既定の変換文字列ウィンドウを抑制しつつ、確定（GCS_RESULTSTR）は Tao の
/// ReceivedImeText 経路へ通す（#532 の二重表示＝ネイティブ窓 非抑制 の修正）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ImeAction {
    /// DefSubclassProc を呼ばず `LRESULT(0)`。既定変換窓を作らせない／描かせない。
    Suppress,
    /// そのまま Tao へ通す（DefSubclassProc）。確定文字・キー等は Tao が担う。
    PassThrough,
}

/// ネイティブ描画を抑制するか Tao へ通すかだけを純粋に判定する（preedit の抽出可否は
/// 呼び出し側が message で決める）。確定を落とさないため GCS_RESULTSTR を最優先で通す。
fn classify_ime_message(message: u32, lparam: u32) -> ImeAction {
    if message == WM_IME_STARTCOMPOSITION {
        // 既定の変換文字列ウィンドウを作らせない（egui が preedit を描く）。
        ImeAction::Suppress
    } else if message == WM_IME_COMPOSITION {
        if lparam & GCS_RESULTSTR.0 != 0 {
            // 確定は Tao の ReceivedImeText 確定経路が要るため通す。
            ImeAction::PassThrough
        } else {
            // 未確定のみ（GCS_COMPSTR 等）は egui が描くのでネイティブ描画を抑制。
            ImeAction::Suppress
        }
    } else {
        // ENDCOMPOSITION・キー等は Tao へ通す（後始末・確定・入力）。
        ImeAction::PassThrough
    }
}

unsafe extern "system" fn ime_subclass_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    ref_data: usize,
) -> LRESULT {
    let action = classify_ime_message(message, lparam.0 as u32);
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: ref_data was installed from a live Box<CallbackState>, and
        // Drop removes the subclass before releasing that Box.
        let Some(state) = (unsafe { (ref_data as *const CallbackState).as_ref() }) else {
            return;
        };

        let event = match message {
            WM_IME_COMPOSITION => read_preedit(hwnd),
            WM_IME_ENDCOMPOSITION => Some(egui::ImeEvent::Preedit {
                text: String::new(),
                active_range_chars: None,
            }),
            _ => None,
        };
        if let Some(event) = event {
            if std::env::var_os("SNOTRA_EGUI_IME_TRACE").is_some()
                && let egui::ImeEvent::Preedit {
                    text,
                    active_range_chars,
                } = &event
            {
                eprintln!(
                    "SNOTRA_EGUI_IME_PREEDIT chars={} active={active_range_chars:?}",
                    text.chars().count()
                );
            }
            let _ = state.sender.send(event);
            // Tao does not expose preedit as WindowEvent. Invalidating the
            // client area produces a RedrawRequested where the queue is drained.
            // SAFETY: hwnd is the window currently dispatching this callback.
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
        }
    }));

    // egui が preedit を自前描画するため、未確定のネイティブ変換窓は描かせない（#532 二重表示）。
    match action {
        ImeAction::Suppress => LRESULT(0),
        // SAFETY: every observed message must continue through Tao's own subclass,
        // which remains responsible for key events and committed ReceivedImeText.
        ImeAction::PassThrough => unsafe { DefSubclassProc(hwnd, message, wparam, lparam) },
    }
}

fn read_preedit(hwnd: HWND) -> Option<egui::ImeEvent> {
    let context = ImeContext::acquire(hwnd)?;
    let text = context.composition_string()?;
    let attributes = context.composition_data(GCS_COMPATTR).unwrap_or_default();
    let cursor = context.composition_cursor();
    Some(egui::ImeEvent::Preedit {
        active_range_chars: active_range_chars(&text, &attributes, cursor),
        text,
    })
}

struct ImeContext {
    hwnd: HWND,
    himc: HIMC,
}

impl ImeContext {
    fn acquire(hwnd: HWND) -> Option<Self> {
        // SAFETY: hwnd belongs to the currently attached live Tauri Window.
        let himc = unsafe { ImmGetContext(hwnd) };
        (!himc.0.is_null()).then_some(Self { hwnd, himc })
    }

    fn composition_string(&self) -> Option<String> {
        let bytes = self.composition_data(GCS_COMPSTR)?;
        if bytes.len() % 2 != 0 {
            return None;
        }
        let utf16 = bytes
            .chunks_exact(2)
            .map(|unit| u16::from_ne_bytes([unit[0], unit[1]]))
            .collect::<Vec<_>>();
        String::from_utf16(&utf16).ok()
    }

    fn composition_cursor(&self) -> Option<usize> {
        // GCS_CURSORPOS returns a UTF-16 code-unit offset directly rather than
        // a byte count, so no buffer is supplied.
        // SAFETY: himc is valid for the lifetime of this guard.
        let cursor = unsafe { ImmGetCompositionStringW(self.himc, GCS_CURSORPOS, None, 0) };
        (cursor >= 0).then_some(cursor as usize)
    }

    fn composition_data(
        &self,
        kind: windows::Win32::UI::Input::Ime::IME_COMPOSITION_STRING,
    ) -> Option<Vec<u8>> {
        // SAFETY: size query does not dereference a buffer.
        let byte_len = unsafe { ImmGetCompositionStringW(self.himc, kind, None, 0) };
        if byte_len < 0 {
            return None;
        }
        let mut bytes = vec![0_u8; byte_len as usize];
        if bytes.is_empty() {
            return Some(bytes);
        }
        // SAFETY: bytes has byte_len writable bytes, exactly as requested by IMM32.
        let copied = unsafe {
            ImmGetCompositionStringW(
                self.himc,
                kind,
                Some(bytes.as_mut_ptr().cast::<c_void>()),
                byte_len as u32,
            )
        };
        if copied < 0 {
            return None;
        }
        bytes.truncate(copied as usize);
        Some(bytes)
    }
}

impl Drop for ImeContext {
    fn drop(&mut self) {
        // SAFETY: acquire obtained this HIMC for this hwnd; this is the paired release.
        unsafe {
            let _ = ImmReleaseContext(self.hwnd, self.himc);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ImeAction, classify_ime_message};
    use windows::Win32::UI::Input::Ime::{GCS_COMPSTR, GCS_RESULTSTR};
    use windows::Win32::UI::WindowsAndMessaging::{
        WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION, WM_IME_STARTCOMPOSITION,
    };

    #[test]
    fn suppresses_native_composition_but_passes_commit_and_end() {
        // 変換開始: 既定の変換文字列ウィンドウを作らせない（egui が preedit を描く）。
        assert_eq!(
            classify_ime_message(WM_IME_STARTCOMPOSITION, 0),
            ImeAction::Suppress
        );
        // 未確定のみ: egui が描くのでネイティブ描画を抑制（＝二重表示の解消）。
        assert_eq!(
            classify_ime_message(WM_IME_COMPOSITION, GCS_COMPSTR.0),
            ImeAction::Suppress
        );
        // 確定あり: Tao の ReceivedImeText 確定経路が要るため通す（確定文字を落とさない）。
        assert_eq!(
            classify_ime_message(WM_IME_COMPOSITION, GCS_RESULTSTR.0),
            ImeAction::PassThrough
        );
        // 確定 + 未確定が同時に立つ IME でも、確定を優先して通す。
        assert_eq!(
            classify_ime_message(WM_IME_COMPOSITION, GCS_RESULTSTR.0 | GCS_COMPSTR.0),
            ImeAction::PassThrough
        );
        // 変換終了・その他のメッセージは Tao へ通す（後始末・キー入力）。
        assert_eq!(
            classify_ime_message(WM_IME_ENDCOMPOSITION, 0),
            ImeAction::PassThrough
        );
        assert_eq!(classify_ime_message(0, 0), ImeAction::PassThrough);
    }
}
