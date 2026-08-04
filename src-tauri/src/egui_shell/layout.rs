//! egui 検索ウィンドウの純粋レイアウト/タイミングヘルパー（#532 SU3）。ウィンドウ高さ算出・
//! results 窓の可視性の導出（SPEC §8.6 の 4 連言）・幾何（上端 y・作業領域の残り）・
//! 検索 debounce の判定・**表示幅に合わせたテキストの中間省略**（`truncate_middle_chars` /
//! `fit_middle_by_measure`・#870）を egui/Win32 非依存で持つ。ユニットテスト対象。

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
    let Some(avail) = available else {
        return desired;
    };
    if desired <= 0.0 {
        return desired;
    }
    desired.min(avail.max(row_height + 8.0))
}

/// results 窓の上端の**物理** y（#752 C1）。`window_coordinator::position_results_below_main`
/// の算術部（#749 で `mod.rs` から移設）。
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
/// `window_coordinator::results_available_height` の算術部（#749 で `mod.rs` から移設）。
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

/// バー矩形の**物理**高さ（#738）。可視中の位置クランプの材料である。
///
/// **実高（status 行・toast 行を含む高さ）ではない。** 実高でクランプすると toast が消えて
/// 窓が縮んだときに位置が戻らず、hide が `read_placement_relative` 経由でそのずれを
/// 永続化する（#755 / #801 で実際に起きた失敗）。「バーの位置はユーザーが決め、行の出没では
/// 動かさない」（人間裁定・`SPEC.md` §4.7）の帰結でもあり、**`show_egui_main` が位置決めの
/// 1 手目で畳む高さと同じ導出である**——書き手が 2 人でも材料は 1 つに保つ（#877 と同型）。
///
/// **返すのは content 高さである。** 非クライアント領域の分は呼び出し側が
/// `outer_size() - inner_size()` として足す。show 経路は `position_on_target_monitor` が
/// `outer_size()` を読み戻すため OS から供給されるが、**フレームループ経路にその読み戻しは
/// 無い**——ここで返した値をそのまま `WorkArea::clamp` の `win_h`（物理 outer）へ渡すと、
/// 装飾が付いた将来の変更で静かに数 px ずれる。
///
/// **show 経路との一致は `dpi` crate の丸め規則に依存する。** あちらは `set_size` に渡した
/// `LogicalSize` を tao/dpi が物理へ戻した結果を `outer_size()` で読むだけで、その変換は
/// `Pixel::from_f64`（= `round`）である。ここの `.round()` と同じ規則ゆえ今日は一致するが、
/// **一致を固定するテストは書けない**（読み戻し側が OS と tao を跨ぐ）。上流が銀行家丸めへ
/// 変われば 1 px 食い違い、show 直後の 1 フレームで位置がスナップする（#755 / #801 と同じ
/// 観測形）。依存先を名指しておくのが最小の担保である。
pub fn bar_rect_height_phys(bar_height_logical: f64, main_scale: f64) -> i32 {
    (bar_height_logical * main_scale).round() as i32
}

/// バー矩形の中心（物理座標・#738）。クランプの**基準モニター**を決めるために使う。
///
/// **窓全体の矩形から基準モニターを決めてはならない。** `MonitorFromWindow`
/// （= `monitor::window_monitor_work_area`）は重なりの大きいモニターを返すため、上下に
/// 並んだモニターで status 行 + toast 行が出て下側へ大きく伸びると基準が下側へ移り、
/// **行が出ただけでバーが隣モニターへ飛ぶ**——本修正自身が #904 の裁定を破ることになる。
/// バー矩形は行の出没で変わらないので、ここから決めれば基準も動かない。
pub fn bar_rect_center(x: i32, y: i32, width_phys: u32, bar_height_phys: i32) -> (i32, i32) {
    (x + (width_phys / 2) as i32, y + bar_height_phys / 2)
}

/// 窓の再サイズが要るか（#749）。`set_size` は Win32 呼び出しゆえ、同値のフレームで撃たない
/// ためのデルタガードである。
///
/// **correctness のフラグではない**——results の可視性は `ResultsWindow` の `visible` が持つ
/// （#671 spec 決定 2 の意図的な分割）。ここが誤って `false` を返しても窓が消えることはなく、
/// 直近に適用したサイズのまま残るだけである。
///
/// 許容 `0.5` は論理 px の丸め差を吸収する値で、#646 PR2 から手書きされていた式をそのまま
/// 移した。引数は `(幅, 高さ)`——`set_size(width, height)` の引数順と揃える。
///
/// 消費者は 2 つある（**状態は共有しない。式だけを共有する**）: `ResultsWindow::set_size`
/// （memo は窓の所有型が持つ）と `view.rs` の main 窓ガード（memo は view が持つ——main の高さの
/// 導出が show 経路（`show_egui_main`）と共有される事実の正本は `src-tauri/CLAUDE.md`「モジュール構成」の
/// `window_coordinator.rs` の項。ここでの結論は**memo は共有しない**ことだけである）。`main_size` を
/// results の導出へ入れないという `ADR-results-presentation-two-stage` 却下 1 の**結論**は今も真である
/// （却下 1 の第 3 理由が挙げた「意図的な 2 導出」の部分は `ADR-show-path-derives-drawn-height` が反転させた）。
pub fn size_delta_exceeds(prev: (f64, f64), next: (f64, f64)) -> bool {
    (next.0 - prev.0).abs() > 0.5 || (next.1 - prev.1).abs() > 0.5
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
/// 見ない）、results だけが最前面に取り残される。
///
/// **並行性は条件に入らない。** 証人型（`EventLoopProof`）を引数に要求する 5 関数はイベント
/// ループスレッドへ閉じており、この判定と `ResultsWindow::show` のあいだに hide が割り込む
/// 並びは**構築できない**。**閉包の射程の正本は `src-tauri/CLAUDE.md`「可視性を変える操作は
/// イベントループスレッドに閉じてある」の bullet 群である**——5 関数の外側に何が残るかもそこ。
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
        Self {
            interval,
            leading,
            armed: false,
        }
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

/// 中間省略を行う最小の文字数。**`results_view::truncate_middle` が元から持っていた
/// `max_chars < 4` ガードとの parity で 4 に固定する**——値を動かすと結果行の見え方が変わる
/// （3 でも `a…z` は構成できるので、4 は「構成可能性」ではなく parity が根拠である）。
///
/// **これ未満では `truncate_middle_chars` が原文を返す**ため、「`max_chars` を減らしたのに
/// 幅が跳ね上がる」非単調な段ができる。`fit_middle_by_measure` の二分探索が探索範囲を
/// ここで打ち切るのは、その段を踏まないためである。
pub const MIN_MIDDLE_KEEP: usize = 4;

/// `s` を `max_chars` 字におよそ収める中間省略（`C:\a\…\app.exe`）。head と tail を等分し、
/// 間に `…`（U+2026）を挟む。
///
/// **`results_view::truncate_middle` の中核でもある**（px ベースの API はあちらが持ち、
/// 文字数への換算だけをあちらが行う）。head/tail 分割の実装を 2 本並べないための集約。
///
/// release は `panic="abort"` ゆえ、`max_chars < MIN_MIDDLE_KEEP` ガードと空文字境界で
/// 範囲外アクセスを避ける。
pub fn truncate_middle_chars(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars || max_chars < MIN_MIDDLE_KEEP {
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

/// `dir` を中間省略し、`measure` の返す実幅が `avail_px` に収まる**最長**の候補を返す（#870）。
///
/// `measure` は「候補を書式へ埋めた文字列（＝ hint 全体）の実幅」を返す。**固定部
/// （接頭辞・接尾辞）の幅を別に推定しなくて済む**のがこの注入の要点で、
/// `ADR-folder-location-display-surface` の却下理由 2（「接尾辞の実幅も別途推定が要る」）は
/// これで消える。CJK も呼び出し側のフォント実寸に乗る。
///
/// **1 件でも収まる候補があれば、返す候補は必ず実測で収まっている**——収まった候補だけを
/// `best` へ記録するため、幅が `max_chars` に対して僅かに非単調でも（`…` が消した 2 字より
/// 広い場合）、収まらない候補を返さない。平均文字幅で 1 回割る近似はこの保証を持たず、
/// 溢れれば egui の末尾省略が leaf を削る＝ #870 の症状へ戻る。
///
/// **唯一の例外**は 1 件も収まらないときで、`MIN_MIDDLE_KEEP` 字版（`best` の初期値・実測を
/// 通っていない）を返す。これ以上短くできないためで、以降は egui 側の末尾省略に委ねる
/// ＝現行と同じ見え方へ退化するだけである。この carve-out は
/// `fit_middle_narrow_avail_falls_back_to_min_keep` が予算超過ごと固定している。
///
/// 測定回数は `1 + ceil(log2(n))`（128 字で 8 回）。egui の galley キャッシュにより
/// 定常フレームではハッシュ引きになる。
pub fn fit_middle_by_measure(
    dir: &str,
    avail_px: f32,
    mut measure: impl FnMut(&str) -> f32,
) -> String {
    if measure(dir) <= avail_px {
        return dir.to_string();
    }
    let n = dir.chars().count();
    if n <= MIN_MIDDLE_KEEP {
        return dir.to_string();
    }
    let mut best = truncate_middle_chars(dir, MIN_MIDDLE_KEEP);
    let (mut lo, mut hi) = (MIN_MIDDLE_KEEP, n - 1);
    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        let cand = truncate_middle_chars(dir, mid);
        if measure(&cand) <= avail_px {
            best = cand;
            lo = mid + 1;
        } else {
            // mid >= MIN_MIDDLE_KEEP(=4) ゆえ underflow しない
            hi = mid - 1;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #672: **文字サイズが font_size に連動する**こと自体を固定する（SPEC §11 の規範
    /// 「文字サイズに固定値を書かない」）。固定値へ戻す変更はここで落ちる。
    #[test]
    fn aux_text_sizes_scale_with_font_size() {
        // 既定 font_size=15 で従来の固定値 13px と実質同じ（この変更は見た目を変えない）。
        assert!(
            (status_size(15) - 13.05).abs() < 0.01,
            "既定で 13px 相当を保つ"
        );
        // font_size を上げれば追従する（固定値なら 15 と 24 で同値になり、ここが落ちる）。
        assert!(status_size(24) > status_size(15), "font_size に連動する");
        assert!((status_size(24) - 20.88).abs() < 0.01);
        // 補助要素どうしの序列: パス行 < status 行 < 主要素（font_size 等倍）。
        assert!(
            path_size(15) < status_size(15),
            "パス行より大きい（行の主メッセージゆえ）"
        );
        assert!(status_size(15) < 15.0, "主要素より小さい");
        // 極小 font_size でも読める下限を持つ（path_size の 9.0 と同型の防御）。
        assert_eq!(status_size(1), 11.0);
    }

    #[test]
    fn leading_fires_on_burst_start_then_trailing() {
        let mut d = Debouncer::new(Duration::from_millis(50), true);
        assert!(d.on_input(), "バースト先頭は leading 発火");
        assert!(!d.on_input(), "連打中は leading 抑止");
        assert!(
            !d.poll(Duration::from_millis(30)),
            "50ms 未満は trailing 抑止"
        );
        assert!(
            d.poll(Duration::from_millis(50)),
            "50ms 経過で trailing 発火"
        );
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
        assert!(
            !d.poll(Duration::from_millis(100)),
            "cancel 後は trailing 発火しない"
        );
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

    /// #738: 位置クランプの材料は**バー高**であって実高ではない。論理 px を物理へ換算する。
    ///
    /// 実高を使うと toast が消えて窓が縮んだとき位置が戻らず、hide が
    /// （`read_placement_relative` 経由で）そのずれを永続化する（#755 / #801 で実際に起きた）。
    #[test]
    fn bar_rect_height_phys_converts_logical_to_physical() {
        assert_eq!(bar_rect_height_phys(43.0, 1.0), 43);
        assert_eq!(bar_rect_height_phys(43.0, 1.5), 65); // 64.5 → 四捨五入で 65
        assert_eq!(bar_rect_height_phys(43.0, 2.0), 86);
        assert_eq!(bar_rect_height_phys(43.0, 1.25), 54); // 53.75 → 54
    }

    /// #738: 基準モニターは**バー矩形の中心**から決める。
    ///
    /// 窓全体の矩形（`MonitorFromWindow`）で決めると、上下に並んだモニターで status 行 +
    /// toast 行が出て下側モニターへ大きく伸びたとき、重なりの大きい下側が選ばれる——
    /// **行が出ただけでバーが隣モニターへ飛ぶ**。バー矩形は行の出没で変わらない。
    #[test]
    fn bar_rect_center_is_midpoint_of_given_rect() {
        // 幅 600・バー高 43 の窓が (100, 200) にある
        assert_eq!(bar_rect_center(100, 200, 600, 43), (400, 221));
        // 奇数幅は切り捨て（整数除算）
        assert_eq!(bar_rect_center(0, 0, 601, 43), (300, 21));
        // 負原点モニター（プライマリの左に置かれた副モニター）
        assert_eq!(bar_rect_center(-1920, -100, 600, 43), (-1620, -79));
    }

    /// #749: 許容 0.5 の境界。**ちょうど 0.5 は撃たない**（`>` であって `>=` ではない）。
    #[test]
    fn size_delta_exceeds_only_past_half_pixel() {
        assert!(!size_delta_exceeds((600.0, 300.0), (600.0, 300.0))); // 同値
        assert!(!size_delta_exceeds((600.0, 300.0), (600.0, 300.5))); // ちょうど境界
        assert!(size_delta_exceeds((600.0, 300.0), (600.0, 300.51))); // 境界の外
        // 負方向も同じ閾値で撃つ（`abs()` を落とすとここが落ちる）。
        assert!(size_delta_exceeds((600.0, 300.0), (600.0, 299.4)));
        assert!(!size_delta_exceeds((600.0, 300.0), (600.0, 299.5)));
    }

    /// #749: 幅と高さの**両方**を見る。片軸を落とすと、幅の live-reload か
    /// 行高の追従のどちらかが黙って効かなくなる。
    #[test]
    fn size_delta_exceeds_watches_both_axes() {
        assert!(size_delta_exceeds((600.0, 300.0), (700.0, 300.0))); // 幅だけ
        assert!(size_delta_exceeds((600.0, 300.0), (600.0, 400.0))); // 高さだけ
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
        assert_eq!(
            present_results(inputs(true, false, 3, 8)),
            Visible { desired_height: h }
        ); // ②t ④t: 唯一の可視
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
        assert_eq!(
            present_results(inputs(false, false, 3, 8)),
            ResultsPresentation::Hidden
        );
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

    // ---- 中間省略（#870） --------------------------------------------------

    /// フェイク測定: ASCII 10px / 非 ASCII 20px + 固定部（接頭辞・接尾辞）30px。
    ///
    /// **実 galley を持ち込まずにアルゴリズムそのものを固定する**のがこの注入の狙いである。
    /// `…`（U+2026）も非 ASCII ＝ 20px として数えるので、「2 字消して `…` を 1 つ足す」段で
    /// 幅が増えも減りもしない——単調性の境界をちょうど踏む値になっている。
    fn fake_measure(candidate: &str) -> f32 {
        30.0 + candidate
            .chars()
            .map(|c| if c.is_ascii() { 10.0 } else { 20.0 })
            .sum::<f32>()
    }

    const DEEP_BASE: &str = "C:\\tmp\\snotra836\\snotra836-entry\\very-long-directory-name-for-truncation\\second-level-also-long\\third-level-segment";

    /// 受け入れ条件 4: 収まるパスは 1 文字も変えない（現行の見え方を変えない）。
    #[test]
    fn fit_middle_leaves_short_dir_untouched() {
        assert_eq!(
            fit_middle_by_measure("C:\\Toolbox\\ghost-launcher", 600.0, fake_measure),
            "C:\\Toolbox\\ghost-launcher"
        );
        // 縮めようがない長さ（n <= MIN_MIDDLE_KEEP）は幅に関係なく原文
        assert_eq!(fit_middle_by_measure("C:\\", 0.0, fake_measure), "C:\\");
        assert_eq!(fit_middle_by_measure("", 0.0, fake_measure), "");
    }

    /// 受け入れ条件 2: 省略しても先頭（ドライブ）と末尾（leaf）の両方が残る。
    /// egui 既定の**末尾**省略では leaf が消える——#870 の症状そのもの。
    #[test]
    fn fit_middle_keeps_head_and_tail() {
        let dir = format!("{DEEP_BASE}\\LEAF-IS-HERE");
        let out = fit_middle_by_measure(&dir, 600.0, fake_measure);
        assert!(out.starts_with("C:\\"), "先頭が残る: {out}");
        assert!(out.ends_with("LEAF-IS-HERE"), "leaf が残る: {out}");
        assert!(out.contains('…'), "中間が省略されている: {out}");
    }

    /// 返した候補は**必ず実測で収まっている**（`best` に収まった候補だけを記録する構造の核心）。
    /// 平均文字幅で 1 回割る近似では、ここが保証されず egui の末尾省略へ落ちる。
    #[test]
    fn fit_middle_result_actually_fits() {
        let dir = format!("{DEEP_BASE}\\LEAF-IS-HERE");
        for avail in [120.0_f32, 200.0, 333.0, 600.0, 900.0] {
            let out = fit_middle_by_measure(&dir, avail, fake_measure);
            assert!(
                fake_measure(&out) <= avail,
                "avail={avail} で収まる: {out} ({}px)",
                fake_measure(&out)
            );
        }
    }

    /// #870 の症状そのもの: 同じ親の下の 2 つの leaf が**相異なる表示**になる。
    /// 実測では第 3 階層と LEAF 到達のキャプチャが SHA256 で完全一致していた。
    #[test]
    fn fit_middle_distinguishes_sibling_dirs() {
        let a = fit_middle_by_measure(&format!("{DEEP_BASE}\\LEAF-IS-HERE"), 600.0, fake_measure);
        let b = fit_middle_by_measure(&format!("{DEEP_BASE}\\LEAF-ELSEWHERE"), 600.0, fake_measure);
        assert_ne!(a, b, "兄弟ディレクトリが同一表示にならない");
        // 親（第 3 階層）自身とも区別が付く
        let parent = fit_middle_by_measure(DEEP_BASE, 600.0, fake_measure);
        assert_ne!(a, parent);
        assert_ne!(b, parent);
    }

    /// **二分探索の正当性の根拠**: `MIN_MIDDLE_KEEP` 以上で幅が単調非減少であること。
    /// `truncate_middle_chars` は `max_chars < MIN_MIDDLE_KEEP` で原文を返す＝そこだけ幅が
    /// 跳ねるため、探索範囲を `[MIN_MIDDLE_KEEP, n-1]` に閉じる根拠でもある（`n` 自体は
    /// 第 1 段の `measure(dir) <= avail_px` が既に落としている）。
    #[test]
    fn truncate_middle_chars_width_is_monotonic_above_min_keep() {
        for dir in [
            &format!("{DEEP_BASE}\\LEAF-IS-HERE") as &str,
            "C:\\ユーザー\\ドキュメント\\プロジェクト\\成果物",
            "C:\\a\\bb\\ccc\\dddd\\eeeee",
        ] {
            let n = dir.chars().count();
            let mut prev = f32::NEG_INFINITY;
            for keep in MIN_MIDDLE_KEEP..=n {
                let w = fake_measure(&truncate_middle_chars(dir, keep));
                assert!(w >= prev, "keep={keep} で幅が減った: {w} < {prev} ({dir})");
                prev = w;
            }
        }
    }

    /// `MIN_MIDDLE_KEEP` 未満は原文（`…` を挟む余地が無い）。範囲外アクセスも起こさない
    /// ——release は `panic="abort"` ゆえ境界は落とせない。
    #[test]
    fn truncate_middle_chars_returns_source_below_min_keep() {
        for keep in 0..MIN_MIDDLE_KEEP {
            assert_eq!(truncate_middle_chars("C:\\abc\\def", keep), "C:\\abc\\def");
            assert_eq!(truncate_middle_chars("", keep), "");
        }
        // 文字数ちょうど・それ以上でも原文
        assert_eq!(truncate_middle_chars("abcde", 5), "abcde");
        assert_eq!(truncate_middle_chars("abcde", 99), "abcde");
    }

    /// CJK 混在でも**フォントの実寸に乗る**（測定を注入する設計の要点）。平均文字幅を
    /// Latin 想定で置く実装は CJK を過小評価して under-truncate する（#632 の方針を継承）。
    ///
    /// **leaf 全体が残るのは予算の半分に収まるときだけである**（head/tail 等分）。予算が
    /// 足りなければ leaf も途中から削れるが、**残るのは leaf の末尾**であって先頭ではない
    /// ——egui 既定の末尾省略（leaf の先頭だけが残る）と逆であり、兄弟ディレクトリを
    /// 区別できるのはこの向きゆえである。
    #[test]
    fn fit_middle_handles_cjk() {
        let dir = "C:\\ユーザー\\ドキュメント\\プロジェクト\\進行中\\最終成果物フォルダ";
        // 予算が足りるとき: leaf 全体が残る
        let wide = fit_middle_by_measure(dir, 360.0, fake_measure);
        assert!(fake_measure(&wide) <= 360.0, "収まる: {wide}");
        assert!(wide.contains('…'), "省略されている: {wide}");
        assert!(
            wide.ends_with("最終成果物フォルダ"),
            "leaf 全体が残る: {wide}"
        );
        // 予算が足りないとき: leaf の**末尾**が残る（先頭ではない）
        let narrow = fit_middle_by_measure(dir, 300.0, fake_measure);
        assert!(fake_measure(&narrow) <= 300.0, "収まる: {narrow}");
        let tail = narrow.rsplit('…').next().unwrap();
        assert!(!tail.is_empty(), "末尾側が空でない: {narrow}");
        assert!(dir.ends_with(tail), "残った末尾は dir の接尾辞: {narrow}");
    }

    /// 極端に狭い幅でも panic せず最短候補を返す（以降は egui の末尾省略へ委ねる）。
    ///
    /// **`fit_middle_by_measure` の doc が挙げる唯一の例外を、ここが固定している**——この
    /// 経路だけは返り値が予算を超える。それを測らないと「必ず収まる」という主張の carve-out が
    /// どのテストにも現れず、doc だけが例外を知っている状態になる。
    #[test]
    fn fit_middle_narrow_avail_falls_back_to_min_keep() {
        let dir = format!("{DEEP_BASE}\\LEAF-IS-HERE");
        let out = fit_middle_by_measure(&dir, 0.0, fake_measure);
        assert_eq!(out, truncate_middle_chars(&dir, MIN_MIDDLE_KEEP));
        assert_eq!(out.chars().count(), MIN_MIDDLE_KEEP);
        assert!(
            fake_measure(&out) > 0.0,
            "fallback だけは予算を超えうる（doc の唯一の例外）: {}px > 0.0",
            fake_measure(&out)
        );
    }
}
