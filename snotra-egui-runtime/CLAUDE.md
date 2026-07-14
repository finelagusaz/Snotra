# snotra-egui-runtime

Tauri管理のネイティブWindowへeguiをwgpuで描画する、Snotra専用の接着層。

## モジュール構成

- `lib.rs`: 公開API
- `input.rs`: Taoイベントからegui入力への純粋変換
- `ime.rs`: IME未確定範囲とDPI座標の純粋変換
- `windows_ime.rs`: IMM32 preedit取得、候補ウィンドウ位置、subclassの所有/破棄
- `gpu.rs`: GPU障害の復旧方針と検証用fault injection型
- `renderer.rs`: wgpu初期化、Surface構成、egui描画
- `repaint.rs`: 即時／遅延repaintをTauriイベントループへ配送
- `runtime.rs`: Tauri wry pluginとWindowごとの状態管理
- `surface.rs`: wgpu Surface状態の復旧方針

## 不変条件

- UI状態は`Send`を要求し、無条件の`unsafe impl Send/Sync`を追加しない
- 0×0のSurfaceをconfigureまたは描画しない
- `Lost`はSurface再作成、`Outdated`は再configureとして区別する
- repaint workerは所有型のDropで停止し、joinする
- Tauri内部型をUI実装へ公開しない
- 通常文字とIME確定文字はTaoの`ReceivedImeText`だけをeguiへ渡し、`KeyboardInput.text`と二重配送しない
