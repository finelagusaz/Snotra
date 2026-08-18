//! Tauri の managed state（`AppState`）定義。
//!
//! `Mutex<Engine>`（検索・履歴・設定を統合した snotra-core facade）と 3 つの `AtomicBool`
//! （`indexing` / `index_build_started` / `main_visible`）、および index build 完了ごとに
//! 単調増加する `index_generation`（`AtomicU64`・#633 の世代カウンタ）を保持する。
//! `main_visible` は Win32 `is_visible()` の ~35ms レイテンシを回避するキャッシュ。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use snotra_core::config::Config;
use snotra_core::engine::Engine;

pub struct AppState {
    pub engine: Mutex<Engine>,
    /// 設定の共有ハンドル（`engine` が持つのと**同じ `Arc`**・#1032）。
    ///
    /// **UI の毎フレームの live-read はこちらを読む**——`engine` の `Mutex` を経ると、
    /// 検索 worker が `engine.search` を走らせている間（実運用点で 40〜95 ms）フレームが
    /// そこで止まる。契約と、写しではないことの理由は `Engine::config_handle` の doc。
    ///
    /// **書き手はここには居ない。** 設定を変えるのは `engine.lock().update_config(..)` の
    /// 1 本だけで、そちらは `&mut Engine` を要求する（`Engine` の `config` フィールドの doc）。
    pub config: Arc<RwLock<Config>>,
    pub indexing: AtomicBool,
    pub index_build_started: AtomicBool,
    /// Tracks main window visibility to avoid costly Win32 `is_visible()` IPC on hotkey toggle.
    ///
    /// **書き手はイベントループ上の 2 経路だけである**——`window_coordinator` の
    /// `show_egui_main`（show の**後**に true）と `hide_egui_main`（`results.hide()` の**前**に
    /// false）。読み手のうち correctness に効くのは `layout::present_results` の連言①である。
    ///
    /// **ただしこれは型ではなく規範で保たれている。** このフィールドは `pub` で、新しい
    /// `store()` を crate 内のどこにでも書ける。加えて `Manager` から main のハンドルを引いた
    /// `.hide()` はコンパイルが通り**実際に効く**ため、このフラグを更新せずに窓だけ消せる
    /// （帰結と受容の理由は `src-tauri/CLAUDE.md`「可視性を変える操作はイベントループ
    /// スレッドに閉じてある」に続く bullet 群が正本）。
    pub main_visible: AtomicBool,
    /// index build 完了ごとに単調増加する世代（#633・SU6 spec 決定 3）。egui view が
    /// last-seen と比較して再検索をトリガするアキュムレータ。panic/spawn 失敗経路の finish でも
    /// bump されるが、無変化 index への再検索は同一結果になるだけで無害（意図的に単純化）。
    pub index_generation: AtomicU64,
}

impl AppState {
    /// config を読む（`engine` の `Mutex` を経ない）。**この crate で read guard を取る唯一の地点である。**
    ///
    /// **`read` の中で lock も I/O も取らないこと。** read guard を保持したまま `engine.lock()` を
    /// 要求すれば錠を分けた意味が消え、待ちが両方に乗る。ファイル I/O
    /// （`Path::is_dir` は死んだ UNC で最大 21 秒塞がる・#524）を挟めば、その間 `update_config` の
    /// 書き込みが進めず、設定の適用がそこで止まる。**`read` へ渡すのは純 CPU だけにする。**
    ///
    /// **クロージャ形が保証するのは guard を外へ持ち出せないことだけである**——中で I/O を
    /// 書く形は構造では止まらない（**受容する残余**）。
    ///
    /// **`&AppHandle` しか持たない呼び出し元は [`crate::egui_shell::read_config`] を使う**
    /// （あちらは `AppState` 不在の面倒を見てからここへ委譲する）。**口は 2 つだが、guard を
    /// 取る地点はここ 1 つである。**
    ///
    /// 規範の全文と害は `src-tauri/CLAUDE.md`「モジュール構成」の config live-read 条項が正本。
    pub fn read_config<T>(&self, read: impl FnOnce(&Config) -> T) -> T {
        read(&self.config.read().unwrap())
    }

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
        let engine = Engine::new(Vec::new(), HistoryStore::load(), Config::default());
        AppState {
            config: engine.config_handle(),
            engine: Mutex::new(engine),
            indexing: AtomicBool::new(false),
            index_build_started: AtomicBool::new(false),
            main_visible: AtomicBool::new(false),
            index_generation: AtomicU64::new(0),
        }
    }

    /// **UI の live-read は engine lock の外で完了する**（#1032）。
    ///
    /// worker は `engine.search` の間ずっと engine lock を握る（実運用点で 40〜95 ms）。
    /// その間に UI が同じ lock を取りに行っていたのが #1032 の主因で、`read_window_width`
    /// 単独で 43,939 µs の待ちを実測した。**この検査はその待ちが構造的に起きえないことを
    /// 測る**——engine lock を保持したまま別スレッドが config を読み切れることが受け入れ条件
    /// である。
    ///
    /// **同一スレッドで 2 つの lock を順に取る形にはしない**——別々の `Mutex` / `RwLock` ゆえ
    /// 必ず成功し、何も測らない。競合を測るには、握っている者と読む者を分ける必要がある。
    #[test]
    fn ui_reads_config_while_the_engine_lock_is_held() {
        let state = std::sync::Arc::new(test_state());
        let held = state.engine.lock().unwrap(); // worker 役（検索中）

        let reader = std::sync::Arc::clone(&state);
        let handle = std::thread::spawn(move || {
            // UI 役。engine lock を一切要求しない。**製品が使う口そのものを測る**——
            // `config` フィールドを直に読むと、`read_config` が engine lock を取り始めても
            // この検査は緑のままになる（#1123）。
            reader.read_config(|c| c.appearance.window_width)
        });

        let width = handle
            .join()
            .expect("UI 側の読みは engine lock の保持中に完了しなければならない");
        assert_eq!(width, Config::default().appearance.window_width);
        drop(held);
    }

    /// `AppState.config` と `Engine` は**同じ Arc を共有する**（写しではない・#1032）。
    ///
    /// 別々に持って両方へ書く形にすると、同じ事実を 2 か所へ書く誤りになる——書き手が片方を
    /// 忘れた瞬間に、UI と検索が違う config で動く。
    #[test]
    fn app_state_config_is_the_same_arc_the_engine_holds() {
        let state = test_state();
        let mut changed = Config::default();
        changed.appearance.window_width = 999;
        state.engine.lock().unwrap().update_config(changed);

        assert_eq!(
            state.read_config(|c| c.appearance.window_width),
            999,
            "engine への update_config が AppState 側の読みへ届かなければ、両者は別物である"
        );
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
