//! egui 検索ウィンドウの純粋レイアウト/タイミングヘルパー（#532 SU3）。ウィンドウ高さ算出と
//! 検索 debounce の判定を egui/Win32 非依存で持つ。ユニットテスト対象。

use std::time::Duration;

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
}
