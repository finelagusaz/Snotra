//! Structured trace logging behind the `SNOTRA_TRACE` env var.
//!
//! Single home of the trace logic (`OnceLock` env check + `AtomicU64`
//! sequence counter + JSON formatting, deduplicated in #433). `main.rs`
//! (`trace_main`) and `commands::trace_command` are thin wrappers that
//! delegate to `trace()` here.
//!
//! The `seq` counter is a single `AtomicU64` shared by both wrappers, so
//! main.rs and commands events interleave on one monotonic sequence. Trace
//! output is diagnostic-only (emitted only when `SNOTRA_TRACE` is set, in
//! release builds too).
//!
//! Using trace as an instrument has caveats — how much time sits between two
//! lines, and why separate sections must not each emit one. The canonical text
//! is in `PERFORMANCE.md`「計測と受け入れ基準」.
//!
//! A line is rendered into one `String` and written with a single `write_all`.
//! `eprintln!` on a `serde_json::Value` splits the line into one write per JSON
//! token or string fragment, and `Stderr` is unbuffered, so each token paid the per-write cost of
//! the redirected handle (#1539; numbers in `PERFORMANCE.md`「採用: trace 1 行を
//! 1 回の書き込みにする（#1539）」).

use std::io::Write;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::json;

/// 真偽 env フラグの共通解析（受理値: `1`/`true`/`yes`/`on`、trim + ASCII 小文字化）。
/// `trace_enabled`（`SNOTRA_TRACE`）等の env フラグが共有する受理仕様の SSOT。
/// キャッシュは呼び出し側の `OnceLock` が担う。
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

/// Emit one `[trace]` JSON line to stderr, in a single write, if `SNOTRA_TRACE` is enabled.
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
    let mut line = json!({
        "seq": seq,
        "ts_ms": ts_ms,
        "event": event,
        "data": data,
    });
    // ヒープ計装は `heap-trace` feature を有効にしたビルドでだけ値を返す（既定は `None`）。
    // **欄そのものを出さない**ことが「測っていない」の表現である——0 を書くと
    // 「測ったら 0 だった」と区別できない。意味と限界は `heap_trace.rs` の `//!`。
    if let Some(heap) = crate::heap_trace::snapshot() {
        line["heap"] = crate::heap_trace::heap_fields(&heap);
    }
    write_line(&mut std::io::stderr(), &render_line(&line));
}

/// 1 行（`[trace] ` + compact JSON + `\n`）を `String` へ組む。
fn render_line(line: &serde_json::Value) -> String {
    format!("[trace] {line}\n")
}

/// 組んだ 1 行を 1 回の `write_all` で書く。失敗は捨てる。診断用の出口なので、
/// 書けなくても製品を止めない（`eprintln!` は書き込みの失敗で panic し、release では
/// `panic = "abort"` によりプロセスごと止まる）。同じ形が `snotra-egui-runtime/src/stderr_line.rs` にも
/// あり、共有しない理由はそちらの `//!` にある。
fn write_line<W: Write>(w: &mut W, line: &str) {
    let _ = w.write_all(line.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `write` の呼び出し回数を数え、受け取ったバイト列を貯める。
    #[derive(Default)]
    struct CountingWriter {
        writes: usize,
        bytes: Vec<u8>,
    }

    impl Write for CountingWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.writes += 1;
            self.bytes.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn sample() -> serde_json::Value {
        json!({
            "seq": 7,
            "ts_ms": 1_700_000_000_000u64,
            "event": "egui_show:foreground",
            "data": { "path": "already", "sent": null, "text": "a\"b\\c\n" },
        })
    }

    #[test]
    fn write_line_issues_exactly_one_write() {
        let line = render_line(&sample());
        let mut w = CountingWriter::default();
        write_line(&mut w, &line);
        assert_eq!(w.writes, 1, "1 行は 1 回の write で書く");
        assert_eq!(w.bytes, line.as_bytes());
    }

    #[test]
    fn render_line_is_prefix_compact_json_and_one_newline() {
        let value = sample();
        let line = render_line(&value);
        let body = line
            .strip_prefix("[trace] ")
            .and_then(|rest| rest.strip_suffix('\n'))
            .expect("`[trace] ` で始まり `\\n` で終わる");
        assert!(!body.contains('\n'), "改行は末尾の 1 つだけ");
        assert_eq!(body, serde_json::to_string(&value).unwrap());
        let parsed: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(parsed, value);
    }
}
