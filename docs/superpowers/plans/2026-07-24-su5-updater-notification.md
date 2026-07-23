# SU5 updater + 通知 primitive + 起動 async 化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** egui 経路（`SNOTRA_EGUI_MAIN`）に通知 primitive（一時 overlay + 持続 toast）を新設し、その上に updater（check/install・`on_before_exit` 保存）と #631（起動 async 化 + single-flight + 失敗通知）+ flush-on-Enter を載せる。

**Architecture:** 純粋核（`notify.rs`/`strings.rs`/既存 `search_state.rs`・`layout.rs`）+ driver（`view.rs`）の既存分業を踏襲。起動は per-launch 専用スレッド + フレーム drain（channel は per-launch・rx を `LaunchInFlight` が所有）。updater 状態は managed `Mutex<UpdaterUi<Box<Update>>>`（payload generic でテスト可能）。

**Tech Stack:** Rust / egui / tauri v2 / tauri-plugin-updater 2.10.1（Rust API・`UpdaterExt`）

**Spec:** `docs/superpowers/specs/2026-07-24-su5-updater-notification-design.md`（決定事項 1–10 を必ず先に読むこと）

## Global Constraints

- **ブランチ**: `feat/su5-updater-notification` を main から作成（main へ直接コミット禁止）。Task 0 で作成済みの前提で各タスクはコミットのみ行う
- **G1 不侵**: WebView2 経路（`ui/src/**`・IPC コマンドの挙動）は一切変更しない。変更は `src-tauri/src/egui_shell/**`・`main.rs` の egui 分岐 + exit listener 切り出し・SPEC/CLAUDE.md のみ
- **検証**: `*.rs` 編集後は PostToolUse hook が clippy + 当該 crate テストを自動実行する（**沈黙 = 合格**・失敗時のみ会話に届く）。明示実行は `cargo test -p snotra --lib`（コマンドの SSOT は `docs/build-commands.md` カテゴリ A）
- **release は panic=abort**: `unwrap()` を新規コードに置かない（既存の `lock().unwrap()` パターンのみ踏襲可）。`unreachable!` は使わず `debug_assert!` + graceful return
- **文言 parity**: UI 文言は `ui/src/lib/i18n.ts` の同キー値と一字一句一致させる（Task 2 の表が正本）
- **タイムアウト**: 起動 4 秒（`LAUNCH_TIMEOUT`）。WebView2 `run_launch_blocking`（`launch.rs:68`）と同値
- **通知 duration**: 起動失敗/結果不明 = 2400ms（`launchNotice.ts` 既定 parity）

---

### Task 1: notify.rs 純粋核（一時レーン NoticeSlot + updater 持続レーン UpdaterUi）

**Files:**
- Create: `src-tauri/src/egui_shell/notify.rs`
- Modify: `src-tauri/src/egui_shell/mod.rs`（`mod notify;` + re-export 追加）

**Interfaces:**
- Produces: `NoticeSlot`（`set/poll/message/remaining/clear`）、`UpdaterUi<U>`（`phase/dismissed/try_begin_install/dismiss/toast`）、`UpdaterPhase<U>`、`ToastRow`、`LAUNCH_TIMEOUT: Duration`、`NOTICE_LAUNCH: Duration`
- Consumes: なし（std のみ・egui/Win32/tauri 非依存）

- [ ] **Step 1: 失敗するテストを含む notify.rs を書く**

```rust
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
```

- [ ] **Step 2: mod.rs に配線して失敗を確認**

`src-tauri/src/egui_shell/mod.rs` の `mod view;` の後に追加:

```rust
mod notify;
```

`pub(crate) use layout::{...};` 行の後に追加:

```rust
// view.rs（driver）が通知 primitive（一時 overlay + updater toast）で消費する（#532 SU5）。
pub(crate) use notify::{
    LAUNCH_TIMEOUT, NOTICE_LAUNCH, NoticeSlot, ToastKind, ToastRow, UpdaterPhase, UpdaterUi,
};
```

Run: `cargo test -p snotra --lib notify`
Expected: コンパイル成功・テスト 6 件 PASS（テスト同梱で書くため Red は「追加前に存在しない」ことで担保。unused import の clippy warn が出たら `#[allow(unused_imports)]` は付けず、Task 4/6 まで re-export を `notify::NoticeSlot` 等の使用箇所到達で解消——**hook が dead_code warn を報告した場合のみ** re-export 行を Task 4 へ後送してよい）

- [ ] **Step 3: コミット**

```powershell
git add src-tauri/src/egui_shell/notify.rs src-tauri/src/egui_shell/mod.rs
git commit -m "feat(#532): SU5 Task1 通知 primitive 純粋核（NoticeSlot + UpdaterUi）"
```

---

### Task 2: strings.rs 最小 i18n テーブル + 既存ヒント移行

**Files:**
- Create: `src-tauri/src/egui_shell/strings.rs`
- Modify: `src-tauri/src/egui_shell/mod.rs`（`mod strings;` + re-export）
- Modify: `src-tauri/src/egui_shell/view.rs`（ハードコード 3 ヒントを置換 + `lang()` ヘルパー）

**Interfaces:**
- Consumes: `snotra_core::config::Language`（`Ja`/`En`・config.rs:23）
- Produces: `strings::{search_hint, tool_select_hint, indexing_hint, launching, launch_failed, launch_timeout, update_available, update_install_now, update_dismiss, update_installing, update_failed}`（全て第一引数 `Language`）、`SearchWindowView::lang()`

- [ ] **Step 1: i18n.ts の正本値を確認する**

Run: `ui/src/lib/i18n.ts` を Read し、次のキーの ja/en 値を確認（下表と一致するはず。placeholder 系キー名は `search.placeholder` / `placeholder.tool_select` 相当を grep で特定し、**実際の値を下の実装へ写す**）:

| キー | ja | en |
|---|---|---|
| search.status.indexing | インデックス構築中... | Building index... |
| search.status.launching | 起動中... | Launching... |
| notice.launch.failed | 起動に失敗しました{detail} | Launch failed{detail} |
| notice.launch.timeout | 起動に時間がかかっています{detail} | Launch is taking a while{detail} |
| update.available | v{version} が利用可能です | v{version} is available |
| update.install_now | 今すぐ更新 | Update now |
| update.dismiss | 閉じる | Dismiss |
| update.installing | インストール中... | Installing... |
| update.failed | 更新に失敗しました | Update failed |

- [ ] **Step 2: strings.rs を書く（テスト同梱）**

```rust
//! egui 経路の UI 文言テーブル（#532 SU5）。`ui/src/lib/i18n.ts` の同キー値と一字一句一致
//! させる（parity の正本は i18n.ts）。言語は config `general.language` を起動時に一回読む
//! 静的解決（hot-reload＝`language-changed` 追従は SU6 の config 反映で拡張・spec 決定 10）。
//! snotra-core は「UI 表示文字列を持たない」規約のため、文言はこの crate（UI 層）に置く。

use snotra_core::config::Language;

pub fn search_hint(l: Language) -> &'static str {
    match l {
        Language::Ja => "検索…",
        Language::En => "Search…", // Step 1 で確認した i18n.ts の実値に合わせること
    }
}

pub fn tool_select_hint(l: Language) -> &'static str {
    match l {
        Language::Ja => "ツールを選択…",
        Language::En => "Select a tool…", // 同上
    }
}

pub fn indexing_hint(l: Language) -> &'static str {
    match l {
        Language::Ja => "インデックス構築中...",
        Language::En => "Building index...",
    }
}

pub fn launching(l: Language) -> &'static str {
    match l {
        Language::Ja => "起動中...",
        Language::En => "Launching...",
    }
}

/// detail は `notifyLaunchFailure` parity: message があれば " (msg)"、無ければ空文字を渡す。
pub fn launch_failed(l: Language, detail: &str) -> String {
    match l {
        Language::Ja => format!("起動に失敗しました{detail}"),
        Language::En => format!("Launch failed{detail}"),
    }
}

/// timeout＝「結果不明」の意味論（spec 決定 8）。WebView2 の文言が既にこの意味を持つ。
pub fn launch_timeout(l: Language, detail: &str) -> String {
    match l {
        Language::Ja => format!("起動に時間がかかっています{detail}"),
        Language::En => format!("Launch is taking a while{detail}"),
    }
}

pub fn update_available(l: Language, version: &str) -> String {
    match l {
        Language::Ja => format!("v{version} が利用可能です"),
        Language::En => format!("v{version} is available"),
    }
}

pub fn update_install_now(l: Language) -> &'static str {
    match l {
        Language::Ja => "今すぐ更新",
        Language::En => "Update now",
    }
}

pub fn update_dismiss(l: Language) -> &'static str {
    match l {
        Language::Ja => "閉じる",
        Language::En => "Dismiss",
    }
}

pub fn update_installing(l: Language) -> &'static str {
    match l {
        Language::Ja => "インストール中...",
        Language::En => "Installing...",
    }
}

pub fn update_failed(l: Language) -> &'static str {
    match l {
        Language::Ja => "更新に失敗しました",
        Language::En => "Update failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_are_interpolated_in_both_languages() {
        assert_eq!(launch_failed(Language::Ja, " (exe not found)"), "起動に失敗しました (exe not found)");
        assert_eq!(launch_failed(Language::En, ""), "Launch failed");
        assert_eq!(update_available(Language::En, "1.2.3"), "v1.2.3 is available");
    }

    #[test]
    fn timeout_wording_is_indeterminate_not_failure() {
        // spec 決定 8: timeout は「失敗」でなく「結果不明」。文言に「失敗」を含めない。
        assert!(!launch_timeout(Language::Ja, "").contains("失敗"));
        assert!(!launch_timeout(Language::En, "").to_lowercase().contains("failed"));
    }
}
```

- [ ] **Step 3: mod.rs 配線 + view.rs のヒント置換**

`mod.rs`: `mod notify;` の後に `mod strings;` を追加し、re-export:

```rust
// view.rs が UI 文言（hint/overlay/toast）で消費する（#532 SU5・言語は起動時一回読み）。
pub(crate) use strings as ui_strings;
```

`view.rs`: `resolve_tools` 付近のヘルパー群に追加:

```rust
/// UI 文言の言語（config general.language・起動時一回でなく都度読み——lock 1 回/フレームの
/// 既存ヘルパー群と同型。SU6 の hot-reload 拡張時もこの読み口のまま動く）。
fn lang(&self) -> snotra_core::config::Language {
    self.app_handle
        .try_state::<crate::AppState>()
        .map(|s| s.engine.lock().unwrap().config().general.language)
        .unwrap_or(snotra_core::config::Language::Ja)
}
```

`update()` の hint 分岐（view.rs:1096-1104）を置換:

```rust
let l = self.lang();
let hint: &str = if in_tool {
    crate::egui_shell::ui_strings::tool_select_hint(l)
} else if !in_folder && self.indexing() && self.state.query().trim().is_empty() {
    crate::egui_shell::ui_strings::indexing_hint(l)
} else {
    crate::egui_shell::ui_strings::search_hint(l)
};
```

- [ ] **Step 4: テスト実行 + コミット**

Run: `cargo test -p snotra --lib strings`
Expected: PASS（+ hook 沈黙）

```powershell
git add src-tauri/src/egui_shell/strings.rs src-tauri/src/egui_shell/mod.rs src-tauri/src/egui_shell/view.rs
git commit -m "feat(#532): SU5 Task2 egui 最小 i18n テーブル + 既存ヒント移行"
```

---

### Task 3: flush-on-Enter（純粋述語 + Enter 前 flush）

**Files:**
- Modify: `src-tauri/src/egui_shell/search_state.rs`（述語 `should_flush_on_enter` 追加 + テスト）
- Modify: `src-tauri/src/egui_shell/view.rs`（Enter ハンドラ・view.rs:1193-1201）
- Modify: `src-tauri/src/egui_shell/mod.rs`（re-export に `should_flush_on_enter` 追加）

**Interfaces:**
- Consumes: `ViewKind`・`QueryIntent`（search_state.rs）・`Debouncer::{is_armed, cancel}`（layout.rs）
- Produces: `pub fn should_flush_on_enter(view_kind: ViewKind, is_plain: bool, armed: bool) -> bool`

- [ ] **Step 1: 述語 + テストを search_state.rs 末尾のテスト mod と本体に追加**

本体（`clamp_selected` の近く）:

```rust
/// Enter 時の trailing flush 要否（#631 flush-on-Enter・SolidJS flushPendingRefresh 同型）。
/// armed になるのは Results∧Plain 経路のみ（folder=同期・instant/command=cancel 済み）だが、
/// 将来の armed 経路追加に対して条件を独立に固定する（誤発火の構造的防止・spec C 節）。
pub fn should_flush_on_enter(view_kind: ViewKind, is_plain: bool, armed: bool) -> bool {
    view_kind == ViewKind::Results && is_plain && armed
}
```

テスト（既存 `#[cfg(test)]` mod 内）:

```rust
#[test]
fn flush_on_enter_only_for_armed_plain_results() {
    assert!(should_flush_on_enter(ViewKind::Results, true, true));
    assert!(!should_flush_on_enter(ViewKind::Results, true, false), "armed でなければ flush 不要");
    assert!(!should_flush_on_enter(ViewKind::Results, false, true), "instant/command では flush しない");
    assert!(!should_flush_on_enter(ViewKind::Folder, true, true), "folder は同期フィルタ");
    assert!(!should_flush_on_enter(ViewKind::Tool, true, true), "tool 中は検索自体が凍結");
}
```

- [ ] **Step 2: Red 確認 → mod.rs re-export → Green**

Run: `cargo test -p snotra --lib flush_on_enter`
Expected: 追加直後は関数未定義でコンパイルエラー（Red 相当）→ 本体追加後 PASS

`mod.rs` の search_state re-export 行へ `should_flush_on_enter` を追加。

- [ ] **Step 3: view.rs Enter ハンドラに flush を差し込む**

view.rs:1193-1201 の Enter ブロックを置換（`enter_pressed` 判定は不変・dispatch 前に flush）:

```rust
let (enter_pressed, shift_held) =
    ctx.input(|i| (i.key_pressed(egui::Key::Enter), i.modifiers.shift));
if enter_pressed {
    // #631 flush-on-Enter: trailing 窓内（打鍵後 50ms 以内）の Enter は leading 時点の
    // 結果で起動しうる。armed な plain クエリは cancel → 同期 run_search で最終クエリの
    // 結果に置換してから dispatch（SolidJS resolveActivationTarget の flushPendingRefresh 同型）。
    let prefix = self.instant_prefix();
    let is_plain = matches!(self.state.interp(&prefix), QueryIntent::Plain);
    if crate::egui_shell::should_flush_on_enter(
        self.state.view_kind(),
        is_plain,
        self.search_debounce.is_armed(),
    ) {
        self.search_debounce.cancel();
        self.run_search_with(&prefix);
        // run_search 後の selected は毎打鍵 reset_selection() 済み（changed ハンドラ）ゆえ
        // 0 のまま＝flush 後の先頭行。SolidJS も flush 経由で clampSelectedIndex される（parity）。
    }
    if !self.state.results().is_empty() {
        if shift_held {
            self.shift_activate(self.state.selected());
        } else {
            self.activate_or_execute(self.state.selected());
        }
    }
}
```

（注意: 旧コードの `if enter_pressed && !self.state.results().is_empty()` を「flush 後に空判定」へ順序変更している——flush で結果が空になった Enter は正しく no-op になる）

- [ ] **Step 4: テスト + コミット**

Run: `cargo test -p snotra --lib`
Expected: 全 PASS（hook 沈黙）

```powershell
git add src-tauri/src/egui_shell/search_state.rs src-tauri/src/egui_shell/view.rs src-tauri/src/egui_shell/mod.rs
git commit -m "fix(#631): SU5 Task3 flush-on-Enter（trailing 窓内 Enter の stale 起動を解消）"
```

---

### Task 4: 起動 async 化（per-launch worker + drain + single-flight・#631 本丸）

**Files:**
- Modify: `src-tauri/src/egui_shell/view.rs`（`LaunchWork`/`LaunchInFlight` 追加、`activate`/`execute_tool_selected`/`execute_instant_selected` の worker 化、drain、reset 連動、入力ガード）

**Interfaces:**
- Consumes: `launch_item_core`/`launch_with_tool_core`/`record_and_save`/`LaunchResult`/`LaunchStatus`（commands/launch.rs）、`execute_instant_action_core`（commands/instant.rs）、`LAUNCH_TIMEOUT`・`NOTICE_LAUNCH`・`NoticeSlot`（Task 1）、`strings`（Task 2）
- Produces: `SearchWindowView` フィールド `launching: Option<LaunchInFlight>`・`notice: NoticeSlot`・`notice_base: Instant`、メソッド `start_launch`/`drain_launch`/`finish_launch`。Task 5 は `launching.is_some()` と `notice.message()` を描画に使う

- [ ] **Step 1: 型とフィールドを追加**

view.rs の `FolderMsg` enum の後に:

```rust
/// 起動 worker への仕事（#631・spec C 節）。worker スレッドが実行し、成功時の履歴記録も
/// worker 側で行う（spec 決定 5: WebView2 が backend 側で UI 可視性と無関係に記録する parity。
/// hide 中に完了した起動の記録消失 gap を閉じる）。Instant は記録しない（IPC 経路 parity）。
enum LaunchWork {
    /// 通常起動（§4.8）。tools 先頭があれば launch_with_tool_core、無ければ launch_item_core。
    Normal { path: String, query: String, tools: Vec<OpenerTool> },
    /// ツール選択起動（§18.4）。
    Tool { target_path: String, launch_query: String, exe: String, args: String },
    /// instant 実行（§19.6）。clipboard 読み + 展開 + 実行の全体を worker で行う
    /// （engine ロック内の action 抽出だけ UI スレッド・spec C 節）。
    Instant { name: String, action: snotra_core::config::InstantAction, instant_query: String },
}

/// 起動成功時に drain が行う UI 後処理の種別（M1/M3 の同期版と同じ末尾へ合流させる）。
#[derive(Clone, Copy)]
enum LaunchTag {
    Normal,  // emit_hide のみ（M1 activate parity・クエリは次 show の reset で消える）
    Tool,    // clear_search + state.reset + emit_hide（execute_tool_selected parity）
    Instant, // clear_search + emit_hide（execute_instant_selected parity）
}

/// in-flight 起動（spec C 節 不変条件 1: channel は per-launch）。rx を本構造体が所有し、
/// `launching = None` で Receiver ごと drop → worker の遅着 send は Err で自然消滅する。
/// folder の「view 寿命の共有 channel + 世代 token」をコピーしないこと（token が要るのは
/// 共有 channel だから。per-launch なら不要——並行性レビューで確定）。
struct LaunchInFlight {
    started: Instant,
    rx: Receiver<crate::commands::launch::LaunchResult>,
    tag: LaunchTag,
}
```

`SearchWindowView` struct にフィールド追加（`last_scrolled_selected` の後）:

```rust
    /// in-flight 起動（single-flight の実体: Some の間は新規起動 dispatch を拒否）。
    launching: Option<LaunchInFlight>,
    /// 一時通知（起動失敗/結果不明）。時刻は notice_base からの経過で注入（純粋核）。
    notice: crate::egui_shell::NoticeSlot,
    /// notice の単調時刻基準（view 生成時に固定・Instant 差分を Duration で渡す）。
    notice_base: Instant,
```

`new()` に初期化を追加:

```rust
            launching: None,
            notice: crate::egui_shell::NoticeSlot::default(),
            notice_base: Instant::now(),
```

- [ ] **Step 2: start_launch / finish_launch / drain_launch を実装**

`clear_search` の前に追加:

```rust
    /// 起動を per-launch worker スレッドへ投げる（#631・spec C 節）。single-flight:
    /// in-flight 中は拒否（WebView2 activationLane parity・二重起動防止）。突入時に results を
    /// クリアする（withLaunchLifecycle の await 前 clearResults parity・spec 決定 7）——
    /// launching 中は 52px collapse・↑↓/クリックは空リストゆえ自然に inert。クエリは保持。
    fn start_launch(&mut self, work: LaunchWork, tag: LaunchTag, ctx: &egui::Context) {
        if self.launching.is_some() {
            return; // single-flight 拒否（拒否された Enter が後で再生されるキューは egui に無い）
        }
        let (tx, rx) = channel::<crate::commands::launch::LaunchResult>();
        self.launching = Some(LaunchInFlight { started: Instant::now(), rx, tag });
        self.state.set_results(Vec::new());
        self.instant_rows_query = None; // 行が消えるため来歴も一体でクリア（finding 0 の規律）
        self.last_scrolled_selected = None;
        let app = self.app_handle.clone();
        let egui_ctx = ctx.clone();
        std::thread::spawn(move || {
            use crate::commands::launch::{LaunchStatus, launch_item_core, launch_with_tool_core, record_and_save};
            let (outcome, record) = match work {
                LaunchWork::Normal { path, query, tools } => {
                    let o = if let Some(first) = tools.first() {
                        launch_with_tool_core(&path, &first.exe, &first.args)
                    } else {
                        launch_item_core(&path)
                    };
                    (o, Some((path, query)))
                }
                LaunchWork::Tool { target_path, launch_query, exe, args } => {
                    let o = launch_with_tool_core(&target_path, &exe, &args);
                    (o, Some((target_path, launch_query)))
                }
                LaunchWork::Instant { name, action, instant_query } => {
                    // clipboard 読み（Win32）はロック外・worker 内（commands/instant.rs と同順）。
                    let clipboard = arboard::Clipboard::new()
                        .and_then(|mut cb| cb.get_text())
                        .unwrap_or_default();
                    let o = crate::commands::instant::execute_instant_action_core(
                        action, &instant_query, &clipboard,
                    );
                    crate::trace_main(
                        "egui_instant",
                        serde_json::json!({ "name": name, "status": format!("{:?}", o.status) }),
                    );
                    (o, None) // instant は履歴を記録しない（IPC 経路 parity）
                }
            };
            // 履歴記録は worker 側（spec 決定 5）。timeout で drain が破棄済みでも記録は行われる
            // ＝「実際に起動したのに履歴が無い」窓を Normal/Tool では作らない。
            if matches!(outcome.status, LaunchStatus::Ok)
                && let Some((path, query)) = record
                && let Some(state) = app.try_state::<crate::AppState>()
            {
                record_and_save(&state, &path, &query);
            }
            let _ = tx.send(outcome); // 遅着（rx drop 済み）は Err で自然消滅（不変条件 1）
            egui_ctx.request_repaint(); // イベント駆動 runtime を起こす（folder/icon と同理由）
        });
    }

    /// drain が回収した結果の UI 後処理（成功列は M1/M3 同期版と同じ末尾へ合流）。
    fn finish_launch(&mut self, tag: LaunchTag, outcome: crate::commands::launch::LaunchResult) {
        use crate::commands::launch::LaunchStatus;
        crate::trace_main(
            "egui_launch_done",
            serde_json::json!({ "status": format!("{:?}", outcome.status) }),
        );
        let l = self.lang();
        match outcome.status {
            LaunchStatus::Ok => match tag {
                LaunchTag::Normal => self.emit_hide(),
                LaunchTag::Tool => {
                    self.clear_search();
                    self.state.reset();
                    self.emit_hide();
                }
                LaunchTag::Instant => {
                    self.clear_search();
                    self.emit_hide();
                }
            },
            LaunchStatus::Failed | LaunchStatus::Timeout => {
                // 失敗: hide しない・同期 run_search で結果を再取得（runRefresh parity）+ 一時通知。
                // Timeout ステータスがここへ来るのは core が同期 Timeout を返す場合のみ
                // （drain 側の 4 秒は Empty 経路で扱う）。文言は失敗系で扱う。
                let detail = outcome
                    .message
                    .as_deref()
                    .map(|m| format!(" ({m})"))
                    .unwrap_or_default();
                self.notice.set(
                    crate::egui_shell::ui_strings::launch_failed(l, &detail),
                    self.notice_base.elapsed(),
                    crate::egui_shell::NOTICE_LAUNCH,
                );
                self.run_search();
            }
        }
    }

    /// フレーム毎の in-flight 回収（spec C 節 不変条件 2: **reset_pending 消費の後**に呼ぶ。
    /// 前に置くと show 直後フレームで stale Ok が reset より先に処理され再 show 窓を hide で撃つ）。
    fn drain_launch(&mut self, ctx: &egui::Context) {
        let Some(inflight) = &self.launching else { return };
        match inflight.rx.try_recv() {
            Ok(outcome) => {
                let tag = inflight.tag;
                self.launching = None;
                self.finish_launch(tag, outcome);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                let elapsed = inflight.started.elapsed();
                if elapsed >= crate::egui_shell::LAUNCH_TIMEOUT {
                    // 4 秒経過＝「結果不明」（spec 決定 8）。rx ごと破棄→遅着は自然消滅。
                    // 起動という副作用は取り消せない（abandoned spawn_blocking parity）。
                    self.launching = None;
                    let l = self.lang();
                    self.notice.set(
                        crate::egui_shell::ui_strings::launch_timeout(l, ""),
                        self.notice_base.elapsed(),
                        crate::egui_shell::NOTICE_LAUNCH,
                    );
                    self.run_search(); // WebView2 timeout 分岐（runRefresh）parity
                } else {
                    // deadline で確実に起きる（**可視中のみ有効**——hidden 中に update() が
                    // 走らない場合は次 show まで宙吊りになるが、reset-on-show の launching
                    // クリアが backstop・spec C 節「hidden 中の drain」）。
                    ctx.request_repaint_after(crate::egui_shell::LAUNCH_TIMEOUT - elapsed);
                }
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                // worker panic 等の異常終了。失敗扱いで回復（永久 in-flight を防ぐ）。
                self.launching = None;
                let l = self.lang();
                self.notice.set(
                    crate::egui_shell::ui_strings::launch_failed(l, ""),
                    self.notice_base.elapsed(),
                    crate::egui_shell::NOTICE_LAUNCH,
                );
                self.run_search();
            }
        }
    }
```

- [ ] **Step 3: 3 起動経路を worker 化する**

`activate`（view.rs:192-236）の同期実行部（`let outcome = ...` から末尾まで）を置換。シグネチャを `fn activate(&mut self, index: usize, ctx: &egui::Context)` に変更:

```rust
        let path = result.path.clone();
        let is_folder = result.is_folder;
        let query = self.state.query().to_string();
        let tools = self.resolve_tools(&path, is_folder);
        crate::trace_main(
            "egui_launch",
            serde_json::json!({ "index": index, "opener": !tools.is_empty() }),
        );
        self.start_launch(LaunchWork::Normal { path, query, tools }, LaunchTag::Normal, ctx);
```

`execute_tool_selected`（view.rs:398-420）の `let outcome = ...` 以降を置換（シグネチャに `ctx: &egui::Context` 追加）:

```rust
        crate::trace_main("egui_tool_launch", serde_json::json!({ "index": index }));
        self.start_launch(
            LaunchWork::Tool {
                target_path,
                launch_query,
                exe: tool.exe.clone(),
                args: tool.args.clone(),
            },
            LaunchTag::Tool,
            ctx,
        );
```

`execute_instant_selected`（view.rs:290-334）: action 抽出（engine ロック内）までは不変。clipboard 読み以降を置換（シグネチャに `ctx: &egui::Context` 追加）:

```rust
        self.start_launch(
            LaunchWork::Instant { name, action, instant_query: instant_query.to_string() },
            LaunchTag::Instant,
            ctx,
        );
```

`activate_or_execute` / `shift_activate` のシグネチャにも `ctx: &egui::Context` を貫通させ、呼び出しサイト（Enter ブロック・クリックブロック）から `&ctx` を渡す。

- [ ] **Step 4: reset 連動 + drain 配置 + 入力ガード**

reset_pending 消費ブロック（view.rs:902-915）の `self.icon_pending.clear();` の後に追加:

```rust
            // SU5: in-flight 起動と一時通知は show を跨がない（resetForShow の
            // setLaunching(false) + clearLaunchNotice parity）。rx ごと drop するため
            // hide 中に完了した遅着結果もここで自然消滅する（stale Ok が再 show 窓を
            // hide で撃つ事故の backstop・並行性レビュー High）。updater toast は触らない。
            self.launching = None;
            self.notice.clear();
```

reset ブロックの直後・folder drain（`let mut latest`）の**前**に drain 呼び出しを追加（不変条件 2: reset 消費の後）:

```rust
        // 起動結果の回収（#631）。reset_pending 消費の後に置くこと（spec C 節 不変条件 2）。
        self.drain_launch(&ctx);
        // 一時通知の期限管理（期限切れで repaint・表示中は残余で wake 予約）。
        if self.notice.poll(self.notice_base.elapsed()) {
            ctx.request_repaint();
        }
        if let Some(remaining) = self.notice.remaining(self.notice_base.elapsed()) {
            ctx.request_repaint_after(remaining);
        }
```

TextEdit の interactive 条件（view.rs:1128）を打鍵ガードへ拡張:

```rust
                .interactive(!in_tool && self.launching.is_none())
```

（`launching` ガードは**打鍵のみ**。Escape/blur/Alt+Q・↑↓は從来どおり通す——spec 決定 3・4。↑↓は空リストゆえ自然 no-op）

- [ ] **Step 5: テスト + 動作トレース確認 + コミット**

Run: `cargo test -p snotra --lib`
Expected: 全 PASS（hook 沈黙。借用エラーが出た場合は `activate_or_execute` 系の `&egui::Context` 貫通漏れを疑う）

Run（スモーク・起動確認）:

```powershell
$env:SNOTRA_EGUI_MAIN = "1"; $env:SNOTRA_TRACE = "1"; cargo run -p snotra 2>&1 | Select-String "egui_launch"
```

Expected: Enter 起動で `egui_launch` → `egui_launch_done {"status":"Ok"}` の 2 行が出て、ウィンドウが hide する（成功列の合流確認）

```powershell
git add src-tauri/src/egui_shell/view.rs
git commit -m "feat(#631): SU5 Task4 起動 async 化（per-launch worker + drain + single-flight）"
```

---

### Task 5: 一時 overlay 描画（起動中… / 失敗通知・painted label）

**Files:**
- Modify: `src-tauri/src/egui_shell/view.rs`（TextEdit 直後に overlay 描画を追加）

**Interfaces:**
- Consumes: `launching.is_some()`・`notice.message()`（Task 4）、`ui_strings::launching`（Task 2）、`row_theme` の色導出パターン
- Produces: なし（描画のみ・Task 7 が同じ挿入位置の直後に toast を積む）

- [ ] **Step 1: overlay 描画を実装**

TextEdit の `response` 取得と focus 処理（view.rs:1124-1173）の**後**に追加:

```rust
        // 一時 overlay（#532 SU5）: 「起動中…」/ 失敗・結果不明通知を検索バーに重ね描く。
        // hint_text は空クエリ時のみ描かれるため使えない（launching/notice 中は query 非空・
        // 状態機械レビュー）——painted label で TextEdit の rect を塗り潰して上書きする。
        // 優先順は WebView2 SearchWindow.tsx の Switch 先頭一致 parity: indexing > 起動中 > 通知。
        // indexing はここでは描かない（egui では空クエリ hint が担う・SU3 as-built）。indexing 中に
        // launching/notice が重なる窓（instant は indexing 中も実行可）は indexing 表示を優先し
        // overlay を抑止する（Switch 順 parity・parity レビュー要修正 3）。
        let overlay_text: Option<String> = if self.indexing() && self.state.view_kind() == ViewKind::Results {
            None // indexing が最優先（hint が見える・overlay は描かない）
        } else if self.launching.is_some() {
            Some(crate::egui_shell::ui_strings::launching(self.lang()).to_string())
        } else {
            self.notice.message().map(|m| m.to_string())
        };
        if let Some(text) = overlay_text {
            let rect = response.rect;
            let (input_bg, hint_color) = self
                .app_handle
                .try_state::<crate::AppState>()
                .map(|s| {
                    let engine = s.engine.lock().unwrap();
                    let v = &engine.config().visual;
                    (v.input_background_color.clone(), v.hint_text_color.clone())
                })
                .unwrap_or_else(|| ("#383838".into(), "#808080".into()));
            ui.painter().rect_filled(
                rect,
                4.0,
                hex_color(&input_bg, egui::Color32::from_rgb(0x38, 0x38, 0x38)),
            );
            ui.painter().text(
                egui::pos2(rect.left() + 8.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                &text,
                egui::FontId::proportional(15.0),
                hex_color(&hint_color, egui::Color32::from_rgb(0x80, 0x80, 0x80)),
            );
        }
```

- [ ] **Step 2: 視覚スモーク + コミット**

Run（失敗通知の視覚確認——存在しないパスを含む検索結果は作りにくいため、`Failed` は dead パスの `.lnk` か手元の無効ファイルで確認。最低限「起動中…」は遅い UNC で確認できるが、正常環境では一瞬で消えるため Failed 経路を主目標にする）:

```powershell
$env:SNOTRA_EGUI_MAIN = "1"; cargo run -p snotra
```

Expected: 起動失敗時（例: インデックス済みの削除済みファイル）に検索バーが通知色に変わり「起動に失敗しました (...)」が約 2.4 秒表示され、自動で消える。ウィンドウは hide しない

```powershell
git add src-tauri/src/egui_shell/view.rs
git commit -m "feat(#631): SU5 Task5 一時 overlay 描画（起動中/失敗通知・painted label）"
```

---

### Task 6: updater 状態 + check 配線

**Files:**
- Modify: `src-tauri/src/egui_shell/mod.rs`（`EguiShellState` に repaint 用 Context スロット + `UpdaterUiState` 型 + check spawn 関数）
- Modify: `src-tauri/src/egui_shell/view.rs`（`setup()` で Context を managed へ登録）
- Modify: `src-tauri/src/main.rs`（egui setup 分岐から check spawn を呼ぶ）

**Interfaces:**
- Consumes: `UpdaterUi`/`UpdaterPhase`（Task 1）、`tauri_plugin_updater::UpdaterExt`（`updater_builder().on_before_exit(..).build()` → `check().await`）、`AutoUpdateMode`（snotra_core::config）
- Produces: `pub(crate) struct UpdaterUiState(pub Mutex<UpdaterUi<Box<tauri_plugin_updater::Update>>>)`（managed）、`pub(crate) fn spawn_update_check(app: &AppHandle)`、`EguiShellState.egui_ctx: Mutex<Option<egui::Context>>`。Task 7 が `UpdaterUiState` を描画・操作、Task 8 が `flush_persistent_state` を hook に登録

- [ ] **Step 1: mod.rs に状態と check spawn を追加**

`EguiShellState` にフィールド追加（`#[derive(Default)]` は維持——`Mutex<Option<..>>` は Default 可）:

```rust
    /// updater check 完了時に可視中の view を起こすための egui Context（view.setup が登録）。
    /// hidden 中は次 show のフレームで toast が読まれるため repaint は可視中のみ意味を持つ
    /// （codex レビュー: 「hidden は次 show でよい」と「visible は repaint が要る」は別条件）。
    pub(crate) egui_ctx: Mutex<Option<egui::Context>>,
```

`EguiShellState` の後に追加:

```rust
use std::sync::Mutex;

/// updater toast の managed 状態（#532 SU5）。view が毎フレーム読む level-triggered
/// （hidden に頑健・launching の channel edge-trigger との構造的対比は spec C 節）。
/// dismissed は view-local に置かない——reset-on-show が view-local を一掃した際に
/// [閉じる] 済み toast が復活するため（状態機械レビュー・spec A 節）。
pub(crate) struct UpdaterUiState(pub(crate) Mutex<crate::egui_shell::UpdaterUi<Box<tauri_plugin_updater::Update>>>);

/// 起動時 updater check（§20.2・spec B 節）。`auto_update != disabled` で一回だけ呼ぶ。
/// `on_before_exit` に終了保存を登録した builder で check する——ここで得た `Update` の
/// install は「download → 保存 → installer 起動 → exit(0)」となり、保存が構造的に保証される
/// （Windows では downloadAndInstall が復帰しない・updater.rs:865・spec「決着済み: 保存順序」）。
pub(crate) fn spawn_update_check(app: &tauri::AppHandle) {
    use snotra_core::config::AutoUpdateMode;
    use tauri_plugin_updater::UpdaterExt;
    let mode = app
        .try_state::<crate::AppState>()
        .map(|s| s.engine.lock().unwrap().config().general.auto_update)
        .unwrap_or(AutoUpdateMode::Full);
    if mode == AutoUpdateMode::Disabled {
        return;
    }
    let can_install = mode == AutoUpdateMode::Full;
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Some(st) = handle.try_state::<UpdaterUiState>() {
            st.0.lock().unwrap().phase = crate::egui_shell::UpdaterPhase::Checking;
        }
        let flush_handle = handle.clone();
        let updater = handle
            .updater_builder()
            .on_before_exit(move || crate::flush_persistent_state(&flush_handle))
            .build();
        let next = match updater {
            Ok(u) => match u.check().await {
                Ok(Some(update)) => crate::egui_shell::UpdaterPhase::Available {
                    version: update.version.clone(),
                    can_install,
                    update: Box::new(update),
                },
                Ok(None) => crate::egui_shell::UpdaterPhase::UpToDate,
                Err(e) => {
                    // check 失敗は無音（console.warn parity・trace のみ）。
                    crate::trace_main("egui_update_check_failed", serde_json::json!({ "error": e.to_string() }));
                    crate::egui_shell::UpdaterPhase::Idle
                }
            },
            Err(e) => {
                crate::trace_main("egui_update_check_failed", serde_json::json!({ "error": e.to_string() }));
                crate::egui_shell::UpdaterPhase::Idle
            }
        };
        if let Some(st) = handle.try_state::<UpdaterUiState>() {
            st.0.lock().unwrap().phase = next;
        }
        // 可視中に check が完了した場合の wake-up（スパイクの request_repaint と同じ・codex レビュー）。
        if let Some(sh) = handle.try_state::<EguiShellState>()
            && let Ok(guard) = sh.egui_ctx.lock()
            && let Some(ctx) = guard.as_ref()
        {
            ctx.request_repaint();
        }
    });
}
```

（注意: `use std::sync::Mutex;` は mod.rs 冒頭の既存 use 群に統合。`flush_persistent_state` は Task 8 で main.rs に作る——**このタスクの時点ではコンパイルを通すため、Task 8 を先に読み、`flush_persistent_state` の関数だけ先行して main.rs に追加してよい**。手順を単純化するため本計画では Step 2 に含める）

- [ ] **Step 2: main.rs に保存専用ルーチンを切り出し、egui setup 分岐から check を呼ぶ**

`setup_exit_listener`（main.rs:943-978）の flush 部（history_save 取得〜icon save_if_dirty ブロックまで・main.rs:946-963）を関数へ抽出:

```rust
/// 終了時と updater install 前（`on_before_exit`）が共有する保存専用ルーチン（#532 SU5）。
/// exit-requested の flush 列は保存 + exit(0) の不可分列だったため、保存だけを再利用可能に
/// 切り出した（spec「決着済み: 保存順序」）。二重 flush（install 前 + 通常終了）は
/// `NEXT_SAVE_SEQUENCE` の単調ガードで安全（最新 seq 勝ち・並行性レビュー実測）。
pub(crate) fn flush_persistent_state(app_handle: &AppHandle) {
    // Capture a consistent final snapshot under the Engine lock, then flush
    // it without holding the lock through filesystem I/O.
    let history_save = {
        let app_state = app_handle.state::<AppState>();
        let mut engine = app_state.engine.lock().unwrap();
        engine.prepare_history_flush()
    };
    if let Some(save) = history_save {
        let _ = save.save();
    }
    {
        let icon_state = app_handle.state::<IconCacheState>();
        let mut cache = icon_state.lock().unwrap();
        if let Some(c) = cache.as_mut() {
            c.save_if_dirty();
        }
    }
}
```

`setup_exit_listener` の該当部を `flush_persistent_state(&handle_for_exit);` の 1 行に置換（child kill 以降は不変）。

egui setup 分岐（main.rs:679-685）の `register_hide_listener` の後に追加:

```rust
                app.manage(egui_shell::UpdaterUiState(std::sync::Mutex::new(Default::default())));
                egui_shell::spawn_update_check(&app_handle);
```

- [ ] **Step 3: view.setup() で Context を登録**

`setup()`（view.rs:890-897）の末尾に追加:

```rust
        // updater check 完了時の wake-up 用（mod.rs spawn_update_check が読む・#532 SU5）。
        if let Some(sh) = self.app_handle.try_state::<crate::egui_shell::EguiShellState>()
            && let Ok(mut guard) = sh.egui_ctx.lock()
        {
            *guard = Some(context.clone());
        }
```

- [ ] **Step 4: テスト + トレース確認 + コミット**

Run: `cargo test -p snotra --lib`
Expected: 全 PASS

Run: `$env:SNOTRA_EGUI_MAIN = "1"; $env:SNOTRA_TRACE = "1"; cargo run -p snotra 2>&1 | Select-String "egui_update"`
Expected: ネットワーク到達不能や最新版なら無出力（UpToDate）または `egui_update_check_failed` 1 行。パニック・ハングしない

```powershell
git add src-tauri/src/egui_shell/mod.rs src-tauri/src/egui_shell/view.rs src-tauri/src/main.rs
git commit -m "feat(#532): SU5 Task6 updater check 配線（UpdaterUiState + on_before_exit 保存）"
```

---

### Task 7: toast 描画 + 高さ配線 + fake 注入

**Files:**
- Modify: `src-tauri/src/egui_shell/view.rs`（toast 行描画 + `has_update_toast` 実値配線）
- Modify: `src-tauri/src/egui_shell/mod.rs`（fake 注入 env・視覚スモーク用）
- Modify: `src-tauri/src/egui_shell/notify.rs`（`Available.update` を `Option<U>` 化——fake が実 `Update` を構築できないため）

**Interfaces:**
- Consumes: `UpdaterUiState`（Task 6）、`ToastRow`/`ToastKind`（Task 1）、`ui_strings::update_*`（Task 2）、`HeightParams.has_update_toast`（layout.rs・既設）
- Produces: toast の click 結果 `Option<Box<Update>>`（install 開始・Task 8 が消費）。`UpdaterPhase::Available { update: Option<U> }`（型変更）

- [ ] **Step 1: notify.rs の `Available.update` を `Option<U>` 化**

`UpdaterPhase::Available` の `update: U` を `update: Option<U>` に変更し、`try_begin_install` を修正:

```rust
            UpdaterPhase::Available { version, can_install: true, update: Some(update) } => {
                self.phase = UpdaterPhase::Installing { version };
                Some(update)
            }
```

`update: None`（fake）で `try_begin_install` すると None を返し**局面は Available のまま残る**分岐を追加:

```rust
            UpdaterPhase::Available { version, can_install: true, update: None } => {
                // fake 注入（SNOTRA_EGUI_FAKE_UPDATE・視覚スモーク専用）: install 実体なし。
                self.phase = UpdaterPhase::Available { version, can_install: true, update: None };
                None
            }
```

Task 1 のテストを `update: Some("U")` / fake 分岐テストに更新:

```rust
    #[test]
    fn fake_available_without_update_cannot_install() {
        let mut u: UpdaterUi<&'static str> = UpdaterUi::default();
        u.phase = UpdaterPhase::Available { version: "9.9.9".into(), can_install: true, update: None };
        assert!(u.try_begin_install().is_none());
        assert!(matches!(u.phase, UpdaterPhase::Available { .. }), "fake は Available のまま");
    }
```

Task 6 の `spawn_update_check` 内 `update: Box::new(update)` を `update: Some(Box::new(update))` に修正。

- [ ] **Step 2: fake 注入 env を spawn_update_check 冒頭に追加**

```rust
    // 視覚スモーク専用（SNOTRA_DISABLE_SUSPEND と同じ E2E エスケープハッチの流儀）:
    // 実 release への依存なしに toast を表示する。install 実体は無い（update: None）。
    if crate::trace::env_flag("SNOTRA_EGUI_FAKE_UPDATE") {
        if let Some(st) = app.try_state::<UpdaterUiState>() {
            st.0.lock().unwrap().phase = crate::egui_shell::UpdaterPhase::Available {
                version: "9.9.9".into(),
                can_install: true,
                update: None,
            };
        }
        return;
    }
```

- [ ] **Step 3: view.rs に toast 描画 + 高さ配線**

Task 5 の overlay 描画ブロックの**後**（結果リスト描画の前）に追加:

```rust
        // updater toast（§20.3・#532 SU5）: 検索バー直下の 52px 行・モード非依存
        //（folder/tool/instant 中も表示・状態機械レビュー項 1）。
        let toast_row = self
            .app_handle
            .try_state::<crate::egui_shell::UpdaterUiState>()
            .and_then(|st| st.0.lock().unwrap().toast());
        let has_toast = toast_row.is_some();
        let mut toast_action: Option<ToastAction> = None;
        if let Some(row) = toast_row {
            let l = self.lang();
            let theme = self.row_theme();
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), 52.0),
                egui::Sense::hover(),
            );
            let line1 = match &row.kind {
                crate::egui_shell::ToastKind::Available { version } => {
                    crate::egui_shell::ui_strings::update_available(l, version)
                }
                crate::egui_shell::ToastKind::Installing => {
                    crate::egui_shell::ui_strings::update_installing(l).to_string()
                }
                crate::egui_shell::ToastKind::Failed { .. } => {
                    crate::egui_shell::ui_strings::update_failed(l).to_string()
                }
            };
            ui.painter().text(
                egui::pos2(rect.left() + 8.0, rect.top() + 13.0),
                egui::Align2::LEFT_CENTER,
                &line1,
                egui::FontId::proportional(13.0),
                theme.name_color,
            );
            // 行2: ボタン（右寄せ・installing 中は disabled・WebView2 UpdateToast parity）。
            let mut cursor_x = rect.right() - 8.0;
            let btn_y = rect.top() + 39.0;
            let dismiss_label = crate::egui_shell::ui_strings::update_dismiss(l);
            if draw_toast_button(ui, &mut cursor_x, btn_y, dismiss_label, row.buttons_enabled, &theme) {
                toast_action = Some(ToastAction::Dismiss);
            }
            if row.show_install {
                let install_label = crate::egui_shell::ui_strings::update_install_now(l);
                if draw_toast_button(ui, &mut cursor_x, btn_y, install_label, row.buttons_enabled, &theme) {
                    toast_action = Some(ToastAction::Install);
                }
            }
        }
        if let Some(action) = toast_action {
            self.handle_toast_action(action);
        }
```

view.rs のトップレベル（`RowTheme` の近く）にボタンヘルパーと action 型を追加:

```rust
/// toast ボタン種別（クリック結果を borrow 外で処理するための遅延 dispatch）。
enum ToastAction {
    Install,
    Dismiss,
}

/// 右端から左へ詰める toast ボタン 1 個。クリックされたら true。disabled は淡色 + 無反応。
fn draw_toast_button(
    ui: &mut egui::Ui,
    cursor_x: &mut f32,
    center_y: f32,
    label: &str,
    enabled: bool,
    theme: &RowTheme,
) -> bool {
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        egui::FontId::proportional(12.0),
        theme.name_color,
    );
    let w = galley.size().x + 16.0;
    let rect = egui::Rect::from_min_max(
        egui::pos2(*cursor_x - w, center_y - 11.0),
        egui::pos2(*cursor_x, center_y + 11.0),
    );
    *cursor_x -= w + 8.0;
    let response = ui.interact(rect, ui.next_auto_id(), egui::Sense::click());
    let color = if enabled { theme.name_color } else { theme.path_color };
    ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0, color), egui::StrokeKind::Inside);
    ui.painter().galley(
        egui::pos2(rect.left() + 8.0, center_y - galley.size().y / 2.0),
        galley,
        color,
    );
    enabled && response.clicked()
}
```

（`egui::StrokeKind` が現行 egui バージョンに無い場合は `rect_stroke(rect, 4.0, egui::Stroke::new(1.0, color))` の旧シグネチャを使う——コンパイルエラーに従う）

`handle_toast_action` を impl に追加（Task 8 で install 本体を実装するため、このタスクでは dismiss + install の取り出しまで）:

```rust
    /// toast ボタンの処理（#532 SU5）。install は Update を原子取得して async へ（Task 8）。
    fn handle_toast_action(&mut self, action: ToastAction) {
        let Some(st) = self.app_handle.try_state::<crate::egui_shell::UpdaterUiState>() else {
            return;
        };
        match action {
            ToastAction::Dismiss => {
                let _ = st.0.lock().unwrap().dismiss(); // Installing 中は拒否（false）＝無視
            }
            ToastAction::Install => {
                let taken = st.0.lock().unwrap().try_begin_install();
                if let Some(update) = taken {
                    self.spawn_install(update);
                } else {
                    crate::trace_main("egui_update_install_noop", serde_json::json!({}));
                }
            }
        }
    }
```

高さ配線（view.rs:1254-1262）の `has_update_toast: false, // SU5` を実値へ:

```rust
            has_update_toast: has_toast,
```

（`spawn_install` は Task 8 で実装。**このタスクをコンパイル可能に保つため**、Task 8 を続けて実施するか、暫定で `fn spawn_install(&self, _update: Box<tauri_plugin_updater::Update>) {}` の空実装を置き Task 8 で本実装に差し替える——空実装を置いた場合はコミットメッセージに `(install は Task8)` を明記）

- [ ] **Step 4: 視覚スモーク + テスト + コミット**

Run: `cargo test -p snotra --lib`
Expected: 全 PASS（notify の型変更に追従した Task 1 テスト含む）

Run: `$env:SNOTRA_EGUI_MAIN = "1"; $env:SNOTRA_EGUI_FAKE_UPDATE = "1"; cargo run -p snotra`
Expected: Alt+Q 表示で検索バー直下に 52px の toast（「v9.9.9 が利用可能です」+ [今すぐ更新] [閉じる]）。ウィンドウ高さが 52→104px。結果リスト表示時は toast の下に結果。[閉じる] で toast が消え高さが戻る。hide → 再 show で dismissed が維持される（toast 復活しない）

```powershell
git add src-tauri/src/egui_shell/view.rs src-tauri/src/egui_shell/mod.rs src-tauri/src/egui_shell/notify.rs
git commit -m "feat(#532): SU5 Task7 updater toast 描画 + 高さ配線 + fake 注入"
```

---

### Task 8: install 列（download_and_install + InstallFailed）

**Files:**
- Modify: `src-tauri/src/egui_shell/view.rs`（`spawn_install` 本実装）

**Interfaces:**
- Consumes: `Box<tauri_plugin_updater::Update>`（Task 7 の try_begin_install）、`UpdaterUiState`・`EguiShellState.egui_ctx`（Task 6）
- Produces: なし（終端。成功時はプロセスが plugin 内部で exit(0)）

- [ ] **Step 1: spawn_install を実装**

```rust
    /// install 実行（§20.4・spec B 節）。`download_and_install` は Windows では内部で
    /// download → `on_before_exit`（=flush_persistent_state・Task 6 で builder に登録済み）→
    /// installer 起動 → `std::process::exit(0)` し**復帰しない**（updater.rs:865）。
    /// Err 復帰時のみ InstallFailed へ遷移して toast をエラー表示にする（updaterError parity）。
    fn spawn_install(&self, update: Box<tauri_plugin_updater::Update>) {
        let handle = self.app_handle.clone();
        crate::trace_main("egui_update_install_begin", serde_json::json!({ "version": update.version }));
        tauri::async_runtime::spawn(async move {
            match update.download_and_install(|_, _| {}, || {}).await {
                Ok(()) => {
                    // Windows では到達しない（内部 exit）。他 OS ビルドや将来変更の防波堤として trace。
                    crate::trace_main("egui_update_install_returned", serde_json::json!({}));
                }
                Err(e) => {
                    crate::trace_main(
                        "egui_update_install_failed",
                        serde_json::json!({ "error": e.to_string() }),
                    );
                    if let Some(st) = handle.try_state::<crate::egui_shell::UpdaterUiState>() {
                        st.0.lock().unwrap().phase =
                            crate::egui_shell::UpdaterPhase::InstallFailed { message: e.to_string() };
                    }
                    if let Some(sh) = handle.try_state::<crate::egui_shell::EguiShellState>()
                        && let Ok(guard) = sh.egui_ctx.lock()
                        && let Some(ctx) = guard.as_ref()
                    {
                        ctx.request_repaint(); // 可視中の失敗を即座に描く
                    }
                }
            }
        });
    }
```

- [ ] **Step 2: テスト + コミット**

Run: `cargo test -p snotra --lib`
Expected: 全 PASS

（install の end-to-end は署名付き実 release が要るため実行しない——実装時スモークは Task 10 の項目 3 で「Err 経路のみ」確認）

```powershell
git add src-tauri/src/egui_shell/view.rs
git commit -m "feat(#532): SU5 Task8 updater install 列（on_before_exit 保存 + InstallFailed）"
```

---

### Task 9: SPEC 同期 + ドキュメント索引

**Files:**
- Modify: `SPEC.md`（§19.6・§20.3・§20.4・§8.6 note）
- Modify: `src-tauri/CLAUDE.md`（egui_shell モジュール一覧に notify.rs / strings.rs）

**Interfaces:** なし（文書のみ）

- [ ] **Step 1: SPEC §19.6 に egui 経路の as-built を追記**

「#### 起動結果」の後に追加:

```markdown
#### egui 経路の起動保護（#532 SU5）

- WebView2 経路の `spawn_blocking` + 4 秒タイムアウトに対応する保護として、egui 経路は
  per-launch 専用スレッド + フレーム drain で起動を実行する（通常起動・ツール起動・
  インスタント実行の 3 経路とも）。イベントループスレッドで `ShellExecuteW` / `spawn` を
  同期実行しない
- single-flight: in-flight 起動中の新規起動要求（Enter/クリック）は拒否する。打鍵は
  入力欄の無効化で抑止する。Escape / blur / ホットキーによる手動 hide は launching 中も通す
  （成功時の自動 hide のみ完了後）
- 4 秒経過は「起動失敗」ではなく**結果不明**として扱い、一時通知（`notice.launch.timeout`
  文言）を表示して in-flight 追跡を破棄する。起動という副作用は取り消せない（`spawn_blocking`
  の abandoned task と同じ意味論）。遅着した結果は破棄する（per-launch channel の drop で構造的に消滅）
- 履歴記録は worker スレッド側で成功時に行う（ウィンドウ可視性と無関係・WebView2 の
  backend 記録と parity）
```

- [ ] **Step 2: SPEC §20.3 / §20.4 を egui as-built + Windows 実挙動へ是正**

§20.3 末尾に追加:

```markdown
- egui 経路（#532 SU5）: toast は検索バー直下の 52px 行としてモード（フォルダ展開・
  ツール選択・インスタントコマンド）非依存に描画し、ウィンドウ高さに加算する。
  インストール中は [今すぐ更新] [閉じる] とも disabled。[閉じる] はセッション中恒久
  （再表示で復活しない）。show 時は 52px collapse 後に toast 分へ拡張する（1 フレームの
  高さスナップを受容）
```

§20.4 を以下へ置換（Windows 実挙動の是正・スパイク #580 の申し送りの明文化）:

```markdown
### 20.4 更新フロー（`full` モード）

1. 起動時に更新を確認し、`Update` オブジェクトを保持する（WebView2: フロントエンドの
   `check()` / egui: Rust `UpdaterExt` の check。egui は `on_before_exit` フックに終了保存
   （履歴 flush + アイコン保存）を登録した builder で check する）
2. トーストの [今すぐ更新] で `downloadAndInstall()` を実行
3. **Windows では `downloadAndInstall` は復帰しない**: プラグインが内部で download →
   `on_before_exit` フック → NSIS installer 起動 → `std::process::exit(0)` する。
   プロセスの終了・再起動は NSIS インストーラに委ねる（`app.restart()` は新プロセスが
   ファイルをロックし NSIS の上書きを失敗させるため使わない）。**`downloadAndInstall`
   復帰後に保存処理を置かない**（到達しないため・保存は `on_before_exit` が正しい合流点）
4. `Err` 復帰（download 失敗等）時のみトーストをエラー表示にする
```

- [ ] **Step 3: SPEC §8.6 に overlay note を追記**

§8.6 の状態図の説明部（note 群）に追加（状態ノードは増やさない・状態機械レビュー項 7）:

```markdown
- `launching`（起動 in-flight）・一時通知・updater トーストは状態ノードではなく
  `IndexingMode` と同様の overlay（どのモードにも重なる直交 boolean）。手動 hide
  （Escape / blur / ホットキー）は launching 中も成立し、成功時の自動 hide のみ
  起動完了後に行われる。表示時リセットで launching と一時通知はクリアされ、
  updater トースト（と dismissed）は維持される
```

- [ ] **Step 4: src-tauri/CLAUDE.md の egui_shell 行を更新**

モジュール構成の `egui_shell/` 行のファイル列挙に `notify.rs` / `strings.rs` を追加し、責務の一文を追記:

```markdown
`notify.rs` は通知 primitive の純粋核（一時通知 NoticeSlot + updater toast 状態機械 UpdaterUi・#532 SU5）、`strings.rs` は egui 経路の UI 文言テーブル（i18n.ts と同文言・言語は config 起動時読み）、
```

- [ ] **Step 5: governance:check + コミット**

Run: `npm run governance:check`
Expected: G1..G10 passed

Run: SPEC 編集は `.claude/rules/spec.md` 配送済み——セクション番号のずれが無いことを目視確認（§20.5 以降の番号は不変のはず）

```powershell
git add SPEC.md src-tauri/CLAUDE.md
git commit -m "docs(#532): SU5 Task9 SPEC 同期（§19.6/§20.3/§20.4/§8.6）+ モジュール索引"
```

---

### Task 10: 実装時スモーク（hidden 要石ほか）+ 全体検証

**Files:** なし（検証のみ。発見があれば view.rs を修正）

- [ ] **Step 1: 全テスト + clippy**

Run: `cargo test -p snotra --lib` および `cargo clippy -p snotra --all-targets`
Expected: 全 PASS・warning 0

- [ ] **Step 2: hidden 中 drain の要石スモーク（spec C 節）**

手順（`SNOTRA_EGUI_MAIN=1` + `SNOTRA_TRACE=1` で起動）:
1. 遅い起動を作る（到達不能な UNC パス `\\192.0.2.1\share\x.exe` への `.lnk` をインデックス対象に置く、または任意の遅延起動対象）
2. Enter 起動 → 即 Alt+Q で hide → 5 秒後に Alt+Q で再 show
3. トレースを観測: 再 show 後のフレームで `egui_launch_done` がいつ出るか・「起動中…」overlay が再 show 後に残っていないか（reset backstop の確認）

Expected（どちらでも安全＝spec の要石デザインの検証）:
- hidden 中に update() が走る場合: hide 中に timeout 通知系トレースが出る
- 走らない場合: 再 show 直後に reset が launching をクリアし、overlay なしのクリーン状態（stale hide が発火しない＝ウィンドウが勝手に消えない）

**再 show した窓が勝手に hide する・古い「起動中…」が残る場合は必ず修正**（drain 位置が reset より前にある兆候）。観測結果（どちらの分岐だったか）を PR 本文へ記録する

- [ ] **Step 3: 主要経路の回帰スモーク**

`SNOTRA_EGUI_MAIN=1` で一通り: 通常検索 Enter 起動（hide + 履歴反映）/ 起動失敗（通知表示・hide しない）/ instant 実行（`@` プレフィックス）/ tool 選択起動（Shift+Enter）/ folder 展開（→←）/ flush-on-Enter（高速打鍵直後の Enter で最終クエリの先頭行が起動）/ `SNOTRA_EGUI_FAKE_UPDATE=1` の toast 表示・dismiss・高さ。WebView2 経路（フラグ無し）が不変であることも 1 度起動して確認（G1）

- [ ] **Step 4: ロードマップ進捗の更新 + 最終コミット**

`docs/superpowers/specs/2026-07-21-phase2-softbuffer-migration-roadmap.md` の「進捗」節に SU5 完了を追記（PR 番号は PR 作成後に確定するため、このコミットでは「SU5 実装完了（PR 未マージ）」の行を足す形でよい）。

```powershell
git add docs/superpowers/specs/2026-07-21-phase2-softbuffer-migration-roadmap.md
git commit -m "docs(#532): SU5 スモーク結果反映 + ロードマップ進捗"
```

PR 作成は計画外（ユーザーの指示で `git push -u origin HEAD && gh pr create` 列に従う。PR 前に `closingIssuesReferences` 手順——ルート CLAUDE.md「Git/GitHub 運用」——を必ず踏む。#631 は本 PR で close 対象、#532 は close しない）

---

## Self-Review 済みの注意（実装者向け）

- **`QueryIntent` の import**: Task 3 の Enter ブロックで `QueryIntent` を使う——view.rs は既に import 済み（view.rs:20）
- **`OpenerTool` の import**: Task 4 の `LaunchWork::Normal.tools` で使用——view.rs は既に import 済み（view.rs:13）
- **旧 `activate`/`execute_tool_selected`/`execute_instant_selected` の同期実行コード**（`launch_item_core` 直呼び・`record_and_save` UI スレッド呼び・成功時 hide）は Task 4 で**完全に削除**する。`use` 文に残った未使用 import は hook の clippy が検出する
- **Task 6 と Task 8 の順序依存**: `spawn_update_check` が `flush_persistent_state` を参照するため Task 6 Step 2 で main.rs の切り出しを先に行う（計画に織り込み済み）
- **タスク実行順**: 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9 → 10 の直列（全タスクが view.rs か mod.rs に触れるため並列委譲はしない——ルート CLAUDE.md「ファイル境界で衝突を予測」）
