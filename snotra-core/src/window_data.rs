use serde::{Deserialize, Serialize};

use crate::binfmt::BinFile;

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

/// Load V5 state, falling back to V4 with search placement cleared
/// (V4 stored absolute coordinates which are meaningless as relative).
fn load_state_v5() -> Option<WindowPlacementState> {
    let bf_v5 = BinFile::new(WINDOW_MAGIC, WINDOW_VERSION_V5, "window.bin")?;
    if let Some(state) = bf_v5.load::<WindowPlacementState>() {
        return Some(state);
    }
    // V4 fallback: load settings/settings_size but discard search placement.
    let bf_v4 = BinFile::new(WINDOW_MAGIC, WINDOW_VERSION_V4, "window.bin")?;
    let v4_state = bf_v4.load::<WindowPlacementState>()?;
    let migrated = WindowPlacementState {
        search: None, // Discard absolute coordinates
        settings: v4_state.settings,
        settings_size: v4_state.settings_size,
    };
    // Persist migrated state as V5 so next load is fast.
    save_state(&migrated);
    Some(migrated)
}

fn save_state(state: &WindowPlacementState) {
    if let Some(bf) = BinFile::new(WINDOW_MAGIC, WINDOW_VERSION_V5, "window.bin")
        && !bf.save(state)
    {
        eprintln!("[window] failed to save {}", bf.path().display());
    }
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;
    use crate::binfmt::{deserialize_with_header, serialize_with_header};

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
            serialize_with_header(WINDOW_MAGIC, WINDOW_VERSION_V4, &state).expect("serialize");
        let restored: WindowPlacementState =
            deserialize_with_header(&bytes, WINDOW_MAGIC, WINDOW_VERSION_V4).expect("deserialize");
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
            serialize_with_header(WINDOW_MAGIC, WINDOW_VERSION_V5, &state).expect("serialize");
        let restored: WindowPlacementState =
            deserialize_with_header(&bytes, WINDOW_MAGIC, WINDOW_VERSION_V5).expect("deserialize");
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
            serialize_with_header(WINDOW_MAGIC, WINDOW_VERSION_V4, &state).expect("serialize");
        let result: Option<WindowPlacementState> =
            deserialize_with_header(&bytes, WINDOW_MAGIC, WINDOW_VERSION_V5);
        assert!(result.is_none(), "V4 data should not load as V5");
    }
}
