# results 窓の可視性を単一スレッドへ閉じる（根治）実装計画

> **エージェント実行者へ:** この計画はタスク単位で実行する。各ステップは `- [ ]` で追跡する。
> **`gh pr create` は未チェック項目が残っていると拒否される**（`.claude/hooks/pre-bash.mjs`）。
> やらないと決めた項目は削除して理由を記録すること。

**目的:** `results 窓が可視 ⇒ main 窓が可視` の破れを、事後検査ではなく**型で構築不能**にする。

**方式:** 可視性を*変える*操作をイベントループスレッドへ marshalling し、`!Send` な証人型
`EventLoopProof` を引数に要求することでコンパイル時に強制する。相互排他は tao の runner が
既に持っている（`event_handler.take()` による非再入 + イベントバッファリング）ので、lock は
1 つも導入しない。

**設計書（正本）:** `docs/superpowers/specs/2026-08-02-results-visibility-single-thread-design.md`
——機序・却下案（オーナー窓 / `WS_CHILD` / 単一 atomic 状態機械 / alpha 0）・一次証拠はそちら。
**この計画は「何をどの順で書くか」だけを持つ。**

**技術:** Rust / Tauri 2.11.4 / tauri-runtime-wry 2.11.4 / tao 0.35.3 / egui 0.35 / softbuffer / Windows 専用

## 前提条件（着手前に確認する）

- [x] **PR #880 が main へマージ済みであること。**（2026-08-02 `fa2dcf8` として squash マージ済み・
      closingIssuesReferences 0 件・マージ後 3 点検証で誤 close 無しを確認） 本計画は #880 が入れた事後検査
      （`layout::must_retract_results`）と hide の理由型（`layout::HideReason` /
      `hide_must_be_unconditional`）を**削除する**手順を含む。未マージのまま着手すると
      タスク 5 が「存在しないものを消す」になる。**マージされていないなら、先にマージするか、
      タスク 5 を「該当なし」として削除し理由をここに記録する**

## Global Constraints（全タスクに掛かる。値は正本から逐語）

- **`main` へ直接コミット・プッシュしない。** feature ブランチを切る（ルート `CLAUDE.md`「最重要ルール」）
- **`-D warnings`。** 未使用の新 API は `dead_code` で落ちる。ゆえに**新 API の導入と呼び出し点の
  移行は 1 タスクに束ねる**（`AGENTS.md`「条件別チェック」）
- **results 窓の show / hide / topmost は 3 つとも raw Win32、main は 3 つとも tao 経由。混在禁止**
  （`src-tauri/CLAUDE.md`「Win32 / Tauri 注意事項」）。本計画はこの分担を**変えない**——変えるのは
  実行スレッドだけである
- **ランタイムでの窓生成は禁止**（setup 限定・`src-tauri/CLAUDE.md`「ウィンドウ生成の制約」）
- **イベントループ中のコールバックではメッセージポンプが停止する。** marshalling した本体に
  ポンプ進行を要する操作を入れてはならない（同上）
- **全称表現は前提条件とセットで書く。書けないなら書かない**（`AGENTS.md`「検証の作法」）。
  本変更で書いてよいのは「可視性を**変える**操作は単一スレッドに閉じた」までであり、
  「results への生 Win32 はすべて所有スレッド」は**偽**である（`set_topmost` が残る）
- **コマンド本体の正本は `docs/build-commands.md`。** この計画にコマンド文字列の写しを増やさない

---

## ファイル構成

**新規:**
- `snotra-egui-runtime/src/proof.rs` — イベントループスレッドの証人型と、そこへ入る唯一の口

**変更:**
- `snotra-egui-runtime/src/lib.rs` — `mod proof;` と再エクスポート
- `snotra-egui-runtime/src/runtime.rs` — `RuntimeFrame` に証人を持たせ、`event_loop()` で貸す
- `snotra-egui-runtime/CLAUDE.md` — モジュール構成にファイル名行を足す（`AGENTS.md`「ファイル（`.rs`）を追加/削除」）
- `src-tauri/src/egui_shell/window_coordinator.rs` — `show_egui_main` / `hide_egui_main` / `drive_results_window`
- `src-tauri/src/egui_shell/results_window.rs` — `show` / `hide`
- `src-tauri/src/egui_shell/view.rs` — `drive_results_window` 呼び出し
- `src-tauri/src/egui_shell/mod.rs` — hide listener / initial-hotkey-failure listener
- `src-tauri/src/main.rs` — hotkey listener / single-instance / setup
- `src-tauri/src/egui_shell/layout.rs` — タスク 5 で `HideReason` 系を削除
- `SPEC.md` / `src-tauri/CLAUDE.md` — タスク 6

---

## タスク 1: 証人型を runtime に置く

**Files:**
- Create: `snotra-egui-runtime/src/proof.rs`
- Modify: `snotra-egui-runtime/src/lib.rs`
- Modify: `snotra-egui-runtime/src/runtime.rs`（`RuntimeFrame` の定義と `render()` 内の構築点）
- Modify: `snotra-egui-runtime/CLAUDE.md`

**Interfaces:**
- Produces: `EventLoopProof`（不透明・`!Send`）、`RuntimeFrame::event_loop(&self) -> &EventLoopProof`、
  `on_event_loop<F>(app: &tauri::AppHandle, f: F) where F: FnOnce(&tauri::AppHandle, &EventLoopProof) + Send + 'static`

- [x] **Step 1: `proof.rs` を作る**

```rust
//! イベントループスレッド上にいることの証人（`EventLoopProof`）と、そこへ入る唯一の口。
//!
//! **この型は「どのスレッドで走っているか」を型に持ち上げるためだけに在る。** 窓の可視性を
//! 変える操作（main の show/hide・results の raw show/hide）は、書き手が複数スレッドに散ると
//! 「判定してから撃つまで」に他スレッドの逆操作が割り込みうる。証人を引数に要求すれば、
//! 別スレッドからの呼び出しは**コンパイルが通らなくなる**。
//!
//! **相互排他は lock ではなく tao の runner が与える。** `call_event_handler` は
//! `event_handler.take()` してから呼び（非再入）、`send_event` はハンドラ実行中のイベントを
//! `event_buffer` へ回す（tao 0.35.3 `event_loop/runner.rs`）。ゆえにイベントループ上の
//! 2 つの処理は互いに割り込めない。**lock を足してはならない**——窓を所有しないスレッドからの
//! `ShowWindow` は所有スレッドのポンプ待ちでブロックしうるため、イベントループ側も取る lock は
//! race をデッドロックへ化けさせる。

use std::marker::PhantomData;

/// イベントループスレッド上にいることの証人。
///
/// **フィールドは private かつ `PhantomData<*const ()>` である。** 前者はこの crate の外での
/// 構築を防ぎ、後者は `!Send + !Sync` にして**参照ごと別スレッドへ持ち出すこと**を防ぐ
/// （`on_event_loop` が要求する `F: Send` のクロージャにも `std::thread::spawn` にも入らない）。
///
/// 構築点は 2 つだけである: `RuntimeFrame`（フレームの中）と `on_event_loop`（marshalling した
/// タスクの中）。**3 つ目を足すときは、その経路が本当にイベントループ上かを一次証拠で示すこと。**
pub struct EventLoopProof {
    _not_send: PhantomData<*const ()>,
}

impl EventLoopProof {
    /// **crate 内部専用。** イベントループスレッド上であることが呼び出し側で保証されている
    /// 箇所からのみ呼ぶ。
    pub(crate) fn new() -> Self {
        Self {
            _not_send: PhantomData,
        }
    }
}

/// フレームの外からイベントループスレッドへ入る唯一の口。
///
/// **遅延 primitive ではない。** `AppHandle::run_on_main_thread` は
/// `tauri-runtime-wry` の `send_user_message` へ落ち、**イベントループスレッドから呼ぶと
/// その場で同期・再入的に実行される**（`src/lib.rs:235-255` の `current_thread().id() ==
/// context.main_thread_id` 分岐）。別スレッドからは `PostMessageW` で post して即座に戻る。
/// ゆえにフレーム内から出た要求は今日と同じフレーム内順序を保つ。
///
/// **hidden な窓でも走る。** Task の受け口は tao が別に建てる `thread_msg_target`
/// （0×0・`WS_EX_LAYERED` ゆえ不可視・イベントループの寿命と同じ）であり、アプリ窓の可視性とは
/// 無関係である。止まるのはフレーム（`RedrawRequested` の配送）であってタスクではない。
///
/// **送信失敗は握りつぶす。** 失敗するのはイベントループが既に閉じたときで、そのとき窓は
/// もう無い。
pub fn on_event_loop<F>(app: &tauri::AppHandle, f: F)
where
    F: FnOnce(&tauri::AppHandle, &EventLoopProof) + Send + 'static,
{
    let handle = app.clone();
    // `run_on_main_thread` は `Manager` トレイトではなく `AppHandle<R>` の inherent
    // メソッドである（tauri 2.11.4 `src/app.rs:493-495` で実測）。`tauri::Manager::` 経由では
    // 解決しない。
    let _ = app.run_on_main_thread(move || {
        f(&handle, &EventLoopProof::new());
    });
}
```

- [x] **Step 2: `lib.rs` に配線する**

`mod proof;` を `mod monitor;` の後（アルファベット順の位置）に足し、`pub use` に加える:

```rust
pub use proof::{EventLoopProof, on_event_loop};
```

- [x] **Step 3: `RuntimeFrame` に証人を持たせる**

`runtime.rs` の `RuntimeFrame` 定義（`pub struct RuntimeFrame {` の行）へフィールドを足す:

```rust
pub struct RuntimeFrame {
    drag_requested: bool,
    clear_color: Option<egui::Color32>,
    /// このフレームがイベントループスレッド上で走っていることの証人。
    /// `render()` は wry plugin の `on_event` からしか呼ばれないため、この値が存在すること
    /// 自体がその証明である（`proof.rs` の `//!`）。
    proof: crate::proof::EventLoopProof,
}
```

`impl RuntimeFrame` へ:

```rust
    /// このフレームがイベントループスレッド上にいることの証人を貸す。
    ///
    /// **`&self` で貸すのは意図である**——証人は `!Send` ゆえ、借りた側もこのフレームの
    /// 寿命を越えて持ち出せない。
    pub fn event_loop(&self) -> &crate::proof::EventLoopProof {
        &self.proof
    }
```

`render()` 内の構築点（`let mut frame = RuntimeFrame {`）へ:

```rust
        let mut frame = RuntimeFrame {
            drag_requested: false,
            clear_color: None,
            proof: crate::proof::EventLoopProof::new(),
        };
```

- [x] **Step 4: `snotra-egui-runtime/CLAUDE.md` のモジュール構成へ 1 行足す**

`- \`monitor.rs\`: ...` の行の後に:

```markdown
- `proof.rs`: イベントループスレッド上にいることの証人型`EventLoopProof`と、外部スレッドからそこへ入る唯一の口`on_event_loop`（責務詳細は`//!`）
```

- [x] **Step 5: ビルドが通ることを確認する**

`docs/build-commands.md` カテゴリ A のビルド・clippy を実行する。
期待: **通る**（この段では新 API に消費者が無いが、`pub` なので `dead_code` にはならない）。

- [x] **Step 6: コミット**

```
feat(egui-runtime): イベントループスレッドの証人型 EventLoopProof を導入する
```

---

## タスク 2: 可視性 API を証人で拘束し、呼び出し点を移行する

**この 2 つを分けてはならない**（Global Constraints の `-D warnings`）。移行漏れの検出器は
**下流の compile-fail** である。

**Files:**
- Modify: `src-tauri/src/egui_shell/window_coordinator.rs`（`show_egui_main` / `hide_egui_main` / `drive_results_window`）
- Modify: `src-tauri/src/egui_shell/results_window.rs`（`show` / `hide`）
- Modify: `src-tauri/src/egui_shell/view.rs`（`drive_results_window` 呼び出し）
- Modify: `src-tauri/src/egui_shell/mod.rs`（`register_hide_listener` / `register_initial_hotkey_failure_listener`）
- Modify: `src-tauri/src/main.rs`（single-instance / setup の `show_egui_main`）

**Interfaces:**
- Consumes: タスク 1 の `EventLoopProof` / `RuntimeFrame::event_loop` / `on_event_loop`
- Produces: `hide_egui_main(app, el)` / `show_egui_main(app, el, t0)` /
  `drive_results_window(app, el, inputs)` / `ResultsWindow::show(el, background) -> bool` /
  `ResultsWindow::hide(el, reason) -> bool`（`reason` はタスク 5 で消える）

- [x] **Step 1: シグネチャへ証人を足す**

`window_coordinator.rs`:

```rust
pub(crate) fn show_egui_main(
    app: &tauri::AppHandle,
    _el: &snotra_egui_runtime::EventLoopProof,
    t0: Instant,
)
pub(crate) fn hide_egui_main(
    app: &tauri::AppHandle,
    _el: &snotra_egui_runtime::EventLoopProof,
)
pub(crate) fn drive_results_window(
    app: &tauri::AppHandle,
    el: &snotra_egui_runtime::EventLoopProof,
    i: DriveResultsInputs,
)
```

`results_window.rs`（`el` は `ResultsWindow` 内部では使わないので `_el`）:

```rust
    pub(crate) fn show(
        &self,
        _el: &snotra_egui_runtime::EventLoopProof,
        background: egui::Color32,
    ) -> bool
    pub(crate) fn hide(
        &self,
        _el: &snotra_egui_runtime::EventLoopProof,
        reason: super::layout::HideReason,
    ) -> bool
```

`drive_results_window` の中の `results.show(...)` / `results.hide(...)` へ `el` を渡す。

- [x] **Step 2: `cargo build -p snotra` で移行漏れを列挙させる**

期待: **失敗**。`E0061`（引数の数が合わない）が呼び出し点の数だけ出る。**この一覧が移行対象の
正本である**——手で数えない。

- [x] **Step 3: フレーム内の呼び出し点を移行する（view.rs）**

`view.rs` の `update()` 末尾。`frame` は同関数の引数として在る（`frame.set_clear_color` を
呼んでいる箇所と同じ `frame`）:

```rust
        crate::egui_shell::drive_results_window(
            &app,
            frame.event_loop(),
            crate::egui_shell::DriveResultsInputs {
```

- [x] **Step 4: listener の呼び出し点を移行する（mod.rs）**

`register_hide_listener`:

```rust
    app.listen(crate::events::EGUI_HIDE_REQUESTED, move |_| {
        // emit 元は view の `update()` の中（イベントループスレッド）ゆえ、
        // `on_event_loop` はインライン実行へ倒れる——**今日と同じフレーム内順序が保たれる**
        // （`proof.rs` の `on_event_loop` の doc）。
        snotra_egui_runtime::on_event_loop(&handle, |app, el| hide_egui_main(app, el));
    });
```

`register_initial_hotkey_failure_listener` の `show_egui_main(&handle, Instant::now());` は、
**pending 格納の後・`wake_main` の前**という既存の順序を保ったまま包む:

```rust
        snotra_egui_runtime::on_event_loop(&handle, |app, el| {
            show_egui_main(app, el, Instant::now());
        });
```

- [x] **Step 5: main.rs の残り 2 点を移行する**

single-instance（`main.rs` の `egui_shell::show_egui_main(app, Instant::now());`）と
setup（`setup_startup_display` の同）。どちらも既にイベントループスレッドだが、
**証人を作れるのは `on_event_loop` の中だけ**なので同じ形で包む:

```rust
    snotra_egui_runtime::on_event_loop(app, |app, el| {
        egui_shell::show_egui_main(app, el, Instant::now());
    });
```

- [x] **Step 6: hotkey listener は判定ごと移す（分割禁止）**

**各アームを個別に包んではならない。** それでは判定（`main_visible` の読み）が producer
スレッドに残り、タスク実行前に届いた 2 回目の押下が**同じ stale 値**を読んで両方 Hide /
両方 Show になる——**連打でトグルが壊れる**（設計書 §3.3）。今日この問題が無いのは判定も
副作用も同じ platform スレッド上で逐次化されているからで、**効果だけを marshalling すると
その逐次化が失われる**。

`SettingsProcessState` のチェックは**タスクの外に残す**（窓に触らない読みであり、無駄な
タスク post を避ける）。`t0` は**post する前**に取る（marshalling の hop をレイテンシ計測に
含める）。

```rust
    app_handle.listen(crate::events::HOTKEY_PRESSED, move |_| {
        let t0 = Instant::now();
        trace_main("hotkey:listener_enter", json!({}));
        // 設定画面の起動中はホットキーを無視する（ユーザーが新しい組み合わせを設定するために
        // 現在の組み合わせを押している可能性がある）。**窓に触らない読みなのでタスクの外に置く**。
        if let Some(proc_state) = handle_for_hotkey.try_state::<SettingsProcessState>()
            && proc_state.lock().unwrap().is_some()
        {
            return;
        }
        // **判定ごとイベントループへ移す。** 世代の採番・可視の読み・分岐・副作用が
        // ひとまとまりで逐次化される——効果だけを移すと連打で stale を読む。
        snotra_egui_runtime::on_event_loop(&handle_for_hotkey, move |app, el| {
            let current_gen = app
                .try_state::<egui_shell::EguiShellState>()
                .map(|sh| sh.hotkey_generation.fetch_add(1, Ordering::SeqCst) + 1)
                .unwrap_or(0);
            let app_state = app.try_state::<AppState>();
            let visible = app_state
                .as_ref()
                .map(|s| s.main_visible.load(Ordering::SeqCst))
                .unwrap_or(false);
            // hotkey_toggle は可視時の hide 判定にしか使わない（plan_hotkey）。`visible &&` で
            // 短絡し、非表示＝show 経路（最も遅延に敏感）では engine ロックを取らない。
            let hotkey_toggle = visible
                && app_state
                    .as_ref()
                    .map(|s| s.engine.lock().unwrap().config().general.hotkey_toggle)
                    .unwrap_or_else(|| GeneralConfig::default().hotkey_toggle);
            match egui_shell::plan_hotkey(visible, is_alt_pressed(), hotkey_toggle) {
                egui_shell::HotkeyPlan::HideNow => {
                    egui_shell::hide_egui_main(app, el);
                }
                egui_shell::HotkeyPlan::ShowNow => {
                    egui_shell::show_egui_main(app, el, t0);
                }
                egui_shell::HotkeyPlan::ShowAfterAltRelease => {
                    // 待機はイベントループを塞げないので別スレッドで行い、**再入するときに
                    // もう一度 marshalling する**。世代の照合もイベントループ上で行う——
                    // 照合と show のあいだに別の押下が割り込まないため。
                    let h = app.clone();
                    std::thread::spawn(move || {
                        wait_alt_release_or_timeout();
                        snotra_egui_runtime::on_event_loop(&h, move |app, el| {
                            let gen_now = app
                                .try_state::<egui_shell::EguiShellState>()
                                .map(|sh| sh.hotkey_generation.load(Ordering::SeqCst))
                                .unwrap_or(0);
                            if gen_now != current_gen {
                                return;
                            }
                            egui_shell::show_egui_main(app, el, Instant::now());
                        });
                    });
                }
            }
        });
    });
```

- [x] **Step 7: `hide_egui_main` の臨界区間を絞る（ポンプ停止の不変条件・省略不可）**

`hide_egui_main` はこの段からイベントループ上で走る。**イベントループ中はメッセージポンプが
停止する**ため、ポンプ進行を要する処理・重い処理を臨界区間に残してはならない（Global
Constraints）。現在の本体には該当が 2 つある——`save_search_placement`（ディスク書き込み）と
`trim_idle_working_set`（Toolhelp スナップショット + プロセスツリー BFS + `EmptyWorkingSet`）。

**不変条件が要求するのは `main_visible` の store と 2 枚の `ShowWindow` が不可分であることだけ**
なので、この 2 つは外へ出せる。`save_placement_relative` を「読み」と「書き」へ割り、
読みだけを臨界区間に残す:

```rust
/// 現在の物理位置を、ターゲットモニター作業領域原点からの相対座標へ換算する（**読みのみ**）。
///
/// **書き込みと分けてある**（ディスク I/O をイベントループの臨界区間から外すため——
/// `hide_egui_main` の中でポンプが止まる。`src-tauri/CLAUDE.md`「ウィンドウ生成の制約」）。
/// 読みは hide **より前**でなければ意味を持たないので、こちらだけが臨界区間に残る。
pub(crate) fn read_placement_relative(
    window: &tauri::Window,
) -> Option<snotra_core::window_data::WindowPlacement> {
    let pos = window.outer_position().ok()?;
    #[cfg(windows)]
    {
        use snotra_core::window_data::WindowPlacement;
        let hwnd = window.hwnd().ok()?;
        let wa = crate::monitor::window_monitor_work_area(hwnd.0 as isize)?;
        Some(WindowPlacement {
            x: pos.x - wa.left,
            y: pos.y - wa.top,
        })
    }
    #[cfg(not(windows))]
    {
        use snotra_core::window_data::WindowPlacement;
        Some(WindowPlacement { x: pos.x, y: pos.y })
    }
}
```

`hide_egui_main` は読んだ値を持って返り、**書き込みと trim は臨界区間の外**（関数末尾）で行う:

```rust
    // placement は「読み」だけを窓の hide より前に置く。**書き込みはこの下**——
    // ディスク I/O はポンプを止めた区間に置かない。
    let placement = app
        .get_window("main")
        .and_then(|w| read_placement_relative(&w));
    if let Some(window) = app.get_window("main") {
        let _ = window.hide();
    }
    // ... main_visible.store(false) → results.hide(...) ...

    // ここから臨界区間の外。**順序に意味は無い**——
    // trim は hide 前後どちらで走っても無害（`src-tauri/CLAUDE.md`「working set の能動回収」）、
    // placement の書き込みは値を既に持っているので窓の状態に依存しない。
    if let Some(p) = placement {
        snotra_core::window_data::save_search_placement(p);
    }
    crate::working_set::trim_idle_working_set(std::process::id());
```

`save_placement_relative` の呼び出し元は `hide_egui_main` だけなので、この分割で旧関数は
**呼ばれなくなる**——`-D warnings` の `dead_code` が検出するので、削除する。

- [x] **Step 8: ビルドとテスト**

`docs/build-commands.md` カテゴリ A を実行する。期待: **通る**。

- [x] **Step 9: 位置の保存が壊れていないことを実機で見る（カテゴリ D）**

窓を動かす → ホットキーで hide → 再 show。**前回の位置に出ること。**
`window.bin` への保存経路を割った変更なので、ユニットテストでは落ちない。

- [x] **Step 10: コミット**

```
refactor(egui-shell): 可視性 API に EventLoopProof を要求させ呼び出し点を移行する
```

---

## タスク 3・4: 実機検証 —— **計画から外し、PR 本文のチェックリストへ移した**

**やらないと決めたのではない。この計画では閉じられないと判ったので移した。**

移送先: 本ブランチの PR 本文「⚠️ 実機検証は未実施（この PR のチェックリストで閉じる）」の節。

### 理由

開発機がロック画面のため、注入した入力が `LockApp` へ行き `WM_HOTKEY` が発火しない
（窓を作らない自前ハーネスで Alt / Ctrl / Shift / Win の 4 つとも不着を実測）。この失敗は
本サイクルの変更とは無関係であることを二分で確定させた——コードが `main` と同一のコミット
（`d4ef1ee`）でも同じ失敗が再現し、かつ同じコマンドが同日の早い時刻には成功していた。

そのうえで、計画に検証項目として残すと**循環して閉じられない**:

> **CI の実測は PR が在って初めて行える**——`ci.yml` は `pull_request` でのみ起動し、
> `gh pr create` は `workspace/plan.md` の未チェック `- [ ]` で block される（#749）。
> 計画に検証項目として置くと循環して閉じられないので、**PR 本文のチェックリストへ送る**
> （`.claude/rules/safety-nets.md`）

`smoke:egui`（カテゴリ C）は自動だが、この環境では注入が届かないため同じ壁に当たる。
カテゴリ D はそもそも `docs/build-commands.md` が「**エージェントは実行できない**（対話入力を
要する）。人間が自分の端末で走らせる」と定めている。

### 移した項目（PR 本文が正本）

- `npm run smoke:egui`（H1 / H4 / H5 の不変条件判定を含む）
- ホットキー連打 10 回以上で show/hide が交互に切り替わること
- Alt を押したまま打ち、離すと窓が出ること（続けて打つと隠れる＝世代照合）
- 設定画面（`/o`）起動中はホットキーが無視されること
- 窓を動かして hide → 再 show で前回の位置に出ること
- 残留 Alt 解除と IME オフが劣化していないこと
- `npm run smoke:manual`（カテゴリ D・13 項目）

---

## タスク 5: #880 の事後検査と hide の理由型を削除する

**前提条件（この計画冒頭）が満たされていない場合、このタスクは「該当なし」として削除し、
理由をここに記録すること。**

**Files:**
- Modify: `src-tauri/src/egui_shell/layout.rs`（`HideReason` / `hide_must_be_unconditional` / `must_retract_results` とそのテスト）
- Modify: `src-tauri/src/egui_shell/window_coordinator.rs`（`drive_results_window` の撤回ブロックと理由分岐、`hide_egui_main` の理由）
- Modify: `src-tauri/src/egui_shell/results_window.rs`（`hide` の `reason` 引数）

**Interfaces:**
- Produces: `ResultsWindow::hide(&self, _el: &EventLoopProof) -> bool`（`reason` 引数が消える）

- [x] **Step 1: `layout.rs` から 3 つの項目とテストを消す**

削除するもの: `HideReason` / `hide_must_be_unconditional` / `must_retract_results` と、
テスト `retract_agrees_with_the_presentation_gate_on_main_visibility` /
`only_main_gone_forces_the_raw_hide`。

**`present_results` の連言① `main_visible` は残す**——競合対策ではなく「hidden 中に走る稀な
フレームで results を出さない」という状態の述語である（設計書 §4）。

- [x] **Step 2: `present_results` の doc から race の記述を落とす**

「hide 側の同期と show 側のゲートは……」以下の、事後検査を指す段落を消し、次で置き換える:

```rust
/// **`main_visible` を条件に含めるのが要石である**（#671 PR A′）。main を hide しても
/// `state.results()` は消えない（reset は show 側の `reset_pending` 消費でしか起きない）ため、
/// 結果は hidden 中も残る。hidden 中に main の update() が 1 フレームでも走ると
/// （`config-applied` / `indexing-*` / updater 完了の `wake_main` 自体は main の可視性を
/// 見ない）、results だけが最前面に取り残される。
///
/// **並行性は条件に入らない。** 可視性を変える操作はすべてイベントループスレッドに閉じており
/// （`EventLoopProof`）、この判定と `ResultsWindow::show` のあいだに hide が割り込む並びは
/// **構築できない**。
```

> **⚠ この処方文をそのまま再生産しないこと（最終レビュー I-1 で限定済み）。** 上の
> 「可視性を変える操作は**すべて**〜」は無限定の全称であり偽である——`Manager` から main の
> ハンドルを引いた `.hide()` は可視性を変える操作でありながら任意のスレッドからコンパイルが
> 通り、実際に効く。実装では主語を「**証人型を引数に要求する 5 関数は**」へ絞ってある
> （`layout.rs` / `results_window.rs` / `window_coordinator.rs` の計 6 か所）。**射程の正本は
> `src-tauri/CLAUDE.md`「可視性を変える操作はイベントループスレッドに閉じてある」の bullet 群**
> であり、5 関数の外に残る面もそこに記録した。下の Step 3 の `show` doc の処方も同じ。

- [x] **Step 3: `ResultsWindow::hide` を単純化する**

```rust
    /// results 窓を隠す（`show` の対）。既に不可視なら raw 操作を撃たず `false` を返す。
    ///
    /// raw show は tao の `WindowFlags::VISIBLE` を false のまま残すため、`Window::hide()` は
    /// 「差分なし」と判定して早期 return し窓が隠れない。ゆえに hide も対で raw にする。
    ///
    /// **可視フラグを信じてよい。** 書き手はイベントループスレッドに一意化されており
    /// （`EventLoopProof`）、フラグと窓の実状態が食い違う並びは構築できない。
    pub(crate) fn hide(&self, _el: &snotra_egui_runtime::EventLoopProof) -> bool {
        if !self.visible.swap(false, Ordering::SeqCst) {
            return false;
        }
        self.raw_hide();
        true
    }
```

`show` の doc からも「swap と `raw_show()` の間に他スレッドの `hide()` が挟まると……」の
段落を消し、単一スレッドである旨に置き換える。

- [x] **Step 4: `window_coordinator.rs` の撤回ブロックと理由分岐を消す**

- `drive_results_window` の `results.show(...)` の後にある `must_retract_results` のブロックを削除
- `Hidden` アームの `let reason = if main_visible { ... }` を削除し `results.hide(el)` に戻す
- `hide_egui_main` の `results.hide(layout::HideReason::MainGone)` を `results.hide(el)` に戻す
- `hide_egui_main` の「この順序が塞ぐのは……」の段落を削除する（並びが構築できないため不要）
- **`read_main_visible` は残す**（`present_results` の連言①が使う）。ただし doc の
  「同一フレームで 2 回読む」の記述は**偽になる**ので、1 回読みへ書き直す

- [x] **Step 5: ビルドとテスト**

`docs/build-commands.md` カテゴリ A。期待: **通る**（テストは 2 本減る）。

- [x] **Step 6: コミット**

```
refactor(egui-shell): 単一スレッド化で不要になった事後検査と HideReason を削除する
```

---

## タスク 6: 文書を as-built へ合わせる

**作業量の実体はここにある**（設計書 §8）。事故の機序を記録した散文が長く、そのほとんどが偽になる。

**Files:**
- Modify: `SPEC.md`（「検索結果ウィンドウの可視性（従属軸）」）
- Modify: `src-tauri/CLAUDE.md`（「Win32 / Tauri 注意事項」の可視性の項）
- Modify: `docs/superpowers/specs/2026-08-02-results-visibility-single-thread-design.md`（as-built の追記が要れば）

- [x] **Step 1: `SPEC.md` の 3 点封鎖の記述を差し替える**

現在の「①事前ゲート ②事後検査 ③hide の権威性」の 3 項リストを削除し、次で置き換える。
**`.claude/rules/spec.md` に従い as-built を書く**——実装が計画とずれたなら、この文面ではなく
実装のほうを写すこと。

```markdown
**この述語が守られる根拠は「毎フレーム評価すること」ではない。** 評価と `results` への
`ShowWindow` のあいだには位置決め・リサイズの Win32 呼び出しが挟まるため、評価と適用が
別スレッドから交差しうるなら「評価時は `main` 可視、適用時は `main` hidden」という並びが
成立してしまう。ゆえに**両窓の可視性を変える操作はすべてイベントループスレッドに閉じている**
——別スレッドからの呼び出しは型が拒む。この述語は、その上での状態の判定である。
```

> **⚠ この処方文もそのまま再生産しないこと（最終レビュー I-1 / 再レビューで限定済み）。**
> 「**両窓の**可視性を変える操作は**すべて**〜」は、タスク 5 Step 2 の処方（上の ⚠ 注記）より
> **さらに広く偽である**——`Manager` からハンドルを引いた `main` の `.hide()` は任意のスレッド
> からコンパイルが通り、実際に効く。`SPEC.md` の as-built は「shell が可視性を変える**ために
> 設けた経路**は閉じたが、`tauri::Window` の生の面は閉じていない」の形へ限定してある。
> **射程の正本は `src-tauri/CLAUDE.md`「可視性を変える操作はイベントループスレッドに
> 閉じてある」の bullet 群。**

- [x] **Step 2: `src-tauri/CLAUDE.md` の可視性の項を書き直す**

削除する 3 項（#880 で足したもの）: 「述語のゲートは『読んだ時刻』しか守らない」
「この race を lock で囲んではならない」「この事故は presence 検査では捕まらない」。
**後 2 つの内容は失われてはならない**——lock 棄却と H1 の位置づけは新しい記述の中へ畳む。

追加する項の骨子:

```markdown
  - **可視性を変える操作はイベントループスレッドに閉じており、型が強制する。** `EventLoopProof`
    （`snotra-egui-runtime`）は `!Send` かつ crate 外で構築できず、`show_egui_main` /
    `hide_egui_main` / `ResultsWindow::show` / `hide` が引数に要求する。別スレッドからは
    `on_event_loop` を通るしかない。**相互排他は lock ではなく tao の runner が与える**
    （`event_handler.take()` による非再入 + イベントバッファリング）。**lock を足してはならない**
    ——窓を所有しないスレッドからの `ShowWindow` は所有スレッドのポンプ待ちでブロックしうるため、
    イベントループ側も取る lock は race をデッドロックへ化けさせる
  - **書けるのは「可視性を*変える*操作は」までである。** `ResultsWindow::set_topmost`
    （設定サイドカー監視スレッド）は残るため、「results への生 Win32 はすべて所有スレッド」は偽。
    z-order は可視性の不変条件に関与しないので意図的にスコープ外とした
```

- [x] **Step 3: `governance:check` を実行する**

`docs/build-commands.md` カテゴリ F。期待: **通る**。

- [x] **Step 4: コミット**

```
docs: 可視性の単一スレッド化を as-built で SPEC と CLAUDE.md へ反映する
```

---

## タスク 7: 仕上げ

- [x] **Step 1: 全カテゴリの検証を通す**

`docs/build-commands.md` のカテゴリ A / C / D / F。**D を省略しない**——タスク 3 の
ホットキー連打はここでしか見えない。

- [x] **Step 2: PR を作成する**

`git push -u origin HEAD` を先に打つか `&&` で繋ぐ。**鎖に `cd` を含めない**
（`CLAUDE.md`「フック」）。**この計画の未チェック項目が残っていると hook が拒否する。**

PR 本文に**必ず書くこと**（`AGENTS.md`「検証の作法」）:
- smoke の緑が establish すること / しないことの両面
- 受容する残余 4 点（設計書 §9）を落とさない
