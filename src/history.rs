use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::Config;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalEntry {
    pub launch_count: u32,
    pub last_launched: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HistoryData {
    pub global: HashMap<String, GlobalEntry>,
    pub query: HashMap<String, HashMap<String, u32>>,
}

pub struct HistoryStore {
    data: HistoryData,
    top_n: usize,
    max_history_display: usize,
}

impl HistoryStore {
    pub fn load(top_n: usize, max_history_display: usize) -> Self {
        let data = Self::data_path()
            .and_then(|path| fs::read(&path).ok())
            .and_then(|bytes| bincode::deserialize(&bytes).ok())
            .unwrap_or_default();

        Self {
            data,
            top_n,
            max_history_display,
        }
    }

    pub fn save(&mut self) {
        self.prune();

        let Some(path) = Self::data_path() else {
            return;
        };

        let Ok(bytes) = bincode::serialize(&self.data) else {
            return;
        };

        // Write to temp file then rename for atomicity
        let tmp_path = path.with_extension("bin.tmp");
        if fs::write(&tmp_path, &bytes).is_ok() {
            let _ = fs::rename(&tmp_path, &path);
        }
    }

    pub fn record_launch(&mut self, path: &str, query: &str) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let entry = self.data.global.entry(path.to_string()).or_default();
        entry.launch_count = entry.launch_count.saturating_add(1);
        entry.last_launched = now;

        let norm_query = query.trim().to_lowercase();
        if !norm_query.is_empty() {
            *self
                .data
                .query
                .entry(norm_query)
                .or_default()
                .entry(path.to_string())
                .or_insert(0) += 1;
        }

        self.save();
    }

    pub fn global_count(&self, path: &str) -> u32 {
        self.data
            .global
            .get(path)
            .map(|e| e.launch_count)
            .unwrap_or(0)
    }

    pub fn query_count(&self, query: &str, path: &str) -> u32 {
        let norm_query = query.trim().to_lowercase();
        self.data
            .query
            .get(&norm_query)
            .and_then(|m| m.get(path))
            .copied()
            .unwrap_or(0)
    }

    pub fn recent_launches(&self) -> Vec<&str> {
        let mut entries: Vec<_> = self
            .data
            .global
            .iter()
            .map(|(path, entry)| (path.as_str(), entry.last_launched))
            .collect();

        entries.sort_by(|a, b| b.1.cmp(&a.1));
        entries.truncate(self.max_history_display);
        entries.into_iter().map(|(path, _)| path).collect()
    }

    fn data_path() -> Option<PathBuf> {
        Config::config_dir().map(|p| p.join("history.bin"))
    }

    fn prune(&mut self) {
        if self.data.global.len() <= self.top_n {
            return;
        }

        // Sort by launch_count descending, keep top_n
        let mut entries: Vec<_> = self.data.global.drain().collect();
        entries.sort_by(|a, b| b.1.launch_count.cmp(&a.1.launch_count));
        entries.truncate(self.top_n);

        let surviving: HashMap<String, GlobalEntry> = entries.into_iter().collect();

        // Remove query entries for apps not in top_n
        self.data.query.retain(|_, app_map| {
            app_map.retain(|path, _| surviving.contains_key(path));
            !app_map.is_empty()
        });

        self.data.global = surviving;
    }
}
