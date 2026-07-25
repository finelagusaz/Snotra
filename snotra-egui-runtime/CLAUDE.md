# snotra-egui-runtime

Tauri管理のネイティブWindowへeguiをsoftbuffer（CPUラスタ）で描画する、Snotra専用の接着層。

## モジュール構成

- `lib.rs`: 公開API
- `input.rs`: Taoイベントからegui入力への純粋変換
- `ime.rs`: IME未確定範囲とDPI座標の純粋変換
- `windows_ime.rs`: IMM32 preedit取得、候補ウィンドウ位置、subclassの所有/破棄
- `raster.rs`: egui Meshを CPU 側でラスタライズする純関数群（`renderer.rs`が消費）
- `renderer.rs`: softbuffer Surface初期化・`raster.rs`によるCPUラスタ・present
- `repaint.rs`: 即時／遅延repaintをTauriイベントループへ配送。窓を外部（別スレッド・別窓・Tauriイベントリスナー）から起こす公開ハンドル`WindowWaker`（`EguiRuntime::attach`の戻り値）もここが所有する
- `runtime.rs`: Tauri wry pluginとWindowごとの状態管理（visibleガード・描画失敗リトライを含む）
- `surface.rs`: `is_renderable_extent`（0×0 Surfaceの描画/configureを防ぐガード。renderer.rsが消費）

## 不変条件

- UI状態は`Send`を要求し、無条件の`unsafe impl Send/Sync`を追加しない
- 0×0のSurfaceをconfigureまたは描画しない
- repaint workerは所有型のDropで停止し、joinする。**外部へ渡す wake 経路に`RepaintScheduler`（の強参照・弱参照いずれも）や`egui::Context`の clone を持たせない**——Context の clone は repaint callback ごと複製し、callback が握る Arc が窓の`Destroyed`を越えて停止を止める（#646 PR2〜#671 PR D で実在した破れ）。`WindowWaker`は mpsc の送信側だけを持ち、`SchedulerInner::drop`が`Stop`を明示送信してから join するため、外部が waker を永久保持しても停止は成立する
- Tauri内部型をUI実装へ公開しない
- 通常文字とIME確定文字はTaoの`ReceivedImeText`だけをeguiへ渡し、`KeyboardInput.text`と二重配送しない
- IME未確定文字列はeguiが自前描画し、ネイティブ変換窓は抑制する（`ime_subclass_proc`が`WM_IME_STARTCOMPOSITION`と未確定`WM_IME_COMPOSITION`を`Suppress`）。確定`GCS_RESULTSTR`だけTaoへ通し`ReceivedImeText`で受ける。通すと二重表示が再発する（#532）
- `RedrawRequested`は`on_event`で`WindowEvent`と別armとして扱い、egui入力（`on_window_event`）へ渡さない。渡すとrepaint応答が再描画要求を生み描画が自己永続ループになる（#579で実測: 15秒で約2,000フレーム）
