# SU2 設計 — ウィンドウシェル + 状態機械（#532 Phase 2）

- 種別: サブユニット設計（spec）。実装計画は本 spec 承認後に別途 writing-plans で作る
- 日付: 2026-07-22
- 親: `docs/superpowers/specs/2026-07-21-phase2-softbuffer-migration-roadmap.md`（SU2）／#532
- 前段: `docs/superpowers/specs/2026-07-21-su1-softbuffer-runtime-design.md`（SU1・実装完了）
- 履歴: 撤去済み spike（`soft_host_main.rs`・commit 7558cc8）から純粋核を復元し一次証拠化。焦点を絞った設計問答（構造の岐路 → backend seam の深さ）で硬化

## 目的

製品 `src-tauri` のメインウィンドウに、**WebView2 と並行して egui/softbuffer 経路**を env フラグ選択で立ち上げる「外殻」を作る。SU2 が持つのは `Standby ⇄ SearchVisible` の**外側の状態機械**——Alt+Q 表示/非表示・blur 自動非表示・フォーカス列・残留 Alt 解除・位置永続・起動時表示・初回フロー。内側の検索モード（Normal/Command/Tool/Folder/Instant/Indexing）と検索体験は SU3。SU2 の egui view は **placeholder**（show/hide/focus/位置が視覚的に検証できる最小の chrome）。

## 決定の要石

### 検証済み（この設計の前提・一次証拠）

1. **純粋な決定核は spike で実証済み**（`soft_host_main.rs:131-207` から復元）。以下 2 関数を `src-tauri` へ移植する。設計を反転させる不確実性は無い:
   - `plan_hotkey(visible, alt_pressed) -> HotkeyPlan{ HideNow | ShowAfterAltRelease | ShowNow }` — Alt+Q 押下時の分岐。表示中なら即 hide、非表示中は Alt 押下中なら解放待ち後 show。
   - `plan_ui_action(state: LifecycleState, cmd: &HostCommand) -> UiAction{ Show | Hide | Refocus | Defer | Ignore }` — ホストコマンドを lifecycle 状態へ適用する計画。**冪等性**（`Visible+Show→Refocus`・`Suspended+Hide→Ignore`）と、**show 進行中に届いた Hide の繰り延べ**（`Recreating+Hide→Defer`）をここで一元決定する。
   - `LifecycleState{ Visible | Suspended | Recreating | Exiting }` と `transition(state, event) -> Result<LifecycleState>`。`Recreating` は「show 済みで最初のフレーム提示待ち」。
2. **focus 観測の経路が確定**（runtime 駆動 probe `snotra-egui-mvp/src/main.rs:174`）。runtime は tao の `Focused` を egui 入力へ流し込み済みで、`ctx.input(|i| i.focused)` で **EguiView（＝製品では `src-tauri` のコード）が focus を観測できる**。→ **blur 自動非表示のために `snotra-egui-runtime` の公開 API を拡張する必要はなく、SU2 は `src-tauri` 境界に留まる**（ロードマップの「境界: src-tauri」を満たす）。runtime は `Focused(true)` を「再表示された」の代理シグナルに `visible` を復帰させる（`runtime.rs:270`）ため、show は外から `set_visible(true)+set_focus()` を打てば runtime が観測して repaint まで運ぶ。
3. **両ウィンドウは同じ `tauri::Window` 抽象を共有する。** WebView2 ウィンドウも egui ウィンドウも `.show()`/`.hide()`/`.set_focus()`/`.hwnd()`/`.set_position()` を持つ。ゆえに show/hide の Win32 レベルの骨格（表示・フォーカス・位置・残留 Alt 解除・WM_NULL 同期）は**両経路で文字通り同一**で、共通の window ハンドルに対して回せる。真に分岐するのは renderer と frontend の副作用だけ（後述 backend seam）。
4. **egui ウィンドウは webview 無しで生成できる**（probe `main.rs:740`・`snotra-egui-mvp/CLAUDE.md` 不変条件「`tauri::Window::builder` だけで生成」「`app.windows` は空」）。`msedgewebview2.exe` 子孫 0 を維持する（`src-tauri/CLAUDE.md`「WebView2 ウィンドウ生成の制約」）。

### 実装初手で確定させる検証ゲート（崩れると設計が反転する）

- **G0（宣言→programmatic 変換 byte-identical）**: `tauri.conf.json` の `windows` を空にし `build_webview2_window` で宣言窓を逐語再現した直後、`SNOTRA_EGUI_MAIN` 未設定で `smoke:startup` / `e2e:tauri` が緑（宣言時と挙動一致）。**最初に接地するゲート**——窓生成の変換が flag OFF を壊さないことを、show/hide 整合より前に確定する。
- **G1（フラグ OFF byte-identical・show/hide 整合後）**: WebView2 の `resume`/`suspend`/`emit`/`trim` を backend フックへ逐語で寄せた後、`SNOTRA_EGUI_MAIN` 未設定で `smoke:startup` / `e2e:tauri` が緑を保つ（回帰なし）。整合の代償（既存 WebView2 経路を触る）が回帰を生まないことを接地する。
- **G2（hotkey → egui show の配線）**: 製品ホットキーは Win32 `RegisterHotKey`（platform スレッド）→ `emit("hotkey-pressed")` → listener。listener は egui 経路では `get_window("main")`（`tauri::Window`）を掴む必要がある（`get_webview_window("main")` は egui ウィンドウに対し `None`）。emit→listener→egui show が実機で 1 回通ることをトレースで確認する（初手スモーク）。
- **G3（focus-lost 観測の起動）**: probe `main.rs:174` は focus を **refocus** に使うのみで自動非表示は実装していない。SU2 は focus 喪失で猶予タイマを起こす。`Focused(false)` → egui repaint → view が `!focused` を観測 → `request_repaint_after(100ms)` が発火 → 猶予明け判定、の一巡が実機で回ることを確認する。
- **G4（外部 hide と runtime.visible の両立）**: hotkey トグル hide は listener から `window.hide()`（外部）で行う。隠れたウィンドウには `RedrawRequested` が配送されないため runtime は描かない（SU1 不変条件⑥の paint ゲートと衝突しない）ことを、hide 後のアイドルで present 失敗リトライstorm が起きないことで確認する。

## backend seam（整合の実体・薄い 2〜3 フック）

**共有オーケストレーションを 1 本に保ち、renderer + frontend 固有の副作用だけをフックへ逃がす。** backend は enum 2 値（`MainBackend::{ WebView2, Egui }`）。フラグが選ぶのは「生成時にどちらの window+backend を作るか」の一点。SU7 は `WebView2` variant とそのフック中身を delete するだけで egui が素で残る（trait を作り込まない）。

```
fn show_main(app, backend, t0):              // ← 両経路が通る唯一の show 経路
    main_visible = true                      // 共有（policy フラグ・後述「2 つの visible」）
    backend.pre_show(app)                     //  WV2: resume_webview   / Egui: nop
    position_on_target_monitor(window)        //  共有（hwnd 経由・monitor.rs）
    window.show(); set_focus();               //  共有（Win32・#558 順序）
    WM_NULL 同期待ち; 残留 Alt 解除            //  共有（focus 確定 & 物理 Alt 解放後にのみ注入）
    backend.post_show(app, t0)                //  WV2: resume 再適用 + ime_control + emit("window-shown")
                                              //  Egui: ime_control のみ（repaint は runtime 任せ）

fn hide_main(app, backend):
    window.hide(); main_visible = false       //  共有
    backend.post_hide(app)                    //  WV2: emit("window-hidden") + suspend + trim
                                              //  Egui: nop
```

- **フックは 3 つ**: `pre_show` / `post_show` / `post_hide`。WV2 の `resume` は show の**前後 2 回**積む必要がある（#576 の残余窓是正）ため pre/post に分けて保つ。egui 側の中身は `pre_show`/`post_hide` が空、`post_show` は ime_control のみ。
- **移すのは逐語**: `resume_webview`/`suspend_and_trim_after_hide`/`emit(window-shown|hidden)`/`working_set::trim` の既存コードをフックへ寄せるだけで、順序制約（`src-tauri/CLAUDE.md`「TrySuspend / Resume パターン」の FIFO 直列化・emit を suspend より先）は温存する。
- **`plan_hotkey`/`plan_ui_action` は backend 非依存の共有決定核**。両経路が同じ核を呼び、副作用適用（`show_main`/`hide_main`）だけが backend で分岐する。

**留保（設計の限界を明示）**: 「差異は WebView2/egui と frontend だけ」が literal に成り立つのは **SU2 の外殻まで**。内側 UI（検索・モード・結果＝SU3）は JS のイベント駆動と egui の即時モードで質が異なり、薄いフックには収まらない。SU2 のスコープが外殻なので、いま整合できる範囲＝整合すべき範囲が一致する。

## 状態機械のデータフロー（合流点は 1 つ）

```
Alt+Q（platform thread RegisterHotKey → emit "hotkey-pressed"）
  → listener: サイドカーガード（SettingsProcessState）→ plan_hotkey(main_visible, alt_pressed)
       HideNow(hotkey_toggle 時) / ShowNow / ShowAfterAltRelease → HostCommand
Escape・focus-lost（view が egui 入力で観測）                    → HostCommand::Hide
                    │
                    ▼
   controller: plan_ui_action(LifecycleState, cmd) → UiAction を適用
       Show    → show_main(app, backend, t0)
       Hide    → hide_main(app, backend)   （view 起点でも必ず controller を通す。加えて view は
                                            RuntimeFrame::hide_window を呼び 1 フレーム早く paint を止めてよい＝迂回ではなく前倒し）
       Refocus → window.set_focus()        （冪等: 表示中 + Show）
       Defer   → FramePresented 後まで Hide 繰り延べ
       Ignore  → 何もしない                （非表示中 + Hide 等）
   → transition で LifecycleState 前進、main_visible を coherent に更新
```

- **合流点（controller）は 1 つ**: hotkey・Escape・focus-lost の全 Hide/Show 要求が `plan_ui_action` を通る。冪等性と Defer をここに一元化する（散らさない）。
- **2 つの `visible` を混同しない**（SU1 申し送り + 助言）: `AppState.main_visible`（policy＝`Standby`/`SearchVisible` 判定・`plan_hotkey` の入力）と `runtime.visible`（SU1 の描画ゲート・不変条件⑥）は**別物**。egui 経路は自前の policy フラグ（`main_visible`）を runtime の描画ゲートと独立に持つ。
- **hotkey listener の toggle 判定**: `hotkey_toggle` は config live-read（既存の #576 パターン踏襲・キャッシュしない）。`hotkey_toggle && main_visible` で hide、さもなくば show。

## blur 自動非表示（policy は src-tauri 側 view に置く）

- **観測**: `view` が `ctx.input(|i| i.focused)` で focus 喪失を検出（要石2・G3）。focus→unfocus の遷移で `unfocus_at` を記録し `request_repaint_after(100ms)` を積む。
- **判定（猶予明け）**: なお非表示 **かつ** `auto_hide_on_focus_lost`（config live-read）**かつ** `SettingsProcessState` が非起動、の三条件で `HostCommand::Hide` を controller へ。refocus で pending を破棄。
- **サイドカーガード必須**（助言・`/state-check` 型の相互作用）: `snotra-settings` を開くと focus を奪い `alwaysOnTop` を落とす。ここで hide してはならない。既存 hotkey listener の `SettingsProcessState` ガードと同型を focus-lost 経路にも置く。
- **policy は runtime クレートに漏らさない**: 100ms 猶予・config ゲート・サイドカーガードはすべて製品 policy。view（`src-tauri` コード）が `app_handle` 経由で config/`SettingsProcessState` を読んで決める。product 非依存の `snotra-egui-runtime` には置かない。
- WebView2 経路は従来どおり JS `onFocusChanged` + `config_watcher` のゲート（`auto-hide-focus-lost-changed`）で不変。egui はこれを view 内でネイティブに再実装する（JS リスナーは無い）。SPEC §8.1 の「100ms 猶予付き」に一致させる。

## 再利用する renderer 非依存部品

- **残留 Alt 解除**（#558）: focus 確認後**かつ物理 Alt が解放済み**のときにのみ `SendInput` で注入。物理 Alt 押下中に key-up を注入すると OS 論理修飾キーが物理キーと desync し Alt+Q が dead-zone する（既存 `show_and_focus_main` の #558 nuance を踏襲）。
- **`monitor.rs`**: `window.hwnd()` に対し作業領域クランプ・中央配置。物理座標ベースで backend 非依存。
- **`window.bin` 相対座標保存**・`setup_first_run`（`--first-run` で `snotra-settings` 起動）・`setup_exit_listener`（history/icon flush は engine 級で不変）。

## 位置永続

- **復元**: show 経路で `position_on_target_monitor`（順序: **サイズ確定 → 位置 → show**）。SPEC §8.2 のマルチモニター規約（作業領域原点からの相対物理座標・`follow_cursor_monitor` でターゲット決定・クランプ・保存なしは中央）を踏襲。
  - **順序制約の SU3 申し送り**: 製品 WebView2 は「高さリセット（52px 折りたたみ）→ 位置 → show」で、位置計算を折りたたみサイズでクランプする（`src-tauri/CLAUDE.md`「`show_main_and_emit` の操作順序制約」）。SU2 の placeholder は固定サイズゆえ高さリセットは no-op だが、**結果リストで高さが動的化する SU3 でこの結合が活性化する**ことを明記する。
- **保存**: **save-on-hide** を主とする（hide 時に現在位置を相対座標で `window.bin` へ保存）。JS チョークポイントが無い egui 経路では、これが最も単純で十分（次回 show で復元・最終ドラッグ位置は hide 時に捕捉）。
  - 表示中終了に備え `setup_exit_listener` でも可視時に位置保存を積む（history/icon flush と同じ終了フラッシュ）。
  - **ドラッグ中デバウンス保存は WANT**: ドラッグは `RuntimeFrame::drag_window`（既存）で動くが、`Moved` を観測してデバウンス保存する経路は現状空白。低コストで載らなければ defer（save-on-hide があるため機能欠落にはならない）。

## フラグと生成

**重要な as-built 事実**: 製品の "main" 窓は **`tauri.conf.json` の `windows[]` で宣言的生成**される（`label:"main"`・`url:"main.html"`・600×52・`visible:false`・`decorations:false`・`skipTaskbar:true`・`alwaysOnTop:true`・`resizable:false`・`center:true`）。setup に `WebviewWindowBuilder` は無い。`build.rs` が config をコンパイル時に焼き込むため、実行時 env フラグで宣言窓だけ抑止する手は無い（`src-tauri/CLAUDE.md`「WebView2 ウィンドウ生成の制約」）。ゆえに **egui が WebView2 を置き換える（子孫 0・ラベル `"main"` 共有）には、宣言窓を config から外し、両経路とも setup で programmatic 生成へ寄せる**（2026-07-22 決定・programmatic 統一）。

- **`SNOTRA_EGUI_MAIN=1`**（env・setup フェーズで 1 回読む。`SNOTRA_DISABLE_SUSPEND`/`SNOTRA_TRACE` と同じ流儀）。ユーザー向け config には出さない（移行/ドッグフード用）。
- **`tauri.conf.json`**: `app.windows` を **`[]`（空）** にする。宣言窓を廃し、両経路とも programmatic 生成にする。CSP・bundle・plugins は不変。
- **ON**: `EguiRuntime::install(app)` → `tauri::Window::builder(app, "main")`（webview 無し・`decorations(false)`・`visible(false)`・600×52）→ `runtime.attach(window, SearchWindowView::new(...))`。WebView2 ウィンドウは**作らない**。
- **OFF**: `build_webview2_window(app)`（新規）が `WebviewWindowBuilder::new(app, "main", WebviewUrl::App("main.html"))` で**宣言窓の 10 フィールドを逐語再現**（title/size/visible:false/decorations:false/skipTaskbar/alwaysOnTop:true/resizable:false/center）。挙動を宣言時と一致させる（G0）。
- **ラベルは `"main"` を踏襲**: `position`/`window.bin`/`monitor.rs`・既存 `get_webview_window("main")` 経路が同ラベルで動く。WebView2 固有経路（resume/suspend・IPC コマンド）は egui ウィンドウを型で掴めず（`get_webview_window` は `None`）、かつ各入口（hotkey listener・`setup_startup_display`・focus-lost）で backend/flag 分岐するため egui 経路では**到達しない**。
- **生成は setup 限定**（同上制約: `WebviewWindowBuilder::build()` はメッセージポンプ進行を要求。setup フェーズは自前で処理でき正常）。egui ウィンドウ（tao）も同様に setup で生成し、ランタイムでは show/hide のみ。

## placeholder view と font-first（SU1 申し送りの義務）

- `SearchWindowView: EguiView`（`egui_shell/view.rs`）は SU2 では最小 chrome（検索バー枠 + 最小テキスト）を描く。本体（検索・結果・モード）は SU3 が埋める。
- **font-first カナリア必須**（SU1 申し送り・`snotra-egui-mvp/CLAUDE.md` #579）: `SearchWindowView::setup` は `jp_font` を Proportional/Monospace 両 family の **index 0**（`insert(0, ...)`）に置く。`push`（末尾 fallback）にすると Latin=egui 既定 / CJK=Yu Gothic の 2 フォントに分離し、被覆 AA を持たない softbuffer が vertical metrics 差を整数 px に丸めてベースラインずれ（#399/#579）を顕在化させる。glow/wgpu は sub-pixel AA で隠すが softbuffer は露見する。**型検査・clippy・単体テストを素通りし視覚でのみ出る**ため、実 `setup` を駆動する config テストで先頭配置を機構的に固定する。

## 変更目録（`src-tauri`）

| 対象 | 扱い |
|---|---|
| `src-tauri/src/egui_shell/mod.rs`（新規） | `MainBackend` enum・生成（`Window::builder`+`install`+`attach`）・`show_main`/`hide_main` 共有オーケストレーション・backend フック（`pre_show`/`post_show`/`post_hide`）・controller（合流点） |
| `src-tauri/src/egui_shell/lifecycle.rs`（新規・純粋核） | `HotkeyPlan`/`plan_hotkey`・`LifecycleState`/`LifecycleEvent`/`transition`・`HostCommand`/`UiAction`/`plan_ui_action`。Win32 非依存・ユニットテスト対象。spike から移植 |
| `src-tauri/src/egui_shell/view.rs`（新規） | `SearchWindowView: EguiView`（placeholder）。`setup` で font-first・`update` で focus 観測（自動非表示の起点） |
| `src-tauri/tauri.conf.json` | `app.windows` を `[]` にする（宣言窓廃止）。CSP・bundle・plugins は不変 |
| `src-tauri/src/main.rs` | setup 本体に **窓生成の flag 分岐**（`if egui_main { egui_shell::create } else { build_webview2_window }`）を足す。`build_webview2_window`（新規）が宣言窓 10 フィールドを `WebviewWindowBuilder` で逐語再現。各 setup 関数（`setup_hotkey_listener`・`setup_startup_display`）に `if egui_main { egui_shell::... } else { 既存 }` の入口分岐。WebView2 の `resume`/`suspend`/`emit`/`trim` を `MainBackend::WebView2` のフックへ**逐語移動**。既存の判定・順序は温存 |
| WebView2 経路の既存ファイル（`commands/`・`config_watcher.rs` 等） | **触らない**（SU2 の egui view は placeholder ゆえ IPC/config 反映は SU3/SU6）。exit-save の位置保存のみ egui 可視時に追加 |
| `Cargo.toml`（`src-tauri`） | **`snotra-egui-runtime = { path = "../snotra-egui-runtime" }` を新規追加**（現状 `src-tauri` は未依存＝workspace member だが dep ではない・実測）。`EguiRuntime`/`EguiView`/`RuntimeFrame` を使うため |

## テスト計画

- **純粋核（ユニット・Win32 非依存）**: spike のテスト（`soft_host_main.rs:1590-1640`）を `egui_shell/lifecycle.rs` へ移植。
  - `plan_hotkey` 表: `(false,false)→ShowNow`・`(false,true)→ShowAfterAltRelease`・`(true,_)→HideNow`。
  - `plan_ui_action` 表: **冪等** `Visible+Show→Refocus`・`Suspended+Hide→Ignore`、**Defer** `Recreating+Hide→Defer`、`Recreating+Show→Ignore`、`Exiting+_→Ignore`。
  - `transition` の valid（`Visible+Hide→Suspended` 等）/ invalid（未定義遷移は `Err`）。
- **font-first カナリア**: 実 `SearchWindowView::setup` を駆動し `jp_font` が両 family index 0（`push` なら fail する canary）。
- **flag-OFF 回帰（G1）**: `SNOTRA_EGUI_MAIN` 未設定で既存 WebView2 テスト + `smoke:startup` / `e2e:tauri` 緑。resume/suspend/emit の逐語移動が挙動を変えないこと。
- **スモーク（trace ベース・`src-tauri.md` カテゴリ C。Win32 依存ゆえユニット前提にしない）**: `SNOTRA_EGUI_MAIN=1` で
  - 起動時表示（`show_on_startup`）/ 初回フロー（`--first-run` で `snotra-settings` 起動）
  - hotkey show / hide（トグル）・`ShowAfterAltRelease`（Alt 押下中の解放待ち）
  - Escape hide / focus-lost 自動非表示（100ms 猶予）・**サイドカー起動中は非 hide**（ガード検証）
  - 位置復元（マルチモニター・クランプ）と save-on-hide
  - `msedgewebview2.exe` 子孫 0 の確認

## 受け入れ条件（SU2）

1. Alt+Q 表示/非表示・blur 自動非表示・フォーカス列・残留 Alt 解除・位置永続・起動時表示・初回フローが SPEC §8（8.1–8.6）と一致する（egui 経路・trace スモークで実証）。
2. **フラグ OFF で WebView2 挙動が不変**（G0 + G1 緑・回帰なし）。宣言→programmatic 窓生成の変換（`build_webview2_window` が 10 フィールド逐語再現）と、resume/suspend/emit/trim のフック移動が、既存挙動・順序制約を温存する。`app.windows=[]` で宣言窓が二重生成されない。
3. **`plan_hotkey`/`plan_ui_action` が純関数**として `egui_shell/lifecycle.rs` に在り、冪等性（表示中+Show / 非表示中+Hide）と show 進行中 Hide の Defer がユニットテストで固定される。
4. focus-lost 自動非表示の policy（100ms 猶予・`auto_hide_on_focus_lost` ゲート・サイドカーガード）が `src-tauri` の view 側にあり、`snotra-egui-runtime` の公開 API を拡張していない。
5. `SearchWindowView::setup` の font-first が実 setup 駆動テストで機構化されている。
6. `cargo clippy --workspace --all-targets` 緑・`src-tauri` テスト緑・`msedgewebview2.exe` 子孫 0（egui 経路）。

## リスク

- **既存 WebView2 経路を触る（整合の代償）**: resume/suspend/emit/trim のフック移動が #576/#361 の順序制約を壊すと hide/show 競合が再発する。G1（flag-OFF byte-identical）を最優先ゲートに置き、逐語移動 + 既存 e2e で押さえる。
- **focus-lost 猶予タイマの immediate-mode 実装**: egui は即時モードゆえ 100ms 猶予を `request_repaint_after` + 状態フラグ（`unfocus_at`）で組む。refocus のキャンセルと猶予明けの再判定を取りこぼさないことを G3 + スモークで固定する。
- **外部 hide と runtime.visible の二重管理**: hotkey トグル hide は外部 `window.hide()`、view 起点 hide は `RuntimeFrame::hide_window`。両者が `main_visible` と `runtime.visible` を coherent に保つこと（G4）。
- **hotkey 配線の window 型差**: listener が egui では `get_window`、WebView2 では `get_webview_window` を掴む。入口分岐の取りこぼしが無いこと（G2）。
- **Tauri 内部 API 追随**: `tauri-runtime-wry` unstable feature 依存は #532 既知。SU2 で新たな追随は増やさない。

## スコープ外（SU2 では触らない）

- 検索体験・クエリ/IME・インクリメンタル検索・直 `Engine`・IPC 撤去・結果リスト/行・キーボードナビ・フォルダ展開・インスタントコマンド・内側モード状態機械（Normal/Command/Tool/Folder/Instant/Indexing）は **SU3**。SU2 の view は placeholder。
- アイコン抽出/キャッシュは SU4。updater は SU5。config 反映（テーマ/ホットキー/index を egui へ）は SU6。
- 署名付き配布・portable ZIP・**既定を egui へ切替 + WebView2 経路撤去**は SU7。
- IME 再変換（reconvert）は WANT（SU5）。
- ルート `CLAUDE.md`/`AGENTS.md` 等の規範文書は変更しない。
