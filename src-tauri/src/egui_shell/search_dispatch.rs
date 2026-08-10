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
}
