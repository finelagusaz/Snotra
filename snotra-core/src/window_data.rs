use serde::{Deserialize, Serialize};

use crate::binfmt::BinFile;

const WINDOW_MAGIC: [u8; 4] = *b"WNDW";
const WINDOW_VERSION_V4: u32 = 4; // postcard (現行)

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
    let bf = bin_file()?;
    bf.load()
}

fn save_state(state: &WindowPlacementState) {
    if let Some(bf) = bin_file() {
        bf.save(state);
    }
}

fn bin_file() -> Option<BinFile> {
    BinFile::new(WINDOW_MAGIC, WINDOW_VERSION_V4, "window.bin")
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
}
