//! 検索を実行する worker（#1004）。**プロセス寿命の 1 本**であり、要求は最新だけを走らせる。
//!
//! **都度 spawn を採らない理由**は `spawn_folder_load` の doc と対である——あちらの per-nav spawn は dead UNC の hang を隔離するための選択で、`engine.search` には転移しない（hang しない代わりに必ず共有 Mutex を要求する）。打鍵ごとに spawn すると、捨てるとわかっている結果のために lock と CPU を払う。
//!
//! **`egui::Context` を持たない**——長寿命 worker が Context clone を握ると `RepaintScheduler` の Arc が窓の `Destroyed` を越えて生き、停止を妨げる（#671 PR D）。起床は `wake_main` を使う。

use std::sync::mpsc::{Receiver, Sender, channel};

use snotra_core::ui_types::SearchResult;
use tauri::Manager;

pub struct SearchRequest {
    pub seq: u64,
    pub query: String,
}

pub enum SearchMsg {
    Done {
        seq: u64,
        results: Vec<SearchResult>,
        /// H6 のゲート材料である。**engine lock を握っている区間で取る**（lock を増やさない）。
        index_entries: usize,
    },
}

/// 溜まった要求から最後の 1 つを選ぶ（最新クエリ勝ち）。
pub fn coalesce(first: SearchRequest, rest: impl Iterator<Item = SearchRequest>) -> SearchRequest {
    rest.fold(first, |_, next| next)
}

/// worker を 1 本立てる。`Sender` が drop されると `recv` が Err を返しループが終わる（join はしない・best-effort）。
pub fn spawn_search_worker(app: tauri::AppHandle) -> (Sender<SearchRequest>, Receiver<SearchMsg>) {
    let (req_tx, req_rx) = channel::<SearchRequest>();
    let (msg_tx, msg_rx) = channel::<SearchMsg>();
    std::thread::spawn(move || {
        while let Ok(first) = req_rx.recv() {
            // recv で 1 つ取った後、溜まっている分を吸って最後だけ採用する。
            let picked = coalesce(first, req_rx.try_iter());
            let Some(state) = app.try_state::<crate::AppState>() else {
                return;
            };
            let (results, index_entries) = {
                let mut engine = state.engine.lock().unwrap();
                let n = engine.entry_count();
                (engine.search(&picked.query), n)
            }; // lock 解放
            if msg_tx
                .send(SearchMsg::Done {
                    seq: picked.seq,
                    results,
                    index_entries,
                })
                .is_err()
            {
                return; // 受け手が消えた（プロセス終了）
            }
            crate::egui_shell::wake_main(&app);
        }
    });
    (req_tx, msg_rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(seq: u64, q: &str) -> SearchRequest {
        SearchRequest {
            seq,
            query: q.to_string(),
        }
    }

    #[test]
    fn coalesce_keeps_only_the_last_request() {
        let picked = coalesce(req(1, "c"), vec![req(2, "c:"), req(3, "c:\\")].into_iter());
        assert_eq!(picked.seq, 3, "溜まった要求は最後だけ走らせる");
        assert_eq!(picked.query, "c:\\");
    }

    #[test]
    fn coalesce_of_single_request_is_itself() {
        let picked = coalesce(req(7, "abc"), std::iter::empty());
        assert_eq!(picked.seq, 7);
    }
}
