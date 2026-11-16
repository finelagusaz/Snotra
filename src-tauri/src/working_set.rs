//! 非表示アイドル時に working set を回収する。
//!
//! hide 経路（`egui_shell::hide_egui_main` 合流点）で Win32 `EmptyWorkingSet` を、自プロセスと
//! 設定サイドカー（存命中のみ）へ能動適用し、hide 直後の物理 RSS を即時に削減する。
//! 削るのは working set であって commit ではない。
//!
//! - **子孫は辿らない**（#1444）。Snotra から起動したアプリと、設定サイドカーが開いたブラウザ等も
//!   子孫に入る。辿ると、それらが起動の最中と、以後の hide のたびに working set を失う。
//! - show 時は OS がページを透過的に re-fault するため、逆操作（untrim）は不要かつ存在しない。
//! - イベントループから直に呼んでよい。`EmptyWorkingSet` はスレッドを問わず、この関数は設定サイドカーの
//!   ロックを待たない（取れなければ見送る）。
//! - best-effort: `EmptyWorkingSet` の失敗は黙って見送る（失敗しても「メモリが減らない」だけで
//!   機能影響はない）。release は `panic = "abort"` のため、ここでは panic させない
//!   （`unwrap`/`expect`/添字を使わない）。

/// 自プロセスと設定サイドカーの working set をトリミングする（Windows）。
#[cfg(windows)]
pub(crate) fn trim_idle_working_set(app: &tauri::AppHandle) {
    use tauri::Manager;
    use windows::Win32::System::ProcessStatus::EmptyWorkingSet;
    use windows::Win32::System::Threading::GetCurrentProcess;

    // 擬似ハンドルは閉じなくてよい。
    unsafe {
        let _ = EmptyWorkingSet(GetCurrentProcess());
    }

    // **`lock` で待たない。** ここはイベントループのスレッドであり、`launch_settings_process` は
    // ロックを持ったまま子を spawn する。取れなければ（`PoisonError` も含めて）今回は見送り、
    // サイドカーは次の hide まで削られない。
    let Some(proc_state) = app.try_state::<crate::commands::SettingsProcessState>() else {
        return;
    };
    let Ok(guard) = proc_state.try_lock() else {
        return;
    };
    if let Some(child) = guard.as_ref() {
        empty_child_working_set(child);
    }
}

/// `child` の working set をトリミングする。
///
/// **`&Child` で受けるのは、ハンドルを `Child` が生きている間だけ使わせるためである**（`Child` は
/// 別スレッドで drop されうる）。`Child` のハンドルは `CreateProcess` が返したものなので、
/// `EmptyWorkingSet` に要る権限を持つ。
#[cfg(windows)]
fn empty_child_working_set(child: &std::process::Child) {
    use std::os::windows::io::AsRawHandle;

    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::ProcessStatus::EmptyWorkingSet;

    unsafe {
        let _ = EmptyWorkingSet(HANDLE(child.as_raw_handle()));
    }
}

/// 非 Windows: no-op（working set 制御 API が存在しない）。
#[cfg(not(windows))]
pub(crate) fn trim_idle_working_set(_app: &tauri::AppHandle) {}
