//! ウィンドウ位置（`window.bin`）の保存/復元。
//!
//! 座標はモニター相対で永続化する。座標系のセマンティクスを変える（絶対→相対等）ときは
//! `binfmt` のヘッダ version バンプが必須——旧データを新解釈で読むと位置がずれるため。

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::binfmt::BinFile;
use crate::config::Config;

const WINDOW_MAGIC: [u8; 4] = *b"WNDW";
const WINDOW_VERSION_V4: u32 = 4; // postcard, 絶対座標（旧）
const WINDOW_VERSION_V5: u32 = 5; // postcard, search: モニター相対物理座標 / settings: 絶対座標

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct WindowPlacement {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
struct WindowPlacementState {
    search: Option<WindowPlacement>,
    settings: Option<WindowPlacement>,
    settings_size: Option<WindowSize>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
struct WindowSize {
    width: i32,
    height: i32,
}

pub fn load_search_placement() -> Option<WindowPlacement> {
    load_state_v5().and_then(|state| state.search)
}

pub fn save_search_placement(placement: WindowPlacement) {
    let mut state = load_state_v5().unwrap_or_default();
    state.search = Some(placement);
    save_state(&state);
}

pub fn load_settings_placement() -> Option<WindowPlacement> {
    load_state_v5().and_then(|state| state.settings)
}

pub fn save_settings_placement(placement: WindowPlacement) {
    let mut state = load_state_v5().unwrap_or_default();
    state.settings = Some(placement);
    save_state(&state);
}

fn bin_file_in(dir: &Path, version: u32) -> BinFile {
    BinFile::new_in(dir, WINDOW_MAGIC, version, "window.bin")
}

/// Load V5 state, falling back to V4 with search placement cleared
/// (V4 stored absolute coordinates which are meaningless as relative).
fn load_state_v5() -> Option<WindowPlacementState> {
    let dir = Config::config_dir()?;
    load_state_v5_in(&dir)
}

/// `load_state_v5` と同じ移行ロジックを `dir` 注入で行う（統合テスト用、issue #429）。
fn load_state_v5_in(dir: &Path) -> Option<WindowPlacementState> {
    let bf_v5 = bin_file_in(dir, WINDOW_VERSION_V5);
    if let Some(state) = bf_v5.load::<WindowPlacementState>() {
        return Some(state);
    }
    // V4 fallback: load settings/settings_size but discard search placement.
    let bf_v4 = bin_file_in(dir, WINDOW_VERSION_V4);
    let v4_state = bf_v4.load::<WindowPlacementState>()?;
    let migrated = WindowPlacementState {
        search: None, // Discard absolute coordinates
        settings: v4_state.settings,
        settings_size: v4_state.settings_size,
    };
    // Persist migrated state as V5 so next load is fast.
    save_state_in(&migrated, dir);
    Some(migrated)
}

fn save_state(state: &WindowPlacementState) {
    if let Some(dir) = Config::config_dir() {
        save_state_in(state, &dir);
    }
}

/// `save_state` と同じ永続化を `dir` 注入で行う（統合テスト用、issue #429）。
fn save_state_in(state: &WindowPlacementState, dir: &Path) {
    let bf = bin_file_in(dir, WINDOW_VERSION_V5);
    if !bf.save(state) {
        eprintln!("[window] failed to save {}", bf.path().display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binfmt::{try_deserialize_with_header, try_serialize_with_header};

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("snotra_window_test_{}", tag));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn save_state_in_then_load_state_v5_in_roundtrip() {
        let dir = temp_dir("v5_roundtrip");
        let state = WindowPlacementState {
            search: Some(WindowPlacement { x: 10, y: 20 }),
            settings: Some(WindowPlacement { x: 30, y: 40 }),
            settings_size: Some(WindowSize {
                width: 500,
                height: 400,
            }),
        };

        save_state_in(&state, &dir);
        let loaded = load_state_v5_in(&dir).expect("load v5");
        assert_eq!(loaded, state);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_state_v5_in_migrates_v4_discarding_search_and_persists_as_v5() {
        // issue #429 の中核テスト: V4→V5 の実移行パス（絶対座標→モニター相対という
        // セマンティクス変更）を、V4 フォーマットの実バイト書き込み→V5 ロード経路で検証する。
        let dir = temp_dir("v4_to_v5_migration");

        // V4 の実バイト形式で window.bin を書き込む（postcard + ヘッダ WNDW/v4）。
        // search は V4 では絶対座標として記録されていた想定値。
        let v4_state = WindowPlacementState {
            search: Some(WindowPlacement { x: 2560, y: 300 }),
            settings: Some(WindowPlacement { x: 100, y: 200 }),
            settings_size: Some(WindowSize {
                width: 800,
                height: 600,
            }),
        };
        let bytes = try_serialize_with_header(WINDOW_MAGIC, WINDOW_VERSION_V4, &v4_state)
            .expect("serialize v4");
        let path = dir.join("window.bin");
        std::fs::write(&path, &bytes).expect("write v4 window.bin");

        // V5 ロード経路（load_state_v5_in）で読む。
        let migrated = load_state_v5_in(&dir).expect("load v4 via v5 path");

        // (a) search（絶対座標）はモニター相対座標として無意味なため破棄され None になる。
        assert_eq!(
            migrated.search, None,
            "V4 absolute search coords must be discarded"
        );
        // settings/settings_size は V4 でも絶対座標のまま意味を保つため引き継がれる。
        assert_eq!(migrated.settings, v4_state.settings);
        assert_eq!(migrated.settings_size, v4_state.settings_size);

        // (b) load_state_v5_in の副作用（save_state_in 呼び出し）により、
        // ディスク上のファイルは V5 として再保存されている。
        let raw = std::fs::read(&path).expect("read persisted window.bin");
        let reloaded_v5: WindowPlacementState =
            try_deserialize_with_header(&raw, WINDOW_MAGIC, WINDOW_VERSION_V5)
                .expect("persisted file should now deserialize as V5");
        assert_eq!(reloaded_v5, migrated);
        let as_v4: Result<WindowPlacementState, _> =
            try_deserialize_with_header(&raw, WINDOW_MAGIC, WINDOW_VERSION_V4);
        assert!(
            as_v4.is_err(),
            "persisted file should no longer be readable as V4"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn placement_state_roundtrip_header_v4() {
        let state = WindowPlacementState {
            search: Some(WindowPlacement { x: 120, y: 340 }),
            settings: Some(WindowPlacement { x: 640, y: 480 }),
            settings_size: Some(WindowSize {
                width: 760,
                height: 560,
            }),
        };
        let bytes =
            try_serialize_with_header(WINDOW_MAGIC, WINDOW_VERSION_V4, &state).expect("serialize");
        let restored: WindowPlacementState =
            try_deserialize_with_header(&bytes, WINDOW_MAGIC, WINDOW_VERSION_V4)
                .expect("deserialize");
        assert_eq!(state, restored);
    }

    #[test]
    fn placement_state_roundtrip_header_v5() {
        let state = WindowPlacementState {
            search: Some(WindowPlacement { x: 50, y: 80 }),
            settings: Some(WindowPlacement { x: 640, y: 480 }),
            settings_size: Some(WindowSize {
                width: 760,
                height: 560,
            }),
        };
        let bytes =
            try_serialize_with_header(WINDOW_MAGIC, WINDOW_VERSION_V5, &state).expect("serialize");
        let restored: WindowPlacementState =
            try_deserialize_with_header(&bytes, WINDOW_MAGIC, WINDOW_VERSION_V5)
                .expect("deserialize");
        assert_eq!(state, restored);
    }

    #[test]
    fn v4_data_not_loadable_as_v5() {
        // V4 data should not deserialize with V5 version check,
        // confirming that the migration path in load_state_v5 is needed.
        let state = WindowPlacementState {
            search: Some(WindowPlacement { x: 2560, y: 300 }),
            settings: Some(WindowPlacement { x: 100, y: 200 }),
            settings_size: None,
        };
        let bytes =
            try_serialize_with_header(WINDOW_MAGIC, WINDOW_VERSION_V4, &state).expect("serialize");
        let result: Result<WindowPlacementState, _> =
            try_deserialize_with_header(&bytes, WINDOW_MAGIC, WINDOW_VERSION_V5);
        assert!(result.is_err(), "V4 data should not load as V5");
    }
}
