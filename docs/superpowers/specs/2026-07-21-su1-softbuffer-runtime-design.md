# SU1 設計 — snotra-egui-runtime の softbuffer 置換（#532 Phase 2）

- 種別: サブユニット設計（spec）。実装計画は本 spec 承認後に別途 writing-plans で作る
- 日付: 2026-07-21
- 親: `docs/superpowers/specs/2026-07-21-phase2-softbuffer-migration-roadmap.md`（SU1）／#532

## 目的

`snotra-egui-runtime` の描画基盤を wgpu → softbuffer + CPU ラスタライズ（`fill_mesh`）へ **in-place 置換**する。`EguiRuntime`/`EguiView` の公開 API と tao + tauri-runtime-wry プラグイン統合を保ち、`EguiView` を差し替えるだけで任意 UI を softbuffer で描けるクレートにする。SU2 以降（製品メインウィンドウ）の描画基盤となる。

## 決定の要石（検証済み）

このスコープを固める前に、SU1 の形を変えうる 2 点を検証で潰した。

1. **softbuffer は tao/wry 管理の `tauri::Window` にバインドできる。** `Cargo.lock` の `raw-window-handle` は 0.6.2 が単一定義（他は依存からの参照）。softbuffer 0.4.6 は rwh 0.6 の `HasWindowHandle`/`HasDisplayHandle` を要求し、`renderer.rs` は既に `tauri::Window` に対し wgpu の `create_surface` を通している。ゆえに `softbuffer::Context::new(window)`/`Surface::new(&ctx, window)` はコンパイルが通る見込み。**実装初手の Red で 5 行のバインド/present コンパイルチェックにより確定させる**（2 メジャー共存の罠が無いことは確認済みだが、コンパイルを一次証拠にする）。
2. **テキスト AA の parity 対象は egui-wgpu の linear filter。** `epaint-0.35.0/src/texture_atlas.rs:191` の `texture_options()` は `TextureOptions::LINEAR` を返す。spike の `fill_mesh` は `sample_nearest` を使う——これが parity 対象からの既知の乖離源。的を絞った修正は **atlas の bilinear サンプル**（texture の filter を尊重）。

## 境界 — 何を変え、何を据え置くか

移行の難所はラスタライザ移植ではなく、「ランタイムが **tao + tauri-runtime-wry プラグイン** で統合される」境界を保つこと。spike（`soft_host_main.rs`）は winit + egui_winit 直で統合しており、その winit イベントループは持ち込まない。変わるのは renderer の中身と wgpu 状態機械の掃除だけで、統合骨格・入力・IME・repaint は無改変で流用する。

| ファイル | 扱い |
|---|---|
| `renderer.rs` | 中身を全面置換（wgpu → softbuffer + `fill_mesh`）。型 `EguiRenderer` と `new`/`configure`/`paint` の外形は維持 |
| `raster.rs`（新規） | spike の純粋ラスタ中核を抽出（`edge`/`blend_premultiplied`/`modulate`/`CpuTexture`/`fill_mesh`/`image_to_pixels`/`apply_texture_delta`）。renderer 非依存の純関数群としてテスト可能に |
| `surface.rs` | wgpu 状態機械（`SurfaceAction`/`surface_action` と関連テスト）を撤去。`is_renderable_extent` は残す（softbuffer は `NonZeroU32` 必須ゆえ 0×0 を弾く用途が生きる） |
| `gpu.rs` | 撤去。softbuffer に device loss / OOM / validation の概念は無い |
| `runtime.rs` | 据え置き。`render()` 内の `PaintOutcome::DeviceRecovered` 分岐と `apply_frame_commands` の `gpu_fault_requested` 経路のみ除去 |
| `input.rs` / `ime.rs` / `windows_ime.rs` / `repaint.rs` | 無改変で流用（いずれも renderer 非依存） |

## レンダリングパイプライン（softbuffer 版 `paint`）

現行 wgpu の `paint` と同じ呼び出し地点（プラグインの `Event::RedrawRequested` → `EguiWindow::render` → `renderer.paint`）を保ち、中身のみ差し替える。softbuffer present は wgpu が present するのと同一地点ゆえ、配送構造は据え置きで成立する。

```
paint(ctx, full_output):
  is_renderable_extent(w,h) が false → PaintOutcome::Skipped
  paint_jobs = ctx.tessellate(shapes, ppp)
  textures_delta.set を CpuTexture へ apply（delta.pos による全面/部分更新、delta.options の filter を保持）
  surface.resize(NonZero(w), NonZero(h))
  buffer = surface.buffer_mut()
  buffer.fill(CLEAR_COLOR)                       // 製品ダーク背景色（フラッシュ回避）
  for mesh in paint_jobs:
      clip_rect を物理 px の clip_min/clip_max へ
      fill_mesh(buffer, w, h, verts, indices, texture, clip, ppp)  // filter は texture に従う
  buffer.present()
  textures_delta.free を破棄
  PaintOutcome::Presented
```

- `softbuffer::Context` と `Surface` は `EguiRenderer::new` で `tauri::Window` から一度だけ生成して保持する。`configure(w,h)` は wgpu の surface 再構成の代わりに、次フレームの `surface.resize` 前提の寸法確定に写像する（0×0 では resize/present しない）。
- **復旧モデルの縮退**: `PaintOutcome` は `Presented` / `Skipped` の 2 値へ縮む。wgpu の `SurfaceRecovered`/`DeviceRecovered` は消滅する。softbuffer の失敗（context/surface 生成・resize・buffer borrow・present）は `RuntimeError` として上へ返し、`runtime.rs` の既存のエラーログ経路（`SNOTRA_EGUI_RENDER_ERROR`）に載せる。
- `max_texture_side()` は wgpu device limit の代わりに CPU 適正の定数を返す（egui のフォントアトラス clamp 用。値は egui 既定上限に相当する固定値）。

## AA 戦略（bilinear now / 図形は verify-then-add）

- **テキスト（now）**: `CpuTexture` に filter 情報（`delta.options.magnification`/`minification`）を持たせ、`fill_mesh` のサンプルを nearest/bilinear で切替える。フォントアトラスは `TextureOptions::LINEAR` ゆえ bilinear が効き、glyph の sub-pixel 位置品質を回復する。これがテキスト parity の高レバレッジな一手。
- **図形エッジ（verify-then-add）**: 自作 CPU ラスタは被覆率 AA を持たない（ピクセル中心二値判定）。選択矩形・角丸などの図形エッジのジャギは分離可能な残差。**製品規模スモークで不足が目視できたときのみ** supersample か解析的被覆を追加する。ロードマップの「AA を作り込む or 品質が十分かを検証」の両論併記と整合。

## harness と受け入れ実証（main.rs 転用）

softbuffer 化した runtime を end-to-end で動かし、受け入れ条件を自証する主体が要る。runtime を `EguiView`/`EguiRuntime` 経由で駆動する唯一の現行 probe は `snotra-egui-mvp/src/main.rs`（wgpu 経路）であり、winit soft spike 群は runtime クレートを経由しない。ロードマップ決定1は `main.rs` を撤去対象に列挙していたが、素直に削除すると softbuffer **ランタイム**を通しで動かすものが消え、SU1 が AA 目視 parity・IME を自証できなくなる（この穴はロードマップ策定時に見落とされていた）。

- **決定（本 brainstorm で改定）**: `main.rs` は撤去せず、`MvpView` を softbuffer 化した runtime 経由で駆動する形へ書き換えて **runtime 駆動 probe を 1 本維持**する。`GpuFaultInjection` 依存を除去する。ロードマップ決定1の「`main.rs` 撤去」は本 spec でこの転用へ置き換える。
- **IME**: runtime 自身の IMM32 経路（`ime.rs`/`windows_ime.rs`）は renderer 非依存ゆえ swap を生き延びる。ただし **spike の winit `Ime` イベント検証（#582）は runtime へ転移しない**——「IME preedit/候補/確定が softbuffer 上で正しい」は転用した runtime harness で**再実証**する。
- **フォント**: 「jp_font を families の先頭（`insert(0, …)`）に置く」を **config テストで機構化**する。この不変条件（`snotra-egui-mvp/CLAUDE.md`・#399/#579）は型検査・clippy・単体テストを素通りし、混在行のベースラインずれとして視覚でのみ露見する。フォント設定は view 側（`EguiView::setup`）で行うため、テストも view/harness と同居させる。
- **winit soft spike 群**（`soft_main.rs`/`soft_host_main.rs`）は撤去候補。役割（softbuffer ラスタが動く実証）が runtime + 転用 main.rs probe に包摂される。ラスタ中核テスト（下記）を runtime へ移植した後に撤去する。

## 公開 API の変更（縮小を明示）

- `lib.rs` から撤去: `GpuFaultInjection`、`SurfaceAction`、`surface_action`。
- 維持: `is_renderable_extent`、`key_from_tao`、`modifiers_from_tao`、`EguiRuntime`、`EguiView`、`RuntimeError`、`RuntimeFrame`。
- `RuntimeFrame::inject_gpu_fault` を除去。
- `RuntimeError` を再形成: wgpu 由来の `GpuInitialization`/`SurfaceValidation`/`GpuOutOfMemory` を softbuffer の失敗モード（surface 生成・resize・present 等）を表す variant へ置換。`Tauri`/`ImeInitialization`/`NotInstalled`/`DuplicateWindow` は維持。
- 現行 API の外部消費者は転用する `main.rs` のみ（`grep` で確認済み）。SU2 以降の製品消費者は未存在ゆえ、この縮小に外部契約破壊は無い。

## テスト計画

- **バインド確定（初手 Red）**: runtime クレート内に `Context::new`+`Surface::new`+`resize`+`present` の最小コンパイルチェックを置き、rwh 0.6 バインドを一次証拠にする。fallback（万一 2 メジャー共存が判明した場合の対処）を注記。
- **ラスタ中核の不変条件**: `soft_host_main.rs` の `raster_core_matches_soft_probe_invariants`（`edge`/`blend_premultiplied`/`modulate`/`fill_mesh` の代表入力）を `raster.rs` へ移植。移植で失われる命題を孤立させない。
- **bilinear**: filter=LINEAR の texture を分数 uv でサンプルしたとき、隣接 texel の補間値が出ることを代表入力で固定（nearest との差分を明示）。
- **フォント先頭配置**: `EguiView::setup` 後の `FontDefinitions` で jp_font が Proportional/Monospace の index 0 にあることを config テストで固定。
- **視覚 + IME スモーク**: 転用 main.rs を実機起動し、製品規模テキスト（日本語 + 長パス）の AA 目視 parity と、IME preedit/候補/確定/Esc キャンセル/二重投入回帰なしをトレースで確認。

## 受け入れ条件（SU1）

ロードマップ「各 SU の受け入れ条件」の SU1 を、本設計の語で具体化する。

1. `EguiView` を差し替えるだけで任意 UI を softbuffer で描画できる（転用 main.rs がその実証）。
2. フォント単一化（jp_font 先頭）が config テストで機構化され、被覆 AA 品質（bilinear 適用後）が製品規模テキストで egui-wgpu と目視 parity。図形エッジ AA の残差は、不足が目視できなければ defer と記録。
3. IME preedit/候補/確定が softbuffer 上（runtime 経由）で正しい。
4. `gpu.rs`/`SurfaceAction`/`GpuFaultInjection` が撤去され、`cargo clippy`・runtime のテストが緑。
5. 撤去済み wgpu/glow probe（`main.rs` は転用、`glow_main.rs`/`glow_lifecycle_main.rs`/`glow_park_host_main.rs` は撤去）と winit soft spike の扱いが確定し、ビルドが緑。

## リスク

- **図形エッジの被覆 AA 不足**: verify-then-add で吸収するが、製品規模で不足が判明した場合の追加実装（supersample/解析的被覆）は非自明。SU1 内で完結させるか SU3 の結果表示へ送るかは、スモーク結果を見てから判断。
- **softbuffer present の隠しウィンドウ配送保証**: 隠れた HWND には `RedrawRequested` の配送保証が無い（spike の知見）。runtime は `RedrawRequested` を入力へ回さない構造で自己ループを既に回避しているが、hide→show 時の初回フレーム提示は SU2 のウィンドウライフサイクルと合わせて確認する（SU1 では転用 main.rs の可視起動で足る）。
- **Tauri 内部 API 追随**: `tauri-runtime-wry` の unstable feature 依存は #532 既知。SU1 で新たな追随は増えない（renderer の内側のみ変更）。

## スコープ外（SU1 では触らない）

- ウィンドウシェル・状態機械（Alt+Q/blur/フォーカス列/位置永続/初回フロー）は SU2。
- 検索体験・直 `Engine`・IPC 撤去は SU3。アイコンは SU4。updater は SU5。config 反映・終了保存は SU6。切替・配布は SU7。
- 再変換（IME reconvert）は WANT。SU1 で低コストに載らなければ触らない。
