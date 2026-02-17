use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::binfmt::{deserialize_with_header, serialize_with_header};
use crate::config::Config;

const WINDOW_MAGIC: [u8; 4] = *b"WNDW";
const WINDOW_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct WindowPlacement {
    pub x: i32,
    pub y: i32,
}

pub fn load_placement() -> Option<WindowPlacement> {
    let path = path()?;
    let bytes = std::fs::read(path).ok()?;
    deserialize_with_header(&bytes, WINDOW_MAGIC, WINDOW_VERSION)
}

pub fn save_placement(placement: WindowPlacement) {
    let Some(path) = path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let Some(bytes) = serialize_with_header(WINDOW_MAGIC, WINDOW_VERSION, &placement) else {
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
    fn placement_roundtrip_header() {
        let placement = WindowPlacement { x: 120, y: 340 };
        let bytes =
            serialize_with_header(WINDOW_MAGIC, WINDOW_VERSION, &placement).expect("serialize");
        let restored: WindowPlacement =
            deserialize_with_header(&bytes, WINDOW_MAGIC, WINDOW_VERSION).expect("deserialize");
        assert_eq!(placement, restored);
    }
}
