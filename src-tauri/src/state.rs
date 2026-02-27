use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use snotra_core::engine::Engine;

pub struct AppState {
    pub engine: Mutex<Engine>,
    pub indexing: AtomicBool,
    pub index_build_started: AtomicBool,
}
