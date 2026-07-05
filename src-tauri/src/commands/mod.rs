mod config;
mod icon;
pub(crate) mod instant;
pub(crate) mod launch;
mod search;
mod system;
mod window;

pub use config::*;
pub use icon::*;
pub use launch::*;
pub use search::*;
pub use system::*;
pub use window::*;

// Re-export pub(crate) items that main.rs references
pub(crate) use window::launch_settings_process;

/// Thin wrapper kept so call sites read `trace_command(...)`; logic lives in
/// the shared `crate::trace` module (deduped with `main.rs`'s `trace_main`, #433).
pub(crate) fn trace_command(event: &str, data: serde_json::Value) {
    crate::trace::trace(event, data);
}
