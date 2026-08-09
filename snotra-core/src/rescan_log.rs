//! 背景再スキャンの in-situ 計器（#1001 受け入れ 1）。
//!
//! **時計も I/O も持たない純粋核と、best-effort の書き手に分かれる。** 前者を分けるのは
//! 測るためである——丸め境界のような fixture は注入でしか作れない（`startup.rs` の
//! `Timeline` と同じ理由）。
//!
//! # この物体は消えても壊れても書けなくてもよい
//!
//! `index.bin` / `history.bin` / `window.bin` / `icons.bin` と違い、**この記録が無くても
//! アプリの振る舞いは 1 ミリも変わらない**。ゆえに `binfmt` の版機構（magic + version +
//! フォールバック鎖）を持たず、追記の JSONL で足りる。**間引きの入力に兼ねさせてはならない**
//! ——計器が振る舞いを決める部品に変わると、「永遠に間引かれて再スキャンが一度も走らない」が
//! 沈黙で起きる（設計の正本は
//! `docs/superpowers/specs/2026-08-09-rescan-in-situ-instrument-design.md` §8.2）。

use std::time::Duration;

/// 記録の語彙としての結末。**`indexer::RescanOutcome` とは別の型である**——あちらは
/// 呼び出し側への通知（アイコン無効化の判断）で、こちらは記録の語彙。**`Skipped` の
/// 理由を分けるのはこちらだけ**であり、それが分ける価値のある区別だからこの型が要る。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoggedOutcome {
    /// 権威的ビルドが書き込み中で書き込みロックを取れなかった。
    SkippedLock,
    /// ロックは取れたが、読み込み後に権威的ビルドが割り込んで世代が変わっていた。
    SkippedGeneration,
    /// 走査したが、エントリ集合はキャッシュと同一だった。
    Unchanged,
    /// エントリ集合が変わり、`index.bin` を更新した。
    Changed,
}

impl LoggedOutcome {
    /// 出力の語。**ハーネスの契約なので固定する**（OS 依存の文言をここへ流さない）。
    pub fn outcome(self) -> &'static str {
        match self {
            LoggedOutcome::SkippedLock | LoggedOutcome::SkippedGeneration => "skipped",
            LoggedOutcome::Unchanged => "unchanged",
            LoggedOutcome::Changed => "changed",
        }
    }

    /// `skipped` の理由。skip でなければ `None`（出力では `null`）。
    pub fn skip_reason(self) -> Option<&'static str> {
        match self {
            LoggedOutcome::SkippedLock => Some("lock"),
            LoggedOutcome::SkippedGeneration => Some("generation"),
            LoggedOutcome::Unchanged | LoggedOutcome::Changed => None,
        }
    }
}

/// 1 回の再スキャンの記録。**時計を持たない**——区間は呼び出し側が測って渡す。
///
/// **通らなかった区間は `None` である。** `Duration::ZERO` で埋めてはならない
/// （「0 ミリ秒で書いた」と「書かなかった」は別である）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RescanRecord {
    pub scan: Option<Duration>,
    pub sort: Option<Duration>,
    pub digest: Option<Duration>,
    pub save: Option<Duration>,
    pub scanned: Option<usize>,
    /// 中身が変わっていないのに保存した（旧版の書き戻し）。**この経路は現状どこからも
    /// 見えない**——`outcome=unchanged` なのに `save_ms` が非 null になる唯一の理由である。
    pub format_upgrade: bool,
}

/// 名前の付いていない残余。**恒等式 `total = scan+sort+digest+save+unattributed` を
/// 全経路で閉じるための項目である。**
///
/// 残余を項目にせず「和が一致すること」だけを不変条件にすると、`Skipped` 経路
/// （どの区間も走らない）で検算が必ず破れ、読み手は理由が読めなくなる。
///
/// **単調でない入力では飽和する**（負にしない）。計器の算術で製品を落とさない。
pub fn unattributed(rec: &RescanRecord, total: Duration) -> Duration {
    let sum = [rec.scan, rec.sort, rec.digest, rec.save]
        .into_iter()
        .flatten()
        .sum::<Duration>();
    total.saturating_sub(sum)
}

/// ミリ秒表示への変換。**丸めはこの 1 か所だけで起きる。**
///
/// `Duration::as_millis` を使わず除算を書いてあるのは、**除数を変異させて検知器が
/// 落ちることを測れるようにする**ためである（`startup.rs::to_ms` と同じ意図）。
fn to_ms(d: Duration) -> u64 {
    (d.as_nanos() / 1_000_000) as u64
}

/// `Option<Duration>` を出力の値へ。**`None` は `null`**（0 にしない）。
fn ms_or_null(d: Option<Duration>) -> serde_json::Value {
    d.map_or(serde_json::Value::Null, |d| serde_json::json!(to_ms(d)))
}

/// 走査を**始める前**に書く行。
///
/// **この行が先に出ることが本設計の全部である**——終端だけを書く形では、走査より短い
/// セッション（実測 12 秒 < 22 秒）が「そもそも起動しなかった」と区別できない。
/// **start だけ在って end が無い行が「完走しなかった起動」を表す。**
pub fn start_line(ts: &str, sid: &str, roots: usize, cached: usize, cached_version: u32) -> String {
    serde_json::json!({
        "ev": "start",
        "ts": ts,
        "sid": sid,
        "roots": roots,
        "cached": cached,
        "cached_version": cached_version,
    })
    .to_string()
}

/// 終端で書く行。`total` は**終端で 1 回読んだ値**であり、部分和から作らない。
pub fn end_line(
    ts: &str,
    sid: &str,
    outcome: LoggedOutcome,
    rec: &RescanRecord,
    total: Duration,
) -> String {
    serde_json::json!({
        "ev": "end",
        "ts": ts,
        "sid": sid,
        "outcome": outcome.outcome(),
        "skip_reason": outcome.skip_reason(),
        "scan_ms": ms_or_null(rec.scan),
        "sort_ms": ms_or_null(rec.sort),
        "digest_ms": ms_or_null(rec.digest),
        "save_ms": ms_or_null(rec.save),
        "unattributed_ms": to_ms(unattributed(rec, total)),
        "total_ms": to_ms(total),
        "scanned": rec.scanned,
        "format_upgrade": rec.format_upgrade,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn parse(line: &str) -> serde_json::Value {
        serde_json::from_str(line).expect("行が JSON として読めること")
    }

    #[test]
    fn start_line_carries_the_fields_a_reader_needs_to_pair_and_interpret() {
        let v = parse(&start_line(
            "2026-08-09T13:32:01.412Z",
            "19212-1786",
            4,
            312_625,
            7,
        ));
        assert_eq!(v["ev"], "start");
        assert_eq!(v["ts"], "2026-08-09T13:32:01.412Z");
        assert_eq!(v["sid"], "19212-1786");
        assert_eq!(v["roots"], 4);
        assert_eq!(v["cached"], 312_625);
        assert_eq!(v["cached_version"], 7);
    }

    #[test]
    fn end_line_reports_outcome_and_segments() {
        let rec = RescanRecord {
            scan: Some(ms(22_013)),
            sort: Some(ms(180)),
            digest: Some(ms(14)),
            save: Some(ms(210)),
            scanned: Some(311_697),
            format_upgrade: false,
        };
        let v = parse(&end_line(
            "t",
            "sid",
            LoggedOutcome::Changed,
            &rec,
            ms(22_420),
        ));
        assert_eq!(v["ev"], "end");
        assert_eq!(v["outcome"], "changed");
        assert!(v["skip_reason"].is_null(), "skip でないなら理由は null");
        assert_eq!(v["scan_ms"], 22_013);
        assert_eq!(v["sort_ms"], 180);
        assert_eq!(v["digest_ms"], 14);
        assert_eq!(v["save_ms"], 210);
        assert_eq!(v["total_ms"], 22_420);
        assert_eq!(v["scanned"], 311_697);
        assert_eq!(v["format_upgrade"], false);
    }

    /// **`null` と `0` を区別する。** 「0 ミリ秒で書いた」と「書かなかった」は別である。
    /// 変異: 書かなかったときに 0 を入れる実装にすると落ちる。
    #[test]
    fn segments_that_never_ran_are_null_not_zero() {
        let rec = RescanRecord {
            scan: Some(ms(10)),
            sort: Some(ms(1)),
            digest: Some(ms(1)),
            save: None, // 書かなかった
            scanned: Some(3),
            format_upgrade: false,
        };
        let v = end_line("t", "sid", LoggedOutcome::Unchanged, &rec, ms(12));
        let v = parse(&v);
        assert!(v["save_ms"].is_null(), "書かなかった区間は null");
        assert!(v.get("save_ms").is_some(), "キー自体は必ず出る");
    }

    /// **`Skipped` の 2 理由を分ける。** 「本式ビルドと競合した」と「世代が変わった」は
    /// 別の話である。変異: 両方を単一の "skipped" へ潰すと落ちる。
    #[test]
    fn skipped_reasons_are_distinguished() {
        let rec = RescanRecord::default();
        let lock = parse(&end_line("t", "s", LoggedOutcome::SkippedLock, &rec, ms(0)));
        let r#gen = parse(&end_line(
            "t",
            "s",
            LoggedOutcome::SkippedGeneration,
            &rec,
            ms(1),
        ));
        assert_eq!(lock["outcome"], "skipped");
        assert_eq!(r#gen["outcome"], "skipped");
        assert_eq!(lock["skip_reason"], "lock");
        assert_eq!(r#gen["skip_reason"], "generation");
        assert_ne!(lock["skip_reason"], r#gen["skip_reason"]);
    }

    /// **恒等式は生の `Duration` で成り立つ。** ms 表示の和では丸めでずれるので、
    /// そちらで検査してはならない（`startup.rs` の同名の教訓）。
    /// 変異: `total` を部分和から作る実装にすると `unattributed` が 0 になって落ちる。
    #[test]
    fn unattributed_closes_the_identity_in_raw_durations() {
        let rec = RescanRecord {
            scan: Some(ms(100)),
            sort: Some(ms(10)),
            digest: Some(ms(5)),
            save: Some(ms(20)),
            scanned: Some(1),
            format_upgrade: false,
        };
        let total = ms(140);
        assert_eq!(unattributed(&rec, total), ms(5));
        let sum = ms(100) + ms(10) + ms(5) + ms(20) + unattributed(&rec, total);
        assert_eq!(sum, total, "恒等式が閉じる");
    }

    /// `Skipped` 経路では走査すらしていない。**残余を項目にしないと、ここで検算が必ず破れる。**
    #[test]
    fn identity_holds_on_the_skipped_path_where_no_segment_ran() {
        let rec = RescanRecord::default();
        let total = ms(3);
        assert_eq!(
            unattributed(&rec, total),
            total,
            "全区間が null なら残余が総計そのもの"
        );
        let v = parse(&end_line("t", "s", LoggedOutcome::SkippedLock, &rec, total));
        assert!(v["scan_ms"].is_null());
        assert_eq!(v["unattributed_ms"], 3);
        assert_eq!(v["total_ms"], 3);
    }

    /// 単調でない入力（区間の和が総計を超える）で panic しない。**計器が製品を落とさない。**
    #[test]
    fn unattributed_saturates_instead_of_underflowing() {
        let rec = RescanRecord {
            scan: Some(ms(100)),
            ..RescanRecord::default()
        };
        assert_eq!(unattributed(&rec, ms(10)), Duration::ZERO);
    }

    /// 丸めは表示境界だけで起きる。変異: 除数を 1_000 にすると落ちる。
    #[test]
    fn to_ms_truncates_toward_zero() {
        assert_eq!(to_ms(Duration::from_nanos(999_999)), 0);
        assert_eq!(to_ms(Duration::from_nanos(1_000_000)), 1);
        assert_eq!(to_ms(Duration::from_nanos(1_999_999)), 1);
    }

    /// `outcome` / `skip_reason` の語はハーネスの契約なので固定する。
    #[test]
    fn outcome_words_are_stable_and_unique() {
        let all = [
            LoggedOutcome::SkippedLock,
            LoggedOutcome::SkippedGeneration,
            LoggedOutcome::Unchanged,
            LoggedOutcome::Changed,
        ];
        let mut pairs: Vec<(&str, Option<&str>)> =
            all.iter().map(|o| (o.outcome(), o.skip_reason())).collect();
        pairs.sort_unstable();
        let before = pairs.len();
        pairs.dedup();
        assert_eq!(
            before,
            pairs.len(),
            "(outcome, skip_reason) の組が衝突している"
        );
        assert_eq!(LoggedOutcome::Changed.outcome(), "changed");
        assert_eq!(LoggedOutcome::Unchanged.outcome(), "unchanged");
    }
}
