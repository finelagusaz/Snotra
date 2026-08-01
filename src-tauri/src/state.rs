//! Tauri の managed state（`AppState`）定義。
//!
//! `Mutex<Engine>`（検索・履歴・設定を統合した snotra-core facade）と 3 つの `AtomicBool`
//! （`indexing` / `index_build_started` / `main_visible`）、および index build 完了ごとに
//! 単調増加する `index_generation`（`AtomicU64`・#633 の世代カウンタ）を保持する。
//! `main_visible` は Win32 `is_visible()` の ~35ms レイテンシを回避するキャッシュ。

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use snotra_core::engine::Engine;

pub struct AppState {
    pub engine: Mutex<Engine>,
    pub indexing: AtomicBool,
    pub index_build_started: AtomicBool,
    /// Tracks main window visibility to avoid costly Win32 `is_visible()` IPC on hotkey toggle.
    pub main_visible: AtomicBool,
    /// index build 完了ごとに単調増加する世代（#633・SU6 spec 決定 3）。egui view が
    /// last-seen と比較して再検索をトリガするアキュムレータ。panic/spawn 失敗経路の finish でも
    /// bump されるが、無変化 index への再検索は同一結果になるだけで無害（意図的に単純化）。
    pub index_generation: AtomicU64,
}

impl AppState {
    /// インデックスビルドの開始権を CAS で取得する。
    /// 成功時は `index_build_started` と `indexing` を両方 true にして `true` を返す。
    /// 既にビルドが始まっている場合は何も変更せず `false` を返す。
    pub fn try_begin_index_build(&self) -> bool {
        if self
            .index_build_started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return false;
        }
        self.indexing.store(true, Ordering::SeqCst);
        true
    }

    /// インデックスビルドの終了。`indexing` と `index_build_started` を両方 false に戻す。
    /// 正常完了経路と `thread::spawn` 失敗経路の両方から呼ぶ。
    pub fn finish_index_build(&self) {
        self.indexing.store(false, Ordering::SeqCst);
        self.index_build_started.store(false, Ordering::SeqCst);
        self.index_generation.fetch_add(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snotra_core::config::Config;
    use snotra_core::history::HistoryStore;

    fn test_state() -> AppState {
        AppState {
            engine: Mutex::new(Engine::new(
                Vec::new(),
                HistoryStore::load(),
                Config::default(),
            )),
            indexing: AtomicBool::new(false),
            index_build_started: AtomicBool::new(false),
            main_visible: AtomicBool::new(false),
            index_generation: AtomicU64::new(0),
        }
    }

    #[test]
    fn try_begin_index_build_succeeds_and_sets_flags_when_idle() {
        let state = test_state();
        let started = state.try_begin_index_build();
        assert!(started, "try_begin_index_build must succeed when idle");
        assert!(
            state.indexing.load(Ordering::SeqCst),
            "indexing flag must be set after a successful begin"
        );
        assert!(
            state.index_build_started.load(Ordering::SeqCst),
            "index_build_started flag must be set after a successful begin"
        );
    }

    #[test]
    fn finish_index_build_clears_both_flags_and_allows_restart() {
        let state = test_state();
        assert!(
            state.try_begin_index_build(),
            "setup: first begin must succeed"
        );
        state.finish_index_build();
        assert!(
            !state.indexing.load(Ordering::SeqCst),
            "indexing flag must be cleared after finish_index_build"
        );
        assert!(
            !state.index_build_started.load(Ordering::SeqCst),
            "index_build_started flag must be cleared after finish_index_build"
        );
        assert!(
            state.try_begin_index_build(),
            "try_begin_index_build must succeed again after finish_index_build"
        );
    }

    #[test]
    fn try_begin_index_build_fails_when_already_started() {
        // カバレッジ: CAS 実装が「二重起動しない」をどう満たすかを固定する。
        let state = test_state();
        assert!(
            state.try_begin_index_build(),
            "setup: first begin must succeed"
        );
        let second = state.try_begin_index_build();
        assert!(
            !second,
            "try_begin_index_build must fail (no double-start) while a build is already in progress"
        );
    }

    #[test]
    fn finish_index_build_bumps_index_generation() {
        // #633: 完了ごとに単調増加。egui view の世代比較トリガの根拠（SU6 spec 決定 3）。
        let state = test_state();
        let g0 = state.index_generation.load(Ordering::SeqCst);
        assert!(state.try_begin_index_build());
        state.finish_index_build();
        assert_eq!(state.index_generation.load(Ordering::SeqCst), g0 + 1);
    }

    #[test]
    fn try_begin_index_build_succeeds_in_first_run_state() {
        // カバレッジ: 初回起動状態 = indexing=true（UI 表示用）だが index_build_started=false
        // （ビルド未開始）。try_begin は index_build_started を CAS するので、この状態でも
        // 成功しなければならない。try_begin を indexing 側にすると first-run が壊れる。
        let state = test_state();
        state.indexing.store(true, Ordering::SeqCst);
        assert!(
            state.try_begin_index_build(),
            "try_begin must succeed in first-run state (indexing=true, index_build_started=false)"
        );
        assert!(state.index_build_started.load(Ordering::SeqCst));
    }
}
