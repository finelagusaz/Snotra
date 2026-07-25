//! egui メインウィンドウのアイコン・テクスチャ層（#532 SU4）。IconCache（PNG 永続層）とは別に、
//! path→TextureHandle をセッション内で保持する。純粋核（PNG→ColorImage decode・可視集合 retain・
//! 抽出要否述語）をここに置き、worker spawn / load_texture の driver は view.rs が持つ。

use std::collections::{HashMap, HashSet};

/// worker → driver のメッセージ。token は載せない——アイコンの staleness は path キー付けで
/// 構造的に無害（遅延到着 texture は現行行の path でしか引かれない・SU4 決定 2）。
/// driver（view.rs の worker spawn / load_texture）が消費する（#532 SU4 Task 5）。
pub(crate) enum IconMsg {
    Loaded(String, egui::ColorImage),
    /// 取得できなかった。**第 2 引数は「再試行してよいか」**（#692）——一過性の失敗
    /// （冷えたシェルのアイコンキャッシュ等）を恒久的な欠落と同じ扱いにすると、その行は
    /// 可視である限りグレーのプレースホルダのまま戻らない。
    Missing(String, bool),
}

/// 自前エンコードの RGBA8 PNG（icon.rs bgra_to_png）を ColorImage へ decode する。
/// 想定外の色種別/深度は None（自前エンコードは常に RGBA8）。driver（view.rs）が消費する
/// （#532 SU4 Task 5）。
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

/// 抽出を諦めるまでの試行回数（#692）。`extract_icon` 内のリトライ（シェルの冷えた
/// キャッシュ対策）とは別の層で、**worker 往復を含む再試行**の上限である。
pub(crate) const ICON_MAX_ATTEMPTS: u8 = 3;

/// path が未取得かつ試行上限に達していないなら true（抽出 worker に積むべきか）。
///
/// **失敗を「有無」ではなく「回数」で持つ**（#692）。一過性の失敗（シェルのアイコン
/// キャッシュが冷えている等）を 1 度で恒久的な欠落として latch すると、その行は可視で
/// ある限りグレーのプレースホルダのまま戻らない。恒久的な失敗（パス不在等）は
/// 呼び出し側が `ICON_MAX_ATTEMPTS` を直接入れて即座に打ち切る。
///
/// 値型 `V` でジェネリック化してあるのは呼び出し側の型（`egui::TextureHandle`）に依存
/// させないため——ctx 無しに生成できない TextureHandle を使わずとも、判定はキー集合と
/// 回数の演算のみで完結する（#532 SU4 Task 5）。
pub(crate) fn needs_extraction<V>(
    path: &str,
    have: &HashMap<String, V>,
    attempts: &HashMap<String, u8>,
) -> bool {
    !have.contains_key(path) && attempts.get(path).copied().unwrap_or(0) < ICON_MAX_ATTEMPTS
}

/// 可視集合に無い path の値を drop（メモリを可視集合に頭打ち・SU4 決定 A メモリ境界）。
/// `needs_extraction` 同様に値型 `V` でジェネリック化（呼び出し側は `egui::TextureHandle`）。
/// driver（view.rs）が消費する（#532 SU4 Task 5）。
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
    fn needs_extraction_retries_until_attempt_cap() {
        let mut have: HashMap<String, u32> = HashMap::new();
        have.insert("present.exe".into(), 1);
        let mut attempts: HashMap<String, u8> = HashMap::new();
        attempts.insert("once.exe".into(), 1);
        attempts.insert("capped.exe".into(), ICON_MAX_ATTEMPTS);

        assert!(super::needs_extraction("new.exe", &have, &attempts), "未知は要抽出");
        assert!(
            super::needs_extraction("once.exe", &have, &attempts),
            "1 度失敗しただけでは諦めない（#692: 一過性の失敗を恒久扱いしない）"
        );
        assert!(
            !super::needs_extraction("capped.exe", &have, &attempts),
            "上限に達したら再抽出しない（無限リトライを作らない）"
        );
        assert!(
            !super::needs_extraction("present.exe", &have, &attempts),
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
