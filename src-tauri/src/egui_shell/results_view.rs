//! 結果リスト窓（"results"）の egui view（#646 PR2）。main（SearchWindowView）が発行する
//! RowsSnapshot を描くだけの従属 view——検索状態の所有者は main のまま（一方向データフロー・
//! spec 決定 5）。クリックは ResultsShared.clicked へ積んで main を wake する（遅延 dispatch）。
//! 窓の可視性・サイズ・位置の driver は main 側（hidden 窓は update() が走らないため）。
//!
//! **禁止: この view で `frame.hide_window()` を呼ばない** — results は focusable(false) で
//! `Focused(true)` が永遠に来ないため、runtime の visible フラグが false に固着し永久非描画
//! になる（復帰経路は Focused(true) のみ・runtime.rs）。hide は必ず外部（`hide_egui_main` /
//! main の drive）の `window.hide()` で行う。

use snotra_core::ui_types::SearchResult;
use tauri::Manager;

use crate::egui_shell::EguiShellState;

/// main が毎フレーム発行する描画用スナップショット（spec 決定 5）。
#[derive(Clone, Default, PartialEq)]
pub(crate) struct RowsSnapshot {
    pub rows: Vec<snotra_core::ui_types::SearchResult>, // ui_types が正（engine ではない）・PartialEq/Eq derive 済み
    pub selected: usize,
    pub show: bool,
    /// 結果集合が総入れ替えされるたびに main が加算するカウンタ（#632 reviewer Important 3
    /// の後継・Fix 3）。`selected` の値だけでは「打鍵で結果が丸ごと変わったが selected は
    /// 偶然 0 のまま」を検出できないため、この世代番号で ResultsView の scroll gate を
    /// 独立にリセットする。Default=0（PartialEq 比較にも自然に入る）。
    pub generation: u64,
}

/// main と results が共有する一方向フローの入れ物（managed state）。
#[derive(Default)]
pub(crate) struct ResultsShared {
    pub snapshot: std::sync::Mutex<RowsSnapshot>,
    /// クリックされた行 index（last-wins）。main の update() が take して起動処理する。
    pub clicked: std::sync::Mutex<Option<usize>>,
}

pub(crate) struct ResultsView {
    app_handle: tauri::AppHandle,
    /// font_family hot-reload 用（main の applied_font_family と同型・ctx が窓ごとに独立
    /// なため複製必須 — plan-review rev-egui の指摘）。
    applied_font_family: String,
    /// 直近に scroll_to_me した選択 index。選択変化時のみ scroll するための gate（#632・Task 4）。
    last_scrolled_selected: Option<usize>,
    /// 直近に観測した snapshot の世代番号（#632 reviewer Important 3 の後継・Fix 3）。
    /// `RowsSnapshot.generation` との差分で「結果集合が総入れ替えされた」を検出し、
    /// `selected` の値が変わらない場合でも scroll gate を強制リセットする。
    last_generation: u64,
}

impl ResultsView {
    pub(crate) fn new(app_handle: tauri::AppHandle) -> Self {
        Self {
            app_handle,
            applied_font_family: String::new(),
            last_scrolled_selected: None,
            last_generation: 0,
        }
    }
}

/// 実行中 config テーマ値から 1 結果行の描画テーマを都度導出する（キャッシュしない・
/// #576 と同設計）。config が読めなければ既定値へフォールバック（view.rs から移設・Task 4）。
pub(crate) fn row_theme(app: &tauri::AppHandle) -> RowTheme {
    let (text, hint, sel, size) = app
        .try_state::<crate::AppState>()
        .map(|s| {
            let engine = s.engine.lock().unwrap();
            let v = &engine.config().visual;
            (v.text_color.clone(), v.hint_text_color.clone(),
             v.selected_row_color.clone(), v.font_size)
        })
        .unwrap_or_else(|| ("#E0E0E0".into(), "#808080".into(), "#333333".into(), 15));
    RowTheme {
        name_color: hex_color(&text, egui::Color32::from_rgb(0xE0, 0xE0, 0xE0)),
        path_color: hex_color(&hint, egui::Color32::from_rgb(0x80, 0x80, 0x80)),
        selection: hex_color(&sel, egui::Color32::from_rgb(0x33, 0x33, 0x33)),
        name_size: size as f32,
        path_size: crate::egui_shell::layout::path_size(size) as f32, // 正本は layout(#646)
    }
}

/// `#RRGGBB` 文字列を Color32 へ。失敗時は fallback（release は panic=abort ゆえ unwrap しない）。
fn hex_color(s: &str, fallback: egui::Color32) -> egui::Color32 {
    egui::Color32::from_hex(s).unwrap_or(fallback)
}

/// 1 行を描画。selected かつ scroll なら scroll_to_me（選択変化時のみ・#632）。返り値:
/// single_clicked。ダブルクリックは扱わない（ユーザー決定: §4.8 の double-click=選択は
/// as-built でも到達不能ゆえ落とす。単クリック=起動のみ）。self を借りない関連関数
/// （借用衝突回避）。色/サイズは呼び出し側が都度導出する `RowTheme` から取る。
/// 2 行表示(#646 決定 9): 上段名前・下段パス。行高は Metrics::row_height(呼び出し側注入)。
/// `show_icons=false` はアイコン slot 自体を畳む（skip でなくレイアウト変更・#532 SU4 Task 6）
/// ——テキストが左端 8px 寄せになり、slot 分の空白が残らない。
#[allow(clippy::too_many_arguments)] // raster.rs::fill_mesh と同型（描画関数は座標/テーマ引数が集中する）
pub(crate) fn draw_result_row(
    ui: &mut egui::Ui,
    result: &SearchResult,
    selected: bool,
    scroll: bool,
    icon: Option<&egui::TextureHandle>,
    show_icons: bool,
    theme: &RowTheme,
    row_h: f32,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), row_h),
        egui::Sense::click(),
    );
    if selected {
        ui.painter().rect_filled(rect, 4.0, theme.selection);
        if scroll {
            // 選択変化時のみ（#632）。None=可視化に必要な最小限だけ（WebView2 の
            // block:"nearest" parity・Center だと中央維持で早期スクロール・#532 SU6.5）
            response.scroll_to_me(None);
        }
    }
    // アイコン: show_icons=true のときのみ左 28px slot の中央に 16x16 を描く。欠落
    // （icon=None）は drawn placeholder（draw_icon_fallback）で埋める。
    let slot = if show_icons { 28.0 } else { 8.0 };
    if show_icons {
        match icon {
            Some(tex) => {
                let icon_size = 16.0;
                let icon_rect = egui::Rect::from_center_size(
                    egui::pos2(rect.left() + 14.0, rect.center().y),
                    egui::vec2(icon_size, icon_size),
                );
                ui.painter().image(
                    tex.id(),
                    icon_rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            // 通常の欠落のみ placeholder。エラー行（is_error＝フォルダ列挙失敗行等）には
            // アイコン形の装飾を描かない（エラーメッセージに不要・whole-branch review Minor）。
            None if !result.is_error => draw_icon_fallback(ui, rect, result, theme),
            None => {}
        }
    }
    // 2 行表示(#646 決定 9): 上段 = 名前(全幅・末尾省略)、下段 = パス(全幅・左寄せ・
    // 幅超過時のみ中間省略)。#632 の「name 60% 制限 + path 右寄せ + 実測幅の重なり回避」は
    // 2 行化で name と path が幅を取り合わなくなったため廃止。
    let text_x = rect.left() + slot;
    let avail = (rect.right() - 8.0 - text_x).max(0.0);
    let mut name_job = egui::text::LayoutJob::single_section(
        result.name.clone(),
        egui::TextFormat {
            font_id: egui::FontId::proportional(theme.name_size),
            color: theme.name_color,
            ..Default::default()
        },
    );
    name_job.wrap = egui::text::TextWrapping::truncate_at_width(avail);
    let name_galley = ui.painter().layout_job(name_job);
    // path 空(エラー行等)は名前 1 行を縦中央に単独描画
    if result.path.is_empty() {
        ui.painter().galley(
            egui::pos2(text_x, rect.center().y - name_galley.size().y / 2.0),
            name_galley,
            theme.name_color,
        );
        return response.clicked();
    }
    let path_font = egui::FontId::proportional(theme.path_size);
    let path_full = ui.painter().layout_no_wrap(
        result.path.clone(),
        path_font.clone(),
        theme.path_color,
    );
    let path_str = if path_full.size().x <= avail {
        result.path.clone()
    } else {
        // per-char 幅は実 galley から実測(CJK 過小評価対策・#632 の方針を継承)
        let per_char_px = path_full.size().x / (result.path.chars().count().max(1) as f32);
        truncate_middle(&result.path, avail, per_char_px)
    };
    let path_galley = ui.painter().layout_no_wrap(path_str, path_font, theme.path_color);
    // 鏡像ケース(folder 列挙エラー行・snotra-core/src/folder.rs の error_result は
    // name 空・path 非空): 上段を空白にせず path 1 行を縦中央に単独描画
    //(上の path 空分岐と対称・plan-review scout-egui 指摘)。
    if result.name.is_empty() {
        ui.painter().galley(
            egui::pos2(text_x, rect.center().y - path_galley.size().y / 2.0),
            path_galley,
            theme.path_color,
        );
        return response.clicked();
    }
    // 2 行ブロックを rect 縦中央へ(行間 4.0 は Metrics::row_height の +4.0 と対)
    let total_h = name_galley.size().y + 4.0 + path_galley.size().y;
    let top = rect.center().y - total_h / 2.0;
    let name_h = name_galley.size().y;
    ui.painter().galley(egui::pos2(text_x, top), name_galley, theme.name_color);
    ui.painter().galley(
        egui::pos2(text_x, top + name_h + 4.0),
        path_galley,
        theme.path_color,
    );
    response.clicked()
}

/// アイコン欠落時の fallback（drawn placeholder）。§3.4 は 📁📄 を規定するが softbuffer +
/// 単一 TTF で色 emoji が描けない懸念があるため単色プレースホルダに倒す（視覚スモークは
/// Task 7 に集約・コントローラ決定）。Task 7 の視覚スモークで jp_font が 📁📄 を描けると
/// 確認できたら emoji へ upgrade を検討する。
fn draw_icon_fallback(ui: &egui::Ui, rect: egui::Rect, result: &SearchResult, theme: &RowTheme) {
    let center = egui::pos2(rect.left() + 14.0, rect.center().y);
    let r = egui::Rect::from_center_size(center, egui::vec2(14.0, 14.0));
    let col = if result.is_folder { theme.name_color } else { theme.path_color };
    ui.painter().rect_filled(r, 2.0, col.linear_multiply(0.5));
}

/// path を avail_px におよそ収める中間省略（`C:\a\...\app.exe`）。`per_char_px` は呼び出し側が
/// 実 galley（`Painter::layout_no_wrap`）から実測した平均文字幅を渡す（固定係数 size*0.55 は
/// Latin 想定で CJK グリフ（~1.0-1.8×）を過小評価し under-truncate する・reviewer Important 2）。
/// release は panic=abort ゆえ、`max_chars < 4` ガードと空文字境界で範囲外アクセスを避ける。
pub(crate) fn truncate_middle(s: &str, avail_px: f32, per_char_px: f32) -> String {
    let per = per_char_px.max(1.0);
    let max_chars = (avail_px / per).floor() as usize;
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars || max_chars < 4 {
        return s.to_string();
    }
    let keep = max_chars - 1; // '…' の分
    let head = keep / 2;
    let tail = keep - head;
    let mut out: String = chars[..head].iter().collect();
    out.push('…');
    out.extend(&chars[chars.len() - tail..]);
    out
}

/// 1 結果行の描画テーマ（config テーマ値から都度導出・#576 と同設計でキャッシュしない）。
pub(crate) struct RowTheme {
    pub name_color: egui::Color32,
    pub path_color: egui::Color32,
    pub selection: egui::Color32,
    pub name_size: f32,
    pub path_size: f32,
}

impl snotra_egui_runtime::EguiView for ResultsView {
    fn setup(&mut self, context: &egui::Context) {
        // 日本語フォント: main と同じ config font_family を適用（ctx は窓ごとに独立）。
        let font_family = self
            .app_handle
            .try_state::<crate::AppState>()
            .map(|s| s.engine.lock().unwrap().config().visual.font_family.clone())
            .unwrap_or_else(|| "Segoe UI".to_string());
        crate::egui_shell::view::configure_japanese_font(context, &font_family);
        self.applied_font_family = font_family;
        // 外部 wake 用に ctx を登録（main の egui_ctx と同型・EguiShellState.results_ctx）。
        if let Some(sh) = self.app_handle.try_state::<EguiShellState>()
            && let Ok(mut guard) = sh.results_ctx.lock()
        {
            *guard = Some(context.clone());
        }
    }

    fn update(&mut self, ui: &mut egui::Ui, _frame: &mut snotra_egui_runtime::RuntimeFrame) {
        let Some(shared) = self.app_handle.try_state::<ResultsShared>() else {
            return;
        };
        let snapshot = shared.snapshot.lock().unwrap().clone();
        if !snapshot.show {
            // #632 の不変条件「再表示後に確実に一度 scroll し直す」の実ゲート。旧実装は main
            // 側の resetForShow（reset_pending 消費）でこの gate を直接クリアしていたが、
            // 移設後は本 view が gate の唯一の所有者ゆえここでリセットする——hide/非表示の
            // たびに戻し、次に見えるフレームで選択行への scroll_to_me を再度発火させる。
            self.last_scrolled_selected = None;
            return; // 窓は main が hide 済みのはず(backstop で何も描かない)
        }
        // font_family hot-reload(view.rs の applied_font_family 比較と同型を複製・
        // ctx は窓ごとに独立なため main 側の適用はこの窓に効かない)。
        let font_family = self
            .app_handle
            .try_state::<crate::AppState>()
            .map(|s| s.engine.lock().unwrap().config().visual.font_family.clone())
            .unwrap_or_else(|| "Segoe UI".to_string());
        if font_family != self.applied_font_family {
            crate::egui_shell::view::configure_japanese_font(ui.ctx(), &font_family);
            self.applied_font_family = font_family;
        }
        let theme = row_theme(&self.app_handle);
        let metrics = crate::egui_shell::read_metrics(&self.app_handle);
        let show_icons = false; // Task 5 でアイコン移設(この Task は placeholder 描画)
        // #632 reviewer Important 3 の後継（Fix 3）: 結果集合が総入れ替えされた（main が
        // snapshot_generation を進めた）フレームは、selected の値が変わらなくても scroll gate を
        // 強制リセットする——selected のみの比較では「打鍵で結果が丸ごと変わったが selected は
        // 偶然 0 のまま」を検出できず、選択行への scroll_to_me が発火しないままになるため。
        if snapshot.generation != self.last_generation {
            self.last_scrolled_selected = None;
            self.last_generation = snapshot.generation;
        }
        // 選択変化時のみ scroll_to_me(#632 のゲートを view 内フィールドで維持)。
        let do_scroll = self.last_scrolled_selected != Some(snapshot.selected);
        let mut clicked: Option<usize> = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, result) in snapshot.rows.iter().enumerate() {
                let sel = i == snapshot.selected;
                if draw_result_row(
                    ui,
                    result,
                    sel,
                    sel && do_scroll,
                    None, // icon: Task 5 まで常に placeholder
                    show_icons,
                    &theme,
                    metrics.row_height as f32,
                ) {
                    clicked = Some(i);
                }
            }
        });
        if do_scroll {
            self.last_scrolled_selected = Some(snapshot.selected);
        }
        // クリック逆流(決定 5): 共有スロットへ積み、main を起こして起動処理させる。
        // ToastAction と同じ遅延 dispatch 型——起動ロジックは main の一箇所に保つ。
        if let Some(i) = clicked {
            *shared.clicked.lock().unwrap() = Some(i);
            crate::egui_shell::wake_view(&self.app_handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::truncate_middle;

    #[test]
    fn truncate_middle_shortens_long_path() {
        // 第3引数は per_char_px（実測 galley 幅から呼び出し側が導出する平均文字幅）。
        // size*0.55 概算値だった旧シグネチャの名残で 11.0 を使うが、意味は「1 文字の
        // 実測幅」に変わった（#632 reviewer Important 2）。
        let long = r"C:\Users\Eoh\AppData\Local\Programs\app\bin\tool.exe";
        let out = truncate_middle(long, 100.0, 11.0);
        assert!(out.chars().count() < long.chars().count(), "省略される");
        assert!(out.contains('…'), "中間省略記号を含む");
        // 短い文字列・極小幅は原文（max_chars<4 ガード）。
        assert_eq!(truncate_middle("a.exe", 1.0, 11.0), "a.exe");
        assert_eq!(truncate_middle("short", 1000.0, 11.0), "short");
        // 空文字列は範囲外アクセスなく原文（空文字）を返す（reviewer Minor）。
        assert_eq!(truncate_middle("", 50.0, 11.0), "");
    }
}
