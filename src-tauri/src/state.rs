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
