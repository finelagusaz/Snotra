//! Tauri の managed state（`AppState`）定義。
//!
//! `Mutex<Engine>`（検索・履歴・設定を統合した snotra-core facade）と `AtomicBool`
//! （`indexing` / `index_build_started`）、および index build 完了ごとに
//! 単調増加する `index_generation`（`AtomicU64`・#633 の世代カウンタ）を保持する。

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use snotra_core::config::Config;
use snotra_core::engine::Engine;

pub struct AppState {
    pub engine: Mutex<Engine>,
    /// 設定の共有ハンドル（`engine` が持つのと**同じ `Arc`**・#1032）。
    ///
    /// **UI の毎フレームの live-read はこちらを読む**——`engine` の `Mutex` を経ると、検索 worker が `engine.search` を走らせている間フレームがそこで止まる。契約と、写しではないことの理由と、実運用点での保持時間は [`Engine::config_handle`] の doc。
    ///
    /// **書き手はここには居ない。** 設定を変えるのは `engine.lock().update_config(..)` の
    /// 1 本だけで、そちらは `&mut Engine` を要求する（`Engine` の `config` フィールドの doc）。
    ///
    /// **このフィールドは private である**（#1129）——読む口は [`AppState::read_config`]、
    /// 建てる口は [`AppState::new`] で、どちらもこのモジュールの中にある。適用範囲は前者の doc。
    config: Arc<RwLock<Config>>,
    /// UI 表示用の「索引ビルド中」。first-run ではビルドスレッドが居なくても
    /// [`AppState::new`] が true で建てる。
    ///
    /// **書くのは [`AppState::new`] / [`AppState::try_begin_index_build`] /
    /// [`AppState::finish_index_build`] だけである**（外からの force-reset は走行中ビルドのガードを
    /// 踏み倒すので private にしてある）。
    /// 読む口は [`AppState::is_indexing`]。
    ///
    /// `index_build_started` とは別物である（あちらは二重起動を防ぐ CAS 専用）。**config 変更 →
    /// 索引再構築のコヒーレンシ判断をこの 2 つから導かない**——判断は engine の `index_stale`
    /// ledger に閉じている（#347/#348-A）。
    indexing: AtomicBool,
    /// 二重起動を防ぐ CAS 専用の旗（[`AppState::try_begin_index_build`]）。書き手と private の
    /// 理由は `indexing` と同じ。読む口は [`AppState::is_index_build_started`]。
    index_build_started: AtomicBool,
    /// index build 完了ごとに単調増加する世代（#633・SU6 spec 決定 3）。egui view が
    /// last-seen と比較して再検索をトリガするアキュムレータ。panic/spawn 失敗経路の finish でも
    /// bump されるが、無変化 index への再検索は同一結果になるだけで無害（意図的に単純化）。
    pub index_generation: AtomicU64,
    /// 構築中に走査したフォルダの数（status 行の進捗表示・#1041）。
    ///
    /// **書くのは [`AppState::begin_scan_progress`] / [`AppState::finish_index_build`] と、
    /// [`AppState::scan_dirs_counter`] を受け取った `scan_all` の加算だけである**（`indexing` と
    /// 同じく、書き手を限定するために private にしてある）。読む口は [`AppState::scan_progress`]。
    scan_dirs: AtomicUsize,
    /// 進捗表示の分母（前回の完走で走査したフォルダの数）。**0 は「無し」である**——0 フォルダの
    /// 完走は分母として役に立たないので、無いのと同じに扱う。書き手は `scan_dirs` と同じ。
    scan_expected: AtomicUsize,
}

impl AppState {
    /// `engine` を預かって managed state を建てる。
    ///
    /// **このモジュールの外から `AppState` を建てる道はこれだけである**（#1129）——`config` が
    /// private ゆえ、外で構造体リテラルを書くと **E0451** で落ちる。`state.rs` の中では今も書ける。
    ///
    /// **`config` は engine が持つのと同じ `Arc` である——写しではない**（#1032・契約は
    /// [`Engine::config_handle`] の doc）。別々に持って両方へ書く形にすると、書き手が片方を忘れた
    /// 瞬間に UI と検索が違う設定で動く。**この一致を守る地点は 1 つになった**（#1129）——かつては
    /// 構築点 3 か所がそれぞれ `engine.config_handle()` と書く規律だった。**規律が消えたのではない**
    /// ——この関数の本体がそれであり、`app_state_config_is_the_same_arc_the_engine_holds` がそこを測る。
    pub(crate) fn new(engine: Engine, initial_indexing: bool) -> Self {
        // `Mutex::new(engine)` が engine をムーブするので、ハンドルを先に取る。
        let config = engine.config_handle();
        Self {
            config,
            engine: Mutex::new(engine),
            indexing: AtomicBool::new(initial_indexing),
            index_build_started: AtomicBool::new(false),
            index_generation: AtomicU64::new(0),
            scan_dirs: AtomicUsize::new(0),
            scan_expected: AtomicUsize::new(0),
        }
    }

    /// drain の 1 反復の先頭で進捗を置く。**`scan_dirs = 0` → `scan_expected` の順に書く**——
    /// 逆順にすると、前の反復のフォルダ数と新しい分母の組が見える窓が広がる（残る窓は
    /// [`AppState::scan_progress`] の doc）。
    pub(crate) fn begin_scan_progress(&self, expected: Option<usize>) {
        self.scan_dirs.store(0, Ordering::SeqCst);
        self.scan_expected
            .store(expected.unwrap_or(0), Ordering::SeqCst);
    }

    /// `rebuild_and_save` へ渡すカウンタ。**加算するのは `scan_all` だけである**（`scan_dirs` の doc）。
    pub(crate) fn scan_dirs_counter(&self) -> &AtomicUsize {
        &self.scan_dirs
    }

    /// 進捗を読む: （走査したフォルダの数, 分母）。分母が 0 なら `None`。
    ///
    /// **2 つの atomic を別々に読む**ので、1 回の要求で 2 反復する構築の境目では「前の反復の
    /// フォルダ数と新しい分母」の組が最大 1 フレーム見えうる（受容する。表示の誤りにとどまり、
    /// 次のフレームで直る）。
    pub(crate) fn scan_progress(&self) -> (usize, Option<usize>) {
        let dirs = self.scan_dirs.load(Ordering::SeqCst);
        let expected = self.scan_expected.load(Ordering::SeqCst);
        (dirs, (expected > 0).then_some(expected))
    }

    /// config を読む（`engine` の `Mutex` を経ない）。**この crate で read guard を取る唯一の地点である。**
    ///
    /// **閉じたのは外から届く綴りである**（#1129）——`config` は private ゆえ、このモジュールの外に
    /// `state.config.read()` と書くと **E0616** で落ちる（回帰の形を注入して 2026-08-18 に実測）。
    /// **`state.rs` の中（`#[cfg(test)] mod tests` を含む）では今も綴れる**——現にフィールドを綴って
    /// いるのは、読む側が下の 1 行、書く側が [`AppState::new`] の初期化である。
    ///
    /// **迂回は塞がっていない**——`engine.lock().unwrap().config_handle().read()` は今も通る（同じ場で
    /// 測った。#1123 が記録した残余であり、フィールドの可視性はそこへ届かない）。
    ///
    /// **`read` へ渡すのは純 CPU だけにする——錠も I/O も取らない。** 読みの間じゅう
    /// `update_config` の書き込みは進めないので、**中で不定時間ブロックしうるものはすべて禁じる**。
    /// 禁止は列挙ではない——`src-tauri` には engine 以外にも `IconCacheState` /
    /// `SettingsProcessState` / updater の状態など多数の `Mutex` があり、どれを取っても同じ害が出る。
    ///
    /// **とくに `engine.lock()` は、遅くなるのではなく両者が永久に待つ。** 製品には
    /// `engine Mutex → config の write` という順序が実在する: `config_watcher` は
    /// `state.engine.lock().unwrap().update_config(..)` と書き、`MutexGuard` が文の間じゅう生きた
    /// ままその内側で `config.write()` を取る。read guard を保持して `engine.lock()` を要求すれば
    /// **その逆順**になり、両者が互いを待つ。
    ///
    /// **ファイル I/O も同じ理由で禁じる**（実例と実測値は `commands/launch.rs` の
    /// `resolve_opener` の doc・#524）。
    /// **確保を伴う読みの実例として、`config_watcher` は `Config` 全体を clone する**——錠も I/O も
    /// 取らないので規則に反しない（移設前も engine 錠の内側で同じ複製をしていた）。
    ///
    /// **クロージャ形が保証するのは guard を外へ持ち出せないことだけである**——中で錠や I/O を
    /// 書く形は構造では止まらない（**受容する残余**）。
    ///
    /// **`&AppHandle` しか持たない呼び出し元は [`crate::egui_shell::read_config`] を使う**
    /// （あちらは `AppState` 不在の面倒を見てからここへ委譲する）。
    ///
    /// **config の読みをここ（と `egui_shell::read_config`）へ通す規則に条件は付かない**（#1123）——どのスレッドで走る読みかも、何のための読みかも問わない。
    /// 害は頻度ではなく待ちである（機序は [`crate::egui_shell::read_config`] の doc）。
    ///
    /// **適用範囲は読みだけである**——書き込みは `update_config` の 1 本で、engine lock の内側に残す
    /// （`snotra-core/CLAUDE.md`「engine.rs のロック最小化パターン」）。
    pub fn read_config<T>(&self, read: impl FnOnce(&Config) -> T) -> T {
        read(&self.config.read().unwrap())
    }

    /// UI 表示用の「索引ビルド中」を読む（`indexing` フィールドの doc）。
    pub fn is_indexing(&self) -> bool {
        self.indexing.load(Ordering::SeqCst)
    }

    /// ビルドの開始権が取られているかを読む（`index_build_started` フィールドの doc）。
    pub fn is_index_build_started(&self) -> bool {
        self.index_build_started.load(Ordering::SeqCst)
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
    ///
    /// **進捗（`scan_dirs` / `scan_expected`）もここで 0 に戻す**——次のビルドは既に 0 の状態から
    /// 始まるので、前回の数は開始時に見えない。
    pub fn finish_index_build(&self) {
        self.indexing.store(false, Ordering::SeqCst);
        self.index_build_started.store(false, Ordering::SeqCst);
        self.scan_dirs.store(0, Ordering::SeqCst);
        self.scan_expected.store(0, Ordering::SeqCst);
        self.index_generation.fetch_add(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snotra_core::config::Config;
    use snotra_core::history::HistoryStore;

    fn test_state() -> AppState {
        // 実 `history.bin` を読まない（#963・#1306）。存在しないパスを渡すと `load_in` は空で返す。
        let history = HistoryStore::load_in(
            &std::env::temp_dir().join(format!("snotra-tauri-test-{}-unused", std::process::id())),
        );
        let engine = Engine::new(Vec::new(), history, Config::default());
        AppState::new(engine, false)
    }

    /// **UI の live-read は engine lock の外で完了する**（#1032）。
    ///
    /// worker は `engine.search` の間ずっと engine lock を握る（実運用点での保持時間は `Engine::config_handle` の doc）。その間に UI が同じ lock を取りに行っていたのが #1032 の主因である（`read_window_width` の読み max の実測値は `PERFORMANCE.md`「設定の読みを engine lock の外へ出す」）。**この検査はその待ちが構造的に起きえないことを測る**——engine lock を保持したまま別スレッドが config を読み切れることが受け入れ条件である。
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
    ///
    /// **測る対象は [`AppState::new`] である**（#1129）——構築がそこへ集約されたので、この性質を
    /// 破りうる地点は 1 つしかない。
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

    /// #1041: 終了で進捗を戻す。戻さないと、次のビルドの開始時（`try_begin_index_build` から
    /// drain の反復の先頭まで）に前回のフォルダ数と分母が見える。
    #[test]
    fn finish_index_build_resets_the_scan_progress() {
        let state = test_state();
        assert!(state.try_begin_index_build());
        state.begin_scan_progress(Some(98));
        state.scan_dirs_counter().fetch_add(41, Ordering::Relaxed);
        assert_eq!(state.scan_progress(), (41, Some(98)));

        state.finish_index_build();
        assert_eq!(state.scan_progress(), (0, None));
    }

    /// 分母 0 は「無し」として読む。反復の先頭は前の反復のフォルダ数を 0 に戻す。
    #[test]
    fn scan_progress_reads_a_zero_denominator_as_none() {
        let state = test_state();
        state.begin_scan_progress(Some(0));
        assert_eq!(state.scan_progress(), (0, None));
        state.scan_dirs_counter().fetch_add(5, Ordering::Relaxed);
        state.begin_scan_progress(None);
        assert_eq!(state.scan_progress(), (0, None));
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
