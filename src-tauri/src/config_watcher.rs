//! `notify` クレートによる `config.toml` の変更監視（100ms debounce）。
//!
//! 差分検出後、`apply_config_change()` がホットキー・トレイ・インデックス・テーマ・
//! ウィンドウ幅・言語を実行中のアプリへ反映する。適用順序・読込失敗時のデータ保全など
//! 多サブシステムに跨る不変条件は `src-tauri/CLAUDE.md` を正とする。

use std::sync::Mutex;
use std::time::Duration;

use notify::{Event, EventKind, RecursiveMode, Watcher};
use snotra_core::config::{Config, LoadOutcome};
use snotra_core::engine::IndexInputs;
use tauri::window::Color;
use tauri::{AppHandle, Emitter, Manager};

use crate::indexing;
use crate::platform::{PlatformBridge, PlatformCommand};
use crate::state::AppState;

/// Parse a CSS hex color string (e.g. "#282828") into a Tauri `Color`.
/// `pub(crate)`: egui_shell::create（窓生成の background_color）が config テーマ値の
/// hex→Color 変換に再利用する（§11・#532 SU4 Task 2・二重実装回避）。
pub(crate) fn parse_hex_color(hex: &str) -> Option<Color> {
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color(r, g, b, 255))
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

    // Detect changes（egui は config-applied wake + 毎フレーム live-read で値を拾うため、
    // 検出が要るのは「副作用を伴う変更」だけ: index 再構築・hotkey 再登録・トレイ・言語）。
    // 旧 WebView2 フロント向けの値運搬 emit 群（language-changed / visual-config-changed 等
    // 7 本）は #532 SU7 PR3 のフロント撤去と同時に削除した。
    let index_changed =
        IndexInputs::from_config(&old_config) != IndexInputs::from_config(&new_config);
    let language_changed = new_config.general.language != old_config.general.language;
    let new_language = new_config.general.language;

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

    // Trigger reindex if needed. ビルド進行中でも常に kick する（!indexing ゲート撤去、#347/#348-A）。
    // start_index_build が mark_index_stale で stale を立て、in-flight ビルドの complete re-diff /
    // finish 後の再チェックが取りこぼしを拾う。CAS が二重起動を防ぐ。
    if index_changed {
        indexing::start_index_build(app);
    }

    // 幅変更の反映は egui view が config-applied wake 後の live-read で自ら set_size する
    //（SU6: view 単独 size writer——notify スレッドとの 2 次元 read-modify-write race 回避）。

    // SU6 spec 決定 1: egui 窓への単一 wake（値は運ばない・受信側は次フレームの live-read が拾う）。
    // update_config（上）より後に置く——先に起こすと旧 config を描いてから二度目の wake が要る。
    let _ = app.emit("config-applied", ());
}

/// `config.toml` の読込結果を実行中エンジンへ適用してよいかの判定。
/// `ReadFailed`（一時的・環境的な read 失敗: 権限/ロック/共有違反等）では fallback-default を
/// 適用しない。適用すると `result_limit` 等が default に落ち、live-read 化した履歴剪定が
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
