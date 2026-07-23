//! 通知 primitive の純粋核（#532 SU5）。一時レーン（検索バー overlay・launchNotice parity の
//! 単一スロット上書き + 自動クリア）と、updater 持続レーン（toast 行の状態機械）を egui/Win32
//! 非依存で持つ。時刻は driver（view.rs）が単調 `Duration`（基準 Instant からの経過）で注入する
//! （layout.rs Debouncer と同じ流儀）。`UpdaterUi<U>` の payload generic はテスト容易性のため:
//! 製品は `U = Box<tauri_plugin_updater::Update>`（テストで構築不能）、テストは `U = ()`。

use std::time::Duration;

/// 起動 worker の応答待ち上限（§19.6 の 4 秒・WebView2 `run_launch_blocking` parity）。
pub const LAUNCH_TIMEOUT: Duration = Duration::from_secs(4);

/// 起動失敗/結果不明の一時通知 duration（`launchNotice.ts` 既定 2400ms parity）。
pub const NOTICE_LAUNCH: Duration = Duration::from_millis(2400);

/// 一時通知の単一スロット。新規 set は旧通知を上書き（`clearLaunchNotice`→set と同型）。
/// `now` は driver の基準 Instant からの経過（単調）。
#[derive(Default)]
pub struct NoticeSlot {
    /// (message, expires_at)。expires_at = set 時の now + duration。
    current: Option<(String, Duration)>,
}

impl NoticeSlot {
    pub fn set(&mut self, message: String, now: Duration, duration: Duration) {
        self.current = Some((message, now + duration));
    }

    /// 期限切れならクリアして true（表示が変わった＝repaint 要）を返す。
    pub fn poll(&mut self, now: Duration) -> bool {
        if let Some((_, expires)) = &self.current
            && now >= *expires
        {
            self.current = None;
            return true;
        }
        false
    }

    pub fn message(&self) -> Option<&str> {
        self.current.as_ref().map(|(m, _)| m.as_str())
    }

    /// 表示中なら期限までの残余（repaint_after 予約用）。
    pub fn remaining(&self, now: Duration) -> Option<Duration> {
        self.current.as_ref().map(|(_, e)| e.saturating_sub(now))
    }

    /// reset-on-show 用の即時クリア（resetForShow の clearLaunchNotice parity）。
    pub fn clear(&mut self) {
        self.current = None;
    }
}

/// updater 持続レーンの局面。`U` は install に使う plugin `Update` の座席
/// （`Available` だけが持つ＝「install できるのに Update が無い」状態を表現不能にする）。
pub enum UpdaterPhase<U> {
    Idle,
    Checking,
    UpToDate,
    Available { version: String, can_install: bool, update: U },
    Installing { version: String },
    InstallFailed { message: String },
}

/// toast 1 行分の表示モデル（描画専用・U 非依存）。
#[derive(Debug, PartialEq)]
pub struct ToastRow {
    /// 行1 のテキスト種別: (version, install 可否) or installing or failed。
    pub kind: ToastKind,
    /// [今すぐ更新] を描くか（can_install かつ Available のときだけ）。
    pub show_install: bool,
    /// ボタンが押せるか（Installing 中は両方 disabled・WebView2 UpdateToast parity）。
    pub buttons_enabled: bool,
}

#[derive(Debug, PartialEq)]
pub enum ToastKind {
    Available { version: String },
    Installing,
    Failed { message: String },
}

/// updater toast の状態機械。dismiss/install の競合は本型のメソッド内で原子的に解決する
/// （呼び出し側は Mutex で包む・spec 決定「Available → Installing は mutex 内で原子遷移」）。
pub struct UpdaterUi<U> {
    pub phase: UpdaterPhase<U>,
    pub dismissed: bool,
}

impl<U> Default for UpdaterUi<U> {
    fn default() -> Self {
        Self { phase: UpdaterPhase::Idle, dismissed: false }
    }
}

impl<U> UpdaterUi<U> {
    /// [今すぐ更新]: Available{can_install} のときだけ Update を取り出し Installing へ原子遷移。
    /// それ以外（二重クリック・Installing 中・dismissed 済）は None。
    pub fn try_begin_install(&mut self) -> Option<U> {
        if self.dismissed {
            return None;
        }
        let phase = std::mem::replace(&mut self.phase, UpdaterPhase::Idle);
        match phase {
            UpdaterPhase::Available { version, can_install: true, update } => {
                self.phase = UpdaterPhase::Installing { version };
                Some(update)
            }
            other => {
                self.phase = other; // 非該当は現状復帰（遷移しない）
                None
            }
        }
    }

    /// [閉じる]: Installing 中は拒否（false）。それ以外は dismissed を立て true。
    pub fn dismiss(&mut self) -> bool {
        if matches!(self.phase, UpdaterPhase::Installing { .. }) {
            return false;
        }
        self.dismissed = true;
        true
    }

    /// 描画モデルの導出。表示しない局面（Idle/Checking/UpToDate/dismissed）は None。
    pub fn toast(&self) -> Option<ToastRow> {
        if self.dismissed {
            return None;
        }
        match &self.phase {
            UpdaterPhase::Available { version, can_install, .. } => Some(ToastRow {
                kind: ToastKind::Available { version: version.clone() },
                show_install: *can_install,
                buttons_enabled: true,
            }),
            UpdaterPhase::Installing { .. } => Some(ToastRow {
                kind: ToastKind::Installing,
                show_install: true, // disabled で描く（WebView2: installing 中もボタンは見える）
                buttons_enabled: false,
            }),
            UpdaterPhase::InstallFailed { message } => Some(ToastRow {
                kind: ToastKind::Failed { message: message.clone() },
                show_install: false,
                buttons_enabled: true,
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(v: u64) -> Duration {
        Duration::from_millis(v)
    }

    #[test]
    fn notice_expires_at_deadline_and_reports_change() {
        let mut n = NoticeSlot::default();
        n.set("失敗".into(), ms(100), NOTICE_LAUNCH);
        assert_eq!(n.message(), Some("失敗"));
        assert!(!n.poll(ms(100 + 2399)), "期限前はクリアしない");
        assert!(n.poll(ms(100 + 2400)), "期限でクリアし repaint 要を返す");
        assert_eq!(n.message(), None);
        assert!(!n.poll(ms(9999)), "空スロットの poll は false（変化なし）");
    }

    #[test]
    fn notice_overwrite_replaces_message_and_deadline() {
        let mut n = NoticeSlot::default();
        n.set("旧".into(), ms(0), ms(1000));
        n.set("新".into(), ms(500), ms(1000)); // 上書き＝旧期限は破棄
        assert_eq!(n.message(), Some("新"));
        assert!(!n.poll(ms(1200)), "旧期限(1000)では消えない");
        assert!(n.poll(ms(1500)), "新期限(500+1000)で消える");
    }

    #[test]
    fn notice_clear_is_idempotent_and_remaining_tracks_deadline() {
        let mut n = NoticeSlot::default();
        assert_eq!(n.remaining(ms(0)), None);
        n.set("x".into(), ms(0), ms(1000));
        assert_eq!(n.remaining(ms(400)), Some(ms(600)));
        n.clear();
        n.clear(); // 冪等
        assert_eq!(n.message(), None);
    }

    #[test]
    fn install_takes_update_only_from_available_with_can_install() {
        let mut u: UpdaterUi<&'static str> = UpdaterUi::default();
        assert!(u.try_begin_install().is_none(), "Idle からは install 不可");
        u.phase = UpdaterPhase::Available { version: "1.2.3".into(), can_install: false, update: "U" };
        assert!(u.try_begin_install().is_none(), "check_only は install 不可");
        u.phase = UpdaterPhase::Available { version: "1.2.3".into(), can_install: true, update: "U" };
        assert_eq!(u.try_begin_install(), Some("U"));
        assert!(matches!(u.phase, UpdaterPhase::Installing { .. }), "原子遷移");
        assert!(u.try_begin_install().is_none(), "二重 install は拒否（Update は一度しか取れない）");
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)] // brief 記載の verbatim を保持（意味は不変）
    fn dismiss_is_refused_while_installing() {
        let mut u: UpdaterUi<()> = UpdaterUi::default();
        u.phase = UpdaterPhase::Installing { version: "1.2.3".into() };
        assert!(!u.dismiss(), "Installing 中の dismiss は拒否（WebView2 disabled parity）");
        assert!(u.toast().is_some(), "toast は出たまま");
        u.phase = UpdaterPhase::InstallFailed { message: "e".into() };
        assert!(u.dismiss());
        assert!(u.toast().is_none(), "dismissed 後は導出も消える");
    }

    #[test]
    fn toast_projection_matches_phase() {
        let mut u: UpdaterUi<()> = UpdaterUi::default();
        assert!(u.toast().is_none(), "Idle は非表示");
        u.phase = UpdaterPhase::Checking;
        assert!(u.toast().is_none(), "Checking は非表示（WebView2 は check 中 UI 無し）");
        u.phase = UpdaterPhase::Available { version: "2.0.0".into(), can_install: true, update: () };
        let t = u.toast().unwrap();
        assert_eq!(t.kind, ToastKind::Available { version: "2.0.0".into() });
        assert!(t.show_install && t.buttons_enabled);
    }
}
