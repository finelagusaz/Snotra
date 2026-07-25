# PR D: ctx 複製の解消（窓ごと wake handle）+ setup 順序の終端形 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `EguiShellState` が窓ごとに持っていた `egui::Context` の clone（`egui_ctx` / `results_ctx`）を撤去し、runtime が `attach` 時に返す**非 Option の wake handle** へ置き換える。あわせて spec 決定 8 の setup 順序を終端形へ移す。

**Architecture:** runtime（`snotra-egui-runtime`）が窓ごとに持つ repaint 経路の**送信側**を `WindowWaker` として公開する。`EguiRuntime::attach` が `WindowWaker` を返し、shell はそれを managed state に置くだけになる——登録スロット（`Mutex<Option<egui::Context>>`）と登録関数（`register_ctx`）が要らなくなる。

**Tech Stack:** Rust / Tauri v2.11 / egui 0.33

**根拠となる spec:** `docs/superpowers/specs/2026-07-25-egui-window-ownership-and-event-delivery-design.md` の **PR D 行（§4）と決定 8**。前提サイクル: PR A / A′ / B / C マージ済み。

## この PR が閉じるもの / 閉じないもの

**閉じる:**

1. **登録スロットの窓ごとの複製**（#671 項目 2）。`Mutex<Option<egui::Context>>` × 2 + `register_ctx` + `wake_ctx` が消える。`EguiShellState` の wake フィールドは**非 Option** になり、「未登録＝無害な no-op」という論法が消える。
2. **`Destroyed` を越える長寿命 Context clone**。現在 managed state の Context clone が repaint callback を生かし、callback が握る `RepaintScheduler` の Arc ゆえ `SchedulerInner::drop`（stop + join）が走らない（`snotra-egui-runtime/CLAUDE.md`「repaint worker は所有型の Drop で停止し、join する」の破れ）。

**閉じない（over-claim しないこと）:**

- **worker スレッドが持つ一時的な Context clone は残る。** `spawn_folder_load`（`view.rs`）・`spawn_icon_load`（`results_view.rs`）は `ui.ctx().clone()` を worker へ move し、送信ごとに `request_repaint()` する（#532 SU5 の不変条件そのもの・これは egui の正しい作法）。dead UNC の `read_dir` でスレッドが hang した場合はこの clone も残る（既に受容済みのリーク・`view.rs` の `spawn_folder_load` doc）。**したがって主張は「`Destroyed` を越える長寿命の Context clone が runtime の外に無くなる」までで、「join が常に走る」ではない。**
- **`try_state::<EguiShellState>()` の `Option` は残る。** これは Tauri managed state の性質であり、本 PR の対象外。消えるのは**2 段目**の Option（登録スロット）である。
- **活性化前の wake は「最初の live-read フレーム」を活性化側の `request(ZERO)` に依存する。** wake 経路が Sender ゆえ要求自体は queue され失われないが、それが描画に化けるのは `attach_pending_windows` が走った後である。

## Global Constraints

- **`WindowWaker` に `#[must_use]` を付けない。** `snotra-egui-mvp/src/main.rs:756` は `attach(window, view)?;` と戻り値を捨てており、`-D warnings` で落ちる。#660 で当 crate を削除するまで壊さない（下流 compile を検証項目に入れる）。
- **`EguiWindow.context` と `visible` / `Focused(true)` arm には触らない**（spec 決定 1）。本 PR が runtime に足すのは wake 経路の送信側の公開だけである。
- **`ctx.request_repaint()` の view 内利用（worker wake・遅延 dispatch）を `WindowWaker` へ置き換えない。** 自窓の Context を持っている場所では `request_repaint()` が正しい作法であり、`WindowWaker` は**外部スレッド・別窓からの wake** のための経路である。
- **`wake_view` / `wake_results` の 2 関数は薄い自由関数として残す（改名する）。** spec §4 の撤去リストは `wake_view` / `wake_results` を含むが、これは「ctx スロットを読む実装ごと」の撤去である。呼び出し点は 7 箇所あり（下記）、各所に `try_state` + `wake()` を手書きすると `/dry-check` が検出する重複になる。**旧名は消し**（`wake_view` → `wake_main`）、新機構であることを名前でも示す。`wake_results` は名前を保つ（対称のため）。
- **`smoke:egui` の trace 期待値を変えない。** 本 PR は trace を追加も削除もしない（PR A が置いた `egui_results:show` / `:hide` と `hotkey:registered` はそのまま）。

## 「束ねる / 消す」前の読み手列挙（AGENTS.md 条件別チェック表）

> 消す・移す各項目について「**後で**読まれる/立つことに依存していないか」を 1 行ずつ書き出す。A′ はこれを省いて回帰した（view-local の stale フラグが再表示を防いでいた）。

| 消す / 移すもの | 誰がいつ読むか | 新しい機構で満たされるか |
|---|---|---|
| `EguiShellState.egui_ctx` | `wake_ctx` 経由で `wake_view` のみ（mod.rs 3 箇所・view.rs 1・results_view.rs 1）。書き手は `view.setup()` 1 箇所 | `main_waker.wake()` が同じ `RequestRedraw` を出す。**書き手が消える**（attach が返す） |
| `EguiShellState.results_ctx` | `wake_results` のみ（view.rs 2 箇所）。書き手は `results_view.setup()` 1 箇所 | 同上（`results_waker`） |
| `register_ctx` | `view.setup()` / `results_view.setup()` の 2 箇所だけ | 呼び出し元ごと消える |
| `wake_ctx` | `wake_view` / `wake_results` の 2 箇所だけ | `WindowWaker::wake()` が代替 |
| 名前 `wake_view` | 7 呼び出し点（mod.rs 164 / 552 / 593、view.rs 940、results_view.rs 458 が main 宛。view.rs 747 / 1552 が results 宛） | 改名のみ。compile-fail が移行漏れを検出 |
| `app.manage(EguiShellState)` の**位置**（`create()` の前 → 後） | **これが本 PR で唯一「後で読まれる」が問題になる項目**。読み手は view.rs 231（hide emit dedup）/ 1034（reset_pending）/ 1085（pending_hotkey_failure）、results_view.rs は**読まなくなる**（register_ctx 撤去で参照ゼロ）、main.rs 395 / 424（hotkey 世代）、mod.rs 312 / 403（show/hide）/ 563 / 589（failure listener） | **前提: `create()` から新 manage 位置までの間に egui フレームは 1 枚も走らない。** 根拠と検証は次節 |
| `EguiShellState::default()` | main.rs 286 の 1 箇所 | `new(main_waker, results_waker)` へ。他フィールドは従来の既定値を明示構築 |

### 前提「create() から manage までにフレームは走らない」の接地

`EguiShellState` の読み手は**すべて `if let Some(...)`** である。したがって manage 前にフレームが 1 枚走ると、`reset_pending` の消費や `pending_hotkey_failure` の消費が**沈黙して skip される**（trace も出ない）。「Tauri の setup フックは `app.run()` のイベントループより前に走る」という記憶に依らせず一次資料で確認した:

**判明したこと（記憶は誤りだった）**: tauri は setup フックを**イベントループの中**で呼ぶ——`tauri-2.11.4/src/app.rs` の `make_run_event_loop_callback` は `RuntimeRunEvent::Ready` の arm で `setup(&mut self)` を実行する（`app.rs:1422-1429`）。したがって「setup はイベントループより前」は偽である。

**それでも前提は成り立つ。根拠は 2 つ**:

1. **setup はイベントループの 1 イテレーション内で走り、その間メッセージポンプは停止している**（`src-tauri/CLAUDE.md`「ウィンドウ生成の制約」が既に記録している不変条件と同じ機構）。wry plugin の `on_event`（= `attach_pending_windows` → 初フレーム）は setup が復帰した後の別イベントでしか走らない。
2. **仮にフレームが走っても、この時点で pending なものは無い。** `reset_pending` は `show_egui_main` が、`pending_hotkey_failure` は 2 つの failure listener が、`hotkey_generation` は `hide_egui_main` と hotkey listener が立てる——**setter は全て manage より後にしか動かない**（`hide_pending` は view 自身が立てるが、起動直後に hide 要求は出ない）。1 が破れても消費すべきものが無いため無害である。

**経験的な追加確認（任意）**: `trace.rs` の seq は単一 `AtomicU64` の単調増加列ゆえ、`smoke:egui` の trace で `hotkey:registered`（setup 中・PR A 決定 10）の seq がフレーム由来 trace（`egui_results:show`）より小さいことを見れば 1 を実測できる。smoke の assertion には足さない（PR A の期待値を動かさないため）。

## テストの位置づけ（AGENTS.md ステップ 9 への回答）

1. **compile-fail**（`attach` の戻り値型変更・`EguiShellState` のフィールド消滅・`wake_view` 改名）が移行漏れ検出器。`cargo build -p snotra-egui-mvp`（下流）も回す
2. `wake_channel` のユニットテスト 2 本（`snotra-egui-runtime`・下記 Task 1）
3. `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test --workspace`
4. `npm run governance:check`（**新規ファイルは追加しない**方針だが、`CLAUDE.md` の索引を触るため回す）
5. `npm run smoke:startup` + `npm run smoke:egui`（カテゴリ C・`.claude/rules/src-tauri.md` のトリガに該当。post-edit hook の沈黙は A だけなので「沈黙 = 合格」は成立しない）
6. **実機目視（省略不可）**: ホットキーで show → 1 文字入力で results 出現 → Escape で両窓 hide → 再 show。加えて**設定を外部から変更**（`config.toml` を保存）して config-applied wake が main を起こすこと（= `wake_main` が効いていること）

**5 は本 PR の wake 経路を end-to-end で検査しない部分がある。** `smoke:egui` は文字注入 → `egui_results:show` を見るため results 宛の wake（`view.rs:1552` / `:747`）は間接的に被覆されるが、**`wake_main` の経路（config-applied / updater / results クリック逆流）は smoke に入力が無い**。6 の config 変更が唯一の接地した観測点である。

---

### Task 1: runtime に `WindowWaker` を足す（TDD）

**Files:**
- Modify: `snotra-egui-runtime/src/repaint.rs`（`WindowWaker` / `WakeReceiver` / `wake_channel` + テスト）
- Modify: `snotra-egui-runtime/src/lib.rs`（公開）

**Interfaces:**
- Produces: `pub struct WindowWaker`（`Clone + Send + Sync`）、`pub fn wake(&self)`
- Produces: `pub(crate) struct WakeReceiver`、`pub(crate) fn wake_channel() -> (WindowWaker, WakeReceiver)`
- Changes: `RepaintScheduler::new(proxy, window_id, wake: WakeReceiver)`（旧: 内部で channel を作っていた）

**設計の核（なぜ `Weak` ではなく Sender か）:**

`WindowWaker` は repaint worker への **mpsc 送信側そのもの**を持つ。`RepaintScheduler` の Arc を（強参照でも弱参照でも）持たないため、shell が waker を永久保持しても `SchedulerInner::drop` は動く——`drop` は `Stop` を**明示送信してから** join するので、チャネルの切断（全 Sender の drop）を待たない。これは実装済みの挙動であり、本 PR が新たに作る前提ではない（`repaint.rs` の `Drop` と worker ループの `Some(SchedulerMessage::Stop) => break` を参照）。

活性化前の `wake()` は**要求が queue される**（`Instant::now()` 起点の過去 deadline ゆえ、worker 起動後ただちに 1 回 `RequestRedraw` が飛ぶ）。活性化自身も `request(ZERO)` を撃つので実効差は無い。

- [ ] **Step 1: テストを書く（Red）**

`repaint.rs` の `mod tests` に:

```rust
#[test]
fn wake_before_activation_is_queued() {
    // 活性化前（worker 未起動）の wake は落ちずに queue される。
    let (waker, wake_rx) = wake_channel();
    waker.wake();
    waker.wake();
    assert!(matches!(wake_rx.receiver.try_recv(), Ok(SchedulerMessage::Request { .. })));
    assert!(matches!(wake_rx.receiver.try_recv(), Ok(SchedulerMessage::Request { .. })));
    assert!(wake_rx.receiver.try_recv().is_err(), "3 通目は無い");
}

#[test]
fn wake_after_receiver_drop_is_silent() {
    // 窓が Destroyed になった後の wake は無害な no-op（panic せず、戻り値も持たない）。
    let (waker, wake_rx) = wake_channel();
    drop(wake_rx);
    waker.wake(); // panic しないこと自体が assertion
}
```

`wake_channel` が無いので compile-fail = Red。

- [ ] **Step 2: `WindowWaker` / `WakeReceiver` / `wake_channel` を実装（Green）**

```rust
/// 窓を外部から起こすハンドル（`EguiRuntime::attach` が返す・#671 PR D）。
///
/// 窓ごとの `egui::Context` を clone して外へ配る代わりに、**repaint worker への
/// 送信側だけ**を渡す。`RepaintScheduler` の Arc を持たないため、このハンドルを
/// 永久保持しても窓の `Destroyed` での stop + join を妨げない
/// （`SchedulerInner::drop` は `Stop` を明示送信してから join する）。
///
/// **活性化前の wake は queue される**（イベントループが窓を活性化した直後に 1 回
/// 描画要求として現れる）。活性化自身も `request(ZERO)` を撃つため実効差は無い。
#[derive(Clone)]
pub struct WindowWaker {
    sender: Sender<SchedulerMessage>,
}

impl WindowWaker {
    /// 次フレームを要求する。窓が既に破棄されていれば無害な no-op。
    pub fn wake(&self) {
        self.request(Duration::ZERO);
    }

    pub(crate) fn request(&self, delay: Duration) { /* Request { deadline: now + delay } を送る */ }

    pub(crate) fn stop(&self) { /* Stop を送る */ }
}

/// wake 経路の受信側（活性化時に `RepaintScheduler::new` が消費する）。
/// `sender` を同梱するのは、活性化側が worker 用の送信側を別途受け取らずに済むため。
pub(crate) struct WakeReceiver {
    sender: Sender<SchedulerMessage>,
    receiver: Receiver<SchedulerMessage>,
}

pub(crate) fn wake_channel() -> (WindowWaker, WakeReceiver) { /* channel を 1 本作り両側を返す */ }
```

`RepaintScheduler` は `SchedulerInner { waker: WindowWaker, worker: Mutex<Option<JoinHandle<()>>> }` を持ち、`request(delay)` は `inner.waker.request(delay)` へ委譲、`Drop` は `waker.stop()` → join。

- [ ] **Step 3: `lib.rs` で `WindowWaker` を公開**（`pub use repaint::WindowWaker;`。`repaint` モジュール自体は private のまま）

- [ ] **Step 4: 検証** — `cargo test -p snotra-egui-runtime` / `cargo clippy -p snotra-egui-runtime --all-targets -- -D warnings`

---

### Task 2: `attach` が `WindowWaker` を返す

**Files:**
- Modify: `snotra-egui-runtime/src/runtime.rs`
- Modify: `snotra-egui-runtime/CLAUDE.md`（モジュール構成の `repaint.rs` 行・不変条件）

**Interfaces:**
- Changes: `EguiRuntime::attach(&self, window, view) -> Result<WindowWaker, RuntimeError>`

- [ ] **Step 1: pending の値を `PendingWindow` にする**

`pending: Arc<Mutex<HashMap<String, PendingWindow>>>` とし

```rust
/// 活性化待ちの窓。`wake_rx` は活性化時に `RepaintScheduler` へ渡して消える
/// （活性化後は存在しないので `Option` にしない）。
struct PendingWindow {
    window: EguiWindow,
    wake_rx: WakeReceiver,
}
```

`RuntimePluginBuilder` / `RuntimePlugin` の `pending` 型も追随（機械的）。

- [ ] **Step 2: `attach` で channel を作り waker を返す**

```rust
let (waker, wake_rx) = crate::repaint::wake_channel();
let egui_window = EguiWindow::new(window, Box::new(view))?;
// ... entry.insert(PendingWindow { window: egui_window, wake_rx }) ...
Ok(waker)
```

**重複ラベルのエラー経路では waker を捨てる**（挿入しないので受信側ごと落ちる）。

- [ ] **Step 3: `attach_pending_windows` で受信側を worker へ渡す**

```rust
let Some(PendingWindow { mut window, wake_rx }) = pending.remove(&label) else { continue };
// renderer 生成（失敗時は continue = wake_rx ごと drop）
let scheduler = RepaintScheduler::new(proxy.clone(), window_id, wake_rx);
```

以降は現行どおり（callback 設定 → `request(ZERO)` → `active.insert`）。**`set_request_repaint_callback` は残す**——view 内の `request_repaint()` / `request_repaint_after()` の経路である。

- [ ] **Step 4: `snotra-egui-runtime/CLAUDE.md` を更新**
  - モジュール構成の `repaint.rs` 行に「窓を外部から起こす `WindowWaker`（`attach` の戻り値）の所有」を足す
  - 不変条件「repaint worker は所有型の Drop で停止し、join する」に、**`WindowWaker` が Arc を持たないことがこの不変条件の前提である**旨を 1 行足す（次に触る人が waker に scheduler を持たせると静かに破れる）

- [ ] **Step 5: 検証** — `cargo build -p snotra-egui-mvp`（下流 compile が移行漏れ検出器）+ clippy + test

---

### Task 3: shell 側の置き換え

**Files:**
- Modify: `src-tauri/src/egui_shell/mod.rs`
- Modify: `src-tauri/src/egui_shell/view.rs`
- Modify: `src-tauri/src/egui_shell/results_view.rs`
- Modify: `src-tauri/src/egui_shell/layout.rs`（コメント内の `wake_view` 参照）

**Interfaces:**
- Produces: `pub(crate) struct EguiShellHandles { results_window: ResultsWindow, main_waker: WindowWaker, results_waker: WindowWaker }`
- Changes: `create(...) -> Result<EguiShellHandles, RuntimeError>`
- Changes: `EguiShellState::new(main_waker, results_waker)`（`Default` は消える）
- Changes: `wake_view` → `wake_main`。`wake_results` は名前を保つ
- Removes: `register_ctx` / `wake_ctx` / `EguiShellState.egui_ctx` / `.results_ctx`

- [ ] **Step 1: `create()` が handle をまとめて返す**

`runtime.attach(results, ...)` と `runtime.attach(window, ...)` の戻り値を受け、`EguiShellHandles` を返す。**attach の順序（results → main）は変えない**——`ResultsWindow::new(results.clone())` が attach 前でなければならない制約（PR A′）と、Moved リスナー登録の位置はそのまま。

- [ ] **Step 2: `EguiShellState` のフィールド差し替え**

`egui_ctx` / `results_ctx` を削除し `main_waker: WindowWaker` / `results_waker: WindowWaker` を足す。`#[derive(Default)]` を外し `new(main_waker, results_waker) -> Self` を置く（他 4 フィールドは従来の既定値を明示構築）。doc コメントの「updater check 完了時に可視中の view を起こすための egui Context」は wake handle の説明へ書き換える。

- [ ] **Step 3: `register_ctx` / `wake_ctx` を削除し `wake_main` / `wake_results` を書き換え**

```rust
/// 可視中の main 窓を起こす（#671 PR D: 窓ごと Context の clone を保持していた旧実装の後継）。
/// hidden 中は OS が hidden 窓を描かないため実効的な no-op（機構は spec §7 残余 2 で未測定）。
pub(crate) fn wake_main(app: &tauri::AppHandle) {
    if let Some(sh) = app.try_state::<EguiShellState>() {
        sh.main_waker.wake();
    }
}
```

`wake_results` も同型。**2 つを 1 関数（窓を引数）に束ねない**——呼び出し側が「どちらの窓を起こすか」を型でなく引数で選ぶ形は、#671 項目 2 が問題にした「窓ごとに線形に増える配線」を関数の中へ移すだけで、可読性を落とす。

- [ ] **Step 4: `view.setup()` / `results_view.setup()` から `register_ctx` を落とす**

`SearchWindowView::setup` は font 設定だけになり、`EguiShellState` を参照しなくなる（**これが決定 8 の順序変更の前提**）。`ResultsView::setup` も同様。`results_view.rs` の `use crate::egui_shell::{EguiShellState, ...}` から `EguiShellState` を落とす（未使用 import は `-D warnings` で落ちる）。

- [ ] **Step 5: 呼び出し点 7 箇所を新名へ** — `wake_view` → `wake_main`（mod.rs 3・view.rs 1・results_view.rs 1）。`wake_results` は無変更（view.rs 2）。コメント内の参照（`layout.rs` の `wake_view`、`view.rs` の `spawn_folder_load` doc、`results_view.rs` の「main の egui_ctx と同型」）も現状に合わせる

- [ ] **Step 6: 検証** — clippy + `cargo test -p snotra`

---

### Task 4: setup 順序を決定 8 の終端形へ

**Files:**
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/CLAUDE.md`（`egui_shell/` 節の wake 記述）

- [ ] **Step 1: manage の順序**

```rust
setup_platform_thread(...);
let handles = egui_shell::create(app, window_width as f64, &bg_color)?;
app.manage(egui_shell::EguiShellState::new(handles.main_waker, handles.results_waker));
app.manage(handles.results_window);
egui_shell::register_hide_listener(&app_handle);
// 以降は現行順を維持
```

コメントは「`create()` の後へ移せる根拠 = 両 `setup()` が `EguiShellState` を読まなくなった」＋「フレームが走る前に manage が済む前提」を書く（spec 決定 8 と本計画の前提節を指す）。**listener 登録より前**という A′ の制約は保たれる。

- [ ] **Step 2: `src-tauri/CLAUDE.md` を更新** — `egui_shell/` 節の「イベント駆動 wake の不変条件」に、外部からの wake は `wake_main` / `wake_results`（runtime の `WindowWaker`）であり **shell は Context を保持しない**旨を足す。`mod.rs` の責務行に残る ctx 前提の記述があれば直す

- [ ] **Step 3: 検証** — clippy + test + `npm run governance:check`

---

### Task 5: 検証と PR

- [ ] **Step 1:** `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test --workspace` / `cargo build -p snotra-egui-mvp`
- [ ] **Step 2:** `npm run smoke:startup` / `npm run smoke:egui`。**trace の seq で前提を接地**（`hotkey:registered` < 最初のフレーム由来 trace）
- [ ] **Step 3:** `npm run governance:check`
- [ ] **Step 4:** 実機目視（テストの位置づけ 6）。**config.toml 外部変更で main が即座に再描画されること**が `wake_main` の唯一の end-to-end 検査
- [ ] **Step 5:** commit / push / PR。**PR 本文に closing keyword を書く前に `gh pr view --json closingIssuesReferences` を見る**（#671 は項目 1 の残余〔表現不能化は達成しない・spec §7-1〕があるため close 対象かはユーザー判断。#673 も項目 2 を「やらない」と決めた記録が spec 決定 5 にあり、PR D で全部が閉じるとは限らない）
