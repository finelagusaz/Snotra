use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use snotra_core::engine::Engine;

pub struct AppState {
    pub engine: Mutex<Engine>,
    pub indexing: AtomicBool,
    pub index_build_started: AtomicBool,
    /// Tracks main window visibility to avoid costly Win32 `is_visible()` IPC on hotkey toggle.
    pub main_visible: AtomicBool,
}

/// Holds a WebView2 COM interface (`ICoreWebView2_6`) extracted during the setup phase.
/// Used to call `put_MemoryUsageTargetLevel` on hide/show to reduce memory when inactive.
///
/// The COM pointer is obtained via `with_webview()` (safe only in setup phase) and stored
/// here for later use in event-loop callbacks where `with_webview()` would deadlock.
///
/// `ICoreWebView2_6` is a COM smart pointer (`Send + Sync`) and `set_low`/`set_normal`
/// take `&self`, so no Mutex is needed.
#[cfg(windows)]
pub struct WebViewMemoryControl {
    inner: webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_6,
}

#[cfg(windows)]
impl WebViewMemoryControl {
    pub fn new(
        core: webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_6,
    ) -> Self {
        Self { inner: core }
    }

    /// Set memory usage target level to Low (best-effort, async internally).
    pub fn set_low(&self) {
        use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW;
        unsafe {
            let _ = self.inner.SetMemoryUsageTargetLevel(COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW);
        }
    }

    /// Set memory usage target level back to Normal.
    pub fn set_normal(&self) {
        use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL;
        unsafe {
            let _ = self.inner.SetMemoryUsageTargetLevel(COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL);
        }
    }
}

/// Set WebView2 memory usage to Low. No-op if WebViewMemoryControl is not registered.
#[cfg(windows)]
pub fn set_webview_memory_low(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(mem) = app.try_state::<WebViewMemoryControl>() {
        mem.set_low();
    }
}

/// Set WebView2 memory usage to Normal. No-op if WebViewMemoryControl is not registered.
#[cfg(windows)]
pub fn set_webview_memory_normal(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(mem) = app.try_state::<WebViewMemoryControl>() {
        mem.set_normal();
    }
}
