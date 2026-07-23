//! egui メインウィンドウのアイコン・テクスチャ層（#532 SU4）。IconCache（PNG 永続層）とは別に、
//! path→TextureHandle をセッション内で保持する。純粋核（PNG→ColorImage decode・可視集合 retain・
//! 抽出要否述語）をここに置き、worker spawn / load_texture の driver は view.rs が持つ。

use std::collections::{HashMap, HashSet};

/// worker → driver のメッセージ。token は載せない——アイコンの staleness は path キー付けで
/// 構造的に無害（遅延到着 texture は現行行の path でしか引かれない・SU4 決定 2）。
/// driver（view.rs の worker spawn / load_texture）は Task 5 で導入されるため、Task 4 単体では
/// 未消費（#532 SU4）。
#[allow(dead_code)]
pub(crate) enum IconMsg {
    Loaded(String, egui::ColorImage),
    Missing(String),
}

/// 自前エンコードの RGBA8 PNG（icon.rs bgra_to_png）を ColorImage へ decode する。
/// 想定外の色種別/深度は None（自前エンコードは常に RGBA8）。driver（view.rs）は Task 5 で消費
/// するため、Task 4 単体では未消費（#532 SU4）。
#[allow(dead_code)]
pub(crate) fn png_to_color_image(png: &[u8]) -> Option<egui::ColorImage> {
    let decoder = png::Decoder::new(std::io::Cursor::new(png));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    let (w, h) = (info.width as usize, info.height as usize);
    let rgba = &buf[..info.buffer_size()];
    let pixels: Vec<egui::Color32> = rgba
        .chunks_exact(4)
        .map(|c| egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]))
        .collect();
    if pixels.len() != w * h {
        return None;
    }
    Some(egui::ColorImage::new([w, h], pixels))
}

/// path が未取得かつ未 missing なら true（抽出 worker に積むべきか）。値型 `V` でジェネリック化
/// してあるのは呼び出し側の型（`egui::TextureHandle`）に依存させないため——ctx 無しに生成できない
/// TextureHandle を使わずとも、判定は `have`/`missing` のキー集合演算のみで完結する（ユニットテスト
/// でダミー値型を使い present/missing/new の全経路を検証できる）。driver（view.rs）は Task 5 で
/// 消費するため、Task 4 単体では未消費（#532 SU4）。
#[allow(dead_code)]
pub(crate) fn needs_extraction<V>(
    path: &str,
    have: &HashMap<String, V>,
    missing: &HashSet<String>,
) -> bool {
    !have.contains_key(path) && !missing.contains(path)
}

/// 可視集合に無い path の値を drop（メモリを可視集合に頭打ち・SU4 決定 A メモリ境界）。
/// `needs_extraction` 同様に値型 `V` でジェネリック化（呼び出し側は `egui::TextureHandle`）。
/// driver（view.rs）は Task 5 で消費するため、Task 4 単体では未消費（#532 SU4）。
#[allow(dead_code)]
pub(crate) fn retain_visible<V>(textures: &mut HashMap<String, V>, visible: &HashSet<String>) {
    textures.retain(|k, _| visible.contains(k));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_to_color_image_roundtrips_rgba8() {
        // 2x2 RGBA8 PNG を png クレートで作り、decode して画素が一致することを確認。
        let mut png_buf = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut png_buf, 2, 2);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().unwrap();
            // R,G,B,A の 4 画素（straight alpha）
            w.write_image_data(&[
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 128,
            ])
            .unwrap();
        }
        let img = super::png_to_color_image(&png_buf).expect("decode");
        assert_eq!(img.size, [2, 2]);
        assert_eq!(img.pixels[0], egui::Color32::from_rgba_unmultiplied(255, 0, 0, 255));
        assert_eq!(img.pixels[3], egui::Color32::from_rgba_unmultiplied(255, 255, 255, 128));
    }

    // needs_extraction / retain_visible は値型 V でジェネリック化してある（egui::TextureHandle は
    // ctx 無しに生成できずユニットテストで present/drop 経路を検証できないため）。ダミー値
    // HashMap<String, u32> で present→skip / missing→skip / new→needs、retain の drop/keep を実際に検証する。

    #[test]
    fn needs_extraction_skips_present_and_missing() {
        let mut have: HashMap<String, u32> = HashMap::new();
        have.insert("present.exe".into(), 1);
        let mut missing: HashSet<String> = HashSet::new();
        missing.insert("m.exe".into());

        assert!(super::needs_extraction("new.exe", &have, &missing), "未知は要抽出");
        assert!(!super::needs_extraction("m.exe", &have, &missing), "missing は再抽出しない");
        assert!(
            !super::needs_extraction("present.exe", &have, &missing),
            "既取得は再抽出しない"
        );
    }

    #[test]
    fn retain_visible_drops_out_of_set() {
        let mut textures: HashMap<String, u32> = HashMap::new();
        textures.insert("keep.exe".into(), 1);
        textures.insert("drop.exe".into(), 2);
        let mut visible: HashSet<String> = HashSet::new();
        visible.insert("keep.exe".into());

        super::retain_visible(&mut textures, &visible);

        assert_eq!(textures.len(), 1);
        assert!(textures.contains_key("keep.exe"), "可視集合内は保持される");
        assert!(!textures.contains_key("drop.exe"), "可視集合外は drop される");
    }
}
