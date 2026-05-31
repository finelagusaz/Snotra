use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::Duration;

use notify::{Event, EventKind, RecursiveMode, Watcher};
use snotra_core::config::{Config, Language, LoadOutcome};
use tauri::window::Color;
use tauri::{AppHandle, Emitter, LogicalSize, Manager};

use crate::indexing;
use crate::platform::{PlatformBridge, PlatformCommand};
use crate::state::AppState;

/// Parse a CSS hex color string (e.g. "#282828") into a Tauri `Color`.
fn parse_hex_color(hex: &str) -> Option<Color> {
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color(r, g, b, 255))
}

/// Set the native window/WebView2 background color to match the theme.
/// This prevents a white flash when the window is resized to show results.
pub(crate) fn sync_webview_background(app: &AppHandle, bg_color_hex: &str) {
    if let Some(color) = parse_hex_color(bg_color_hex)
        && let Some(w) = app.get_webview_window("main")
    {
        let _ = w.set_background_color(Some(color));
    }
}

/// Start watching `config.toml` for external changes (e.g. from snotra-settings).
///
/// Returns the watcher handle which must be kept alive for the duration of the app.
/// Dropping the handle stops watching.
pub fn start(app_handle: &AppHandle) -> Option<notify::RecommendedWatcher> {
    let config_path = Config::config_path()?;
    let config_dir = config_path.parent()?;
    let config_filename = config_path.file_name()?.to_owned();

    let handle = app_handle.clone();
    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        let Ok(event) = res else { return };

        // Only react to write/create/rename events
        match event.kind {
            EventKind::Modify(_) | EventKind::Create(_) => {}
            _ => return,
        }

        // Only react if config.toml was the file affected
        let is_config = event
            .paths
            .iter()
            .any(|p| p.file_name().is_some_and(|n| n == config_filename));
        if !is_config {
            return;
        }

        // Debounce: small delay to let atomic rename complete
        std::thread::sleep(Duration::from_millis(100));

        apply_config_change(&handle);
    })
    .ok()?;

    // Watch the directory (not the file) because atomic write creates a new file
    watcher.watch(config_dir, RecursiveMode::NonRecursive).ok()?;

    Some(watcher)
}

/// Load config from disk and apply changes, mirroring save_config logic.
fn apply_config_change(app: &AppHandle) {
    // 一時的ロック等による ReadFailed は予算内で再読込を試みる（正規の変更を取りこぼさない）。
    let (new_config, load_outcome) = load_with_read_failed_retry(
        Config::load_reporting,
        || std::thread::sleep(CONFIG_READ_RETRY_BACKOFF),
        CONFIG_READ_RETRY_MAX,
    );

    // リトライ予算を使い切っても ReadFailed のままなら、fallback-default を実行中エンジンへ
    // 適用しない。適用すると live-read 化した履歴剪定（#348）が default 上限で走り history.bin が
    // 不可逆に縮むデータ損失が起き、needs_reindex も default scan で誤再構築を起こす。ファイルは
    // 無傷なので、ロックが解けた次の保存イベントで正規の変更を拾う。
    if !should_apply_config_change(load_outcome) {
        eprintln!(
            "[config-watcher] config read failed (transient, retries exhausted); keeping current config (no apply)"
        );
        return;
    }

    let state = app.state::<AppState>();
    let old_config = state.engine.lock().unwrap().config().clone();

    // Detect changes
    let show_icons_changed = new_config.appearance.show_icons != old_config.appearance.show_icons;
    let new_show_icons = new_config.appearance.show_icons;
    let index_changed = needs_reindex(&old_config, &new_config);
    let language_changed = new_config.general.language != old_config.general.language;
    let new_language = new_config.general.language;
    let instant_prefix_changed = new_config.search.instant_command_prefix
        != old_config.search.instant_command_prefix;
    let new_instant_prefix = new_config.search.instant_command_prefix.clone();
    let visual_changed = new_config.visual != old_config.visual;
    let max_results_changed =
        new_config.appearance.max_results != old_config.appearance.max_results;
    let new_max_results = new_config.appearance.max_results;
    let new_top_n_history = new_config.search.effective_top_n_history();
    let top_n_history_changed =
        new_top_n_history != old_config.search.effective_top_n_history();
    let width_changed =
        new_config.appearance.window_width != old_config.appearance.window_width;
    let new_visual = if visual_changed {
        Some(new_config.visual.clone())
    } else {
        None
    };
    let new_width = new_config.appearance.window_width;

    // Emit language change BEFORE hotkey failure so the frontend receives
    // the new language before it formats any error notification strings.
    if language_changed {
        let lang_str = match new_language {
            Language::Ja => "ja",
            Language::En => "en",
        };
        let _ = app.emit("language-changed", lang_str);
    }

    // Hotkey change — best-effort, log on failure (don't block)
    if let Some(bridge) = app.try_state::<Mutex<PlatformBridge>>()
        && let Ok(b) = bridge.lock()
    {
        if new_config.hotkey != old_config.hotkey {
            let (tx, rx) = std::sync::mpsc::channel();
            b.send_command(PlatformCommand::SetHotkey {
                config: new_config.hotkey.clone(),
                reply: tx,
            });
            match rx.recv_timeout(Duration::from_secs(2)) {
                Ok(false) | Err(_) => {
                    eprintln!(
                        "[config-watcher] hotkey registration failed: {} + {}",
                        new_config.hotkey.modifier, new_config.hotkey.key
                    );
                    let hotkey_str =
                        format!("{}+{}", new_config.hotkey.modifier, new_config.hotkey.key);
                    let _ = app.emit("hotkey-registration-failed", hotkey_str);
                }
                Ok(true) => {}
            }
        }
        if new_config.general.show_tray_icon != old_config.general.show_tray_icon {
            b.send_command(PlatformCommand::SetTrayVisible(
                new_config.general.show_tray_icon,
            ));
        }
        if language_changed {
            b.send_command(PlatformCommand::SetLanguage(new_language));
        }
        // 実行中に config が壊れて既定値に復旧した場合、トレイバルーンで通知する。
        // SetLanguage の後に送り、バルーン文言が更新後の言語で表示されるようにする。
        if load_outcome == LoadOutcome::RecoveredFromCorrupt {
            b.send_command(PlatformCommand::ShowConfigRecoveryBalloon);
        }
    }

    // Update engine config
    {
        state.engine.lock().unwrap().update_config(new_config);
    }

    // Trigger reindex if needed
    let indexing_in_progress = state.indexing.load(Ordering::SeqCst);
    if index_changed && !indexing_in_progress {
        indexing::start_index_build(app);
    }

    // Emit visual config change and sync native background color
    if let Some(visual) = new_visual {
        sync_webview_background(app, &visual.background_color);
        let _ = app.emit("visual-config-changed", &visual);
    }

    // Emit show_icons change
    if show_icons_changed {
        let _ = app.emit("show-icons-changed", new_show_icons);
    }

    // Emit max_results change
    if max_results_changed {
        let _ = app.emit("max-results-changed", new_max_results);
    }

    // Emit instant command prefix change
    if instant_prefix_changed {
        let _ = app.emit("instant-prefix-changed", new_instant_prefix);
    }

    // Emit top_n_history change
    if top_n_history_changed {
        let _ = app.emit("top-n-history-changed", new_top_n_history);
    }

    // Resize main window if width changed
    if width_changed
        && new_width > 0
        && let Some(w) = app.get_webview_window("main")
        && let Ok(size) = w.inner_size()
        && let Ok(sf) = w.scale_factor()
    {
        let logical = size.to_logical::<f64>(sf);
        let _ = w.set_size(LogicalSize::new(f64::from(new_width), logical.height));
    }
}

/// インデックス再構築が必要かの判定ロジック。apply_config_change から抽出。
/// scan / show_hidden_system / show_icons / include_path_env / migemo_enabled の
/// いずれかが変わった場合 true。
/// migemo_enabled: kana_lower_names の構築状態を変えるため再構築が必要（issue #337）。
/// update_config は engine を再構築しないので、トグルの反映はこの reindex 経路に依存する。
pub(crate) fn needs_reindex(old: &Config, new: &Config) -> bool {
    old.paths.scan != new.paths.scan
        || old.search.show_hidden_system != new.search.show_hidden_system
        || old.appearance.show_icons != new.appearance.show_icons
        || old.search.include_path_env != new.search.include_path_env
        || old.search.migemo_enabled != new.search.migemo_enabled
}

/// `config.toml` の読込結果を実行中エンジンへ適用してよいかの判定。
/// `ReadFailed`（一時的・環境的な read 失敗: 権限/ロック/共有違反等）では fallback-default を
/// 適用しない。適用すると `top_n_history` 等が default に落ち、live-read 化した履歴剪定が
/// default 上限で走って `history.bin` が不可逆に縮む（データ損失）うえ、default scan で
/// 誤った再構築も走る。`Config::load` の「一時的失敗は退避も上書きもしない」保全方針
/// （snotra-core/CLAUDE.md）を apply 側にも揃える。ファイルは無傷なので、次の保存イベントで
/// 正規の変更を拾う。`RecoveredFromCorrupt`（真の破損・.bak 退避済み）は #343 の意図的 default
/// 適用なので適用する。
pub(crate) fn should_apply_config_change(outcome: LoadOutcome) -> bool {
    !matches!(outcome, LoadOutcome::ReadFailed)
}

/// config 読み込みのリトライ予算（一時的ロック解除を待つ）。
const CONFIG_READ_RETRY_MAX: u32 = 3;
const CONFIG_READ_RETRY_BACKOFF: Duration = Duration::from_millis(150);

/// `ReadFailed`（一時的・環境的なロック等）のときだけ `backoff` を挟んで最大 `max_retries` 回
/// `load` を再試行する。ReadFailed 以外（Loaded/FirstRun/RecoveredFromCorrupt）が返れば即座に
/// それを返す（リトライしない＝`.bak` 暴発を避ける）。バウンド付きで必ず終了する。
///
/// 一時的ロックが予算内で解ければ正規の変更を取りこぼさず適用でき（Codex P2 / MECE の D2b 対策）、
/// 予算超過の永続ロックでは最後の ReadFailed を返し、呼び出し側が apply をスキップする
/// （fallback-default を適用しないデータ損失安全を維持＝両不変条件を同時に満たす）。
///
/// `load`/`sleep` を注入可能にしてディスク・実時間に依存せずユニットテストする。
fn load_with_read_failed_retry<F, S>(
    mut load: F,
    mut sleep: S,
    max_retries: u32,
) -> (Config, LoadOutcome)
where
    F: FnMut() -> (Config, LoadOutcome),
    S: FnMut(),
{
    let (mut config, mut outcome) = load();
    let mut retries = 0;
    while outcome == LoadOutcome::ReadFailed && retries < max_retries {
        sleep();
        (config, outcome) = load();
        retries += 1;
    }
    (config, outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::window::Color;

    #[test]
    fn parse_hex_color_valid() {
        assert_eq!(parse_hex_color("#282828"), Some(Color(0x28, 0x28, 0x28, 255)));
        assert_eq!(parse_hex_color("#FF00AA"), Some(Color(0xFF, 0x00, 0xAA, 255)));
        assert_eq!(parse_hex_color("#000000"), Some(Color(0, 0, 0, 255)));
        assert_eq!(parse_hex_color("#ffffff"), Some(Color(0xFF, 0xFF, 0xFF, 255)));
    }

    #[test]
    fn parse_hex_color_no_hash() {
        assert_eq!(parse_hex_color("282828"), None);
    }

    #[test]
    fn parse_hex_color_wrong_length() {
        assert_eq!(parse_hex_color("#28282"), None);
        assert_eq!(parse_hex_color("#2828288"), None);
    }

    #[test]
    fn parse_hex_color_invalid_hex() {
        assert_eq!(parse_hex_color("#gggggg"), None);
    }

    #[test]
    fn parse_hex_color_empty() {
        assert_eq!(parse_hex_color(""), None);
        assert_eq!(parse_hex_color("#"), None);
    }

    #[test]
    fn needs_reindex_no_change() {
        let config = Config::load();
        assert!(!needs_reindex(&config, &config));
    }

    #[test]
    fn needs_reindex_scan_change() {
        let old = Config::load();
        let mut new = old.clone();
        new.paths.scan.push(snotra_core::config::ScanPath {
            path: "C:\\new".into(),
            extensions: vec![".exe".into()],
            include_folders: false,
        });
        assert!(needs_reindex(&old, &new));
    }

    #[test]
    fn needs_reindex_show_icons_change() {
        let old = Config::load();
        let mut new = old.clone();
        new.appearance.show_icons = !old.appearance.show_icons;
        assert!(needs_reindex(&old, &new));
    }

    #[test]
    fn needs_reindex_include_path_env_change() {
        let old = Config::load();
        let mut new = old.clone();
        new.search.include_path_env = !old.search.include_path_env;
        assert!(needs_reindex(&old, &new));
    }

    #[test]
    fn needs_reindex_show_hidden_system_change() {
        let old = Config::load();
        let mut new = old.clone();
        new.search.show_hidden_system = !old.search.show_hidden_system;
        assert!(needs_reindex(&old, &new));
    }

    #[test]
    fn needs_reindex_migemo_change() {
        // migemo トグルは kana_lower_names の構築状態を変えるため reindex が必要（issue #337）。
        let old = Config::load();
        let mut new = old.clone();
        new.search.migemo_enabled = !old.search.migemo_enabled;
        assert!(needs_reindex(&old, &new));
    }

    #[test]
    fn needs_reindex_unrelated_change_does_not_trigger() {
        let old = Config::load();
        let mut new = old.clone();
        new.appearance.max_results = old.appearance.max_results + 10;
        assert!(!needs_reindex(&old, &new));
    }

    #[test]
    fn should_apply_config_change_skips_read_failed() {
        // 一時的 read 失敗（権限/ロック等）の fallback-default は実行中エンジンへ適用しない。
        // 適用すると live-read 化した履歴剪定が default 上限で走り history.bin が不可逆に縮む
        // （Codex アドバーサリアルレビュー検出のデータ損失経路）。
        assert!(!should_apply_config_change(LoadOutcome::ReadFailed));
    }

    #[test]
    fn should_apply_config_change_applies_normal_outcomes() {
        // 正常読込・first-run・破損復旧（#343 の意図的 default 適用）は適用する。
        assert!(should_apply_config_change(LoadOutcome::Loaded));
        assert!(should_apply_config_change(LoadOutcome::FirstRun));
        assert!(should_apply_config_change(LoadOutcome::RecoveredFromCorrupt));
    }

    #[test]
    fn load_retry_returns_first_success_without_retrying() {
        // ReadFailed でなければ即返す（リトライしない）。
        let calls = std::cell::Cell::new(0usize);
        let sleeps = std::cell::Cell::new(0u32);
        let load = || {
            calls.set(calls.get() + 1);
            (Config::default(), LoadOutcome::Loaded)
        };
        let sleep = || sleeps.set(sleeps.get() + 1);
        let (_c, outcome) = load_with_read_failed_retry(load, sleep, 3);
        assert_eq!(outcome, LoadOutcome::Loaded);
        assert_eq!(calls.get(), 1, "成功時は再試行しない");
        assert_eq!(sleeps.get(), 0, "成功時は backoff しない");
    }

    #[test]
    fn load_retry_does_not_retry_recovered_from_corrupt() {
        // 破損復旧（.bak 退避済み）はリトライ対象外（部分読み再試行で .bak を暴発させない）。
        let calls = std::cell::Cell::new(0usize);
        let load = || {
            calls.set(calls.get() + 1);
            (Config::default(), LoadOutcome::RecoveredFromCorrupt)
        };
        let (_c, outcome) = load_with_read_failed_retry(load, || {}, 3);
        assert_eq!(outcome, LoadOutcome::RecoveredFromCorrupt);
        assert_eq!(calls.get(), 1, "ReadFailed 以外はリトライしない");
    }

    #[test]
    fn load_retry_recovers_when_lock_clears_within_budget() {
        // ReadFailed が続いた後に解ければ、その成功結果を返す（Codex P2 / D2b 対策）。
        let calls = std::cell::Cell::new(0usize);
        let sleeps = std::cell::Cell::new(0u32);
        let seq = [
            LoadOutcome::ReadFailed,
            LoadOutcome::ReadFailed,
            LoadOutcome::Loaded,
        ];
        let load = || {
            let i = calls.get();
            calls.set(i + 1);
            (Config::default(), seq[i.min(seq.len() - 1)])
        };
        let sleep = || sleeps.set(sleeps.get() + 1);
        let (_c, outcome) = load_with_read_failed_retry(load, sleep, 3);
        assert_eq!(outcome, LoadOutcome::Loaded, "予算内でロック解除→成功結果を採用");
        assert_eq!(calls.get(), 3, "2 回 ReadFailed の後 3 回目で成功");
        assert_eq!(sleeps.get(), 2, "再試行ごとに backoff");
    }

    #[test]
    fn load_retry_gives_up_after_max_retries_and_terminates() {
        // 永続ロックでは予算で打ち切り最後の ReadFailed を返す（無限ループしない＝終了保証）。
        let calls = std::cell::Cell::new(0usize);
        let sleeps = std::cell::Cell::new(0u32);
        let load = || {
            calls.set(calls.get() + 1);
            (Config::default(), LoadOutcome::ReadFailed)
        };
        let sleep = || sleeps.set(sleeps.get() + 1);
        let (_c, outcome) = load_with_read_failed_retry(load, sleep, 3);
        assert_eq!(
            outcome,
            LoadOutcome::ReadFailed,
            "予算超過は ReadFailed のまま（apply はスキップされデータ損失安全を維持）"
        );
        assert_eq!(calls.get(), 4, "初回 + 最大 3 リトライ = 4 回で打ち切り");
        assert_eq!(sleeps.get(), 3, "リトライ回数ぶん backoff");
    }
}
