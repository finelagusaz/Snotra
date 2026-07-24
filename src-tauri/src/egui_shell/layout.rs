//! egui 検索ウィンドウの純粋レイアウト/タイミングヘルパー（#532 SU3）。ウィンドウ高さ算出と
//! 検索 debounce の判定を egui/Win32 非依存で持つ。ユニットテスト対象。

use std::time::Duration;

/// path 行のフォントサイズ(#646 決定 9)。view.rs `RowTheme::path_size` と同係数——
/// 正本はここ(layout の Metrics が同じ値で行高を積算するため。二重定義は行高と描画の
/// 不一致バグになる)。driver(view.rs)から Task 3・4 で配線済み。
pub fn path_size(font_size: u32) -> f64 {
    (font_size as f64 * 0.78).max(9.0)
}

/// 行高・バー高・toast 高の算出値(#646 決定 2)。config `visual` から毎フレーム導出し
/// キャッシュしない(font_size と同じ live-read 方針)。driver からの消費は Task 3・4 で配線済み。
pub struct Metrics {
    /// font_size + bar_padding。既定(15+28)=43、font 24 で現行 52 を再現。
    pub bar_height: f64,
    /// 2 行表示(決定 9)の積算: font_size + path_size + row_padding + 行間 4。下限 24。
    pub row_height: f64,
    /// bar_height と同値(§20.3 の toast 行)。
    pub toast_height: f64,
}

impl Metrics {
    pub fn from_config(font_size: u32, row_padding: u32, bar_padding: u32) -> Self {
        let f = font_size as f64;
        let bar_height = f + bar_padding as f64;
        let row_height = (f + path_size(font_size) + row_padding as f64 + 4.0).max(24.0);
        Self { bar_height, row_height, toast_height: bar_height }
    }
}

/// compute_window_height の入力。SU5 まで has_update_toast は常に false。
pub struct HeightParams {
    pub show_results: bool,
    pub max_results: u32,
    pub has_update_toast: bool,
    pub search_bar_height: f64,
    pub result_row_height: f64,
    pub results_padding: f64,
    pub update_toast_height: f64,
}

/// ウィンドウ論理高さ。show_results なら bar + max*row + pad、否なら bar。toast は加算。
/// windowHeight.ts の computeWindowHeight と同一。
pub fn compute_window_height(p: &HeightParams) -> f64 {
    let content = if p.show_results {
        p.search_bar_height + p.max_results as f64 * p.result_row_height + p.results_padding
    } else {
        p.search_bar_height
    };
    content + if p.has_update_toast { p.update_toast_height } else { 0.0 }
}

/// 打鍵 debounce（決定7）。時刻は driver が注入する（純粋・テスト可能）。
/// - on_input: 入力フレーム。leading（バースト先頭）なら true を返し、以後 armed。
/// - poll: 各フレーム。armed かつ elapsed>=interval で trailing 発火し disarm。
pub struct Debouncer {
    interval: Duration,
    leading: bool,
    armed: bool,
}

impl Debouncer {
    pub fn new(interval: Duration, leading: bool) -> Self {
        Self { interval, leading, armed: false }
    }

    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// armed のまま（trailing 未発火）か。呼び出し側が repaint deadline の再要求要否を
    /// 判定するために使う（coalescing 対策・#532 SU3 M1 レビュー）。
    pub fn is_armed(&self) -> bool {
        self.armed
    }

    /// 入力があったフレームで呼ぶ。leading 有効かつバースト先頭なら true（＝今すぐ検索）。
    /// いずれにせよ armed にして trailing を予約する。
    pub fn on_input(&mut self) -> bool {
        let fire_leading = self.leading && !self.armed;
        self.armed = true;
        fire_leading
    }

    /// 入力の無いフレームで呼ぶ。前回入力からの経過が interval 以上なら trailing 発火し disarm。
    pub fn poll(&mut self, elapsed_since_input: Duration) -> bool {
        if self.armed && elapsed_since_input >= self.interval {
            self.armed = false;
            return true;
        }
        false
    }

    /// armed を解除し予約済み trailing を取り消す（SolidJS cancelDebounce parity・#532 SU3 M3）。
    /// instant/command モード突入時に driver が呼ぶ——モード外で予約された検索が
    /// モード中に遅延発火する経路を塞ぐ（run_search は再導出ゆえ実害は無いが無駄撃ちを消す）。
    pub fn cancel(&mut self) {
        self.armed = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(show: bool, max: u32) -> HeightParams {
        HeightParams {
            show_results: show,
            max_results: max,
            has_update_toast: false,
            search_bar_height: 52.0,
            result_row_height: 30.0,
            results_padding: 8.0,
            update_toast_height: 52.0,
        }
    }

    #[test]
    fn collapsed_is_search_bar_only() {
        assert_eq!(compute_window_height(&params(false, 8)), 52.0);
    }

    #[test]
    fn expanded_is_bar_plus_rows_plus_padding() {
        // 52 + 8*30 + 8 = 300
        assert_eq!(compute_window_height(&params(true, 8)), 300.0);
    }

    #[test]
    fn toast_adds_height() {
        let mut p = params(false, 8);
        p.has_update_toast = true;
        assert_eq!(compute_window_height(&p), 52.0 + 52.0);
    }

    #[test]
    fn leading_fires_on_burst_start_then_trailing() {
        let mut d = Debouncer::new(Duration::from_millis(50), true);
        assert!(d.on_input(), "バースト先頭は leading 発火");
        assert!(!d.on_input(), "連打中は leading 抑止");
        assert!(!d.poll(Duration::from_millis(30)), "50ms 未満は trailing 抑止");
        assert!(d.poll(Duration::from_millis(50)), "50ms 経過で trailing 発火");
        assert!(!d.poll(Duration::from_millis(100)), "発火後は disarm");
    }

    #[test]
    fn no_leading_mode_only_trailing() {
        let mut d = Debouncer::new(Duration::from_millis(30), false);
        assert!(!d.on_input(), "leading なしは即時発火しない");
        assert!(d.poll(Duration::from_millis(30)), "trailing のみ発火");
    }

    #[test]
    fn cancel_disarms_pending_trailing() {
        let mut d = Debouncer::new(Duration::from_millis(50), true);
        d.on_input();
        assert!(d.is_armed());
        d.cancel();
        assert!(!d.is_armed());
        assert!(!d.poll(Duration::from_millis(100)), "cancel 後は trailing 発火しない");
        // cancel 後の次入力はバースト先頭扱い（leading 再発火）
        assert!(d.on_input());
    }

    /// #646 決定 2: bar_padding=28 は font 24 で現行 52px をピクセル再現する(後方互換の要)。
    #[test]
    fn metrics_bar_reproduces_current_at_font24() {
        let m = Metrics::from_config(24, 6, 28);
        assert_eq!(m.bar_height, 52.0);
        assert_eq!(m.toast_height, 52.0);
    }

    /// #646 決定 2・9: row_height は 2 行積算(name 行 + path 行 + 行間 4 + row_padding)。
    #[test]
    fn metrics_row_is_two_line_sum() {
        let m = Metrics::from_config(15, 6, 28);
        assert_eq!(m.bar_height, 43.0);
        // path_size = max(15*0.78, 9) = 11.7 → 15 + 11.7 + 6 + 4 = 36.7
        assert!((m.row_height - 36.7).abs() < 1e-9, "row={}", m.row_height);
    }

    /// #646 決定 2: 下限 24(アイコン 16px + 余白)。8 + 9 + 0 + 4 = 21 → 24 へ床上げ。
    #[test]
    fn metrics_row_floor_is_24() {
        assert_eq!(Metrics::from_config(8, 0, 28).row_height, 24.0);
    }

    /// path_size は RowTheme と同係数(0.78・下限 9)。
    #[test]
    fn path_size_matches_row_theme_coefficient() {
        assert_eq!(path_size(8), 9.0);
        assert!((path_size(15) - 11.7).abs() < 1e-9);
    }
}
