//! Structured trace logging behind the `SNOTRA_TRACE` env var.
//!
//! Single home of the trace logic (`OnceLock` env check + `AtomicU64`
//! sequence counter + JSON formatting, deduplicated in #433). `main.rs`
//! (`trace_main`) and `commands::trace_command` are thin wrappers that
//! delegate to `trace()` here.
//!
//! The `seq` counter is a single `AtomicU64` shared by both wrappers, so
//! main.rs and commands events interleave on one monotonic sequence. Trace
//! output is debug-only (emitted only when `SNOTRA_TRACE` is set).

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::json;

/// 真偽 env フラグの共通解析（受理値: `1`/`true`/`yes`/`on`、trim + ASCII 小文字化）。
/// `trace_enabled`（`SNOTRA_TRACE`）と `suspend_disabled`（`SNOTRA_DISABLE_SUSPEND`、
/// `main.rs`）が共有する受理仕様の SSOT。キャッシュは呼び出し側の `OnceLock` が担う。
pub(crate) fn env_flag(name: &str) -> bool {
    let Ok(v) = std::env::var(name) else {
        return false;
    };
    matches!(
        v.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

pub(crate) fn trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| env_flag("SNOTRA_TRACE"))
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
