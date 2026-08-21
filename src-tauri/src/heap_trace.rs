//! Rust ヒープ常駐の計装（`heap-trace` feature を有効にしたビルドでのみ効く）。
//!
//! **既定ビルドには入らない。** feature 無しでは `#[global_allocator]` を差し替えず、
//! [`snapshot`] は `None` を返し、`trace.rs` の出力も従来どおりである（費用ゼロ）。
//! 有効時は確保・解放ごとに 2 つの `Relaxed` 原子操作が乗るので、**フレーム時間の計測と
//! 同じ実行で使わないこと**（計器が測定対象を乱す側の道具である）。
//!
//! **測るもの**: live バイト（`Layout::size()` の和）・live ブロック数・確保/解放の累計。
//! **測らないもの**: Windows ヒープのブロックヘッダとサイズクラス丸め、OS・シェル DLL・
//! スレッドスタック・GDI の面。ゆえに **プロセスの commit（PrivComm）とは必ず食い違い、
//! その差こそが「我々のヒープではない分」である**——この差を読むことが本計装の目的で、
//! 一致しないことは欠陥ではない（同じ流儀の先例は `snotra-core/tests/memory_footprint.rs`）。
//!
//! **出力先は既存の `[trace]` 行**（`SNOTRA_TRACE`）であり、新しい env も新しい行も足さない。
//! 段階ごとの標本は「その段階の終わりに既存イベントを起こす」ことで採る（`egui_show:done` /
//! `egui_results:show` / `egui_hide:done`）。区間ごとに専用の行を吐かない理由は
//! `PERFORMANCE.md`「計測と受け入れ基準」。
//!
//! **撤去条件**: 検索段の PrivComm 増分のうち Rust ヒープの取り分が `PERFORMANCE.md` へ
//! 出所つきで着地し、以後の判断がその記録で足りるようになったら消してよい。撤去対象は
//! 本ファイル・`src-tauri/Cargo.toml` の `heap-trace` feature・`trace.rs` の `heap` 欄の合流・
//! `main.rs` の `mod heap_trace;`・`src-tauri/CLAUDE.md` のモジュール索引行である。

/// 1 時点のヒープ常駐。[`snapshot`] が返す（feature 無効時は `None`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HeapSnapshot {
    pub(crate) live_bytes: usize,
    pub(crate) live_blocks: usize,
    pub(crate) allocs: u64,
    pub(crate) frees: u64,
}

/// `[trace]` 行の `heap` 欄。**KiB へ丸めた値と生バイトの両方を出す**——読むのは人間だが、
/// 差分を取るのは後段のスクリプトであり、丸めた値どうしの引き算では 1 KiB 未満の
/// 変化が消える。
pub(crate) fn heap_fields(snapshot: &HeapSnapshot) -> serde_json::Value {
    serde_json::json!({
        "live_bytes": snapshot.live_bytes,
        "live_kib": snapshot.live_bytes / 1024,
        "live_blocks": snapshot.live_blocks,
        "allocs": snapshot.allocs,
        "frees": snapshot.frees,
    })
}

/// 現在のヒープ常駐。**feature 無効なら `None`**（呼び出し側は何も出さない）。
pub(crate) fn snapshot() -> Option<HeapSnapshot> {
    #[cfg(feature = "heap-trace")]
    {
        use std::sync::atomic::Ordering;
        Some(HeapSnapshot {
            live_bytes: counting::LIVE.load(Ordering::Relaxed),
            live_blocks: counting::BLOCKS.load(Ordering::Relaxed),
            allocs: counting::ALLOCS.load(Ordering::Relaxed),
            frees: counting::FREES.load(Ordering::Relaxed),
        })
    }
    #[cfg(not(feature = "heap-trace"))]
    {
        None
    }
}

/// 計数アロケータ本体。**`realloc` / `alloc_zeroed` は実装しない**——`GlobalAlloc` の既定実装が
/// `self.alloc` / `self.dealloc` を経由するため、この 2 つを数えれば両方が数に入る。
#[cfg(feature = "heap-trace")]
mod counting {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    pub(super) static LIVE: AtomicUsize = AtomicUsize::new(0);
    pub(super) static BLOCKS: AtomicUsize = AtomicUsize::new(0);
    pub(super) static ALLOCS: AtomicU64 = AtomicU64::new(0);
    pub(super) static FREES: AtomicU64 = AtomicU64::new(0);

    pub(super) struct Counting;

    // 順序は `Relaxed` でよい——読み手（`snapshot`）は「ある瞬間の整合した組」ではなく
    // 規模を見る。強い順序を要求すると確保のたびにバリアが乗り、計器の費用が跳ねる。
    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let ptr = unsafe { System.alloc(layout) };
            if !ptr.is_null() {
                LIVE.fetch_add(layout.size(), Ordering::Relaxed);
                BLOCKS.fetch_add(1, Ordering::Relaxed);
                ALLOCS.fetch_add(1, Ordering::Relaxed);
            }
            ptr
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
            BLOCKS.fetch_sub(1, Ordering::Relaxed);
            FREES.fetch_add(1, Ordering::Relaxed);
            unsafe { System.dealloc(ptr, layout) }
        }
    }

    #[global_allocator]
    static GLOBAL: Counting = Counting;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 生バイトと KiB の両方を出す（丸めた値どうしの差分で小さい変化を落とさないため）。
    #[test]
    fn heap_fields_carry_both_raw_bytes_and_rounded_kib() {
        let value = heap_fields(&HeapSnapshot {
            live_bytes: 3_145_728,
            live_blocks: 4_096,
            allocs: 1_000,
            frees: 900,
        });
        assert_eq!(value["live_bytes"], 3_145_728);
        assert_eq!(value["live_kib"], 3_072);
        assert_eq!(value["live_blocks"], 4_096);
        assert_eq!(value["allocs"], 1_000);
        assert_eq!(value["frees"], 900);
    }

    /// 1 KiB 未満は KiB 側では 0 に潰れる。**潰れても生バイトが残る**ことがこの形の要件である。
    #[test]
    fn heap_fields_keep_sub_kib_values_in_raw_bytes() {
        let value = heap_fields(&HeapSnapshot {
            live_bytes: 512,
            live_blocks: 1,
            allocs: 1,
            frees: 0,
        });
        assert_eq!(value["live_kib"], 0);
        assert_eq!(value["live_bytes"], 512);
    }

    /// feature 無効の既定ビルドでは計器が動かない——`None` は「測っていない」であり、
    /// 「0 だった」ではない。呼び出し側はこの区別に依存して `heap` 欄ごと出さない。
    #[cfg(not(feature = "heap-trace"))]
    #[test]
    fn snapshot_is_absent_without_the_feature() {
        assert_eq!(snapshot(), None);
    }
}
