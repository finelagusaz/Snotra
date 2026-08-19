//! 結果リスト窓（"results"）の egui view（#646 PR2）。main（SearchWindowView）が発行する
//! RowsSnapshot を描くだけの従属 view——検索状態の所有者は main のまま（一方向データフロー・
//! spec 決定 5）。クリックは ResultsShared.clicked へ積んで main を wake する（遅延 dispatch）。
//! 窓の可視性・サイズ・位置の driver は main 側（hidden 窓は update() が走らないため）。
//! hide は外部（`hide_egui_main` / main の `drive_results_window`）が所有する。runtime に
//! view 側から hide する API は無い（`RuntimeFrame::hide_window` は #671 サイクル PR A で削除。
//! 対称の `close_window` も #660 の `snotra-egui-mvp` 撤去に伴い削除）。

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender, channel};

use snotra_core::ui_types::SearchResult;
use tauri::Manager;

use crate::commands::IconOutcome;
use crate::egui_shell::icon_textures::ICON_MAX_ATTEMPTS;
use crate::egui_shell::{IconMsg, RowTheme, retain_visible};

/// main が毎フレーム発行する描画用スナップショット（spec 決定 5）。
#[derive(Clone, Default, PartialEq)]
pub(crate) struct RowsSnapshot {
    /// 描画する行。**空 = results 窓は非表示**（別途 `show` フラグは持たない・/simplify）——
    /// main は「表示すべきでないフレーム」に空 Vec を発行するため、`rows.is_empty()` が
    /// 可視性そのものを表す。2 つの書き込みを手で同期させる規約を型で消す。
    pub rows: Vec<snotra_core::ui_types::SearchResult>, // ui_types が正（engine ではない）・PartialEq/Eq derive 済み
    pub selected: usize,
    /// 結果集合が総入れ替えされるたびに main が加算するカウンタ（#632 reviewer Important 3
    /// の後継・Fix 3）。`selected` の値だけでは「打鍵で結果が丸ごと変わったが selected は
    /// 偶然 0 のまま」を検出できないため、この世代番号で ResultsView の scroll gate を
    /// 独立にリセットする。Default=0（PartialEq 比較にも自然に入る）。
    pub generation: u64,
    /// main の `search_debounce` が**予約を持っていない**かどうか（= 非 armed）。旧実装（Task 5 移設前）の `!self.search_debounce.is_armed()`（[`crate::egui_shell::layout::Debouncer::is_armed`] の**否定**）ゲート（連打中は icon worker を積まない・perf 最適化）の後継——`ResultsView` は `search_debounce` を持てないため、main が代わりに live 値を snapshot に載せて運ぶ（#532 SU4 の系譜・controller fix 依頼）。Default=false（PartialEq 比較にも自然に入る。armed → 非 armed の遷移だけで snapshot 差分が生まれ wake が 1 回走るのは「打鍵停止後にアイコン要求を起こす」という意図どおりの信号）。
    ///
    /// **「打鍵が止まった」より広い**（#1074）——`armed` は [`crate::egui_shell::layout::Debouncer::cancel`] でも下りるため、Enter の flush 経路を通った直後もここは真になる。名前が言うのは**debounce が予約を持っていないこと**であって、ユーザーが打鍵を止めたことではない。
    ///
    /// **これは perf ヒューリスティックであって正しさの述語ではない。** [`crate::egui_shell::search_state::SearchState::is_unsettled`]（最終クエリの結果が未反映か）とは別概念であり、否定の関係にも無い——食い違うのは `armed == false` かつ、あちらの残る disjunct のいずれかが立つときである（#1079 で `pending != 0` に「folder の往復を跨いで運ばれた未反映」が加わったため、**式ではなく `is_unsettled` の doc を正本として指す**）。**同じ修正を当ててはならない**（worker 走査中のアイコン取得が遅れて悪化する）。
    pub input_idle: bool,
}

impl RowsSnapshot {
    /// 発行前の差分判定（/simplify・効率）。`Vec` を作ってから `PartialEq` で比べると、
    /// **無変化フレームでも** 行数ぶんの `String` 確保が走る（入力欄のキャレット点滅だけでも
    /// main は repaint する）。行はスライスのまま突き合わせ、差があるときだけ `to_vec()` する。
    ///
    /// **フィールドを増やしたらここも増やす**——先頭の分解束縛が漏れを compile-fail にする。
    pub(crate) fn matches(
        &self,
        rows: &[SearchResult],
        selected: usize,
        generation: u64,
        input_idle: bool,
    ) -> bool {
        let Self {
            rows: cur_rows,
            selected: cur_selected,
            generation: cur_generation,
            input_idle: cur_input_idle,
        } = self;
        cur_rows.as_slice() == rows
            && *cur_selected == selected
            && *cur_generation == generation
            && *cur_input_idle == input_idle
    }
}

/// main と results が共有する一方向フローの入れ物（managed state）。
#[derive(Default)]
pub(crate) struct ResultsShared {
    pub snapshot: std::sync::Mutex<RowsSnapshot>,
    /// クリックされた行（last-wins）。**世代を同梱する**（#699）——裸の index は、積んだ
    /// フレームと消費するフレームの間に行集合が総入れ替えされると**別の行を指す**。
    /// `activate` の `.get(index)` は境界チェックであって行の同一性チェックではないので、
    /// index が生きていれば別の行が起動してしまう。
    ///
    /// **取り出しは `take_clicked_for` だけを使う**（フィールドが private なので、世代照合を
    /// 迂回して index を得る経路が型から消えている）。`docs/development-principles.md`
    /// 「構造的設計原則と強制の階梯」の 2（不変条件は単一の通過点に閉じ込める）。
    ///
    /// **reset-on-show 用の専用クリアは置かない**——`SearchState::reset` が世代を進めるため、
    /// hide を跨いだ stale クリックは同じ照合で失効する。専用クリアを足すと「クリアする経路」と
    /// 「世代で弾く経路」の 2 本立てになり、どちらが正本か読めなくなる。
    ///
    /// `snapshot` は世代を運び `clicked` は運ばない、という非対称は**見落としであった**
    /// （#699 備考が「意図的か見落としかコードから読めない」と問うていた点の結論）。
    clicked: std::sync::Mutex<Option<(u64, usize)>>,
}

/// `take_clicked_for` の結果（#699）。`Option<usize>` にしないのは、**「クリックが無かった」と
/// 「世代不一致で捨てた」を呼び出し側が区別できるようにする**ため——破棄は目に見えない経路で、
/// 手で再現もできないので、trace を出せる形にしておく。
pub(crate) enum ClickTake {
    /// 世代が一致した。起動してよい。
    Current(usize),
    /// 世代が食い違った。**破棄した**（スロットは空になっている）。
    Stale { stamped: u64 },
    /// クリックは積まれていなかった。
    None,
}

impl ResultsShared {
    /// results 側が積んだクリックを、**世代が一致するときだけ**取り出す（#699）。
    ///
    /// 不一致でも `take` する——残すと次フレーム以降に同じ stale クリックが再評価される。
    pub(crate) fn take_clicked_for(&self, generation: u64) -> ClickTake {
        match self.clicked.lock().unwrap().take() {
            Some((g, i)) if g == generation => ClickTake::Current(i),
            Some((g, _)) => ClickTake::Stale { stamped: g },
            None => ClickTake::None,
        }
    }

    /// results 側からクリックを積む。`generation` は**そのフレームで実際に描いた**
    /// `RowsSnapshot.generation` を渡すこと（別の値を渡すと照合が意味を失う）。
    pub(crate) fn push_clicked(&self, generation: u64, index: usize) {
        *self.clicked.lock().unwrap() = Some((generation, index));
    }
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
    /// path→TextureHandle（セッション内保持。可視集合に頭打ち・#532 SU4。Task 5 で view.rs から
    /// 本 view へ移設——TextureHandle は窓の Context 従属のため、行描画と同じ窓に置く）。
    icon_textures: HashMap<String, egui::TextureHandle>,
    /// path→抽出の失敗回数（#692）。**「失敗したか」ではなく「何回失敗したか」を持つ**
    /// ——一過性の失敗（冷えたシェルのアイコンキャッシュ等）を 1 度で恒久的な欠落として
    /// latch すると、その行は可視である限りグレーのプレースホルダのまま戻らない。
    /// 恒久的な失敗（パス不在・decode 不能）は `ICON_MAX_ATTEMPTS` を直接入れて即打ち切る。
    icon_attempts: HashMap<String, u8>,
    /// worker へ渡し済みだが IconMsg 未 drain の path（in-flight）。request_icons_for_results の
    /// wanted 収集から除外し、同一 settle に対する repaint 毎の重複 spawn を防ぐ（thread pileup
    /// 対策）。drain 時（Loaded/Missing）に remove する。show=false 時の明示 clear は行わない
    /// ——retain_visible が空 rows で textures/missing を自然に刈るのに合わせ、pending も次の
    /// request_icons_for_results 呼び出し（wanted 収集）で自然に収束する（Task 5 申し送り）。
    icon_pending: HashSet<String>,
    icon_tx: Sender<IconMsg>,
    icon_rx: Receiver<IconMsg>,
}

impl ResultsView {
    pub(crate) fn new(app_handle: tauri::AppHandle) -> Self {
        let (icon_tx, icon_rx) = channel();
        Self {
            app_handle,
            applied_font_family: String::new(),
            last_scrolled_selected: None,
            last_generation: 0,
            icon_textures: HashMap::new(),
            icon_attempts: HashMap::new(),
            icon_pending: HashSet::new(),
            icon_tx,
            icon_rx,
        }
    }

    /// 渡された行の未取得アイコンを worker に積む（[`RowsSnapshot::input_idle`] 相当）。**`rows` は結果全件ではなく `layout::icon_prefetch_range` で絞った viewport 範囲である**——呼び出し側がそのフレームで実際に描いた `ScrollArea` の状態から導く（絞る理由と実測は同関数の doc）。ゆえに**描画より後に呼ぶ**（範囲がスクロール状態に依存するため）。in-flight（icon_pending）の path は除外し、抽出中の repaint（マウス移動・カーソル点滅等）による同一 path 集合への重複 spawn を防ぐ（thread pileup 対策）。spawn した path は icon_pending へ積み、drain（Loaded/Missing）で remove する。wanted が空なら insert も spawn もしない。Task 5 で view.rs から移設し、入力を `self.state.results()` から `snapshot.rows` へ変更した（本 view は snapshot を描くだけで検索状態を持たないため）。
    fn request_icons_for_results(
        &mut self,
        rows: &[SearchResult],
        show_icons: bool,
        ctx: &egui::Context,
    ) {
        // #673 spec 決定 4: **ここで config を読み直さない。** 呼び出し側（update()）が
        // フレーム冒頭で読んだ値を渡す——同一フレーム内で 2 度読むと、間に config_watcher の
        // 適用が挟まったとき「アイコンを積むかどうか」と「アイコン枠を描くかどうか」が
        // 食い違う（枠は描いたのに抽出は走らない、の 1 フレーム）。
        if !show_icons {
            return;
        }
        // **キーの導出は `SearchResult::icon_key` ただ 1 つである**（#1133 / #1134）——要求
        // （`wanted_icon_keys`）・引き（`icon_for_row`）・剪定（`visible_icon_keys`）が同じ導出を
        // 通ることだけが、「抽出したものを引ける」「列挙失敗行が絵を引かない」の根拠である。
        let wanted = crate::egui_shell::wanted_icon_keys(
            rows,
            &self.icon_textures,
            &self.icon_attempts,
            &self.icon_pending,
        );
        if wanted.is_empty() {
            return;
        }
        for p in &wanted {
            self.icon_pending.insert(p.clone());
        }
        self.spawn_icon_load(wanted, ctx.clone());
    }

    /// 現結果集合の未取得アイコンを別スレッドで抽出し IconMsg を channel へ送る（SU4）。folder の
    /// per-nav thread パターン踏襲。token は載せない（staleness はアイコンキーで無害）。
    /// show_icons=false 時は呼ばない（呼び出し側でガード）。Task 5 で view.rs から移設
    /// （worker の wake は clone した results の ctx の `request_repaint()`・形は不変）。
    fn spawn_icon_load(&self, paths: Vec<String>, egui_ctx: egui::Context) {
        if paths.is_empty() {
            return;
        }
        let app = self.app_handle.clone();
        let tx = self.icon_tx.clone();
        std::thread::spawn(move || {
            let (Some(state), Some(icons)) = (
                app.try_state::<crate::AppState>(),
                app.try_state::<crate::icon::IconCacheState>(),
            ) else {
                return;
            };
            let loaded = crate::commands::load_icon_pngs(&state, &icons, paths);
            for (path, outcome) in loaded {
                // **「取れなかった」を 1 つに潰さない**（#692）。`retryable=false` は
                // 「このパスにアイコンは無い」に近い恒久的失敗で、driver は即座に打ち切る。
                let msg = match outcome {
                    IconOutcome::Png(bytes) => {
                        match crate::egui_shell::png_to_color_image(&bytes) {
                            Some(img) => IconMsg::Loaded(path, img),
                            // decode 失敗は保存済みバイト列の問題であり、再試行しても同じ。
                            None => {
                                crate::trace_main(
                                    "egui_icon:decode_failed",
                                    serde_json::json!({ "path": path, "bytes": bytes.len() }),
                                );
                                IconMsg::Missing(path, false)
                            }
                        }
                    }
                    IconOutcome::Failed(reason) => IconMsg::Missing(path, reason.is_transient()),
                    // キャッシュ自体が使えなかった（show_icons=false / 無効化と競合）。
                    // パス固有の失敗ではないので再試行してよい。
                    IconOutcome::Unavailable => IconMsg::Missing(path, true),
                };
                let _ = tx.send(msg);
            }
            egui_ctx.request_repaint(); // イベント駆動 runtime を起こす（folder/main と同理由）
        });
    }
}

/// 行スクロールの指示（#714）。世代交代（結果集合の総入れ替え）は前のリストの位置を
/// 持ち越さず瞬時に選択行へ、選択移動（世代不変・↑↓）は従来のアニメーションで寄せる。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum RowScroll {
    None,
    Animated,
    Instant,
}

/// scroll gate（#632・選択変化時のみ）と世代検知（#714）から行スクロールの指示を導く純粋核。
/// 「先頭へ」ではなく「選択行へ」が正しい目標である——`on_escape` の folder/tool 復帰と
/// index 再構築後の再検索は selected≠0 のまま世代を進める（offset=0 直置き案を却下した理由。
/// plan-review 2026-07-26）。
fn scroll_directive(selected: bool, do_scroll: bool, generation_changed: bool) -> RowScroll {
    if !(selected && do_scroll) {
        RowScroll::None
    } else if generation_changed {
        RowScroll::Instant
    } else {
        RowScroll::Animated
    }
}

fn row_hover_target(hovered: bool, is_error: bool) -> bool {
    hovered && !is_error
}

/// 1 行を描画。scroll の指示に従い選択行へ寄せる（gate は呼び出し側 `scroll_directive`・#632/#714）。
/// 返り値:
/// single_clicked。ダブルクリックは扱わない（ユーザー決定: §4.8 の double-click=選択は
/// as-built でも到達不能ゆえ落とす。単クリック=起動のみ）。self を借りない関連関数
/// （借用衝突回避）。色/サイズは呼び出し側が都度導出する `RowTheme` から取る。
/// 2 行表示(#646 決定 9): 上段名前・下段パス。行高は Metrics::row_height(呼び出し側注入)。
/// `show_icons=false` はアイコン slot 自体を畳む（skip でなくレイアウト変更・#532 SU4 Task 6）
/// ——テキストが左端 8px 寄せになり、slot 分の空白が残らない。
/// `reset_hover_animation` は結果集合の世代交代フレームだけ true。固定した行 ID の進捗を
/// 現在のポインタ状態へスナップし、古いホバーを別の行へ引き継がない（#724）。
#[allow(clippy::too_many_arguments)] // raster.rs::fill_mesh と同型（描画関数は座標/テーマ引数が集中する）
pub(crate) fn draw_result_row(
    ui: &mut egui::Ui,
    result: &SearchResult,
    selected: bool,
    scroll: RowScroll,
    reset_hover_animation: bool,
    icon: Option<&egui::TextureHandle>,
    show_icons: bool,
    theme: &RowTheme,
    row_h: f32,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), row_h),
        egui::Sense::click(),
    );
    // **スクロール要求は可視性より前に出す。** 画面外の選択行を viewport へ引き戻すことが
    // この機構の目的そのものであり、下の早期 return より後ろへ落とすと、選択行が画面外の
    // ときに `scroll_to_me` が発火せず世代交代で選択行へ戻れなくなる（`show_rows` による
    // 仮想化を却下したのと同じ理由が、同じ形で戻ってくる・`layout::icon_prefetch_range` の doc）。
    if selected {
        match scroll {
            RowScroll::None => {}
            RowScroll::Animated => {
                // 選択変化時のみ（#632）。None=可視化に必要な最小限だけ（WebView2 の
                // block:"nearest" parity・Center だと中央維持で早期スクロール・#532 SU6.5）
                response.scroll_to_me(None);
            }
            RowScroll::Instant => {
                // 世代交代フレーム（#714）: 別のリストへの切り替えに前のリストの
                // スクロール位置は持ち越さない。アニメーション経路（0.1〜0.3s の連続描画）を
                // 通さず、次の repaint で offset を中間フレームなしに target へ置く。
                // 受容する残余（plan-review 2026-07-26）: (a) ↑↓ 直後の in-flight
                // アニメーション中に世代が来ると target 置換までの残り時間が食い込みうる
                // （目標位置は正しい）。(b) 内容同一の reindex 再検索でも世代は進むため
                // 「アニメーション付きで選択行へ戻る」が「瞬時に戻る」へ変わる（頻度稀・目標は同じ）。
                response.scroll_to_me_animation(None, egui::style::ScrollAnimation::none());
            }
        }
    }
    // **viewport の外に出た行は、ここから下を一切やらない。** 行の確保（＝窓高・
    // スクロールバー・ピッチ）とスクロール要求は上で済んでおり、以降は「見えるものを描く」
    // 仕事だけである。`snapshot.rows` は可視行数ではなく `effective_result_limit`
    // （既定 200・設定次第で 1000）ゆえ、間引かないと画面に 8 行しか出ないフレームでも
    // 全行ぶんのアイコン描画とテキストレイアウトを払う。
    //
    // 実測（release・`kittest_row_draw_cost_probe`・可視 8 行の実寸窓）: 画面外 1 行あたり
    // 約 0.8us で、1 フレームは rows=8 で 13us・200 で 156us・1000 で 807us だった。
    // 同じ idiom の先例は `snotra-settings/src/tabs/visual.rs` の `is_rect_visible`。
    if !ui.is_rect_visible(rect) {
        return response.clicked();
    }
    // hover のアニメーションは**間引きの下**で足りる。画面外の行は `hovered()` が偽ゆえ
    // 進捗は 0 へ向かうだけであり、スクロールで入ってきた行が 0 から立ち上がるのは
    // 望ましい挙動である（画面外で進んだ進捗を持ち込まない）。
    let hover_id = response.id.with("hover");
    let hover_target = row_hover_target(response.hovered(), result.is_error);
    let hover_progress = if reset_hover_animation {
        ui.ctx().animate_bool_with_time(hover_id, hover_target, 0.0)
    } else {
        ui.ctx().animate_bool_responsive(hover_id, hover_target)
    };
    if selected {
        ui.painter().rect_filled(rect, 4.0, theme.selection);
    } else if hover_progress > 0.0 {
        ui.painter()
            .rect_filled(rect, 4.0, theme.hover.gamma_multiply(hover_progress));
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
    let path_full =
        ui.painter()
            .layout_no_wrap(result.path.clone(), path_font.clone(), theme.path_color);
    let path_str = if path_full.size().x <= avail {
        result.path.clone()
    } else {
        // per-char 幅は実 galley から実測(CJK 過小評価対策・#632 の方針を継承)
        let per_char_px = path_full.size().x / (result.path.chars().count().max(1) as f32);
        truncate_middle(&result.path, avail, per_char_px)
    };
    let path_galley = ui
        .painter()
        .layout_no_wrap(path_str, path_font, theme.path_color);
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
    ui.painter()
        .galley(egui::pos2(text_x, top), name_galley, theme.name_color);
    ui.painter().galley(
        egui::pos2(text_x, top + name_h + 4.0),
        path_galley,
        theme.path_color,
    );
    response.clicked()
}

/// 行のリストを 1 フレーム分描く。**`tauri::AppHandle` にも `ResultsShared` にも触らない**ので、
/// `egui_kittest` の `Harness` から実コードのまま駆動できる（`mod tests` の kittest 検査）。
/// 先例と理由は `view.rs` の `search_input_ui`——純粋核へ出せない描画の不変条件は、実コードを
/// ヘッドレスで走らせる以外に測る手段が無い。
///
/// **返す `ScrollAreaOutput.content_size.y` が検出器の測定点である。** これは
/// `content_ui.min_size()`（egui 0.35 `scroll_area.rs`）＝**実際に積んだ行の総高**であり、
/// viewport にクランプされない。`layout::results_window_height(N, row_height)` と等しいことが
/// 「窓に N 行がちょうど収まる」の機械的な表現になる——**この 2 つを結ぶ検査は他に無い**
/// （`layout.rs` からは行間の余白が見えず、式が式であることしか測れない）。
///
/// `clicked` は `ScrollArea` の inner として返す。呼び出し側は `.inner` だけを読み、
/// `content_size` はテストだけが読む。
#[allow(clippy::too_many_arguments)] // draw_result_row と同型（描画関数は座標/テーマ引数が集中する）
pub(crate) fn results_list_ui(
    ui: &mut egui::Ui,
    rows: &[SearchResult],
    selected: usize,
    generation_changed: bool,
    do_scroll: bool,
    icons: &HashMap<String, egui::TextureHandle>,
    show_icons: bool,
    theme: &RowTheme,
    row_height: f32,
) -> egui::scroll_area::ScrollAreaOutput<Option<usize>> {
    // **行ピッチを `row_height` ちょうどにする。** 窓高（`layout::results_window_height`）は
    // `max_results × row_height` で**行間を勘定していない**ため、egui 既定の
    // `item_spacing.y`（3.0）が入ると `(N-1) × 3.0` だけ中身が窓を超え、最終行が切れる
    // （実測: `visible_rows` 8 で 0.38 行・20 で 1.38 行はみ出していた）。
    //
    // **高さの式へ `(N-1) × spacing` を足す形は採らない**——`layout.rs` は egui 非依存の
    // 純粋核であり、egui の style 既定値をあちらへ書き写すと、上流が変えたときに黙って
    // ずれる（あちらからは spacing を読めないので検知もできない）。ここで 0 にすれば
    // 「行ピッチ = `row_height`」が構造的に真になり、式は件数だけの関数のままでいられる。
    //
    // **`ScrollArea` の構築より前に置くこと**——visuals の適用点（`view.rs`）と同型の
    // 順序制約であり、後ろへ動かすと既定の spacing で積まれる。**この位置は
    // `kittest_row_pitch_equals_row_height` が縛る**（後ろへ動かすと落ちる）。
    ui.spacing_mut().item_spacing.y = 0.0;
    egui::ScrollArea::vertical().show(ui, |ui| {
        let mut clicked: Option<usize> = None;
        for (i, result) in rows.iter().enumerate() {
            let sel = i == selected;
            if draw_result_row(
                ui,
                result,
                sel,
                scroll_directive(sel, do_scroll, generation_changed),
                generation_changed,
                crate::egui_shell::icon_for_row(icons, result),
                show_icons,
                theme,
                row_height,
            ) {
                clicked = Some(i);
            }
        }
        clicked
    })
}

/// アイコン欠落時の fallback（drawn placeholder）。§3.4 は 📁📄 を規定するが softbuffer +
/// 単一 TTF で色 emoji が描けない懸念があるため単色プレースホルダに倒す（視覚スモークは
/// Task 7 に集約・コントローラ決定）。Task 7 の視覚スモークで jp_font が 📁📄 を描けると
/// 確認できたら emoji へ upgrade を検討する。
fn draw_icon_fallback(ui: &egui::Ui, rect: egui::Rect, result: &SearchResult, theme: &RowTheme) {
    let center = egui::pos2(rect.left() + 14.0, rect.center().y);
    let r = egui::Rect::from_center_size(center, egui::vec2(14.0, 14.0));
    let col = if result.is_folder {
        theme.name_color
    } else {
        theme.path_color
    };
    ui.painter().rect_filled(r, 2.0, col.linear_multiply(0.5));
}

/// path を avail_px におよそ収める中間省略（`C:\a\...\app.exe`）。`per_char_px` は呼び出し側が
/// 実 galley（`Painter::layout_no_wrap`）から実測した平均文字幅を渡す（固定係数 size*0.55 は
/// Latin 想定で CJK グリフ（~1.0-1.8×）を過小評価し under-truncate する・reviewer Important 2）。
/// release は panic=abort ゆえ、`max_chars < MIN_MIDDLE_KEEP` ガードと空文字境界で範囲外
/// アクセスを避ける（どちらも `truncate_middle_chars` 側が持つ）。
///
/// **px → 文字数の換算だけがここの責務である**（#870）。head/tail 分割の本体は
/// `layout::truncate_middle_chars` にあり、フォルダ現在地の hint（`view.rs`）と共有する
/// ——実装を 2 本並べると片方だけ直る。
pub(crate) fn truncate_middle(s: &str, avail_px: f32, per_char_px: f32) -> String {
    let per = per_char_px.max(1.0);
    let max_chars = (avail_px / per).floor() as usize;
    crate::egui_shell::layout::truncate_middle_chars(s, max_chars)
}

impl snotra_egui_runtime::EguiView for ResultsView {
    fn setup(&mut self, context: &egui::Context) {
        // 日本語フォント: main と同じ config font_family を適用（ctx は窓ごとに独立）。
        let font_family = crate::egui_shell::font_stack::font_family_from_config(&self.app_handle);
        crate::egui_shell::font_stack::configure_japanese_font(context, &font_family);
        self.applied_font_family = font_family;
        // 外部 wake 用の ctx 登録はここに無い（#671 PR D・main の setup と同じ理由）。
    }

    fn update(&mut self, ui: &mut egui::Ui, frame: &mut snotra_egui_runtime::RuntimeFrame) {
        // テーマ値は 1 フレーム 1 読み（#673 spec 決定 4）。**早期 return より前**で取る——
        // 背景（clear color）は rows が空でも塗られるからである。旧実装はここが空 rows の
        // early return より後にあり「描かないフレームで lock を取らない」ことを理由にしていたが、
        // clear color を view が決めるようになって「空 rows でも描く」へ変わった。
        let visual = crate::egui_shell::read_visual(&self.app_handle, &self.applied_font_family);
        frame.set_clear_color(visual.background);

        // app_handle を clone してから State を取る: `shared` の借用を `self.app_handle`
        // （＝self 全体）ではなくこのローカル変数に結び付け、後段の `self.request_icons_for_results`
        // （&mut self）呼び出しと `shared` の同時生存を両立させる（Task 5 で挿入した icon
        // パイプライン呼び出しが E0502 を起こしたための整理）。
        let app_handle = self.app_handle.clone();
        let Some(shared) = app_handle.try_state::<ResultsShared>() else {
            return;
        };
        let snapshot = shared.snapshot.lock().unwrap().clone();
        if snapshot.rows.is_empty() {
            // #632 の不変条件「再表示後に確実に一度 scroll し直す」の実ゲート。旧実装は main
            // 側の resetForShow（reset_pending 消費）でこの gate を直接クリアしていたが、
            // 移設後は本 view が gate の唯一の所有者ゆえここでリセットする——hide/非表示の
            // たびに戻し、次に見えるフレームで選択行への scroll_to_me を再度発火させる。
            self.last_scrolled_selected = None;
            return; // 窓は main が hide 済みのはず(backstop で何も描かない)
        }
        // font_family hot-reload(view.rs の applied_font_family 比較と同型を複製・
        // ctx は窓ごとに独立なため main 側の適用はこの窓に効かない)。
        if let Some(name) = &visual.font_family_changed {
            crate::egui_shell::font_stack::configure_japanese_font(ui.ctx(), name);
            self.applied_font_family = name.clone();
        }
        let theme = &visual.row;
        let metrics = &visual.metrics;
        let show_icons = visual.show_icons;
        // アイコン drain（token 無し・アイコンキーで適用）。到着したら load_texture して map へ。
        // load_texture は egui context 必須ゆえ、ここ（results の update()・自窓 ctx）でのみ呼ぶ
        // ——worker（spawn_icon_load）は ColorImage を送るだけで load_texture は呼ばない。Task 5 で
        // main（view.rs）から本 view へ移設: TextureHandle は窓の Context 従属のため、行描画と
        // 同じ窓に置く。
        let icon_ctx = ui.ctx().clone();
        let mut icon_arrived = false;
        while let Ok(msg) = self.icon_rx.try_recv() {
            match msg {
                IconMsg::Loaded(path, img) => {
                    self.icon_pending.remove(&path); // in-flight 解除（thread pileup 対策）
                    let handle = icon_ctx.load_texture(&path, img, egui::TextureOptions::LINEAR);
                    self.icon_textures.insert(path, handle);
                    icon_arrived = true;
                }
                IconMsg::Missing(path, retryable) => {
                    self.icon_pending.remove(&path); // in-flight 解除（thread pileup 対策）
                    // 一過性なら 1 回ぶん加算して次の要求で再試行、恒久なら上限を直接入れて
                    // 打ち切る（#692）。`retain_visible` で可視集合外は自然に忘れる。
                    let entry = self.icon_attempts.entry(path).or_insert(0);
                    *entry = if retryable {
                        entry.saturating_add(1)
                    } else {
                        ICON_MAX_ATTEMPTS
                    };
                }
            }
        }
        if icon_arrived {
            icon_ctx.request_repaint();
        }
        // #632 reviewer Important 3 の後継（Fix 3）: 結果集合が総入れ替えされた
        // （`SearchState::rows_generation` が進んだ）フレームは、selected の値が変わらなくても scroll gate を
        // 強制リセットする——selected のみの比較では「打鍵で結果が丸ごと変わったが selected は
        // 偶然 0 のまま」を検出できず、選択行への scroll_to_me が発火しないままになるため。
        let generation_changed = snapshot.generation != self.last_generation;
        if generation_changed {
            self.last_scrolled_selected = None;
            self.last_generation = snapshot.generation;
        }
        // 選択変化時のみ scroll_to_me(#632 のゲートを view 内フィールドで維持)。
        // 世代交代フレームはアニメーションなし（#714・指示の導出は scroll_directive）。
        let do_scroll = self.last_scrolled_selected != Some(snapshot.selected);
        // 行の描画は `results_list_ui`（`AppHandle` 非依存の自由関数）へ出してある——
        // 行ピッチと窓高の対を kittest が実コードのまま測れるようにするため（同関数の doc）。
        let row_height = metrics.row_height as f32;
        let list_out = results_list_ui(
            ui,
            &snapshot.rows,
            snapshot.selected,
            generation_changed,
            do_scroll,
            &self.icon_textures,
            show_icons,
            theme,
            row_height,
        );
        let clicked = list_out.inner;
        if do_scroll {
            self.last_scrolled_selected = Some(snapshot.selected);
        }
        // アイコン: 可視集合（このフレームで実際に描画した rows）に頭打ちして drop（メモリ境界・
        // SU4 決定 A）。Task 5 で main（view.rs）から本 view へ移設。この一連の処理は
        // update() 冒頭の早期 return（rows 空 = 非表示）を通過したフレーム、すなわち results
        // 可視中にしか走らない——旧実装（main 側）は show_results の真偽に関係なく毎フレーム
        // 走っていた（隠れていても先読み）。reindexing 明けの再表示でアイコンが一瞬
        // placeholder → 追いつく体感になりうるが、クラッシュ・不整合ではなく hidden 中の
        // update 停止（SU5 要石）と整合する自然な帰結として受容する（plan-review rev-egui
        // 指摘・受容）。
        //
        // **世代交代フレームだけで足りる**（毎フレームやらない）。世代内で 3 つの map/set に
        // 入る path は先読み範囲 ⊆ `snapshot.rows` ゆえ必ず `visible` に含まれ、retain は
        // 構造的に no-op である。刈る必要が生じるのは rows が入れ替わった瞬間だけで、それは
        // `rows_generation` が必ず進む——`search_state.rs` の `rows_generation` の doc が
        // 「`self.results` へ代入・`clear` するメソッドは、必ずここを進める」と定めており、
        // 代入・`clear` の 6 箇所すべてで成立している（逆向きに数え上げて確認済み）。
        // 毎フレームやっていた頃は、可視集合の構築だけで結果件数ぶんの `String` 確保を
        // 払っていた（`result_limit` は既定 200・設定次第で 1000）。
        if generation_changed {
            let visible = crate::egui_shell::visible_icon_keys(&snapshot.rows);
            retain_visible(&mut self.icon_textures, &visible);
            self.icon_attempts.retain(|p, _| visible.contains(p));
            // **pending も可視集合で刈る**（#692）。worker が 1 通も送らずに終わる経路
            // （managed state 不在で早期 return）があり、刈らないと path が永久に in-flight
            // 扱いのまま `needs_extraction` を素通りできず、その行のアイコンは戻らない。
            self.icon_pending.retain(|p| visible.contains(p));
        }
        // snapshot.input_idle は旧 view.rs の `!self.search_debounce.is_armed()` ゲート（連打中は
        // icon worker を積まない・perf 最適化）の後継（#532 SU4 の系譜）。main の search_debounce は
        // ResultsView から参照できないため、live 値を snapshot 経由で運ぶ（Task 5 concern 2 の fix）。
        if snapshot.input_idle {
            // **抽出だけを viewport 範囲へ絞る**（描画・テクスチャ保持は全件のまま）。
            // `snapshot.rows` は可視行数ではなく `effective_result_limit`（既定 200）ゆえ、
            // 絞る前は画面に出る 8 個のために 200 回シェルへ問い合わせていた
            // （実測 105ms／settle。内訳と却下した代替は `layout::icon_prefetch_range` の doc）。
            //
            // **範囲は「この描画の結果として次に見えることになる行」である**——`state.offset` は
            // `ScrollArea::end()` が `scroll_to_me` の行き先を適用した**後**の値で（egui 0.35
            // `scroll_area.rs` の `show_viewport_dyn` は content closure の後に `end()` を呼び、
            // その戻り値を `ScrollAreaOutput.state` に載せる）、`inner_rect` は `begin()` が
            // 決めた矩形である。両者は厳密には別の時点の値だが、**この混成は利点である**：
            // 世代交代フレーム（`RowScroll::Instant`）で「飛び先の行」を同じフレームのうちに
            // 先読みでき、追加の repaint を待たない。
            //
            // **`ScrollArea::show_viewport` へ移してはならない**——あれが渡す `Rect` は
            // `begin()` 由来ゆえ `scroll_to_me` 適用前で、飛び先ではなく元の位置を先読みする。
            let range = crate::egui_shell::layout::icon_prefetch_range(
                list_out.state.offset.y,
                list_out.inner_rect.height(),
                row_height,
                snapshot.rows.len(),
            );
            self.request_icons_for_results(&snapshot.rows[range], show_icons, &icon_ctx);
        }
        // クリック逆流(決定 5): 共有スロットへ積み、main を起こして起動処理させる。
        // ToastAction と同じ遅延 dispatch 型——起動ロジックは main の一箇所に保つ。
        if let Some(i) = clicked {
            // 世代は**このフレームで実際に描いた** snapshot のものを添える（#699）。
            // main が同フレームで行を差し替えていれば、消費側が照合で破棄する。
            shared.push_clicked(snapshot.generation, i);
            crate::egui_shell::wake_main(&self.app_handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::results_list_ui;
    use super::truncate_middle;
    use super::{ClickTake, ResultsShared};
    use super::{RowScroll, row_hover_target, scroll_directive};
    use crate::egui_shell::RowTheme;
    use crate::egui_shell::layout::results_window_height;
    use snotra_core::ui_types::{IconSource, SearchResult};
    use std::collections::HashMap;

    // ---- 行ピッチと窓高の対（kittest・実コードをヘッドレスで走らせる）----------------
    //
    // **ここが `layout.rs` の式と実際の描画を結ぶ唯一の点である。** あちらのユニットテストは
    // 「式が式であること」しか測れず（行間の余白は egui 側にあって見えない）、描画側には
    // これまで検査が無かった。#646 PR2 から続いた「設定した件数が窓に収まらない」欠陥は、
    // どちらの検査からも見えない場所に居たためレビューと CI を素通りしている。

    fn theme_for_test(row_font: f32) -> RowTheme {
        RowTheme {
            name_color: egui::Color32::WHITE,
            path_color: egui::Color32::GRAY,
            selection: egui::Color32::DARK_BLUE,
            hover: egui::Color32::DARK_GRAY,
            name_size: row_font,
            path_size: row_font * 0.78,
            status_size: row_font,
            button_size: row_font,
        }
    }

    fn rows_for_test(n: usize) -> Vec<SearchResult> {
        (0..n)
            .map(|i| SearchResult {
                name: format!("item {i}"),
                path: format!("C:\\test\\item{i}.exe"),
                is_folder: false,
                is_error: false,
                icon: IconSource::FromPath,
            })
            .collect()
    }

    /// 描いた行の総高（`ScrollAreaOutput.content_size.y`）を返す。**`row_height` を変えて
    /// 呼べる**ことが対照実験の足場になる。
    ///
    /// 測定値を `Harness` の state に持たせるのは借用のためである——ローカル変数を閉包へ
    /// 貸すと、`harness` の drop まで借用が続いて読み出せない。
    fn measure_content_height(rows: &[SearchResult], row_height: f32) -> f32 {
        let icons: HashMap<String, egui::TextureHandle> = HashMap::new();
        let theme = theme_for_test(12.0);
        let rows = rows.to_vec();
        let mut harness = egui_kittest::Harness::builder()
            // 窓の実寸に近い幅と、行が全部収まる高さ。**高さは content_size に影響しない**
            // （`content_ui.min_size()` は viewport にクランプされない）が、狭いと
            // `scroll_to_me` が働いて測定が読みにくくなる。
            .with_size(egui::vec2(600.0, 2000.0))
            .build_ui_state(
                move |ui, measured: &mut f32| {
                    let out = results_list_ui(
                        ui, &rows, 0, false, false, &icons, true, &theme, row_height,
                    );
                    *measured = out.content_size.y;
                },
                0.0_f32,
            );
        harness.run();
        *harness.state()
    }

    /// 行描画の **per-row コスト**計測（`/simplify` の Efficiency 指摘の検証）。
    /// `cargo test -p snotra --release kittest_row_draw_cost_probe -- --ignored --nocapture`
    ///
    /// 測りたいのは「画面外の行に毎フレーム払っているコスト」なので、**窓は実寸**
    /// （可視 8 行相当）にする——`measure_content_height` の 2000px 窓では clip rect が
    /// 全行を含み、画面外という状態自体が作れない。`step()`（1 フレーム固定）で測るのは
    /// `run()` が安定するまでの可変フレーム数を割り算に持ち込まないため。
    #[test]
    #[ignore = "計測プローブ（release 実行専用）"]
    fn kittest_row_draw_cost_probe() {
        use std::time::Instant;

        let row_height = 34.0_f32;
        let measure = |n: usize| -> u128 {
            let icons: HashMap<String, egui::TextureHandle> = HashMap::new();
            let theme = theme_for_test(12.0);
            let rows = rows_for_test(n);
            let mut harness = egui_kittest::Harness::builder()
                .with_size(egui::vec2(600.0, row_height * 8.0))
                .build_ui(move |ui| {
                    let _ = results_list_ui(
                        ui, &rows, 0, false, false, &icons, true, &theme, row_height,
                    );
                });
            // galley キャッシュとフォントアトラスを温める（cold 初回を混ぜない）。
            for _ in 0..5 {
                harness.step();
            }
            const N: u32 = 50;
            let t = Instant::now();
            for _ in 0..N {
                harness.step();
            }
            t.elapsed().as_micros() / u128::from(N)
        };

        for n in [8_usize, 200, 1000] {
            println!(
                "[rows={n:>4}] 1 フレーム {}us（frame budget=16700us）",
                measure(n)
            );
        }
    }

    /// **行ピッチが `row_height`（を egui の UI 丸めに通した値）ちょうどである**
    /// ——`item_spacing.y = 0.0` の代入が消えるか、`ScrollArea::show` の後ろへ動くと落ちる
    /// （どちらも今日は他のどの検査にも掛からない）。
    ///
    /// **`round_ui()` を噛ませるのは egui の実挙動だからである。** egui は UI 座標を
    /// `GUI_ROUNDING`（1/32）の倍数へ丸めるため、`row_height = 34.92` の行は実際には
    /// 34.90625 で積まれる。丸めを無視して素の値と比べると、**行間の余白が 0 でも落ちる**
    /// 検査になり、何を縛っているのか読めなくなる（実測して気づいた）。
    ///
    /// 総高を行数で割る形にするのは、1 行だけでは行間が現れないためである。
    #[test]
    fn kittest_row_pitch_equals_row_height() {
        use egui::emath::GuiRounding as _;
        let row_height = 34.92_f32;
        let expected = row_height.round_ui(); // egui が実際に積む 1 行分
        for n in [2_usize, 8, 20] {
            let total = measure_content_height(&rows_for_test(n), row_height);
            let pitch = total / n as f32;
            assert!(
                (pitch - expected).abs() < 1e-4,
                "n={n}: 行ピッチ {pitch} が {expected}（row_height {row_height} の UI 丸め）と\
                 一致しない。総高 {total}・1 行あたり {} の超過は行間に余白が入っている徴候",
                pitch - expected
            );
        }
    }

    /// **`layout::results_window_height` が返す高さと、egui が実際に積む高さが厳密に一致する。**
    /// 式（`layout.rs`）と描画（egui）を直に突き合わせる——一方だけを変えれば落ちる。
    ///
    /// **`row_height` を設定 UI の全域（`font_size` 8..=48）で回す。** ここを 1 つの値で
    /// 固定していたことが、この検査の盲点だった——34.92 は丸めが**下**へ向かう側の値で、
    /// 上へ向かう 13/41 の font_size（12・26〜37）を素通りさせていた。3 種の故障注入で
    /// 発火を確かめておきながら、実装に生きていた 4 つ目の破れに対しては盲目だったのである。
    ///
    /// **等式で測れるのは `results_window_height` が丸めてから掛けるようになったからである**
    /// （案 3）。以前は余りが出るため挟み込みでしか書けなかった。
    #[test]
    fn kittest_rows_exactly_fill_the_window_height() {
        use crate::egui_shell::layout::Metrics;
        for f in 8_u32..=48 {
            let row_height = Metrics::from_config(f, 6, 28).row_height;
            for n in [1_u32, 8, 20, 50] {
                let drawn = measure_content_height(&rows_for_test(n as usize), row_height as f32);
                let window = results_window_height(n, row_height) as f32;
                assert!(
                    (window - drawn).abs() < 1e-3,
                    "font={f} n={n}: 窓 {window} と描画 {drawn} が一致しない（差 {}）。\
                     正なら {} 行目の頭が覗き、負なら最終行が切れる",
                    window - drawn,
                    n + 1
                );
            }
        }
    }

    /// **検出器が発火することの検算**（フォールトインジェクション・`.claude/rules/safety-nets.md`）。
    ///
    /// 上の 2 検査が縛れているかは、**縛りが破れた入力で実際に落ちること**でしか確かめられない。
    /// 実コードを変異させる代わりに、同じ `Harness` へ**行間の余白が入ったのと同じ総高**を
    /// 作る——`row_height` に spacing 相当（3.0）を足して描けば、行が 1 本ずつ高くなる。
    /// これは `item_spacing.y` が戻ったときに起きることと総高において等価である。
    ///
    /// **ずれの大きさまで見る**——「一致しない」だけでは、1/32 の丸め誤差で落ちても同じ
    /// 判定になり、検出器の分解能が確かめられない。
    #[test]
    fn kittest_detector_fires_when_rows_are_taller_than_the_formula_assumes() {
        use egui::emath::GuiRounding as _;
        let row_height = 34.92_f32;
        let n = 8_u32;
        let drawn = measure_content_height(&rows_for_test(n as usize), row_height + 3.0);
        let window = results_window_height(n, f64::from(row_height)) as f32;
        let overflow = drawn - window;
        assert!(
            overflow > 0.0,
            "行が高くなったのに窓へ収まった＝上の 2 検査は何も縛っていない\
             （描画 {drawn} / 窓 {window}）"
        );
        // 超過は「1 行あたり 3.0 が N 行」の規模になる。**両辺とも丸めを通る**——案 3 で
        // `results_window_height` も掛ける前に `round_ui` するようになったため、差は
        // 丸め済みどうしの引き算で表せる（案 3 より前は窓側だけが生の値だった）。
        let expected = ((row_height + 3.0).round_ui() - row_height.round_ui()) * n as f32;
        assert!(
            (overflow - expected).abs() < 1e-3,
            "超過 {overflow} が行間相当 {expected} と規模で合わない"
        );
    }

    /// #724: ホバー開始と終了は同じ述語の true / false で対称に駆動する。選択はポインタと
    /// 直交するため target を変えず、描画側で選択色を優先する。起動不能なエラー行だけを除く。
    #[test]
    fn row_hover_target_tracks_pointer_and_excludes_error_rows() {
        assert!(row_hover_target(true, false));
        assert!(!row_hover_target(false, false));
        assert!(!row_hover_target(true, true));
    }

    #[test]
    fn scroll_directive_maps_gate_and_generation() {
        // 非選択・gate 閉はスクロールしない（選択行以外が寄せを撃つと毎行スクロールになる）。
        assert_eq!(scroll_directive(false, true, true), RowScroll::None);
        assert_eq!(scroll_directive(true, false, false), RowScroll::None);
        // 世代不変の選択移動（↑↓）はアニメーション維持（#714 issue 要件）。
        assert_eq!(scroll_directive(true, true, false), RowScroll::Animated);
        // 世代交代（結果集合の総入れ替え）は瞬時——前のリストの位置を持ち越さない（#714）。
        assert_eq!(scroll_directive(true, true, true), RowScroll::Instant);
    }

    // ---- クリック逆流の世代照合（#699）----
    //
    // 実機の競合窓（results がクリックを積んでから main が消費するまでに行が総入れ替え
    // される）は**手で再現できない**。純粋核をここで固定するのが唯一の担保である。

    #[test]
    fn clicked_survives_matching_generation() {
        let shared = ResultsShared::default();
        shared.push_clicked(7, 2);
        assert!(matches!(shared.take_clicked_for(7), ClickTake::Current(2)));
    }

    /// 世代が食い違えば破棄する。**かつスロットが空く**——残すと次フレーム以降に
    /// 同じ stale クリックが再評価される。
    #[test]
    fn clicked_is_discarded_on_generation_mismatch() {
        let shared = ResultsShared::default();
        shared.push_clicked(7, 2);
        assert!(matches!(
            shared.take_clicked_for(8),
            ClickTake::Stale { stamped: 7 }
        ));
        assert!(
            matches!(shared.take_clicked_for(7), ClickTake::None),
            "破棄後は空であること（元の世代で引いても戻らない）"
        );
    }

    /// 積まれていなければ `None`。`Stale` と区別できることが trace の前提。
    #[test]
    fn clicked_is_none_when_never_pushed() {
        let shared = ResultsShared::default();
        assert!(matches!(shared.take_clicked_for(0), ClickTake::None));
    }

    /// last-wins（既存契約の回帰）。世代同梱で壊れていないこと。
    #[test]
    fn clicked_is_last_wins() {
        let shared = ResultsShared::default();
        shared.push_clicked(3, 0);
        shared.push_clicked(3, 5);
        assert!(matches!(shared.take_clicked_for(3), ClickTake::Current(5)));
    }

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
