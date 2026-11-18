//! egui の Mesh を CPU 側でラスタライズする純関数群（softbuffer 提示用の中核）。
//!
//! #532 Phase 1 の技術スパイク（採用判断用の検証バイナリ・#660 で撤去）から移植。`renderer.rs` の
//! `EguiRenderer::paint` から呼ばれる（#532 SU1 Task 4 で配線）。

use std::collections::HashMap;

/// 2D エッジ関数（外積）。正なら点 p は a→b の左側にある。
pub(crate) fn edge(ax: f32, ay: f32, bx: f32, by: f32, px: f32, py: f32) -> f32 {
    (bx - ax) * (py - ay) - (by - ay) * (px - ax)
}

/// premultiplied sRGB 同士の over 合成。dst は 0x00RRGGBB、src は `[r, g, b, a]`。
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
                let x =
                    ((u * self.width as f32) as isize).clamp(0, self.width as isize - 1) as usize;
                let y =
                    ((v * self.height as f32) as isize).clamp(0, self.height as isize - 1) as usize;
                self.pixels[y * self.width + x]
            }
            // half-texel 規約 uv*size - 0.5 で 4 近傍を ClampToEdge 補間(egui-wgpu 一致)。
            TexFilter::Linear => self.sample_bilinear(u, v),
        }
    }

    /// `WHITE_UV`（= (0, 0)）での [`Self::sample`] が白 `[255; 4]` を返すか。
    ///
    /// (0, 0) の標本は、Nearest でも Linear（half-texel の位置を端で切り詰め、重みが左上へ 1 つに寄る）でも
    /// `pixels[0]` そのものである。大きさ 0 のテクスチャは白を返す。epaint のフォントアトラスは (0, 0) に白を書く。
    pub(crate) fn samples_white(&self) -> bool {
        self.width == 0 || self.height == 0 || self.pixels.first() == Some(&[255, 255, 255, 255])
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

/// [`row_span`] が境界の外へ広げる余白（物理画素）。境界の計算の誤差（座標が数千画素の f32 で
/// 1 画素の千分の一の桁）より桁で大きくとり、真に内側の画素を範囲から落とさない。
const ROW_SPAN_MARGIN_PX: f32 = 2.0;

/// [`row_span`] が範囲を絞る頂点の座標の上限（物理画素・絶対値）。境界の誤差は座標に比例して
/// 伸び、約 3e7 画素で余白を超える。epaint は無限の矩形を ±1e7 pt に切り詰めて渡すので、
/// それより桁で小さいここで線を引き、超える三角形は範囲を絞らずに `min_x..max_x` をすべて判定する。
const ROW_SPAN_COORD_LIMIT_PX: f32 = 1.0e6;

/// 行（画素中心の y 座標が `py`）のうち、三角形の内側になりうる画素の x の範囲 `[start, end)` を返す。
///
/// **保守的な範囲であって、内外の判定ではない。** 3 つの辺の式は、行を固定すると x の 1 次式なので、
/// 条件を満たす x はそれぞれ半直線になる。その共通部分を [`ROW_SPAN_MARGIN_PX`] だけ広げ、
/// `min_x..max_x` で切る。呼び手は範囲の中で、今までどおりの内外判定を行う——ゆえに範囲が真に内側の
/// 画素をすべて含む限り、描く画素は範囲を絞らないときと 1 つも変わらない。水平な辺（`by - ay == 0`）と、
/// 境界が NaN になる辺は範囲を狭めない。頂点の座標が [`ROW_SPAN_COORD_LIMIT_PX`] を超える三角形も狭めない。
fn row_span(
    tri: [(f32, f32); 3],
    area: f32,
    py: f32,
    min_x: usize,
    max_x: usize,
) -> Option<(usize, usize)> {
    let (mut lo, mut hi) = (f32::NEG_INFINITY, f32::INFINITY);
    let huge = tri
        .iter()
        .any(|(x, y)| !(x.abs() <= ROW_SPAN_COORD_LIMIT_PX && y.abs() <= ROW_SPAN_COORD_LIMIT_PX));
    if huge {
        return (min_x < max_x).then_some((min_x, max_x));
    }
    // 3 つの辺（a → b）。
    for ((ax, ay), (bx, by)) in [(tri[1], tri[2]), (tri[2], tri[0]), (tri[0], tri[1])] {
        let dy = by - ay;
        if dy == 0.0 {
            continue;
        }
        // w(px) = (bx - ax)(py - ay) - dy (px - ax) は px について傾き -dy の 1 次式で、x* で 0 になる。
        let boundary = ax + (bx - ax) * (py - ay) / dy;
        if boundary.is_nan() {
            continue;
        }
        // 面積が正なら w >= 0、負なら w <= 0 が内側。dy > 0 のとき w は px とともに減る。
        if (dy > 0.0) == (area > 0.0) {
            hi = hi.min(boundary);
        } else {
            lo = lo.max(boundary);
        }
    }
    // 画素 x の中心は x + 0.5。中心が [lo - 余白, hi + 余白] に入る x を `min_x..max_x` の中で選ぶ。
    let start = (lo - ROW_SPAN_MARGIN_PX - 0.5).ceil().max(min_x as f32);
    let end = ((hi + ROW_SPAN_MARGIN_PX - 0.5).floor() + 1.0).min(max_x as f32);
    (start < end).then_some((start as usize, end as usize))
}

/// 1 つの epaint Mesh を framebuffer へ描く。pos は物理ピクセル座標へ変換済みであること。
///
/// 各行で内外を判定するのは [`row_span`] の範囲の画素だけである。3 頂点の UV がすべて `WHITE_UV` で、
/// テクスチャがそこで白を返すなら（[`CpuTexture::samples_white`]）、標本化と変調を省く——白との変調は
/// 恒等なので、描く値は変わらない。
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
    let texture_is_white = texture.samples_white();
    for triangle in indices.as_chunks::<3>().0 {
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
        let white = texture_is_white
            && [v0.uv, v1.uv, v2.uv]
                .iter()
                .all(|uv| *uv == egui::epaint::WHITE_UV);
        let min_x = x0.min(x1).min(x2).floor().max(clip_min.0 as f32) as usize;
        let min_y = y0.min(y1).min(y2).floor().max(clip_min.1 as f32) as usize;
        let max_x = (x0.max(x1).max(x2).ceil() as usize)
            .min(clip_max.0)
            .min(width);
        let max_y = (y0.max(y1).max(y2).ceil() as usize)
            .min(clip_max.1)
            .min(height);
        for y in min_y..max_y {
            let py = y as f32 + 0.5;
            let Some((start, end)) =
                row_span([(x0, y0), (x1, y1), (x2, y2)], area, py, min_x, max_x)
            else {
                continue;
            };
            for x in start..end {
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
                let c0 = v0.color.to_array();
                let c1 = v1.color.to_array();
                let c2 = v2.color.to_array();
                let color = [
                    (c0[0] as f32 * b0 + c1[0] as f32 * b1 + c2[0] as f32 * b2) as u8,
                    (c0[1] as f32 * b0 + c1[1] as f32 * b1 + c2[1] as f32 * b2) as u8,
                    (c0[2] as f32 * b0 + c1[2] as f32 * b1 + c2[2] as f32 * b2) as u8,
                    (c0[3] as f32 * b0 + c1[3] as f32 * b1 + c2[3] as f32 * b2) as u8,
                ];
                let src = if white {
                    color
                } else {
                    let u = v0.uv.x * b0 + v1.uv.x * b1 + v2.uv.x * b2;
                    let v = v0.uv.y * b0 + v1.uv.y * b1 + v2.uv.y * b2;
                    modulate(color, texture.sample(u, v))
                };
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

/// CPU 側テクスチャ常駐の内訳（バイト）。**フォントアトラスとそれ以外を分ける**
/// ——前者はウィンドウごとに 1 枚で高さの倍加とともに階段状に伸び、後者（アイコン等）は件数が動く。
/// 混ぜて 1 つの数にすると、増えたのがどちらかを外から区別できない。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TextureResidency {
    pub(crate) font_bytes: usize,
    pub(crate) other_bytes: usize,
    pub(crate) other_count: usize,
}

/// `textures` が保持する RGBA8 画素のバイト数を、フォントアトラスとそれ以外へ振り分ける。
///
/// **フォントアトラスの判定は `TextureId::Managed(0)`** である——「最初の managed テクスチャが
/// フォントデータ」は epaint の規約（`epaint::TextureId::Managed` の doc）であって、この層が
/// 決めた約束ではない。規約が変われば `font_bytes` が 0 に落ち、同じ額が `other_bytes` 側へ
/// 現れる（黙って消えはしない）。
///
/// 数えるのは `pixels` の**長さ**であり `Vec` の容量ではない。`apply_texture_delta` の全面更新は
/// `collect` で正確な長さの `Vec` を作るため、両者は一致する。
pub(crate) fn texture_residency(
    textures: &HashMap<egui::TextureId, CpuTexture>,
) -> TextureResidency {
    let mut out = TextureResidency::default();
    for (id, texture) in textures {
        let bytes = texture.pixels.len() * std::mem::size_of::<[u8; 4]>();
        if *id == egui::TextureId::Managed(0) {
            out.font_bytes += bytes;
        } else {
            out.other_bytes += bytes;
            out.other_count += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::epaint::image::{ImageData, ImageDelta};

    fn texture(width: usize, height: usize) -> CpuTexture {
        CpuTexture {
            width,
            height,
            pixels: vec![[0u8; 4]; width * height],
            filter: TexFilter::Nearest,
        }
    }

    /// 空の store は 0 を返す。**「測れなかった」と「0 だった」を区別できる形にしておく**
    /// ——呼び出し側は件数も併せて出すので、0 件と 0 バイトが同時に見える。
    #[test]
    fn texture_residency_of_empty_store_is_zero() {
        let textures: HashMap<egui::TextureId, CpuTexture> = HashMap::new();
        assert_eq!(texture_residency(&textures), TextureResidency::default());
    }

    /// `Managed(0)` はフォントアトラス（epaint の規約）。RGBA8 ゆえ画素数 × 4 バイト。
    #[test]
    fn texture_residency_attributes_managed_zero_to_the_font_atlas() {
        let mut textures = HashMap::new();
        textures.insert(egui::TextureId::Managed(0), texture(4096, 32));
        assert_eq!(
            texture_residency(&textures),
            TextureResidency {
                font_bytes: 4096 * 32 * 4,
                other_bytes: 0,
                other_count: 0,
            }
        );
    }

    /// アイコン等は `Managed(0)` 以外に載る。**件数も返す**——バイト数だけでは
    /// 「大きいのが 1 枚」と「小さいのが多数」を区別できない。
    #[test]
    fn texture_residency_counts_non_font_textures_separately() {
        let mut textures = HashMap::new();
        textures.insert(egui::TextureId::Managed(0), texture(64, 2));
        textures.insert(egui::TextureId::Managed(1), texture(32, 32));
        textures.insert(egui::TextureId::User(7), texture(16, 16));
        assert_eq!(
            texture_residency(&textures),
            TextureResidency {
                font_bytes: 64 * 2 * 4,
                other_bytes: (32 * 32 + 16 * 16) * 4,
                other_count: 2,
            }
        );
    }

    #[test]
    fn raster_core_matches_soft_probe_invariants() {
        assert!(edge(0.0, 0.0, 4.0, 0.0, 1.0, 1.0) > 0.0);
        assert_eq!(
            blend_premultiplied(0x0000_0000, [255, 0, 0, 255]),
            0x00FF_0000
        );
        assert_eq!(blend_premultiplied(0x0012_3456, [0, 0, 0, 0]), 0x0012_3456);
        assert_eq!(
            modulate([200, 100, 50, 255], [255, 255, 255, 255]),
            [200, 100, 50, 255]
        );

        let mut buffer = vec![0u32; 8 * 8];
        let vertex = |x: f32, y: f32| egui::epaint::Vertex {
            pos: egui::pos2(x, y),
            uv: egui::pos2(0.0, 0.0),
            color: egui::Color32::from_rgba_premultiplied(255, 255, 255, 255),
        };
        let vertices = [vertex(0.0, 0.0), vertex(8.0, 0.0), vertex(0.0, 8.0)];
        let white = CpuTexture {
            width: 0,
            height: 0,
            pixels: Vec::new(),
            filter: TexFilter::Nearest,
        };
        fill_mesh(
            &mut buffer,
            8,
            8,
            &vertices,
            &[0, 1, 2],
            &white,
            (0, 0),
            (8, 8),
            1.0,
        );
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
        let nearest = CpuTexture {
            filter: TexFilter::Nearest,
            ..CpuTexture {
                width: 2,
                height: 1,
                pixels: vec![[0, 0, 0, 255], [255, 255, 255, 255]],
                filter: TexFilter::Nearest,
            }
        };
        let n = nearest.sample(0.5, 0.5);
        assert!(n == [0, 0, 0, 255] || n == [255, 255, 255, 255]);
    }

    fn solid(id_pixels: [u8; 4], w: usize, h: usize, filter: egui::TextureOptions) -> ImageDelta {
        // egui::ColorImage::new は source_size を size と同値で埋める（テスト用合成画像に SVG 由来の別サイズは無い）。
        let img = egui::ColorImage::new(
            [w, h],
            vec![
                egui::Color32::from_rgba_premultiplied(
                    id_pixels[0],
                    id_pixels[1],
                    id_pixels[2],
                    id_pixels[3]
                );
                w * h
            ],
        );
        ImageDelta::full(ImageData::Color(std::sync::Arc::new(img)), filter)
    }

    #[test]
    fn texture_store_full_then_partial_and_filter_retained() {
        let mut store: HashMap<egui::TextureId, CpuTexture> = HashMap::new();
        let id = egui::TextureId::Managed(0);
        // pos=None 全面（Linear）。
        apply_texture_delta(
            &mut store,
            id,
            &solid([10, 20, 30, 255], 2, 2, egui::TextureOptions::LINEAR),
        );
        assert_eq!(store[&id].filter, TexFilter::Linear);
        // pos=Some 部分上書き（filter は既存 Linear を保持）。
        let mut partial = solid([99, 99, 99, 255], 1, 1, egui::TextureOptions::NEAREST);
        partial.pos = Some([0, 0]);
        apply_texture_delta(&mut store, id, &partial);
        assert_eq!(
            store[&id].filter,
            TexFilter::Linear,
            "部分更新は filter を変えない"
        );
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

    // ---- #1451: 行の範囲と白テクセルの省略が、描く画素を 1 つも変えないことの検知器 ----

    /// テスト用の小さな擬似乱数（xorshift64）。crate は乱数の依存を持たない。
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
        fn unit(&mut self) -> f32 {
            (self.next() >> 40) as f32 / (1u64 << 24) as f32
        }
        fn byte(&mut self) -> u8 {
            self.next() as u8
        }
    }

    /// premultiplied の色（各チャネルが alpha 以下）。
    fn random_color(rng: &mut Rng) -> egui::Color32 {
        let a = rng.byte();
        let mut channel = || (rng.byte() as u16 * a as u16 / 255) as u8;
        egui::Color32::from_rgba_premultiplied(channel(), channel(), channel(), a)
    }

    /// (0, 0) が `corner` で、残りはばらばらなテクスチャ。
    fn random_texture(rng: &mut Rng, filter: TexFilter, corner: [u8; 4]) -> CpuTexture {
        let (width, height) = (8, 8);
        let mut pixels: Vec<[u8; 4]> = (0..width * height)
            .map(|_| random_color(rng).to_array())
            .collect();
        pixels[0] = corner;
        CpuTexture {
            width,
            height,
            pixels,
            filter,
        }
    }

    /// 座標の 1 成分。画面の外へはみ出す値・画素中心の行（`n + 0.5`）に極めて近い値（水平に近い辺の境界が
    /// 最も動く形）・境界の誤差が余白を超える桁の値（[`ROW_SPAN_COORD_LIMIT_PX`] の外）を混ぜる。
    fn random_coord(rng: &mut Rng, extent: f32) -> f32 {
        match rng.below(16) {
            0 => (rng.unit() - 0.5) * 3.0e8,
            1..=4 => rng.below(extent as u64) as f32 + 0.5 + (rng.unit() - 0.5) * 1e-4,
            _ => rng.unit() * (extent + 40.0) - 20.0,
        }
    }

    /// `fill_mesh` と、#1451 より前の実装（`fill_mesh_reference`）で同じ buffer を描き、全画素を比べる。
    #[allow(clippy::too_many_arguments)]
    fn assert_same_as_reference(
        label: &str,
        width: usize,
        height: usize,
        vertices: &[egui::epaint::Vertex],
        indices: &[u32],
        texture: &CpuTexture,
        clip_min: (usize, usize),
        clip_max: (usize, usize),
        ppp: f32,
    ) {
        let mut got = vec![0x0012_3456u32; width * height];
        let mut want = got.clone();
        fill_mesh(
            &mut got, width, height, vertices, indices, texture, clip_min, clip_max, ppp,
        );
        fill_mesh_reference(
            &mut want, width, height, vertices, indices, texture, clip_min, clip_max, ppp,
        );
        let differ = got.iter().zip(&want).filter(|(a, b)| a != b).count();
        assert_eq!(differ, 0, "{label}: 前の実装と {differ} 画素が違う");
    }

    /// **ランダムな三角形で、描く画素が前の実装と 1 つも変わらない。** 両方の巻き方・水平に近い辺・
    /// 画面の外へはみ出す頂点・巨大な座標・白 UV とテクスチャ UV・(0, 0) が白のテクスチャと白でないテクスチャ・
    /// Nearest / Linear / 大きさ 0 のテクスチャ・切り抜き矩形・
    /// 4 つの表示倍率を混ぜる。
    #[test]
    fn fill_mesh_matches_reference_on_random_meshes() {
        let mut rng = Rng(0x1451_0000_dead_beef);
        let (width, height) = (96usize, 64usize);
        const WHITE: [u8; 4] = [255, 255, 255, 255];
        const NEAR_WHITE: [u8; 4] = [255, 255, 255, 254];
        let textures = [
            random_texture(&mut rng, TexFilter::Nearest, WHITE),
            random_texture(&mut rng, TexFilter::Linear, WHITE),
            // (0, 0) が白でない: 白 UV でも標本化を省いてはならない。
            random_texture(&mut rng, TexFilter::Nearest, NEAR_WHITE),
            random_texture(&mut rng, TexFilter::Linear, NEAR_WHITE),
            texture(0, 0),
        ];
        for ppp in [1.0f32, 1.25, 1.5, 2.0] {
            for case in 0..1000 {
                let white_uv = rng.below(2) == 0;
                let vertices: Vec<egui::epaint::Vertex> = (0..3)
                    .map(|_| egui::epaint::Vertex {
                        pos: egui::pos2(
                            random_coord(&mut rng, width as f32) / ppp,
                            random_coord(&mut rng, height as f32) / ppp,
                        ),
                        uv: if white_uv {
                            egui::epaint::WHITE_UV
                        } else {
                            egui::pos2(rng.unit(), rng.unit())
                        },
                        color: random_color(&mut rng),
                    })
                    .collect();
                let clip_min = (rng.below(16) as usize, rng.below(16) as usize);
                let clip_max = (
                    width - rng.below(16) as usize,
                    height - rng.below(16) as usize,
                );
                let tex = &textures[rng.below(textures.len() as u64) as usize];
                assert_same_as_reference(
                    &format!("ppp={ppp} case={case}"),
                    width,
                    height,
                    &vertices,
                    &[0, 1, 2],
                    tex,
                    clip_min,
                    clip_max,
                    ppp,
                );
            }
        }
    }

    /// **egui が組んだ実際のフレームで、描く画素が前の実装と 1 つも変わらない。** 結果リストに似た UI
    /// （2 段のラベル・選択行の角丸の塗り・フォントアトラスの一部を貼った矩形〔アイコンの代わり〕・枠線）を 4 つの表示倍率で描く。
    #[test]
    fn fill_mesh_matches_reference_on_an_egui_frame() {
        for ppp in [1.0f32, 1.25, 1.5, 2.0] {
            let ctx = egui::Context::default();
            ctx.set_pixels_per_point(ppp);
            let size = egui::vec2(600.0, 330.0);
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                for row in 0..8 {
                    let rect = egui::Rect::from_min_size(
                        egui::pos2(4.0, 4.0 + row as f32 * 40.0),
                        egui::vec2(592.0, 38.0),
                    );
                    if row == 2 {
                        ui.painter()
                            .rect_filled(rect, 6.0, egui::Color32::from_rgb(60, 90, 160));
                    }
                    ui.painter().rect_stroke(
                        rect,
                        6.0,
                        egui::Stroke::new(1.0, egui::Color32::from_gray(90)),
                        egui::StrokeKind::Inside,
                    );
                    let icon = egui::Rect::from_min_size(
                        rect.min + egui::vec2(6.0, 6.0),
                        egui::vec2(26.0, 26.0),
                    );
                    ui.painter().image(
                        egui::TextureId::Managed(0),
                        icon,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(0.5, 0.5)),
                        egui::Color32::WHITE,
                    );
                    ui.painter().text(
                        rect.min + egui::vec2(40.0, 4.0),
                        egui::Align2::LEFT_TOP,
                        format!("Application {row} — 検索結果"),
                        egui::FontId::proportional(15.0),
                        egui::Color32::from_gray(230),
                    );
                    ui.painter().text(
                        rect.min + egui::vec2(40.0, 21.0),
                        egui::Align2::LEFT_TOP,
                        format!("C:\\Program Files\\App{row}\\app.exe"),
                        egui::FontId::proportional(11.0),
                        egui::Color32::from_gray(160),
                    );
                }
            });
            let mut store: HashMap<egui::TextureId, CpuTexture> = HashMap::new();
            for (id, deltas) in output.textures_delta.set.drain() {
                for delta in deltas {
                    apply_texture_delta(&mut store, id, &delta);
                }
            }
            output.textures_delta.free.clear();
            let clipped = ctx.tessellate(output.shapes, ppp);
            let (width, height) = (
                (size.x * ppp).ceil() as usize,
                (size.y * ppp).ceil() as usize,
            );
            let blank = texture(0, 0);
            let mut meshes = 0;
            for primitive in &clipped {
                let egui::epaint::Primitive::Mesh(mesh) = &primitive.primitive else {
                    continue;
                };
                meshes += 1;
                let tex = store.get(&mesh.texture_id).unwrap_or(&blank);
                let clip = primitive.clip_rect;
                let clip_min = (
                    (clip.min.x * ppp).round().max(0.0) as usize,
                    (clip.min.y * ppp).round().max(0.0) as usize,
                );
                let clip_max = (
                    (clip.max.x * ppp).round().min(width as f32) as usize,
                    (clip.max.y * ppp).round().min(height as f32) as usize,
                );
                assert_same_as_reference(
                    &format!("ppp={ppp}"),
                    width,
                    height,
                    &mesh.vertices,
                    &mesh.indices,
                    tex,
                    clip_min,
                    clip_max,
                    ppp,
                );
            }
            assert!(
                meshes > 0,
                "ppp={ppp}: メッシュが 1 つも無い（検知器が何も見ていない）"
            );
        }
    }

    /// **`samples_white` は (0, 0) の標本が白のときだけ真になる。** 大きさ 0 のテクスチャは白を返す。
    #[test]
    fn samples_white_only_when_the_corner_texel_is_white() {
        let mut rng = Rng(7);
        for filter in [TexFilter::Nearest, TexFilter::Linear] {
            let white = random_texture(&mut rng, filter, [255, 255, 255, 255]);
            assert!(white.samples_white());
            assert_eq!(white.sample(0.0, 0.0), [255, 255, 255, 255]);
            let near = random_texture(&mut rng, filter, [255, 255, 255, 254]);
            assert!(!near.samples_white());
        }
        assert!(texture(0, 0).samples_white());
    }

    /// **範囲は、真に内側の画素をすべて含む。** 水平な辺・垂直な辺・両方の巻き方で、範囲の外に内側の画素が無いこと。
    #[test]
    fn row_span_contains_every_inside_pixel() {
        let triangles: [[(f32, f32); 3]; 4] = [
            [(1.0, 1.0), (20.0, 1.0), (1.0, 15.0)], // 水平な辺と垂直な辺
            [(1.0, 1.0), (1.0, 15.0), (20.0, 1.0)], // 逆の巻き方
            [(0.3, 7.5), (30.7, 7.5001), (12.0, 0.2)], // 行の中心に乗る、ほぼ水平な辺
            [(5.0, -3.0), (40.0, 20.0), (-6.0, 18.0)], // 画面の外へはみ出す
        ];
        for tri in triangles {
            let [(x0, y0), (x1, y1), (x2, y2)] = tri;
            let area = edge(x0, y0, x1, y1, x2, y2);
            for y in 0..20usize {
                let py = y as f32 + 0.5;
                let span = row_span(tri, area, py, 0, 32);
                for x in 0..32usize {
                    let px = x as f32 + 0.5;
                    let w = [
                        edge(x1, y1, x2, y2, px, py),
                        edge(x2, y2, x0, y0, px, py),
                        edge(x0, y0, x1, y1, px, py),
                    ];
                    let inside = if area > 0.0 {
                        w.iter().all(|&w| w >= 0.0)
                    } else {
                        w.iter().all(|&w| w <= 0.0)
                    };
                    if inside {
                        let (start, end) = span.unwrap_or_else(|| {
                            panic!("{tri:?} y={y}: 内側の画素 x={x} があるのに範囲が空")
                        });
                        assert!(
                            (start..end).contains(&x),
                            "{tri:?} y={y}: 内側の画素 x={x} が範囲 {start}..{end} の外"
                        );
                    }
                }
            }
        }
    }

    /// #1451 より前の `fill_mesh`。比較の基準として、変えずに残す。
    #[allow(clippy::too_many_arguments)]
    fn fill_mesh_reference(
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
        for triangle in indices.as_chunks::<3>().0 {
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
            let max_x = (x0.max(x1).max(x2).ceil() as usize)
                .min(clip_max.0)
                .min(width);
            let max_y = (y0.max(y1).max(y2).ceil() as usize)
                .min(clip_max.1)
                .min(height);
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
}
