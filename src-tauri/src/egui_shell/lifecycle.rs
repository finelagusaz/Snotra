//! Alt+Q ホットキー分岐・blur 判定の純粋な決定核（Win32 非依存）。SU1 spike で実証済み・#532 SU2。

use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HotkeyPlan {
    HideNow,
    ShowAfterAltRelease,
    ShowNow,
}

/// Alt+Q 押下時の分岐（製品 WebView2 経路と同じ意味論）。表示中かつ hotkey_toggle=true なら
/// 即 hide。それ以外（非表示、または表示中でも hotkey_toggle=false ＝ 既に見えている窓を
/// 再フォーカス/再配置）は show 側へ回り、Alt が押されている限り解放を待ってから show する。
pub(crate) fn plan_hotkey(visible: bool, alt_pressed: bool, hotkey_toggle: bool) -> HotkeyPlan {
    if visible && hotkey_toggle {
        HotkeyPlan::HideNow
    } else if alt_pressed {
        HotkeyPlan::ShowAfterAltRelease
    } else {
        HotkeyPlan::ShowNow
    }
}

/// blur（focus 喪失）から hide 判定までの猶予（#532 SU2）。
/// **予約と判定の両方がこの値を使う**——片方だけ変えると「予約は 100ms 後・判定は別の閾値」
/// という静かな不整合になる。
const BLUR_GRACE: Duration = Duration::from_millis(100);

/// blur 猶予のこのフレームでの処置（#711・契約③）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BlurAction {
    /// 猶予明け + 条件成立 → hide を要求する。
    Hide,
    /// 猶予中 → 残余で再要求する（armed の間は毎フレーム）。
    Rearm(Duration),
    /// 何もしない。**3 つの由来がある**——(1) `blur_grace_action` が「**猶予明け**だが
    /// `auto_hide` が off」で返す（猶予**中**は auto_hide の値によらず `Rearm` である）、
    /// (2) `BlurGrace::observe` が focus を得たフレームで返す、(3) 同じく `NeverFocused`
    /// （show 直後でまだ focus を得ていない）のフレームで返す。
    Idle,
}

/// blur 猶予のこのフレームでの処置を決める純粋核（#711・契約③）。
///
/// **`elapsed` は呼び出し側が 1 回だけ読んで渡すこと。** 関数内で時刻を読み直す形にすると、
/// 判定（`>= BLUR_GRACE`）と減算（`BLUR_GRACE - elapsed`）の間に時計が進んだとき
/// `Duration` 減算が underflow して panic する（release は `panic = "abort"` ゆえプロセスが
/// 落ちる）。**このフレームは猶予境界に着弾するよう予約されており `elapsed ≈ BLUR_GRACE` が
/// 典型ケースなので、確率は低くない。** 減算を `elapsed < BLUR_GRACE` の分岐内へ閉じるのと
/// 併せて、この 1 回読みが安全性を担っている（設計 spec §5 の errata）。
///
/// **再要求するのは「時間経過で解消する不成立」だけ**である。`auto_hide` は時計と無関係な
/// 入力で、時間を進めても変わらない——再要求すると `request_repaint_after(ZERO)` の永久
/// スピンになり、契約②で潰した消費を別の扉から再導入する。その変化は変えた側が wake する
/// 責務を負う（契約①）——`auto_hide` は `config-applied` wake が持ち主であり、例外は無い。
///
/// **`focused` を引数に取らない**（#745）——focus 復帰は `BlurGrace::observe` の早期 return が
/// 吸収しており、この層へは届かない。引数に残すと**到達不能な分岐**を読み手に追わせ、かつ
/// 隣接する 2 つの `bool` として取り違えの余地を作る。focus のゲート自体は
/// `blur_should_hide` の 3 連言が SPEC §8.6 と一対一で保持している。
///
/// `Idle` を返したフレームで猶予が armed のまま残ることは意図である（`BlurGrace` の doc）。
///
/// **このモジュールの外へ出さない**（#745）——外の消費点は `BlurGrace::observe` に一本化した。
/// 公開したままにすると `observe` を迂回する経路が残り、#711 が「消費点の一本化を型で塞ぐ」
/// ことで得たものが散文へ戻る。
fn blur_grace_action(elapsed: Duration, auto_hide: bool) -> BlurAction {
    if blur_should_hide(false, elapsed >= BLUR_GRACE, auto_hide) {
        BlurAction::Hide
    } else if elapsed < BLUR_GRACE {
        BlurAction::Rearm(BLUR_GRACE - elapsed)
    } else {
        BlurAction::Idle
    }
}

/// blur（focus 喪失）猶予明けに hide 要求を出すべきかの純粋判定。焦点が戻っておらず、
/// 100ms 猶予が明け、auto_hide が有効なときだけ hide する。
/// 猶予タイマの発火・repaint 予約という状態は view 側（update）に残す。
/// **このモジュールの外へ出さない**（#711）——消費点は `blur_grace_action` に一本化してあり、
/// 判定と再要求を別々に呼ぶ 2 経路が生まれるのを型で塞ぐ。
///
/// **ガードは `auto_hide_on_focus_lost` 1 つだけである**（`SPEC.md` §8.6 の
/// `SearchVisible --> Standby: focus_lost [auto_hide_on_focus_lost]` と一対一）。
/// かつて第 4 項 `!settings_running`（設定サイドカー存命中は hide しない）が在ったが、
/// **SPEC に根拠を持たず egui 移行時に記録なく入った逸脱**であり #746 で撤去した。
/// SPEC が設定サイドカーについて定めるのは `alwaysOnTop` の一時解除だけである（§8.5）。
fn blur_should_hide(focused: bool, grace_elapsed: bool, auto_hide: bool) -> bool {
    !focused && grace_elapsed && auto_hide
}

/// blur 猶予の状態機械（#745）。**hide を跨いで持ち越さないことがこの型の責務である。**
///
/// **時計を読まない**——`now` は呼び出し側がフレームに 1 回だけ読んで渡す。型の中で
/// `Instant::now()` を呼ぶと、武装（`Blurred(now)`）と経過の算出が別の時刻になり、
/// `blur_grace_action` の doc が警告する underflow を、構造で消したと称して持ち込む。
///
/// **フレーム内の入口は `reset`（段 3）と `observe`（段 16–17）の 2 つだけである。**
/// かつては 2 フィールド（`was_focused` / `unfocus_at`）に分解されており、「前フレームの
/// focus を畳む」段 34 と「focus が戻ったら片方だけ消す」段 14 が独立していた。**#745 は
/// その分解が原因ではない**（SU2 の設計 spec は初日から reset-on-show を要求しており、
/// 実装がそれを落とした）が、1 フィールドにすると `reset` が 1 行になり、
/// 「両方消したか」という問い自体が消える。
///
/// **`Idle` は武装を解かない。** `auto_hide` が off の間に猶予が明けても `Blurred` のまま
/// 留まり、後から `auto_hide` が有効化されれば（`config-applied` wake で）hide できる。
///
/// **同じ `egui_shell/` の armed 期限 2 つ（`notify::NoticeSlot` / `layout::Debouncer`）とは
/// 時刻の持ち方が違う**——あちらは基準 `Instant` を driver 側に置いて `Duration` を注入されるが、
/// こちらは `Instant` 自体を状態として持つ。**意図的な差である**: 猶予の起点は「blur が起きた
/// 瞬間」であって driver が基準を保つ意味が無く、`Instant` を呼び出し点で 1 回読む形のほうが
/// #711 errata（多重読みの禁止）へ強く適合する（`notice_base.elapsed()` は実際に 1 フレームで
/// 3 回読まれている）。揃え忘れではないので `Duration` 注入へ書き換えないこと。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum BlurGrace {
    /// show 直後。**まだ一度も focus を得ていない**——この状態からは武装しない。
    /// `Hide` を返した直後もここへ戻る（旧 2 フィールド表現の `(false, None)` に対応）。
    /// **初期状態でもある**——`reset()` と `LauncherController::new` の両方がここへ寄る。
    #[default]
    NeverFocused,
    /// focus を持っている。
    Focused,
    /// focus を失った。猶予の起点を持つ。
    Blurred(Instant),
}

impl BlurGrace {
    /// 段 3: reset-on-show。**呼び出し点は `LauncherController::consume_reset_pending` である。**
    /// この呼び出しが消えると #745 が再発するが、`launcher_controller.rs` は `AppHandle` に
    /// 縛られてユニットテストを持てないため**検知手段が無い**（受容残余・機械化は #930）。
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    /// 段 16–17: 今フレームの focus を畳み、このフレームの処置を返す。
    ///
    /// 副作用（`emit_hide` / `request_repaint_after`）は呼び出し側が持つ。**`auto_hide` は
    /// 値渡しである**（実行中 config の毎フレーム live-read・#576）——遅延評価やキャッシュを
    /// 選ばなかった理由は `ADR-blur-grace-single-field-state-machine`。
    #[must_use]
    pub(crate) fn observe(&mut self, focused: bool, now: Instant, auto_hide: bool) -> BlurAction {
        if focused {
            *self = Self::Focused;
            return BlurAction::Idle;
        }
        match *self {
            // 一度も focus を得ていない窓に `focus_lost` は起きない（`SPEC.md` §8.6 と整合）。
            Self::NeverFocused => BlurAction::Idle,
            Self::Focused => {
                *self = Self::Blurred(now);
                // 武装したフレームの経過は厳密に 0 ゆえ必ず `Rearm(BLUR_GRACE)` になる
                // （`blur_grace_rearms_while_armed_and_hides_after` が固定）。判定を
                // `blur_grace_action` に通すのは、閾値の出所を 1 つに保つためである。
                blur_grace_action(Duration::ZERO, auto_hide)
            }
            Self::Blurred(at) => {
                // `now - at` ではなく飽和減算を使う——呼び出し側が単調な `now` を渡す限り
                // 負にはならないが、`Instant` の `Sub` が持つ panic 経路を残さない。
                let action = blur_grace_action(now.saturating_duration_since(at), auto_hide);
                if action == BlurAction::Hide {
                    *self = Self::NeverFocused;
                }
                action
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BLUR_GRACE, BlurAction, BlurGrace, HotkeyPlan, blur_grace_action, plan_hotkey};
    use std::time::{Duration, Instant};

    /// `BlurGrace` の駆動に使う基準時刻。**テストは時計を進めず、`t0 + Duration` を渡す**
    /// ——`observe` が時計を読まない設計であることが、この形を可能にしている。
    fn t0() -> Instant {
        Instant::now()
    }

    /// **シナリオ A**（issue 本文の欠陥）: 猶予 armed のまま別経路で hide され、再 show の
    /// 初フレームが `focused == false` になる。reset が効いていれば武装は残らない。
    #[test]
    fn blur_grace_resets_stale_arm_across_hide() {
        let t = t0();
        let mut g = BlurGrace::NeverFocused;
        let _ = g.observe(true, t, true); // focus を得る
        let _ = g.observe(false, t, true); // blur → 武装
        g.reset(); // reset-on-show
        assert_eq!(
            g.observe(false, t + Duration::from_secs(10), true),
            BlurAction::Idle,
            "reset 後は stale な猶予が残らない"
        );
    }

    /// A の **vacuity guard**。テスト A（`blur_grace_resets_stale_arm_across_hide`）が
    /// 「reset が効いた」ではなく「そもそも武装していなかった」で自明に緑になっていないことを
    /// 示す——reset を抜けば `Hide` になる状況を作れている、という対照である。
    ///
    /// **このテストは `consume_reset_pending` の `reset()` 呼び出しが消えたことを検出しない**
    /// （実測: 呼び出しを削除しても blur 系 12 本すべて通る）。呼び出し点の消失に検知手段が
    /// 無いことは `BlurGrace::reset` の doc が記すとおりで、機械化は #930 が追う。
    /// **`reset` の実装が部分的になった場合**（例: `Blurred` のときだけ戻す）は、
    /// テスト A ではなく `blur_grace_resets_prior_focus_across_hide` が落ちる（実測）。
    #[test]
    fn blur_grace_without_reset_would_hide_on_stale_arm() {
        let t = t0();
        let mut g = BlurGrace::NeverFocused;
        let _ = g.observe(true, t, true);
        let _ = g.observe(false, t, true);
        // reset を呼ばない
        assert_eq!(
            g.observe(false, t + Duration::from_secs(10), true),
            BlurAction::Hide,
            "reset が無ければ stale な武装で即 hide する（これが #745）"
        );
    }

    /// **シナリオ B**（issue 未記載）: focus を持ったまま hide（Enter での起動成功・
    /// ホットキーのトグル・トレイ）し、再 show の初フレームが `focused == false` になる。
    #[test]
    fn blur_grace_resets_prior_focus_across_hide() {
        let t = t0();
        let mut g = BlurGrace::NeverFocused;
        let _ = g.observe(true, t, true); // focus を持ったまま hide される
        g.reset();
        assert_eq!(
            g.observe(false, t + Duration::from_millis(1), true),
            BlurAction::Idle,
            "reset 後は「一度も focus を得ていない」ので武装しない"
        );
    }

    /// B の **vacuity guard**。**`Focused` の持ち越しは、武装済みの猶予の持ち越し（A）とは
    /// 独立に危険**であることを示す——reset を抜けば新規武装から `Hide` まで進む状況を
    /// 作れている、という対照である。`reset` の呼び出し点の消失は検出しない（A の対照の
    /// doc を参照）。
    #[test]
    fn blur_grace_without_reset_would_arm_on_stale_prior_focus() {
        let t = t0();
        let mut g = BlurGrace::NeverFocused;
        let _ = g.observe(true, t, true);
        // reset を呼ばない
        assert_eq!(
            g.observe(false, t + Duration::from_millis(1), true),
            BlurAction::Rearm(BLUR_GRACE),
            "stale な Focused から新規に武装してしまう"
        );
        assert_eq!(
            g.observe(false, t + Duration::from_millis(151), true),
            BlurAction::Hide,
            "その 100ms 後に hide まで進む（これがシナリオ B）"
        );
    }

    /// 正常系: focus 喪失のエッジで武装する。
    #[test]
    fn blur_grace_arms_on_focus_loss_edge() {
        let t = t0();
        let mut g = BlurGrace::NeverFocused;
        let _ = g.observe(true, t, true);
        assert_eq!(g.observe(false, t, true), BlurAction::Rearm(BLUR_GRACE));
    }

    /// focus が戻れば武装を捨てる（旧・段 14 の責務）。**捨てた後は `Focused` なので、
    /// 次の blur は `Idle` ではなく新規武装になる。**
    #[test]
    fn blur_grace_drops_pending_when_focus_returns() {
        let t = t0();
        let mut g = BlurGrace::NeverFocused;
        let _ = g.observe(true, t, true);
        let _ = g.observe(false, t, true); // 武装
        assert_eq!(
            g.observe(true, t + Duration::from_millis(50), true),
            BlurAction::Idle
        );
        assert_eq!(
            g.observe(false, t + Duration::from_secs(10), true),
            BlurAction::Rearm(BLUR_GRACE),
            "focus 復帰後の blur は新規武装（stale な起点を使わない）"
        );
    }

    /// `Hide` を返した後は `NeverFocused` へ戻る（旧 2 フィールド表現の `(false, None)` に対応）。
    #[test]
    fn blur_grace_hide_returns_to_never_focused() {
        let t = t0();
        let mut g = BlurGrace::NeverFocused;
        let _ = g.observe(true, t, true);
        let _ = g.observe(false, t, true);
        assert_eq!(
            g.observe(false, t + Duration::from_millis(150), true),
            BlurAction::Hide
        );
        // **内部表現ではなく振る舞いで固定する**——`Idle` が返ることは `NeverFocused` を
        // 一意に示す（`Focused` なら次の blur で `Rearm`、`Blurred` のままなら `Hide`）。
        assert_eq!(
            g.observe(false, t + Duration::from_secs(1), true),
            BlurAction::Idle,
            "hide 後は武装が解けている"
        );
    }

    /// **`Idle` は武装を解かない。** auto_hide を後から有効化すれば hide できる経路が生きる。
    #[test]
    fn blur_grace_idle_keeps_arm_when_auto_hide_off() {
        let t = t0();
        let mut g = BlurGrace::NeverFocused;
        let _ = g.observe(true, t, true);
        let _ = g.observe(false, t, true); // 武装
        assert_eq!(
            g.observe(false, t + Duration::from_millis(150), false),
            BlurAction::Idle,
            "auto_hide off の猶予明けは何もしない"
        );
        assert_eq!(
            g.observe(false, t + Duration::from_millis(160), true),
            BlurAction::Hide,
            "武装は残っているので、auto_hide を有効化すれば hide できる"
        );
    }

    #[test]
    fn blur_grace_rearms_while_armed_and_hides_after() {
        let ms = Duration::from_millis;
        // 猶予中は残余で再要求する（契約③: 予約はフレームの到来を約束しない）。
        assert_eq!(blur_grace_action(ms(0), true), BlurAction::Rearm(ms(100)));
        assert_eq!(blur_grace_action(ms(99), true), BlurAction::Rearm(ms(1)));
        // 境界ちょうどは Hide 側（`>=`）——減算に落ちないことも兼ねて固定する。
        assert_eq!(blur_grace_action(BLUR_GRACE, true), BlurAction::Hide);
        assert_eq!(blur_grace_action(ms(150), true), BlurAction::Hide);
    }

    #[test]
    fn blur_grace_stays_idle_when_time_cannot_resolve_it() {
        let ms = Duration::from_millis;
        // 猶予明けだが時計と無関係な条件で不成立 → **再要求しない**（永久スピンを作らない）。
        // **この層に残る時計非依存の入力は `auto_hide` だけである**——focus 復帰は
        // `BlurGrace::observe` の早期 return が吸収しており、その挙動は
        // `blur_grace_drops_pending_when_focus_returns` が固定する。
        assert_eq!(blur_grace_action(ms(150), false), BlurAction::Idle);
    }

    #[test]
    fn blur_grace_does_not_underflow_far_past_the_deadline() {
        // 猶予を大きく超えた経過でも減算に落ちない（設計 spec §5 errata の回帰検出器——
        // release は panic = "abort" ゆえ underflow はプロセス停止になる）。
        assert_eq!(
            blur_grace_action(Duration::from_secs(10), false),
            BlurAction::Idle
        );
    }

    #[test]
    fn hotkey_branches_match_product_semantics() {
        // (visible, alt_pressed, hotkey_toggle)
        assert_eq!(plan_hotkey(true, false, true), HotkeyPlan::HideNow); // 表示+toggle → hide
        assert_eq!(plan_hotkey(true, true, true), HotkeyPlan::HideNow);
        assert_eq!(plan_hotkey(true, false, false), HotkeyPlan::ShowNow); // 表示+非toggle → 再フォーカス
        assert_eq!(
            plan_hotkey(true, true, false),
            HotkeyPlan::ShowAfterAltRelease // 表示+非toggle+Alt → 解放待ち show
        );
        assert_eq!(
            plan_hotkey(false, true, true),
            HotkeyPlan::ShowAfterAltRelease
        );
        assert_eq!(plan_hotkey(false, false, true), HotkeyPlan::ShowNow);
        assert_eq!(plan_hotkey(false, false, false), HotkeyPlan::ShowNow);
    }

    #[test]
    fn blur_hides_only_when_all_gates_pass() {
        use super::blur_should_hide;
        // focused, grace_elapsed, auto_hide。各行は 1 つだけ倒して落ちることを見るので、
        // この 4 行が「3 連言」を一意に固定する（ゲートが 3 つで全部である根拠は関数の doc）。
        assert!(blur_should_hide(false, true, true)); // 全成立 → hide
        assert!(!blur_should_hide(true, true, true)); // 焦点復帰 → hide しない
        assert!(!blur_should_hide(false, false, true)); // 猶予未明け → hide しない
        assert!(!blur_should_hide(false, true, false)); // auto_hide 無効 → hide しない
    }
}
