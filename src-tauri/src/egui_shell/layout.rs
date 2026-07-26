//! egui 検索ウィンドウの純粋レイアウト/タイミングヘルパー（#532 SU3）。ウィンドウ高さ算出・
//! results 窓の可視性の導出（SPEC §8.6 の 4 連言）・幾何（上端 y・作業領域の残り）・
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
/// **`desired == 0.0` は素通しする。** 0.0 は `present_results` が「hide」と読む契約値で
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

/// results 窓の上端の**物理** y（#752 C1）。`mod.rs::position_results_below_main` の算術部。
///
/// **算出と適用（`set_position`）を分けるために出したのではない**——融合は #675 の判断として
/// 保つ（計算した値を捨てる関数は、次の利用者に写しを書かせる）。ここへ出すのは**式を
/// テスト可能にするため**であり、`mod.rs` 側は Win32 を 1 回だけ読んでこの式を呼ぶ薄い
/// ラッパーのままである。消費者は 2 つ（毎フレームの drive と `Moved` リスナー）で、
/// どちらも同じラッパーを通る。
///
/// `main_scale` は **main 窓の** scale である（`available_below` が取る results 窓の scale とは
/// 別・#675）。**型では区別できない**——取り違えの検出器は無く、守るのはラッパーの単一性と
/// この doc だけである（計画 §7 の受容残余）。
pub fn results_top_y(main_y: i32, main_height_phys: u32, gap_logical: u32, main_scale: f64) -> i32 {
    main_y + main_height_phys as i32 + (f64::from(gap_logical) * main_scale).round() as i32
}

/// results 上端から作業領域の下端までの高さ（**論理 px**・#752 C1）。
/// `mod.rs::results_available_height` の算術部。
///
/// **`.max(0.0)` の床を落とさない**——main が作業領域の外にあると差が負になる。
///
/// `results_scale` は **results 窓の** scale である。tao は `set_inner_size` に渡した
/// `LogicalSize` を**その窓の** `scale_factor()` で物理へ戻すため、main の scale を流用すると
/// 混在 DPI 環境で高さが食い違う（#675）。
///
/// **`#[cfg]` の外に置く**——cfg の内側に置くと「純粋だからテストできる」が構造的に
/// 成り立たなくなる（非 Windows でテストが到達しない）。
pub fn available_below(work_area_bottom_phys: i32, top_y_phys: i32, results_scale: f64) -> f64 {
    (f64::from(work_area_bottom_phys - top_y_phys) / results_scale).max(0.0)
}

/// SPEC §8.6「検索結果ウィンドウの可視性（従属軸）」の 4 連言を、**生の入力から**受け取る
/// （#752 C2）。
///
/// **融合した bool を受け取らない。** 旧 `results_should_show` は連言②「結果が空でない」と
/// ③「通常結果を隠していない」を `show_results` という 1 つの bool へ潰した 3 引数であり、
/// テストが「0 件だから隠れた」と「carve-out だから隠れた」を**区別できなかった**。
/// それを解くことが #752 の実質である。
#[derive(Debug, Clone, Copy)]
pub struct ResultsInputs {
    /// 連言①: main が可視か（`AppState.main_visible`）。
    pub main_visible: bool,
    /// 連言③: 通常結果を隠すか（`search_state::plain_results_hidden` の結果）。
    /// **クリック逆流の消費より前に読んだ値**を渡す（`present_results` の doc）。
    pub plain_hidden: bool,
    /// 連言②の材料: 現在の結果件数。**クリック逆流の消費より後に読む**（同上）。
    pub result_count: usize,
    /// 連言④を②から独立させる唯一の入力（`appearance.effective_visible_rows()`）。
    /// **0 は到達可能である**——本体の config 適用経路は `Config::validate()` を通らず、
    /// 設定 UI の `1..=50` clamp は `config.toml` の手編集を止めない。
    pub max_results: u32,
    pub row_height: f64,
}

/// results 窓の見せ方の決定（#752 C2）。**クランプ前**の値であり、最終的な表示状態ではない
/// ——作業領域による調整は driver が `clamp_results_height` で行う。
///
/// `{ visible: bool, height: f64 }` の struct にはしない。`visible: true, height: 0.0` という
/// 不正状態が構築でき、「高さ 0 は hide」という契約（`clamp_results_height` の doc）と
/// 矛盾するためである。先例は `search_state::EscapeOutcome` と `lifecycle::BlurAction::Rearm`。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResultsPresentation {
    Hidden,
    Visible { desired_height: f64 },
}

/// 4 連言を導く（#752 C2）。判定式の正本はここ 1 か所である。
///
/// **`main_visible` を条件に含めるのが要石である**（#671 PR A′）。main を hide しても
/// `state.results()` は消えない（reset は show 側の `reset_pending` 消費でしか起きない）ため、
/// 結果は hidden 中も残る。hidden 中に main の update() が 1 フレームでも走ると
/// （`config-applied` / `indexing-*` / updater 完了の `wake_main` 自体は main の可視性を
/// 見ない）、results だけが最前面に取り残される。hide 側の同期（`hide_egui_main`）と
/// この show 側のゲートは**対**であり、片方では閉じない。
/// **「hidden 中は update() が走らない」という命題には依存しない**（機構は tao/OS 層の配送
/// 抑止と #697 で実測済みだが、この判定はそれに依存せず成立する）。
///
/// **読み点の非対称は呼び出し側の責務である**（#752 F2）。同一フレーム内で、③ `plain_hidden`
/// はクリック逆流の消費**前**に、②の材料 `result_count` は消費**後**に読む。間に挟まる
/// `start_launch` が `set_results(Vec::new())` を撃つため、行クリック起動フレームでは②が
/// false になって窓が隠れる（旧構造は②を消費前に読んでいたので隠していたのは④だった——
/// **帰結は同じ**）。**読み点を前へ寄せてはならない**——起動直後に古い行が 1 フレーム
/// 描かれる。`cargo test` では落ちない種類の回帰である。
/// **`plain_results_hidden` を前後で 2 回読んでもならない**——`indexing` は `AtomicBool` の
/// live-read で、同一フレーム内でも値が変わりうる。
pub fn present_results(i: ResultsInputs) -> ResultsPresentation {
    let desired_height = results_window_height(i.result_count, i.max_results, i.row_height);
    if i.main_visible && !i.plain_hidden && desired_height > 0.0 {
        ResultsPresentation::Visible { desired_height }
    } else {
        ResultsPresentation::Hidden
    }
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
        // **0 件は 0 のまま**——0.0 は present_results が hide と読む契約値で、
        // クランプが床を当てて作り替えてはならない
        assert_eq!(clamp_results_height(0.0, Some(0.0), row), 0.0);
        assert_eq!(clamp_results_height(0.0, Some(500.0), row), 0.0);
    }

    /// 真理値表を読みやすくするための入力組み立て。`row_height` は固定でよい
    /// （連言④の真偽を動かすのは `result_count` と `max_results` である）。
    fn inputs(main_visible: bool, plain_hidden: bool, count: usize, max: u32) -> ResultsInputs {
        ResultsInputs {
            main_visible,
            plain_hidden,
            result_count: count,
            max_results: max,
            row_height: 37.0,
        }
    }

    /// #752 AC1: SPEC §8.6「検索結果ウィンドウの可視性（従属軸）」の 4 連言の真理値表。
    ///
    /// 連言は ①`main_visible` ②結果が空でない ③通常結果を隠していない ④窓高さ > 0。
    /// **②と③を区別できることが #752 の眼目である**——旧 `results_should_show` は両者を
    /// `show_results` へ潰しており、「0 件で隠れた」と「carve-out で隠れた」を固定できなかった。
    ///
    /// ④を②から独立に false にできる唯一の入力は `max_results = 0`（到達可能・
    /// `ResultsInputs::max_results` の doc）。
    ///
    /// **16 行のうち 4 行は到達不能である。** 「②false ∧ ④true」は生の入力から構成できない
    /// （`result_count = 0` なら高さも 0 になる）。到達不能な行を assert で「検出器」に
    /// 見せかけないため、**構成不能である事実を記述に留める**（#697「トートロジーテスト削除」）。
    #[test]
    fn present_results_truth_table_distinguishes_all_four_conjuncts() {
        use ResultsPresentation::{Hidden, Visible};
        let h = 3.0 * 37.0 + 8.0; // results_window_height(3, 8, 37.0)

        // ①true ③true（plain_hidden = false）
        assert_eq!(present_results(inputs(true, false, 3, 8)), Visible { desired_height: h }); // ②t ④t: 唯一の可視
        assert_eq!(present_results(inputs(true, false, 3, 0)), Hidden); // ②t ④f（max_results=0）
        assert_eq!(present_results(inputs(true, false, 0, 8)), Hidden); // ②f ④f
        // ①true ③false（carve-out で隠す）
        assert_eq!(present_results(inputs(true, true, 3, 8)), Hidden); // ②t ④t だが③で隠れる
        assert_eq!(present_results(inputs(true, true, 3, 0)), Hidden);
        assert_eq!(present_results(inputs(true, true, 0, 8)), Hidden);
        // ①false ③true — 要石: main hidden なら行があっても出さない
        assert_eq!(present_results(inputs(false, false, 3, 8)), Hidden);
        assert_eq!(present_results(inputs(false, false, 3, 0)), Hidden);
        assert_eq!(present_results(inputs(false, false, 0, 8)), Hidden);
        // ①false ③false
        assert_eq!(present_results(inputs(false, true, 3, 8)), Hidden);
        assert_eq!(present_results(inputs(false, true, 3, 0)), Hidden);
        assert_eq!(present_results(inputs(false, true, 0, 8)), Hidden);
    }

    /// #671 PR A′: main が hidden の間は、結果が残っていても results を出さない。
    /// これを落とすと「main は隠れたまま results だけが最前面に残る」（レビュー Important 1）。
    ///
    /// **命題は `results_should_show` 時代から不変**（#752 C2 で `present_results` へ移設）。
    /// 上の真理値表にも同じ行があるが、**この命題は名前で追跡する価値がある**ため独立に残す
    /// ——表から 1 行落ちても、名前付きのこれが落ちる。
    #[test]
    fn results_hidden_while_main_is_hidden_even_with_rows() {
        assert_eq!(present_results(inputs(false, false, 3, 8)), ResultsPresentation::Hidden);
        // 対照: main が可視なら同じ入力で出る（要石が効いているのは①だけだと示す）
        assert!(matches!(
            present_results(inputs(true, false, 3, 8)),
            ResultsPresentation::Visible { .. }
        ));
    }

    /// #752 AC4: 旧実装との等価グリッド。
    ///
    /// 旧式（`results_should_show` + `results_window_height`）を**テストローカルのクロージャ**
    /// として再現し、直積で新実装と突き合わせる。production に旧関数を残す形にはしない
    /// ——`-D warnings` の `dead_code` で落ちるうえ、導出が 2 か所になる（`AGENTS.md`）。
    /// クロージャならこのコミットの中に閉じる。**期待値を手計算で literal に書き写さない**
    /// （直積の転記ミスがそこに湧く）。
    ///
    /// **このグリッドが固定するのは「pre-click 件数 == post-click 件数」のフレームに限られる。**
    /// 行クリック起動フレーム（`start_launch` が結果を空にするため pre ≠ post）の等価性は
    /// グリッドでは表現できず、`present_results` の doc に書いた論証が担う（#752 F2 / AC5）。
    #[test]
    fn present_results_matches_legacy_predicate_over_input_grid() {
        // 削除前の `results_should_show` を逐語で再現する。
        let legacy = |main_visible: bool, show_results: bool, results_height: f64| -> bool {
            main_visible && show_results && results_height > 0.0
        };
        let row = 37.0;
        for &main_visible in &[false, true] {
            for &plain_hidden in &[false, true] {
                for &count in &[0usize, 1, 3, 20] {
                    for &max in &[0u32, 1, 8] {
                        let i = inputs(main_visible, plain_hidden, count, max);
                        let desired = results_window_height(count, max, row);
                        // 旧経路の `show_results` は「件数 > 0 ∧ carve-out でない」の融合だった。
                        let show_results = count > 0 && !plain_hidden;
                        let expected_visible = legacy(main_visible, show_results, desired);
                        match present_results(i) {
                            ResultsPresentation::Visible { desired_height } => {
                                assert!(expected_visible, "新は可視・旧は不可視: {i:?}");
                                assert_eq!(desired_height, desired, "高さが旧と違う: {i:?}");
                            }
                            ResultsPresentation::Hidden => {
                                assert!(!expected_visible, "新は不可視・旧は可視: {i:?}");
                            }
                        }
                    }
                }
            }
        }
    }

    /// #752 C1: 上端 = main の下端 + gap（論理 → 物理へ換算して四捨五入）。
    ///
    /// **`.round()` の境界を通す入力を必ず含める。** `gap × scale` が整数になる組み合わせ
    /// （gap ∈ {0,4,16} × scale ∈ {1.0,1.5,2.0} など）だけでは、丸めを**一度も検査しない**
    /// テストになる。Rust の `f64::round` は half を 0 から遠い側へ倒す。
    #[test]
    fn results_top_y_rounds_gap_at_half_boundary() {
        // scale 1.0: 換算なし
        assert_eq!(results_top_y(100, 43, 4, 1.0), 147);
        // 積がちょうど x.5（丸めの境界）
        assert_eq!(results_top_y(0, 0, 3, 1.5), 5); // 4.5 → 5
        assert_eq!(results_top_y(0, 0, 1, 1.5), 2); // 1.5 → 2
        assert_eq!(results_top_y(0, 0, 3, 0.5), 2); // 1.5 → 2
        // x.5 未満は切り捨て側へ
        assert_eq!(results_top_y(0, 0, 2, 1.2), 2); // 2.4 → 2
        // 高 DPI
        assert_eq!(results_top_y(100, 43, 4, 2.0), 151);
        // main が負座標のモニターにいる（マルチモニターで左/上に並べた配置）
        assert_eq!(results_top_y(-1080, 43, 4, 1.0), -1033);
    }

    /// #752 C1: results 上端から作業領域下端までの論理高さ。
    ///
    /// **0 床を落とさない**——main が作業領域の外にあると差が負になる。負値をそのまま
    /// `clamp_results_height` へ渡すと `avail.max(row+8)` が床へ倒れて意味が変わる。
    #[test]
    fn available_below_divides_by_results_scale_and_floors_at_zero() {
        assert_eq!(available_below(1000, 400, 1.0), 600.0);
        assert_eq!(available_below(1000, 400, 2.0), 300.0); // 物理 600 → 論理 300
        assert_eq!(available_below(1000, 400, 1.5), 400.0);
        // 上端が作業領域の下端より下（main が画面外）→ 負にせず 0
        assert_eq!(available_below(1000, 1200, 1.0), 0.0);
        assert_eq!(available_below(1000, 1200, 2.0), 0.0);
        // ちょうど 0
        assert_eq!(available_below(1000, 1000, 1.0), 0.0);
    }
}
