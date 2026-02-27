use serde::{Deserialize, Serialize};

#[allow(deprecated)]
use crate::binfmt::{deserialize_bincode_with_header, deserialize_with_header, BinFile};

const WINDOW_MAGIC: [u8; 4] = *b"WNDW";
const WINDOW_VERSION_V1: u32 = 1;
const WINDOW_VERSION_V2: u32 = 2;
const WINDOW_VERSION_V3: u32 = 3;
const WINDOW_VERSION_V4: u32 = 4; // postcard (現行)

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

#[allow(deprecated)]
fn load_state() -> Option<WindowPlacementState> {
    let bf = bin_file()?;
    // V2/V1 deserialize different types, so we use load_bytes + manual fallback.
    let bytes = bf.load_bytes()?;

    // V4: postcard (current)
    if let Some(state) =
        deserialize_with_header::<WindowPlacementState>(&bytes, WINDOW_MAGIC, WINDOW_VERSION_V4)
    {
        return Some(state);
    }

    // V3: bincode (legacy, written before postcard migration)
    if let Some(state) = deserialize_bincode_with_header::<WindowPlacementState>(
        &bytes,
        WINDOW_MAGIC,
        WINDOW_VERSION_V3,
    ) {
        return Some(state);
    }

    // V2: bincode (legacy, different struct type)
    if let Some(state) = deserialize_bincode_with_header::<WindowPlacementStateV2>(
        &bytes,
        WINDOW_MAGIC,
        WINDOW_VERSION_V2,
    ) {
        return Some(WindowPlacementState {
            search: state.search,
            settings: state.settings,
            settings_size: None,
        });
    }

    // V1: bincode (legacy, search window position only)
    deserialize_bincode_with_header::<WindowPlacement>(&bytes, WINDOW_MAGIC, WINDOW_VERSION_V1)
        .map(|search| WindowPlacementState {
            search: Some(search),
            settings: None,
            settings_size: None,
        })
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
    use crate::binfmt::{serialize_bincode_with_header, serialize_with_header};

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
    fn load_state_reads_v3_bincode_payload() {
        let state = WindowPlacementState {
            search: Some(WindowPlacement { x: 100, y: 200 }),
            settings: Some(WindowPlacement { x: 300, y: 400 }),
            settings_size: Some(WindowSize {
                width: 800,
                height: 600,
            }),
        };
        // Simulate a bincode V3 file written before postcard migration
        let bytes = serialize_bincode_with_header(WINDOW_MAGIC, WINDOW_VERSION_V3, &state)
            .expect("serialize bincode v3");

        // V4 postcard should not read bincode data
        let v4_attempt: Option<WindowPlacementState> =
            deserialize_with_header(&bytes, WINDOW_MAGIC, WINDOW_VERSION_V4);
        assert!(v4_attempt.is_none());

        let restored = load_state_from_bytes(&bytes).expect("mapped v3 bincode");
        assert_eq!(state, restored);
    }

    #[test]
    fn load_state_reads_v2_payload() {
        let state = WindowPlacementStateV2 {
            search: Some(WindowPlacement { x: 120, y: 340 }),
            settings: Some(WindowPlacement { x: 640, y: 480 }),
        };
        let bytes = serialize_bincode_with_header(WINDOW_MAGIC, WINDOW_VERSION_V2, &state)
            .expect("serialize bincode v2");

        let state_v4: Option<WindowPlacementState> =
            deserialize_with_header(&bytes, WINDOW_MAGIC, WINDOW_VERSION_V4);
        assert!(state_v4.is_none());

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
        let bytes = serialize_bincode_with_header(WINDOW_MAGIC, WINDOW_VERSION_V1, &placement)
            .expect("serialize bincode v1");

        let state_v4: Option<WindowPlacementState> =
            deserialize_with_header(&bytes, WINDOW_MAGIC, WINDOW_VERSION_V4);
        assert!(state_v4.is_none());

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
        // V4: postcard (current)
        if let Some(state) =
            deserialize_with_header::<WindowPlacementState>(bytes, WINDOW_MAGIC, WINDOW_VERSION_V4)
        {
            return Some(state);
        }
        // V3: bincode (legacy)
        if let Some(state) = deserialize_bincode_with_header::<WindowPlacementState>(
            bytes,
            WINDOW_MAGIC,
            WINDOW_VERSION_V3,
        ) {
            return Some(state);
        }
        // V2: bincode (legacy)
        if let Some(state) = deserialize_bincode_with_header::<WindowPlacementStateV2>(
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
        // V1: bincode (legacy)
        deserialize_bincode_with_header::<WindowPlacement>(bytes, WINDOW_MAGIC, WINDOW_VERSION_V1)
            .map(|search| WindowPlacementState {
                search: Some(search),
                settings: None,
                settings_size: None,
            })
    }
}
