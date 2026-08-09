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
//! **`RegisterInitialHotkey` の arm だけに閉じてはならない**——bridge の初期化失敗・窓の
//! 生成失敗のように、その arm 自体が実行されない経路が実在する。そこで終端を出さないと、
//! ハーネスには「タイムアウト」としか見えず、**診断したい相手が読めなくなる**。
//!
//! 呼び出し点は `main.rs`（`setup_platform_thread` / `egui_shell::create` の早期 return /
//! `setup_hotkey_listener`）と `platform/mod.rs`（arm）に在り、**列挙は [`StartupFailure`]
//! の variant が持つ**（数をここへ書くと、経路を足したときにこの行だけが腐る）。
//!
//! 一度きり性は [`FINISHED`] の CAS が持つ。**二重の守りではなく必須である**——platform の
//! 初期化に失敗した後、`setup_hotkey_listener` が bridge 不在をもう一度観測して二つ目の
//! 失敗行を出す経路が実在する。
//!
//! # 受容する残余
//!
//! **失敗経路（`StartupFailure` のうち `HotkeyRegistration` 以外）は実機で一度も観測して
//! いない。** bridge の spawn / Win32 初期化 / channel を実際に失敗させる手段が無いためで、
//! **注入点を製品コードへ足す案は却下した**（2026-08-09 のユーザー判断）——計測しきれない
//! リスクは残るが、**本来不要なコードがトラブルの原因を作り込む理由にはならない**。
//! `SNOTRA_FAKE_INITIAL_HOTKEY_FAILURE`（`platform/mod.rs`）のような既存のハッチを増やす
//! 方向も同じ理由で採らない。
//!
//! ゆえに次の 2 つは**書けない**: (a) これらの経路が実際に `startup:failed` を出すこと
//! (b) `PlatformBridgePending::wait` の channel 切断が本番でどう起きるか（`recv()` の失敗
//! 経路は実在するが、thread panic 等の原因は未確定）。守っているのは**写像の網羅性**
//! （[`StartupFailure::from`] の match と `reason` の一意性）だけである。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::json;

/// 起動経路の区間。**出力はこの全 variant を並べる**。
/// variant を足すときの手当ては [`Phase::COUNT`] の doc を見ること。
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
    /// 出力に並べる区間の数。**variant を足したらここも上げる。**
    ///
    /// 足し忘れは `count_matches_the_enum_declaration` が捕まえる——`include_str!` で
    /// **このファイルのソーステキスト自身**を走査し、`enum Phase` の宣言から variant を
    /// 数えて突き合わせる。
    ///
    /// **かつてここには「原理的に守れない」と書いてあった。それは誤りだった**——
    /// 手書きの一覧（`const ALL` や `index()` の match）を母集団にする限り守れない、
    /// というところまでは正しいが、**母集団を「一覧」ではなく「ソーステキスト」に取れば
    /// 守れる**（code-reviewer が反証を作って両方向に測った: 変異で FAILED・素で passed）。
    /// `std::mem::variant_count` が nightly であることは、検知手段が無いことを意味しない。
    ///
    /// 加えて**害そのものも消してある**（検知器と二重に持つ）。[`Timeline::mark`] と
    /// [`Timeline::to_json`] は範囲外の添字で panic せず、[`Self::all`] は `None` を
    /// 読み飛ばす——release は `panic = "abort"` ゆえ、計器の欠陥で製品を落とすより
    /// その区間を捨てるほうが軽い。**欠けは出力の `null` として読める。**
    pub(crate) const COUNT: usize = 9;

    pub(crate) fn all() -> impl Iterator<Item = Phase> {
        (0..Self::COUNT).filter_map(Self::from_index)
    }

    /// 添字 → variant。**`index()` の逆であり、両者が唯一の対応表である。**
    ///
    /// 範囲外は `None`。`all()` はここが `Some` を返す間だけ回るので、**`COUNT` を
    /// 大きく取りすぎても出力に幽霊の区間は現れない**。
    fn from_index(i: usize) -> Option<Phase> {
        Some(match i {
            0 => Phase::ConfigLoad,
            1 => Phase::IndexLoad,
            2 => Phase::PathMerge,
            3 => Phase::HistoryLoad,
            4 => Phase::EngineBuild,
            5 => Phase::TauriInit,
            6 => Phase::WindowsCreate,
            7 => Phase::SetupRest,
            8 => Phase::HotkeyRegister,
            _ => return None,
        })
    }

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

    /// variant → 添字。**網羅 match ゆえ、variant を足すとコンパイルが通らない**
    /// （足し忘れが検査ではなくコンパイラに捕まる）。
    fn index(self) -> usize {
        match self {
            Phase::ConfigLoad => 0,
            Phase::IndexLoad => 1,
            Phase::PathMerge => 2,
            Phase::HistoryLoad => 3,
            Phase::EngineBuild => 4,
            Phase::TauriInit => 5,
            Phase::WindowsCreate => 6,
            Phase::SetupRest => 7,
            Phase::HotkeyRegister => 8,
        }
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
    /// 窓の生成に失敗した（`egui_shell::create`）。setup ブロック唯一の早期 return。
    WindowCreation,
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
            StartupFailure::WindowCreation => "window-creation",
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

/// bridge の失敗を終端の分類へ写す。**この写像はここ 1 か所である。**
///
/// 呼び出し点ごとに `match` を書くと、片方だけがワイルドカードを持つ形になりやすく、
/// `BridgeError` に variant を足したとき**黙って既存の `reason` へ潰れる**——`reason` は
/// ハーネスの契約なので、潰れても赤にならず意味だけがずれる。網羅 match をここへ集めておけば、
/// variant の追加はコンパイラが指す。
impl From<crate::platform::BridgeError> for StartupFailure {
    fn from(e: crate::platform::BridgeError) -> Self {
        use crate::platform::BridgeError;
        match e {
            BridgeError::Spawn => StartupFailure::PlatformSpawn,
            BridgeError::Init => StartupFailure::PlatformInit,
            BridgeError::Handshake => StartupFailure::PlatformHandshake,
            BridgeError::Disconnected => StartupFailure::PlatformCommandDisconnected,
        }
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
    /// **固定長配列ではなく `Vec` である。** 長さは常に [`Phase::COUNT`] だが、
    /// 型で固定すると**「範囲外の添字を渡された」状況をテストで構築できない**
    /// ——`Phase` の値からは範囲外を作れず、配列を縮めると型が合わない。
    /// 「害を消した」（panic しない）という主張が検査できなくなるので `Vec` を取る。
    durations: Vec<Option<Duration>>,
    branch: Branch,
    index_load_stats_ms: Option<u64>,
}

impl Timeline {
    pub(crate) fn new(pre_main: Option<Duration>) -> Self {
        Self {
            pre_main,
            last: Duration::ZERO,
            durations: vec![None; Phase::COUNT],
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
        // **添字が範囲外でも panic しない。** `Phase` に variant を足して `COUNT` を
        // 上げ忘れた状態がそれに当たる——**検査では守れない**（一覧が母集団なので、
        // `index()`/`key()` へ arm を足せばテストは通ってしまう。実測済み）。
        // release は `panic = "abort"` ゆえ、**計器の欠陥で製品プロセスを落とすより
        // その区間を黙って捨てるほうが害が小さい**（欠けは出力の `null` で読める）。
        if let Some(slot) = self.durations.get_mut(phase.index()) {
            *slot = Some(delta);
        }
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
        self.durations
            .get(phase.index())
            .copied()
            .flatten()
            .map(|d| d.as_nanos())
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
        for p in Phase::all() {
            // **範囲外でも panic しない**（`mark` と同じ理由——`COUNT` の doc）。
            // 添字を持つ 3 か所すべてを揃えること: ここ・`mark`・下の `index_load`。
            put(
                p.key().to_string(),
                self.durations.get(p.index()).copied().flatten(),
            );
        }

        // **終端で anchor から直接読んだ値であり、部分和から作らない。** 作れば検算は
        // 同語反復になり、基準点・終点の取り違えを 1 つも検出しなくなる。
        m.insert(
            "post_main_ns".into(),
            json!(post_main_elapsed.as_nanos() as u64),
        );
        m.insert("post_main_ms".into(), json!(to_ms(post_main_elapsed)));
        m.insert("sum_phase_ns".into(), json!(self.sum_phase_ns() as u64));

        // **名前の付いていない末尾**。`post_main_ns - sum_phase_ns` であり、**恒等式
        // `post_main_ns == sum_phase_ns + unmarked_tail_ns` は全経路で成り立つ**。
        //
        // 0 でなくなるのは、終端が最後の区間をマークせずに出たときである——bridge の
        // 初期化失敗など、`RegisterInitialHotkey` の arm が走らなかった経路がそれに当たる。
        // **残余を項目にせず「和が一致すること」だけを不変条件にすると、その経路で検算が
        // 必ず破れ、ハーネスが二重に失敗して理由が読めなくなる**（反復 6 が `digest_ms` を
        // 足してフェーズ間の隙間を塞いだのと同じ形）。
        m.insert(
            "unmarked_tail_ns".into(),
            json!(
                post_main_elapsed
                    .as_nanos()
                    .saturating_sub(self.sum_phase_ns()) as u64
            ),
        );

        // `load_or_scan_with_stats` の中にある未命名の処理。**first-run 枝では
        // `LoadOrScanStats` 自体が存在しないので `null`**（0 にしない）。
        m.insert(
            "index_load_unattributed_ms".into(),
            match (
                self.durations
                    .get(Phase::IndexLoad.index())
                    .copied()
                    .flatten(),
                self.index_load_stats_ms,
            ) {
                (Some(measured), Some(inner)) => json!(to_ms(measured) as i64 - inner as i64),
                _ => serde_json::Value::Null,
            },
        );

        // **どこまで進んだか。** 失敗終端では以降の区間が `null` になるが、その `null` は
        // 「マークの取り落とし」ではなく「そこまで行かなかった」である。**両者を区別する
        // 材料をハーネスへ渡す**——これが無いと、ハーネスは `reason` から Phase を再導出する
        // （写しが 2 部になる）か、失敗経路の `null` を一律免除する（取り落としが一切
        // 見えなくなる）かのどちらかになる。
        m.insert(
            "reached_phase".into(),
            self.durations
                .iter()
                .rposition(Option::is_some)
                .and_then(Phase::from_index)
                .map_or(serde_json::Value::Null, |p| json!(p.key())),
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
    fn index_and_from_index_are_inverse_over_the_whole_enum() {
        // **variant の足し忘れを捕まえるのはコンパイラである。** `index()` が網羅 match
        // ゆえ variant を足すとコンパイルが通らず、`COUNT` だけ増やすと `from_index` の
        // `unreachable!()` へ落ちてこのテストが落ちる。
        //
        // **この検査は往復だけを見る。** 足し忘れそのものは
        // `count_matches_the_enum_declaration` が別に捕まえる——手書きの一覧を走査する形
        // （かつての `const ALL`・その後の `index()`/`key()` の match）は、**一覧自身が
        // 母集団なので足し忘れを検出できない**（両方とも変異で実測した）。
        for i in 0..Phase::COUNT {
            let p = Phase::from_index(i).expect("COUNT の範囲は from_index が Some を返す");
            assert_eq!(p.index(), i, "添字 {i} で往復しない");
        }
        let mut seen = Vec::new();
        for p in Phase::all() {
            assert!(!seen.contains(&p), "重複がある: {p:?}");
            seen.push(p);
        }
        assert_eq!(seen.len(), Phase::COUNT);
    }

    #[test]
    fn count_matches_the_enum_declaration() {
        // **母集団はこのファイルのソーステキストそのものである。** 手書きの一覧
        // （`const ALL`・`index()` / `key()` の match）を走査する形では、一覧自身が
        // 母集団になるので足し忘れを検出できない——**両方とも変異で実測した**。
        // ソースを読めば `COUNT` の据え置きが左右の食い違いとして現れる。
        //
        // `std::mem::variant_count` は nightly だが、**検知手段が無いことは意味しない**
        // （かつてこの doc は「原理的に守れない」と断定していた。誤りだった）。
        let src = include_str!("startup.rs");
        let decl = src
            .split_once("pub(crate) enum Phase {")
            .expect("enum Phase の宣言が見つからない（改名したらこの検査も直す）")
            .1
            .split_once('}')
            .expect("enum Phase の宣言が閉じていない")
            .0;
        let declared = decl
            .lines()
            .map(str::trim)
            .filter(|l| l.ends_with(',') && !l.starts_with("//") && !l.starts_with("/*"))
            .count();
        assert_eq!(
            declared,
            Phase::COUNT,
            "enum Phase の variant 数と COUNT がずれている（variant を足したら COUNT も上げる）"
        );
    }

    #[test]
    fn out_of_range_index_is_dropped_instead_of_panicking() {
        // **`COUNT` を上げ忘れた variant を `mark` / `to_json` へ渡した状況**を、
        // 配列側を縮めて再現する（`Phase` の値では範囲外を作れないため）。release は
        // `panic = "abort"` ゆえ、ここで panic すると計器の欠陥が製品を落とす。
        //
        // 前版はこの検査を `[T; 9].get_mut(9) == None` という std の恒真式で書いており、
        // **`mark` も `to_json` も一度も呼んでいなかった**（code-reviewer が指摘）。
        let mut t = Timeline::new(None);
        t.durations = Vec::new(); // COUNT より短い＝全 variant が範囲外
        t.mark(Phase::ConfigLoad, ms(1));
        t.mark(Phase::HotkeyRegister, ms(2));
        let json = t.to_json(ms(2), Ok(()));
        // 区間は 1 つも記録されないが、キーは全部出て `null` になる。
        for p in Phase::all() {
            assert!(json[format!("{}_ns", p.key())].is_null(), "{:?}", p);
        }
        assert_eq!(json["sum_phase_ns"], 0u64);
        assert_eq!(json["unmarked_tail_ns"], ms(2).as_nanos() as u64);
    }

    #[test]
    fn keys_are_unique() {
        let mut keys: Vec<&str> = Phase::all().map(|p| p.key()).collect();
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
    fn unmarked_tail_closes_the_sum_when_the_last_phase_never_ran() {
        // bridge の初期化失敗など、`RegisterInitialHotkey` の arm が走らなかった経路。
        // **恒等式 post_main == sum_phase + unmarked_tail は全経路で成り立つ**——
        // 残余を項目にしないと、この経路で検算が必ず破れてハーネスが二重に失敗する。
        let mut t = Timeline::new(None);
        t.mark(Phase::ConfigLoad, ms(10));
        let json = t.to_json(ms(25), Err(StartupFailure::PlatformSpawn));

        assert!(json["hotkey_register_ns"].is_null(), "arm は走っていない");
        assert_eq!(json["sum_phase_ns"], 10_000_000u64);
        assert_eq!(json["unmarked_tail_ns"], 15_000_000u64);
        assert_eq!(
            json["post_main_ns"].as_u64().unwrap(),
            json["sum_phase_ns"].as_u64().unwrap() + json["unmarked_tail_ns"].as_u64().unwrap(),
        );
    }

    #[test]
    fn reached_phase_names_the_last_marked_interval() {
        // **失敗経路の `null` を 2 種に分ける材料である**——「そこまで行かなかった」と
        // 「マークを取り落とした」。これが無いとハーネスは `reason` から Phase を
        // 再導出する（写しが 2 部）か、一律免除する（取り落としが見えない）しかない。
        let mut t = Timeline::new(None);
        assert!(
            t.to_json(Duration::ZERO, Ok(()))["reached_phase"].is_null(),
            "1 つも刻んでいなければ null"
        );

        t.mark(Phase::ConfigLoad, ms(1));
        t.mark(Phase::HistoryLoad, ms(2)); // index_load / path_merge は飛ばす
        let json = t.to_json(ms(2), Err(StartupFailure::PlatformSpawn));
        assert_eq!(
            json["reached_phase"], "history_load",
            "飛ばした区間ではなく、実際に刻んだ最後を指す"
        );
    }

    #[test]
    fn post_main_is_taken_independently_of_the_partial_sum() {
        // **変異 (h)「`post_main` を部分和から作る」を落とす唯一の検査である。**
        //
        // 恒等式（検査 3）は `unmarked_tail_ns = post_main - sum_phase` として計算する以上
        // **構成上ほぼ常に真**であり、同語反復化した実装を通してしまう。ハーネスの外部壁時計
        // （検査 4）も上限しか縛らないので、内側で小さく辻褄を合わせる形は素通りする
        // （どちらも実際に変異を当てて素通りを実測した）。
        //
        // ここでは**部分和と食い違う終端値**を渡す。`to_json` が引数を使わず部分和から
        // 組み立てるようになれば、`unmarked_tail_ns` が 0 になってこの検査が落ちる。
        let mut t = Timeline::new(None);
        t.mark(Phase::ConfigLoad, ms(10));
        let json = t.to_json(ms(30), Ok(()));
        assert_eq!(
            json["post_main_ns"],
            ms(30).as_nanos() as u64,
            "引数をそのまま出す"
        );
        assert_eq!(json["sum_phase_ns"], ms(10).as_nanos() as u64);
        assert_eq!(
            json["unmarked_tail_ns"],
            ms(20).as_nanos() as u64,
            "部分和から作っていたらここが 0 になる"
        );
    }

    #[test]
    fn unmarked_tail_is_zero_on_the_normal_path() {
        let mut t = Timeline::new(None);
        t.mark(Phase::ConfigLoad, ms(10));
        t.mark(Phase::HotkeyRegister, ms(25));
        let json = t.to_json(ms(25), Ok(()));
        assert_eq!(json["unmarked_tail_ns"], 0u64, "全区間を刻めば残余は 0");
    }

    #[test]
    fn every_phase_key_is_present_even_when_skipped() {
        // キーの欠落は異常、通らなかった区間は null。**両者を区別する。**
        let t = Timeline::new(None);
        let json = t.to_json(Duration::ZERO, Ok(()));
        for p in Phase::all() {
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
        // **手書きの列挙である**（`Phase` と同じ弱さを持つ——variant を足してここへ
        // 書き足さなくても落ちない）。`StartupFailure` は添字を持たないので `Phase` の
        // `COUNT`/`from_index` に当たる仕掛けが無く、`reason()` の網羅 match だけが
        // 足し忘れを止める。**その match は `todo!()` を書けば通ってしまう**ので、
        // ここは網羅の証明ではなく「既存の `reason` が衝突せず固定である」ことの検査である。
        let all = [
            StartupFailure::PlatformSpawn,
            StartupFailure::PlatformInit,
            StartupFailure::PlatformHandshake,
            StartupFailure::PlatformBridgeUnavailable,
            StartupFailure::PlatformCommandDisconnected,
            StartupFailure::WindowCreation,
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
