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
    /// `SNOTRA_EGUI_PAINT_TRACE` の行に載せる窓の識別子（`main` / `results`）。
    /// **窓ごとに別の `egui::Context` を持つ構成ゆえ、窓を名指さない計測値は合算できない**
    /// ——同じ字形の集合が窓の数だけ実体化されるため、どちらの窓の額かで意味が変わる。
    label: String,
}

impl EguiRenderer {
    /// present するスレッド（tao イベントループ）で呼ぶこと。GDI 親和性のため。
    pub(crate) fn new(window: tauri::Window) -> Result<Self, RuntimeError> {
        // label は `window` を softbuffer へ渡す前に取る（渡すと move される）。
        let label = window.label().to_string();
        let context = softbuffer::Context::new(window.clone())
            .map_err(|e| RuntimeError::SurfaceInit(e.to_string()))?;
        let surface = softbuffer::Surface::new(&context, window)
            .map_err(|e| RuntimeError::SurfaceInit(e.to_string()))?;
        Ok(Self {
            surface,
            textures: HashMap::new(),
            label,
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
        mut output: egui::FullOutput,
        size: PhysicalSize<u32>,
        clear_color: Option<egui::Color32>,
    ) -> Result<PaintOutcome, RuntimeError> {
        if !is_renderable_extent(size.width, size.height) {
            return Ok(PaintOutcome::Skipped);
        }
        // paint フェーズ計測（#628・#532 SU6.5 G3(b)）。env 未設定なら Instant も取らない
        // ——常時 2 回の時刻取得を入れないため（計測器が測定対象を汚さない）。
        let trace = crate::env::trace_hatch_enabled("SNOTRA_EGUI_PAINT_TRACE");
        let t_begin = trace.then(std::time::Instant::now);
        let ppp = output.pixels_per_point;

        // texture delta（set）を CPU store へ。free は present 成否に依らず後で確定。
        //
        // **1 つの `TextureId` に複数の delta が来る**（egui 0.36 で値が `SmallVec` になった）。
        // **到着順に全部適用すること**——部分更新は前の状態へ重ねる前提で作られており、
        // 最後の 1 件だけを採ると間の更新が落ちる（フォントアトラスの追記が典型）。
        //
        // **参照で舐めずに `drain` で消費する。** egui 0.36 の `TexturesDelta` は `Drop` で
        // 「未適用の delta が残っていないか」を `debug_assert!` する。参照で読むだけだと中身が
        // 残ったまま drop され、**debug ビルドが panic する**（release は素通りするので、
        // 気づかないまま出荷しうる側の差）。free も同じ理由でここで取り出しておく——
        // **適用は present の後だが、所有権を先に移さないと、下の `?` で早期 return した経路が
        // 未適用のまま drop する**。
        for (id, deltas) in output.textures_delta.set.drain() {
            for delta in deltas {
                raster::apply_texture_delta(&mut self.textures, id, &delta);
            }
        }
        let to_free: Vec<egui::TextureId> = output.textures_delta.free.drain().collect();
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
        for id in &to_free {
            self.textures.remove(id);
        }
        present_result?;
        if let (Some(b), Some(t), Some(r)) = (t_begin, t_tess, t_raster) {
            // **メモリ内訳は時刻の後に測る**——`fonts()` はフォントのロックを取るので、
            // 上の 3 区間（tess / raster / present）へその待ちを混ぜない。
            //
            // ここが出す 3 つは**同じ字形集合の別々の実体**である（外から区別できないと、
            // 増えた額をどれにも帰属させられない）:
            // - `atlas`（epaint 側の `TextureAtlas::image`・RGBA8・幅は `MAX_TEXTURE_SIDE` 固定で
            //   高さは 32 から倍々に伸びる）
            // - `tex_font`（この層が `apply_texture_delta` で持つ**その複製**。CPU ラスタは
            //   epaint のアトラス実体を借りられないため構造上 2 部目が要る）
            // - `tex_other`（アイコン等の実行時テクスチャ。件数も出す）
            //
            // **撤去してよいのは、この 3 つの関係が別の手段で観測できるようになったときである。**
            // 現状これらは外から測れない——プロセスの commit にまとめて現れるだけで、窓ごとにも
            // 実体ごとにも分けられない。計器の一覧と読み方は `PERFORMANCE.md`「計測と受け入れ基準」。
            let residency = raster::texture_residency(&self.textures);
            let atlas = context.fonts(|f| f.font_image_size());
            let kib = |bytes: usize| bytes / 1024;
            eprintln!(
                "SNOTRA_EGUI_PAINT win={} tess_ms={:.2} raster_ms={:.2} total_ms={:.2} meshes={} px={} \
                 surface_kib={} atlas={}x{} atlas_kib={} tex_font_kib={} tex_other_kib={} tex_other_n={}",
                self.label,
                (t - b).as_secs_f64() * 1000.0,
                (r - t).as_secs_f64() * 1000.0,
                b.elapsed().as_secs_f64() * 1000.0,
                clipped.len(),
                size.width as u64 * size.height as u64,
                kib(width * height * 4),
                atlas[0],
                atlas[1],
                kib(atlas[0] * atlas[1] * 4),
                kib(residency.font_bytes),
                kib(residency.other_bytes),
                residency.other_count,
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
