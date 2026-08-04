//! Alt+Q ホットキー分岐・blur 判定の純粋な決定核（Win32 非依存）。SU1 spike で実証済み・#532 SU2。

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
pub(crate) const BLUR_GRACE: std::time::Duration = std::time::Duration::from_millis(100);

/// blur 猶予のこのフレームでの処置（#711・契約③）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BlurAction {
    /// 猶予明け + 条件成立 → hide を要求する。
    Hide,
    /// 猶予中 → 残余で再要求する（armed の間は毎フレーム）。
    Rearm(std::time::Duration),
    /// 猶予明けだが条件不成立（auto_hide off / focus 復帰）→ 何もしない。
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
/// **再要求するのは「時間経過で解消する不成立」だけ**である。`focused` / `auto_hide` は
/// 時計と無関係な入力で、時間を進めても変わらない——再要求すると
/// `request_repaint_after(ZERO)` の永久スピンになり、契約②で潰した消費を別の扉から
/// 再導入する。それらの変化は変えた側が wake する責務を負う（契約①）。
///
/// **時計と無関係な入力はこの 2 つで全部であり、どちらも wake の持ち主がいる**——`focused` は
/// tao の窓イベント（`on_window_event`）、`auto_hide` は `config-applied` wake。ゆえに
/// **契約①の観点では**例外が無い（#746 以前は `settings_running` が第 3 の入力として在り、
/// 設定サイドカーの終了監視スレッドが wake を負っていなかった。項ごと撤去して解消した）。
///
/// **この主張は wake の持ち主についてだけである。** `Idle` を返したフレームで `unfocus_at` が
/// クリアされないこと（＝猶予が armed のまま残り、hide を跨いで持ち越されうること）は
/// 別の未解決事項であり #745 が追う。
pub(crate) fn blur_grace_action(
    elapsed: std::time::Duration,
    focused: bool,
    auto_hide: bool,
) -> BlurAction {
    if blur_should_hide(focused, elapsed >= BLUR_GRACE, auto_hide) {
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

#[cfg(test)]
mod tests {
    use super::{BLUR_GRACE, BlurAction, HotkeyPlan, blur_grace_action, plan_hotkey};
    use std::time::Duration;

    #[test]
    fn blur_grace_rearms_while_armed_and_hides_after() {
        let ms = Duration::from_millis;
        // 猶予中は残余で再要求する（契約③: 予約はフレームの到来を約束しない）。
        assert_eq!(
            blur_grace_action(ms(0), false, true),
            BlurAction::Rearm(ms(100))
        );
        assert_eq!(
            blur_grace_action(ms(99), false, true),
            BlurAction::Rearm(ms(1))
        );
        // 境界ちょうどは Hide 側（`>=`）——減算に落ちないことも兼ねて固定する。
        assert_eq!(blur_grace_action(BLUR_GRACE, false, true), BlurAction::Hide);
        assert_eq!(blur_grace_action(ms(150), false, true), BlurAction::Hide);
    }

    #[test]
    fn blur_grace_stays_idle_when_time_cannot_resolve_it() {
        let ms = Duration::from_millis;
        // 猶予明けだが時計と無関係な条件で不成立 → **再要求しない**（永久スピンを作らない）。
        // **この分岐に残る入力は 2 つだけである**（auto_hide・focused）。どちらも変えた側に
        // wake の持ち主がいる（config-applied wake / tao の窓イベント）ため、契約①に例外は無い。
        assert_eq!(blur_grace_action(ms(150), false, false), BlurAction::Idle); // auto_hide off
        assert_eq!(blur_grace_action(ms(150), true, true), BlurAction::Idle); // focus 復帰
    }

    #[test]
    fn blur_grace_does_not_underflow_far_past_the_deadline() {
        // 猶予を大きく超えた経過でも減算に落ちない（設計 spec §5 errata の回帰検出器——
        // release は panic = "abort" ゆえ underflow はプロセス停止になる）。
        assert_eq!(
            blur_grace_action(Duration::from_secs(10), false, false),
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
        // focused, grace_elapsed, auto_hide —— **ゲートはこの 3 つで全部である**（#746）。
        // 各行は 1 つだけ倒して落ちることを見るので、この 4 行が「3 連言」を一意に固定する。
        assert!(blur_should_hide(false, true, true)); // 全成立 → hide
        assert!(!blur_should_hide(true, true, true)); // 焦点復帰 → hide しない
        assert!(!blur_should_hide(false, false, true)); // 猶予未明け → hide しない
        assert!(!blur_should_hide(false, true, false)); // auto_hide 無効 → hide しない
    }
}
