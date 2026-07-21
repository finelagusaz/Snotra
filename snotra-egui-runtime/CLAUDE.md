# snotra-egui-runtime

Tauri管理のネイティブWindowへeguiをsoftbuffer（CPUラスタ）で描画する、Snotra専用の接着層。

## モジュール構成

- `lib.rs`: 公開API
- `input.rs`: Taoイベントからegui入力への純粋変換
- `ime.rs`: IME未確定範囲とDPI座標の純粋変換
- `windows_ime.rs`: IMM32 preedit取得、候補ウィンドウ位置、subclassの所有/破棄
- `gpu.rs`: 旧wgpuレンダラー用のGPU障害復旧方針・fault injection型（#532 SU1で呼び手を失った中間状態。Task 6でgpu.rs一式を撤去予定）
- `raster.rs`: egui Meshを CPU 側でラスタライズする純関数群（`renderer.rs`が消費）
- `renderer.rs`: softbuffer Surface初期化・`raster.rs`によるCPUラスタ・present
- `repaint.rs`: 即時／遅延repaintをTauriイベントループへ配送
- `runtime.rs`: Tauri wry pluginとWindowごとの状態管理（visibleガード・描画失敗リトライを含む）
- `surface.rs`: `is_renderable_extent`（renderer.rsが消費）と、wgpu Surface状態の復旧方針`surface_action`/`SurfaceAction`（#532 SU1でrenderer.rsのsoftbuffer化に伴い消費者を失った中間状態）

## 不変条件

- UI状態は`Send`を要求し、無条件の`unsafe impl Send/Sync`を追加しない
- 0×0のSurfaceをconfigureまたは描画しない
- `Lost`はSurface再作成、`Outdated`は再configureとして区別する
- repaint workerは所有型のDropで停止し、joinする
- Tauri内部型をUI実装へ公開しない
- 通常文字とIME確定文字はTaoの`ReceivedImeText`だけをeguiへ渡し、`KeyboardInput.text`と二重配送しない
