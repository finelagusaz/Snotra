# snotra-egui-runtime

Tauri管理のネイティブWindowへeguiをsoftbuffer（CPUラスタ）で描画する、Snotra専用の接着層。

## モジュール構成

- `lib.rs`: 公開API
- `env.rs`: trace ハッチ（`SNOTRA_EGUI_*_TRACE`）の env 述語。**空文字を「未設定」として扱う唯一の場所**（#872）
- `input.rs`: Taoイベントからegui入力への純粋変換
- `ime.rs`: IME未確定範囲とDPI座標の純粋変換
- `windows_ime.rs`: IMM32 preedit取得、候補ウィンドウ位置、subclassの所有/破棄
- `raster.rs`: egui Meshを CPU 側でラスタライズする純関数群（`renderer.rs`が消費）
- `renderer.rs`: softbuffer Surface初期化・`raster.rs`によるCPUラスタ・present
- `monitor.rs`: 窓が載っているモニターのリフレッシュレート取得（現在モード→OS既定→Noneのカスケード・#737。`runtime.rs`が消費）
- `proof.rs`: イベントループスレッド上にいることの証人型`EventLoopProof`と、フレームの外から**その証人を得る**唯一の口`on_event_loop`（イベントループスレッドへ入る手段自体は`AppHandle::run_on_main_thread`にもある——`on_event_loop`はその薄い包みで、唯一なのは証人が付くことのほう。責務詳細は`//!`）
- `repaint.rs`: 即時／遅延repaintをTauriイベントループへ配送（配送規律は「不変条件」を参照）。窓を外部（別スレッド・別窓・Tauriイベントリスナー）から起こす公開ハンドル`WindowWaker`（`EguiRuntime::attach`の戻り値）もここが所有する
- `runtime.rs`: Tauri wry pluginとWindowごとの状態管理（visibleガード・描画失敗リトライを含む）
- `surface.rs`: `is_renderable_extent`（0×0 Surfaceの描画/configureを防ぐガード。renderer.rsが消費）

## 不変条件

### 配送の規律（フレームスケジューリング契約）

この crate が消費側（`src-tauri/egui_shell/`）へ与える保証は次の 2 つで、**消費側の規範（armed の間は毎フレーム再要求する）はこの 2 つから導かれる**——`src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」。導出の経緯・却下案・errata は `docs/superpowers/specs/2026-07-26-frame-scheduling-contract-design.md`（**日付付き設計書ゆえ歴史記録であり、規範の正本はここ**）。

- **配送には下限間隔がある**（フレーム上限＝窓が載っているモニターのリフレッシュレート・取得失敗時 60Hz・#737）。gate は要求 deadline を**早めも取りこぼしもしない**（遅らせるだけ）。**`min_interval` の変更は次の dispatch から完全反映される**——リフレッシュレートが下がった直後の 1 回だけ旧値の下限で配送されうる（自己回復するため是正しない）
- **予約は「フレームが来ること」を約束しない**（#711）。worker は最も早い deadline だけを**単一スロット**で保持し、dispatch 時に `pending.take()` で**予約全体を空にする**——より早い要求（入力・外部 wake・アニメーション）が 1 つ割り込むと、両者は 1 回の dispatch へ畳まれ、**後の deadline は黙って消える**。`request_repaint_after(d)` を「d 後に 1 枚は来る」と要約してはならない
- **要求しても永久に描かれない窓がある**: 活性化時の softbuffer surface 初期化に失敗した窓は `active` へ入らず、`attach()` が既に返した `WindowWaker` は恒久 no-op になる（`Destroyed`・proxy 切断・hidden も同様に「要求は消えないが何も起きない」経路）

### 一般

- UI状態は`Send`を要求し、無条件の`unsafe impl Send/Sync`を追加しない
- 0×0のSurfaceをconfigureまたは描画しない
- repaint workerは所有型のDropで停止し、joinする。**外部へ渡す wake 経路に`RepaintScheduler`（の強参照・弱参照いずれも）や`egui::Context`の clone を持たせない**——Context の clone は repaint callback ごと複製し、callback が握る Arc が窓の`Destroyed`を越えて停止を止める（#646 PR2〜#671 PR D で実在した破れ）。`WindowWaker`は mpsc の送信側だけを持ち、`SchedulerInner::drop`が`Stop`を明示送信してから join するため、外部が waker を永久保持しても停止は成立する
- Tauri内部型をUI実装へ公開しない
- 通常文字とIME確定文字はTaoの`ReceivedImeText`だけをeguiへ渡し、`KeyboardInput.text`と二重配送しない
- IME未確定文字列はeguiが自前描画し、ネイティブ変換窓は抑制する（`ime_subclass_proc`が`WM_IME_STARTCOMPOSITION`と未確定`WM_IME_COMPOSITION`を`Suppress`）。確定`GCS_RESULTSTR`だけTaoへ通し`ReceivedImeText`で受ける。通すと二重表示が再発する（#532）
- `RedrawRequested`は`on_event`で`WindowEvent`と別armとして扱い、egui入力（`on_window_event`）へ渡さない。渡すとrepaint応答が再描画要求を生み描画が自己永続ループになる（#579で実測: 15秒で約2,000フレーム）
- **`RawInput` の埋めないフィールドは既定値が黙って効く**——`InputState::take` が書く値の集合が、そのまま egui への契約である。`predicted_dt` は **0**（イベント駆動ゆえ「次フレームは vsync 後に来る」前提を持たない）。既定の 1/60 に戻すと egui が `request_repaint_after(d)` を `d - 16.7ms` へ切り詰め、短い予約を「即時再描画」へ飽和させて**遷移ごとにスピンする**（#628 実測: キャレット点滅で 2fps のはずが 11.5fps・CPU 5.1%）。値は `input.rs` のテストが固定する。アイドルの基準値は `PERFORMANCE.md`
- **`RuntimeFrame` の埋めないフィールドは既定値が黙って効く**（`RawInput` と同型）——`set_clear_color` を呼ばなかったフレームは `renderer.rs` の `CLEAR_COLOR`（`0x0028_2828`）へ落ちる。**呼び忘れはビルドでも自動テストでも落ちない**——検知するのは `npm run check:colors` と目視で、どちらも CI には無い（`docs/build-commands.md`「`[visual]` の色を変える変更は、**非既定色で**目視する」）。**`CLEAR_COLOR` と `snotra-core` の既定背景色の一致は規約ではなく機構が固定する**——`src-tauri/src/egui_shell/window_coordinator.rs` の `runtime_fallback_matches_config_default_background`（由来と理由は `snotra-egui-runtime/src/renderer.rs` の doc）
- **OS へ書く platform output は「変化したときだけ」撃つ**——`handle_platform_output` が毎フレーム無条件に呼ぶと、**窓に紐づかない Win32 API では 2 窓が撃ち合う**。`set_cursor_icon` は tao が `SetCursor` を直接呼ぶ（スレッド共通・最後に呼んだ者が勝つ）ため、ポインタを持つ窓（`Text`）と持たない窓（`Default`）が交互に上書きしてカーソルが点滅した（#628 の計測中に実機発見。マウス静止中は `WM_SETCURSOR` が来ないので OS の復元も入らない）。同じ形の経路（IME 位置の更新等）を足すときも変化検出を伴わせる
