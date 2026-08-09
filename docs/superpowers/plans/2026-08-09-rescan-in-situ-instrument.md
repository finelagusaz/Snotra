# 背景再スキャンの in-situ 計器 実装計画（#1001 受け入れ 1）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 背景再スキャンが毎起動「いつ始まり・何を返し・完走したか・各区間に何ミリ秒かけたか」を、実運用の起動でも後から読める形で `%APPDATA%/Snotra/rescan-log.jsonl` に残す。

**Architecture:** `snotra-core/src/rescan_log.rs` を新設し、(1) 時計も I/O も持たない純粋核（記録 → JSON 行）と (2) best-effort の追記・剪定に分ける。`indexer::try_background_rescan_in` が走査の**前**に `start` 行、終端で `end` 行を書く。start だけ在って end が無い行が「完走しなかった起動」を表す。

**Tech Stack:** Rust 2024 / `serde_json`（本計画で snotra-core へ追加）/ `chrono`（既存依存）/ `std::time::{Duration, Instant}`

**設計の正本:** `docs/superpowers/specs/2026-08-09-rescan-in-situ-instrument-design.md`

## Global Constraints

- **挙動を変えない。** 再スキャンの頻度・条件・結果・`RescanOutcome` の値は一切変えない。足すのは記録だけである
- **best-effort。** 記録の失敗（ディレクトリ不在・権限・ロック）は握り潰す。**計器のために製品を落とさない**（release は `panic = "abort"`）
- **通らなかった区間は `null`。** `0` にしない（「0 ミリ秒で書いた」と「書かなかった」は別である）
- **`total` は終端で 1 回読み、部分和から作らない。** 残余 `unattributed` を項目に置き、恒等式 `total = scan + sort + digest + save + unattributed` を**生の `Duration` で**全経路成立させる。**ms 表示の和で恒等式を検査してはならない**（丸めでずれる。`startup.rs` の `rounding_happens_only_at_the_display_boundary` と同じ理由）
- **`-D warnings` で通す。** `cargo clippy --workspace --all-targets -- -D warnings` が PostToolUse hook とタスク末で走る。未使用の項目は落ちるので、`rescan_log` の項目は `pub`（lib crate の公開 API）にする——**これは意図的である**: タスク間で未使用にならず、`tests/` からも読める
- **ファイル名は `rescan-log.jsonl`。** `%APPDATA%/Snotra` の他ファイルが `.bin` / `.toml` なので、拡張子そのものが「格が違う物体」の合図になる
- **コミットは各タスク末。** `main` へ直接コミットしない（本作業のブランチは `feat/rescan-in-situ-instrument`）

---

## File Structure

| ファイル | 責務 | タスク |
|---|---|---|
| `snotra-core/src/rescan_log.rs`（新規） | 記録の語彙・JSON 行の組み立て（純粋核）・追記・剪定 | 1, 2 |
| `snotra-core/src/lib.rs` | `pub mod rescan_log;` の宣言 | 1 |
| `snotra-core/Cargo.toml` | `serde_json` 追加 | 1 |
| `snotra-core/src/indexer.rs` | `try_background_rescan_in` への結線・`cached` 件数の運搬 | 3 |
| `snotra-core/tests/memory_footprint.rs` | `report_scan_all_cost` を経路全体へ拡張 | 5 |
| `SPEC.md` / `snotra-core/CLAUDE.md` / `docs/build-commands.md` | 文書同期 | 6 |

---

### Task 1: 記録の語彙と JSON 行（純粋核）

時計も I/O も持たない部分をここで完成させる。**時計を持たないのは測るためである**——丸め境界のような fixture は注入でしか作れない。

**Files:**
- Create: `snotra-core/src/rescan_log.rs`
- Modify: `snotra-core/src/lib.rs`（`pub mod rescan_log;` を alphabetical 順で `query` の後・`search` の前に挿入）
- Modify: `snotra-core/Cargo.toml`（`serde_json` を依存へ追加）
- Test: `snotra-core/src/rescan_log.rs` の `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: なし
- Produces:
  - `pub enum LoggedOutcome { SkippedLock, SkippedGeneration, Unchanged, Changed }`
  - `pub struct RescanRecord { pub scan: Option<Duration>, pub sort: Option<Duration>, pub digest: Option<Duration>, pub save: Option<Duration>, pub scanned: Option<usize>, pub format_upgrade: bool }`（`Default` を導出）
  - `pub fn start_line(ts: &str, sid: &str, roots: usize, cached: usize, cached_version: u32) -> String`
  - `pub fn end_line(ts: &str, sid: &str, outcome: LoggedOutcome, rec: &RescanRecord, total: Duration) -> String`
  - `pub fn unattributed(rec: &RescanRecord, total: Duration) -> Duration`

- [ ] **Step 1: `serde_json` を依存へ足す**

`snotra-core/Cargo.toml` の `[dependencies]` に 1 行足す（`serde` の直後）:

```toml
serde_json = "1"
```

**手で JSON を組み立てない。** 今の行に載る文字列はすべて自前の閉じた集合だが、フィールドを 1 つ足した瞬間にエスケープが要る値が入りうる。`serde_json` は workspace に既に入っている（`src-tauri` が `trace.rs` で使う）ので、ビルド時間は増えない。

- [ ] **Step 2: 失敗するテストを書く**

`snotra-core/src/rescan_log.rs` を作り、**まずテストだけ**を書く（実装は Step 4）。

```rust
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
        let v = parse(&start_line("2026-08-09T13:32:01.412Z", "19212-1786", 4, 312_625, 7));
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
        let v = parse(&end_line("t", "sid", LoggedOutcome::Changed, &rec, ms(22_420)));
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
        let gen = parse(&end_line("t", "s", LoggedOutcome::SkippedGeneration, &rec, ms(1)));
        assert_eq!(lock["outcome"], "skipped");
        assert_eq!(gen["outcome"], "skipped");
        assert_eq!(lock["skip_reason"], "lock");
        assert_eq!(gen["skip_reason"], "generation");
        assert_ne!(lock["skip_reason"], gen["skip_reason"]);
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
        assert_eq!(unattributed(&rec, total), total, "全区間が null なら残余が総計そのもの");
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
        assert_eq!(before, pairs.len(), "(outcome, skip_reason) の組が衝突している");
        assert_eq!(LoggedOutcome::Changed.outcome(), "changed");
        assert_eq!(LoggedOutcome::Unchanged.outcome(), "unchanged");
    }
}
```

- [ ] **Step 3: テストが落ちることを確認する**

Run: `cargo test -p snotra-core rescan_log`
Expected: **コンパイルエラー**（`start_line` / `end_line` / `unattributed` / `to_ms` / `LoggedOutcome` / `RescanRecord` が未定義）

- [ ] **Step 4: 最小の実装を書く**

`snotra-core/src/rescan_log.rs` の `mod tests` の**上**に置く:

```rust
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
```

`snotra-core/src/lib.rs` に 1 行足す（`pub mod query;` の次の行）:

```rust
pub mod rescan_log;
```

- [ ] **Step 5: テストが通ることを確認する**

Run: `cargo test -p snotra-core rescan_log`
Expected: PASS（9 テスト）

- [ ] **Step 6: lint と整形**

Run: `cargo fmt --all` → `cargo clippy --workspace --all-targets -- -D warnings`
Expected: 警告なし

- [ ] **Step 7: コミット**

コミットメッセージは Write ツールで一時ファイルへ書いて `-F` で渡す（**bash の HEREDOC は Windows で壊れる**）。

```bash
git add snotra-core/src/rescan_log.rs snotra-core/src/lib.rs snotra-core/Cargo.toml Cargo.lock
git commit -F <path>
```

件名: `feat(core): 再スキャン記録の純粋核（語彙・JSON 行・恒等式）`

---

### Task 2: 追記と剪定（best-effort の書き手）

**Files:**
- Modify: `snotra-core/src/rescan_log.rs`（純粋核の下に I/O を足す）
- Test: 同ファイルの `mod tests`

**Interfaces:**
- Consumes: Task 1 の `start_line` / `end_line`
- Produces:
  - `pub const MAX_LINES: usize = 200;`
  - `pub fn log_path_in(dir: &Path) -> PathBuf`
  - `pub fn append_in(dir: &Path, line: &str)`
  - `pub fn prune_in(dir: &Path)`
  - `pub fn new_sid() -> String`
  - `pub fn now_rfc3339() -> String`

- [ ] **Step 1: 失敗するテストを書く**

`mod tests` に足す（`use std::fs;` と `use std::path::PathBuf;` を tests の先頭に足す）:

```rust
    /// テスト用の一時ディレクトリ。**プロセス ID とテスト名で分ける**（並列実行で衝突しない）。
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "snotra-rescanlog-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir を作れること");
        dir
    }

    fn lines_in(dir: &Path) -> Vec<String> {
        fs::read_to_string(log_path_in(dir))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn append_creates_the_file_and_adds_one_line_per_call() {
        let dir = temp_dir("append");
        append_in(&dir, "a");
        append_in(&dir, "b");
        assert_eq!(lines_in(&dir), vec!["a", "b"]);
        let _ = fs::remove_dir_all(&dir);
    }

    /// **書けなくても製品は止まらない。** 存在しないディレクトリへの追記は黙って捨てる。
    #[test]
    fn append_is_best_effort_when_the_directory_does_not_exist() {
        let dir = std::env::temp_dir().join("snotra-rescanlog-does-not-exist-xyz");
        let _ = fs::remove_dir_all(&dir);
        append_in(&dir, "a"); // panic しないこと
        assert!(!log_path_in(&dir).exists());
    }

    /// **剪定は最新を残す。** 変異: 末尾ではなく先頭 N 行を残す実装にすると落ちる。
    #[test]
    fn prune_keeps_the_newest_lines_not_the_oldest() {
        let dir = temp_dir("prune_newest");
        for i in 0..(MAX_LINES + 50) {
            append_in(&dir, &format!("line{i}"));
        }
        prune_in(&dir);
        let lines = lines_in(&dir);
        assert_eq!(lines.len(), MAX_LINES, "上限まで縮む");
        assert_eq!(
            lines.last().map(String::as_str),
            Some(format!("line{}", MAX_LINES + 49).as_str()),
            "最新が残る"
        );
        assert_eq!(
            lines.first().map(String::as_str),
            Some(format!("line{}", 50).as_str()),
            "古い 50 行が落ちる"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn prune_does_nothing_below_the_limit() {
        let dir = temp_dir("prune_below");
        append_in(&dir, "only");
        prune_in(&dir);
        assert_eq!(lines_in(&dir), vec!["only"]);
        let _ = fs::remove_dir_all(&dir);
    }

    /// **壊れた行があっても剪定は続く。** 変異: parse 失敗で早期 return する実装にすると
    /// 落ちる——この物体は JSON として読めない行が混ざっても機能し続けなければならない
    /// （2 プロセスの追記が競合した残骸がありうる）。
    #[test]
    fn prune_survives_lines_that_are_not_json() {
        let dir = temp_dir("prune_garbage");
        for i in 0..(MAX_LINES + 10) {
            append_in(&dir, if i % 7 == 0 { "}{ broken" } else { "{\"ev\":\"x\"}" });
        }
        prune_in(&dir);
        assert_eq!(lines_in(&dir).len(), MAX_LINES);
        let _ = fs::remove_dir_all(&dir);
    }

    /// `sid` は同一プロセス内で安定し、pid を含む（読み手がプロセスを辿れる）。
    #[test]
    fn sid_contains_the_pid() {
        let sid = new_sid();
        assert!(
            sid.starts_with(&format!("{}-", std::process::id())),
            "sid={sid}"
        );
    }

    #[test]
    fn now_rfc3339_is_parseable_and_utc() {
        let ts = now_rfc3339();
        assert!(ts.ends_with('Z'), "UTC で出す: {ts}");
        chrono::DateTime::parse_from_rfc3339(&ts).expect("RFC3339 として読めること");
    }
```

- [ ] **Step 2: テストが落ちることを確認する**

Run: `cargo test -p snotra-core rescan_log`
Expected: **コンパイルエラー**（`append_in` / `prune_in` / `log_path_in` / `MAX_LINES` / `new_sid` / `now_rfc3339` が未定義）

- [ ] **Step 3: 実装を書く**

`rescan_log.rs` の純粋核の下（`mod tests` の上）に足す。ファイル先頭の `use` に `std::path::{Path, PathBuf}` を加える。

```rust
/// 保つ行数の上限。1 行 200 B 程度なので約 40 KB。**全読みして書き直しても安い。**
pub const MAX_LINES: usize = 200;

/// 記録の在り処。**`dir` は `Config::config_dir()` から来る**——保存先を導く経路は
/// あの 1 つだけであり、`SNOTRA_CONFIG_DIR` もそこで効く。
pub fn log_path_in(dir: &Path) -> PathBuf {
    dir.join("rescan-log.jsonl")
}

/// 1 行追記する。**失敗は握り潰す**（ディレクトリ不在・権限・ロック）。
///
/// **計器のために製品を落とさない**——release は `panic = "abort"` ゆえ、ここでの
/// `unwrap` はプロセスの死に直結する。
pub fn append_in(dir: &Path, line: &str) {
    use std::io::Write;
    let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path_in(dir))
    else {
        return;
    };
    // 1 回の write で改行まで出す。**行の途中で他プロセスに割り込まれる窓を狭める**
    // （Windows の追記は小さい書き込みなら原子的に扱われる）。
    let _ = f.write_all(format!("{line}\n").as_bytes());
}

/// 上限を超えていたら**末尾** `MAX_LINES` 行を残して書き直す。`start` を書く前に 1 回だけ呼ぶ。
///
/// **最新を残す**（古い方を捨てる）。逆にすると、読みたい直近の起動が真っ先に消える。
///
/// **JSON として読めない行も 1 行として数える。** この物体は壊れた行が混ざっても
/// 機能し続けねばならない（2 プロセスの追記が競合した残骸がありうる）。
///
/// **受容する残余**: 2 プロセスが同時にここへ来ると行が失われうる。読み→書き直しの
/// 間にロックを置かないためで、**使い捨ての物体ゆえ許容する**。
pub fn prune_in(dir: &Path) {
    let path = log_path_in(dir);
    let Ok(body) = std::fs::read_to_string(&path) else {
        return;
    };
    let lines: Vec<&str> = body.lines().collect();
    if lines.len() <= MAX_LINES {
        return;
    }
    let kept = &lines[lines.len() - MAX_LINES..];
    let mut out = kept.join("\n");
    out.push('\n');
    let _ = std::fs::write(&path, out);
}

/// この 1 回の再スキャンを指す識別子。**pid だけでは再利用で衝突する**ので起動時刻を混ぜる
/// （2 プロセスの同時稼働は実在する・2026-08-09 実測）。
pub fn new_sid() -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{}-{}", std::process::id(), ms)
}

/// 記録の時刻。**UTC の RFC3339（ミリ秒まで）**。読み手は人間なので文字列で出す。
pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cargo test -p snotra-core rescan_log`
Expected: PASS（16 テスト）

- [ ] **Step 5: lint と整形**

Run: `cargo fmt --all` → `cargo clippy --workspace --all-targets -- -D warnings`

- [ ] **Step 6: コミット**

件名: `feat(core): 再スキャン記録の追記と剪定（best-effort・最新を残す）`

---

### Task 3: `try_background_rescan_in` への結線

ここで初めて製品の経路が記録を書く。**挙動は変えない**——`RescanOutcome` の値も分岐も、1 つも変わらない。

**Files:**
- Modify: `snotra-core/src/indexer.rs`
  - `BackgroundRescanTask` に `cached_len: usize` を足す
  - `load_or_scan_with_stats` の構築点（`rescan_task` を組む箇所）で `cached_len` を渡す
  - `try_background_rescan` / `try_background_rescan_in` の引数へ `cached_len` を通す
  - `try_background_rescan_in` の本体に計測と 2 行の書き出しを入れる
  - 既存のテスト呼び出し 4 か所（`try_background_rescan_in(...)`）へ引数を足す
- Test: `snotra-core/src/indexer.rs` の `mod tests`

**Interfaces:**
- Consumes: `rescan_log::{LoggedOutcome, RescanRecord, append_in, prune_in, start_line, end_line, new_sid, now_rfc3339, log_path_in}`
- Produces: `try_background_rescan_in(dir, scan, show_hidden_system, config_hash, cached_digest, generation, cached_version, cached_len) -> RescanOutcome`（引数が 1 つ増える。返り値の型と意味は不変）

- [ ] **Step 1: 失敗するテストを書く**

`indexer.rs` の `mod tests` に足す（`use crate::rescan_log;` を tests の先頭に足す）:

```rust
    /// 記録の行を読む小道具。
    fn rescan_log_lines(dir: &Path) -> Vec<serde_json::Value> {
        std::fs::read_to_string(rescan_log::log_path_in(dir))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }

    /// **走査を始める前に `start` が出る。** 変異: `start` を書かず終端だけ書く実装に
    /// すると、この検査が落ちる——それは「完走しなかった起動」を観測不能にする変更である。
    #[test]
    fn background_rescan_writes_start_before_scanning_and_end_at_the_terminal() {
        let _serial = INDEX_LOCK_TEST_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = temp_dir("rescan_log_pair");
        let scan: Vec<ScanPath> = Vec::new();
        save_cache_sorted_in(&dir, Vec::new(), compute_config_hash(&scan, false));

        let outcome = try_background_rescan_in(
            &dir,
            &scan,
            false,
            compute_config_hash(&scan, false),
            entries_digest(&[]),
            current_index_generation(),
            INDEX_CACHE_VERSION,
            0,
        );
        assert_eq!(outcome, RescanOutcome::Unchanged);

        let lines = rescan_log_lines(&dir);
        assert_eq!(lines.len(), 2, "start と end の 2 行が出る");
        assert_eq!(lines[0]["ev"], "start");
        assert_eq!(lines[1]["ev"], "end");
        assert_eq!(lines[0]["sid"], lines[1]["sid"], "同じ sid で組になる");
        assert_eq!(lines[1]["outcome"], "unchanged");
        assert!(lines[1]["save_ms"].is_null(), "書かなかったので null");
        assert!(!lines[1]["scan_ms"].is_null(), "走査はした");

        let _ = fs::remove_dir_all(&dir);
    }

    /// **ロック競合でも `end` を書く。** 終端は 1 か所ではない——`Skipped` の経路で
    /// 黙ると、「起動したが再スキャンは走らなかった」が読めなくなる。
    #[test]
    fn background_rescan_records_the_skip_when_the_write_lock_is_held() {
        let _serial = INDEX_LOCK_TEST_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = temp_dir("rescan_log_skip_lock");
        let guard = INDEX_WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let outcome = try_background_rescan_in(
            &dir,
            &[],
            false,
            0,
            0,
            current_index_generation(),
            INDEX_CACHE_VERSION,
            0,
        );
        drop(guard);
        assert_eq!(outcome, RescanOutcome::Skipped);

        let lines = rescan_log_lines(&dir);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1]["outcome"], "skipped");
        assert_eq!(lines[1]["skip_reason"], "lock", "世代不一致と区別する");
        assert!(lines[1]["scan_ms"].is_null(), "走査していない");

        let _ = fs::remove_dir_all(&dir);
    }

    /// 世代不一致の skip は `lock` と別の理由として出る。
    #[test]
    fn background_rescan_records_the_skip_when_the_generation_moved() {
        let _serial = INDEX_LOCK_TEST_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = temp_dir("rescan_log_skip_generation");

        let stale_generation = current_index_generation().wrapping_sub(1);
        let outcome = try_background_rescan_in(
            &dir,
            &[],
            false,
            0,
            0,
            stale_generation,
            INDEX_CACHE_VERSION,
            0,
        );
        assert_eq!(outcome, RescanOutcome::Skipped);

        let lines = rescan_log_lines(&dir);
        assert_eq!(lines[1]["skip_reason"], "generation");

        let _ = fs::remove_dir_all(&dir);
    }

    /// **`cached` は索引のエントリ件数である。** 木の節点数と食い違えば、読み手は
    /// 「何件から何件へ変わったか」を誤読する。
    #[test]
    fn tree_len_is_the_entry_count() {
        let entries = vec![
            AppEntry {
                name: "A".into(),
                target_path: "C:\\a.txt".into(),
                is_folder: false,
            },
            AppEntry {
                name: "B".into(),
                target_path: "C:\\dir\\b.txt".into(),
                is_folder: false,
            },
        ];
        let n = entries.len();
        let dir = temp_dir("tree_len_is_entry_count");
        let (tree, _) = save_cache_sorted_in(&dir, entries, 0);
        assert_eq!(tree.len(), n, "木の len は索引のエントリ件数と一致する");
        let _ = fs::remove_dir_all(&dir);
    }
```

- [ ] **Step 2: テストが落ちることを確認する**

Run: `cargo test -p snotra-core background_rescan_writes_start`
Expected: **コンパイルエラー**（`try_background_rescan_in` の引数が 7 個しかない）

- [ ] **Step 3: `cached_len` を運ぶ**

`indexer.rs` の `BackgroundRescanTask` にフィールドを足す（`cached_version` の下）:

```rust
    /// ロード時点の索引のエントリ件数。**記録の `cached` に出す**——結末が `Changed` の
    /// とき「何件から何件へ変わったか」を読み手が追えるようにするため。
    /// **digest だけを持つ設計（反復 6）は壊さない**——増えるのは `usize` 1 つである。
    cached_len: usize,
```

`load_or_scan_with_stats` のキャッシュヒット枝、`rescan_task` を組む箇所に 1 行足す（`cached_version,` の隣）:

```rust
            cached_len: material.tree().len(),
```

`BackgroundRescanTask::run` の `try_background_rescan(...)` 呼び出しに `self.cached_len,` を足し、`try_background_rescan` の引数にも `cached_len: usize` を足して `try_background_rescan_in` へ渡す。

- [ ] **Step 4: `try_background_rescan_in` に計測と記録を入れる**

本体を次で置き換える（**分岐と返り値は 1 つも変えていない**——足したのは計測と 2 行の書き出しだけ）:

```rust
#[allow(clippy::too_many_arguments)]
fn try_background_rescan_in(
    dir: &Path,
    scan: &[ScanPath],
    show_hidden_system: bool,
    config_hash: u64,
    cached_digest: u64,
    generation: u64,
    cached_version: u32,
    cached_len: usize,
) -> RescanOutcome {
    // **`start` は走査の前に書く。** 終端だけを書く形では、走査より短いセッション
    // （実測 12 秒 < 22 秒）が「そもそも起動しなかった」と区別できない（#1001）。
    let started = Instant::now();
    let sid = rescan_log::new_sid();
    rescan_log::prune_in(dir);
    rescan_log::append_in(
        dir,
        &rescan_log::start_line(
            &rescan_log::now_rfc3339(),
            &sid,
            scan.len(),
            cached_len,
            cached_version,
        ),
    );

    let mut rec = rescan_log::RescanRecord::default();

    // 権威的ビルド（rebuild_and_save / cache-miss save）が書き込み中なら Skipped。
    let changed = try_with_index_write_lock(|| {
        // 世代検査は書き込みロック取得後に行う。検査後に権威的ビルドが割り込む
        // TOCTOU を防ぎ、古い snapshot が新しい index.bin を巻き戻さないため。
        if current_index_generation() != generation {
            return None;
        }
        let t = Instant::now();
        let mut scanned = scan_all(scan, show_hidden_system);
        rec.scan = Some(t.elapsed());
        rec.scanned = Some(scanned.len());

        let t = Instant::now();
        sort_entries_canonical(&mut scanned);
        rec.sort = Some(t.elapsed());

        let t = Instant::now();
        let changed = entries_digest(&scanned) != cached_digest;
        rec.digest = Some(t.elapsed());

        // **書く条件は 2 つある。** 中身が変わったとき（従来）と、**読めた形式が旧版のとき**。
        // 後者を欠くと、索引の中身が変わらない限り旧版が何日でも残り、そのユーザーは
        // 新形式の削減を**永久に受け取らない**（2026-08-07 に実運用点で実測。詳細は
        // `background_rescan_upgrades_stale_format_when_entries_are_unchanged`）。
        //
        // 昇格をこの経路に置くのは、ここが **`sort_entries_canonical` を通した自前の
        // 走査結果を既に持っている唯一の場所**だからである。ロード側で書こうとすると、
        // engine へ move する `entries` の複製が要り、反復 6 で消した 62.5 MiB が復活する。
        let stale_format = cached_version != INDEX_CACHE_VERSION;
        if changed || stale_format {
            // **中身が変わっていないのに書いた**ことを記録へ残す。この経路は
            // `outcome=unchanged` なのに `save_ms` が非 null になる唯一の理由である。
            rec.format_upgrade = !changed && stale_format;
            let t = Instant::now();
            // **返る木と `CachedMasks` はここでは捨てる。** 背景再スキャンは索引を建てない
            // ——建てるのは呼び出し側（`Changed` を見た `src-tauri` が再構築を kick する）で
            // あり、ここが抱えると起動段の外に索引 1 本ぶんの常駐が生まれる。
            drop(save_cache_sorted_in(dir, scanned, config_hash));
            rec.save = Some(t.elapsed());
        }
        Some(changed)
    });
    let (outcome, logged) = match changed {
        None => (RescanOutcome::Skipped, rescan_log::LoggedOutcome::SkippedLock),
        Some(None) => (
            RescanOutcome::Skipped,
            rescan_log::LoggedOutcome::SkippedGeneration,
        ),
        Some(Some(false)) => (
            RescanOutcome::Unchanged,
            rescan_log::LoggedOutcome::Unchanged,
        ),
        Some(Some(true)) => (RescanOutcome::Changed, rescan_log::LoggedOutcome::Changed),
    };
    // **`total` は終端で 1 回読む。** 部分和から作らない（作れば検算が同語反復になる）。
    rescan_log::append_in(
        dir,
        &rescan_log::end_line(
            &rescan_log::now_rfc3339(),
            &sid,
            logged,
            &rec,
            started.elapsed(),
        ),
    );
    outcome
}
```

ファイル先頭の `use` に `use crate::rescan_log;` を足す。`Instant` は既に import 済み。

- [ ] **Step 5: 既存のテスト呼び出しへ引数を足す**

`try_background_rescan_in(` の呼び出しを grep し、既存 4 か所すべてに末尾引数 `0,`（`cached_len`）を足す。

Run: `grep -n "try_background_rescan_in(" snotra-core/src/indexer.rs`

- [ ] **Step 6: テストが通ることを確認する**

Run: `cargo test -p snotra-core`
Expected: PASS（既存テストも全部通ること。**`RescanOutcome` の値を変えていないので、既存の再スキャンテストは 1 つも書き換えなくてよい**——書き換えが必要になったら、それは挙動を変えてしまった合図である）

- [ ] **Step 7: lint と整形**

Run: `cargo fmt --all` → `cargo clippy --workspace --all-targets -- -D warnings` → `cargo doc --workspace --no-deps --document-private-items`

（`cargo doc` は intra-doc link 切れを捕まえる。**PostToolUse hook は沈黙するので手で走らせる**）

- [ ] **Step 8: コミット**

件名: `feat(core): 背景再スキャンが 2 行 1 組で結末と完走を記録する（#1001）`

---

### Task 4: 実機ゲート（新設した経路を自分で走らせる）

**新設した検証経路は、報告の前にその経路自体を走らせる。** 単体テストが緑でも、実運用点で 1 行も出ない可能性は残る（`Config::config_dir()` の導出・release ビルド・別スレッド）。

**Files:**
- 変更なし（観測のみ）

**Interfaces:**
- Consumes: Task 3 の実装
- Produces: なし（判定の記録は PR 本文へ）

- [ ] **Step 1: release をビルドする**

Run: `cargo build --release -p snotra`

- [ ] **Step 2: 通常起動 → `end` 行が出ることを確かめる**

```powershell
Remove-Item "$env:APPDATA/Snotra/rescan-log.jsonl" -ErrorAction SilentlyContinue
$p = Start-Process 'C:/workspace/Snotra/target/release/snotra.exe' -PassThru
Start-Sleep -Seconds 60
Stop-Process -Id $p.Id -Force
Get-Content "$env:APPDATA/Snotra/rescan-log.jsonl"
```

Expected: `start` 行と `end` 行が 1 組ずつ。`end` の `outcome` は `unchanged` か `changed`、`scan_ms` は 20,000 前後（**環境で数倍ぶれる。この秒数を恒久文書へ書かない**）。

- [ ] **Step 3: 短命セッション → `start` だけが残ることを確かめる**

```powershell
$p = Start-Process 'C:/workspace/Snotra/target/release/snotra.exe' -PassThru
Start-Sleep -Seconds 12
Stop-Process -Id $p.Id -Force
Get-Content "$env:APPDATA/Snotra/rescan-log.jsonl" -Tail 3
```

Expected: 最終行が `ev":"start"` で、対応する `end` が無い。**これが本計器の存在理由そのものであり、ここが緑にならなければ設計が実現していない。**

- [ ] **Step 4: 判定を PR 本文のチェックリストへ書く**

2 つの観測結果（行そのもの）を PR 本文へ貼る。**「確かめた」と書くのではなく、出力を貼る。**

---

### Task 5: harness を経路全体へ広げる

**Files:**
- Modify: `snotra-core/tests/memory_footprint.rs` の `report_scan_all_cost`
- Test: なし（`#[ignore]` の計測ハーネス自身が対象）

**Interfaces:**
- Consumes: `indexer::scan_all`（既存）
- Produces: なし

- [ ] **Step 1: 区間を広げる**

`report_scan_all_cost` の測定区間を `scan_all` だけから `sort_entries_canonical` + `entries_digest` まで広げる。**どちらも `pub(crate)` なので、テストからは呼べない**——`indexer` に計測用の `#[doc(hidden)] pub` な入口を足すのではなく、**`load_or_scan_with_stats` が既に測っている `sort_ms` / `digest_ms` を使う**。

すなわちこのタスクの実体は次の 2 つである:

1. `report_scan_all_cost` の doc に「**この区間は `scan_all` だけであり、`sort` / `digest` / `save` は含まない**」と明記し、**それぞれの測り手を名指しする**（`sort` / `digest` = Phase A の `LoadOrScanStats`、`save` = `cache_save_ms` と `rescan-log.jsonl` の `save_ms`）
2. 出力の末尾に「経路全体の内訳はどこを見るか」の 1 行を足す

```rust
    println!(
        "  ※ この区間は scan_all のみ。sort / digest は Phase A の LoadOrScanStats、\
         save は cache_save_ms と rescan-log.jsonl の save_ms が測る（#1001 受け入れ 1）"
    );
```

**なぜ「広げない」のか**: `sort_entries_canonical` と `entries_digest` を測るには可視性を緩めるか計測専用の入口を製品へ足すことになり、**#1000 が却下した「注入点を製品コードへ足す」と同型**である。しかも実運用の値は Task 3 の記録が毎起動出すので、**手元のハーネスで測り直す動機がそもそも消えている**。

- [ ] **Step 2: ハーネスを走らせて出力を確かめる**

Run: `cargo test --release -p snotra-core --test memory_footprint -- --ignored --nocapture --test-threads=1`
Expected: 既存の出力に加えて上記 1 行が出る。**`--test-threads=1` は外せない**（計数アロケータはプロセス大域）。

- [ ] **Step 3: コミット**

件名: `docs(core): scan_all 計測の射程と、経路の残りを誰が測るかを明記する`

---

### Task 6: 文書同期と governance

**Files:**
- Modify: `SPEC.md`（§13.4 を新設）
- Modify: `snotra-core/CLAUDE.md`（モジュール構成に 1 行）
- Modify: `docs/build-commands.md`（読み方）

- [ ] **Step 1: `SPEC.md` に §13.4 を足す**

`### 13.3 設定バックアップ` の**末尾**（`## 14. 実行仕様（起動）` の直前）に足す。**§13.2 の一覧へ足してはならない**——あの節は見出しが「アプリケーションデータ（**バイナリ**）」で、直下の共通保存仕様（magic + version ヘッダ / tmp→rename / 失敗時は再生成）と一体である。JSONL の追記ファイルはその 3 つのどれも満たさない。

```markdown
### 13.4 計器の記録（テキスト・使い捨て）

- `%APPDATA%\Snotra\rescan-log.jsonl`（JSON Lines）
- 背景再スキャン（§3.3）が 1 回につき 2 行（`start` / `end`）を追記する。**`start` は走査の前に書く**ため、`start` だけ在って `end` が無い行は「完走せずにプロセスが終わった起動」を表す
- **§13.2 の共通保存仕様は適用しない**（ヘッダを持たず、追記であり、原子的置換もしない）
- **消えても壊れても書けなくてもアプリの振る舞いは変わらない。** 読み手は人間と計測ハーネスであり、アプリはこのファイルを読まない
- 直近 200 行を保ち、超過分は古い方から捨てる
```

- [ ] **Step 2: `snotra-core/CLAUDE.md` のモジュール構成に 1 行足す**

`- `query.rs` — …` の次に:

```markdown
- `rescan_log.rs` — 背景再スキャンの in-situ 計器（責務は `//!`）。**アプリはこのファイルを読まない**——消えても壊れても振る舞いが変わらないので `binfmt` の版機構を持たず、**間引きの入力に兼ねさせてはならない**（計器が振る舞いを決める部品に変わると「永遠に間引かれて再スキャンが一度も走らない」が沈黙で起きる）
```

- [ ] **Step 3: `docs/build-commands.md` に読み方を足す**

「変更後の検証チェックリスト」とは別の、計測コマンドの節へ:

```powershell
Get-Content "$env:APPDATA/Snotra/rescan-log.jsonl" -Tail 20   # 背景再スキャンの記録（#1001・start だけの行 = 未完走の起動）
```

- [ ] **Step 4: governance:check を走らせる**

Run: `npm run governance:check`
Expected: 全検査 passed。**新規ファイルを含むブランチは PR 前にこれを走らせる**（#629/#630 で同型の索引更新漏れが再発している）

- [ ] **Step 5: コミット**

件名: `docs: rescan-log.jsonl を SPEC §13.4・モジュール索引・build-commands へ同期`

- [ ] **Step 6: PR を作る**

**`gh pr create` の前に push する**（未 push＝空 PR は pre-bash hook が拒む）。**鎖に `cd` を含めない。**

```bash
git push -u origin HEAD
```

PR 本文には Task 4 の 2 つの観測結果（行そのもの）を貼り、`Refs #1001` を書く。**`Closes #1001` にはしない**——本 issue は間引きと SPEC §3.3 の整合まで含んでおり、この反復は受け入れ 1 だけを満たす。

---

## Self-Review

**1. Spec coverage**

| spec の節 | 実装するタスク |
|---|---|
| §2.1 物体（`rescan_log.rs` / `config_dir` 派生） | Task 1（モジュール）・Task 2（`log_path_in`） |
| §2.2 JSONL であって binfmt でない | Task 1（`serde_json`）・Task 6（SPEC §13.4 の「共通保存仕様は適用しない」） |
| §2.3 2 行 1 組・sid・skip の 2 理由・`format_upgrade` | Task 1（語彙）・Task 3（結線） |
| §2.4 純粋核 | Task 1 |
| §3.1 null と 0 | Task 1（`segments_that_never_ran_are_null_not_zero`） |
| §3.2 恒等式と残余 | Task 1（`unattributed_closes_the_identity_in_raw_durations` / `identity_holds_on_the_skipped_path...`） |
| §3.3 丸めは表示境界 | Task 1（`to_ms_truncates_toward_zero`） |
| §3.4 best-effort | Task 2（`append_is_best_effort_when_the_directory_does_not_exist`） |
| §3.5 剪定 | Task 2（`prune_keeps_the_newest_lines_not_the_oldest`） |
| §4 受容する残余 | Task 1・2 の doc コメント（`config_dir` 不在 / 同時剪定） |
| §5 検知器と完了ゲート | Task 1・2・3 のテスト、Task 4 の実機ゲート |
| §6 計器の分担 | Task 5 |
| §7 文書更新 | Task 6 |

**ギャップ 1 件と、その解消**: spec §6 は `report_scan_all_cost` を「`sort` + `digest` まで広げる」と書いたが、両関数は `pub(crate)` で、広げるには可視性を緩めるか計測専用の入口を製品へ足すことになる——**#1000 が却下した「注入点を製品コードへ足す」と同型**である。しかも実運用の値は Task 3 の記録が毎起動出すので動機自体が消えている。**Task 5 を「射程を明記し、経路の残りを誰が測るか名指しする」へ変更した。** spec §6 の表（3 つの器で経路全体を覆う）はそのまま成立する。

**2. Placeholder scan**: TBD / TODO / 「適切なエラー処理を足す」の類は無し。全コードステップに実コードが入っている。

**3. Type consistency**: `LoggedOutcome` / `RescanRecord` のフィールド名は Task 1 の定義と Task 3 の使用で一致（`scan` / `sort` / `digest` / `save` / `scanned` / `format_upgrade`）。`try_background_rescan_in` の引数順は Task 3 の Step 4 と Step 1 のテスト呼び出しで一致（末尾が `cached_len`）。`log_path_in` は Task 2 で定義し Task 3 のテストで使用。

**4. 既知の注意点（実装者向け）**

- `#[allow(clippy::too_many_arguments)]` が要る（引数が 8 個になる）。**既に 7 個で境界を超えていた可能性があるので、まず `cargo clippy` を走らせて実際に警告が出るか確かめてから足す**——不要な `allow` は残さない
- **`Skipped` の 2 理由は既存の返り値の形がそのまま持っている**（`None` = ロック競合 / `Some(None)` = 世代不一致）。新しい判定を書き起こさず、その `match` を 2 つ組へ広げるだけでよい——別実装を書くと、`RescanOutcome` と記録が食い違う経路が生まれる
- **既存の再スキャンテストを 1 つも書き換えずに通ること**が「挙動を変えていない」の実質的な検査である。書き換えが要ったら、それは挙動を変えてしまった合図として扱う
