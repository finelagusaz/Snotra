//! Structured trace logging behind the `SNOTRA_TRACE` env var.
//!
//! `main.rs` (`trace_main`) and `commands::trace_command` used to each carry
//! their own near-identical copy of this logic (`OnceLock` env check +
//! `AtomicU64` sequence counter + JSON formatting, #433). Both now delegate
//! to `trace()` here.
//!
//! Collapsing the two `AtomicU64` counters into one changes the `seq` values
//! observed in trace output: main.rs and commands events now interleave on a
//! single monotonic counter instead of each keeping its own. Trace output is
//! debug-only (emitted only when `SNOTRA_TRACE` is set) — this is an accepted
//! behavior change called out in the PR description, not a functional
//! regression.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::json;

pub(crate) fn trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        let Ok(v) = std::env::var("SNOTRA_TRACE") else {
            return false;
        };
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

/// Emit one `[trace]` JSON line to stderr if `SNOTRA_TRACE` is enabled.
/// `seq` is a single process-wide monotonic counter shared by every call
/// site (both `main.rs` and `commands/`), so trace lines from both sides
/// interleave in one total order.
pub(crate) fn trace(event: &str, data: serde_json::Value) {
    if !trace_enabled() {
        return;
    }
    static TRACE_SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = TRACE_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    let ts_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    eprintln!(
        "[trace] {}",
        json!({
            "seq": seq,
            "ts_ms": ts_ms,
            "event": event,
            "data": data,
        })
    );
}
