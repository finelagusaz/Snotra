//! results 窓の所有型（#671 spec 決定 2）。
//!
//! 生 Win32 の 3 点セット（`SW_SHOWNOACTIVATE` / `SW_HIDE` / `SetWindowPos`）と可視フラグ、
//! および直近に適用したサイズ（#749 で `view.rs` から移設）を 1 つの型が同時に所有する。
//! **可視フラグとサイズ memo は概念が別である**——前者は correctness、後者は冗長な Win32
//! 呼び出しを避ける性能上のガードで、誤っても窓は消えない（#671 spec 決定 2 の意図的な分割）。#646 PR2 では 3 関数が自由関数で、可視フラグは
//! `SearchWindowView` 側の view-local な bool であり、片方の hide 経路（`drive_results_window`）
//! だけが更新し、もう片方（`hide_egui_main`）は更新しない非対称があった。reset-on-show が
//! 後始末することで閉じていたが、**窓とフラグを同じ物として持てば 2 経路が同じ
//! オブジェクトを通る**ため、この非対称は構造的に消える。
//!
//! **得られないもの**: `Manager` から results の生ハンドルを引いて `.hide()` を呼ぶ書き方は
//! 依然コンパイルが通り、実行時に黙って no-op する（tao の `WindowFlags::VISIBLE` が raw show
//! 後も false のままであるため）。ハンドルの取得は `AppHandle` を持つ誰からでもできるため
//! footgun は表現不能にできない。本型の目的は**正しい経路を 1 つにし、誤った経路を書く動機を
//! 消す**ことであって、表現不能化ではない（spec §2.6 / §7-1）。
//!
//! **可視性の全体像はこの型だけでは閉じない。** main が hidden の間に results が出る事故を
//! 塞ぐのは show 述語側のゲート（`layout::present_results` の連言① `main_visible`）である。
//! 本型は「誰が raw 操作を撃つか」を一点に集めるのであって、「撃ってよい状況か」は判定しない。
//!
//! **かつては事後検査（`layout::must_retract_results`）と、main が消える向きの hide を
//! 無条件にする理由の型（`layout::HideReason::MainGone`）も要った**——可視性を変える操作が
//! 複数スレッドから呼べた頃は、ゲートの読みと raw 操作の間に別スレッドの hide が割り込み、
//! 「フラグ = false・窓 = 可視」の食い違いが残りえたためである。**#880 サイクル段 2 で
//! 可視性を変える 5 関数が証人型（`EventLoopProof`）によりイベントループスレッドへ閉じ、
//! その並びが構築不能になったため、同段で撤去した。**（閉包の射程の正本は
//! `src-tauri/CLAUDE.md`「可視性を変える操作はイベントループスレッドに閉じてある」の bullet 群）

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// results 窓（`focusable(false)` の従属窓）とその可視状態。
///
/// `Deref<Target = tauri::Window>` は**実装しない**——実装すると `.hide()` / `.show()` /
/// `.set_always_on_top()` が生え、この型が避けている当の footgun が復活する。
/// 必要な操作は inherent method として明示的に公開する。
pub(crate) struct ResultsWindow {
    window: tauri::Window,
    /// raw show / hide の直近状態。`Cell` ではなく `AtomicBool` である理由: managed state は
    /// `Send + Sync` を要求する。**書き手（swap する側）は `show` / `hide` の 2 経路だけであり、
    /// どちらも証人型（`EventLoopProof`）によりイベントループスレッドへ閉じている**——
    /// topmost 復帰（`commands/window.rs` の設定プロセス監視、spawn したポーリングスレッド
    /// から来る）は `set_topmost` の doc が言うとおり可視フラグを変えない。`Ordering` は
    /// 同居する `main_visible` / `hotkey_generation` に合わせ `SeqCst`。
    visible: AtomicBool,
    /// 直近 `set_size` の (幅, 高さ)（論理 px・#749 で `view.rs` の `last_results_width` /
    /// `last_results_height` から移設）。**`visible` とは概念が別**——冗長な Win32 呼び出しを
    /// 避ける性能上のガードであり、correctness のフラグではない。
    ///
    /// **物理指定へ移った後も論理のまま保つ**（案 3）。`size_delta_exceeds` の許容 0.5 は
    /// 論理 px を想定した値であり、物理で覚えると同じ 0.5 が scale 2.0 では論理 0.25 を
    /// 意味してガードが実質狭まる。**覚える単位と比べる単位を揃える**のが要点で、
    /// 物理への変換は比較を済ませた後に行う。
    ///
    /// `Cell` ではなく `Mutex` である理由は `visible` が `AtomicBool` である理由と同じ
    /// （managed state は `Send + Sync` を要求する）。f64 の組は atomic で表せない。
    /// **書き手はイベントループスレッドの 2 経路だけである**（`window_coordinator` の
    /// `drive_results_window` と、view の reset-on-show が呼ぶ `reset_size_guard`）——
    /// `commands/window.rs` のポーリングスレッドが触るのは `set_topmost` だけで、
    /// この memo を読まない。
    last_size: Mutex<(f64, f64)>,
    /// 直近に適用した下地の色。**`last_size` と同じ「性能上のガード」層**であり correctness の
    /// フラグではない——`set_background_color` は値については冪等だが**副作用については冪等でない**
    /// （tao は無条件に `InvalidateRect(erase=true)` + `UpdateWindow` を撃つ）。同じ色を撃ち直すと
    /// クライアント領域全体の消去を毎回誘発するだけで、得るものが無い。
    ///
    /// **決定 3 の論拠を壊さない**: 「hidden 中は `update()` が走らず変化の瞬間に居合わせられない」
    /// が要求するのは「変化後の**最初の**呼び出しで撃つ」ことであって、変化していない呼び出しでも
    /// 撃つことではない。hidden 中に色が変わっても、次の show / リサイズでこの比較が差分を検出する。
    last_background: Mutex<Option<egui::Color32>>,
}

impl ResultsWindow {
    /// 窓ハンドルを取り込む。**`create()` が `.visible(false)` で生成した直後に呼ぶ**前提で
    /// 初期値は false（builder の宣言と一致させる。`mod.rs` の `create` を参照）。
    /// `last_size` の初期値 `(0.0, 0.0)` は「まだ一度も適用していない」を表し、最初の
    /// `set_size` が必ず撃たれるようにする（旧 `SearchWindowView::new` と同値）。
    pub(crate) fn new(window: tauri::Window) -> Self {
        Self {
            window,
            visible: AtomicBool::new(false),
            last_size: Mutex::new((0.0, 0.0)),
            // `create` が builder の `.background_color` で同じ色を入れているが、**それを初期値に
            // しない**——一致を仮定すると、生成時と show 直前で config が変わった場合に撃たなくなる。
            last_background: Mutex::new(None),
        }
    }

    /// results 窓を**フォーカスを奪わずに**表示する（#646 PR2・実機スモークで発見）。
    /// 既に可視なら raw 操作を撃たず `false` を返す。**表示へ遷移したときだけ `true`**。
    ///
    /// 戻り値は呼び出し側の trace 用である（trace を本型の内側に置かない理由は
    /// spec 決定 7——`egui_results:show` は `drive_results_window` が 1 回だけ出す）。
    ///
    /// `tauri::Window::show()` は tao の `set_visible(true)` を経て `ShowWindow(hwnd, SW_SHOW)` を
    /// 呼ぶが、`SW_SHOW` は**プログラム的に窓を活性化する**。`focusable(false)` が付ける
    /// `WS_EX_NOACTIVATE` が防ぐのはユーザークリックによる活性化だけなので、1 文字目の入力で
    /// results が現れた瞬間に入力欄からフォーカスが奪われ 2 文字目が打てなくなる。
    /// tao 内部で `SW_SHOWNOACTIVATE` に至る唯一の経路（`MARKER_DONT_FOCUS`）は窓生成時に
    /// 1 回だけ立ち初回 show で消費されるため、繰り返し show する用途には使えない。
    ///
    /// `background` は下地（softbuffer が present するまでの一瞬に見えるネイティブブラシ）へ
    /// 適用する。**show 遷移のときだけ撃つ**（下の早期 return の後に置く理由）——可視のまま
    /// 毎フレーム撃つと `InvalidateRect` + `UpdateWindow` を送り続けることになる。
    ///
    /// **`_el` はイベントループスレッド上であることの証人である**（`EventLoopProof`）。この型は
    /// 証人を使わないが（`_` 始まり）、**シグネチャから外してはならない**——results の可視性を
    /// 変える経路を単一スレッドへ閉じるための拘束である。証人型を引数に要求する 5 関数は
    /// イベントループスレッドへ一意化されており、フラグと窓の実状態が食い違う並びは構築できない
    /// （射程の正本は `src-tauri/CLAUDE.md`「可視性を変える操作はイベントループスレッドに閉じてある」の bullet 群）。
    pub(crate) fn show(
        &self,
        _el: &snotra_egui_runtime::EventLoopProof,
        background: egui::Color32,
    ) -> bool {
        // 先に flag を swap して test-and-set を原子にする。**証人型（`EventLoopProof`）を
        // 引数に要求する 5 関数はイベントループスレッドへ閉じている**ため、swap と `raw_show()`
        // の間に他スレッドの `hide()` が挟まる並びは構築できない——フラグと窓の実状態が食い違う
        // 並びも同じ理由で構築できない。swap 自体は残す——「遷移したときだけ raw 操作を撃つ」
        // 戻り値の契約がこれで決まるためである。
        if self.visible.swap(true, Ordering::SeqCst) {
            return false;
        }
        self.apply_native_background(background);
        self.raw_show();
        true
    }

    /// 下地を config 色へ合わせる（本体は `window_coordinator::apply_native_background`——
    /// main と同じ経路を通す理由はそちらの doc）。**リサイズでも下地が露出する**ため
    /// show 遷移時とサイズ変更時の両方で撃つ（SU6 spec 決定 2 の codex 反証）。
    ///
    /// **private である**: 呼ぶのは `show` / `set_size` という遷移判定・デルタガードの内側だけで、
    /// 外から撃てると「ガードの内側でだけ撃つ」不変条件が型の外で破れる。`ResultsWindow` は
    /// raw 操作の所有点であり、外へ出す面を増やすことがこの型の値打ちを削る。
    ///
    /// tao の `set_background_color` は `window_state` への代入と `InvalidateRect` だけで
    /// `apply_diff` を通らない（tao 0.35.3 実測）。ゆえに「results の 3 操作は raw へ寄せる」
    /// 規約（`src-tauri/CLAUDE.md`「Win32 / Tauri 注意事項」）の対象外であり、tao 経由でよい。
    fn apply_native_background(&self, color: egui::Color32) {
        {
            // lock は Win32 呼び出しの前に手放す（`set_size` と同じ理由——再入不可の Mutex を
            // 握ったまま tao の窓プロシージャへ至りうる経路を作らない）
            let mut last = self.last_background.lock().unwrap();
            if *last == Some(color) {
                return;
            }
            *last = Some(color);
        }
        super::window_coordinator::apply_native_background(&self.window, color);
    }

    /// results 窓を隠す（`show` の対）。既に不可視なら raw 操作を撃たず `false` を返す。
    ///
    /// raw show は tao の `WindowFlags::VISIBLE` を false のまま残すため、`Window::hide()` は
    /// 「差分なし」と判定して早期 return し窓が隠れない。ゆえに hide も対で raw にする。
    ///
    /// **可視フラグを信じてよい。** 書き手はイベントループスレッドに一意化されており
    /// （`EventLoopProof`）、フラグと窓の実状態が食い違う並びは構築できない。
    pub(crate) fn hide(&self, _el: &snotra_egui_runtime::EventLoopProof) -> bool {
        if !self.visible.swap(false, Ordering::SeqCst) {
            return false;
        }
        self.raw_hide();
        true
    }

    /// results 窓の TOPMOST を切り替える（設定サイドカー起動中の一時解除・#646 PR2）。
    /// `set_always_on_top` は tao のフラグ差分適用を通り、`VISIBLE` を false と信じている
    /// results 窓に対しては `SW_HIDE` を副作用で撃ってしまう。`SWP_NOACTIVATE` 付きの
    /// `SetWindowPos` で Z オーダーだけを動かす。**可視フラグは変えない**——Z 順の変更は
    /// 表示/非表示の遷移ではない。
    #[cfg(windows)]
    pub(crate) fn set_topmost(&self, topmost: bool) {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
        };
        let Ok(hwnd) = self.window.hwnd() else { return };
        let insert_after = if topmost {
            HWND_TOPMOST
        } else {
            HWND_NOTOPMOST
        };
        unsafe {
            let _ = SetWindowPos(
                HWND(hwnd.0),
                Some(insert_after),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }

    #[cfg(not(windows))]
    pub(crate) fn set_topmost(&self, topmost: bool) {
        let _ = self.window.set_always_on_top(topmost);
    }

    /// サイズを設定する。**高さは物理 px で指定する**（案 3・#835 の受容残余の巻き戻し）。
    ///
    /// **なぜ物理か**: `LogicalSize` を渡すと tao が `round` で物理へ落とし、半分の確率で
    /// 下へ倒れて最終行が削れる（実測 10,250 通り中 3,702 件）。切り上げは
    /// `layout::results_height_phys` が担い、ここはその結果を適用するだけである
    /// （`ceil` を窓の型へ持ち込むとユニットテストが届かなくなる）。
    ///
    /// **この窓の `scale_factor()` を読み、その場で `ResultsScale` へ包む。** #835 のクランプ撤去で「results 窓の scale を読む箇所」は一度消え、`layout::results_top_y` の doc は「同型の値が 1 種類になったので取り違えは構造的に起こらない」と記していた。案 3 で読みは戻ったが、**残余としては戻していない**——`MainScale` / `ResultsScale` に型で分かれており、取り違えはコンパイルが通らない（実測: 双方向で `expected ResultsScale, found MainScale` / その逆）。**読む窓と型は同じ式で決めること**——先に `f64` へ落として後から包む書き方にすると、包む場所が読む場所から離れて取り違えが戻る。
    ///
    /// **幅も高さも `layout::results_size_phys` の 1 つの口を通す。** 幅は `round`（行の描画に
    /// 影響しないので足りる。`ceil` にすると幅だけが 1px ずつ育つ）、高さは `ceil`。
    /// **`ResultsScale` から生の `f64` を取り出さない**——取り出せる口を作ると、そこで
    /// `MainScale` の値を包み直せてしまい、型で分けた意味が消える。
    ///
    /// **tao 経由のままにする。**
    ///
    /// 理由は「差分適用を通らないから」では**ない**——tao 0.35.3 の `set_inner_size` /
    /// `set_outer_position` はどちらも `set_window_flags(|f| f.set(MAXIMIZED, false))` を呼び
    /// `apply_diff` に**入る**。results では MAXIMIZED が元から false ゆえ**フラグ差分が空**に
    /// なり、`apply_diff` 冒頭の `if diff == empty { return }` で助かっている。
    ///
    /// **判定基準は「apply_diff を通るか」ではなく「フラグ差分が生じるか」である。**
    /// 差分を生む操作（`set_resizable` 等）は `apply_diff` 末尾の
    /// `if !new.contains(VISIBLE) { ShowWindow(SW_HIDE) }` に到達し、results 窓を消す
    /// （`set_always_on_top` が #646 PR2 で窓を消したのと同一機構）。
    ///
    /// **デルタガードを内蔵する**（#749）——同値のフレームでは Win32 を撃たない。判定式の
    /// 正本は `layout::size_delta_exceeds`（純粋核・ユニットテスト対象）で、ここに手書きしない。
    /// `show()` / `hide()` が「遷移したときだけ raw 操作を撃つ」のと同型である。
    ///
    /// **lock は Win32 呼び出しの前に手放す。** `std::sync::Mutex` は再入不可であり、tao の
    /// `set_inner_size` は `set_window_flags` → `apply_diff` を経て窓プロシージャに至りうる
    /// ——guard を握ったまま呼ぶ形は、将来その経路が再入したときにデッドロックする。
    /// 手放すことによる TOCTOU は生じない（書き手は `last_size` の doc のとおり単一スレッド）。
    /// `background` は**リサイズで露出する下地**へ適用する（`apply_native_background` の doc）。
    /// デルタガードの内側で撃つため、同値のフレームでは Win32 を呼ばない。
    pub(crate) fn set_size(&self, width: f64, height: f64, background: egui::Color32) {
        // **scale はデルタガードより前に読む。** memo は論理値のままに保つ（`last_size` の doc）
        // ——物理へ移すと許容 0.5 の意味が scale で変わり、scale 2.0 では論理 0.25 の
        // ガードになる（撃つ頻度が意図せず上がる）。比較する単位と覚える単位を揃える。
        // **読む窓と型を同じ式で決める**（`layout::ResultsScale` の doc）——先に `f64` へ
        // 落として後から包む書き方にすると、包む場所が読む場所から離れて取り違えが戻る。
        let scale =
            crate::egui_shell::layout::ResultsScale::new(self.window.scale_factor().unwrap_or(1.0));
        {
            let mut last = self.last_size.lock().unwrap();
            if !crate::egui_shell::layout::size_delta_exceeds(*last, (width, height)) {
                return;
            }
            *last = (width, height);
        }
        let (width_phys, height_phys) =
            crate::egui_shell::layout::results_size_phys(width, height, scale);
        let _ = self
            .window
            .set_size(tauri::PhysicalSize::new(width_phys, height_phys));
        self.apply_native_background(background);
    }

    /// サイズ memo を「まだ適用していない」へ戻す（#749・旧 `view.rs` の reset-on-show 2 行）。
    ///
    /// 再 show 後に必ず一度は現行 metrics で `set_size` させるためのもので、**呼ぶのは view の
    /// reset-on-show（`reset_pending` の消費）である**。呼び出し点をそこに保つのは順序の
    /// ためである——同一フレームの `drive_results_window` より**前**でなければ、再 show 後の
    /// 1 フレーム目が旧 metrics のサイズで描かれる。`show_egui_main` へ移すと、この
    /// 「**同一フレーム**」の前提が消える——あちらは証人型（`EventLoopProof`）の導入で
    /// 同じイベントループスレッドに閉じたが、**フレームの中ではない**（`update()` の外から
    /// 走る）。呼び出し点はここに保つ。
    pub(crate) fn reset_size_guard(&self) {
        *self.last_size.lock().unwrap() = (0.0, 0.0);
    }

    // 物理 ↔ 論理の換算係数（`scale_factor`・#675）は #835 のクランプ撤去で消えた。
    // **この窓の scale を crate 側で読む必要が無くなったためである**——`set_size` へ渡すのは
    // 論理 px であり、tao の `set_inner_size` がこの窓の `scale_factor()` で物理へ戻す。
    // 読んでいたのは「作業領域の残り（物理）を論理へ換算する」ためだけだった。

    /// 物理座標で位置を設定する（`set_size` と同じ理由で tao 経由）。
    pub(crate) fn set_position(&self, x: i32, y: i32) {
        let _ = self.window.set_position(tauri::PhysicalPosition::new(x, y));
    }

    #[cfg(windows)]
    fn raw_show(&self) {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{SW_SHOWNOACTIVATE, ShowWindow};
        let Ok(hwnd) = self.window.hwnd() else { return };
        unsafe {
            let _ = ShowWindow(HWND(hwnd.0), SW_SHOWNOACTIVATE);
        }
    }

    #[cfg(not(windows))]
    fn raw_show(&self) {
        let _ = self.window.show();
    }

    #[cfg(windows)]
    fn raw_hide(&self) {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};
        let Ok(hwnd) = self.window.hwnd() else { return };
        unsafe {
            let _ = ShowWindow(HWND(hwnd.0), SW_HIDE);
        }
    }

    #[cfg(not(windows))]
    fn raw_hide(&self) {
        let _ = self.window.hide();
    }
}
