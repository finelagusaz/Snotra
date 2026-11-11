# #1365 独立導出（plan.md / research.md / adversarial-1365.txt は未読）

base: ブランチ refactor/main-visible-live-read、HEAD = 8fa4fc60（作業ツリーの変更は `workspace/` のみ）。#1364 = f506783c はマージ済み。
コードは変更していない。

## 0. 前提の確認（一次証拠）

- キャッシュの現所在（#1364 で state.rs から移動済み）:
  - `src-tauri/src/egui_shell/window_coordinator.rs`: 型 `MainVisible(AtomicBool)`（`new` / `get` / module-private の `set`）と、その型の doc
  - `src-tauri/src/egui_shell/mod.rs`: `EguiShellState.main_visible` フィールド、struct doc の bullet、`EguiShellState::new` の初期化
  - **`state.rs` には main_visible も「~35ms」も、もう無い**（`git grep` 0 件）。issue の「state.rs の doc」は #1364 で解消済みで、所在が古い
- 書き手: `show_egui_main`（`window.show()` の後に `sh.main_visible.set(true)`）、`hide_egui_main`（`window.hide()` の後・`results.hide(el)` の前に `set(false)`）
- 読み手: `main.rs` `setup_hotkey_listener` の `on_event_loop` タスク内（`sh.main_visible.get()`）、`window_coordinator.rs` `read_main_visible` → `drive_results_window` の連言①
- 依存の実測（cargo registry のソース。Cargo.lock: tauri 2.12.5 / tauri-runtime-wry 2.12.3 / tao 0.37.2）:
  - `tauri::Window::is_visible` → `dispatcher.is_visible()` → `window_getter!` → `getter!` = `send_user_message` + `rx.recv()`（tauri-runtime-wry 2.12.3 `src/lib.rs:196-210`）
  - `send_user_message` は `current_thread().id() == self.main_thread_id` のときインラインで実行する（同 `lib.rs:263-`）。別スレッドからは post して `recv` で待つ＝往復
  - `WindowMessage::IsVisible(tx) => tx.send(window.is_visible())`（同 `lib.rs:3285`）→ tao `window.rs:628` → `util::is_visible` → `IsWindowVisible`（tao `util.rs:203`）
  - hide 側: tao `set_visible` は `execute_in_thread`（イベントループ上ならインライン・tao `event_loop.rs:518-`）→ `set_window_flags` → `apply_diff` → `ShowWindow(SW_HIDE)` を同期に撃つ（tao `window_state.rs:406-410`）。**ゆえにイベントループ上で `window.hide()` が返った時点で `IsWindowVisible` は false、`window.show()` が返った時点で true**
  - 同じ `window_getter!` 経路のゲッター（`main.outer_position()` / `outer_size()` / `scale_factor()`、1302-1304 行）は `position_results_below_main` の中にある。この関数は `drive_results_window` が Visible のフレームごとにガード無しで呼ぶ（1461 行。ほかに `mod.rs:405` の Moved リスナー）。フレーム内から getter を毎フレーム呼ぶ形には先例がある。新しい読みは Hidden のフレームでも走る点だけが違う
  - 切り詰められた grep（70KB）は `src-tauri/src` / `snotra-egui-runtime/src` に絞って再実行した。`is_visible` / `IsWindowVisible` の出現は `window_coordinator.rs:58`（`MainVisible` の doc）だけ。`docs/glossary.md` と `RETROSPECTIVE.md` には「可視」「visible」が 0 件

## 1. 変更するファイルとシンボル

### コード

| # | ファイル : シンボル | 変更 | 根拠 |
|---|---|---|---|
| C1 | `window_coordinator.rs` : `struct MainVisible` と `impl MainVisible`（`new`/`get`/`set`）と型の doc | 削除 | issue「キャッシュを撤去」。grep で参照は mod.rs の 2 か所と本ファイル内だけ |
| C2 | `window_coordinator.rs` : `use std::sync::atomic::{AtomicBool, Ordering}` | `AtomicBool` を外す（C1 の後、本ファイルで `AtomicBool` を使うのは `MainVisible` だけ。152 行は doc 内の語）。`-D warnings` で unused import になる | `grep -n AtomicBool window_coordinator.rs` |
| C3 | `window_coordinator.rs` : `fn read_main_visible(app)` と doc | 実状態を読む関数に置き換える。例: `pub(crate) fn main_is_visible(app: &tauri::AppHandle, _el: &snotra_egui_runtime::EventLoopProof) -> bool { app.get_window("main").and_then(\|w\| w.is_visible().ok()).unwrap_or(false) }`。hotkey（main.rs）からも呼ぶので `pub(crate)` にし、`mod.rs` の `pub(crate) use window_coordinator::{…}` に足す。doc には「イベントループ上ではインライン（往復なし）」「証人は別スレッドからの往復を型で防ぐため」「生 `IsWindowVisible` を呼ばない理由」を書く | issue「変更」・「EventLoopProof を受ける形」 |
| C4 | `window_coordinator.rs` : `show_egui_main` の本体、`window.show()` 直後の `// main_visible は show() の後に立てる（「順序不変」制約）。…` コメントと `sh.main_visible.set(true)` のブロック | 削除。順序制約は「実状態は `show()` が返った時点で true」に吸収される | C1 |
| C5 | `window_coordinator.rs` : `hide_egui_main` の本体、`// main_visible は **results.hide() より前**に落とす（#671）…`（7 行）と `sh.main_visible.set(false)` | 削除。`window.hide()` が先に同期で効くので、順序は「main の `window.hide()` → `results.hide()`」のまま自動的に保たれる | C1、tao の同期 hide |
| C6 | `window_coordinator.rs` : `drive_results_window` 本体、`let main_visible = read_main_visible(app);` と直前のコメント（`main_visible は hide_egui_main が results.hide() の**前**に false へ落とすため、…` から `…MainVisible の doc を見よ。` まで） | 呼び出しを `main_is_visible(app, el)` へ替え、コメントを「実状態をイベントループ上で読む／`window.hide()` は同期で効く／Manager 経由の hide は閉じ込めの外（正本は `hide_egui_main` の doc）」へ書き直す | C3 |
| C7 | `mod.rs` : `EguiShellState` の `main_visible` フィールド・struct doc の `- main_visible: …` bullet（2 行）・`new()` の `main_visible: window_coordinator::MainVisible::new(),` | 削除 | C1 |
| C8 | `main.rs` : `setup_hotkey_listener` の `on_event_loop` タスク内 `let visible = shell.as_ref().map(\|sh\| sh.main_visible.get()).unwrap_or(false);` | `let visible = egui_shell::main_is_visible(app, el);` へ。`shell` は `current_gen` の採番だけに残る | issue「hotkey は … へ置き換える」 |
| C9 | （任意）`main.rs` の hotkey タスク、あるいは `main_is_visible` 内 | issue の任意受け入れ条件（cold/warm 所要）を測るなら、一時的な trace を置く。**置くなら AGENTS.md「一時的な足場」行に従い、撤去までを新設の時点で決める** | issue 受け入れ条件（任意） |

### コード内の散文（doc / コメント）— 2. の全件表のうち、コード側で直すもの

2. の表を参照（W 系・L 系・R 系・M 系）。

### 文書

| # | ファイル : 箇所 | 変更 |
|---|---|---|
| D1 | `SPEC.md` §8.6 状態図 `SearchVisible --> Standby: hotkey-pressed [hotkey_toggle && main_visible]`（555 行）と遷移ルール要約「`hotkey_toggle && main_visible` が前提」（596 行） | ⚠️ `main_visible` は消える識別子の名前。SPEC は語彙層で書くので、同節の述語表と揃えて `main 可視` に替えるのが筋（挙動は変わらない）。直さないなら「SPEC の語としての `main_visible`」と読める理由を書く必要がある |
| D2 | `SPEC.md` 「検索結果ウィンドウの可視性（従属軸）」の最後の段落（631 行） | 現在形の主張はおおむね真のまま（Manager 経由の hide で results が取り残される残余は閉じない＝issue「残るもの」）。**偽になる文は見つけていない**。⚠️ ただし「連言①は main の実状態で評価する」の一文を足すかは任意。足すなら新規記述として検算する |
| D3 | `PERFORMANCE.md:17` の一般則「Win32 IPC の cold-call を避ける … `AtomicBool` 等で Rust 側にキャッシュし…」 | 偽になる（本変更が唯一の適用例を消す。さらに前提の「WebView2 への初回アクセス」は #532 SU7 で撤去済み）。書き直しの方向: 「tauri のウィンドウ getter は、イベントループの外から呼ぶとイベントループへの往復になる（`send_user_message` + `recv`）。イベントループ上ではインラインで走る。往復を避けるには読みをイベントループ上へ置く（キャッシュではなく）」。冪等な操作を pre-check なしで呼ぶ文は残してよい |
| D4 | `PERFORMANCE.md`「ホットキー表示レイテンシ（`show_main_and_emit`、SNOTRA_TRACE=1 計測、2026-03-07）」節（3338-3348 行） | **触らない**（日付付きの測定記録で、歴史として真）。任意の測定を行ったら、別の日付付き節として足す（機体名つき） |
| D5 | `scripts/manual-smoke.ps1` 項目 2 の `inv = "#671 PR A′（main_visible=false は results.hide() の前）・#646 PR2（raw 3 操作）"` | 偽になる（store が無い）。例: `"#671 PR A′（main の hide は results の hide と同じイベントループ上の 1 呼び出しで済む・連言①は main の実状態）・#646 PR2（raw 3 操作）"`。expect / trace は変えない |
| D6 | 新規 `docs/adr/ADR-<slug>.md`（例: `ADR-main-visibility-live-read`） | 否定の知識を残す（下記 3. と 4.）。内容: (a) 決定＝キャッシュを撤去し、イベントループ上で `tauri::Window::is_visible()` を読む、読み口は証人を要求する。(b) 覆した決定＝2026-08-02 設計書 §9.3 の却下。2 つの却下理由がどちらも #881 で失効した（別スレッドからの往復→読みはイベントループ上でインライン／「生 `IsWindowVisible` は tao の hide 着弾まで stale TRUE」→イベントループ上の hide は同期で `SW_HIDE` まで効く）。(c) 却下した代替: キャッシュを残す（写しを保つ規範だけが残り、Manager 経由の hide で次の hotkey が空振りする）／生 `IsWindowVisible` を直接呼ぶ（`.claude/rules/src-tauri.md` の「Win32 は PlatformBridge 経由」に当たり、従えば往復が生じる。tauri の getter で足りる）／読み口に証人を要求しない（別スレッドから呼ぶと往復に戻るうえ、着弾前の hide を読む）。(d) 「旧 ADR との関係（凍結ゆえ直さず、ここへ書く）」節: `ADR-results-gate-single-point` の「事前ゲート — `present_results` が `main_visible` を読んで」の入力の出所がフラグから実状態に変わったこと、`ADR-appstate-config-visibility` 案 3 の `main_visible` についての記述（既に #1364 で `AppState` から外れ、本変更で消える）が古くなったこと。先例は `ADR-appstate-config-visibility` の同名節。(e) 残余: Manager 経由の `.hide()` で results が取り残される件は閉じない |

## 2. 変更で偽になる散文（全件）

探し方: `git grep -n main_visible`（全件）、`git grep -n -E "5 関数|可視性を変える"`、`git grep -n -E "可視かのキャッシュ|順序不変|results\.hide\(\) の|show\(\) の後に|MainVisible|Win32 IPC|cold-call|is_visible|IsWindowVisible|事前ゲート|連言①|可視フラグ|実状態"`、`git grep -n -E "main( が\| の)?可視|main 可視|hotkey_toggle"`。いずれも `workspace/` は除外し、`docs/superpowers/` と `docs/adr/` は 3. で別に扱った。

| # | 場所（ファイル : シンボル/行） | いまの文 | 偽になる理由と直し方 |
|---|---|---|---|
| W1 | `window_coordinator.rs` : `MainVisible` の doc 全体（57-68 行） | 「main ウィンドウが可視かのキャッシュ…#156…撤去の検討は #1365」ほか | 型ごと削除（C1） |
| W2 | `window_coordinator.rs` : `show_egui_main` 本体のコメント（422-423 行） | 「main_visible は show() の後に立てる（「順序不変」制約）。show 完了前に visible=true を読んだホットキートグルが hide するのを避ける。」 | 削除（C4） |
| W3 | `window_coordinator.rs` : `hide_egui_main` の doc（501 行） | 「位置保存・main_visible=false・working set trim はここにしか無い」 | `main_visible=false` を外す |
| W4 | `window_coordinator.rs` : `hide_egui_main` の doc（508-514 行） | 「そのとき `EguiShellState.main_visible` は更新されず、results が既に可視なら最前面に取り残される…」 | 「フラグが更新されず」の部分が偽になる。取り残しの残余（hidden の間はフレームが来ない）は真のまま。直し方: 「フラグが更新されず」を外し、結果として残るのは取り残しだけであること（次の hotkey は実状態を読むので空振りしない）を書く |
| W5 | `window_coordinator.rs` : `hide_egui_main` の doc（520-522 行） | 「臨界区間（`main_visible` の store と 2 枚の `ShowWindow` が不可分であるべき区間）」 | store が無い。「2 枚の `ShowWindow`（main の `window.hide()` と `results.hide()`）が不可分であるべき区間」へ |
| W6 | `window_coordinator.rs` : `hide_egui_main` 本体のコメント（546-552 行） | 「main_visible は **results.hide() より前**に落とす（#671）…show 側の「show() の後に true を立てる」（順序不変制約）との対称…」 | 削除（C5）。必要なら「main の `window.hide()` は同期で効くので、この時点で連言①の読みは false」の 1 行に置き換える |
| W7 | `window_coordinator.rs` : `read_main_visible` の doc（1367-1370 行） | 「[`MainVisible::get`] の live-read。…`EguiShellState` 不在は false」 | 関数ごと置き換え（C3） |
| W8 | `window_coordinator.rs` : `drive_results_window` 本体のコメント（1429-1436 行） | 「main_visible は hide_egui_main が results.hide() の**前**に false へ落とすため、…**ただし `main_visible` を更新しない main の hide はこの閉じ込めの外にある**——`MainVisible` の doc を見よ。」 | 書き直し（C6）。`MainVisible` の doc は消える＝読者を存在しないものへ送る |
| W9 | `window_coordinator.rs` : `hide_egui_main` 本体 548 行と `drive_results_window` 本体 1433 行の「証人型（`EventLoopProof`）を引数に要求する 5 関数は…」 | C3 で読み口に証人を要求すると、証人を要求する関数が 6 つになり、**数え上げが偽になる**。W6 で 548 行は消えるが、1433 行は残る。直し方: 数をやめ「可視性を**変える**関数は証人型によりイベントループへ閉じている」と述語で書く（正本は `window_coordinator.rs` の `//!`） |
| L1 | `layout.rs` : `ResultsInputs.main_visible` の doc（396 行） | 「連言①: main が可視か（`EguiShellState.main_visible`）。」 | 出所が偽になる。「（`window_coordinator::main_is_visible` が読む main の実状態）」へ。**フィールド自体は残す**（issue） |
| L2 | `layout.rs` : `present_results` の doc（436-438 行） | 「証人型（`EventLoopProof`）を引数に要求する 5 関数はイベントループスレッドへ閉じており…5 関数の外側に何が残るかもそこ。」 | W9 と同じ数え上げ。「可視性を変える関数」へ |
| R1 | `results_window.rs` : `ResultsWindow.visible` の doc（49-50 行） | 「`Ordering` は同居する `main_visible` / `hotkey_generation` に合わせ `SeqCst`。」 | `main_visible` が同居しなくなる。「同居する `hotkey_generation` に合わせ」へ |
| R2 | `results_window.rs` : `raw_show`/`show` 付近の doc（124 行）とコメント（133 行）の「証人型を引数に要求する 5 関数は」 | W9 と同じ。述語へ |
| R3 | `results_window.rs` : `//!`（17-19 行）「塞ぐのは show 述語側のゲート（`layout::present_results` の連言① `main_visible`）である」 | **真のまま**（フィールド名として残る）。⚠️ 直さなくてよいが、W 系を直した後に読み直す |
| R4 | `results_window.rs` : `//!`（23 行）「可視性を変える 5 関数は…」 | **真のまま**（「可視性を変える」で絞っているので 5 のまま）。ただし数え上げなので、読み口を足す今回に合わせて述語化してもよい |
| M1 | `mod.rs` : `EguiShellState` の struct doc（140-141 行）「- main_visible: main ウィンドウが可視かのキャッシュ。…」 | 削除（C7） |
| M2 | `main.rs` : setup のコメント（313-317 行）「reset_pending は `show_egui_main` が、main_visible は show と hide が、pending_hotkey_failure は…立てる——setter はすべてこの manage より後にしか動かない。」 | `main_visible は show と hide が、` を外す。#1364 PR 本文の残余（create 中の single-instance で show が manage より前に走ると書き込みが落ちる）は、本変更で消える |
| M3 | `main.rs` : `setup_hotkey_listener` のコメント（438-447 行） | 「世代の採番・可視の読み・分岐・副作用がひとまとまりで逐次化される」は**真のまま**（読みがイベントループ上にあることが一層の理由になる）。`共有 EguiShellState.hotkey_generation を使い…` も真。直すのはコード（C8）だけ |
| H1 | `launcher_controller/hide_request.rs` : `//!`（4 行）「可視性を変える 5 関数は」 | 真のまま（述語が「変える」なので） |
| S1 | `SPEC.md` 555 / 596 行 | D1 |
| P1 | `PERFORMANCE.md:17` | D3 |
| P2 | `scripts/manual-smoke.ps1:91` | D5 |

偽にならないことを確かめた主な出現（直さない）:
- `lifecycle.rs` `plan_hotkey(visible, …)` の doc とテスト — 引数名 `visible` は出所を言っていない
- `layout.rs` 430 行「`main_visible` を条件に含めるのが要石」、948 行・1016 行のテスト doc — フィールド名として真
- `commands/window.rs` 146-157 行（topmost 復帰は `show_egui_main` / `hide_egui_main` と逐次化）— 真
- `view.rs` 591 行「可視フラグはここに無い——`ResultsWindow` が所有し」— results のフラグの話で真
- `snotra-egui-runtime/src/proof.rs` の `//!`「この型は「どのスレッドで走っているか」を型に持ち上げるためだけに在る。ウィンドウの可視性を変える操作（…）は…」— 真のまま。⚠️ 読み口にも証人を使うので、用途に「可視性の読み（別スレッドからの往復を防ぐ）」を 1 文足すかは判断が要る。`snotra-egui-runtime/CLAUDE.md` の `proof.rs` 行も同様に真のまま
- `docs/architecture.md` 79/83/231 行 — キャッシュに触れていない
- `src-tauri/CLAUDE.md` — `main_visible` もキャッシュも出てこない
- `scripts/lib/SnotraTraceInvariants.psm1` の H1 — trace の `egui_show:done` / `egui_hide:done` で区間を切るので、キャッシュに依存しない
- `ADR-results-gate-single-point`「帰結」が指す `src-tauri/CLAUDE.md`「可視性は『誰が撃つか』だけでは閉じない」は**既に存在しない見出し**（凍結 ADR の既存の古さで、本変更とは無関係。直さない）

## 3. 触ってはならないもの

| 対象 | 根拠 |
|---|---|
| `docs/adr/ADR-results-gate-single-point.md`・`docs/adr/ADR-appstate-config-visibility.md`（issue が「同期する文書」に挙げた 2 本） | `ADR-adr-frozen-history`「ADR 本文は決定日時点の世界の記述として凍結」、`.claude/rules/governance-docs.md`「ADR は凍結された歴史であり、古くなるままにする」。先例は `ADR-appstate-config-visibility` 自身の「旧 ADR との関係（凍結ゆえ直さず、ここへ書く）」。**issue の「ADR 2 本を同期」は規範と衝突するので、新 ADR の中で古くなった点を書く形へ読み替える**（D6）。⚠️ issue 起票者の意図と食い違うので、計画でユーザーに明示する |
| `docs/superpowers/specs/2026-08-02-results-visibility-single-thread-design.md`（§9.3 を含む全体） | 同設計書 §10 冒頭「本節より上は設計時点の記録であり、書き換えない」。`scripts/prose-sweep.mjs` も `docs/(adr\|superpowers)/` を「凍結された歴史」に分類する。§9.3 の覆しは新 ADR に書く。⚠️ §10（as-built 追記）に「§9.3 は #1365 で覆った」と足すかは判断が分かれる。§10 は「その実装サイクル」の追記で、後続サイクルの記録先ではないと読み、足さない方を推す |
| `docs/superpowers/specs/2026-07-22-su2-window-shell-design.md`・`2026-07-25-egui-window-ownership-and-event-delivery-design.md`・`docs/superpowers/plans/*`（`main_visible` が 15 か所ほど出る） | 同上（日付付き設計書・計画は歴史記録） |
| `PERFORMANCE.md`「ホットキー表示レイテンシ（…2026-03-07）」節 | 日付付きの測定記録。歴史として真 |
| `layout::ResultsInputs.main_visible` フィールドと `present_results` の判定式・`layout.rs` のテスト | issue「純粋核の入力なので残す」。純粋核は入力の出所を知らない |
| `lifecycle::plan_hotkey` のシグネチャ | 引数 `visible: bool` は出所に依らない純粋核 |
| 生の `IsWindowVisible` を src-tauri から呼ぶこと | issue、`.claude/rules/src-tauri.md`「Win32 を呼ぶ経路を新しく作るときは、PlatformBridge を通すことを既定にする」（従えば往復が生じる）。tauri の `is_visible()` で足りる |
| `snotra-egui-runtime` の `runtime.visible` | issue「調査の記録」: 可視性の写しではなく、常に true のゲート |

## 4. 不変条件の保ち方と検証手段

### 不変条件 A: results は main より先に出ない（show 側）

- 旧: `show_egui_main` が `window.show()` の**後**に `set(true)` → それまで連言①は false。
- 新: 連言①は `IsWindowVisible(main)`。`window.show()` はイベントループ上で同期に `ShowWindow` まで効く（tao `execute_in_thread` インライン → `apply_diff`）。results を出すのは `drive_results_window`（main のフレームの末尾）だけで、main のフレームは main が可視になった後にしか来ない。`drive_results_window` と `show_egui_main` は同じスレッドで逐次化される（tao runner は非再入。`proof.rs` の `//!`）。ゆえに「main がまだ不可視なのに連言①が true」は構築できない。**順序の規範（show の後に立てる）が、実状態の読みに吸収されて消える**。
- 再入の確認（tao 0.37.2 の `platform_impl/windows/event_loop/runner.rs`）: `call_event_handler` は `event_handler.take()` してから呼ぶ（242-257 行。再入すると `expect` で panic する）。`send_event` は、ハンドラの実行中なら `RedrawRequested` 以外のイベントを `event_buffer` へ回す（208-228 行）。`RedrawRequested` だけはバッファされずに直接呼ばれるが、それが来るのは `WM_PAINT` からで、`ShowWindow` は `WM_PAINT` を同期に送らない（Win32 の一般則。実測はしていない）。そもそも本変更は `ShowWindow` を呼ぶ位置を変えないので、この性質への依存は新しく生じない。旧でも新でも、`show()` の途中で連言①を読むフレームは来ない。

### 不変条件 B: main の hide 後に results が残らない（hide 側）

- 旧: `window.hide()` → `set(false)` → `results.hide(el)`。フラグを results.hide より前に落とすのは「間のフレームが素通りしないため」だったが、証人型（#880）以降は間にフレームが入る並びが構築できない（コード内コメント自身がそう書き、「対称のため」に順序を保っていた）。
- 新: `window.hide()`（同期で `SW_HIDE`）→ `results.hide(el)`。連言①の実状態は `window.hide()` が返った時点で false。後続の（同フレーム内も含む）`drive_results_window` は Hidden を返す。とくに hide 要求の経路は view の `update()` から emit → listener → `on_event_loop` がインライン実行（`mod.rs` `register_hide_listener` のコメント）なので、**同じフレームの末尾で `drive_results_window` が走る**。旧はフラグ false で Hidden、新は実状態 false で Hidden。結果は同じ。
- hide 側の results の hide 自体（`hide_egui_main` の `results.hide(el)`）は変えない。これが B の主担保で、連言①は「hidden の間に走る稀なフレーム（wake）で results を出さない」担保。
- 改善される点: Manager 経由で main を `.hide()` した後の次の hotkey は、旧ではフラグ true を読んで HideNow（空振り）、新では実状態 false を読んで ShowNow。#1364 の残余（setup 中の single-instance で show が manage より前に走ると true が落ちる）も消える。
- 閉じない残余（issue「残るもの」）: Manager 経由の `.hide()` で results が最前面に残る（main が hidden の間はフレームが来ない）。W4 の書き直しで残す。

### 不変条件 C: hotkey のトグルが壊れない（連打で stale を読まない）

- 読みは `on_event_loop` タスク内で、世代の採番・分岐・副作用と同じ 1 タスクに入る（`main.rs` のコメントのまま）。`main_is_visible` に証人を要求するので、タスクの外（platform スレッド）へ読みを出す変更はコンパイルが通らない。これが issue の「型で防ぐ」の中身。ただし証人は「別スレッドから呼ぶ」ことしか検出しない（`window_coordinator.rs` `//!` の既存の断り）。

### 検証手段

| 何を見るか | 手段 | 備考 |
|---|---|---|
| hotkey トグル（表示 → 非表示 → 表示）が実状態の読みで正しく分岐する | `npm run smoke:egui` の held-key シナリオ（`scripts/smoke-egui.ps1` の #927 / #1274 の節、手順 1・3・4: hotkey で show → hotkey で hide（`egui_hide:done` を増分で待つ）→ hotkey で show） | 読みが常に false なら手順 3 で hide が来ず失敗、常に true なら手順 1 で show が来ず失敗。**両向きの退行を検出できる**。実行条件は `$failures.Count -eq 0 -and $resultsChecked`（633 行）で、`$resultsChecked` は seed 済みプロファイルにより「常に走る」（439-441 行のコメント・#804）。つまり、それより前の段がすべて緑なら既定で走る。前段が赤なら走らないが、そのときはどのみち赤である。hotkey によるトグルの hide を既定の経路で踏むのは、このシナリオだけである（シナリオ 2 の hide は Escape）。前提: `hotkey_toggle` の既定は true（`snotra-core/src/config/schema.rs` `default_hotkey_toggle`。⚠️ seed プロファイルがこれを上書きしていないことは確かめていない） |
| Escape による hide 後の再表示 | `smoke:egui`（556 行付近の Escape → `egui_hide:done`、シナリオ 2 の round 間の Escape → 次の hotkey で `egui_show:done`） | シナリオ 2 は increment で待つので、hide が効かず可視のままトグルした場合を拾う（同ファイル 876-880 行のコメント） |
| hide 後に results が残らない（B） | `smoke:egui` の H1 判定（`scripts/lib/SnotraTraceInvariants.psm1`）＋ `scripts/manual-smoke.ps1` 項目 2（目視） | H1 は「hidden 区間に `egui_results:show` が出たら FAIL」。正常時は SKIP（smoke-egui.ps1 585 行のコメント）。**H1 は区間マーカーを trace から取るのでキャッシュに依存しない**——撤去の前後で同じ物差しになる |
| results が main より先に出ない（A） | trace の順序（`egui_show:done` → `egui_results:show`）を manual-smoke 項目 1 の trace で見る | `egui_show:done` は `show_egui_main` の最後に出るので、再 show で「次の show:done より前に results:show」が出れば H1 の区間（hide:done から次の show:done まで）に入って FAIL になる。**下限の主張: H1 は再 show の A だけを兼ねる**（下の訂正行）。初回 show の A を自動で落とす検査は見つけていない |
| （訂正）H1 の射程 | `SnotraTraceInvariants.psm1` 319-345 行 | H1 は `egui_show:done` / `egui_hide:done` を一度も見ていない間（`$mainState = 'unknown'`）の `egui_results:show` を判定不能（SKIP）にする。**A を兼ねるのは「一度 hide した後の再 show」だけ**で、初回 show の前に results が出る並びは判定しない |
| 連言①の読みそのもの（hidden 中の wake フレームで results を出さない） | ⚠️ **自動検査なし**。`layout.rs` の真理値表テストは純粋核だけを見る。`main_is_visible` を常に true にする変異を入れても、hidden 中に wake フレームが走らない限り smoke は緑のはず。提案: 変異（常に true）を一度入れ、smoke が緑のままなことを確かめて「検出器なし・受容」と PR に書くか、manual-smoke に「hidden 中に config.toml を保存して wake させ、results が出ないことを見る」項目を足すか（manual-smoke の方針「回帰したときだけ足す」に照らすと、足さない方が規範に合う） |
| single-instance・初回ホットキー失敗時の表示 | カテゴリ D（手動）。`smoke:startup` は初回登録まで | 読みは関与しない（show 経路だけ）が、issue の受け入れ条件に列挙されている |
| ビルド・型 | カテゴリ A（`cargo build` / `clippy -D warnings` / `cargo test` / `cargo doc`）。`MainVisible` 撤去で compile-fail が移行漏れを挙げる。doc を触るので `cargo doc` を手で走らせる（`.claude/rules/comments.md`） | |
| 文書 | `npm run governance:check`（新 ADR・SPEC・PERFORMANCE・ADR 短縮引用の照合） | |
| 検証カテゴリ（`docs/build-commands.md`「変更後の検証チェックリスト」を読んで割り当てた） | A（`.rs`）、C（ホットキーと表示経路。`cargo build -p snotra` / `npm test` / `npm run test:powershell` / `smoke:startup` / `smoke:egui`）、E（`scripts/manual-smoke.ps1` を変える＝`scripts/**` → `npm test`）、F（`SPEC.md`・`PERFORMANCE.md`・新 ADR → `governance:check`） | **D は該当しない**と読む。D の発火条件は「変えたのが表示の見た目か」で、本変更は見た目を変えない（同節の #1106 の教訓）。issue の受け入れ条件は「カテゴリ C/D」と書くが、D の目視ではなく manual-smoke 項目 2（人間への申し送り）が受け入れ条件の実体である。⚠️ issue の文言と食い違う |
| 適用する条件別チェック | AGENTS.md「重複した読み・冗長に見える状態を束ねる/消す」行（下の 4.1）、`/race-check`（フレーム内 live-read の変更）、`/symmetric-check`（show/hide の対の書き手を消す）、`/state-check`（hotkey ガードの入力の出所）、LSP findReferences（`MainVisible` / `read_main_visible` / `main_visible`）、`/dry-check`（新関数）、AGENTS.md「`Option` / フラグ … の値の出所を変更」行（下流の初めて走る行: plan_hotkey の HideNow アーム、present_results の Hidden アーム） | |

### 4.1 旧フラグと実状態が食い違う区間（AGENTS.md「重複した読み・冗長に見える状態を束ねる/消す」）

旧フラグと実状態は、少なくとも次の区間で食い違う。区間ごとに、その差に依存する読み手があるかを書く。

| 区間 | 旧フラグ | 実状態 | その区間で読む者 | 依存 |
|---|---|---|---|---|
| `show_egui_main` の `window.show()` から `set(true)` まで | false | true | 無い（同じ関数の中で連続している。間に呼ぶのは `app.try_state` だけ） | 無し |
| `hide_egui_main` の `window.hide()` から `set(false)` まで | true | false | 無い（同上） | 無し |
| `set(false)` の後、`results.hide(el)` まで | false | false | — | 一致 |
| `EguiShellState` を manage する前（setup 中） | 読みは false（state が無い） | main は `.visible(false)` で生成（`mod.rs:365`）なので false | — | 一致。ただし #1364 の残余（manage 前に single-instance の show が走ると、旧は書き込みが落ちて false のまま・実状態は true）は新で消える |
| `Manager` 経由で main を `.hide()` / `.show()` した後 | 更新されない | 変わる | 次の hotkey・次の `drive_results_window` | **旧の欠陥**（issue「問題」）。新では食い違わない。現状そう書いているコードは無い（`git grep -E "\.(hide\|show)\(\)" -- src-tauri/src` で、main への呼び出しは `window_coordinator.rs` の 421 行（show）と 544 行（hide）だけ。`results_window.rs` の 355 / 370 行は非 windows ビルドの results の `raw_show` / `raw_hide`） |
| main が最小化されている間（Win+D など） | true | `IsWindowVisible` も true（WS_VISIBLE は残る） | — | 一致（挙動不変） |

「後で読まれる/立つことに依存」の観点: 2026-08-02 設計書 §9.3 は「ゲートが要るのは意図であってラグ付きの実状態ではない」と書いた。意図と実状態がずれるのは、hide が別スレッドから post されて着弾を待つ間だけである。本変更の後は、shell の hide も show もイベントループ上で同期に `ShowWindow` まで効くので、イベントループ上の読みにとって意図と実状態の差は無い。

### 4.2 ADR を直すかの規範上の根拠（反対の先例との比較）

- 反対の先例: `ADR-window-coordinator-split-rule` 却下 6 の「この理由は #880 サイクル段 2 で失効した」は、決定の後に本文へ足した追記である。追記したのは 4c5892a2（#881、2026-08-03 13:36）
- `ADR-adr-frozen-history` を入れたのは 53ad731d（#896、2026-08-03 21:44）。`git merge-base --is-ancestor` で #881 → #896 の順を確かめた。**追記は凍結の規範より前**である
- 凍結の後の先例は `ADR-appstate-config-visibility`（c0c51878、#1131、2026-08-18）。古い ADR は直さず、新 ADR の「旧 ADR との関係（凍結ゆえ直さず、ここへ書く）」節で受けている
- ゆえに現行の規範は「直さない」。issue の「ADR 2 本を同期」は、新 ADR の中で受ける形に読み替える

## 5. 確信の持てない判断（⚠️）

1. ⚠️ ADR 2 本の扱い: issue は「同期する」と書くが、規範（ADR 凍結）では直さない。新 ADR の「旧 ADR との関係」節で受ける（根拠と反対の先例との比較は 4.2）。issue の文言とは食い違うので、ユーザーへ明示する。
2. ⚠️ 2026-08-02 設計書 §10 に追記するか: 足さない方を推す（§10 はそのサイクルの as-built、§9.3 の覆しは新 ADR が正本）。
3. ⚠️ SPEC の `main_visible`（555/596 行）: 語彙層へ直す（`main 可視`）のを推すが、挙動不変なので「直さない」も成り立つ。issue は同期対象に挙げている。
4. ⚠️ `proof.rs` の `//!` に「読みにも使う」ことを足すか。現行文は「ためだけに在る」の対象を「どのスレッドで走っているか」としていて偽にはならない。足すなら新規記述として検算する。
5. ⚠️ 読み口の名前と置き場（`main_is_visible` を `window_coordinator.rs` に置き `mod.rs` から re-export）。`//!`「撃つ主体を集めた場所であって判定は持たない」に照らすと、読みを置くのは既存の `read_*` 群（`read_indexing` / `read_visible_rows` など）と同じ扱いで整合すると判断した。
6. ⚠️ フレーム内（`drive_results_window`）で毎フレーム `is_visible()` を呼ぶ費用: `send_user_message` インライン + mpsc channel 1 本の確保 + `IsWindowVisible`。results が Visible のフレームでは、同じ経路の getter が既に 3 本呼ばれている（`position_results_below_main`・0. 参照）。足すのは 1 本で、Hidden のフレームでも走る。**未測定**（issue も未測定と書いている）。
7. （解消）再入: tao runner を読んで確かめた（4. の A）。残るのは「`ShowWindow` は `WM_PAINT` を同期に送らない」という Win32 の一般則だけで、本変更は `ShowWindow` の位置を変えないので、この前提への依存は新しく生じない。
8. （解消）held-key シナリオは既定で走る（4. の検証表）。残る ⚠️ は、seed プロファイルが `hotkey_toggle` を上書きしていないかを見ていないことだけ。
11. ⚠️ issue の受け入れ条件は「カテゴリ C/D」だが、`docs/build-commands.md` の D の発火条件（見た目を変えたか）には当たらないと読んだ。人間への申し送りは manual-smoke 項目 2 に絞るべき。
9. ⚠️ `is_visible()` の `Err`（ウィンドウ破棄後など）を false に倒すことは、旧の「`EguiShellState` 不在は false（results を出さない側）」と同じ向き。hotkey 側では false → ShowNow になる。旧も shell 不在で false だったので向きは同じ。
10. ⚠️ 最小化された main（Win+D）は `IsWindowVisible` が true のまま。旧キャッシュも true のまま。挙動は変わらない（2026-08-02 設計書 §9-4 の射程外の残余のまま）。
