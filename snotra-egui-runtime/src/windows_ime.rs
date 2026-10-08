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
        WindowsAndMessaging::{WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION, WM_IME_STARTCOMPOSITION},
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

        // tauri::Window::hwnd() は tauri が依存する windows 版の HWND を返す。当 crate は
        // 別版を使うため、生ポインタを取り出して自版の型で組み直す（src-tauri と同じ形）。
        // 両版の HWND は同一定義（pub struct HWND(pub *mut c_void)）ゆえ表現は等しい。
        //
        // SAFETY: hwnd is owned by the attached Tauri Window. callback_ptr stays
        // valid until Drop removes this exact callback/id pair.
        let installed = unsafe {
            SetWindowSubclass(
                HWND(hwnd.0),
                Some(ime_subclass_proc),
                IME_SUBCLASS_ID,
                callback_ptr,
            )
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
        if crate::env::trace_hatch_enabled("SNOTRA_EGUI_IME_TRACE")
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

/// IME サブクラスが受けたメッセージを tao へ通すか（#532・#1266）。
///
/// **IME の 3 メッセージは確定を含めてすべてこの subclass が持つ。** 未確定は egui が自前で描くので
/// 既定の変換文字列ウィンドウを作らせない（#532 の二重表示）。確定は [`ime_events_for`] が
/// `GCS_RESULTSTR` から読んで送る。確定と `ENDCOMPOSITION` を通さないのは tao 0.37 以降の理由で、
/// 通すと二重に入る——tao が `WM_IME_ENDCOMPOSITION` で `GCS_RESULTSTR` を読み直して
/// `ReceivedImeText` を送り、`DefWindowProc` が確定から `WM_IME_CHAR` → `WM_CHAR` を作る。
/// tao 0.35 は確定をその `WM_CHAR` から拾っていた（`minimal_ime`）ので、以前は確定を通していた。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ImeAction {
    /// DefSubclassProc を呼ばず `LRESULT(0)`。tao にも既定の IME 処理にも届かない。
    Suppress,
    /// そのまま tao へ通す（DefSubclassProc）。キー入力などは tao が担う。
    PassThrough,
}

fn classify_ime_message(message: u32) -> ImeAction {
    if matches!(
        message,
        WM_IME_STARTCOMPOSITION | WM_IME_COMPOSITION | WM_IME_ENDCOMPOSITION
    ) {
        ImeAction::Suppress
    } else {
        ImeAction::PassThrough
    }
}

/// IME メッセージ 1 通から egui へ送るイベント列を導く（#1266）。読み出しは呼び出し側が
/// 渡す——IMM32 に触らずに順序と条件をテストで固定するため。
///
/// **確定は `GCS_RESULTSTR` が立っているときだけ読み、Commit を Preedit より先に置く。**
/// 変換中の続け打ちでは確定と新しい未確定が同じメッセージで来る。egui は Commit で preedit の
/// 範囲を消してから確定を挿し、後続の Preedit が新しい範囲を張るので、この順でなければ
/// 新しい未確定が確定に上書きされる。空の確定は送らない。
fn ime_events_for(
    message: u32,
    lparam: u32,
    read_result: impl FnOnce() -> Option<String>,
    read_preedit: impl FnOnce() -> Option<egui::ImeEvent>,
) -> Vec<egui::ImeEvent> {
    match message {
        WM_IME_COMPOSITION => {
            let mut events = Vec::with_capacity(2);
            if lparam & GCS_RESULTSTR.0 != 0
                && let Some(text) = read_result()
                && !text.is_empty()
            {
                events.push(egui::ImeEvent::Commit(text));
            }
            events.extend(read_preedit());
            events
        }
        WM_IME_ENDCOMPOSITION => vec![egui::ImeEvent::Preedit {
            text: String::new(),
            active_range_chars: None,
        }],
        _ => Vec::new(),
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
    let action = classify_ime_message(message);
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: ref_data was installed from a live Box<CallbackState>, and
        // Drop removes the subclass before releasing that Box.
        let Some(state) = (unsafe { (ref_data as *const CallbackState).as_ref() }) else {
            return;
        };

        let events = ime_events_for(
            message,
            lparam.0 as u32,
            || ImeContext::acquire(hwnd)?.composition_string(GCS_RESULTSTR),
            || read_preedit(hwnd),
        );
        if events.is_empty() {
            return;
        }
        let trace = crate::env::trace_hatch_enabled("SNOTRA_EGUI_IME_TRACE");
        for event in events {
            if trace {
                // 文字列そのものは出さない（入力内容が trace に残らないように）。
                match &event {
                    egui::ImeEvent::Commit(text) => {
                        eprintln!("SNOTRA_EGUI_IME_COMMIT chars={}", text.chars().count());
                    }
                    egui::ImeEvent::Preedit {
                        text,
                        active_range_chars,
                    } => eprintln!(
                        "SNOTRA_EGUI_IME_PREEDIT chars={} active={active_range_chars:?}",
                        text.chars().count()
                    ),
                    _ => {}
                }
            }
            let _ = state.sender.send(event);
        }
        // tao は preedit も確定も WindowEvent にしない（ここが送る）。後続のウィンドウイベントが
        // あればその処理前に `drain_native_ime` が回収し、無ければこの再描画要求が
        // `RedrawRequested` → `render` の drain で回収する。
        // SAFETY: hwnd is the window currently dispatching this callback.
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }));

    match action {
        ImeAction::Suppress => LRESULT(0),
        // SAFETY: every non-IME message must continue through tao's own window procedure,
        // which remains responsible for key events (and the text they carry).
        ImeAction::PassThrough => unsafe { DefSubclassProc(hwnd, message, wparam, lparam) },
    }
}

fn read_preedit(hwnd: HWND) -> Option<egui::ImeEvent> {
    let context = ImeContext::acquire(hwnd)?;
    let text = context.composition_string(GCS_COMPSTR)?;
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

    /// kind の文字列（GCS_COMPSTR = 未確定・GCS_RESULTSTR = 確定）を UTF-16 から復号する。
    fn composition_string(
        &self,
        kind: windows::Win32::UI::Input::Ime::IME_COMPOSITION_STRING,
    ) -> Option<String> {
        let bytes = self.composition_data(kind)?;
        if bytes.len() % 2 != 0 {
            return None;
        }
        let utf16 = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|unit| u16::from_ne_bytes(*unit))
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
    use super::{ImeAction, classify_ime_message, ime_events_for};
    use windows::Win32::UI::Input::Ime::{GCS_COMPSTR, GCS_RESULTSTR};
    use windows::Win32::UI::WindowsAndMessaging::{
        WM_CHAR, WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION, WM_IME_STARTCOMPOSITION,
    };

    fn preedit(text: &str) -> egui::ImeEvent {
        egui::ImeEvent::Preedit {
            text: text.to_owned(),
            active_range_chars: None,
        }
    }

    fn commit(text: &str) -> egui::ImeEvent {
        egui::ImeEvent::Commit(text.to_owned())
    }

    /// #1266: IME の 3 メッセージは確定を含めてすべて subclass が持ち、tao へも
    /// `DefWindowProc` へも通さない。確定を通すと tao 0.37 が `ENDCOMPOSITION` で
    /// `ReceivedImeText` を送り直し、`DefWindowProc` が `WM_IME_CHAR` → `WM_CHAR` を作る。
    #[test]
    fn ime_messages_are_owned_by_the_subclass() {
        for message in [
            WM_IME_STARTCOMPOSITION,
            WM_IME_COMPOSITION,
            WM_IME_ENDCOMPOSITION,
        ] {
            assert_eq!(
                classify_ime_message(message),
                ImeAction::Suppress,
                "message={message:#x}"
            );
        }
        // キー入力などは tao へ通す。
        assert_eq!(classify_ime_message(WM_CHAR), ImeAction::PassThrough);
        assert_eq!(classify_ime_message(0), ImeAction::PassThrough);
    }

    /// 確定と未確定が同じメッセージで立つとき（変換中の続け打ちで前の文節が確定する）、
    /// **Commit を先に送る**。egui は Commit で preedit の範囲を消してから確定を挿し、
    /// 後続の Preedit が新しい範囲を張る——逆にすると新しい未確定が確定で上書きされる。
    #[test]
    fn commit_precedes_preedit_in_the_same_message() {
        let events = ime_events_for(
            WM_IME_COMPOSITION,
            GCS_RESULTSTR.0 | GCS_COMPSTR.0,
            || Some("日本".to_owned()),
            || Some(preedit("ご")),
        );
        assert_eq!(events, vec![commit("日本"), preedit("ご")]);
    }

    #[test]
    fn result_only_composition_commits_then_reports_the_composition_state() {
        let events = ime_events_for(
            WM_IME_COMPOSITION,
            GCS_RESULTSTR.0,
            || Some("日本語".to_owned()),
            || Some(preedit("")),
        );
        assert_eq!(events, vec![commit("日本語"), preedit("")]);
    }

    /// 確定の読み出しは `GCS_RESULTSTR` が立っているときだけ行い、空なら送らない。
    #[test]
    fn commit_is_read_only_when_flagged_and_dropped_when_empty() {
        let events = ime_events_for(
            WM_IME_COMPOSITION,
            GCS_COMPSTR.0,
            || panic!("GCS_RESULTSTR が立っていないのに確定を読んだ"),
            || Some(preedit("にほ")),
        );
        assert_eq!(events, vec![preedit("にほ")]);

        let events = ime_events_for(
            WM_IME_COMPOSITION,
            GCS_RESULTSTR.0,
            || Some(String::new()),
            || Some(preedit("")),
        );
        assert_eq!(events, vec![preedit("")]);
    }

    #[test]
    fn end_composition_clears_the_preedit_and_other_messages_send_nothing() {
        let events = ime_events_for(
            WM_IME_ENDCOMPOSITION,
            0,
            || panic!("ENDCOMPOSITION で確定を読んだ"),
            || panic!("ENDCOMPOSITION で preedit を読んだ"),
        );
        assert_eq!(events, vec![preedit("")]);

        let events = ime_events_for(
            WM_IME_STARTCOMPOSITION,
            0,
            || panic!("確定を読んだ"),
            || panic!("preedit を読んだ"),
        );
        assert!(events.is_empty());
    }
}
