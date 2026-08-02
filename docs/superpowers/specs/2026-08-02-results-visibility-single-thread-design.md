# results 窓の可視性を単一スレッドへ閉じる（根治）

対象の不変条件: **results 窓が可視 ⇒ main 窓が可視**（`SPEC.md`「検索結果ウィンドウの可視性（従属軸）」）。

本設計は PR #880（対症療法）を**置き換える**。#880 が足した事後検査（`layout::must_retract_results`）と hide の理由型（`layout::HideReason`）は、本設計の完了時に**削除される**。

---

## 1. 何が壊れているか

不変条件はこれまで 2 点で守られていた——hide 側の順序（`main_visible` を results の hide より先に false へ）と、show 側の述語ゲート（`layout::present_results` の連言①）。

**この 2 点は「読んだ時刻」しか守らない。** 判定と適用のあいだに Win32 呼び出しが挟まり、`hide_egui_main` は別スレッドからそこへ丸ごと割り込める。帰結は 2 つ:

1. 「読んだ時は main 可視・撃った時は main hidden」の並びが成立し、results だけが表示される
2. `ResultsWindow::show` の `swap(true)` と `raw_show()` のあいだに hide が挟まると「可視フラグ = false・窓 = 可視」が残り、以後フラグを見る hide が**黙って no-op** する

どちらも main が hidden になった後に起きる。hidden な窓へ `RedrawRequested` は配送されない（#697 実測）ため `update()` が走らず、**拾い直すフレームが来ない**。

### 1.1 実際の並び（一次証拠で精密化した）

`tauri-runtime-wry 2.11.4` の `getter!` マクロ（`src/lib.rs:195-203`）は `send_user_message` の後に**タイムアウト無しの `rx.recv()`** で待つ。一方 setter（`show` / `hide` / `set_size` / `set_position`）は**非ブロッキングの post** である。

ホットキー（Win32 メッセージループスレッド）から `hide_egui_main` を呼んだときの実際の並び:

| # | 処理 | 実体 |
|---|---|---|
| 1 | `outer_position()` | **blocking 往復①** |
| 2 | `hwnd()` | **blocking 往復②** |
| 3 | `save_search_placement` | ディスク書き込み |
| 4 | `window.hide()` | post（`SW_HIDE` は後でイベントループが撃つ） |
| 5 | `main_visible.store(false)` | 即時 |
| 6 | `results.hwnd()` | **blocking 往復③** |
| 7 | `ShowWindow(SW_HIDE)` | 生 Win32・クロススレッド |

**`main_visible.store(false)` は往復②と往復③のあいだに落ちる。** hide 側は store の直前と直後の両方でイベントループへ処理を明け渡しており、イベントループが `drive_results_window` の途中にいる確率はそのぶん高い。**競合の窓が広いのは偶然ではなく構造である。**

---

## 2. 採用する機構

**可視性を*変える*操作をイベントループスレッドへ閉じ、それを型で強制する。**

### 2.1 なぜ lock ではないのか（棄却の維持）

窓を所有しないスレッドからの `ShowWindow` は所有スレッドのメッセージポンプ待ちでブロックしうるため、イベントループ側も取る lock は race をデッドロックへ化けさせる。**さらに強い理由が §1.1 から出た**——ホットキースレッドは既にイベントループを**タイムアウト無しで待っている**（往復①②③）。ここへイベントループ側も取る lock を足せば待ちが循環する。棄却は覆さない。

**「非所有スレッドの `ShowWindow` がブロックする」の直接測定は試みて失敗した（未測定のまま残す）。** WinForms + P/Invoke の測定装置を書き、positive control（`SendMessageTimeoutW(WM_NULL)` が wedge 中に timeout を使い切る）とベースライン（オーナーがポンプ中の `ShowWindow` / `SetWindowPos`）までは通ったが、**肝心の「オーナーのポンプを止めた区間」を engage させられず** 2 回とも `wedge never engaged` で終わった。ゆえにこの命題は**依然として伝聞である**——ただし上段の「既にタイムアウト無しで待っている」は `getter!` マクロの実体から直接読めるので、**lock 棄却そのものは直接の一次証拠を持つ**。ブロックするか否かが判明しても棄却は変わらない。

### 2.2 相互排他は既にイベントループが持っている

`tao 0.35.3` の runner（`src/platform_impl/windows/event_loop/runner.rs`）:

- `call_event_handler`（:242-256）は `event_handler.take()` してから呼び、戻すときに `assert!(... .is_none())` する（**非再入**）
- `send_event`（:208-227）は `should_buffer()`（= ハンドラの中にいる）なら `event_buffer` へ push して**後回し**にする

`drive_results_window` は `RedrawRequested` ハンドラの中で走る。`run_on_main_thread` の実体である `Message::Task` は `Event::UserEvent` として届き、ハンドラ実行中はバッファへ積まれる。**両者は互いに割り込めない。lock を 1 つも導入せずに相互排他が得られる。**

### 2.3 「hidden な窓では update() が走らない」とは衝突しない

Task の受け口は tao が別に建てる 0×0 の `thread_msg_target`（`event_loop.rs:649-689`・`WS_EX_LAYERED` ゆえ不可視・イベントループの寿命と同じ）である。アプリの窓が 2 枚とも hidden でも生きている。**止まるのはフレームであってタスクではない。**

### 2.4 `run_on_main_thread` は遅延 primitive ではない

`send_user_message`（`tauri-runtime-wry-2.11.4/src/lib.rs:235-255`）はイベントループスレッドから呼ぶと**その場で同期・再入的に実行**し、別スレッドからは `PostMessageW` で post して即座に戻る。ゆえに:

- フレーム内から出る hide 要求（`EGUI_HIDE_REQUESTED`）は**今日と同じフレーム内順序を保つ**
- 別スレッドからの hide 要求は待たない

---

## 3. 設計

### 3.1 証人型 `EventLoopProof`

`snotra-egui-runtime` に `!Send` な不透明型を置き、構築点を 2 つに絞る。

```rust
/// イベントループスレッド上にいることの証人。`!Send`・フィールド private ゆえ
/// この crate の外では構築できない。
pub struct EventLoopProof { _not_send: PhantomData<*const ()> }

impl RuntimeFrame {
    /// フレームは `EguiWindow::render()` の中でしか作られず、`render()` は
    /// `Plugin::on_event` からしか呼ばれない。ゆえにこの参照の存在自体が証明である。
    pub fn event_loop(&self) -> &EventLoopProof;
}

/// フレームの外からイベントループへ入る唯一の口。
pub fn on_event_loop<F>(app: &tauri::AppHandle, f: F)
where F: FnOnce(&tauri::AppHandle, &EventLoopProof) + Send + 'static;
```

**`ResultsWindow` は `Send + Sync` のままにする**（Tauri managed state の要求）。証人はメソッド**引数**にだけ現れ、フィールドには入れない。

### 3.2 拘束するシグネチャ

```rust
pub(crate) fn hide_egui_main(app: &AppHandle, _el: &EventLoopProof)
pub(crate) fn show_egui_main(app: &AppHandle, _el: &EventLoopProof, t0: Instant)
pub(crate) fn drive_results_window(app: &AppHandle, el: &EventLoopProof, i: DriveResultsInputs)
impl ResultsWindow {
    pub(crate) fn show(&self, _el: &EventLoopProof, background: Color32) -> bool
    pub(crate) fn hide(&self, _el: &EventLoopProof) -> bool
}
```

### 3.3 判定も一緒に運ぶ（省略不可）

**効果だけを marshalling してはならない。** ホットキーの判定（`main.rs` の `plan_hotkey` が `main_visible` を読む）が producer スレッドに残ると、連打時に 2 回とも同じ stale 値を読み、**トグルが壊れる**（両方 Hide / 両方 Show になる）。

今日この問題が無いのは、判定も副作用も同じ platform スレッド上で逐次化されているからである。marshalling するなら **`hotkey_generation` の採番・`plan_hotkey`・分岐先の実行をひとまとめにタスクへ入れる**。`is_alt_pressed()` は `GetAsyncKeyState` でスレッド非依存ゆえタスク内で読める。

alt 解放待ちの spawn スレッドは、待機後に `on_event_loop` でタスクを投げる形へ変える。世代照合もタスク内で行う。

### 3.4 臨界区間の切り出し（ポンプ停止の不変条件との整合）

`src-tauri/CLAUDE.md`「ウィンドウ生成の制約」の不変条件——イベントループ中のコールバックではメッセージポンプが停止する——は marshalling した本体の全行に効く。ポンプ進行を要さない行だけを臨界区間に置く:

```
hide 要求
  ├─ [イベントループ] ← 臨界区間はここだけ
  │    placement の値を読む → window.hide() → main_visible.store(false) → results.hide()
  └─ [その後・タスク外] save_search_placement（ディスク I/O）/ trim_idle_working_set
```

`trim_idle_working_set` を外へ出せるのは、`src-tauri/CLAUDE.md`「working set の能動回収」が「trim が hide 前後どちらで走っても無害」と明言しているためである。placement は `outer_position()` の読みが hide **より前**である必要があるだけなので、読みを臨界区間の冒頭に置き、書き込みを外へ回す。

**この分割は §4 の削除可能性を弱めない**——不変条件が要求するのは `main_visible` の store と 2 枚の `ShowWindow` が不可分であることだけである。

### 3.5 副次的に消えるもの

| 消えるもの | 理由 |
|---|---|
| blocking 往復①②③ | タスク内では `send_user_message` がインライン分岐へ倒れる |
| クロススレッドの生 `ShowWindow` | results への raw 操作が所有スレッドの呼び出しになる |

**hide は速くなる。** 代価は負である。

---

## 4. PR #880 の 3 点封鎖はどうなるか

| 封鎖 | 本設計後 |
|---|---|
| ①事前ゲート（`present_results` の連言① `main_visible`） | **残す。** これは競合対策ではなく「hidden 中に走る稀なフレームで results を出さない」という状態の述語である |
| ②事後検査（`must_retract_results`） | **削除。** ゲートの読みと `raw_show` のあいだに hide が割り込めなくなり、検査対象の並びが構築できない |
| ③hide の権威性（`HideReason` / `hide_must_be_unconditional`） | **削除。** 「フラグ = false・窓 = 可視」の食い違いは非原子な並行 swap でしか生じない。単一スレッドならフラグは窓の実状態と常に一致する |

**②③を消せることが、本設計が「対症療法の置き換え」たりうる根拠である。** 消せないなら封鎖を 4 つ目に増やしただけになる。

---

## 5. 却下した案（否定の知識）

いずれも一次証拠つきで棄却した。**同じ案が再提案されたときの反証はここにある。**

### 5.1 オーナーウィンドウ（`GWLP_HWNDPARENT` / tauri `WindowBuilder::owner`）

**却下。hide は伝播しない。** Microsoft Learn "Window Features" → Window Visibility が明言する——"Hiding an owner window has no effect on the visibility state of the owned windows."

伝播の契機は **minimize と destroy だけ**であり、しかも本アプリは `minimize` / `SW_MINIMIZE` / `set_minimized` を 1 箇所も持たない（grep 0 件・main は `decorations(false)` + `skip_taskbar(true)`）。**発火しうる propagation が 1 つも無い。**

実測（WinForms + P/Invoke のハーネス・リポジトリ無改変）でも、オーナーを hidden にしたまま owned へ `SW_SHOWNOACTIVATE` を撃つと**可視になった**。

`ShowOwnedPopups` が別 API として存在すること自体が非伝播の自白である（同段落が「minimize せずに**も hide せずに**も owned を隠したい場合」と書いている）。なお `ShowOwnedPopups(FALSE)` は呼んだ瞬間に隠すだけの一撃であり、後続の show を禁止する状態ではない。

### 5.2 `WS_CHILD` 化

**機構としては唯一の正解。幾何が成立せず却下。** 親子は伝播イベントではなく**連言**である——"If the window's parent window is not visible, it will also not be visible."。実測でも、親が hidden の状態で子へ `SW_SHOWNOACTIVATE` を撃つと `WS_VISIBLE` は立つが `IsWindowVisible` は false のままだった。

これなら①②③が丸ごと落ちる。しかし:

1. **子は親のクライアント領域外に出られない。** results は main の**外形の下 + `window_gap`** に置かれ、main は show ごとに `bar_height`（既定 43px）へ collapse する。43px の中に下のリストは入らない
2. **z-order 制御が消える。** 子は親と一体で `HWND_TOPMOST` が意味を持たず、`ResultsWindow::set_topmost`（設定サイドカー存命中の一時解除）の等価物が無い
3. **新しい回帰を持ち込む。** 親の再表示で子が `WS_VISIBLE` を保ったまま自動復帰するため、reset-on-show の消費より前に**古い結果が 1 フレーム出る**

**唯一効く OS 機構は「2 ウィンドウであること」自体と非両立である。**

### 5.3 単一 atomic 状態機械（`Hidden` / `MainOnly` / `MainAndResults` + CAS）

**却下。原子化する相手が違う。** 真実源は 2 つではなく **3 つ**である——`main_visible`・`ResultsWindow.visible`・**窓の実状態**。enum が消せるのは前 2 つの食い違いだけで、3 つ目は外界にある。

CAS に成功したスレッドが `raw_show` を撃つ前に停止し、他スレッドが `Hidden` へ CAS して両窓を隠した後で遅れて撃てば、enum は `Hidden` なのに results は可視——**現在の事故そのもの**が残る。CAS 後の世代再検査を足せば、それは #880 の事後検査の改名にすぎない。

**この案には強い改良版がある**ので、再提案に備えて記録する: 遷移の請求を非 `Clone` な `ShowClaim` トークンで表し `raw_show` の引数に要求する（ゲートを経ない発砲が構築不能になる）、`commit` の戻りを `#[must_use]` な `Result<(), MustRetract>` にする（事後検査の「忘れ」が `-D warnings` で compile-fail になる）というものである。**多スレッドを前提に置いたままなら、これが到達できる最良である**——事後検査は消えないが、規範から型の義務へ変わり、正しさの論証も「SeqCst の全順序で store より前に…」という 3 ファイルに散る順序の話から「自分の CAS が負けたか」という局所の事実へ縮む。

**本設計がこれを採らないのは、単一スレッド化が事後検査そのものを不要にするからである**（§4 の②）。書き手が 1 スレッドに揃えば「請求と着弾のあいだに状態が変わる」並びが構築できず、守るべき義務が残らない。**多スレッドを維持する判断へ戻すなら、この改良版が第一候補である。**

### 5.4 レイヤード alpha 0 / `BeginDeferWindowPos`

**却下。race は消えない。** 撃つスレッドも順序も変わらず、判定と適用の隔たりは 1 ビットも縮まない。alpha 0 は加えて、results が可視のまま描かれ続けるため hidden 時の `EmptyWorkingSet` 回収（#532 SU6.5）の前提を崩す。

`BeginDeferWindowPos` は Win32 における数少ない本物の原子性だが、hide 側 2 操作を原子化しても**その後に**撃たれる show は防げない。

### 5.5 `run_on_main_thread` で包むだけ（型の拘束なし）

**単独では却下。** 挙動は正しくなるが、新しい呼び出し点を書く人が包み忘れれば同じ事故が戻り、**戻ったことを検知する手段が無い**。段階分割の第 1 段としてのみ有用で、「これで完了」としてはならない。

---

## 6. 段階

1. **証人型の導入**（`snotra-egui-runtime`）。`EventLoopProof` / `RuntimeFrame::event_loop` / `on_event_loop`
2. **可視性 API の拘束**。`hide_egui_main` / `show_egui_main` / `drive_results_window` / `ResultsWindow::show` / `hide` に `&EventLoopProof` を要求し、呼び出し点を移行する。**新 API の導入と呼び出し点の移行は 1 段に束ねる**（`AGENTS.md`「条件別チェック」・`-D warnings` 下で未使用の新 API は `dead_code` で落ちる）
3. **ホットキー判定の marshalling**。`plan_hotkey` と世代採番をタスク内へ。alt 解放待ちも同様
4. **#880 の②③削除**。`must_retract_results` / `HideReason` / `hide_must_be_unconditional` と、それらのテストを消す
5. **文書の改訂**。`SPEC.md` / `src-tauri/CLAUDE.md` / 各 doc（§8 参照）

---

## 7. 検証

- カテゴリ A（`docs/build-commands.md`）— 段ごとに実行
- カテゴリ C（`smoke:egui`）— 窓の表示順序を変えるため必須。検出器は `scripts/lib/SnotraTraceInvariants.psm1` の H1
- カテゴリ D（実機目視）— **ホットキー連打でトグルが壊れていないこと**を見る。§3.3 の危険は `cargo test` でも smoke でも落ちない
- カテゴリ F（`governance:check`）

**smoke と unit test は race が閉じた証拠にはならない。** 本設計の主張は「並びが構築できない」であって「並びを踏まなかった」ではない。証拠は型と tao の非再入性であり、テストが測るのは回帰の不在である。

---

## 8. 影響を受ける文書

事故の機序を記録した散文が長く、そのほとんどが偽になる。**作業量の実体はコードよりここにある。**

- `SPEC.md`「検索結果ウィンドウの可視性（従属軸）」— 3 点封鎖の記述を差し替える
- `src-tauri/CLAUDE.md`「Win32 / Tauri 注意事項」— 可視性の 3 点、lock 棄却、presence 検査の項
- `layout.rs` の `present_results` / `results_window.rs` の `//!` と `show` / `window_coordinator.rs` の `hide_egui_main`
- `snotra-egui-runtime/CLAUDE.md` — 証人型の責務

---

## 9. 受容する残余（全称で書かないこと）

1. **`Manager` から results の生ハンドルを引いて `.hide()` を呼ぶ書き方は依然コンパイルが通る。** `src-tauri/CLAUDE.md` が既に留保している地点であり、本設計でも塞げない
2. **「results への生 Win32 はすべて所有スレッドで走る」は偽である。** 設定サイドカー監視スレッドの `set_topmost`（`SetWindowPos` + `hwnd()` の blocking 往復）が残る。書けるのは「可視性を**変える**操作はすべて」まで。z-order は不変条件に関与しないため意図的にスコープ外とする（別タスクで寄せれば往復が 1 つ消える）
3. **`main_visible: AtomicBool` は消えない。** ホットキー listener が `plan_hotkey` のために読む経路が残る（§3.3 でタスク内へ移すが、`AppState` の型としては atomic のまま）。「並行性を消した」と全称で書いてはならない——**「可視性を*変える*操作は単一スレッドに閉じた。読みは他スレッドに残る」**が正しい

   **キャッシュを廃止して Win32 の実状態を真実源にする案は採らない。** 理由は 2 つで、どちらも本設計とは独立に成り立つ:
   - tauri の `is_visible()` を別スレッドから呼ぶのは §1.1 の blocking 往復と同じ危険クラスである
   - 生の `IsWindowVisible` は tao の hide が着弾するまで stale な TRUE を返す。**ゲートが要るのは「意図」であってラグ付きの実状態ではない**

   なお `state.rs` の「Win32 `is_visible()` の ~35ms レイテンシを回避するキャッシュ」という記述の出所は `PERFORMANCE.md`「ホットキー表示レイテンシ」の 2026-03-07 計測（最適化前 cold 191ms の内訳に `is_visible()` pre-check 61ms + `is_visible_after_show` 39ms）である。**測ったのは WebView2 期の値であり、WebView2 は #532 SU7 で撤去済み**——現行の egui 窓での同コストは**未検証**である。数値を根拠に判断を積み増さないこと。
4. **OS 由来の最小化（Win+D / Show Desktop）** は本設計の射程外。main が最小化され results が残りうる。tauri が何を emit するかの一次証拠が無く、**列挙のみ・未検証**

---

## 10. as-built（実装後の追記）

**本節より上は設計時点の記録であり、書き換えない。** 日付付き設計書は歴史記録であって規範の正本ではない（同じ扱いを別の設計書に与えている例は `snotra-egui-runtime/CLAUDE.md`「不変条件」）。実装が確定させた事実と、上の記述とのずれだけをここに足す。

### 10.1 段の進捗

§6 の 5 段のうち **1〜4 が入った**。段 2 と段 3 は同一コミットで束ねた（`-D warnings` 下で未使用の新 API が `dead_code` で落ちるため・§6 段 2 の但し書き）。段 5（文書）は本節を含めて完了。

| 段 | コミット |
|---|---|
| 1 証人型の導入 | `dae6195` |
| 2 可視性 API の拘束 + 3 ホットキー判定の marshalling | `b5ccff2` |
| 4 事後検査・`HideReason` の削除 | `976a8ae` |
| 5 文書 | `e627c77` / `94d3c06` / `a486ac6` / `abe37b9` / `2ce65a4` と本節 |

### 10.2 §7 の検証のうち、走っていないもの

- カテゴリ A と F は各段で実行した
- **カテゴリ C（`smoke:egui`）とカテゴリ D（実機目視）は 1 度も走っていない。** 実機への打鍵注入がこの環境で届かず、後追いに回した（判断はリード）。ゆえに §7 が名指しした「**ホットキー連打でトグルが壊れていないこと**」は**未確認**のままである——§3.3 の危険は `cargo test` でも `governance:check` でも落ちない

### 10.3 §8 の一覧は不完全だった

as-built へ直した文書は §8 の 4 点に加えて次がある。

- `docs/adr/ADR-window-coordinator-split-rule.md` の却下 6（`reset_size_guard` を `show_egui_main` へ移す）—— 却下理由が「`show_egui_main` は別スレッドから走りうる」に依っており、**理由だけが失効した**（結論は変わらない）。否定の知識は結論と理由の対で意味を持つので、理由の失効は追記が要る
- `src-tauri/CLAUDE.md` の `SendInput` の項、`src-tauri/src/main.rs` の `send_alt_key_up`、`src-tauri/src/platform/mod.rs` —— いずれも 10.4 の同期バリア撤去の波及

### 10.4 設計時に予見していなかった撤去: フォーカス同期バリア

§4 の表は #880 の②③だけを挙げるが、**`show_egui_main` の `SendMessageTimeoutW(hwnd, WM_NULL, …)` も同じサイクルで撤去した**。イベントループへ移った結果、宛先窓が呼び出しスレッド自身の所有になり、`SendMessage` 系は窓プロシージャを直接サブルーチンとして呼んで即座に戻る——キューを 1 通も排出せず、**恒久的に no-op** になったためである。

**同じ型の失効がもう 1 件あり、そちらは撤去していない**: `send_alt_key_up` の 5ms スリープ。根拠（「窓がキー up を処理する時間を作る」）は同じ理由で失効するが、`set_focus()` はフォアグラウンド移行を同期しないため「まだ旧窓が前景なら合成キー up はそちらのスレッドで処理されうる」という前提が残る。**測ってからでないと削除の可否を言えない**ため次段へ送った（判断材料と申し送りの正本は `main.rs` の当該コメント）。

### 10.5 §3.4 の臨界区間切り出しは実装したが、範囲は設計より狭い

`hide_egui_main` は placement の**読み**だけを臨界区間に残し、**書き込み**と `trim_idle_working_set` を末尾へ出した——ここまでは §3.4 のとおりである。ただし **「臨界区間の外」は「イベントループの外」ではない**: 末尾の 2 つも同じ関数の中、すなわちイベントループスレッド上で走り、その間メッセージポンプは止まる（本変更以前は platform スレッド上でありループを塞がなかった）。ポンプ進行を要する操作ではないのでデッドロックはせず、窓を隠した**後**なので視覚的なジャンクにもならない。**受容する残余**であり、別スレッドへ出すかは後段の判断に残す。

### 10.6 §3.5 の「消えるもの」は予測のままである——測っていない

「blocking 往復①②③が消える」「**hide は速くなる。代価は負である。**」は `send_user_message` の分岐（イベントループスレッドからはインライン実行）から導いた**推論であって実測ではない**。本サイクルで hide のレイテンシは 1 度も測っていない。数値を根拠に次の判断を積み増さないこと（§9-3 の 35ms への注意と同じ理由）。

### 10.7 §9 の残余は as-built でも 4 件とも成立する

とくに残余 2 —— **`ResultsWindow::set_topmost` は `commands/window.rs` のポーリングスレッドから来る**ため、「results への生 Win32 はすべて所有スレッド」は**書けない**。書けるのは「可視性を**変える**操作は」までである。この限定は `SPEC.md`「検索結果ウィンドウの可視性（従属軸）」・`src-tauri/CLAUDE.md`「Win32 / Tauri 注意事項」・`layout::present_results` の doc で守ってある。
