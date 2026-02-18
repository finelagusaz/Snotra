use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::binfmt::{deserialize_with_header, serialize_with_header};
use crate::config::Config;

const WINDOW_MAGIC: [u8; 4] = *b"WNDW";
const WINDOW_VERSION_V1: u32 = 1;
const WINDOW_VERSION_V2: u32 = 2;
const WINDOW_VERSION_V3: u32 = 3;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct WindowPlacement {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct WindowSize {
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
struct WindowPlacementState {
    search: Option<WindowPlacement>,
    settings: Option<WindowPlacement>,
    settings_size: Option<WindowSize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
struct WindowPlacementStateV2 {
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

pub fn load_settings_size() -> Option<WindowSize> {
    load_state().and_then(|state| state.settings_size)
}

pub fn save_settings_size(size: WindowSize) {
    let mut state = load_state().unwrap_or_default();
    state.settings_size = Some(size);
    save_state(&state);
}

fn load_state() -> Option<WindowPlacementState> {
    let path = path()?;
    let bytes = std::fs::read(path).ok()?;

    if let Some(state) =
        deserialize_with_header::<WindowPlacementState>(&bytes, WINDOW_MAGIC, WINDOW_VERSION_V3)
    {
        return Some(state);
    }

    if let Some(state) =
        deserialize_with_header::<WindowPlacementStateV2>(&bytes, WINDOW_MAGIC, WINDOW_VERSION_V2)
    {
        return Some(WindowPlacementState {
            search: state.search,
            settings: state.settings,
            settings_size: None,
        });
    }

    // Backward compatibility for v1 payload (search window position only).
    deserialize_with_header::<WindowPlacement>(&bytes, WINDOW_MAGIC, WINDOW_VERSION_V1).map(
        |search| WindowPlacementState {
            search: Some(search),
            settings: None,
            settings_size: None,
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
    let Some(bytes) = serialize_with_header(WINDOW_MAGIC, WINDOW_VERSION_V3, state) else {
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
    fn placement_state_roundtrip_header_v3() {
        let state = WindowPlacementState {
            search: Some(WindowPlacement { x: 120, y: 340 }),
            settings: Some(WindowPlacement { x: 640, y: 480 }),
            settings_size: Some(WindowSize {
                width: 760,
                height: 560,
            }),
        };
        let bytes =
            serialize_with_header(WINDOW_MAGIC, WINDOW_VERSION_V3, &state).expect("serialize");
        let restored: WindowPlacementState =
            deserialize_with_header(&bytes, WINDOW_MAGIC, WINDOW_VERSION_V3).expect("deserialize");
        assert_eq!(state, restored);
    }

    #[test]
    fn load_state_reads_v2_payload() {
        let state = WindowPlacementStateV2 {
            search: Some(WindowPlacement { x: 120, y: 340 }),
            settings: Some(WindowPlacement { x: 640, y: 480 }),
        };
        let bytes =
            serialize_with_header(WINDOW_MAGIC, WINDOW_VERSION_V2, &state).expect("serialize v2");

        let state_v3: Option<WindowPlacementState> =
            deserialize_with_header(&bytes, WINDOW_MAGIC, WINDOW_VERSION_V3);
        assert!(state_v3.is_none());

        let restored = load_state_from_bytes(&bytes).expect("mapped v2");
        assert_eq!(
            restored,
            WindowPlacementState {
                search: state.search,
                settings: state.settings,
                settings_size: None,
            }
        );
    }

    #[test]
    fn load_state_reads_v1_payload() {
        let placement = WindowPlacement { x: 120, y: 340 };
        let bytes = serialize_with_header(WINDOW_MAGIC, WINDOW_VERSION_V1, &placement)
            .expect("serialize v1");

        let state_v3: Option<WindowPlacementState> =
            deserialize_with_header(&bytes, WINDOW_MAGIC, WINDOW_VERSION_V3);
        assert!(state_v3.is_none());

        let mapped = load_state_from_bytes(&bytes).expect("mapped v1");
        assert_eq!(
            mapped,
            WindowPlacementState {
                search: Some(placement),
                settings: None,
                settings_size: None,
            }
        );
    }

    fn load_state_from_bytes(bytes: &[u8]) -> Option<WindowPlacementState> {
        if let Some(state) =
            deserialize_with_header::<WindowPlacementState>(bytes, WINDOW_MAGIC, WINDOW_VERSION_V3)
        {
            return Some(state);
        }
        if let Some(state) = deserialize_with_header::<WindowPlacementStateV2>(
            bytes,
            WINDOW_MAGIC,
            WINDOW_VERSION_V2,
        ) {
            return Some(WindowPlacementState {
                search: state.search,
                settings: state.settings,
                settings_size: None,
            });
        }
        deserialize_with_header::<WindowPlacement>(bytes, WINDOW_MAGIC, WINDOW_VERSION_V1).map(
            |search| WindowPlacementState {
                search: Some(search),
                settings: None,
                settings_size: None,
            },
        )
    }
}
