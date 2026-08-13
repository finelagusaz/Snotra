//! 検索 dispatch の同一性を測る純粋核（#1004）。
//!
//! **`SearchState::rows_generation` とは別の量である**——世代は「行が差し替わったか」、ここの seq は「どの要求か」を指す。#699 の世代は `set_results` が持ったままにする。
//!
//! **PR 1（同期）と PR 2（worker）で同じ型を使う**——計器が改修の前後で同じ区間を測ることが受け入れの条件である（#1000 の「同じ器を当てられること」）。

use std::time::{Duration, Instant};

/// 採り込みが成立したときの経過。**打鍵起点と dispatch 起点の両方を持つ**——打鍵起点には 50 ms の trailing debounce 待ちが必ず入るため、片方では worker 往復の費用を読めない。
pub struct Settled {
    pub seq: u64,
    pub since_key: Duration,
    pub since_dispatch: Duration,
}

struct Pending {
    seq: u64,
    key_at: Instant,
    dispatched_at: Instant,
}

#[derive(Default)]
pub struct SearchDispatch {
    next_seq: u64,
    pending: Option<Pending>,
}

impl SearchDispatch {
    /// 新しい要求へ seq を振る。前の要求は破棄される（最新クエリ勝ち）。
    pub fn issue(&mut self, key_at: Instant, now: Instant) -> u64 {
        self.next_seq += 1;
        self.pending = Some(Pending {
            seq: self.next_seq,
            key_at,
            dispatched_at: now,
        });
        self.next_seq
    }

    /// 結果が届いたときに呼ぶ。**現 pending と一致するときだけ `Some`** を返し、pending を消す。
    pub fn accept(&mut self, seq: u64, now: Instant) -> Option<Settled> {
        match &self.pending {
            Some(p) if p.seq == seq => {}
            _ => return None,
        }
        let pending = self.pending.take()?;
        Some(Settled {
            seq,
            since_key: now.duration_since(pending.key_at),
            since_dispatch: now.duration_since(pending.dispatched_at),
        })
    }

    /// in-flight を失効させる。**同期で `set_results` を呼ぶ出所は必ずここを通す**（spec §4.5）。
    pub fn invalidate(&mut self) {
        self.pending = None;
    }

    /// 現在 in-flight の seq（無ければ 0）。判定器が「失効した結果を採っていないか」を読む材料である。
    pub fn pending_seq(&self) -> u64 {
        self.pending.as_ref().map_or(0, |p| p.seq)
    }
}

/// **最終クエリの結果がまだ行へ反映されていないか**（#1038）。[`crate::egui_shell::search_state::should_flush_on_enter`] の第 3 引数がこれである。
///
/// **`armed` だけでは足りない。** 同期実装の頃は `armed == false` が「反映済み」を含意していたが、worker 化（#1004）で trailing 発火の直後は必ず「`armed == false` かつ in-flight あり」になり、含意が壊れた。
///
/// **`armed` を残すのは、最新クエリの要求がまだ出ていない間を覆うためである。** `pending_seq == 0` だが未反映、という状態はバースト継続中に直前の seq が `accept` 済みのフレーム（[`crate::egui_shell::layout::Debouncer::on_input`] は armed を立てるだけで発行せず、**打鍵のフレームで**要求を出すのは leading だけである——trailing は [`crate::egui_shell::layout::Debouncer::poll`] 経由で別に出す）と、空クエリ・`indexing`・送信失敗が [`SearchDispatch::invalidate`] を撃った後に打鍵が armed だけ残した状態で生じる。**これらを名指すのは、「打鍵したフレームの一瞬」と誤読されるのを防ぐためである**——どちらも複数フレームにわたって続き、解けるのは trailing が発火するか（[`crate::egui_shell::layout::Debouncer::poll`]・**最後の**打鍵から interval）、`cancel` / [`SearchDispatch::invalidate`] が撃たれたときである。**打鍵が続く限り armed は立ったままなので、持続はバーストの長さで決まる。**
///
/// **armed が下りる経路はメソッドだけではない。** `LauncherController::consume_reset_pending` は show のたびに [`crate::egui_shell::layout::Debouncer`] を丸ごと作り直すため、`poll` も `cancel` も通らずに armed が落ちる（[`crate::egui_shell::layout::Debouncer::new`] は `armed: false` で作る）。**この述語を型の内側へ移すなら、reset 経路で落とす責務も一緒に移ること**——いま取り残しが起きないのは `Debouncer` ごと捨てているからであり、フィールドだけを移した瞬間にその救いは消える。対で動く `LauncherController::last_input_at` は `consume_reset_pending` が触らないので show を跨いで古いまま残っており、**今それが無害なのは `armed` が false だからである**——`armed` を移して reset を忘れると、show 直後の最初のフレームで `poll` が「隠れていた時間」を経過として読み、その場で trailing を撃つ。**同型の残余（show を跨ぐ状態を足して `consume_reset_pending` の一覧へ入れ忘れる形。検知手段が無いことまで）と、その解き方の先例は [`crate::egui_shell::lifecycle::BlurGrace::reset`] の doc が正本である**（#745。`*self = Self::default()` を `consume_reset_pending` から呼ぶ形で、フィールド代入ではなくメソッドで丸ごと畳む）。
///
/// `pending_seq` を `bool` でなく生値で受けるのは、**sentinel（0 = in-flight なし）の解釈をテストの届く場所へ入れる**ためである（[`SearchDispatch::issue`] が `next_seq += 1` を先に行うので seq は 1 始まり）。
///
/// **#1039 への申し送り**（この述語を型の内側へ移すとき）:
///
/// - **否定形で置いたのは呼び出し点に `!` を出さないためであり、#1039 の issue 本文が想定する肯定形 `is_settled()` とは極性が逆である**（引っ越し時は `!is_unsettled(..)` として吸収する）。
/// - **[`crate::egui_shell::results_view::RowsSnapshot::settled`]（icon worker のゲート・`!armed`）とは別概念である**——両者は否定の関係に無く、あちらは正しさの述語ではなく「連打中はアイコンを積まない」perf ヒューリスティックである。同じ語なので同一視しないこと。
pub fn is_unsettled(armed: bool, pending_seq: u64) -> bool {
    armed || pending_seq != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_the_latest_seq() {
        let base = Instant::now();
        let mut d = SearchDispatch::default();
        let first = d.issue(base, base + Duration::from_millis(50));
        let second = d.issue(
            base + Duration::from_millis(60),
            base + Duration::from_millis(110),
        );
        assert!(second > first, "seq は単調に増える");
        assert!(
            d.accept(first, base + Duration::from_millis(120)).is_none(),
            "追い越された結果は採らない"
        );
        let settled = d
            .accept(second, base + Duration::from_millis(130))
            .expect("最新の seq は採る");
        assert_eq!(settled.since_key, Duration::from_millis(70), "打鍵起点");
        assert_eq!(
            settled.since_dispatch,
            Duration::from_millis(20),
            "dispatch 起点"
        );
    }

    #[test]
    fn accept_is_once_per_issue() {
        let base = Instant::now();
        let mut d = SearchDispatch::default();
        let seq = d.issue(base, base);
        assert!(d.accept(seq, base).is_some());
        assert!(
            d.accept(seq, base).is_none(),
            "同じ結果を二度採らない（採り込みは行の差し替えと一対一）"
        );
    }

    #[test]
    fn invalidate_drops_in_flight() {
        let base = Instant::now();
        let mut d = SearchDispatch::default();
        let seq = d.issue(base, base);
        d.invalidate();
        assert!(
            d.accept(seq, base).is_none(),
            "同期で行を差し替えたら in-flight は必ず古い（spec §4.5）"
        );
    }

    #[test]
    fn unsettled_covers_in_flight_after_trailing_fired() {
        assert!(
            !is_unsettled(false, 0),
            "予約も in-flight も無ければ反映済み"
        );
        assert!(
            is_unsettled(true, 0),
            "最新クエリの要求がまだ出ていない（バースト継続中・invalidate 直後の打鍵）"
        );
        assert!(
            is_unsettled(false, 1),
            "trailing 発火の直後（armed == false かつ in-flight あり）が #1038 の欠陥そのものである"
        );
        assert!(is_unsettled(true, 3), "両方立っていても未反映");
        // #1038 の受け入れ 1 を逐語で写す（`should_flush_on_enter` との合成まで固定する）。
        assert!(crate::egui_shell::should_flush_on_enter(
            crate::egui_shell::ViewKind::Results,
            true,
            is_unsettled(false, 1),
        ));
    }

    #[test]
    fn unsettled_is_grounded_on_real_dispatch() {
        // sentinel をリテラルで書かず `SearchDispatch` 自身から取る（判定の入力が出力側へすり替わる不動点化を避けるため、`armed` は false 固定で渡す）。
        let base = Instant::now();
        let mut d = SearchDispatch::default();
        assert!(!is_unsettled(false, d.pending_seq()), "発行前は反映済み");
        let seq = d.issue(base, base);
        assert!(
            is_unsettled(false, d.pending_seq()),
            "worker へ出した直後は未反映"
        );
        assert!(d.accept(seq, base).is_some());
        assert!(
            !is_unsettled(false, d.pending_seq()),
            "採り込めば反映済みへ戻る"
        );
        let _ = d.issue(base, base);
        d.invalidate();
        assert!(
            !is_unsettled(false, d.pending_seq()),
            "同期で差し替えた（invalidate）後も反映済みである"
        );
    }

    #[test]
    fn stale_result_is_dropped_after_synchronous_replacement() {
        let base = Instant::now();
        let mut d = SearchDispatch::default();
        // `c:\u` を打って worker が走り出した
        let in_flight = d.issue(base, base);
        // クエリを空にした → 同期でクリアした出所が invalidate を呼ぶ
        d.invalidate();
        // worker の結果が遅れて届く
        assert!(
            d.accept(in_flight, base + Duration::from_millis(20))
                .is_none(),
            "空クエリの下に古い行が生え直してはならない"
        );
    }
}
