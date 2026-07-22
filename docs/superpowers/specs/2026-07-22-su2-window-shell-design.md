# SU2 設計 — ウィンドウシェル + 状態機械（#532 Phase 2）

- 種別: サブユニット設計（spec）。実装計画は本 spec 承認後に別途 writing-plans で作る
- 日付: 2026-07-22
- 親: `docs/superpowers/specs/2026-07-21-phase2-softbuffer-migration-roadmap.md`（SU2）／#532
- 前段: `docs/superpowers/specs/2026-07-21-su1-softbuffer-runtime-design.md`（SU1・実装完了）
- 履歴: 初版は「共有 show_main + MainBackend 3 フック seam + LifecycleController(4 状態)」だったが、codex 敵対的レビュー（2026-07-22）が (a) E2E が宣言窓へ依存、(b) controller の lock 非保持競合、(c) egui 専用で `Recreating`/`Defer` が live 到達不能な過剰設計、を実測で指摘。**egui 専用フォーク（共有は位置計算のみ）へ簡素化**した。旧設計の否定の知識は本節末「否定の知識」に残す

## 目的

製品 `src-tauri` のメインウィンドウに、WebView2 と**並行して** egui/softbuffer 経路（クレート `snotra-egui-runtime`＝SU1 完了）を env フラグ選択で立ち上げる「外殻」を作る。SU2 が持つのは外側の状態機械——Alt+Q 表示/非表示・blur 自動非表示・フォーカス列・残留 Alt 解除・位置永続・起動時表示・初回フロー。内側の検索モードと検索体験は SU3。SU2 の egui view は **placeholder**。

## アプローチ（簡素化後の要石）

**egui 経路は WebView2 経路と分離した専用の show/hide を持つ。共有するのは位置計算（`position_on_target_monitor` の `&tauri::Window` 一般化）だけ。** WebView2 経路（フラグ OFF）は `show_main_and_emit` を含め**一切変更しない**。理由（codex #13）: controller/4 状態機械/`plan_ui_action`/`Defer` は egui 専用で live 到達不能（egui の show は同期）——共有 seam は共通化でなく二重の状態所有と競合面を作る。SU7 は egui 専用関数がそのまま残り WebView2 経路を削除するだけ。

### 検証済み（この設計の前提・一次証拠）

1. **フラグ選択は config の実行時ミューテーションで行う**（codex #2 の解）。製品 "main" 窓は `tauri.conf.json` の宣言生成（`src-tauri/tauri.conf.json:14-28`）。E2E feature は `app_context.config_mut().app.windows` を実行時に書き換えてブラウザ引数を注入する（`main.rs:588-599`・`configure_e2e_webview`）。**同じ経路でフラグ ON のとき `config_mut().app.windows` から "main" を除去**すれば、Tauri は WebView2 窓を作らず（子孫 0）、egui を programmatic 生成できる。**フラグ OFF は宣言窓も E2E 注入も完全に不変**——`tauri.conf.json` は書き換えない。
2. **`plan_hotkey` は spike で実証済み**（`soft_host_main.rs:139-147`）。`plan_hotkey(visible, alt_pressed) -> HotkeyPlan{HideNow|ShowAfterAltRelease|ShowNow}`。表示中なら即 hide、非表示中は Alt 押下中なら解放待ち後 show。これは egui/WebView2 両経路が同じ意味論で使う純粋核。**`plan_ui_action`/`LifecycleState`/`Recreating`/`Defer` は採らない**（下記「否定の知識」）。
3. **egui window は webview 無しで生成できる**（probe `snotra-egui-mvp/src/main.rs:740`・`snotra-egui-mvp/CLAUDE.md`）。`msedgewebview2.exe` 子孫 0 を維持。
4. **focus 観測は view 内 egui 入力**（probe `main.rs:174`: `ctx.input(|i| i.focused)`）。runtime は tao の `Focused` を egui 入力へ流し込み済みで、blur policy を **src-tauri の view 側**に置ける。runtime API を拡張しない。
5. **`EguiRuntime::install` は `&mut tauri::App<Wry>` を取る**（`runtime.rs:77`・codex #1）。生成関数 `create` は setup クロージャの `&mut App` を受ける。

### 実装初手で確定させる検証ゲート（崩れると設計が反転する）

- **G1（フラグ OFF 完全不変）**: `config_mut().app.windows` の条件付き除去はフラグ ON のときだけ走る。フラグ未設定で `smoke:startup` / `e2e:tauri` が緑（WebView2 経路・E2E 注入とも無改変）。**最優先ゲート**——簡素化の眼目は「flag OFF を一行も触らない」こと。
- **G2（hotkey → egui show の配線）**: 製品ホットキー（Win32 `RegisterHotKey` → `emit("hotkey-pressed")` → listener）が egui 経路では `get_window("main")` を掴む必要がある。emit→listener→egui show が実機で 1 回通ることを確認（初手スモーク）。
- **G3（hide→show で runtime が確実に再描画する・codex #4）**: egui の全 hide を**外部 `window.hide()`** で行い（`RuntimeFrame::hide_window` を使わない）、runtime の `visible` を false にしない。隠れ窓は `RedrawRequested` が来ないので描かれず、show（`window.show()`）で OS が配送を再開すると runtime が `visible=true` のまま再描画する。hide→show サイクルで空白窓（`set_focus` 失敗時に `Focused(true)` が来ず runtime が描かない縁）が起きないことを確認。**起きるなら** runtime に最小の「可視化/再描画」フックを足す（SU1 隣接・要相談）。
- **G4（`config_mut` 除去が子孫 0 を生む）**: フラグ ON で `msedgewebview2.exe` 子孫 0。宣言窓が二重生成されない。

## フラグと生成（簡素化後）

- **`SNOTRA_EGUI_MAIN=1`**（env・`main()` で 1 回読む。`crate::trace::env_flag`）。
- **`tauri.conf.json` は不変**（宣言窓 "main" を残す）。
- **`main()`**: `generate_context!()` と E2E 注入ブロックの後、フラグ ON なら `app_context.config_mut().app.windows` から `label=="main"` を除去（`.retain(|w| w.label != "main")`）。これで Tauri は WebView2 窓を作らない。
- **setup（フラグ ON）**: `egui_shell::create(app, window_width)`（`&mut App`）が `EguiRuntime::install(app)` → `tauri::Window::builder(app, "main")`（webview 無し・`visible(false)`・`decorations(false)`・**config の `window_width`×52**・**`skip_taskbar(true)`・`always_on_top(true)`**＝宣言窓プロパティ再現・codex #11/(B)#1）→ `attach(window, SearchWindowView::new(app_handle))`。生成は setup 限定で、**`setup_platform_thread` の後**に置く（SPEC §8.5 の「platform thread を窓生成より前に spawn」の並列化順序・codex #12）。
- **setup（フラグ OFF）**: 変更なし。宣言窓が Tauri により生成され、既存 WebView2 経路がそのまま動く。
- **ラベルは両方 `"main"`**: egui 経路は `get_window("main")` で掴む。フラグ OFF の `get_webview_window("main")` 依存（幅復元・config 幅反映・位置保存・Settings 制御）は宣言窓が在るので従来どおり動く。フラグ ON でそれらが no-op になる箇所（幅反映は SU3/SU6・幅復元は placeholder 固定 600px で無害）は各所で確認する（codex #9/#11）。

## egui 経路の show/hide（専用・共有は位置計算のみ）

```
show_egui_main(app, t0):
    main_visible = true                       // AppState.main_visible（policy・plan_hotkey 入力）
    position_on_target_monitor(app, &window)  // 唯一の共有（&tauri::Window 一般化）
    window.show(); window.set_focus()          // Win32・runtime は Focused(true) で visible 復帰
    WM_NULL 同期待ち; 残留 Alt 解除             // focus 確定後かつ物理 Alt 解放後のみ（#558）
    // ime_off_on_show が有効なら Win32 IME off（hwnd 経由）
    // emit は無し（JS フロントが無い）

hide_egui_main(app):
    save_placement_relative(app, &window)      // save-on-hide（JS チョークポイントが無い）
    window.hide(); main_visible = false        // 外部 hide（RuntimeFrame::hide_window は使わない・#4）
```

- **状態は `AppState.main_visible`（bool）+ 共有 `EguiShellState`**（managed）。controller も 4 状態機械も持たない。ホットキーは `plan_hotkey(main_visible, is_alt_pressed())` で分岐する。`EguiShellState { hotkey_generation: AtomicU64, hide_pending: AtomicBool }` が show/hide/view を跨ぐ 2 点を協調させる:
  - **`hotkey_generation`**: `ShowAfterAltRelease` の spawn スレッドが待機後に世代一致を確認して show する。**hide（`hide_egui_main`）が世代を bump** し、保留中の alt 解放待ち show を無効化する（hide 後の再表示を防ぐ・codex #5/(B)#2）。
  - **`hide_pending`**: view の `emit("egui-hide-requested")` 多重防止フラグ。**show（`show_egui_main`）がクリア**する（view-local だと hide 後に `Focused(true)` が来ないとき true が残り以後の hide を抑止する・codex #8）。
- **全 hide は外部 `window.hide()`**（codex #4/#7 の解）。`RuntimeFrame::hide_window`（runtime.visible=false）は使わない。これで (a) 次の show が `Focused(true)` に依存せず確実に描け、(b) hide の副作用所有が一箇所（`hide_egui_main`）に一元化する。
- **`main_visible` は `AtomicBool`**（`state.rs:17`・既存）。hotkey（メインスレッド）と hide 経路が触るが単一 atomic ゆえ tear しない。plan_hotkey の判定→show/hide は各トリガーが直列に行う（controller の lock 非保持競合＝codex #5 は controller を廃したことで消える）。

## 状態機械のデータフロー

```
Alt+Q（platform thread → emit "hotkey-pressed"）
  → listener（フラグ ON 分岐）: サイドカーガード → plan_hotkey(main_visible, is_alt_pressed())
       HideNow(hotkey_toggle 時) → hide_egui_main
       ShowNow                   → show_egui_main
       ShowAfterAltRelease       → spawn: alt 解放待ち → generation 確認 → show_egui_main
Escape・focus-lost（view が egui 入力で観測）
  → view が emit("egui-hide-requested")（多重防止フラグで 1 回）
  → src-tauri listener → hide_egui_main    // view から直接 window を触らず listener 合流（#7）
```

- **view は window を直接 hide しない**（codex #7）。focus-lost/Escape を検出したら `app_handle.emit("egui-hide-requested")` し、src-tauri の listener（メインスレッド）が `hide_egui_main` を呼ぶ。hide の副作用（位置保存・window.hide・main_visible）は listener 側の 1 経路に集約する。
- **2 つの visible を混同しない**: `AppState.main_visible`（policy）と runtime 内 `visible`（SU1 描画ゲート）。本設計では runtime.visible は常に true に保つ（外部 hide のみ使う）。

## blur 自動非表示（policy は src-tauri 側 view に置く）

- **観測**: view が `ctx.input(|i| i.focused)` で focus 喪失を検出。focus→unfocus 遷移で `unfocus_at=Some(now)` + `request_repaint_after(100ms)`。
- **判定（猶予明け）**: なお非表示 **かつ** `auto_hide_on_focus_lost`（config live-read）**かつ** `SettingsProcessState` 非起動、で `emit("egui-hide-requested")`（多重防止）。
- **stale 猶予の防止（codex #8）**: `focused` のとき `unfocus_at=None`。加えて **show のたびに view 側の `was_focused`/`unfocus_at` をリセット**（再表示直後に前回の stale な猶予で即 hide しない）。focus 復帰と多重 focus-loss を状態として扱う。
- **サイドカーガード必須**（`/state-check` 型）: `snotra-settings` は focus を奪うので focus-lost で hide してはならない。
- **policy は runtime クレートに漏らさない**: 100ms 猶予・config ゲート・サイドカーガードは view（src-tauri）が `app_handle` 経由で読む。

## SPEC §8.5 の alwaysOnTop（codex #3・SU2 に最小で含める）

現行は `snotra-settings` 起動中にメインの `alwaysOnTop=false`、終了検知で復元する（`SPEC.md:412`）。この制御は `get_webview_window("main")` にキーされ、egui 窓では no-op になる。**egui 窓が常に最前面のまま設定 UI を覆うのは dogfooding を不能にする**ため、SU2 に最小で含める: 設定 launch/exit 監視で、フラグ ON なら `get_window("main")` に対し `set_always_on_top(false/true)` を適用する（`get_webview_window` 分岐と並置）。**これは §8.5 の parity であって設定サイドカー共存の本体（SU6）ではない**——SU6 は config 反映・終了保存の統合を担う。

## 再利用する renderer 非依存部品

- **残留 Alt 解除**（#558）: `is_alt_pressed()`/`wait_alt_release_or_timeout()`/`send_alt_key_up()`（`main.rs:90,106,131`）。focus 確定後かつ物理 Alt 解放後にのみ注入。
- **位置計算**: `position_on_target_monitor`（`main.rs:329`）を `&tauri::WebviewWindow` → `&tauri::Window` へ一般化（唯一の共有点）。`monitor.rs`・`window_data::load_search_placement()`/`save_search_placement()` を再利用。
- `setup_first_run`（`--first-run` で `snotra-settings` 起動）・`setup_exit_listener`（history/icon flush）はフラグに依らず不変。

## 位置永続

- **復元**: `show_egui_main` の `position_on_target_monitor`（順序: サイズ確定 → 位置 → show）。SPEC §8.2 のマルチモニター規約（相対物理座標・`follow_cursor_monitor`・クランプ・中央フォールバック）を踏襲。SU2 placeholder は固定サイズゆえ高さリセットは不要だが、結果リストで高さが動的化する **SU3 で高さリセット→位置→show の結合が活性化**する旨を申し送る。
- **保存**: **save-on-hide**（`hide_egui_main`）+ 可視時終了保存（`setup_exit_listener`）。
- **ドラッグ中デバウンス保存は残余（codex #10）**: SPEC §8.2 はデバウンス保存を要求するが、`Moved` 観測経路が現状空白。save-on-hide は Alt+F4/クラッシュで位置を巻き戻す穴が残る。低コストで `Moved`→デバウンス保存を載せられれば載せ、困難なら **SU2 の受容する残余**として記録し SU3/SU6 で解消する（placeholder 段階では優先度低）。

## placeholder view と font-first（SU1 申し送りの義務）

- `SearchWindowView: EguiView`（`egui_shell/view.rs`）は最小 chrome（検索バー枠 + 混在テキスト）を描く。本体は SU3。
- **font-first カナリア必須**（`snotra-egui-mvp/CLAUDE.md` #579）: `setup` は `jp_font` を Proportional/Monospace の **index 0**（`insert(0, ...)`）。`push` にすると softbuffer で #399/#579 のベースラインずれ再発。実 `setup` を駆動する config テストで固定。

## 変更目録（`src-tauri`）

| 対象 | 扱い |
|---|---|
| `src-tauri/src/egui_shell/mod.rs`（新規） | `create(&mut App)`・`show_egui_main`/`hide_egui_main`・`plan_hotkey` の re-export・`egui-hide-requested` listener 登録・`save_placement_relative` |
| `src-tauri/src/egui_shell/lifecycle.rs`（新規・純粋核） | `HotkeyPlan`/`plan_hotkey` のみ。Win32 非依存・ユニットテスト。spike から移植 |
| `src-tauri/src/egui_shell/view.rs`（新規） | `SearchWindowView`（placeholder）。`setup` で font-first・`update` で focus 観測 → `emit("egui-hide-requested")` |
| `src-tauri/src/main.rs` | `main()` に `config_mut().app.windows` の条件付き "main" 除去。setup に窓生成の flag 分岐（`egui_shell::create`）。`setup_hotkey_listener`/`setup_startup_display`/`setup_exit_listener`/設定 launch-exit（§8.5）に flag 分岐。`position_on_target_monitor` を `&tauri::Window` へ一般化 |
| `src-tauri/src/main.rs`（`show_and_focus_main` 等） | **WebView2 経路は変更しない**（`show_main_and_emit`・resume/suspend/emit・hooks 化はしない）。共有するのは `position_on_target_monitor` の型一般化のみ |
| `src-tauri/Cargo.toml` | `snotra-egui-runtime = { path = "../snotra-egui-runtime" }` 追加（現状未依存） |
| `src-tauri/tauri.conf.json` | **不変**（宣言窓を残す） |

## テスト計画

- **純粋核（ユニット）**: `plan_hotkey` 表（`(false,false)→ShowNow`・`(false,true)→ShowAfterAltRelease`・`(true,_)→HideNow`）。
- **font-first カナリア**: 実 `SearchWindowView::setup` を駆動し jp_font が両 family index 0。
- **flag-OFF 完全不変（G1）**: `SNOTRA_EGUI_MAIN` 未設定で既存テスト + `smoke:startup` + `e2e:tauri` 緑。`config_mut` 除去がフラグ ON でしか走らないこと。
- **スモーク（trace ベース・Win32 依存ゆえユニット前提にしない）**: `SNOTRA_EGUI_MAIN=1` で 起動時表示/初回フロー・hotkey show/hide/`ShowAfterAltRelease`・Escape/focus-lost 自動非表示（100ms・サイドカー起動中は非 hide）・**hide→show を反復して空白窓が出ない（G3）**・設定起動で最前面を明け渡す（§8.5）・位置復元/save-on-hide・`msedgewebview2.exe` 子孫 0（G4）。

## 受け入れ条件（SU2）

1. Alt+Q・blur・フォーカス列・残留 Alt 解除・位置永続・起動時表示・初回フローが SPEC §8（8.1–8.6）と一致（trace スモーク）。
2. **フラグ OFF で WebView2 挙動・E2E 注入が完全不変**（G1）。`config_mut().app.windows` 除去はフラグ ON でのみ走り、`tauri.conf.json`・`show_main_and_emit`・E2E 経路を触らない。
3. `plan_hotkey` が純関数として `lifecycle.rs` に在りユニットテストされる。ホットキー分岐がテストで固定。
4. blur policy（100ms 猶予・`auto_hide_on_focus_lost`・サイドカーガード・stale リセット）が view 側にあり runtime API を拡張していない。全 hide が外部 `window.hide()` で runtime.visible を false にしない（G3）。
5. `SearchWindowView::setup` の font-first が実 setup 駆動テストで機構化。
6. `cargo clippy -p snotra --all-targets` 緑・`src-tauri` テスト緑・`msedgewebview2.exe` 子孫 0（G4）。

## リスク

- **egui runtime の可視化（codex #4）**: 全 hide を外部化して runtime.visible を false に落とさない設計で回避するが、view 起点の hide も外部化（emit→listener）で通す必要がある。hide→show サイクルの空白窓不在を G3 で接地。破れるなら runtime に最小フックを足す（要相談）。
- **フラグ ON での `get_webview_window("main")` no-op（codex #9/#11）**: 幅復元・config 幅反映・位置保存・Settings 制御が egui 窓を掴めない。SU2 で必要な §8.5（alwaysOnTop）は含める。幅系は placeholder 固定 600px で無害・本格対応は SU3/SU6。各所を列挙して確認する。
- **デバウンス保存の欠落（codex #10）**: save-on-hide では Alt+F4/クラッシュで位置巻き戻り。残余として記録。
- **Tauri 内部 API 追随**: `tauri-runtime-wry` unstable 依存（#532 既知）。SU2 で増やさない。

## 否定の知識（なぜ初版の seam を却下したか）

- **共有 `show_main` + `MainBackend` 3 フック seam を却下**（codex #13）。egui 専用で `plan_ui_action`/`Recreating`/`Defer` が live 到達不能。共有できるのは実質 show/hide/位置の数行で、enum seam は共通化でなく二重の状態所有と競合面（codex #5/#6/#7）を作る。→ egui 専用 `show_egui_main`/`hide_egui_main` に分離し、共有は `position_on_target_monitor` の型一般化のみ。SU7 は egui 関数を残し WebView2 を削除するだけ。
- **`LifecycleController`（4 状態 + `plan_ui_action` + `Defer`）を却下**。roadmap SU2 受け入れは「`plan_ui_action`・冪等性・show 進行中 Hide の Defer をテストで固定」を課すが、**egui の show は同期**（`window.show()` は即・SU1 runtime は surface を再生成せず `Focused` 観測で repaint）ゆえ mid-flight race が無く、Defer は live 到達不能。controller の lock を critical section 全体で保持しない実装は競合を生む（codex #5）。→ **live は `main_visible`(bool) + `plan_hotkey` のみ**。roadmap のこの受け入れ条件は egui 同期 show の下では obsolete と判断（本 spec が roadmap を supersede する明示的逸脱）。
- **`tauri.conf.json` の `app.windows=[]` 静的空化を却下**（codex #2）。E2E feature が宣言窓 "main" へブラウザ引数を注入するため空化は `e2e:tauri` を panic させる。→ フラグ ON のときだけ `config_mut().app.windows` を実行時に除去（flag OFF は宣言窓・E2E とも不変）。
- **view から `RuntimeFrame::hide_window` で直接 hide を却下**（codex #4/#7）。runtime.visible=false が次 show の空白窓リスクを生み、controller 合流を破る。→ view は `emit("egui-hide-requested")` で listener へ委ね、全 hide を外部 `window.hide()` に一本化。

## スコープ外（SU2 では触らない）

- 検索体験・IPC 撤去・内側モード状態機械は SU3。アイコンは SU4。updater は SU5。config 反映・**設定サイドカー共存の本体（終了保存統合等）**は SU6。切替・配布は SU7。
- IME 再変換（reconvert）は WANT（SU5）。
- ドラッグ中デバウンス保存は残余（低コストで載れば SU2、困難なら SU3/SU6）。
- ルート `CLAUDE.md`/`AGENTS.md` 等の規範文書は変更しない。
