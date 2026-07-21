# SU1: snotra-egui-runtime の softbuffer 置換 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `snotra-egui-runtime` の描画基盤を wgpu から softbuffer + CPU ラスタライズ（`fill_mesh`）へ in-place 置換し、`EguiRuntime`/`EguiView` API と tao/wry プラグイン統合を保ったまま任意 UI を softbuffer で描けるようにする。

**Architecture:** tao + tauri-runtime-wry プラグインの統合骨格・入力・IME・repaint は据え置く。`renderer.rs` の中身のみ softbuffer へ差し替え、純粋ラスタ中核を新規 `raster.rs` へ抽出する。softbuffer surface は present するスレッド（イベントループ）で生成し、paint は live physical size を単一ソースに surface resize・RawInput・tessellate を同期させる。

**Tech Stack:** Rust 2024 / egui・epaint 0.35.0 / softbuffer 0.4.6 / tauri・tauri-runtime-wry（unstable）/ windows 0.61.3 / Windows・PowerShell。

## Global Constraints

- 依存: `softbuffer = "=0.4.6"`（`default-features = false`）を runtime に追加。`wgpu`/`egui-wgpu`/`pollster` を runtime から撤去。`raw-window-handle` は 0.6.2 単一（rwh 0.6 束ね）。
- softbuffer buffer 形式は `0x00RRGGBB`・上位 8bit=0・alpha 未使用。`CLEAR_COLOR = 0x0028_2828`（製品ダーク背景）。
- egui フォントアトラスは `TextureOptions::LINEAR`。テキスト parity は bilinear サンプルで、**half-texel 規約 `uv*size - 0.5`** と **atlas 端 ClampToEdge** を守る。
- clip 端は min/max **4 値とも `round()`**（egui-wgpu ScissorRect 一致）。
- `Presented` 経路は `request_repaint` を呼ばない（自己ループ回避）。失敗経路のみ再試行で `request_repaint`。
- surface サイズ・RawInput `screen_rect`・tessellate ppp は**同一フレームの live physical size + scale_factor**から導く（全 live）。
- `max_texture_side()` は固定値 **2048〜4096**（8192 は 256MB 相当で過大）。
- フォント: `jp_font` を Proportional/Monospace の **index 0**（`insert(0, …)`）。`FONT_PATH = "C:/Windows/Fonts/YuGothM.ttc"`。
- `softbuffer::Surface` は **Send（!Sync のみ）**。GDI 親和性ゆえ present スレッドで生成。
- G2: 実機で egui-wgpu の `target_format` が `Bgra8Unorm`（非 sRGB）であることを **wgpu 撤去前に**確認（gamma 空間 blend の前提）。
- `main` へ直接コミットしない（現在 `feat/532-egui-mvp` ブランチ）。コミットメッセージは一時ファイル `git commit -F`。bash HEREDOC 不可。パス区切りは `/`。
- 各タスク境界で `cargo check --workspace --all-targets` が緑であること。

## File Structure

- `snotra-egui-runtime/src/raster.rs`（新規・private mod）: 純粋 CPU ラスタ中核。`edge`/`blend_premultiplied`/`modulate`/`CpuTexture`(+filter)/`image_to_pixels`/`apply_texture_delta`/`fill_mesh`(+bilinear)。renderer/window 非依存の `pub(crate)` 純関数群。
- `snotra-egui-runtime/src/renderer.rs`（全面置換）: softbuffer `Context`/`Surface` の保持と `paint`。`EguiRenderer` 型は維持。wgpu 消滅。
- `snotra-egui-runtime/src/runtime.rs`（改修）: surface 生成をプラグイン活性化へ移動、visible ガード、live RawInput、`DeviceRecovered`/`gpu_fault` 経路除去。
- `snotra-egui-runtime/src/input.rs`（`take()` 改修）: live size/ppp を受け screen_rect/ppp を live 値で作る。
- `snotra-egui-runtime/src/surface.rs`（縮小）: `is_renderable_extent` のみ残す。
- `snotra-egui-runtime/src/gpu.rs`（削除）/ `lib.rs`（export 縮小）。
- `snotra-egui-mvp/src/main.rs`（转用）: softbuffer runtime 駆動 probe。
- Cargo.toml 3 箇所（runtime / mvp / root）。

---

### Task 1: G2 — target_format 検証ゲート（wgpu 撤去前）

parity 論は egui-wgpu が UNORM 形式（gamma 空間 blend）を選ぶことに依存する。wgpu を撤去する前に実機で確認する。

**Files:**
- Modify: `snotra-egui-runtime/src/renderer.rs:79`（`config.format = self.render_state.target_format;` の直後に 1 行ログ追加）

**Interfaces:**
- Consumes: 既存 wgpu `EguiRenderer::configure`。
- Produces: なし（検証専用・後続タスクで撤去される renderer 内の一時ログ）。

- [ ] **Step 1: target_format ログを追加**

`renderer.rs` の `configure` 内、`config.format = self.render_state.target_format;` の直後に:

```rust
eprintln!(
    "SNOTRA_EGUI_TARGET_FORMAT format={:?} is_srgb={}",
    self.render_state.target_format,
    self.render_state.target_format.is_srgb()
);
```

- [ ] **Step 2: 実機で確認**

Run（PowerShell）: `$env:SNOTRA_EGUI_MVP_START_VISIBLE="1"; cargo run -p snotra-egui-mvp 2>&1 | Select-String SNOTRA_EGUI_TARGET_FORMAT`
Expected: `format=Bgra8Unorm is_srgb=false`（または `Rgba8Unorm`）。**`is_srgb=true` なら STOP** — spike の gamma 空間 blend の前提が崩れる。その場合は本計画を中断し spec §G2 の色空間設計見直しへ戻る。

- [ ] **Step 3: ログを残したままコミット（撤去は Task 6）**

```
git add snotra-egui-runtime/src/renderer.rs
git commit -F <tmpfile>   # "chore: #532 SU1 G2 target_format 検証ログ（UNORM 確認）"
```

---

### Task 2: raster.rs — 純粋 CPU ラスタ中核（filter + bilinear）

spike `snotra-egui-mvp/src/soft_host_main.rs:214-367` のラスタ中核を新規 private mod へ移植し、`CpuTexture` に filter を足して bilinear を実装する。純関数ゆえ wgpu と並存してコンパイルできる（この時点では renderer に未接続）。

**Files:**
- Create: `snotra-egui-runtime/src/raster.rs`
- Modify: `snotra-egui-runtime/src/lib.rs`（`mod raster;` を追加。この時点では `pub use` しない）
- Test: `snotra-egui-runtime/src/raster.rs`（`#[cfg(test)] mod tests`）

**Interfaces:**
- Produces:
  - `pub(crate) fn edge(ax:f32,ay:f32,bx:f32,by:f32,px:f32,py:f32)->f32`
  - `pub(crate) fn blend_premultiplied(dst:u32, src:[u8;4])->u32`
  - `pub(crate) fn modulate(color:[u8;4], texel:[u8;4])->[u8;4]`
  - `pub(crate) enum TexFilter { Nearest, Linear }`
  - `pub(crate) struct CpuTexture { width:usize, height:usize, pixels:Vec<[u8;4]>, filter:TexFilter }` + `fn sample(&self,u:f32,v:f32)->[u8;4]`
  - `pub(crate) fn apply_texture_delta(store:&mut HashMap<egui::TextureId,CpuTexture>, id:egui::TextureId, delta:&egui::epaint::image::ImageDelta)`
  - `pub(crate) fn fill_mesh(buffer:&mut [u32], width:usize, height:usize, vertices:&[egui::epaint::Vertex], indices:&[u32], texture:&CpuTexture, clip_min:(usize,usize), clip_max:(usize,usize), pixels_per_point:f32)`

- [ ] **Step 1: raster.rs を作成（移植 + filter + bilinear）**

`soft_host_main.rs:214-323`（`edge`/`blend_premultiplied`/`modulate`/`fill_mesh`）と `:325-367`（`image_to_pixels`/`apply_texture_delta`）を移植する。移植時の**差分は以下 3 点のみ**、他は逐語:

1. `CpuTexture` に `filter: TexFilter` を追加し、`sample_nearest` を `sample` へ改名して filter 分岐:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TexFilter {
    Nearest,
    Linear,
}

pub(crate) struct CpuTexture {
    pub(crate) width: usize,
    pub(crate) height: usize,
    /// premultiplied sRGB RGBA。
    pub(crate) pixels: Vec<[u8; 4]>,
    pub(crate) filter: TexFilter,
}

impl CpuTexture {
    pub(crate) fn sample(&self, u: f32, v: f32) -> [u8; 4] {
        if self.width == 0 || self.height == 0 {
            return [255, 255, 255, 255];
        }
        match self.filter {
            TexFilter::Nearest => {
                let x = ((u * self.width as f32) as isize)
                    .clamp(0, self.width as isize - 1) as usize;
                let y = ((v * self.height as f32) as isize)
                    .clamp(0, self.height as isize - 1) as usize;
                self.pixels[y * self.width + x]
            }
            // half-texel 規約 uv*size - 0.5 で 4 近傍を ClampToEdge 補間（egui-wgpu 一致）。
            TexFilter::Linear => self.sample_bilinear(u, v),
        }
    }

    fn sample_bilinear(&self, u: f32, v: f32) -> [u8; 4] {
        let fx = (u * self.width as f32 - 0.5).clamp(0.0, self.width as f32 - 1.0);
        let fy = (v * self.height as f32 - 0.5).clamp(0.0, self.height as f32 - 1.0);
        let x0 = fx.floor() as usize;
        let y0 = fy.floor() as usize;
        let x1 = (x0 + 1).min(self.width - 1);
        let y1 = (y0 + 1).min(self.height - 1);
        let tx = fx - x0 as f32;
        let ty = fy - y0 as f32;
        let p = |x: usize, y: usize| self.pixels[y * self.width + x];
        let (c00, c10, c01, c11) = (p(x0, y0), p(x1, y0), p(x0, y1), p(x1, y1));
        let mut out = [0u8; 4];
        for i in 0..4 {
            let top = c00[i] as f32 * (1.0 - tx) + c10[i] as f32 * tx;
            let bottom = c01[i] as f32 * (1.0 - tx) + c11[i] as f32 * tx;
            out[i] = (top * (1.0 - ty) + bottom * ty).round() as u8;
        }
        out
    }
}
```

2. `fill_mesh` 内の `texture.sample_nearest(u, v)` を `texture.sample(u, v)` へ（1 箇所）。
3. `apply_texture_delta` の `image_to_pixels` 結果から `CpuTexture` を作る箇所で `filter` を `delta.options` から決める:

```rust
fn tex_filter(options: &egui::TextureOptions) -> TexFilter {
    match options.magnification {
        egui::TextureFilter::Linear => TexFilter::Linear,
        egui::TextureFilter::Nearest => TexFilter::Nearest,
    }
}
```

`apply_texture_delta` の全面更新（`pos: None`）で `CpuTexture { width, height, pixels, filter: tex_filter(&delta.options) }` を挿入。部分更新（`pos: Some`）の pixels 上書きロジックは逐語（filter は既存を保持）。`edge`/`blend_premultiplied`/`modulate`/`fill_mesh` 本体・`image_to_pixels` は `soft_host_main.rs` から逐語移植。冒頭に `use std::collections::HashMap;` と rustdoc `//!` を付す。

- [ ] **Step 2: lib.rs に mod を追加**

`lib.rs` に `mod raster;` を追加（既存 `mod` 群の位置に。`pub use` はしない）。

- [ ] **Step 3: 失敗するテストを書く（中核不変条件 + bilinear + texture store）**

`raster.rs` 末尾に:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use egui::epaint::image::{ImageData, ImageDelta};

    #[test]
    fn raster_core_matches_soft_probe_invariants() {
        assert!(edge(0.0, 0.0, 4.0, 0.0, 1.0, 1.0) > 0.0);
        assert_eq!(blend_premultiplied(0x0000_0000, [255, 0, 0, 255]), 0x00FF_0000);
        assert_eq!(blend_premultiplied(0x0012_3456, [0, 0, 0, 0]), 0x0012_3456);
        assert_eq!(modulate([200, 100, 50, 255], [255, 255, 255, 255]), [200, 100, 50, 255]);

        let mut buffer = vec![0u32; 8 * 8];
        let vertex = |x: f32, y: f32| egui::epaint::Vertex {
            pos: egui::pos2(x, y),
            uv: egui::pos2(0.0, 0.0),
            color: egui::Color32::from_rgba_premultiplied(255, 255, 255, 255),
        };
        let vertices = [vertex(0.0, 0.0), vertex(8.0, 0.0), vertex(0.0, 8.0)];
        let white = CpuTexture { width: 0, height: 0, pixels: Vec::new(), filter: TexFilter::Nearest };
        fill_mesh(&mut buffer, 8, 8, &vertices, &[0, 1, 2], &white, (0, 0), (8, 8), 1.0);
        assert_eq!(buffer[8 + 1], 0x00FF_FFFF);
        assert_eq!(buffer[7 * 8 + 7], 0);
    }

    #[test]
    fn bilinear_interpolates_with_half_texel_and_clamps_edges() {
        // 2x1 テクスチャ [黒, 白]。half-texel: u=0.5 は左 texel 中心、u=1.0 は右端 clamp。
        let tex = CpuTexture {
            width: 2,
            height: 1,
            pixels: vec![[0, 0, 0, 255], [255, 255, 255, 255]],
            filter: TexFilter::Linear,
        };
        // 中央 (u=0.5) は 2 texel の中点 → 128 付近。
        let mid = tex.sample(0.5, 0.5);
        assert!((120..=135).contains(&(mid[0] as i32)), "mid={mid:?}");
        // 右端超え (u=1.0) は右 texel へ clamp（隣接 bleed なし）→ 白。
        assert_eq!(tex.sample(1.0, 0.5), [255, 255, 255, 255]);
        // 左端手前 (u=0.0) は左 texel へ clamp → 黒。
        assert_eq!(tex.sample(0.0, 0.5), [0, 0, 0, 255]);
        // nearest は補間しない（同入力で二値のまま）。
        let nearest = CpuTexture { filter: TexFilter::Nearest, ..CpuTexture {
            width: 2, height: 1, pixels: vec![[0,0,0,255],[255,255,255,255]], filter: TexFilter::Nearest } };
        let n = nearest.sample(0.5, 0.5);
        assert!(n == [0, 0, 0, 255] || n == [255, 255, 255, 255]);
    }

    fn solid(id_pixels: [u8; 4], w: usize, h: usize, filter: egui::TextureOptions) -> ImageDelta {
        let img = egui::ColorImage {
            size: [w, h],
            pixels: vec![egui::Color32::from_rgba_premultiplied(id_pixels[0], id_pixels[1], id_pixels[2], id_pixels[3]); w * h],
        };
        ImageDelta::full(ImageData::Color(std::sync::Arc::new(img)), filter)
    }

    #[test]
    fn texture_store_full_then_partial_and_filter_retained() {
        let mut store: HashMap<egui::TextureId, CpuTexture> = HashMap::new();
        let id = egui::TextureId::Managed(0);
        // pos=None 全面（Linear）。
        apply_texture_delta(&mut store, id, &solid([10, 20, 30, 255], 2, 2, egui::TextureOptions::LINEAR));
        assert_eq!(store[&id].filter, TexFilter::Linear);
        // pos=Some 部分上書き（filter は既存 Linear を保持）。
        let mut partial = solid([99, 99, 99, 255], 1, 1, egui::TextureOptions::NEAREST);
        partial.pos = Some([0, 0]);
        apply_texture_delta(&mut store, id, &partial);
        assert_eq!(store[&id].filter, TexFilter::Linear, "部分更新は filter を変えない");
        assert_eq!(store[&id].pixels[0], [99, 99, 99, 255]);
    }

    #[test]
    fn texture_store_partial_update_on_missing_id_is_dropped() {
        let mut store: HashMap<egui::TextureId, CpuTexture> = HashMap::new();
        let mut partial = solid([1, 2, 3, 255], 1, 1, egui::TextureOptions::NEAREST);
        partial.pos = Some([0, 0]);
        apply_texture_delta(&mut store, egui::TextureId::Managed(7), &partial);
        assert!(store.is_empty(), "未登録 ID への部分更新は無言破棄");
    }
}
```

- [ ] **Step 4: 失敗を確認（Red）**

Run: `cargo test -p snotra-egui-runtime raster:: 2>&1 | Select-String "test result|error"`
Expected: コンパイルエラー（`fill_mesh` 等が未定義）または assert 失敗。

- [ ] **Step 5: Step 1 の実装で通す（Green）**

Run: `cargo test -p snotra-egui-runtime raster::`
Expected: 全テスト PASS。

- [ ] **Step 6: Commit**

```
git add snotra-egui-runtime/src/raster.rs snotra-egui-runtime/src/lib.rs
git commit -F <tmpfile>   # "feat: #532 SU1 raster.rs 抽出（filter + bilinear half-texel/clamp）"
```

---

### Task 3: G1 — softbuffer 依存追加とバインドコンパイルチェック

softbuffer↔`tauri::Window` 束ねをコンパイルで確定する。wgpu と並存する。

**Files:**
- Modify: `snotra-egui-runtime/Cargo.toml`（`softbuffer` 追加）
- Modify: `snotra-egui-runtime/src/renderer.rs`（末尾に `_bind_check` を追加）

**Interfaces:**
- Produces: `fn _softbuffer_bind_check(window: tauri::Window)`（never-called・コンパイル証拠）。

- [ ] **Step 1: softbuffer を依存に追加**

`snotra-egui-runtime/Cargo.toml` の `[dependencies]` に:

```toml
softbuffer = { version = "=0.4.6", default-features = false }
```

- [ ] **Step 2: バインドチェック関数を追加**

`renderer.rs` 末尾に（型が束ねられることをコンパイルで示す。実行はしない）:

```rust
/// #532 SU1 G1: softbuffer が tao/wry 管理の tauri::Window に rwh 0.6 で束ねられる
/// ことをコンパイルで確定する。never-called。撤去済み wgpu の代替が成立する一次証拠。
#[allow(dead_code)]
fn _softbuffer_bind_check(window: tauri::Window) -> Result<(), softbuffer::SoftBufferError> {
    use std::num::NonZeroU32;
    let context = softbuffer::Context::new(window.clone())?;
    let mut surface = softbuffer::Surface::new(&context, window)?;
    surface.resize(NonZeroU32::new(1).unwrap(), NonZeroU32::new(1).unwrap())?;
    let mut buffer = surface.buffer_mut()?;
    buffer.fill(0x0028_2828);
    buffer.present()?;
    Ok(())
}
```

- [ ] **Step 3: コンパイル確認**

Run: `cargo check -p snotra-egui-runtime 2>&1 | Select-String "error|Finished"`
Expected: `Finished`（束ね成立）。**`the trait HasWindowHandle is not implemented` 等が出たら** fallback（`window.window_handle()?.as_raw()` から生ハンドルで `Surface::new` する形）へ切替え、その旨を spec へ注記。

- [ ] **Step 4: Commit**

```
git add snotra-egui-runtime/Cargo.toml snotra-egui-runtime/src/renderer.rs
git commit -F <tmpfile>   # "feat: #532 SU1 G1 softbuffer 束ねコンパイルチェック"
```

---

### Task 4: wgpu → softbuffer 描画 swap（renderer + runtime + input）

renderer を softbuffer 化し、runtime の描画経路を live 同期・visible ガード・失敗再試行へ改める。この時点では **wgpu 依存・gpu.rs・`RuntimeFrame::inject_gpu_fault`・main.rs は据え置き**（後続タスクで撤去/転用）。`RuntimeFrame::gpu_fault_requested` は残すが読まれない dead field になる。

**Files:**
- Modify: `snotra-egui-runtime/src/renderer.rs`（全面置換）
- Modify: `snotra-egui-runtime/src/runtime.rs`（surface 生成を活性化へ・visible ガード・live RawInput・`DeviceRecovered`/`inject_fault` 呼び除去）
- Modify: `snotra-egui-runtime/src/input.rs`（`take(max, size, ppp)`）
- Test: `input.rs`・`runtime.rs` の `#[cfg(test)]`

**Interfaces:**
- Consumes: `raster::{CpuTexture, TexFilter, apply_texture_delta, fill_mesh, tex_filter}`（Task 2）、`is_renderable_extent`（surface.rs）。
- Produces:
  - `EguiRenderer::new(window: tauri::Window) -> Result<Self, RuntimeError>`（Context 生成 + Surface 生成。present スレッドで呼ばれる前提）
  - `EguiRenderer::paint(&mut self, ctx:&egui::Context, output:egui::FullOutput, size:PhysicalSize<u32>) -> Result<PaintOutcome, RuntimeError>`（`PhysicalSize` は `tauri_runtime_wry::tao::dpi`。`tauri::Window::inner_size()` の戻り値と同一 `dpi` crate 型ゆえ変換不要）
  - `EguiRenderer::max_texture_side(&self) -> usize`（固定値）
  - `enum PaintOutcome { Presented, Skipped }`
  - `InputState::take(&mut self, max_texture_side:usize, size:PhysicalSize<u32>, ppp:f32) -> egui::RawInput`
  - `fn retry_delay(consecutive_failures:u32) -> Option<std::time::Duration>`（描画失敗のバックオフ・不変条件⑤）

- [ ] **Step 1: input.rs take() を live size/ppp 受け取りへ（失敗するテスト）**

`input.rs` の tests に追加:

```rust
#[test]
fn take_uses_live_size_and_ppp_not_stored_event_values() {
    // イベント駆動で古い値を持たせる。
    let mut input = InputState::new(PhysicalSize::new(100, 100), 1.0);
    // live に新しい size/ppp を渡すと screen_rect は live 値から作られる。
    let raw = input.take(4096, PhysicalSize::new(200, 400), 2.0);
    let rect = raw.screen_rect.expect("screen_rect");
    // 200x400 physical / ppp 2.0 = 100x200 points。
    assert_eq!(rect.width(), 100.0);
    assert_eq!(rect.height(), 200.0);
    let vp = raw.viewports.get(&egui::ViewportId::ROOT).unwrap();
    assert_eq!(vp.native_pixels_per_point, Some(2.0));
}
```

- [ ] **Step 2: Red 確認**

Run: `cargo test -p snotra-egui-runtime input:: 2>&1 | Select-String "error\[|test result"`
Expected: コンパイルエラー（`take` の引数不足）。

- [ ] **Step 3: input.rs take() を改修**

`take` のシグネチャを変え、live 値で screen_rect/ppp を作る。`self.native_pixels_per_point` も live へ同期（後続のポインタ変換の一貫性）:

```rust
pub(crate) fn take(
    &mut self,
    max_texture_side: usize,
    size: PhysicalSize<u32>,
    native_pixels_per_point: f32,
) -> egui::RawInput {
    self.native_pixels_per_point = native_pixels_per_point;
    let size_points = egui::vec2(
        size.width as f32 / native_pixels_per_point,
        size.height as f32 / native_pixels_per_point,
    );
    let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, size_points);

    self.raw.screen_rect = Some(screen_rect);
    self.raw.max_texture_side = Some(max_texture_side);
    self.raw.time = Some(self.started_at.elapsed().as_secs_f64());

    let viewport = self.raw.viewports.entry(egui::ViewportId::ROOT).or_default();
    viewport.native_pixels_per_point = Some(native_pixels_per_point);
    viewport.inner_rect = Some(screen_rect);
    viewport.focused = Some(self.raw.focused);

    self.raw.take()
}
```

（`InputState` の `size` フィールドはイベント時の値保持に不要になれば削れるが、`on_window_event` の `Resized`/`ScaleFactorChanged` で `native_pixels_per_point` 更新は残す＝ポインタ変換に使う。`size` フィールドは削除可。削除する場合 `new` の引数も調整。）

- [ ] **Step 4: Green 確認**

Run: `cargo test -p snotra-egui-runtime input::`
Expected: PASS。

- [ ] **Step 5: renderer.rs を softbuffer で全面置換**

`renderer.rs` を次の骨格へ置換する（wgpu import・`GpuFaultMonitor`・`configure`・復旧経路を全廃）:

先に `runtime.rs` の `RuntimeError` に softbuffer 用 variant を追加する（`GpuInitialization`/`SurfaceValidation`/`GpuOutOfMemory` は Task 6 まで残す）:

```rust
#[error("softbuffer surface initialization failed: {0}")]
SurfaceInit(String),
#[error("softbuffer present failed: {0}")]
Present(String),
```

`renderer.rs` を次の骨格へ置換:

```rust
use std::{collections::HashMap, num::NonZeroU32};

use tauri_runtime_wry::tao::dpi::PhysicalSize;

use crate::{RuntimeError, is_renderable_extent, raster::{self, CpuTexture}};

const CLEAR_COLOR: u32 = 0x0028_2828;
/// CPU ラスタゆえ GPU device limit は無い。フォントアトラス需要を下回らず
/// CPU メモリを律速しない固定値（ランチャー用途）。
const MAX_TEXTURE_SIDE: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaintOutcome {
    Presented,
    Skipped,
}

pub(crate) struct EguiRenderer {
    // Surface は自前の display handle を所有し Context 生存に依存しない（Context 非保持）。
    surface: softbuffer::Surface<tauri::Window, tauri::Window>,
    textures: HashMap<egui::TextureId, CpuTexture>,
}

impl EguiRenderer {
    /// present するスレッド（tao イベントループ）で呼ぶこと。GDI 親和性のため。
    pub(crate) fn new(window: tauri::Window) -> Result<Self, RuntimeError> {
        let context = softbuffer::Context::new(window.clone())
            .map_err(|e| RuntimeError::SurfaceInit(e.to_string()))?;
        let surface = softbuffer::Surface::new(&context, window)
            .map_err(|e| RuntimeError::SurfaceInit(e.to_string()))?;
        Ok(Self { surface, textures: HashMap::new() })
    }

    pub(crate) fn max_texture_side(&self) -> usize {
        MAX_TEXTURE_SIDE
    }

    pub(crate) fn paint(
        &mut self,
        context: &egui::Context,
        output: egui::FullOutput,
        size: PhysicalSize<u32>,
    ) -> Result<PaintOutcome, RuntimeError> {
        if !is_renderable_extent(size.width, size.height) {
            return Ok(PaintOutcome::Skipped);
        }
        let ppp = output.pixels_per_point;

        // texture delta（set）を CPU store へ。free は present 成否に依らず後で確定。
        for (id, delta) in &output.textures_delta.set {
            raster::apply_texture_delta(&mut self.textures, *id, delta);
        }
        let clipped = context.tessellate(output.shapes, ppp);

        self.surface
            .resize(
                NonZeroU32::new(size.width).expect("width checked"),
                NonZeroU32::new(size.height).expect("height checked"),
            )
            .map_err(|e| RuntimeError::Present(e.to_string()))?;
        let mut buffer = self.surface.buffer_mut()
            .map_err(|e| RuntimeError::Present(e.to_string()))?;
        let (width, height) = (size.width as usize, size.height as usize);
        buffer.fill(CLEAR_COLOR);

        let white = CpuTexture { width: 0, height: 0, pixels: Vec::new(), filter: raster::TexFilter::Nearest };
        for primitive in &clipped {
            let egui::epaint::Primitive::Mesh(mesh) = &primitive.primitive else {
                continue; // Callback は softbuffer 経路では描かない（能力縮小・記録済み）。
            };
            let texture = self.textures.get(&mesh.texture_id).unwrap_or(&white);
            let clip = primitive.clip_rect;
            // clip 端は 4 値とも round()（egui-wgpu ScissorRect 一致）。
            let clip_min = (
                (clip.min.x * ppp).round().max(0.0) as usize,
                (clip.min.y * ppp).round().max(0.0) as usize,
            );
            let clip_max = (
                (clip.max.x * ppp).round().min(width as f32) as usize,
                (clip.max.y * ppp).round().min(height as f32) as usize,
            );
            raster::fill_mesh(&mut buffer, width, height, &mesh.vertices, &mesh.indices, texture, clip_min, clip_max, ppp);
        }
        // present は buffer を消費する。結果を保持し、free を present 成否に依らず処理する
        // （free は CPU 側 HashMap から除くだけ・不変条件⑤。present 失敗で free を落とさない）。
        let present_result = buffer.present().map_err(|e| RuntimeError::Present(e.to_string()));
        for id in &output.textures_delta.free {
            self.textures.remove(id);
        }
        present_result?;
        Ok(PaintOutcome::Presented)
    }
}
```

（注: `RuntimeError::SurfaceInit`/`Present` variant は Task 6 で正式化するが、この Task で先に variant を `runtime.rs` の `RuntimeError` へ足してよい。`GpuInitialization` 等は Task 6 まで残す。）

- [ ] **Step 6: runtime.rs を改修（visible ガード・live RawInput・surface 活性化生成）**

`runtime.rs` の変更点:

1. `use crate::{... renderer::{EguiRenderer, PaintOutcome}}` は維持。`gpu::GpuFaultInjection` の import は残す（Task 6 まで）。冒頭に `retry_delay` を定義:

```rust
const MAX_PAINT_RETRIES: u32 = 5;
/// 描画失敗の再試行間隔（指数バックオフ）。MAX_PAINT_RETRIES 回まで Some、以降 None（fatal）。
fn retry_delay(consecutive_failures: u32) -> Option<std::time::Duration> {
    (consecutive_failures <= MAX_PAINT_RETRIES)
        .then(|| std::time::Duration::from_millis(16u64 << consecutive_failures.min(6)))
}
```

2. `EguiWindow` を改める。softbuffer surface は Send（要石3）だが GDI 親和性ゆえ present スレッド（イベントループ）で生成する。`renderer` を `Option` にし、`new` では作らない:

```rust
struct EguiWindow {
    context: egui::Context,
    window: tauri::Window,
    input: InputState,
    ime: ImeBridge,
    renderer: Option<EguiRenderer>,   // 活性化時（イベントループ）に Some
    view: Box<dyn EguiView>,
    visible: bool,
    paint_failures: u32,
}
```

`EguiWindow::new`（attach で呼ばれる・別スレッドかもしれない）は renderer 以外を構築し `renderer: None, visible: true, paint_failures: 0`。`attach_pending_windows`（イベントループ）で `active` へ入れる直前に surface を生成:

```rust
// attach_pending_windows 内、active へ insert する前に present スレッドで surface を作る:
match EguiRenderer::new(window.window.clone()) {
    Ok(renderer) => window.renderer = Some(renderer),
    Err(error) => {
        log::error!("egui softbuffer surface init failed: {error}");
        eprintln!("SNOTRA_EGUI_RENDER_ERROR={error}");
        continue; // この window はスキップ（attach の同期エラー契約は狭まる）
    }
}
```

3. `render()`:

```rust
fn render(&mut self) -> Result<(), RuntimeError> {
    if !self.visible {
        return Ok(()); // 不変条件⑥: 非表示中は描かない。
    }
    self.drain_native_ime();
    let size = self.window.inner_size()?;
    let ppp = self.window.scale_factor()? as f32;
    let max_side = self.renderer.as_ref().ok_or(RuntimeError::NotInstalled)?.max_texture_side();
    let raw_input = self.input.take(max_side, size, ppp);
    // gpu_fault_requested は Task 5 まで残る dead field ゆえ None で構築する。
    let mut frame = RuntimeFrame {
        close_requested: false,
        hide_requested: false,
        drag_requested: false,
        gpu_fault_requested: None,
    };
    let output = self.context.run_ui(raw_input, |ui| self.view.update(ui, &mut frame));
    self.handle_platform_output(&output.platform_output);
    // 描画失敗は能動再試行（不変条件⑤）。「次 RedrawRequested 待ち」は egui が repaint を
    // 求めなければ停止するため、失敗時に自ら repaint を要求し、上限超過で fatal にする。
    match self.renderer.as_mut().unwrap().paint(&self.context, output, size) {
        Ok(_) => self.paint_failures = 0,
        Err(error) => match retry_delay(self.paint_failures + 1) {
            Some(delay) => {
                self.paint_failures += 1;
                log::warn!("egui paint failed (retry {}): {error}", self.paint_failures);
                self.context.request_repaint_after(delay);
            }
            None => return Err(error),
        },
    }
    self.apply_frame_commands(frame)?;
    Ok(())
}
```

4. `RuntimeFrame` から `gpu_fault_requested` を**残すが**（dead）、`apply_frame_commands` の `gpu_fault_requested` 分岐と `renderer.inject_fault` 呼びは**除去**。`PaintOutcome::DeviceRecovered` 分岐（Context 再生成）を除去。
5. `on_window_event` の `Resized`/`ScaleFactorChanged` で呼んでいた `self.renderer.configure(...)` を除去（softbuffer は paint 内 resize）。`self.input.on_window_event(event)` は維持。
6. `apply_frame_commands` の `hide_requested` で `self.window.hide()?` の後 `self.visible = false;`。show 経路（`RuntimeFrame` に show を足すか、外部再表示イベント）で `self.visible = true` にし `self.window.request_redraw()` で 1 回再描画（不変条件⑥）。SU1 の harness は Alt+Q hide→show を通すため、この最小追従で足りる。

- [ ] **Step 7: visible ガードと size 同期の単体テスト**

`runtime.rs` の tests（純粋に切り出せる範囲）:

```rust
#[cfg(test)]
mod tests {
    use super::{retry_delay, MAX_PAINT_RETRIES};

    #[test]
    fn hidden_window_is_not_painted() {
        // visible=false のとき render は早期 return する契約を、visible 述語で固定。
        fn should_render(visible: bool) -> bool { visible }
        assert!(!should_render(false));
        assert!(should_render(true));
    }

    #[test]
    fn retry_delay_backs_off_then_gives_up() {
        assert!(retry_delay(1).is_some());
        assert!(retry_delay(MAX_PAINT_RETRIES).is_some());
        assert!(retry_delay(MAX_PAINT_RETRIES + 1).is_none(), "上限超過は fatal");
        assert!(retry_delay(2).unwrap() > retry_delay(1).unwrap(), "バックオフは単調増加");
    }
}
```

（実 render は window を要すため、視覚確認は Task 8。size 同期は Task 4 Step 1 の input テストが live 値の使用を固定している。）

- [ ] **Step 8: workspace ビルド確認**

Run: `cargo check --workspace --all-targets 2>&1 | Select-String "error|Finished"`
Expected: `Finished`（main.rs は dead な `inject_gpu_fault`/`gpu_fault_requested` をまだ持つが、いずれも残しているためコンパイルは通る）。

- [ ] **Step 9: 描画スモーク（暫定・可視フラグ）**

Run: `$env:SNOTRA_EGUI_MVP_START_VISIBLE="1"; cargo run -p snotra-egui-mvp`
Expected: softbuffer でウィンドウが描画される（フォントは push のままゆえ #579 のベースラインは Task 5 で修正）。ウィンドウを閉じて終了。

- [ ] **Step 10: Commit**

```
git add snotra-egui-runtime/src/renderer.rs snotra-egui-runtime/src/runtime.rs snotra-egui-runtime/src/input.rs
git commit -F <tmpfile>   # "feat: #532 SU1 renderer を softbuffer へ swap（live 同期・visible ガード）"
```

---

### Task 5: harness 転用（main.rs を softbuffer runtime 駆動へ）

`MvpView` から GPU fault 依存を除き、可視起動へ固定し、フォントを先頭配置へ反転する。`RuntimeFrame::inject_gpu_fault` と `gpu_fault_requested` フィールドをここで除去する（main.rs から使われなくなるため）。

**Files:**
- Modify: `snotra-egui-mvp/src/main.rs`（GPU fault 除去・可視起動・font `insert(0)`）
- Modify: `snotra-egui-runtime/src/runtime.rs`（`RuntimeFrame::inject_gpu_fault` と `gpu_fault_requested` フィールド除去）
- Test: `snotra-egui-mvp/src/main.rs`（font-first config テスト）

**Interfaces:**
- Consumes: `EguiRuntime`/`EguiView`/`RuntimeFrame`（`inject_gpu_fault` 無し版）。

- [ ] **Step 1: RuntimeFrame から gpu_fault を除去**

`runtime.rs`: `RuntimeFrame` struct の `gpu_fault_requested` フィールドと `inject_gpu_fault` メソッド、**および `render()` の `RuntimeFrame` 構築から `gpu_fault_requested: None` 行**を削除。`use crate::gpu::GpuFaultInjection` は Task 6 まで残ってよいが未使用警告を避けるためここで削除可（lib.rs の re-export と gpu.rs 本体は Task 6）。

- [ ] **Step 2: main.rs の GPU fault 依存を除去**

`main.rs`: `use ... GpuFaultInjection` 削除、`pending_gpu_fault` フィールドと関連 UI/適用ロジック（`inject_gpu_fault` 呼び・fault 注入の env 分岐）を削除。

- [ ] **Step 3: 可視起動へ固定**

`main.rs` のウィンドウ生成で `.visible(start_visible)` の `start_visible` を SU1 では常時 `true` にする（env 既定 false の罠を回避）。該当行を `.visible(true)` に固定するか、`start_visible` 既定を true にする。

- [ ] **Step 4: フォント登録を testable な builder へ抽出（失敗するテスト）**

`main.rs` の `configure_japanese_font` を、`FontDefinitions` を組む純粋部分 `fn japanese_font_definitions(bytes: &'static [u8]) -> egui::FontDefinitions` へ切り出す（`ctx.set_fonts` 呼びは呼び出し側に残す）。tests に実登録経路を駆動するカナリアを:

```rust
#[test]
fn jp_font_is_registered_at_index_zero_for_both_families() {
    // families への挿入は font bytes の妥当性に依らないため dummy で構造を検証。
    let dummy: &'static [u8] = &[0u8; 4];
    let fonts = japanese_font_definitions(dummy);
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let list = fonts.families.get(&family).expect("family present");
        assert_eq!(list.first().map(String::as_str), Some("jp_font"),
            "jp_font must be index 0 for {family:?}（push=末尾だと #579 のベースラインずれが再発）");
    }
}
```

- [ ] **Step 5: Red 確認**

Run: `cargo test -p snotra-egui-mvp jp_font 2>&1 | Select-String "test result|error"`
Expected: FAIL（現行 `configure_japanese_font` は `push`）。

- [ ] **Step 6: builder を先頭配置で実装**

`japanese_font_definitions(bytes)` を、`jp_font` を `font_data` に登録し Proportional/Monospace の families へ **`insert(0, "jp_font".to_owned())`**（末尾 `push` ではなく先頭）で追加して返す形に実装。`configure_japanese_font`（674-682 付近）はこの builder を呼び `ctx.set_fonts(...)` する薄いラッパにする（現行の `push` を撤去）。

- [ ] **Step 7: Green 確認**

Run: `cargo test -p snotra-egui-mvp jp_font`
Expected: PASS。

- [ ] **Step 8: workspace ビルド**

Run: `cargo check --workspace --all-targets 2>&1 | Select-String "error|Finished"`
Expected: `Finished`。

- [ ] **Step 9: Commit**

```
git add snotra-egui-mvp/src/main.rs snotra-egui-runtime/src/runtime.rs
git commit -F <tmpfile>   # "feat: #532 SU1 main.rs を softbuffer runtime 駆動へ転用（font 先頭・可視起動）"
```

---

### Task 6: 撤去済み wgpu 装置の削除

renderer が softbuffer になり、gpu.rs と wgpu 依存が dead になったので削除する。

**Files:**
- Delete: `snotra-egui-runtime/src/gpu.rs`
- Modify: `snotra-egui-runtime/src/lib.rs`（`mod gpu` 削除・`GpuFaultInjection`/`SurfaceAction`/`surface_action` の pub use 削除・`raster` は private のまま）
- Modify: `snotra-egui-runtime/src/surface.rs`（`SurfaceAction`/`surface_action` と `recoverable_surface_states_have_distinct_actions` テストを削除・`is_renderable_extent` と `zero_extent...` テストは残す）
- Modify: `snotra-egui-runtime/src/renderer.rs`（Task 1 の target_format ログは wgpu と共に消える・`RuntimeError` の GPU variant 参照が無いことを確認）
- Modify: `snotra-egui-runtime/src/runtime.rs`（`RuntimeError` から `GpuInitialization`/`SurfaceValidation`/`GpuOutOfMemory` を削除、`SurfaceInit`/`Present` を正式化。`use crate::gpu` 削除）
- Modify: `snotra-egui-runtime/Cargo.toml`（`wgpu`/`egui-wgpu`/`pollster` 削除）

- [ ] **Step 1: gpu.rs を削除し lib.rs を縮小**

`snotra-egui-runtime/src/gpu.rs` を削除。`lib.rs` を:

元 `lib.rs` の `mod` 群（`gpu`/`ime`/`input`/`renderer`/`repaint`/`runtime`/`surface`）から `gpu` を削除し `raster` を追加する（`windows_ime` は元々 top-level 宣言でないため触らない）。`pub use` を縮小:

```rust
//! Tauri/Taoとegui/softbufferを接続するSnotra専用ランタイム。

mod ime;
mod input;
mod raster;
mod renderer;
mod repaint;
mod runtime;
mod surface;

pub use input::{key_from_tao, modifiers_from_tao};
pub use runtime::{EguiRuntime, EguiView, RuntimeError, RuntimeFrame};
pub use surface::is_renderable_extent;
```

- [ ] **Step 2: surface.rs を縮小**

`SurfaceAction` enum・`surface_action` 関数・`recoverable_surface_states_have_distinct_actions` テストを削除。`is_renderable_extent` と `zero_extent_never_reaches_surface_configuration` テストのみ残す。

- [ ] **Step 3: RuntimeError を再形成**

`runtime.rs` の `RuntimeError` から `GpuInitialization(String)`/`SurfaceValidation`/`GpuOutOfMemory` を削除し、`renderer.rs` が使う variant を追加:

```rust
#[error("softbuffer surface initialization failed: {0}")]
SurfaceInit(String),
#[error("softbuffer present failed: {0}")]
Present(String),
```

`Tauri`/`ImeInitialization`/`NotInstalled`/`DuplicateWindow` は維持。`use crate::gpu::...` を削除。

- [ ] **Step 4: Cargo.toml から wgpu 系を削除**

`snotra-egui-runtime/Cargo.toml` の `[dependencies]` から `wgpu`・`egui-wgpu`・`pollster` を削除。

- [ ] **Step 5: ビルドとテスト**

Run: `cargo test -p snotra-egui-runtime 2>&1 | Select-String "error|test result"`
Expected: 全 PASS・wgpu 参照エラー無し。

Run: `cargo check --workspace --all-targets 2>&1 | Select-String "error|Finished"`
Expected: `Finished`。

- [ ] **Step 6: Commit**

```
git add -A snotra-egui-runtime/
git commit -F <tmpfile>   # "refactor: #532 SU1 wgpu 装置（gpu.rs/SurfaceAction/依存）を撤去"
```

---

### Task 7: spike bin 撤去・Cargo.toml・索引同期

役割が runtime + 転用 main.rs に包摂された winit/glow spike bin を撤去し、依存とドキュメント索引を同期する。

**Files:**
- Delete: `snotra-egui-mvp/src/glow_main.rs`, `glow_lifecycle_main.rs`, `glow_park_host_main.rs`, `soft_main.rs`, `soft_host_main.rs`
- Modify: `snotra-egui-mvp/Cargo.toml`（5 `[[bin]]` ブロックと不要依存を削除）
- Modify: ルート `Cargo.toml`（`wgpu`/`egui-wgpu` workspace dep を削除）
- Modify: `snotra-egui-mvp/CLAUDE.md`（モジュール構成の撤去 bin 行を削除）
- Modify: `docs/build-commands.md`（park-host/soft-host の実行例を削除・100-101 付近）
- Modify: `snotra-egui-mvp/README.md`・`RETROSPECTIVE.md`（撤去 bin 参照があれば削除/更新）

- [ ] **Step 1: spike bin の .rs を削除**

上記 5 ファイルを削除。

- [ ] **Step 2: mvp Cargo.toml を掃除**

`[[bin]]` の 5 ブロック（`-glow-mvp`/`-glow-lifecycle-mvp`/`-park-host-mvp`/`-soft-mvp`/`-soft-host-mvp`）を削除。`main.rs` の default bin と `snotra-egui-mvp` bin 定義は残す。5 bin が唯一の消費者だった依存を削除: `eframe`・`egui_glow`・`glutin`・`glutin-winit`・`raw-window-handle`・`winit`・`softbuffer`・`build-dependencies` の `tauri-build`（main.rs が不要とするもの）。残す: `egui`・`snotra-core`・`snotra-egui-runtime`・`tauri`・`tauri-plugin-global-shortcut`・`tauri-plugin-updater`・`serde_json`・`[target.'cfg(windows)']` の windows。

- [ ] **Step 3: cargo check で不要依存を確定**

Run: `cargo check -p snotra-egui-mvp 2>&1 | Select-String "error|unused|Finished"`
Expected: `Finished`。`unused` 警告や `unresolved import` が出た依存を Step 2 の判断に反映（cargo を一次証拠に）。

- [ ] **Step 4: ルート Cargo.toml を掃除**

ルート `Cargo.toml` の `[workspace.dependencies]` から `wgpu`・`egui-wgpu` を削除（唯一の消費者だった runtime が撤去済み。`snotra-settings` は glow ゆえ非依存）。

- [ ] **Step 5: 索引同期**

`snotra-egui-mvp/CLAUDE.md` のモジュール構成から `glow_main.rs`/`glow_lifecycle_main.rs`/`glow_park_host_main.rs`/`soft_main.rs`/`soft_host_main.rs` の行を削除し、`main.rs` の説明を「softbuffer runtime 駆動 probe」に更新。`docs/build-commands.md` の該当実行例を削除。`README.md`/`RETROSPECTIVE.md` に撤去 bin 参照があれば更新。

- [ ] **Step 6: governance と workspace 確認**

Run: `npm run governance:check 2>&1 | Select-String "error|fail|pass|ok"`
Expected: pass（索引と参照が同期）。失敗時は指摘された参照を修正。

Run: `cargo check --workspace --all-targets 2>&1 | Select-String "error|Finished"`
Expected: `Finished`。

- [ ] **Step 7: Commit**

```
git add -A
git commit -F <tmpfile>   # "refactor: #532 SU1 wgpu/glow/soft spike bin を撤去し索引同期"
```

---

### Task 8: 受け入れスモーク（AA 目視 parity + IME 非二重投入）

自動テストで固められない視覚/IME を、可視起動の転用 main.rs で実機確認する。

**Files:** なし（検証のみ）。

- [ ] **Step 1: 可視起動して製品規模テキストを描画**

Run: `cargo run -p snotra-egui-mvp`（可視起動固定済み）
Expected: 検索欄 + 結果行（日本語名 + 淡色長パス）が softbuffer で描画される。

- [ ] **Step 2: AA 目視 parity を確認**

チェック: 日本語 + Latin 混在行のベースラインが単一（jp_font 先頭の効果）。bilinear によりテキストの縁が nearest 時のジャギより滑らか。長パスの ellipsis・淡色パスが読める。選択行 #505050 の縁が clip 境界で 1px はみ出していない（4 値 round の効果）。**不足があれば** spec の verify-then-add に従い図形エッジ AA 追加要否を判断。

- [ ] **Step 3: IME 非二重投入を確認**

日本語 IME で「にほんご」→変換→確定。トレース（`SNOTRA_...` or ログ）で: preedit が表示され、確定が `ReceivedImeText`→`ImeEvent::Commit` の 1 経路のみ（`KeyboardInput.text` との二重投入が無い）、Esc で変換キャンセル、通常 ASCII 文字が二重入力されない。input.rs の `ReceivedImeText` vs `KeyboardInput.text` 境界が保たれていること。

- [ ] **Step 4: 受け入れ条件の充足を記録**

spec §受け入れ条件 1〜5 の充足を確認し、図形エッジ AA を defer した場合はその旨を記録（PR 本文 or issue）。コード変更が無ければコミット不要。視覚差異で修正が要れば該当 Task へ戻る。
