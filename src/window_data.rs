use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::binfmt::{deserialize_with_header, serialize_with_header};
use crate::config::Config;

const WINDOW_MAGIC: [u8; 4] = *b"WNDW";
const WINDOW_VERSION_V1: u32 = 1;
const WINDOW_VERSION_V2: u32 = 2;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct WindowPlacement {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
struct WindowPlacementState {
    search: Option<WindowPlacement>,
    settings: Option<WindowPlacement>,
}

pub fn load_search_placement() -> Option<WindowPlacement> {
    load_state().and_then(|state| state.search)
}

pub fn save_search_placement(placement: WindowPlacement) {
    let mut state = load_state().unwrap_or_default();
    state.search = Some(placement);
    save_state(&state);
}

pub fn load_settings_placement() -> Option<WindowPlacement> {
    load_state().and_then(|state| state.settings)
}

pub fn save_settings_placement(placement: WindowPlacement) {
    let mut state = load_state().unwrap_or_default();
    state.settings = Some(placement);
    save_state(&state);
}

fn load_state() -> Option<WindowPlacementState> {
    let path = path()?;
    let bytes = std::fs::read(path).ok()?;

    if let Some(state) =
        deserialize_with_header::<WindowPlacementState>(&bytes, WINDOW_MAGIC, WINDOW_VERSION_V2)
    {
        return Some(state);
    }

    // Backward compatibility for v1 payload (search window position only).
    deserialize_with_header::<WindowPlacement>(&bytes, WINDOW_MAGIC, WINDOW_VERSION_V1).map(
        |search| WindowPlacementState {
            search: Some(search),
            settings: None,
        },
    )
}

fn save_state(state: &WindowPlacementState) {
    let Some(path) = path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let Some(bytes) = serialize_with_header(WINDOW_MAGIC, WINDOW_VERSION_V2, state) else {
        return;
    };
    let tmp_path = path.with_extension("bin.tmp");
    if std::fs::write(&tmp_path, &bytes).is_ok() {
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::rename(&tmp_path, &path);
    }
}

fn path() -> Option<PathBuf> {
    Config::config_dir().map(|p| p.join("window.bin"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_state_roundtrip_header_v2() {
        let state = WindowPlacementState {
            search: Some(WindowPlacement { x: 120, y: 340 }),
            settings: Some(WindowPlacement { x: 640, y: 480 }),
        };
        let bytes =
            serialize_with_header(WINDOW_MAGIC, WINDOW_VERSION_V2, &state).expect("serialize");
        let restored: WindowPlacementState =
            deserialize_with_header(&bytes, WINDOW_MAGIC, WINDOW_VERSION_V2).expect("deserialize");
        assert_eq!(state, restored);
    }

    #[test]
    fn load_state_reads_v1_payload() {
        let placement = WindowPlacement { x: 120, y: 340 };
        let bytes = serialize_with_header(WINDOW_MAGIC, WINDOW_VERSION_V1, &placement)
            .expect("serialize v1");

        let state_v2: Option<WindowPlacementState> =
            deserialize_with_header(&bytes, WINDOW_MAGIC, WINDOW_VERSION_V2);
        assert!(state_v2.is_none());

        let legacy: WindowPlacement =
            deserialize_with_header(&bytes, WINDOW_MAGIC, WINDOW_VERSION_V1).expect("legacy");
        let mapped = WindowPlacementState {
            search: Some(legacy),
            settings: None,
        };
        assert_eq!(
            mapped,
            WindowPlacementState {
                search: Some(placement),
                settings: None
            }
        );
    }
}
