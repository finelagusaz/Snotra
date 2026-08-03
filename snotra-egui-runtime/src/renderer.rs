use std::{collections::HashMap, num::NonZeroU32};

use tauri_runtime_wry::tao::dpi::PhysicalSize;

use crate::{
    RuntimeError, is_renderable_extent,
    raster::{self, CpuTexture},
};

/// view が色を決める前（起動直後の 1 枚目）と `set_clear_color` 呼び忘れのフォールバック。
/// **`snotra-core` の `default_background_color()` と同値であり、一致は機構が固定する**
/// ——`src-tauri/src/egui_shell/window_coordinator.rs` の
/// `runtime_fallback_matches_config_default_background` が両者を突き合わせる。この crate は
/// `snotra-core` に依存しないため、検査は**両方に依存する下流**（`src-tauri`）にしか置けない。
pub const CLEAR_COLOR: u32 = 0x0028_2828;

/// フレームの背景色を softbuffer の `0x00RRGGBB` へ畳む純関数。
///
/// **alpha 成分は載せない**——buffer が持てないためである。**ここへ来る `Color32` の RGB は
/// premultiply 済みでありうる**（消費側が `Color32::from_hex` の 8 桁 / 4 桁を通した場合。
/// 減衰はこの関数より上流で起きるので、ここが落とすのは alpha 成分だけである）。
/// `None`（view が `set_clear_color` を呼ぶ前・呼び忘れ）は `CLEAR_COLOR` へ落ちる。
fn clear_color_u32(color: Option<egui::Color32>) -> u32 {
    match color {
        Some(c) => ((c.r() as u32) << 16) | ((c.g() as u32) << 8) | c.b() as u32,
        None => CLEAR_COLOR,
    }
}
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
        Ok(Self {
            surface,
            textures: HashMap::new(),
        })
    }

    pub(crate) fn max_texture_side(&self) -> usize {
        MAX_TEXTURE_SIDE
    }

    /// `clear_color` は**そのフレームの view が決めた背景色**（`RuntimeFrame::set_clear_color`）。
    /// `run_ui` → `paint` の順に進むため、view が決めた色は同じフレームの `buffer.fill` に間に合う。
    pub(crate) fn paint(
        &mut self,
        context: &egui::Context,
        output: egui::FullOutput,
        size: PhysicalSize<u32>,
        clear_color: Option<egui::Color32>,
    ) -> Result<PaintOutcome, RuntimeError> {
        if !is_renderable_extent(size.width, size.height) {
            return Ok(PaintOutcome::Skipped);
        }
        // paint フェーズ計測（#628・#532 SU6.5 G3(b)）。env 未設定なら Instant も取らない
        // ——常時 2 回の時刻取得を入れないため（計測器が測定対象を汚さない）。
        let trace = std::env::var_os("SNOTRA_EGUI_PAINT_TRACE").is_some();
        let t_begin = trace.then(std::time::Instant::now);
        let ppp = output.pixels_per_point;

        // texture delta（set）を CPU store へ。free は present 成否に依らず後で確定。
        for (id, delta) in &output.textures_delta.set {
            raster::apply_texture_delta(&mut self.textures, *id, delta);
        }
        let clipped = context.tessellate(output.shapes, ppp);
        let t_tess = trace.then(std::time::Instant::now);

        self.surface
            .resize(
                NonZeroU32::new(size.width).expect("width checked"),
                NonZeroU32::new(size.height).expect("height checked"),
            )
            .map_err(|e| RuntimeError::Present(e.to_string()))?;
        let mut buffer = self
            .surface
            .buffer_mut()
            .map_err(|e| RuntimeError::Present(e.to_string()))?;
        let (width, height) = (size.width as usize, size.height as usize);
        buffer.fill(clear_color_u32(clear_color));

        let white = CpuTexture {
            width: 0,
            height: 0,
            pixels: Vec::new(),
            filter: raster::TexFilter::Nearest,
        };
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
            raster::fill_mesh(
                &mut buffer,
                width,
                height,
                &mesh.vertices,
                &mesh.indices,
                texture,
                clip_min,
                clip_max,
                ppp,
            );
        }
        let t_raster = trace.then(std::time::Instant::now);
        // present は buffer を消費する。結果を保持し、free を present 成否に依らず処理する
        // （free は CPU 側 HashMap から除くだけ・不変条件⑤。present 失敗で free を落とさない）。
        let present_result = buffer
            .present()
            .map_err(|e| RuntimeError::Present(e.to_string()));
        for id in &output.textures_delta.free {
            self.textures.remove(id);
        }
        present_result?;
        if let (Some(b), Some(t), Some(r)) = (t_begin, t_tess, t_raster) {
            eprintln!(
                "SNOTRA_EGUI_PAINT tess_ms={:.2} raster_ms={:.2} total_ms={:.2} meshes={} px={}",
                (t - b).as_secs_f64() * 1000.0,
                (r - t).as_secs_f64() * 1000.0,
                b.elapsed().as_secs_f64() * 1000.0,
                clipped.len(),
                size.width as u64 * size.height as u64,
            );
        }
        Ok(PaintOutcome::Presented)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `None` は従来の既定へ落ちる（view が色を決める前の 1 枚目・呼び忘れの安全網）。
    #[test]
    fn clear_color_falls_back_to_default_when_unset() {
        assert_eq!(clear_color_u32(None), CLEAR_COLOR);
    }

    /// softbuffer の buffer は `0x00RRGGBB` ゆえ **alpha 成分を載せられない**。
    /// **この層は premultiply を行わない**——config の `#RRGGBBAA` が減衰するのは消費側の
    /// `Color32::from_hex` であり、その命題は `src-tauri` の
    /// `background_color_premultiplies_alpha_rather_than_ignoring_it` が測る。
    #[test]
    fn clear_color_packs_rgb_and_drops_alpha() {
        assert_eq!(
            clear_color_u32(Some(egui::Color32::from_rgb(0x4A, 0x2B, 0x5C))),
            0x004A_2B5C
        );
        assert_eq!(
            clear_color_u32(Some(egui::Color32::from_rgba_premultiplied(
                0x12, 0x34, 0x56, 0x80
            ))),
            0x0012_3456
        );
    }
}
