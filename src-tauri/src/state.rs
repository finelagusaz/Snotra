use std::sync::Mutex;

use snotra_core::config::Config;
use snotra_core::history::HistoryStore;
use snotra_core::search::SearchEngine;

pub struct AppState {
    pub engine: Mutex<SearchEngine>,
    pub history: Mutex<HistoryStore>,
    pub config: Mutex<Config>,
}
