use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::binfmt::{deserialize_bincode_with_header, deserialize_with_header, serialize_with_header};
use crate::config::Config;
use crate::indexer::normalize_entry_key;
use crate::query::normalize_query;

const HISTORY_MAGIC: [u8; 4] = *b"HIST";
const HISTORY_VERSION: u32 = 2;        // postcard (現行)
const HISTORY_VERSION_LEGACY: u32 = 1; // bincode (旧)

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalEntry {
    pub launch_count: u32,
    pub last_launched: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HistoryData {
    pub global: FxHashMap<String, GlobalEntry>,
    pub query: FxHashMap<String, FxHashMap<String, u32>>,
    #[serde(default)]
    pub folder_expansion: FxHashMap<String, u32>,
}

pub struct HistoryStore {
    data: HistoryData,
    top_n: usize,
    max_history_display: usize,
    dirty_count: u32,
}

impl HistoryStore {
    pub fn load(top_n: usize, max_history_display: usize) -> Self {
        let data = if let Some(path) = Self::data_path() {
            if let Ok(bytes) = fs::read(&path) {
                // Try current format (postcard V2) first
                deserialize_with_header(&bytes, HISTORY_MAGIC, HISTORY_VERSION)
                    // Fall back to legacy format (bincode V1)
                    .or_else(|| {
                        deserialize_bincode_with_header(
                            &bytes,
                            HISTORY_MAGIC,
                            HISTORY_VERSION_LEGACY,
                        )
                    })
                    .unwrap_or_default()
            } else {
                HistoryData::default()
            }
        } else {
            HistoryData::default()
        };

        Self {
            data,
            top_n,
            max_history_display,
            dirty_count: 0,
        }
    }

    pub fn save(&mut self) {
        self.prune();

        let Some(path) = Self::data_path() else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }

        let Some(bytes) = serialize_with_header(HISTORY_MAGIC, HISTORY_VERSION, &self.data) else {
            return;
        };

        // Write to temp file then rename for atomicity
        let tmp_path = path.with_extension("bin.tmp");
        if fs::write(&tmp_path, &bytes).is_ok() {
            let _ = fs::remove_file(&path);
            let _ = fs::rename(&tmp_path, &path);
        }
    }

    pub fn record_launch(&mut self, path: &str, query: &str) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let norm_path = normalize_entry_key(path);
        let entry = self.data.global.entry(norm_path.clone()).or_default();
        entry.launch_count = entry.launch_count.saturating_add(1);
        entry.last_launched = now;

        let norm_query = normalize_query(query);
        if !norm_query.is_empty() {
            *self
                .data
                .query
                .entry(norm_query.into_owned())
                .or_default()
                .entry(norm_path)
                .or_insert(0) += 1;
        }

        self.dirty_count += 1;
    }

    /// Save if dirty_count has reached the given threshold, then reset.
    pub fn save_if_dirty(&mut self, threshold: u32) {
        if self.dirty_count >= threshold {
            self.save();
            self.dirty_count = 0;
        }
    }

    pub fn global_count(&self, path: &str) -> u32 {
        self.data
            .global
            .get(&normalize_entry_key(path))
            .map(|e| e.launch_count)
            .unwrap_or(0)
    }

    pub fn last_launched(&self, path: &str) -> Option<u64> {
        self.data.global.get(&normalize_entry_key(path)).map(|e| e.last_launched)
    }

    pub fn get_global_stats(&self, path: &str) -> (u32, u64) {
        self.data
            .global
            .get(&normalize_entry_key(path))
            .map(|e| (e.launch_count, e.last_launched))
            .unwrap_or((0, 0))
    }

    pub fn query_count(&self, query: &str, path: &str) -> u32 {
        let norm_query = normalize_query(query);
        self.query_count_normalized(&norm_query, path)
    }

    pub fn query_count_normalized(&self, normalized_query: &str, path: &str) -> u32 {
        self.data
            .query
            .get(normalized_query)
            .and_then(|m| m.get(&normalize_entry_key(path)))
            .copied()
            .unwrap_or(0)
    }

    pub fn get_query_stats(&self, normalized_query: &str) -> Option<&FxHashMap<String, u32>> {
        self.data.query.get(normalized_query)
    }

    pub fn recent_launches(&self) -> Vec<&str> {
        let mut entries: Vec<_> = self
            .data
            .global
            .iter()
            .map(|(path, entry)| (path.as_str(), entry.last_launched))
            .collect();

        entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        entries.truncate(self.max_history_display);
        entries.into_iter().map(|(path, _)| path).collect()
    }

    pub fn record_folder_expansion(&mut self, folder_path: &str) {
        *self
            .data
            .folder_expansion
            .entry(normalize_entry_key(folder_path))
            .or_insert(0) += 1;
        self.dirty_count += 1;
    }

    pub fn folder_expansion_count(&self, folder_path: &str) -> u32 {
        self.data
            .folder_expansion
            .get(&normalize_entry_key(folder_path))
            .copied()
            .unwrap_or(0)
    }

    fn data_path() -> Option<PathBuf> {
        Config::config_dir().map(|p| p.join("history.bin"))
    }

    fn prune(&mut self) {
        // Prune global + query entries
        if self.data.global.len() > self.top_n {
            let mut entries: Vec<_> = self.data.global.drain().collect();
            entries.sort_by(|a, b| b.1.launch_count.cmp(&a.1.launch_count));
            entries.truncate(self.top_n);

            let surviving: FxHashMap<String, GlobalEntry> = entries.into_iter().collect();

            self.data.query.retain(|_, app_map| {
                app_map.retain(|path, _| surviving.contains_key(path));
                !app_map.is_empty()
            });

            self.data.global = surviving;
        }

        // Prune folder_expansion independently
        if self.data.folder_expansion.len() > self.top_n {
            let mut fentries: Vec<_> = self.data.folder_expansion.drain().collect();
            fentries.sort_by(|a, b| b.1.cmp(&a.1));
            fentries.truncate(self.top_n);
            self.data.folder_expansion = fentries.into_iter().collect();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binfmt::serialize_bincode_with_header;
    use crate::query::normalize_query;

    fn fresh_store() -> HistoryStore {
        HistoryStore {
            data: HistoryData::default(),
            top_n: 100,
            max_history_display: 8,
            dirty_count: 0,
        }
    }

    fn fresh_store_with_top_n(top_n: usize) -> HistoryStore {
        HistoryStore {
            data: HistoryData::default(),
            top_n,
            max_history_display: 8,
            dirty_count: 0,
        }
    }

    #[test]
    fn record_launch_increments_global_count() {
        let mut store = fresh_store();
        let path = "C:\\fake\\app.lnk";
        assert_eq!(store.global_count(path), 0);
        store
            .data
            .global
            .entry(path.to_string())
            .or_default()
            .launch_count += 1;
        assert_eq!(store.global_count(path), 1);
        store
            .data
            .global
            .entry(path.to_string())
            .or_default()
            .launch_count += 1;
        assert_eq!(store.global_count(path), 2);
    }

    #[test]
    fn record_launch_tracks_query_count() {
        let mut store = fresh_store();
        let path = "C:\\fake\\notepad.lnk";
        let query = "note";

        // Simulate record_launch logic without save()
        store
            .data
            .global
            .entry(path.to_string())
            .or_default()
            .launch_count += 1;
        *store
            .data
            .query
            .entry(query.to_string())
            .or_default()
            .entry(path.to_string())
            .or_insert(0) += 1;

        assert_eq!(store.query_count(query, path), 1);

        *store
            .data
            .query
            .entry(query.to_string())
            .or_default()
            .entry(path.to_string())
            .or_insert(0) += 1;
        assert_eq!(store.query_count(query, path), 2);
    }

    #[test]
    fn query_count_normalized_to_lowercase() {
        let mut store = fresh_store();
        let path = "C:\\fake\\vs.lnk";
        let norm = "vs";
        *store
            .data
            .query
            .entry(norm.to_string())
            .or_default()
            .entry(path.to_string())
            .or_insert(0) += 1;

        assert_eq!(store.query_count("vs", path), 1);
        assert_eq!(store.query_count("VS", path), 1);
    }

    #[test]
    fn query_count_normalizes_whitespace() {
        let mut store = fresh_store();
        let path = "C:\\fake\\app.lnk";
        let key = normalize_query("foo bar");
        *store
            .data
            .query
            .entry(key.into_owned())
            .or_default()
            .entry(path.to_string())
            .or_insert(0) += 1;

        assert_eq!(store.query_count("  foo   bar  ", path), 1);
    }

    #[test]
    fn empty_query_not_tracked_in_query_map() {
        let mut store = fresh_store();
        let path = "C:\\fake\\app.lnk";

        // Simulate record_launch with empty query
        store
            .data
            .global
            .entry(path.to_string())
            .or_default()
            .launch_count += 1;
        let norm_query = "".trim().to_lowercase();
        if !norm_query.is_empty() {
            *store
                .data
                .query
                .entry(norm_query)
                .or_default()
                .entry(path.to_string())
                .or_insert(0) += 1;
        }

        assert_eq!(store.global_count(path), 1);
        assert_eq!(store.query_count("", path), 0);
    }

    #[test]
    fn record_folder_expansion_increments_count() {
        let mut store = fresh_store();
        let folder = "C:\\Projects";
        assert_eq!(store.folder_expansion_count(folder), 0);
        *store
            .data
            .folder_expansion
            .entry(folder.to_string())
            .or_insert(0) += 1;
        assert_eq!(store.folder_expansion_count(folder), 1);
    }

    #[test]
    fn recent_launches_sorted_by_last_launched() {
        let mut store = fresh_store();
        store.data.global.insert(
            "C:\\app_old.lnk".to_string(),
            GlobalEntry {
                launch_count: 1,
                last_launched: 1000,
            },
        );
        store.data.global.insert(
            "C:\\app_new.lnk".to_string(),
            GlobalEntry {
                launch_count: 1,
                last_launched: 2000,
            },
        );

        let recent = store.recent_launches();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0], "C:\\app_new.lnk");
        assert_eq!(recent[1], "C:\\app_old.lnk");
    }

    #[test]
    fn recent_launches_same_timestamp_sorted_by_path() {
        let mut store = fresh_store();
        store.data.global.insert(
            "C:\\zeta.lnk".to_string(),
            GlobalEntry {
                launch_count: 1,
                last_launched: 2000,
            },
        );
        store.data.global.insert(
            "C:\\alpha.lnk".to_string(),
            GlobalEntry {
                launch_count: 1,
                last_launched: 2000,
            },
        );

        let recent = store.recent_launches();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0], "C:\\alpha.lnk");
        assert_eq!(recent[1], "C:\\zeta.lnk");
    }

    #[test]
    fn legacy_bincode_v1_migrates_to_current_format() {
        let mut data = HistoryData::default();
        data.global.insert(
            "C:\\legacy.lnk".to_string(),
            GlobalEntry {
                launch_count: 7,
                last_launched: 1_600_000_000,
            },
        );
        data.query
            .entry("leg".to_string())
            .or_default()
            .insert("C:\\legacy.lnk".to_string(), 4);
        data.folder_expansion
            .insert("C:\\LegacyFolder".to_string(), 3);

        // Simulate a bincode V1 file written before postcard migration
        let bytes = serialize_bincode_with_header(HISTORY_MAGIC, HISTORY_VERSION_LEGACY, &data)
            .expect("serialize bincode v1");

        // Should be unreadable by current postcard V2 deserializer
        let v2_attempt: Option<HistoryData> =
            deserialize_with_header(&bytes, HISTORY_MAGIC, HISTORY_VERSION);
        assert!(v2_attempt.is_none());

        // Should be readable via bincode V1 fallback
        let migrated: HistoryData =
            deserialize_bincode_with_header(&bytes, HISTORY_MAGIC, HISTORY_VERSION_LEGACY)
                .expect("bincode v1 fallback");
        assert_eq!(migrated.global["C:\\legacy.lnk"].launch_count, 7);
        assert_eq!(migrated.query["leg"]["C:\\legacy.lnk"], 4);
        assert_eq!(migrated.folder_expansion["C:\\LegacyFolder"], 3);
    }

    #[test]
    fn binary_roundtrip() {
        let mut data = HistoryData::default();
        data.global.insert(
            "C:\\app.lnk".to_string(),
            GlobalEntry {
                launch_count: 5,
                last_launched: 1_700_000_000,
            },
        );
        data.query
            .entry("notepad".to_string())
            .or_default()
            .insert("C:\\app.lnk".to_string(), 3);
        data.folder_expansion.insert("C:\\Projects".to_string(), 2);

        let bytes =
            serialize_with_header(HISTORY_MAGIC, HISTORY_VERSION, &data).expect("serialize");
        let roundtripped: HistoryData =
            deserialize_with_header(&bytes, HISTORY_MAGIC, HISTORY_VERSION).expect("deserialize");

        assert_eq!(roundtripped.global["C:\\app.lnk"].launch_count, 5);
        assert_eq!(roundtripped.query["notepad"]["C:\\app.lnk"], 3);
        assert_eq!(roundtripped.folder_expansion["C:\\Projects"], 2);
    }

    #[test]
    fn prune_keeps_top_n_by_launch_count() {
        let mut store = fresh_store_with_top_n(2);

        store.data.global.insert(
            "C:\\low.lnk".to_string(),
            GlobalEntry {
                launch_count: 1,
                last_launched: 100,
            },
        );
        store.data.global.insert(
            "C:\\high.lnk".to_string(),
            GlobalEntry {
                launch_count: 10,
                last_launched: 200,
            },
        );
        store.data.global.insert(
            "C:\\med.lnk".to_string(),
            GlobalEntry {
                launch_count: 5,
                last_launched: 150,
            },
        );

        store.prune();

        assert_eq!(store.data.global.len(), 2);
        assert!(store.data.global.contains_key("C:\\high.lnk"));
        assert!(store.data.global.contains_key("C:\\med.lnk"));
        assert!(!store.data.global.contains_key("C:\\low.lnk"));
    }
}
