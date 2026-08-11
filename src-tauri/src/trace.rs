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

/// `trace()` が stderr への書き込みに費やした累積時間（µs・#1032 の調査足場）。
///
/// **計器が計器自身を測ってしまうのを防ぐための控除項である。** trace 1 本の書き込みは
/// 実運用の観測条件（`SNOTRA_TRACE=1` + stderr をファイルへリダイレクト）で約 10 ms かかり
/// （#1004 PR 1 の実測）、フレームを区間へ切っても差し引かなければ「どの区間が trace を
/// 吐くか」の表にしかならない。`egui_search:settled` は**行が差し替わったフレームでだけ**
/// 出るため、#1032 が跳ねると観測した当のフレームと発火条件が一致する。
///
/// **これはプロセス大域であってスレッドごとではない。** ゆえに UI の区間を測っている最中に
/// 別スレッド（worker・config_watcher・icon worker）が trace を吐くと、その書き込み時間まで
/// UI の区間から差し引かれる——**区間を過小に見せる向きの誤差である**。thread-local にすれば
/// 正確になるが、**A/B を同じ器で比べるという前提のほうを優先して直していない**（A 側の標本は
/// この器で採ってあり、実装前には戻れない）。実測では A 側の競合区間が 43,939 µs と出ており
/// 0 へ潰れてはいないので、#1032 の帰属はこの誤差では覆らない。
///
/// **撤去条件**: #1036（この計器を入れた PR）がマージされたら、直ちに撤去 PR を出す。
/// **#1032 はその撤去 PR で閉じる**——「#1032 が閉じたら消す」と書くと、閉じるのが撤去
/// そのものなので条件が自分を指してしまい、誰も撤去を始めない。
///
/// **撤去の対象**: この静的変数・`Segment`・`view.rs` の区間計装（`egui_frame` の内訳
/// フィールド）・`window_coordinator::DriveTiming` と `GAP_LOCK_US`・
/// `ResultsWindow::set_size` の `bool` 返し。**`PERFORMANCE.md` の実測記録は残す**
/// （計器は使い捨てだが、測った値は判断の根拠として生き続ける）。
static TRACE_ELAPSED_US: AtomicU64 = AtomicU64::new(0);

/// 区間の所要から、その区間で吐いた trace の書き込み時間を差し引いて測る（#1032 の調査足場）。
///
/// 撤去条件は `TRACE_ELAPSED_US` の doc。
pub(crate) struct Segment {
    at: std::time::Instant,
    trace_at: u64,
}

impl Segment {
    pub(crate) fn start() -> Self {
        Self {
            at: std::time::Instant::now(),
            trace_at: TRACE_ELAPSED_US.load(Ordering::Relaxed),
        }
    }

    /// 区間の所要（µs・trace の書き込みを除く）。
    pub(crate) fn end(self) -> u64 {
        let raw = self.at.elapsed().as_micros() as u64;
        let traced = TRACE_ELAPSED_US
            .load(Ordering::Relaxed)
            .saturating_sub(self.trace_at);
        // 控除が所要を上回るのは µs 丸めの ±1 だけである（trace は区間の内側で走る）。
        raw.saturating_sub(traced)
    }
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
    // #1032 の調査足場: 書き込みに費やした時間を控除項へ積む（`TRACE_ELAPSED_US` の doc）。
    let write_started = std::time::Instant::now();
    eprintln!(
        "[trace] {}",
        json!({
            "seq": seq,
            "ts_ms": ts_ms,
            "event": event,
            "data": data,
        })
    );
    TRACE_ELAPSED_US.fetch_add(
        write_started.elapsed().as_micros() as u64,
        Ordering::Relaxed,
    );
}
