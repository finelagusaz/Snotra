//! 索引の構築の記録（`%APPDATA%\Snotra\scan-log.jsonl`・`SPEC.md` §13.4・#1041）。
//!
//! **時計も I/O も持たない純粋核と、best-effort の書き手・読み手に分かれる。** 前者を分けるのは
//! 測るためである——丸め境界のような fixture は注入でしか作れない（`startup.rs` の `Timeline` と
//! 同じ理由）。
//!
//! # このファイルは消えても壊れても書けなくてもよい
//!
//! `index.bin` などと違い、**この記録が無くても索引の構築は変わらない**。ゆえに `binfmt` の版機構を
//! 持たず、追記の JSONL で足りる。**構築の振る舞いの入力にしてはならない**——計測コードが構築を決める
//! 部品に変わると、記録の欠損・破損が構築の誤りとして沈黙で現れる（撤去前の計測コードの設計
//! `docs/superpowers/specs/2026-08-09-rescan-in-situ-instrument-design.md` §8.2 が却下した形）。
//!
//! **読むのは進捗表示の分母だけである**（[`begin_in`] が返す値）。表示は誤っても欠けても
//! 「フォルダ数だけを出す」へ落ちるだけで、構築は変わらない。§8.2 が却下したのは構築の入力に
//! 兼ねさせる形であり、表示が読む形はそれに当たらない。
//!
//! # 2 行 1 組
//!
//! 走査する構築 1 回（`indexing::drain_index` の走査する 1 反復）につき、走査の**前**に `start` 行、索引の差し替えの
//! 後に `end` 行を書く。**`start` だけあって `end` が無い行は、完走しなかった構築である。**
//!
//! # 行は読めないことがある
//!
//! kill で行の途中が切れる・2 プロセスの書き込みが競合する・非 UTF-8 が混ざる。**読み手は行を
//! バイト列で分け、1 行ずつ解釈し、読めない行を飛ばす**——ファイル全体を文字列として読むと、
//! 1 バイトの非 UTF-8 で全行が読めなくなる。

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::startup::to_ms;

/// 保つ行数の上限。1 行 300 B 程度なので 60 KB ほど。**全読みして書き直しても安い。**
pub(crate) const MAX_LINES: usize = 200;

/// 1 回の構築の区間と数。**時計を持たない**——区間は呼び出し側が測って渡す。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ScanRecord {
    pub(crate) scan: Duration,
    pub(crate) sort: Duration,
    pub(crate) save: Duration,
    /// PATH のマージと `PrebuiltIndex` の構築（`indexing::build_index_from_material`）。
    pub(crate) build: Duration,
    /// 登録したエントリの件数（PATH のマージ前）。
    pub(crate) scanned: usize,
    /// `read_dir` に成功したディレクトリの数。
    pub(crate) dirs: usize,
}

/// 名前の付いていない残余。**恒等式 `total = scan + sort + save + build + unattributed` を閉じる
/// ための項目である。** ロックの取得待ち（engine・`INDEX_WRITE_LOCK`）と swap がここに入る。
///
/// **単調でない入力では 0 へ飽和する**（負にしない）。計測の算術で製品を落とさない。
pub(crate) fn unattributed(rec: &ScanRecord, total: Duration) -> Duration {
    let sum = rec.scan + rec.sort + rec.save + rec.build;
    total.saturating_sub(sum)
}

/// 走査を**始める前**に書く行。`expected` は進捗表示の分母（無ければ `null`）。
pub(crate) fn start_line(
    ts_ms: u64,
    sid: &str,
    scan_id: &str,
    roots: usize,
    expected: Option<usize>,
) -> String {
    serde_json::json!({
        "ev": "start",
        "ts_ms": ts_ms,
        "sid": sid,
        "scan_id": scan_id,
        "roots": roots,
        "expected": expected,
    })
    .to_string()
}

/// 索引の差し替えの後に書く行。`total` は**終端で 1 回読んだ値**であり、部分和から作らない。
pub(crate) fn end_line(
    ts_ms: u64,
    sid: &str,
    scan_id: &str,
    rec: &ScanRecord,
    total: Duration,
) -> String {
    serde_json::json!({
        "ev": "end",
        "ts_ms": ts_ms,
        "sid": sid,
        "scan_id": scan_id,
        "scan_ms": to_ms(rec.scan),
        "sort_ms": to_ms(rec.sort),
        "save_ms": to_ms(rec.save),
        "build_ms": to_ms(rec.build),
        "unattributed_ms": to_ms(unattributed(rec, total)),
        "total_ms": to_ms(total),
        "scanned": rec.scanned,
        "dirs": rec.dirs,
    })
    .to_string()
}

/// 本文を行へ分ける。**バイト列のまま `\n` で分ける**（文字列として読むと、1 バイトの非 UTF-8 で
/// 全体が読めなくなる）。空の行は数えない（末尾の改行と、[`append_in`] が足す区切り）。
fn split_lines(body: &[u8]) -> impl DoubleEndedIterator<Item = &[u8]> {
    body.split(|b| *b == b'\n').filter(|l| !l.is_empty())
}

/// 記録の本文から、`scan_id` が一致する**最後の** `end` 行の `dirs` を返す。
///
/// 読めない行・`start` 行・別の `scan_id` の行・`dirs` が正の整数でない行は飛ばす。**0 は分母として
/// 役に立たないので「無し」と同じに扱う**（`AppState::scan_progress` と同じ意味）。
pub(crate) fn last_completed_dirs(body: &[u8], scan_id: &str) -> Option<usize> {
    split_lines(body).rev().find_map(|line| {
        let v: serde_json::Value = serde_json::from_slice(line).ok()?;
        if v["ev"] != "end" || v["scan_id"] != scan_id {
            return None;
        }
        let dirs = usize::try_from(v["dirs"].as_u64()?).ok()?;
        (dirs > 0).then_some(dirs)
    })
}

/// 記録の在り処。**`dir` は `Config::config_dir()` から来る**——保存先を導く経路は 1 つだけであり、
/// `SNOTRA_CONFIG_DIR` もそこで効く。
pub(crate) fn log_path_in(dir: &Path) -> PathBuf {
    dir.join("scan-log.jsonl")
}

/// 1 行追記する。**失敗は握り潰す**（ディレクトリ不在・権限・ロック）。
///
/// **行末の `\n` まで 1 回の `write_all` で書く。** ファイルの末尾が `\n` で終わっていなければ
/// （kill で行の途中が切れた）、先頭に `\n` を足して同じ 1 回で書く——さもないと新しい行が
/// 切れた行に連結され、両方読めなくなる。
pub(crate) fn append_in(dir: &Path, line: &str) {
    use std::io::Write;
    let path = log_path_in(dir);
    let mut buf = String::with_capacity(line.len() + 2);
    if ends_without_newline(&path) {
        buf.push('\n');
    }
    buf.push_str(line);
    buf.push('\n');
    let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return;
    };
    let _ = f.write_all(buf.as_bytes());
}

/// 空でないファイルの最後のバイトが `\n` でないか。読めなければ `false`（区切りを足さない）。
fn ends_without_newline(path: &Path) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    // 空のファイルでは負の位置へのシークが失敗する。
    if f.seek(SeekFrom::End(-1)).is_err() {
        return false;
    }
    let mut last = [0u8; 1];
    matches!(f.read(&mut last), Ok(1)) && last[0] != b'\n'
}

/// 上限を超えていたら**末尾** [`MAX_LINES`] 行を残して書き直す。**最新を残す**（古い方を捨てる）。
///
/// **読めない行も 1 行として数える。** 書き直しは同じディレクトリの固定名の tmp へ書き、
/// **`remove_file` を挟まずに** `rename` だけで置き換える——消した直後にプロセスが終わると
/// 全行を失う（`binfmt.rs` の `BinFile` の保存手順を写さない理由）。失敗は握り潰す。
pub(crate) fn prune_in(dir: &Path) {
    let path = log_path_in(dir);
    let Ok(body) = std::fs::read(&path) else {
        return;
    };
    let lines: Vec<&[u8]> = split_lines(&body).collect();
    let Some(drop) = lines.len().checked_sub(MAX_LINES).filter(|n| *n > 0) else {
        return;
    };
    let mut out = Vec::with_capacity(body.len());
    for line in &lines[drop..] {
        out.extend_from_slice(line);
        out.push(b'\n');
    }
    let tmp = dir.join("scan-log.jsonl.tmp");
    if std::fs::write(&tmp, &out).is_err() || std::fs::rename(&tmp, &path).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// 構築の開始: 剪定 → 分母の読み → `start` 行の追記を**この順で**行い、分母を返す。
///
/// 剪定を先に置くのは、剪定で消える行を分母にも使わないためである（200 行の中に同じ走査対象の
/// 完走が無ければ分母は出ない）。
pub(crate) fn begin_in(
    dir: &Path,
    ts_ms: u64,
    sid: &str,
    scan_id: &str,
    roots: usize,
) -> Option<usize> {
    prune_in(dir);
    let expected = std::fs::read(log_path_in(dir))
        .ok()
        .and_then(|body| last_completed_dirs(&body, scan_id));
    append_in(dir, &start_line(ts_ms, sid, scan_id, roots, expected));
    expected
}

/// この 1 回の構築を指す識別子。pid + エポックミリ秒 + プロセス内の連番——同じミリ秒の連続した
/// 構築でも衝突せず、2 プロセスの同時稼働も pid で区別が付く。
pub(crate) fn new_sid() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{}-{}-{seq}", std::process::id(), now_ts_ms())
}

/// 記録の時刻（エポックミリ秒）。`trace.rs` の `ts_ms` と同じ単位にそろえてある（計算は
/// 別に持つ。あちらは trace の行を組む関数の内側にあり、取り出す口が無い）。
pub(crate) fn now_ts_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 走査対象の同一性（`indexer::scan_identity_hash`）を記録に書く形へ。16 桁の小文字 16 進。
pub(crate) fn scan_id(hash: u64) -> String {
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn parse(line: &str) -> serde_json::Value {
        serde_json::from_str(line).expect("行が JSON として読めること")
    }

    /// テストごとに別の一時ディレクトリ。実 `%APPDATA%\Snotra` には触れない。
    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("snotra-scan-log-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn record() -> ScanRecord {
        ScanRecord {
            scan: ms(22_013),
            sort: ms(180),
            save: ms(210),
            build: ms(95),
            scanned: 7,
            dirs: 11,
        }
    }

    fn lines_of(dir: &Path) -> Vec<String> {
        let body = fs::read(log_path_in(dir)).expect("read log");
        body.split(|b| *b == b'\n')
            .filter(|l| !l.is_empty())
            .map(|l| String::from_utf8_lossy(l).into_owned())
            .collect()
    }

    #[test]
    fn start_line_carries_the_fields_and_null_for_a_missing_denominator() {
        let v = parse(&start_line(1_000, "sid-a", "scan-b", 5, Some(98_000)));
        assert_eq!(v["ev"], "start");
        assert_eq!(v["ts_ms"], 1_000);
        assert_eq!(v["sid"], "sid-a");
        assert_eq!(v["scan_id"], "scan-b");
        assert_eq!(v["roots"], 5);
        assert_eq!(v["expected"], 98_000);

        let v = parse(&start_line(1_000, "sid-a", "scan-b", 5, None));
        assert!(
            v["expected"].is_null(),
            "分母が無いときは null（0 にしない）"
        );
    }

    /// 同じ型のフィールドに互いに違う値を入れ、キーごとに照合する（`sid` と `scan_id`、
    /// `scanned` と `dirs` の取り違えを検出する）。
    #[test]
    fn end_line_reports_every_segment_under_its_own_key() {
        let v = parse(&end_line(2_000, "sid-a", "scan-b", &record(), ms(22_600)));
        assert_eq!(v["ev"], "end");
        assert_eq!(v["ts_ms"], 2_000);
        assert_eq!(v["sid"], "sid-a");
        assert_eq!(v["scan_id"], "scan-b");
        assert_eq!(v["scan_ms"], 22_013);
        assert_eq!(v["sort_ms"], 180);
        assert_eq!(v["save_ms"], 210);
        assert_eq!(v["build_ms"], 95);
        assert_eq!(v["unattributed_ms"], 102);
        assert_eq!(v["total_ms"], 22_600);
        assert_eq!(v["scanned"], 7);
        assert_eq!(v["dirs"], 11);
    }

    /// 和が下回れば差が残余になり、上回れば 0 へ飽和する。`total` を部分和から作る実装では
    /// 前者が 0 になって落ちる。
    #[test]
    fn unattributed_is_the_remainder_and_saturates_at_zero() {
        assert_eq!(unattributed(&record(), ms(22_600)), ms(102));
        assert_eq!(unattributed(&record(), ms(1)), Duration::ZERO);
    }

    /// **丸めは表示境界だけ。** 各 500,000 ns の区間はミリ秒では 0 だが、総計 1 ms は 1 と出る。
    #[test]
    fn rounding_happens_only_at_the_display_boundary() {
        let half = Duration::from_nanos(500_000);
        let rec = ScanRecord {
            scan: half,
            sort: half,
            ..ScanRecord::default()
        };
        let v = parse(&end_line(0, "s", "x", &rec, ms(1)));
        assert_eq!(v["scan_ms"], 0);
        assert_eq!(v["sort_ms"], 0);
        assert_eq!(v["unattributed_ms"], 0);
        assert_eq!(v["total_ms"], 1);
    }

    /// 書き手と読み手のキー名が一致する。
    #[test]
    fn last_completed_dirs_reads_what_end_line_writes() {
        let line = end_line(0, "s", "scan-b", &record(), ms(23_000));
        assert_eq!(last_completed_dirs(line.as_bytes(), "scan-b"), Some(11));
    }

    /// 手で書いた凍結の行（`end_line` を通さない）から読める。読み手が新しいキーを必須に
    /// すると、ここが落ちる。
    #[test]
    fn last_completed_dirs_reads_a_frozen_end_line() {
        let body = br#"{"ev":"end","ts_ms":1,"sid":"1-2-0","scan_id":"00000000000000ff","scan_ms":1,"sort_ms":1,"save_ms":1,"build_ms":1,"unattributed_ms":0,"total_ms":4,"scanned":3,"dirs":42}
"#;
        assert_eq!(last_completed_dirs(body, "00000000000000ff"), Some(42));
    }

    #[test]
    fn last_completed_dirs_takes_the_last_matching_end_line() {
        let body = b"{\"ev\":\"end\",\"scan_id\":\"a\",\"dirs\":10}\n\
{\"ev\":\"end\",\"scan_id\":\"a\",\"dirs\":20}\n";
        assert_eq!(last_completed_dirs(body, "a"), Some(20));
    }

    /// 末尾側に置いた「飛ばすべき行」を全部飛ばして、その前の完走の行に届く。
    #[test]
    fn last_completed_dirs_skips_lines_that_are_not_a_usable_completion() {
        let mut body = b"{\"ev\":\"end\",\"scan_id\":\"a\",\"dirs\":10}\n".to_vec();
        body.extend_from_slice(b"{\"ev\":\"start\",\"scan_id\":\"a\",\"expected\":10}\n");
        body.extend_from_slice(b"{\"ev\":\"end\",\"scan_id\":\"other\",\"dirs\":99}\n");
        body.extend_from_slice(b"{\"ev\":\"end\",\"scan_id\":\"a\",\"dirs\":null}\n");
        body.extend_from_slice(b"{\"ev\":\"end\",\"scan_id\":\"a\",\"dirs\":0}\n");
        body.extend_from_slice(b"{\"ev\":\"end\",\"scan_id\":\"a\",\"di\n");
        body.extend_from_slice(b"\xff\xfe not utf-8\n");
        assert_eq!(last_completed_dirs(&body, "a"), Some(10));
    }

    #[test]
    fn last_completed_dirs_is_none_without_a_completion() {
        assert_eq!(last_completed_dirs(b"", "a"), None);
        assert_eq!(
            last_completed_dirs(b"{\"ev\":\"start\",\"scan_id\":\"a\"}\n", "a"),
            None
        );
    }

    #[test]
    fn append_in_creates_the_file_and_writes_one_line_each() {
        let dir = temp_dir("append");
        append_in(&dir, "{\"n\":1}");
        append_in(&dir, "{\"n\":2}");
        assert_eq!(lines_of(&dir), vec!["{\"n\":1}", "{\"n\":2}"]);
        let _ = fs::remove_dir_all(&dir);
    }

    /// kill で行の途中が切れたファイルへ追記しても、新しい行は独立した 1 行として読める。
    #[test]
    fn append_in_separates_a_line_cut_off_by_a_kill() {
        let dir = temp_dir("append_cut");
        fs::write(log_path_in(&dir), b"{\"ev\":\"start\",\"sc").expect("write");
        append_in(&dir, "{\"n\":2}");
        assert_eq!(lines_of(&dir), vec!["{\"ev\":\"start\",\"sc", "{\"n\":2}"]);
        let _ = fs::remove_dir_all(&dir);
    }

    /// fixture は [`MAX_LINES`] を跨ぐ。先頭を残す実装では最新が消えて落ちる。読めない行
    /// （非 UTF-8 を含む）も 1 行と数え、全体が読めなくならない。
    #[test]
    fn prune_in_keeps_the_newest_lines_and_counts_unreadable_ones() {
        let dir = temp_dir("prune");
        let mut body = b"\xff\xfe broken\n".to_vec();
        for i in 0..MAX_LINES + 5 {
            body.extend_from_slice(format!("{{\"n\":{i}}}\n").as_bytes());
        }
        fs::write(log_path_in(&dir), &body).expect("write");

        prune_in(&dir);

        let lines = lines_of(&dir);
        assert_eq!(lines.len(), MAX_LINES);
        assert_eq!(lines[0], "{\"n\":5}");
        assert_eq!(lines[MAX_LINES - 1], format!("{{\"n\":{}}}", MAX_LINES + 4));
        assert!(!dir.join("scan-log.jsonl.tmp").exists(), "tmp が残っている");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn prune_in_leaves_a_short_file_alone() {
        let dir = temp_dir("prune_short");
        fs::write(log_path_in(&dir), b"{\"n\":1}").expect("write");
        prune_in(&dir);
        assert_eq!(fs::read(log_path_in(&dir)).expect("read"), b"{\"n\":1}");
        let _ = fs::remove_dir_all(&dir);
    }

    /// 末尾が `\n` で終わらない最終行を持つファイルを剪定してから追記しても、新しい行が
    /// 独立した 1 行として読める。
    #[test]
    fn append_after_pruning_a_file_whose_last_line_was_cut() {
        let dir = temp_dir("prune_cut");
        let mut body = Vec::new();
        for i in 0..MAX_LINES + 1 {
            body.extend_from_slice(format!("{{\"n\":{i}}}\n").as_bytes());
        }
        body.extend_from_slice(b"{\"cut");
        fs::write(log_path_in(&dir), &body).expect("write");

        prune_in(&dir);
        append_in(&dir, "{\"n\":\"new\"}");

        let lines = lines_of(&dir);
        assert_eq!(lines.last().map(String::as_str), Some("{\"n\":\"new\"}"));
        assert_eq!(lines[lines.len() - 2], "{\"cut");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn writers_and_readers_do_not_panic_on_a_missing_directory() {
        let dir = std::env::temp_dir().join(format!(
            "snotra-scan-log-missing-{}/nested",
            std::process::id()
        ));
        append_in(&dir, "{}");
        prune_in(&dir);
        assert_eq!(begin_in(&dir, 0, "s", "a", 1), None);
    }

    /// 返り値の分母と、追記された `start` 行の `expected` が一致する。上限を超えるファイルでは
    /// 剪定の後に追記される（追記の後に剪定すると、行数が上限で止まらず落ちる）。
    #[test]
    fn begin_in_prunes_then_reads_the_denominator_then_appends_start() {
        let dir = temp_dir("begin");
        let mut body = Vec::new();
        body.extend_from_slice(end_line(0, "old", "a", &record(), ms(23_000)).as_bytes());
        body.push(b'\n');
        for i in 0..MAX_LINES {
            body.extend_from_slice(format!("{{\"n\":{i}}}\n").as_bytes());
        }
        fs::write(log_path_in(&dir), &body).expect("write");

        // 剪定が先なので、最古の `end` 行は消えてから分母を読む → 分母は無し。
        assert_eq!(begin_in(&dir, 5, "new", "a", 2), None);
        let lines = lines_of(&dir);
        assert_eq!(lines.len(), MAX_LINES + 1);
        let start = parse(lines.last().expect("start line"));
        assert_eq!(start["ev"], "start");
        assert!(start["expected"].is_null());

        // 短いファイルでは `end` 行が残り、分母になる。
        let dir2 = temp_dir("begin_short");
        fs::write(
            log_path_in(&dir2),
            format!("{}\n", end_line(0, "old", "a", &record(), ms(23_000))),
        )
        .expect("write");
        assert_eq!(begin_in(&dir2, 5, "new", "a", 2), Some(11));
        let start = parse(lines_of(&dir2).last().expect("start line"));
        assert_eq!(start["expected"], 11);
        assert_eq!(start["sid"], "new");

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&dir2);
    }

    #[test]
    fn new_sid_differs_between_consecutive_calls() {
        assert_ne!(new_sid(), new_sid());
    }
}
