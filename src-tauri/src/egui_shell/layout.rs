//! egui 検索ウィンドウの純粋レイアウト/タイミングヘルパー（#532 SU3）。ウィンドウ高さ算出と
//! 検索 debounce の判定を egui/Win32 非依存で持つ。ユニットテスト対象。

use std::time::Duration;

/// path 行のフォントサイズ(#646 決定 9)。view.rs `RowTheme::path_size` と同係数——
/// 正本はここ(layout の Metrics が同じ値で行高を積算するため。二重定義は行高と描画の
/// 不一致バグになる)。driver(view.rs)から Task 3・4 で配線済み。
pub fn path_size(font_size: u32) -> f64 {
    (font_size as f64 * 0.78).max(9.0)
}

/// status 行・updater トーストの文字サイズ（#672）。SPEC §11 の規範「文字サイズに固定値を
/// 書かない・補助要素は `font_size` からの比率で導く」の適用。
///
/// **係数がパス行（0.78）と違うのは意図である。** パスは行の副次情報だが、status 行と
/// トーストは**その行の主メッセージ**であり、読ませる必要がある。0.87 は既定
/// `font_size = 15` で 13.05px となり、固定値だった 13px と実質同じ見た目を保つ——
/// この変更は「連動するようになった」だけで、既定設定の利用者の見た目は変えない。
pub fn status_size(font_size: u32) -> f64 {
    (font_size as f64 * 0.87).max(11.0)
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
    /// バー帯の内側に取る**四辺一様**の余白(#646 PR2・実機目視で追加)。入力欄はこの余白の
    /// 内側いっぱい(高さ `bar_height - 2*bar_inset`)を占めるため、上下左右の見た目の枠が
    /// 等しくなる。`bar_padding / 4` は「font_size に対する入力欄の内部余白」も同値に保つ
    /// 導出(既定 28/4=7 のとき font 15 で欄高 29・font 24 で欄高 38——どちらも文字の上下に 7)。
    pub bar_inset: f64,
}

impl Metrics {
    pub fn from_config(font_size: u32, row_padding: u32, bar_padding: u32) -> Self {
        let f = font_size as f64;
        let bar_height = f + bar_padding as f64;
        let row_height = (f + path_size(font_size) + row_padding as f64 + 4.0).max(24.0);
        Self {
            bar_height,
            row_height,
            toast_height: bar_height,
            bar_inset: bar_padding as f64 / 4.0,
        }
    }
}

/// main 窓の高さ(#646 PR2 決定 6)。bar(+status/toast)のみで結果に伸縮しない。
///
/// `status_height`: indexing 案内・起動中・一時通知の行(#700)。**入力欄に重ねず独立した行を
/// 占める**——重ね描きは「編集できるのに文字が見えない」状態を作り、実際に編集不能と報告された
/// (#700 発見 C)。`toast_height`: updater toast の行(§20.3)。
///
/// 両者は独立に積む。同時成立時にどちらかを畳むと、畳んだ側の情報が黙って消える。
pub fn main_window_height(
    bar_height: f64,
    status_height: Option<f64>,
    toast_height: Option<f64>,
) -> f64 {
    bar_height + status_height.unwrap_or(0.0) + toast_height.unwrap_or(0.0)
}

/// 結果窓の高さ(#646 PR2 決定 7)。実件数フィット・上限 max_results・padding 8。
/// 0 件は 0.0(呼び出し側が hide する契約)。
pub fn results_window_height(result_count: usize, max_results: u32, row_height: f64) -> f64 {
    let n = result_count.min(max_results as usize);
    if n == 0 {
        0.0
    } else {
        n as f64 * row_height + 8.0
    }
}

/// 結果窓の高さを作業領域の下端で抑える（#675）。単位はすべて**論理 px**。
///
/// - `desired`: `results_window_height` の値
/// - `available`: 結果窓の上端から作業領域下端まで。**`None` はクランプしない**
///   （非 Windows・作業領域の取得失敗。従来どおりの挙動へ倒す）
/// - `row_height`: 1 行の高さ
///
/// **`desired == 0.0` は素通しする。** 0.0 は `results_should_show` が「hide」と読む契約値で
/// あり（同関数）、クランプの結果として 0 を**作ってはならない**し、0 を**消してもならない**。
///
/// `available` が 1 行に満たなくても **1 行 + padding 8 を床**にする。ここで 0 まで潰すと
/// 「main を画面下端へ置くと結果が一切出ない」という別の欠陥になる。床を割ったぶんの
/// はみ出しは受容する（行はスクロールで到達できる）。
pub fn clamp_results_height(desired: f64, available: Option<f64>, row_height: f64) -> f64 {
    let Some(avail) = available else { return desired };
    if desired <= 0.0 {
        return desired;
    }
    desired.min(avail.max(row_height + 8.0))
}

/// results 窓を表示してよいか（#671 PR A′・レビュー Important 1）。
///
/// **`main_visible` を条件に含めるのが要石である。** main を hide しても `state.results()` は
/// 消えない（reset は show 側の `reset_pending` 消費でしか起きない）ため、`show_results` は
/// hidden 中も true のまま残る。hidden 中に main の update() が 1 フレームでも走ると
/// （`config-applied` / `indexing-*` / updater 完了の `wake_main` は main の可視性に関係なく
/// 発火する）、results だけが最前面に取り残される。
///
/// PR A′ 以前はこの事故を `SearchWindowView` の view-local な可視フラグが偶然に防いでいた
/// ——`hide_egui_main` から到達できず stale な true のまま残るため show を skip していた。
/// 可視フラグを `ResultsWindow` へ移して正直に false にした結果、その防波堤が消えた。
/// **「hidden 中は update() が走らない」という命題には依存しない**（機構は tao/OS 層の
/// 配送抑止と #697 で実測済みだが、この判定はそれに依存せず成立する）。
pub fn results_should_show(main_visible: bool, show_results: bool, results_height: f64) -> bool {
    main_visible && show_results && results_height > 0.0
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

    /// #672: **文字サイズが font_size に連動する**こと自体を固定する（SPEC §11 の規範
    /// 「文字サイズに固定値を書かない」）。固定値へ戻す変更はここで落ちる。
    #[test]
    fn aux_text_sizes_scale_with_font_size() {
        // 既定 font_size=15 で従来の固定値 13px と実質同じ（この変更は見た目を変えない）。
        assert!((status_size(15) - 13.05).abs() < 0.01, "既定で 13px 相当を保つ");
        // font_size を上げれば追従する（固定値なら 15 と 24 で同値になり、ここが落ちる）。
        assert!(status_size(24) > status_size(15), "font_size に連動する");
        assert!((status_size(24) - 20.88).abs() < 0.01);
        // 補助要素どうしの序列: パス行 < status 行 < 主要素（font_size 等倍）。
        assert!(path_size(15) < status_size(15), "パス行より大きい（行の主メッセージゆえ）");
        assert!(status_size(15) < 15.0, "主要素より小さい");
        // 極小 font_size でも読める下限を持つ（path_size の 9.0 と同型の防御）。
        assert_eq!(status_size(1), 11.0);
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

    /// #646 PR2: バー内の一様余白。入力欄は帯の内側いっぱい（bar_height - 2*inset）を占める。
    #[test]
    fn bar_inset_leaves_symmetric_room_for_field() {
        let m = Metrics::from_config(15, 6, 28);
        assert_eq!(m.bar_inset, 7.0);
        assert_eq!(m.bar_height - 2.0 * m.bar_inset, 29.0); // 文字 15 + 上下 7 ずつ
        // font 24（旧 52px バー）でも内部余白は同値に保たれる
        let big = Metrics::from_config(24, 6, 28);
        assert_eq!(big.bar_inset, 7.0);
        assert_eq!(big.bar_height - 2.0 * big.bar_inset, 38.0);
    }

    /// #646 PR2 決定 6: main 窓は bar(+status/toast)のみで、結果による伸縮をしない。
    /// #700: status 行（indexing 案内・一時通知）は入力欄を覆わず独立した行を占める。
    #[test]
    fn main_height_is_bar_plus_optional_status_and_toast() {
        assert_eq!(main_window_height(43.0, None, None), 43.0);
        assert_eq!(main_window_height(43.0, None, Some(43.0)), 86.0);
        // status 行だけ（indexing 中・updater toast 無し）。
        assert_eq!(main_window_height(43.0, Some(43.0), None), 86.0);
        // 両方（indexing 中に更新 toast が出た）: 独立に積む——どちらも隠さない。
        assert_eq!(main_window_height(43.0, Some(43.0), Some(43.0)), 129.0);
    }

    /// #646 PR2 決定 7: 結果窓は実件数フィット(上限 max_results)+ padding 8。
    #[test]
    fn results_height_fits_actual_count_capped_at_max() {
        let row = 37.0;
        assert_eq!(results_window_height(3, 8, row), 3.0 * row + 8.0); // 実件数
        assert_eq!(results_window_height(8, 8, row), 8.0 * row + 8.0); // ちょうど境界(result_count == max_results)
        assert_eq!(results_window_height(20, 8, row), 8.0 * row + 8.0); // 上限で頭打ち
        assert_eq!(results_window_height(0, 8, row), 0.0); // 0 件は非表示(高さ 0 = 呼び出し側で hide)
    }

    /// #675: 作業領域の下端で抑える。**両端を固定する**——抑えすぎると「下端に置くと結果が
    /// 一切出ない」、抑えなさすぎると元の欠陥（タスクバーの下へ潜る）に戻る。
    #[test]
    fn results_height_is_clamped_at_work_area_bottom() {
        let row = 37.0;
        let floor = row + 8.0; // 1 行 + padding
        // 取得できないときは従来どおり（非 Windows・API 失敗）
        assert_eq!(clamp_results_height(300.0, None, row), 300.0);
        // 余裕があれば素通し（既存挙動と同一）
        assert_eq!(clamp_results_height(300.0, Some(500.0), row), 300.0);
        // 下端で切る
        assert_eq!(clamp_results_height(300.0, Some(120.0), row), 120.0);
        // 1 行に満たなくても 1 行は出す（0 まで潰さない）
        assert_eq!(clamp_results_height(300.0, Some(20.0), row), floor);
        // main が作業領域の外にある（available が負）
        assert_eq!(clamp_results_height(300.0, Some(-50.0), row), floor);
        // **0 件は 0 のまま**——0.0 は results_should_show が hide と読む契約値で、
        // クランプが床を当てて作り替えてはならない
        assert_eq!(clamp_results_height(0.0, Some(0.0), row), 0.0);
        assert_eq!(clamp_results_height(0.0, Some(500.0), row), 0.0);
    }

    /// #671 PR A′: main が hidden の間は、結果が残っていても results を出さない。
    /// これを落とすと「main は隠れたまま results だけが最前面に残る」（レビュー Important 1）。
    #[test]
    fn results_hidden_while_main_is_hidden_even_with_rows() {
        assert!(!results_should_show(false, true, 120.0)); // 要石: main hidden なら常に false
        assert!(results_should_show(true, true, 120.0)); // 通常の表示条件
        assert!(!results_should_show(true, false, 120.0)); // 表示ゲート（plain_results_hidden 等）
        assert!(!results_should_show(true, true, 0.0)); // 0 件（高さ 0）
    }
}
