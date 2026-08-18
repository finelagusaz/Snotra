//! egui メインウィンドウのアイコン・テクスチャ層（#532 SU4）。IconCache（PNG 永続層）とは別に、
//! path→TextureHandle をセッション内で保持する。純粋核（PNG→ColorImage decode・可視集合 retain・
//! 抽出要否述語）をここに置き、worker spawn / load_texture の driver は results_view.rs が持つ
//! （#646 PR2 で view.rs から移管——テクスチャは egui Context〔= 窓の renderer〕従属のため）。

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
        assert_eq!(
            img.pixels[0],
            egui::Color32::from_rgba_unmultiplied(255, 0, 0, 255)
        );
        assert_eq!(
            img.pixels[3],
            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 128)
        );
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

        assert!(
            super::needs_extraction("new.exe", &have, &attempts),
            "未知は要抽出"
        );
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
        assert!(
            !textures.contains_key("drop.exe"),
            "可視集合外は drop される"
        );
    }

    /// アイコン抽出ゲートが [`crate::egui_shell::results_view::RowsSnapshot::input_idle`] の
    /// **意味論のまま**立っていることを、**ソーステキストで**固定する（#1133）。
    ///
    /// **述語のテストでは呼び出し点の脱落を捕まえられない**（この規範の正本は
    /// `launcher_controller.rs` の `activation_entry_points_consult_the_display_gate` の doc）。
    /// `input_idle` が運ぶのは「main の `search_debounce` が予約を持っていないか」だけで、
    /// `ResultsView` にはテスト席が無い（構築が `AppHandle` を要求する）——ソーステキスト検査は
    /// そのどちらも要らない。
    ///
    /// **この検査が `view.rs` にも `results_view.rs` にも住めない理由**（移設してはならない）:
    /// 探す綴りは assert 自身の文字列リテラルとして**この検査のソースに書かれている**。検査を
    /// 被検査ファイルへ置くと `include_str!` の母集団に needle が混入し、**本物のゲートを消しても
    /// 自分自身のリテラルが見つかって緑のまま通る**——永久に落ちない不動点になる。#1133 の
    /// Phase 0a で、実際に `results_view.rs` へ置いた版が変異 7（ゲートの削除）を素通りした。
    /// **両方のファイルから独立した第三の場所に置くことが、この検査の成立条件である。**
    ///
    /// **これが落ちたとき失うもの**: instant 行のアイコンを出さない仕様（`SPEC.md` §3.4 / §19.5）を
    /// **キー側ではなくこのゲート側で**表現すると、`input_idle` は「打鍵が止まった」より広い述語
    /// なので、**worker 走査中のアイコン取得まで一緒に遅れる**退行が入る（`RowsSnapshot::input_idle`
    /// の doc が `is_unsettled` について「同じ修正を当ててはならない」と名指ししている・#1074）。
    /// しかも**絵は正しく見える**ため、挙動テストでは捕まらない。
    ///
    /// **残る死角は 2 つある。**
    /// 1. 測っているのは部分文字列一致であって呼び出しではない。ゲートを別ヘルパーへ移しても、
    ///    本体にこの綴りが残れば緑のまま通る。
    /// 2. **より踏みやすい迂回がある**——`view.rs` が呼ぶ `is_search_armed()` の**中身**へ instant の
    ///    条件を足せば、同じ害を、ここが見ている `view.rs` の 1 行を 1 文字も変えずに達成できる。
    ///    **そこまで綴りで縛る形は採らない**——正当なリファクタリングまで赤にする検知器は無視される
    ///    ようになる（#1133 のユーザー裁定「必要な分だけ縛る」）。
    ///
    /// **存在形の assert だけで書く**（否定形は母集団が消えたときに沈黙する）。`include_str!` の
    /// 母集団は実ファイル全体なので空になりえない。
    #[test]
    fn icon_gate_keeps_input_idle_semantics() {
        let view = include_str!("view.rs");
        assert!(
            view.contains("let input_idle = !self.controller.is_search_armed();"),
            "view.rs の input_idle が search_debounce の armed 以外を材料にしている——\
             instant のスキップをこのゲートで表現すると worker 走査中のアイコン取得まで遅れる（#1074 / #1133）"
        );
        let results_view = include_str!("results_view.rs");
        assert!(
            results_view.contains("if snapshot.input_idle {"),
            "アイコン抽出要求が input_idle ゲートの内側に無い——連打中に icon worker を積む\
             perf 退行が戻る（#532 SU4 の系譜）"
        );
    }
}
