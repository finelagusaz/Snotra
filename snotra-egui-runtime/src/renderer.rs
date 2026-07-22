use std::{collections::HashMap, num::NonZeroU32};

use tauri_runtime_wry::tao::dpi::PhysicalSize;

use crate::{
    RuntimeError, is_renderable_extent,
    raster::{self, CpuTexture},
};

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
        Ok(Self {
            surface,
            textures: HashMap::new(),
        })
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
        let mut buffer = self
            .surface
            .buffer_mut()
            .map_err(|e| RuntimeError::Present(e.to_string()))?;
        let (width, height) = (size.width as usize, size.height as usize);
        buffer.fill(CLEAR_COLOR);

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
