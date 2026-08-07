//! 起動履歴・クエリ別履歴・フォルダ展開履歴の管理とバイナリ永続化（`HistoryStore`）。
//!
//! 剪定容量 `top_n` は `HistoryStore` に焼き込まず、保存/参照時に現在 config から受け取る
//! （live-read）——`result_limit` 設定変更を再起動なしで反映するため（#348）。

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::binfmt::{BinFile, try_serialize_with_header};
use crate::config::Config;
use crate::indexer::normalize_entry_key;
use crate::query::normalize_history_query_key;

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
    dirty_count: u32,
}

static NEXT_SAVE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static HISTORY_WRITE_STATE: OnceLock<Mutex<HashMap<PathBuf, u64>>> = OnceLock::new();

/// A fully serialized history snapshot whose filesystem write can run after the
/// caller releases the `Engine` lock.
pub struct PreparedHistorySave {
    file: BinFile,
    bytes: Vec<u8>,
    sequence: u64,
}

impl PreparedHistorySave {
    /// Persist this snapshot without allowing an older, delayed snapshot to
    /// overwrite a newer one for the same history file.
    #[must_use]
    pub fn save(self) -> bool {
        let path = self.file.path().to_path_buf();
        let mut latest_by_path = HISTORY_WRITE_STATE
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .unwrap();
        if latest_by_path
            .get(&path)
            .is_some_and(|latest| *latest > self.sequence)
        {
            return true;
        }
        if !self.file.save_bytes(&self.bytes) {
            eprintln!("[history] failed to save {}", path.display());
            return false;
        }
        latest_by_path.insert(path, self.sequence);
        true
    }
}

impl HistoryStore {
    /// 履歴をディスクから読み込む。`top_n`（剪定容量）は焼き込まず、`save`/`prune` 呼び出し時に
    /// 引数で受け取る（live-read 化、issue #348）。これにより `result_limit` の設定変更が
    /// 再起動なしで反映され、`HistoryStore.top_n` の焼き込みによるドリフトが構造的に発生しない。
    ///
    /// **ユニットテストの fixture にこれを使わないこと**——実 `%APPDATA%\Snotra\history.bin` を
    /// 読むため開発者のマシン状態でテスト結果が変わる。空が欲しいなら [`Self::empty`]、
    /// 特定の内容が欲しいなら [`Self::load_in`] に注入する（理由の全文は `empty` の doc・#963）。
    pub fn load() -> Self {
        match Config::config_dir() {
            Some(dir) => Self::load_in(&dir),
            None => Self::from_loaded(HistoryData::default(), HISTORY_VERSION),
        }
    }

    /// 空の履歴を作る（**テスト fixture 専用**・ファイル I/O を伴わない）。
    ///
    /// **テストで `load()` を fixture に使ってはならない**（#963）。`load()` は
    /// `Config::config_dir()`（= 実 `%APPDATA%\Snotra`）を読むため開発機の実履歴が混入し、
    /// 順序や件数を見る検査の結果が開発者のマシン状態で変わる。しかも CI のランナーには
    /// そのファイルが無く `load()` も空を返すので、**食い違いは CI では緑のまま、
    /// 開発機でだけ現れる**——検出器が構造的に届かない形になる。
    ///
    /// 実履歴を意図して読みたいのは計測ハーネス（`tests/` 配下の `#[ignore]` ベンチ）だけで、
    /// あちらは実運用の姿を測るのが目的ゆえ `load()` のままでよい。
    #[cfg(test)]
    pub(crate) fn empty() -> Self {
        Self::from_loaded(HistoryData::default(), HISTORY_VERSION)
    }

    /// `load()` と同じ読み込みを `dir` 注入で行う（統合テスト用、issue #429）。
    pub fn load_in(dir: &Path) -> Self {
        let (loaded_data, loaded_version) = Self::bin_file_in(dir)
            .load_with_fallback(HISTORY_FALLBACKS)
            .unwrap_or((HistoryData::default(), HISTORY_VERSION));
        Self::from_loaded(loaded_data, loaded_version)
    }

    /// 読み込んだ生データに対し、バージョン別マイグレーションを適用して Self を組み立てる。
    /// `load()` / `load_in()` の共有ロジック。
    fn from_loaded(loaded_data: HistoryData, loaded_version: u32) -> Self {
        let data = migrate_time_unit_if_legacy(loaded_version, loaded_data);
        let data = migrate_normalize_keys(data);
        Self {
            data,
            dirty_count: 0,
        }
    }

    /// 剪定＋永続化を `dir` 注入で行う（統合テスト用、issue #429）。
    /// 本番経路は `prepare_save_if_dirty` / `prepare_flush` を使う。
    pub fn save_in(&mut self, top_n: usize, dir: &Path) {
        if let Some(save) = self.prepare_save_in(top_n, dir) {
            let _ = save.save();
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

        let norm_query = normalize_history_query_key(query);
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

    /// Prepare a thresholded save without performing filesystem I/O.
    pub fn prepare_save_if_dirty(
        &mut self,
        threshold: u32,
        top_n: usize,
    ) -> Option<PreparedHistorySave> {
        if self.dirty_count < threshold {
            return None;
        }
        self.dirty_count = 0;
        match Config::config_dir() {
            Some(dir) => self.prepare_save_in(top_n, &dir),
            None => {
                self.prune(top_n);
                None
            }
        }
    }

    /// Testable directory-injected form of `prepare_save_if_dirty`.
    pub fn prepare_save_if_dirty_in(
        &mut self,
        threshold: u32,
        top_n: usize,
        dir: &Path,
    ) -> Option<PreparedHistorySave> {
        if self.dirty_count < threshold {
            return None;
        }
        self.dirty_count = 0;
        self.prepare_save_in(top_n, dir)
    }

    /// Prepare an unconditional final snapshot for application exit.
    ///
    /// This must not consult `dirty_count`: a threshold save may already have reset the
    /// counter while its detached filesystem write is still pending. Issuing a newer
    /// sequence here makes the exit snapshot authoritative over that in-flight save.
    pub fn prepare_flush(&mut self, top_n: usize) -> Option<PreparedHistorySave> {
        match Config::config_dir() {
            Some(dir) => self.prepare_flush_in(top_n, &dir),
            None => {
                self.prune(top_n);
                self.dirty_count = 0;
                None
            }
        }
    }

    /// Testable directory-injected form of [`Self::prepare_flush`].
    pub fn prepare_flush_in(&mut self, top_n: usize, dir: &Path) -> Option<PreparedHistorySave> {
        let save = self.prepare_save_in(top_n, dir);
        if save.is_some() {
            self.dirty_count = 0;
        }
        save
    }

    fn prepare_save_in(&mut self, top_n: usize, dir: &Path) -> Option<PreparedHistorySave> {
        self.prune(top_n);
        let file = Self::bin_file_in(dir);
        let bytes = match try_serialize_with_header(HISTORY_MAGIC, HISTORY_VERSION, &self.data) {
            Ok(bytes) => bytes,
            Err(_) => {
                // #428: シリアライズ失敗を黙って捨てず surface する
                // （PreparedHistorySave::save の書込失敗 eprintln と対称）。
                eprintln!("[history] failed to serialize {}", file.path().display());
                return None;
            }
        };
        Some(PreparedHistorySave {
            file,
            bytes,
            sequence: NEXT_SAVE_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        })
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
        let norm_query = normalize_history_query_key(query);
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

    fn bin_file_in(dir: &Path) -> BinFile {
        BinFile::new_in(dir, HISTORY_MAGIC, HISTORY_VERSION, "history.bin")
    }

    /// `top_n` 件まで剪定する。`top_n` は引数で受け取る（焼き込まない＝live-read、issue #348）。
    /// 同一 store に対し異なる `top_n` で呼べば、その都度その容量で剪定される。
    fn prune(&mut self, top_n: usize) {
        // Prune global + query entries
        if self.data.global.len() > top_n {
            let mut entries: Vec<_> = self.data.global.drain().collect();
            entries.sort_by_key(|b| std::cmp::Reverse(b.1.launch_count));
            entries.truncate(top_n);

            let surviving: FxHashMap<String, GlobalEntry> = entries.into_iter().collect();

            self.data.query.retain(|_, app_map| {
                app_map.retain(|path, _| surviving.contains_key(path));
                !app_map.is_empty()
            });

            self.data.global = surviving;
        }

        // Prune folder_expansion independently
        if self.data.folder_expansion.len() > top_n {
            let mut fentries: Vec<_> = self.data.folder_expansion.drain().collect();
            fentries.sort_by_key(|b| std::cmp::Reverse(b.1));
            fentries.truncate(top_n);
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

    // query: outer キー（クエリ）は normalize_history_query_key で正規化
    // （normalize_query + パス区切り統一）。inner キー（パス）は normalize_entry_key で正規化。
    // 衝突時はカウント加算。
    let mut new_query: FxHashMap<String, FxHashMap<String, u32>> = FxHashMap::default();
    for (q_key, app_map) in data.query {
        let norm_q = normalize_history_query_key(&q_key).into_owned();
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
mod tests {
    use super::*;
    use crate::binfmt::{try_deserialize_with_header, try_serialize_with_header};
    use crate::query::normalize_query;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("snotra_hist_test_{}", tag));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn fresh_store() -> HistoryStore {
        HistoryStore {
            data: HistoryData::default(),
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
        store.data.global.entry(key).or_default().launch_count += 1;
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
        *store.data.folder_expansion.entry(folder_key).or_insert(0) += 1;
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
        let bytes = try_serialize_with_header(HISTORY_MAGIC, HISTORY_VERSION_POSTCARD_V2, &data)
            .expect("serialize v2 postcard");
        let bf = HistoryStore::bin_file_in(&dir);
        std::fs::write(bf.path(), &bytes).unwrap();
        let (loaded, version): (HistoryData, u32) = bf
            .load_with_fallback(HISTORY_FALLBACKS)
            .expect("load v2 postcard");
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
        let bf = HistoryStore::bin_file_in(&dir);
        assert!(bf.save(&data));
        let (loaded, version): (HistoryData, u32) = bf
            .load_with_fallback(HISTORY_FALLBACKS)
            .expect("load v3 postcard");
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
            try_serialize_with_header(HISTORY_MAGIC, HISTORY_VERSION, &data).expect("serialize");
        let roundtripped: HistoryData =
            try_deserialize_with_header(&bytes, HISTORY_MAGIC, HISTORY_VERSION)
                .expect("deserialize");

        assert_eq!(roundtripped.global["C:\\app.lnk"].launch_count, 5);
        assert_eq!(roundtripped.query["notepad"]["C:\\app.lnk"], 3);
        assert_eq!(roundtripped.folder_expansion["C:\\Projects"], 2);
    }

    #[test]
    fn prune_keeps_top_n_by_launch_count() {
        let mut store = fresh_store();

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

        store.prune(2);

        assert_eq!(store.data.global.len(), 2);
        assert!(store.data.global.contains_key("C:\\high.lnk"));
        assert!(store.data.global.contains_key("C:\\med.lnk"));
        assert!(!store.data.global.contains_key("C:\\low.lnk"));
    }

    #[test]
    fn prune_uses_passed_top_n_each_call_not_frozen() {
        // issue #348-B の核心: top_n は prune 呼び出しごとに引数で渡る（焼き込まない）。
        // 同一 store に異なる top_n で prune すれば、その都度その容量で剪定される
        // ＝ HistoryStore が top_n を保持しないので「設定変更が反映されない」ドリフトは起きない。
        let mut store = fresh_store();
        for (path, count) in [("a", 10u32), ("b", 5), ("c", 1)] {
            store.data.global.insert(
                format!("c:\\{}.lnk", path),
                GlobalEntry {
                    launch_count: count,
                    last_launched: u64::from(count),
                },
            );
        }

        // top_n=2 で剪定 → 上位 2 件（a, b）が残る
        store.prune(2);
        assert_eq!(store.data.global.len(), 2);
        assert!(store.data.global.contains_key("c:\\a.lnk"));
        assert!(store.data.global.contains_key("c:\\b.lnk"));

        // 同じ store に、より小さい top_n=1 で再剪定 → さらに絞れる（焼き込みでない証拠）
        store.prune(1);
        assert_eq!(store.data.global.len(), 1);
        assert!(store.data.global.contains_key("c:\\a.lnk"));
    }

    #[test]
    fn prune_grow_top_n_keeps_more_entries() {
        // #348-B（拡大方向）: 大きい top_n では剪定されない（深さが増える）。
        let mut store = fresh_store();
        for i in 0..15u32 {
            store.data.global.insert(
                format!("c:\\e{}.lnk", i),
                GlobalEntry {
                    launch_count: 15 - i,
                    last_launched: u64::from(15 - i),
                },
            );
        }
        // top_n=200 >= 15 → 全件残る
        store.prune(200);
        assert_eq!(store.data.global.len(), 15);
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
        assert_eq!(
            migrated.query["tool\\editor"]["c:\\tool\\editor\\app.exe"],
            5
        );
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
        assert_eq!(
            migrated.query["tool\\editor"]["c:\\tool\\editor\\app.exe"],
            4
        );
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

    // --- save_in / load_in ラウンドトリップ（issue #429, dir 注入で統合テスト可能に）---

    #[test]
    fn save_in_load_in_roundtrip_preserves_data() {
        let dir = temp_dir("save_load_roundtrip");
        let mut store = fresh_store();
        store.record_launch("C:\\fake\\app.lnk", "app query");
        store.record_launch("C:\\fake\\app.lnk", "app query");
        store.record_folder_expansion("C:\\Projects");

        store.save_in(100, &dir);

        let loaded = HistoryStore::load_in(&dir);
        assert_eq!(loaded.global_count("C:\\fake\\app.lnk"), 2);
        assert_eq!(loaded.query_count("app query", "C:\\fake\\app.lnk"), 2);
        assert_eq!(loaded.folder_expansion_count("C:\\Projects"), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_in_prunes_before_persisting_and_load_in_reflects_pruned_data() {
        // save_in は prune → 永続化の順（HistoryStore::save_in 実装どおり）。
        // top_n=2 で保存した内容を dir から読み直し、低カウントの entry が
        // ディスク上でも剪定されていること（インメモリだけの剪定ではないこと）を検証する。
        let dir = temp_dir("save_prune_roundtrip");
        let mut store = fresh_store();
        for (name, count) in [("a", 10u32), ("b", 5), ("c", 1)] {
            let path = format!("c:\\{}.lnk", name);
            for _ in 0..count {
                store.record_launch(&path, "");
            }
        }

        store.save_in(2, &dir);

        let loaded = HistoryStore::load_in(&dir);
        assert_eq!(loaded.global_count("c:\\a.lnk"), 10);
        assert_eq!(loaded.global_count("c:\\b.lnk"), 5);
        assert_eq!(
            loaded.global_count("c:\\c.lnk"),
            0,
            "low-count entry should be pruned before persisting to disk"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prepare_save_if_dirty_in_defers_file_io_until_job_runs() {
        let dir = temp_dir("prepare_save_defers_io");
        let mut store = fresh_store();
        store.record_launch("C:\\fake\\app.lnk", "app");

        let save = store
            .prepare_save_if_dirty_in(1, 100, &dir)
            .expect("threshold reached");

        assert!(
            !dir.join("history.bin").exists(),
            "preparing under the Engine lock must not perform file I/O"
        );

        assert!(save.save());
        let loaded = HistoryStore::load_in(&dir);
        assert_eq!(loaded.global_count("C:\\fake\\app.lnk"), 1);
        assert_eq!(loaded.query_count("app", "C:\\fake\\app.lnk"), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prepare_save_if_dirty_in_returns_none_below_threshold() {
        let dir = temp_dir("prepare_save_below_threshold");
        let mut store = fresh_store();
        store.record_launch("C:\\fake\\app.lnk", "");

        assert!(store.prepare_save_if_dirty_in(2, 100, &dir).is_none());
        assert!(!dir.join("history.bin").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delayed_older_prepared_save_cannot_overwrite_newer_snapshot() {
        let dir = temp_dir("prepared_save_order");
        let mut store = fresh_store();
        store.record_launch("C:\\fake\\app.lnk", "");
        let older = store
            .prepare_save_if_dirty_in(1, 100, &dir)
            .expect("first snapshot");

        store.record_launch("C:\\fake\\app.lnk", "");
        let newer = store
            .prepare_save_if_dirty_in(1, 100, &dir)
            .expect("second snapshot");

        assert!(newer.save());
        assert!(older.save());

        let loaded = HistoryStore::load_in(&dir);
        assert_eq!(loaded.global_count("C:\\fake\\app.lnk"), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn exit_flush_prepares_latest_snapshot_when_threshold_save_is_in_flight() {
        let dir = temp_dir("exit_flush_with_in_flight_save");
        let mut store = fresh_store();
        store.record_launch("C:\\fake\\app.lnk", "");
        let older = store
            .prepare_save_if_dirty_in(1, 100, &dir)
            .expect("threshold save should be prepared");

        let exit_save = store
            .prepare_flush_in(100, &dir)
            .expect("exit must prepare a snapshot even after dirty_count was reset");

        assert!(exit_save.save());
        assert!(older.save());

        let loaded = HistoryStore::load_in(&dir);
        assert_eq!(loaded.global_count("C:\\fake\\app.lnk"), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
