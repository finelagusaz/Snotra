//! 非表示アイドル時にプロセスツリーの working set を回収する。
//!
//! hide 経路（`egui_shell::hide_egui_main` 合流点）で Win32 `EmptyWorkingSet` を
//! プロセスツリー（自プロセス + 子孫。設定サイドカー存命中はそれも含む）へ能動適用し、
//! hide 直後の物理 RSS を即時に削減する（`src-tauri/CLAUDE.md`
//! 「working set の能動回収」節。旧 WebView2 suspend 層は #532 SU7 で消滅）。
//!
//! - show 時は OS がページを透過的に re-fault するため、逆操作（untrim）は不要かつ存在しない。
//! - best-effort: Toolhelp / `OpenProcess` / `EmptyWorkingSet` の全失敗は黙ってスキップする
//!   （失敗しても「メモリが減らない」だけで機能影響はない）。release は `panic = "abort"`
//!   のため、ここでは panic させない（`unwrap`/`expect`/添字を使わない）。

/// `(pid, parent_pid)` のリストから `root` を含む子孫 PID を BFS で収集する純関数。
///
/// `visited` 集合で循環参照・自己参照・PID 再利用（実在しない `ppid`）に対する停止性を
/// 保証する。Win32 非依存ゆえユニットテスト可能（プロセスツリー走査の唯一のロジック）。
pub(crate) fn collect_descendant_pids(pairs: &[(u32, u32)], root: u32) -> Vec<u32> {
    use std::collections::{HashSet, VecDeque};

    let mut result = Vec::new();
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    visited.insert(root);
    queue.push_back(root);

    while let Some(cur) = queue.pop_front() {
        result.push(cur);
        for &(pid, ppid) in pairs {
            // cur の子であり、かつ未訪問のものだけを辿る（`insert` が true = 新規）。
            if ppid == cur && visited.insert(pid) {
                queue.push_back(pid);
            }
        }
    }

    result
}

/// `root_pid` を根とするプロセスツリー全体の working set をトリミングする（Windows）。
///
/// 全 Win32 呼び出しは best-effort。失敗時は途中で諦めてメモリ削減を見送るだけで、
/// ウィンドウの hide/show・状態遷移・IPC 契約には一切影響しない。
#[cfg(windows)]
pub(crate) fn trim_idle_working_set(root_pid: u32) {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::ProcessStatus::EmptyWorkingSet;
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_SET_QUOTA,
    };

    /// HANDLE を確実に閉じる RAII ガード（`icon.rs` の `BitmapCleanup` と同じ作法）。
    /// `?` / `continue` / early-return を含む全分岐での close 漏れを防ぐ。
    struct HandleGuard(HANDLE);
    impl Drop for HandleGuard {
        fn drop(&mut self) {
            if !self.0.is_invalid() {
                unsafe {
                    let _ = CloseHandle(self.0);
                }
            }
        }
    }

    unsafe {
        // 1. プロセススナップショットから (pid, ppid) を収集。
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return;
        };
        // 名前付き束縛でスコープ末尾まで生かす（`let _ =` だと即 drop してしまう）。
        let _snapshot_guard = HandleGuard(snapshot);

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        let mut pairs: Vec<(u32, u32)> = Vec::new();
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                pairs.push((entry.th32ProcessID, entry.th32ParentProcessID));
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }

        // 2. 自プロセス + 子孫の PID を収集。
        let pids = collect_descendant_pids(&pairs, root_pid);

        // 3. 各プロセスの working set をトリミング（best-effort）。
        for pid in pids {
            let Ok(handle) = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_SET_QUOTA, false, pid)
            else {
                continue;
            };
            // 名前付き束縛でループ反復末尾まで生かす（EmptyWorkingSet 完了まで close しない）。
            let _handle_guard = HandleGuard(handle);
            let _ = EmptyWorkingSet(handle);
        }
    }
}

/// 非 Windows: no-op（working set 制御 API が存在しない）。
#[cfg(not(windows))]
pub(crate) fn trim_idle_working_set(_root_pid: u32) {}

#[cfg(test)]
mod tests {
    use super::collect_descendant_pids;

    fn sorted(mut v: Vec<u32>) -> Vec<u32> {
        v.sort_unstable();
        v
    }

    #[test]
    fn linear_chain_collects_all_and_ignores_unrelated() {
        // 1 -> 2 -> 3、(99,100) は無関係
        let pairs = [(2, 1), (3, 2), (99, 100)];
        assert_eq!(sorted(collect_descendant_pids(&pairs, 1)), vec![1, 2, 3]);
    }

    #[test]
    fn webview_tree_shape() {
        // snotra(1) -> browser(2) -> {renderer(3), gpu(4), utility(5)}, crashpad(6) も browser の子
        let pairs = [(2, 1), (3, 2), (4, 2), (5, 2), (6, 2)];
        assert_eq!(
            sorted(collect_descendant_pids(&pairs, 1)),
            vec![1, 2, 3, 4, 5, 6]
        );
    }

    #[test]
    fn stops_on_cycle_and_self_reference() {
        // 1 -> 2 -> 1（循環）と 3 -> 3（自己参照）
        let pairs = [(2, 1), (1, 2), (3, 3)];
        assert_eq!(sorted(collect_descendant_pids(&pairs, 1)), vec![1, 2]);
    }

    #[test]
    fn root_only_when_no_children() {
        let pairs = [(10, 20), (11, 20)];
        assert_eq!(collect_descendant_pids(&pairs, 1), vec![1]);
    }

    #[test]
    fn ignores_separate_tree() {
        // root 1 -> 2、別系統 100 -> 101 -> 102 は巻き込まない
        let pairs = [(2, 1), (101, 100), (102, 101)];
        assert_eq!(sorted(collect_descendant_pids(&pairs, 1)), vec![1, 2]);
    }

    #[test]
    fn nonexistent_parent_pid_not_traversed() {
        // ppid 7 は pid として存在しない（PID 再利用相当）。root からのみ辿る
        let pairs = [(2, 1), (3, 7)];
        assert_eq!(sorted(collect_descendant_pids(&pairs, 1)), vec![1, 2]);
    }
}
