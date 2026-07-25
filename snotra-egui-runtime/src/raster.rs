//! egui の Mesh を CPU 側でラスタライズする純関数群（softbuffer 提示用の中核）。
//!
//! #532 Phase 1 の技術スパイク（採用判断用の検証バイナリ・#660 で撤去）から移植。`renderer.rs` の
//! `EguiRenderer::paint` から呼ばれる（#532 SU1 Task 4 で配線）。

use std::collections::HashMap;

/// 2D エッジ関数（外積）。正なら点 p は a→b の左側にある。
pub(crate) fn edge(ax: f32, ay: f32, bx: f32, by: f32, px: f32, py: f32) -> f32 {
    (bx - ax) * (py - ay) - (by - ay) * (px - ax)
}

/// premultiplied sRGB 同士の over 合成。dst は 0x00RRGGBB、src は [r,g,b,a]。
pub(crate) fn blend_premultiplied(dst: u32, src: [u8; 4]) -> u32 {
    let inverse = 255 - src[3] as u32;
    let dst_r = (dst >> 16) & 0xFF;
    let dst_g = (dst >> 8) & 0xFF;
    let dst_b = dst & 0xFF;
    let r = (src[0] as u32 + dst_r * inverse / 255).min(255);
    let g = (src[1] as u32 + dst_g * inverse / 255).min(255);
    let b = (src[2] as u32 + dst_b * inverse / 255).min(255);
    (r << 16) | (g << 8) | b
}

/// 頂点色 × テクスチャの変調。どちらも premultiplied で、(c*t + 127) / 255。
pub(crate) fn modulate(color: [u8; 4], texel: [u8; 4]) -> [u8; 4] {
    let channel = |c: u8, t: u8| ((c as u16 * t as u16 + 127) / 255) as u8;
    [
        channel(color[0], texel[0]),
        channel(color[1], texel[1]),
        channel(color[2], texel[2]),
        channel(color[3], texel[3]),
    ]
}

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
            // half-texel 規約 uv*size - 0.5 で 4 近傍を ClampToEdge 補間(egui-wgpu 一致)。
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

/// 1 つの epaint Mesh を framebuffer へ描く。pos は物理ピクセル座標へ変換済みであること。
#[allow(clippy::too_many_arguments)]
pub(crate) fn fill_mesh(
    buffer: &mut [u32],
    width: usize,
    height: usize,
    vertices: &[egui::epaint::Vertex],
    indices: &[u32],
    texture: &CpuTexture,
    clip_min: (usize, usize),
    clip_max: (usize, usize),
    pixels_per_point: f32,
) {
    for triangle in indices.chunks_exact(3) {
        let v0 = &vertices[triangle[0] as usize];
        let v1 = &vertices[triangle[1] as usize];
        let v2 = &vertices[triangle[2] as usize];
        let (x0, y0) = (v0.pos.x * pixels_per_point, v0.pos.y * pixels_per_point);
        let (x1, y1) = (v1.pos.x * pixels_per_point, v1.pos.y * pixels_per_point);
        let (x2, y2) = (v2.pos.x * pixels_per_point, v2.pos.y * pixels_per_point);
        let area = edge(x0, y0, x1, y1, x2, y2);
        if area.abs() < f32::EPSILON {
            continue;
        }
        let min_x = x0.min(x1).min(x2).floor().max(clip_min.0 as f32) as usize;
        let min_y = y0.min(y1).min(y2).floor().max(clip_min.1 as f32) as usize;
        let max_x = (x0.max(x1).max(x2).ceil() as usize).min(clip_max.0).min(width);
        let max_y = (y0.max(y1).max(y2).ceil() as usize).min(clip_max.1).min(height);
        for y in min_y..max_y {
            let py = y as f32 + 0.5;
            for x in min_x..max_x {
                let px = x as f32 + 0.5;
                let w0 = edge(x1, y1, x2, y2, px, py);
                let w1 = edge(x2, y2, x0, y0, px, py);
                let w2 = edge(x0, y0, x1, y1, px, py);
                let inside = if area > 0.0 {
                    w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0
                } else {
                    w0 <= 0.0 && w1 <= 0.0 && w2 <= 0.0
                };
                if !inside {
                    continue;
                }
                let (b0, b1, b2) = (w0 / area, w1 / area, w2 / area);
                let u = v0.uv.x * b0 + v1.uv.x * b1 + v2.uv.x * b2;
                let v = v0.uv.y * b0 + v1.uv.y * b1 + v2.uv.y * b2;
                let c0 = v0.color.to_array();
                let c1 = v1.color.to_array();
                let c2 = v2.color.to_array();
                let color = [
                    (c0[0] as f32 * b0 + c1[0] as f32 * b1 + c2[0] as f32 * b2) as u8,
                    (c0[1] as f32 * b0 + c1[1] as f32 * b1 + c2[1] as f32 * b2) as u8,
                    (c0[2] as f32 * b0 + c1[2] as f32 * b1 + c2[2] as f32 * b2) as u8,
                    (c0[3] as f32 * b0 + c1[3] as f32 * b1 + c2[3] as f32 * b2) as u8,
                ];
                let src = modulate(color, texture.sample(u, v));
                if src[3] == 0 && src[0] == 0 && src[1] == 0 && src[2] == 0 {
                    continue;
                }
                let index = y * width + x;
                buffer[index] = blend_premultiplied(buffer[index], src);
            }
        }
    }
}

fn image_to_pixels(image: &egui::epaint::image::ImageData) -> (usize, usize, Vec<[u8; 4]>) {
    match image {
        egui::epaint::image::ImageData::Color(color) => (
            color.size[0],
            color.size[1],
            color.pixels.iter().map(|c| c.to_array()).collect(),
        ),
    }
}

fn tex_filter(options: &egui::TextureOptions) -> TexFilter {
    match options.magnification {
        egui::TextureFilter::Linear => TexFilter::Linear,
        egui::TextureFilter::Nearest => TexFilter::Nearest,
    }
}

pub(crate) fn apply_texture_delta(
    textures: &mut HashMap<egui::TextureId, CpuTexture>,
    id: egui::TextureId,
    delta: &egui::epaint::image::ImageDelta,
) {
    let (width, height, pixels) = image_to_pixels(&delta.image);
    match delta.pos {
        None => {
            textures.insert(
                id,
                CpuTexture {
                    width,
                    height,
                    pixels,
                    filter: tex_filter(&delta.options),
                },
            );
        }
        Some([x, y]) => {
            if let Some(existing) = textures.get_mut(&id) {
                for row in 0..height {
                    for column in 0..width {
                        let dst_x = x + column;
                        let dst_y = y + row;
                        if dst_x < existing.width && dst_y < existing.height {
                            existing.pixels[dst_y * existing.width + dst_x] =
                                pixels[row * width + column];
                        }
                    }
                }
            }
        }
    }
}

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
        // egui::ColorImage::new は source_size を size と同値で埋める（テスト用合成画像に SVG 由来の別サイズは無い）。
        let img = egui::ColorImage::new(
            [w, h],
            vec![egui::Color32::from_rgba_premultiplied(id_pixels[0], id_pixels[1], id_pixels[2], id_pixels[3]); w * h],
        );
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
