use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

use snotra_core::config::Config;
use snotra_core::history::HistoryStore;
use snotra_core::search::SearchEngine;

pub struct AppState {
    pub engine: Mutex<SearchEngine>,
    pub history: Mutex<HistoryStore>,
    pub config: Mutex<Config>,
    pub indexing: AtomicBool,
    pub index_build_started: AtomicBool,
}
