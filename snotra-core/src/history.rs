use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::binfmt::BinFile;
use crate::indexer::normalize_entry_key;
use crate::query::normalize_query;

const HISTORY_MAGIC: [u8; 4] = *b"HIST";
const HISTORY_VERSION: u32 = 3; // postcard (現行, ms timestamp)
const HISTORY_VERSION_POSTCARD_V2: u32 = 2; // postcard (旧, sec timestamp)

/// Fallback chain for loading legacy history formats (all postcard).
const HISTORY_FALLBACKS: &[u32] = &[HISTORY_VERSION_POSTCARD_V2];

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
    dirty_count: u32,
}

impl HistoryStore {
    pub fn load(top_n: usize) -> Self {
        let (loaded_data, loaded_version) = Self::bin_file()
            .and_then(|bf| bf.load_with_fallback(HISTORY_FALLBACKS))
            .unwrap_or((HistoryData::default(), HISTORY_VERSION));

        let data = migrate_time_unit_if_legacy(loaded_version, loaded_data);
        let data = migrate_normalize_keys(data);

        Self {
            data,
            top_n,
            dirty_count: 0,
        }
    }

    pub fn save(&mut self) {
        self.prune();

        if let Some(bf) = Self::bin_file() {
            bf.save(&self.data);
        }
    }

    pub fn record_launch(&mut self, path: &str, query: &str) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let norm_path = normalize_entry_key(path);
        let entry = self.data.global.entry(norm_path.clone()).or_default();
        entry.launch_count = entry.launch_count.saturating_add(1);
        entry.last_launched = now;

        let norm_query = normalize_query(query);
        if !norm_query.is_empty() {
            // パス区切り文字（/ ¥）を含むクエリは \ に統一して履歴キーを正規化する。
            // tool/editor と tool\editor が同じバケットに入るようにする。
            let query_key = if norm_query.contains('/')
                || norm_query.contains('\u{00a5}')
            {
                norm_query.replace(['/', '\u{00a5}'], "\\")
            } else {
                norm_query.into_owned()
            };
            *self
                .data
                .query
                .entry(query_key)
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
        self.data
            .global
            .get(&normalize_entry_key(path))
            .map(|e| e.last_launched)
    }

    pub fn get_global_stats(&self, path: &str) -> (u32, u64) {
        self.data
            .global
            .get(&normalize_entry_key(path))
            .map(|e| (e.launch_count, e.last_launched))
            .unwrap_or((0, 0))
    }

    /// Same as `get_global_stats` but accepts a pre-normalized key.
    pub fn get_global_stats_normalized(&self, normalized_key: &str) -> (u32, u64) {
        self.data
            .global
            .get(normalized_key)
            .map(|e| (e.launch_count, e.last_launched))
            .unwrap_or((0, 0))
    }

    pub fn query_count(&self, query: &str, path: &str) -> u32 {
        let nq = normalize_query(query);
        // record_launch と同じ正規化: パス区切り（/ ¥）を \ に統一
        let norm_query = if nq.contains('/') || nq.contains('\u{00a5}') {
            std::borrow::Cow::Owned(nq.replace(['/', '\u{00a5}'], "\\"))
        } else {
            nq
        };
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

    /// Same as `query_count_normalized` but accepts a pre-normalized path key.
    pub fn query_count_pre_normalized(&self, normalized_query: &str, normalized_key: &str) -> u32 {
        self.data
            .query
            .get(normalized_query)
            .and_then(|m| m.get(normalized_key))
            .copied()
            .unwrap_or(0)
    }

    pub fn recent_launches(&self, max: usize) -> Vec<&str> {
        let mut entries: Vec<_> = self
            .data
            .global
            .iter()
            .map(|(path, entry)| (path.as_str(), entry.last_launched))
            .collect();

        entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        entries.truncate(max);
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

    /// Same as `folder_expansion_count` but accepts a pre-normalized key.
    pub fn folder_expansion_count_normalized(&self, normalized_key: &str) -> u32 {
        self.data
            .folder_expansion
            .get(normalized_key)
            .copied()
            .unwrap_or(0)
    }

    fn bin_file() -> Option<BinFile> {
        BinFile::new(HISTORY_MAGIC, HISTORY_VERSION, "history.bin")
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

fn migrate_time_unit_if_legacy(version: u32, mut data: HistoryData) -> HistoryData {
    if version < HISTORY_VERSION {
        for entry in data.global.values_mut() {
            entry.last_launched = entry.last_launched.saturating_mul(1000);
        }
    }
    data
}

/// デシリアライズ直後に全パスキーを正規化するマイグレーション。
/// 旧バージョンで大文字・小文字が混在したキーを統一し、衝突時は加算/max で統合する。
/// normalize_entry_key / normalize_query は冪等なので、正規化済みデータへの再適用も安全。
fn migrate_normalize_keys(data: HistoryData) -> HistoryData {
    // global: キー正規化。衝突は launch_count 加算、last_launched は max
    let mut new_global: FxHashMap<String, GlobalEntry> = FxHashMap::default();
    for (path, entry) in data.global {
        let norm = normalize_entry_key(&path);
        let e = new_global.entry(norm).or_default();
        e.launch_count = e.launch_count.saturating_add(entry.launch_count);
        e.last_launched = e.last_launched.max(entry.last_launched);
    }

    // query: outer キー（クエリ）も normalize_query で再正規化（アクセント折りたたみ統一）
    // + パス区切り（/ ¥）を \ に統一。inner キー（パス）は normalize_entry_key で正規化。
    // 衝突時はカウント加算。
    let mut new_query: FxHashMap<String, FxHashMap<String, u32>> = FxHashMap::default();
    for (q_key, app_map) in data.query {
        let mut norm_q = normalize_query(&q_key).into_owned();
        if norm_q.contains('/') || norm_q.contains('\u{00a5}') {
            norm_q = norm_q.replace(['/', '\u{00a5}'], "\\");
        }
        let new_app_map = new_query.entry(norm_q).or_default();
        for (path, count) in app_map {
            let norm = normalize_entry_key(&path);
            *new_app_map.entry(norm).or_insert(0) += count;
        }
    }

    // folder_expansion: キー正規化。衝突は加算
    let mut new_folder: FxHashMap<String, u32> = FxHashMap::default();
    for (path, count) in data.folder_expansion {
        let norm = normalize_entry_key(&path);
        *new_folder.entry(norm).or_insert(0) += count;
    }

    HistoryData {
        global: new_global,
        query: new_query,
        folder_expansion: new_folder,
    }
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;
    use crate::binfmt::{deserialize_with_header, serialize_with_header};
    use crate::query::normalize_query;
    use std::path::Path;

    /// Helper: create a BinFile pointing to a specific directory (bypasses Config::config_dir)
    fn bin_file_in(dir: &Path) -> BinFile {
        BinFile {
            magic: HISTORY_MAGIC,
            version: HISTORY_VERSION,
            path: dir.join("history.bin"),
        }
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("snotra_hist_test_{}", tag));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn fresh_store() -> HistoryStore {
        HistoryStore {
            data: HistoryData::default(),
            top_n: 100,
            dirty_count: 0,
        }
    }

    fn fresh_store_with_top_n(top_n: usize) -> HistoryStore {
        HistoryStore {
            data: HistoryData::default(),
            top_n,
            dirty_count: 0,
        }
    }

    #[test]
    fn record_launch_increments_global_count() {
        let mut store = fresh_store();
        let path = "C:\\fake\\app.lnk";
        let key = normalize_entry_key(path);
        assert_eq!(store.global_count(path), 0);
        store
            .data
            .global
            .entry(key.clone())
            .or_default()
            .launch_count += 1;
        assert_eq!(store.global_count(path), 1);
        store
            .data
            .global
            .entry(key)
            .or_default()
            .launch_count += 1;
        assert_eq!(store.global_count(path), 2);
    }

    #[test]
    fn record_launch_tracks_query_count() {
        let mut store = fresh_store();
        let path = "C:\\fake\\notepad.lnk";
        let query = "note";
        let path_key = normalize_entry_key(path);
        let query_key = normalize_query(query).into_owned();

        // Simulate record_launch logic without save()
        store
            .data
            .global
            .entry(path_key.clone())
            .or_default()
            .launch_count += 1;
        *store
            .data
            .query
            .entry(query_key.clone())
            .or_default()
            .entry(path_key.clone())
            .or_insert(0) += 1;

        assert_eq!(store.query_count(query, path), 1);

        *store
            .data
            .query
            .entry(query_key)
            .or_default()
            .entry(path_key)
            .or_insert(0) += 1;
        assert_eq!(store.query_count(query, path), 2);
    }

    #[test]
    fn query_count_normalized_to_lowercase() {
        let mut store = fresh_store();
        let path = "C:\\fake\\vs.lnk";
        let path_key = normalize_entry_key(path);
        let norm = "vs";
        *store
            .data
            .query
            .entry(norm.to_string())
            .or_default()
            .entry(path_key)
            .or_insert(0) += 1;

        assert_eq!(store.query_count("vs", path), 1);
        assert_eq!(store.query_count("VS", path), 1);
    }

    #[test]
    fn query_count_normalizes_whitespace() {
        let mut store = fresh_store();
        let path = "C:\\fake\\app.lnk";
        let path_key = normalize_entry_key(path);
        let key = normalize_query("foo bar");
        *store
            .data
            .query
            .entry(key.into_owned())
            .or_default()
            .entry(path_key)
            .or_insert(0) += 1;

        assert_eq!(store.query_count("  foo   bar  ", path), 1);
    }

    #[test]
    fn empty_query_not_tracked_in_query_map() {
        let mut store = fresh_store();
        let path = "C:\\fake\\app.lnk";
        let path_key = normalize_entry_key(path);

        // Simulate record_launch with empty query
        store
            .data
            .global
            .entry(path_key.clone())
            .or_default()
            .launch_count += 1;
        let norm_query = "".trim().to_lowercase();
        if !norm_query.is_empty() {
            *store
                .data
                .query
                .entry(norm_query)
                .or_default()
                .entry(path_key)
                .or_insert(0) += 1;
        }

        assert_eq!(store.global_count(path), 1);
        assert_eq!(store.query_count("", path), 0);
    }

    #[test]
    fn record_folder_expansion_increments_count() {
        let mut store = fresh_store();
        let folder = "C:\\Projects";
        let folder_key = normalize_entry_key(folder);
        assert_eq!(store.folder_expansion_count(folder), 0);
        *store
            .data
            .folder_expansion
            .entry(folder_key)
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

        let recent = store.recent_launches(8);
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

        let recent = store.recent_launches(8);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0], "C:\\alpha.lnk");
        assert_eq!(recent[1], "C:\\zeta.lnk");
    }

    #[test]
    fn postcard_v2_seconds_migrates_to_milliseconds() {
        let dir = temp_dir("v2_migrate");
        let mut data = HistoryData::default();
        data.global.insert(
            "C:\\v2.lnk".to_string(),
            GlobalEntry {
                launch_count: 2,
                last_launched: 1_700_000_000,
            },
        );
        let bytes = serialize_with_header(HISTORY_MAGIC, HISTORY_VERSION_POSTCARD_V2, &data)
            .expect("serialize v2 postcard");
        let bf = bin_file_in(&dir);
        std::fs::write(bf.path(), &bytes).unwrap();
        let (loaded, version): (HistoryData, u32) =
            bf.load_with_fallback(HISTORY_FALLBACKS).expect("load v2 postcard");
        assert_eq!(version, HISTORY_VERSION_POSTCARD_V2);
        let migrated = migrate_time_unit_if_legacy(version, loaded);
        assert_eq!(
            migrated.global["C:\\v2.lnk"].last_launched,
            1_700_000_000_000
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn postcard_v3_not_reconverted() {
        let dir = temp_dir("v3_no_reconvert");
        let mut data = HistoryData::default();
        data.global.insert(
            "C:\\v3.lnk".to_string(),
            GlobalEntry {
                launch_count: 2,
                last_launched: 1_700_000_000_000,
            },
        );
        let bf = bin_file_in(&dir);
        assert!(bf.save(&data));
        let (loaded, version): (HistoryData, u32) =
            bf.load_with_fallback(HISTORY_FALLBACKS).expect("load v3 postcard");
        assert_eq!(version, HISTORY_VERSION);
        let migrated = migrate_time_unit_if_legacy(version, loaded);
        assert_eq!(
            migrated.global["C:\\v3.lnk"].last_launched,
            1_700_000_000_000
        );

        let _ = std::fs::remove_dir_all(&dir);
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

    // --- migrate_normalize_keys テスト ---

    #[test]
    fn migrate_normalize_keys_lowercases_global_key() {
        let mut data = HistoryData::default();
        data.global.insert(
            "C:\\FAKE\\APP.LNK".to_string(),
            GlobalEntry {
                launch_count: 3,
                last_launched: 1000,
            },
        );
        let migrated = migrate_normalize_keys(data);
        assert!(migrated.global.contains_key("c:\\fake\\app.lnk"));
        assert!(!migrated.global.contains_key("C:\\FAKE\\APP.LNK"));
        assert_eq!(migrated.global["c:\\fake\\app.lnk"].launch_count, 3);
    }

    #[test]
    fn migrate_normalize_keys_merges_collisions() {
        let mut data = HistoryData::default();
        data.global.insert(
            "C:\\FAKE\\APP.LNK".to_string(),
            GlobalEntry {
                launch_count: 3,
                last_launched: 2000,
            },
        );
        data.global.insert(
            "c:\\fake\\app.lnk".to_string(),
            GlobalEntry {
                launch_count: 5,
                last_launched: 1000,
            },
        );
        let migrated = migrate_normalize_keys(data);
        assert_eq!(migrated.global.len(), 1);
        let entry = &migrated.global["c:\\fake\\app.lnk"];
        assert_eq!(entry.launch_count, 8);
        assert_eq!(entry.last_launched, 2000);
    }

    #[test]
    fn migrate_normalize_keys_is_idempotent() {
        let mut data = HistoryData::default();
        data.global.insert(
            "c:\\fake\\app.lnk".to_string(),
            GlobalEntry {
                launch_count: 5,
                last_launched: 1000,
            },
        );
        data.query
            .entry("app".to_string())
            .or_default()
            .insert("c:\\fake\\app.lnk".to_string(), 2);
        let once = migrate_normalize_keys(data.clone());
        let twice = migrate_normalize_keys(once.clone());
        assert_eq!(once.global["c:\\fake\\app.lnk"].launch_count, 5);
        assert_eq!(twice.global["c:\\fake\\app.lnk"].launch_count, 5);
        assert_eq!(once.query["app"]["c:\\fake\\app.lnk"], 2);
        assert_eq!(twice.query["app"]["c:\\fake\\app.lnk"], 2);
    }

    #[test]
    fn migrate_normalize_keys_normalizes_query_inner_map() {
        let mut data = HistoryData::default();
        let inner = data.query.entry("app".to_string()).or_default();
        inner.insert("C:\\FAKE\\APP.LNK".to_string(), 3);
        inner.insert("c:\\fake\\app.lnk".to_string(), 2);
        let migrated = migrate_normalize_keys(data);
        let inner = &migrated.query["app"];
        assert_eq!(inner.len(), 1);
        assert_eq!(inner["c:\\fake\\app.lnk"], 5);
    }

    #[test]
    fn migrate_normalize_keys_folds_accented_query_outer_key() {
        let mut data = HistoryData::default();
        // "résumé" と "resume" は同じバケットに統合される
        data.query
            .entry("résumé".to_string())
            .or_default()
            .insert("c:\\fake\\app.lnk".to_string(), 3);
        data.query
            .entry("resume".to_string())
            .or_default()
            .insert("c:\\fake\\app.lnk".to_string(), 2);
        let migrated = migrate_normalize_keys(data);
        // 両方とも "resume" に正規化されて統合
        assert_eq!(migrated.query.len(), 1);
        assert!(migrated.query.contains_key("resume"));
        assert_eq!(migrated.query["resume"]["c:\\fake\\app.lnk"], 5);
    }

    #[test]
    fn migrate_normalize_keys_accent_idempotent() {
        let mut data = HistoryData::default();
        data.query
            .entry("cafe".to_string())
            .or_default()
            .insert("c:\\fake\\app.lnk".to_string(), 4);
        let once = migrate_normalize_keys(data.clone());
        let twice = migrate_normalize_keys(once.clone());
        assert_eq!(once.query["cafe"]["c:\\fake\\app.lnk"], 4);
        assert_eq!(twice.query["cafe"]["c:\\fake\\app.lnk"], 4);
    }

    #[test]
    fn migrate_normalize_keys_unifies_path_separators_in_query() {
        // 既存の tool/editor と tool\editor の履歴バケットが統合される
        let mut data = HistoryData::default();
        data.query
            .entry("tool/editor".to_string())
            .or_default()
            .insert("c:\\tool\\editor\\app.exe".to_string(), 3);
        data.query
            .entry("tool\\editor".to_string())
            .or_default()
            .insert("c:\\tool\\editor\\app.exe".to_string(), 2);
        let migrated = migrate_normalize_keys(data);
        assert_eq!(migrated.query.len(), 1);
        assert!(migrated.query.contains_key("tool\\editor"));
        assert_eq!(migrated.query["tool\\editor"]["c:\\tool\\editor\\app.exe"], 5);
    }

    #[test]
    fn migrate_normalize_keys_unifies_yen_sign_in_query() {
        // ¥（U+00A5）も \ に統一される
        let mut data = HistoryData::default();
        data.query
            .entry("tool\u{00a5}editor".to_string())
            .or_default()
            .insert("c:\\tool\\editor\\app.exe".to_string(), 4);
        let migrated = migrate_normalize_keys(data);
        assert!(migrated.query.contains_key("tool\\editor"));
        assert_eq!(migrated.query["tool\\editor"]["c:\\tool\\editor\\app.exe"], 4);
    }

    #[test]
    fn migrate_normalize_keys_path_separator_idempotent() {
        let mut data = HistoryData::default();
        data.query
            .entry("tool\\editor".to_string())
            .or_default()
            .insert("c:\\tool\\editor\\app.exe".to_string(), 3);
        let once = migrate_normalize_keys(data.clone());
        let twice = migrate_normalize_keys(once.clone());
        assert_eq!(once.query["tool\\editor"]["c:\\tool\\editor\\app.exe"], 3);
        assert_eq!(twice.query["tool\\editor"]["c:\\tool\\editor\\app.exe"], 3);
    }
}
