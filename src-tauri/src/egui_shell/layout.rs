//! egui 検索ウィンドウの純粋レイアウト/タイミングヘルパー（#532 SU3）。ウィンドウ高さ算出と
//! 検索 debounce の判定を egui/Win32 非依存で持つ。ユニットテスト対象。

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
}
