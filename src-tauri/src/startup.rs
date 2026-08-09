//! 起動の端から端まで（プロセス作成 → ホットキー登録完了）を刻む計器（issue #1000）。
//!
//! **常設の計器である**（`AGENTS.md`「条件別チェック」が撤去条件を要求する「調査・測定のための
//! 一時的な足場」ではない）。上流の改修の前後で同じ器を当てられることが存在理由であり、
//! issue が閉じても残る。
//!
//! # 基準点は 1 つ、以降は単調時計で刻む
//!
//! 壁時計の引き算は `pre_main`（プロセス作成 → `main()` 突入）の 1 か所だけに閉じる。
//! 全マークを `SystemTime` で取ると時刻補正・分解能の影響が全区間に乗るためである。
//! `main()` 突入時の [`Instant`] を anchor とし、以降の区間はすべてそこからの経過で刻む。
//!
//! # 丸めは表示境界でだけ行う
//!
//! 区間は生の [`Duration`] で保持し、`*_ms` は出力時にだけ切り捨てる。隣接区間を個別に
//! ミリ秒へ落とすと、タイミングが正しくても丸め境界で和が合わない（各 500,000 ns の 2 区間は
//! ミリ秒では 0 + 0 だが総計は 1）。**厳密に検算するのは生 ns だけである。**
//!
//! # 区間は網羅列挙する
//!
//! 出力は [`Phase`] の全 variant を必ず並べ、通らなかった区間は `null` として出す。
//! **累積タイムラインの区間和は telescoping sum であり、総和の検算では「マークを 1 つ
//! 落とす」誤りを原理的に検出できない**（落ちた区間の時間は隣へ吸収され、等式は崩れない）。
//! 取り落としを捕まえるのはキーの網羅であって総和ではない。
//!
//! # 終端は 1 か所ではない
//!
//! 終端（[`finish`]）は 3 か所から呼ばれる——`setup_platform_thread`（bridge の初期化失敗）・
//! `setup_hotkey_listener`（bridge state 不在・lock 失敗・初回 command の送信失敗）・
//! platform スレッドの `RegisterInitialHotkey` の arm（登録の成否）。**arm だけに閉じると、
//! arm 自体が実行されない失敗経路がハーネスの「タイムアウト」に化ける**——診断したい相手が
//! 読めなくなる。
//!
//! 一度きり性は [`FINISHED`] の CAS が持つ。**二重の守りではなく必須である**——platform の
//! 初期化に失敗した後、`setup_hotkey_listener` が bridge 不在をもう一度観測して二つ目の
//! 失敗行を出す経路が実在する。
//!
//! # 受容する残余
//!
//! `PlatformBridgePending::wait` の channel 切断が本番でどう起きるかは特定できていない
//! （`recv()` の失敗経路は実在するが、thread panic 等の原因は未確定）。**原因の特定を
//! 成立条件にしていない**——変異で経路を模擬し、終端が出ることだけを測る。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::json;

/// 起動経路の区間。**出力はこの全 variant を並べる**——`ALL` に足し忘れると
/// `all_covers_every_variant` が落ちる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    ConfigLoad,
    IndexLoad,
    PathMerge,
    HistoryLoad,
    EngineBuild,
    TauriInit,
    WindowsCreate,
    SetupRest,
    HotkeyRegister,
}

impl Phase {
    pub(crate) const ALL: [Phase; 9] = [
        Phase::ConfigLoad,
        Phase::IndexLoad,
        Phase::PathMerge,
        Phase::HistoryLoad,
        Phase::EngineBuild,
        Phase::TauriInit,
        Phase::WindowsCreate,
        Phase::SetupRest,
        Phase::HotkeyRegister,
    ];

    /// 出力の JSON キー（`*_ns` / `*_ms` の接頭辞）。
    pub(crate) fn key(self) -> &'static str {
        match self {
            Phase::ConfigLoad => "config_load",
            Phase::IndexLoad => "index_load",
            Phase::PathMerge => "path_merge",
            Phase::HistoryLoad => "history_load",
            Phase::EngineBuild => "engine_build",
            Phase::TauriInit => "tauri_init",
            Phase::WindowsCreate => "windows_create",
            Phase::SetupRest => "setup_rest",
            Phase::HotkeyRegister => "hotkey_register",
        }
    }

    fn index(self) -> usize {
        // `ALL` の並びが添字の正本である（`key` と同じく網羅 match で書けるが、
        // 並びを 2 か所へ写すと `ALL` を並べ替えたときに黙ってずれる）。
        Phase::ALL
            .iter()
            .position(|p| *p == self)
            .expect("Phase::ALL が全 variant を含む（all_covers_every_variant が守る）")
    }
}

/// 終端の分類。**イベント名がこの意味を運ぶ**——`data.ok` をハーネスが見忘れても
/// 沈黙で通らないようにするため、成功と失敗でイベント名そのものを変える。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartupFailure {
    /// platform スレッドの spawn に失敗した。
    PlatformSpawn,
    /// platform スレッドの Win32 初期化に失敗した（`GetModuleHandleW` / `CreateWindowExW`）。
    PlatformInit,
    /// platform スレッドの初期化結果を受け取れなかった（channel 切断）。
    PlatformHandshake,
    /// managed な bridge state を取得できなかった（不在・`Mutex` の poison）。
    PlatformBridgeUnavailable,
    /// 初回 command の送信に失敗した（channel 切断）。
    PlatformCommandDisconnected,
    /// `RegisterHotKey` が失敗した（キー競合・不正な設定）。
    HotkeyRegistration,
}

impl StartupFailure {
    /// ハーネスの契約になる安定した文字列。**OS 依存のエラー文をここへ流さない。**
    pub(crate) fn reason(self) -> &'static str {
        match self {
            StartupFailure::PlatformSpawn => "platform-spawn",
            StartupFailure::PlatformInit => "platform-init",
            StartupFailure::PlatformHandshake => "platform-handshake",
            StartupFailure::PlatformBridgeUnavailable => "platform-bridge-unavailable",
            StartupFailure::PlatformCommandDisconnected => "platform-command-disconnected",
            StartupFailure::HotkeyRegistration => "hotkey-registration",
        }
    }

    /// 終端の区間（`hotkey_register`）を記録してよい失敗か。**`RegisterHotKey` の失敗だけが
    /// 「arm まで到達した」を意味する**——それ以外は arm が走っていないので、その区間は
    /// 通っていない（`null`）。
    fn reached_the_arm(self) -> bool {
        matches!(self, StartupFailure::HotkeyRegistration)
    }
}

/// 起動経路のどの枝を通ったか。**出力に載せる**——反復 11 の「計器が測る枝と変更が触る枝が
/// 同じか」を読み手が毎回確かめられるようにするため。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Branch {
    pub(crate) first_run: bool,
    pub(crate) cache_hit: bool,
    pub(crate) include_path_env: bool,
}

/// 時計を持たない純粋核。呼び出し側が anchor からの経過を渡す。
///
/// **時計を持たないのは測るためである**——`Instant` は任意の値を構築できないので、
/// 丸め境界（各 500,000 ns の 2 区間）のような fixture は注入でしか作れない。
#[derive(Debug)]
pub(crate) struct Timeline {
    pre_main: Option<Duration>,
    /// 直前のマークの anchor からの経過。最初のマークは anchor 起点。
    last: Duration,
    durations: [Option<Duration>; Phase::ALL.len()],
    branch: Branch,
    index_load_stats_ms: Option<u64>,
}

impl Timeline {
    pub(crate) fn new(pre_main: Option<Duration>) -> Self {
        Self {
            pre_main,
            last: Duration::ZERO,
            durations: [None; Phase::ALL.len()],
            branch: Branch::default(),
            index_load_stats_ms: None,
        }
    }

    /// `phase` の終端を記録する。区間長は「直前のマークからの差」である。
    ///
    /// **スキップされた区間のぶんは次の区間へ畳まれる**（`include_path_env = false` の
    /// `path_merge` 等）。区間そのものは `None` のまま残り、出力では `null` になる。
    pub(crate) fn mark(&mut self, phase: Phase, elapsed_since_anchor: Duration) {
        // 単調でない入力（時計の巻き戻り・呼び出し順の誤り）は 0 幅として記録する。
        // **負にはしない**——符号なしで持つ以上、飽和させるほうが panic より読める。
        let delta = elapsed_since_anchor.saturating_sub(self.last);
        self.durations[phase.index()] = Some(delta);
        self.last = elapsed_since_anchor.max(self.last);
    }

    pub(crate) fn set_branch(&mut self, branch: Branch) {
        self.branch = branch;
    }

    pub(crate) fn set_index_load_stats_ms(&mut self, total_ms: u64) {
        self.index_load_stats_ms = Some(total_ms);
    }

    /// 検査専用のアクセサ。製品は [`Timeline::to_json`] を通る（区間を 1 本ずつ引く
    /// 呼び出し点を製品側に作ると、網羅列挙を迂回する経路が生まれる）。
    #[cfg(test)]
    pub(crate) fn phase_ns(&self, phase: Phase) -> Option<u128> {
        self.durations[phase.index()].map(|d| d.as_nanos())
    }

    /// 記録済み区間の生 ns の総和。**終端で anchor から直接読んだ値と突き合わせる相手**であり、
    /// この値から終端値を作ってはならない（同語反復になり、基準点・終点の取り違えを 1 つも
    /// 検出しなくなる）。
    pub(crate) fn sum_phase_ns(&self) -> u128 {
        self.durations.iter().flatten().map(|d| d.as_nanos()).sum()
    }

    /// 出力の JSON。`post_main_elapsed` は**終端で anchor から直接読んだ経過**である。
    pub(crate) fn to_json(
        &self,
        post_main_elapsed: Duration,
        outcome: Result<(), StartupFailure>,
    ) -> serde_json::Value {
        let mut m = serde_json::Map::new();

        let mut put = |key: String, d: Option<Duration>| {
            m.insert(
                format!("{key}_ns"),
                d.map_or(serde_json::Value::Null, |d| json!(d.as_nanos() as u64)),
            );
            m.insert(
                format!("{key}_ms"),
                d.map_or(serde_json::Value::Null, |d| json!(to_ms(d))),
            );
        };

        put("pre_main".to_string(), self.pre_main);
        for p in Phase::ALL {
            put(p.key().to_string(), self.durations[p.index()]);
        }

        // **終端で anchor から直接読んだ値であり、部分和から作らない。** 作れば検算は
        // 同語反復になり、基準点・終点の取り違えを 1 つも検出しなくなる。
        m.insert(
            "post_main_ns".into(),
            json!(post_main_elapsed.as_nanos() as u64),
        );
        m.insert("post_main_ms".into(), json!(to_ms(post_main_elapsed)));
        m.insert("sum_phase_ns".into(), json!(self.sum_phase_ns() as u64));

        // `load_or_scan_with_stats` の中にある未命名の処理。**first-run 枝では
        // `LoadOrScanStats` 自体が存在しないので `null`**（0 にしない）。
        m.insert(
            "index_load_unattributed_ms".into(),
            match (
                self.durations[Phase::IndexLoad.index()],
                self.index_load_stats_ms,
            ) {
                (Some(measured), Some(inner)) => json!(to_ms(measured) as i64 - inner as i64),
                _ => serde_json::Value::Null,
            },
        );

        m.insert("first_run".into(), json!(self.branch.first_run));
        m.insert("cache_hit".into(), json!(self.branch.cache_hit));
        m.insert(
            "include_path_env".into(),
            json!(self.branch.include_path_env),
        );

        m.insert("ok".into(), json!(outcome.is_ok()));
        m.insert(
            "reason".into(),
            outcome
                .err()
                .map_or(serde_json::Value::Null, |f| json!(f.reason())),
        );

        serde_json::Value::Object(m)
    }
}

/// ミリ秒表示への変換。**丸めはこの 1 か所だけで起きる。**
///
/// `Duration::as_millis` を使わず除算を書いてあるのは、**除数を変異させて検知器が
/// 落ちることを測れるようにする**ためである（`to_ms_truncates_toward_zero`）。
fn to_ms(d: Duration) -> u64 {
    (d.as_nanos() / 1_000_000) as u64
}

static TIMELINE: OnceLock<Mutex<(Instant, Timeline)>> = OnceLock::new();
static FINISHED: AtomicBool = AtomicBool::new(false);

/// `main()` の先頭で呼ぶ。anchor を据え、プロセス作成からの経過（`pre_main`）を取る。
///
/// **順序は anchor → `pre_main_elapsed()` である**——`pre_main` は「anchor 時点の壁時計」
/// として測る。creation 時刻は動かないので順序が額を決めるわけではないが、決めておかないと
/// 実装のたびに揺れる。
pub(crate) fn begin() {
    if !crate::trace::trace_enabled() {
        return;
    }
    let anchor = Instant::now();
    let pre_main = pre_main_elapsed();
    let _ = TIMELINE.set(Mutex::new((anchor, Timeline::new(pre_main))));
}

fn with_timeline<R>(f: impl FnOnce(Instant, &mut Timeline) -> R) -> Option<R> {
    let cell = TIMELINE.get()?;
    let mut guard = cell.lock().ok()?;
    let (anchor, timeline) = &mut *guard;
    Some(f(*anchor, timeline))
}

pub(crate) fn mark(phase: Phase) {
    with_timeline(|anchor, t| t.mark(phase, anchor.elapsed()));
}

pub(crate) fn set_branch(branch: Branch) {
    with_timeline(|_, t| t.set_branch(branch));
}

pub(crate) fn set_index_load_stats_ms(total_ms: u64) {
    with_timeline(|_, t| t.set_index_load_stats_ms(total_ms));
}

/// 終端。**一度だけ**出力する（2 回目以降は何もしない）。
///
/// 一度きり性は必須である——platform の初期化に失敗した後、`setup_hotkey_listener` が
/// bridge 不在をもう一度観測して二つ目の失敗行を出す経路が実在する。
pub(crate) fn finish(outcome: Result<(), StartupFailure>) {
    if FINISHED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    let Some(payload) = with_timeline(|anchor, t| {
        // **1 回の読みを終端の区間と `post_main` の両方に使う。** 別々に読むと
        // `sum_phase_ns == post_main_ns` がナノ秒差で崩れる。
        let post_main = anchor.elapsed();
        if outcome.is_ok() || outcome.is_err_and(StartupFailure::reached_the_arm) {
            t.mark(Phase::HotkeyRegister, post_main);
        }
        t.to_json(post_main, outcome)
    }) else {
        return;
    };
    let event = if outcome.is_ok() {
        "startup:ready"
    } else {
        "startup:failed"
    };
    crate::trace::trace(event, payload);
}

/// プロセス作成からの経過。取れなければ `None`（**0 にしない**——測れなかったことと
/// 0 ms は別である）。
#[cfg(windows)]
fn pre_main_elapsed() -> Option<Duration> {
    use std::time::{SystemTime, UNIX_EPOCH};

    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    /// FILETIME（1601-01-01 起点・100ns 単位）から UNIX epoch までの差。
    /// **実測で確かめてある**——既知プロセスの `StartTime` と逆変換の差は ms 切り捨て分だけ。
    const FILETIME_TO_UNIX_EPOCH_100NS: u64 = 116_444_736_000_000_000;

    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;

    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
        .ok()?;
    }

    let created_100ns = ((creation.dwHighDateTime as u64) << 32) | (creation.dwLowDateTime as u64);
    let created_unix_100ns = created_100ns.checked_sub(FILETIME_TO_UNIX_EPOCH_100NS)?;
    let created = Duration::from_nanos(created_unix_100ns.checked_mul(100)?);

    // **負なら `None` を返す**（時計の巻き戻り等）。0 に丸めると「測れなかった」が
    // 「0 ms で通った」に化ける。
    now.checked_sub(created)
}

#[cfg(not(windows))]
fn pre_main_elapsed() -> Option<Duration> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn all_covers_every_variant() {
        // `ALL` への足し忘れを捕まえる。`Phase` に variant を足したらここが落ちる。
        let mut seen = Vec::new();
        for p in Phase::ALL {
            assert!(!seen.contains(&p), "ALL に重複がある: {p:?}");
            seen.push(p);
        }
        assert_eq!(
            seen.len(),
            9,
            "variant を足したら ALL と この件数を同時に直す"
        );
    }

    #[test]
    fn keys_are_unique() {
        let mut keys: Vec<&str> = Phase::ALL.iter().map(|p| p.key()).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(before, keys.len(), "Phase::key が衝突している");
    }

    #[test]
    fn marks_record_the_delta_from_the_previous_mark() {
        let mut t = Timeline::new(Some(ms(100)));
        t.mark(Phase::ConfigLoad, ms(10));
        t.mark(Phase::IndexLoad, ms(30));
        assert_eq!(t.phase_ns(Phase::ConfigLoad), Some(ms(10).as_nanos()));
        assert_eq!(t.phase_ns(Phase::IndexLoad), Some(ms(20).as_nanos()));
    }

    #[test]
    fn skipped_phase_stays_null_and_its_time_folds_into_the_next() {
        // `include_path_env = false` の path_merge がこの形。**0 ではなく null である。**
        let mut t = Timeline::new(None);
        t.mark(Phase::IndexLoad, ms(10));
        t.mark(Phase::HistoryLoad, ms(15));
        assert_eq!(
            t.phase_ns(Phase::PathMerge),
            None,
            "通らなかった区間は null"
        );
        assert_eq!(t.phase_ns(Phase::HistoryLoad), Some(ms(5).as_nanos()));
    }

    #[test]
    fn sum_of_phase_ns_equals_the_last_mark() {
        // これが終端で anchor から直接読んだ値と突き合わせる相手である。
        let mut t = Timeline::new(None);
        t.mark(Phase::ConfigLoad, ms(10));
        t.mark(Phase::IndexLoad, ms(30));
        t.mark(Phase::HotkeyRegister, ms(31));
        assert_eq!(t.sum_phase_ns(), ms(31).as_nanos());
    }

    #[test]
    fn rounding_happens_only_at_the_display_boundary() {
        // 変異 (g): 各 500,000 ns の 2 区間 + 終端 1,000,000 ns。
        // **ms 表示の和は 0 だが、生 ns の検算は通る。**
        let mut t = Timeline::new(None);
        t.mark(Phase::ConfigLoad, Duration::from_nanos(500_000));
        t.mark(Phase::IndexLoad, Duration::from_nanos(1_000_000));

        let post_main = Duration::from_nanos(1_000_000);
        assert_eq!(t.sum_phase_ns(), post_main.as_nanos(), "生 ns は一致する");

        let json = t.to_json(post_main, Ok(()));
        assert_eq!(json["config_load_ms"], 0, "500,000 ns は 0 ms へ切り捨てる");
        assert_eq!(json["index_load_ms"], 0);
        assert_eq!(json["post_main_ms"], 1, "総計だけが 1 ms になる");
        // ms 表示の和（0）は総計（1）と一致しない。**これを検査してはならない。**
    }

    #[test]
    fn to_ms_truncates_toward_zero() {
        // 変異 (k): 除数を 1_000 にするとここが落ちる。
        assert_eq!(to_ms(Duration::from_nanos(999_999)), 0);
        assert_eq!(to_ms(Duration::from_nanos(1_000_000)), 1);
        assert_eq!(to_ms(Duration::from_nanos(1_999_999)), 1);
    }

    #[test]
    fn every_phase_key_is_present_even_when_skipped() {
        // キーの欠落は異常、通らなかった区間は null。**両者を区別する。**
        let t = Timeline::new(None);
        let json = t.to_json(Duration::ZERO, Ok(()));
        for p in Phase::ALL {
            let ns = format!("{}_ns", p.key());
            let ms_key = format!("{}_ms", p.key());
            assert!(json.get(&ns).is_some(), "{ns} が出力に無い");
            assert!(json.get(&ms_key).is_some(), "{ms_key} が出力に無い");
            assert!(json[&ns].is_null(), "通らなかった区間は null であること");
        }
    }

    #[test]
    fn pre_main_is_null_when_unavailable_not_zero() {
        let t = Timeline::new(None);
        let json = t.to_json(Duration::ZERO, Ok(()));
        assert!(
            json["pre_main_ns"].is_null(),
            "測れなかったことと 0 は別である"
        );
    }

    #[test]
    fn branch_flags_are_reported() {
        let mut t = Timeline::new(None);
        t.set_branch(Branch {
            first_run: false,
            cache_hit: true,
            include_path_env: false,
        });
        let json = t.to_json(Duration::ZERO, Ok(()));
        assert_eq!(json["cache_hit"], true);
        assert_eq!(json["first_run"], false);
        assert_eq!(json["include_path_env"], false);
    }

    #[test]
    fn index_load_unattributed_is_null_without_stats() {
        // first-run 枝では `LoadOrScanStats` 自体が存在しない。**0 にしない。**
        let mut t = Timeline::new(None);
        t.mark(Phase::IndexLoad, ms(50));
        let json = t.to_json(ms(50), Ok(()));
        assert!(json["index_load_unattributed_ms"].is_null());
    }

    #[test]
    fn index_load_unattributed_is_the_gap_against_load_stats() {
        let mut t = Timeline::new(None);
        t.mark(Phase::IndexLoad, ms(50));
        t.set_index_load_stats_ms(42);
        let json = t.to_json(ms(50), Ok(()));
        assert_eq!(json["index_load_unattributed_ms"], 8);
    }

    #[test]
    fn failure_reasons_are_stable_and_unique() {
        let all = [
            StartupFailure::PlatformSpawn,
            StartupFailure::PlatformInit,
            StartupFailure::PlatformHandshake,
            StartupFailure::PlatformBridgeUnavailable,
            StartupFailure::PlatformCommandDisconnected,
            StartupFailure::HotkeyRegistration,
        ];
        let mut reasons: Vec<&str> = all.iter().map(|f| f.reason()).collect();
        reasons.sort_unstable();
        let before = reasons.len();
        reasons.dedup();
        assert_eq!(before, reasons.len(), "reason が衝突している");
        assert_eq!(
            StartupFailure::PlatformSpawn.reason(),
            "platform-spawn",
            "reason はハーネスの契約なので固定する"
        );
    }

    #[test]
    fn outcome_is_carried_in_the_payload() {
        let t = Timeline::new(None);
        let ok = t.to_json(Duration::ZERO, Ok(()));
        assert_eq!(ok["ok"], true);
        assert!(ok["reason"].is_null());

        let ng = t.to_json(Duration::ZERO, Err(StartupFailure::HotkeyRegistration));
        assert_eq!(ng["ok"], false);
        assert_eq!(ng["reason"], "hotkey-registration");
    }
}
