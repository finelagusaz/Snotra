use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::Config;

const HEADER_LEN: usize = 8;

/// A typed handle for a versioned binary file with magic+version header.
///
/// Encapsulates the file path (derived from `Config::config_dir()`), magic bytes,
/// and current version. Provides atomic save (write to `.tmp` then rename) and
/// load with optional legacy-version fallback chain.
pub struct BinFile {
    pub(crate) magic: [u8; 4],
    pub(crate) version: u32,
    pub(crate) path: PathBuf,
}

impl BinFile {
    /// Create a new BinFile handle. Returns `None` if config dir is unavailable.
    pub fn new(magic: [u8; 4], version: u32, filename: &str) -> Option<Self> {
        let dir = Config::config_dir()?;
        Some(Self {
            magic,
            version,
            path: dir.join(filename),
        })
    }

    /// Load data from the file using the current version (postcard format).
    pub fn load<T: DeserializeOwned>(&self) -> Option<T> {
        let bytes = fs::read(&self.path).ok()?;
        deserialize_with_header(&bytes, self.magic, self.version)
    }

    /// Read raw file bytes. Useful when the caller needs custom deserialization
    /// logic (e.g., different types for different legacy versions).
    pub fn load_bytes(&self) -> Option<Vec<u8>> {
        fs::read(&self.path).ok()
    }

    /// Load data, trying the current version first, then each fallback in order.
    ///
    /// `fallbacks` is a slice of `(version, is_bincode)` pairs. Each entry
    /// specifies a legacy version number and whether it was serialized with
    /// bincode (`true`) or postcard (`false`).
    ///
    /// Returns `Some((data, version))` where `version` is the version that
    /// succeeded, so the caller can apply version-specific migrations.
    pub fn load_with_fallback<T: DeserializeOwned>(
        &self,
        fallbacks: &[(u32, bool)],
    ) -> Option<(T, u32)> {
        let bytes = fs::read(&self.path).ok()?;

        // Try current version (always postcard)
        if let Some(data) = deserialize_with_header(&bytes, self.magic, self.version) {
            return Some((data, self.version));
        }

        // Try each fallback
        for &(ver, is_bincode) in fallbacks {
            let result = if is_bincode {
                deserialize_bincode_with_header(&bytes, self.magic, ver)
            } else {
                deserialize_with_header(&bytes, self.magic, ver)
            };
            if let Some(data) = result {
                return Some((data, ver));
            }
        }

        None
    }

    /// Atomically save data: write to `.tmp`, remove old file, rename `.tmp`.
    /// Returns `true` on success.
    pub fn save<T: Serialize>(&self, data: &T) -> bool {
        if let Some(dir) = self.path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let Some(bytes) = serialize_with_header(self.magic, self.version, data) else {
            return false;
        };
        let tmp = self.path.with_extension("bin.tmp");
        if fs::write(&tmp, &bytes).is_err() {
            return false;
        }
        let _ = fs::remove_file(&self.path);
        fs::rename(&tmp, &self.path).is_ok()
    }

    /// Delete the file.
    pub fn remove(&self) {
        let _ = fs::remove_file(&self.path);
    }

    /// The resolved file path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub fn serialize_with_header<T: Serialize>(
    magic: [u8; 4],
    version: u32,
    payload: &T,
) -> Option<Vec<u8>> {
    let body = postcard::to_allocvec(payload).ok()?;
    let mut out = Vec::with_capacity(HEADER_LEN + body.len());
    out.extend_from_slice(&magic);
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(&body);
    Some(out)
}

/// Legacy bincode deserializer for migration from pre-postcard format.
pub fn deserialize_bincode_with_header<T: DeserializeOwned>(
    bytes: &[u8],
    magic: [u8; 4],
    version: u32,
) -> Option<T> {
    if bytes.len() < HEADER_LEN {
        return None;
    }
    if bytes[0..4] != magic {
        return None;
    }
    let mut ver = [0u8; 4];
    ver.copy_from_slice(&bytes[4..8]);
    if u32::from_le_bytes(ver) != version {
        return None;
    }
    bincode::deserialize(&bytes[HEADER_LEN..]).ok()
}

#[cfg(test)]
pub fn serialize_bincode_with_header<T: Serialize>(
    magic: [u8; 4],
    version: u32,
    payload: &T,
) -> Option<Vec<u8>> {
    let body = bincode::serialize(payload).ok()?;
    let mut out = Vec::with_capacity(HEADER_LEN + body.len());
    out.extend_from_slice(&magic);
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(&body);
    Some(out)
}

pub fn deserialize_with_header<T: DeserializeOwned>(
    bytes: &[u8],
    magic: [u8; 4],
    version: u32,
) -> Option<T> {
    if bytes.len() < HEADER_LEN {
        return None;
    }
    if bytes[0..4] != magic {
        return None;
    }
    let mut ver = [0u8; 4];
    ver.copy_from_slice(&bytes[4..8]);
    if u32::from_le_bytes(ver) != version {
        return None;
    }
    postcard::from_bytes(&bytes[HEADER_LEN..]).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Dummy {
        value: u32,
    }

    #[test]
    fn roundtrip_with_header() {
        let input = Dummy { value: 42 };
        let bytes = serialize_with_header(*b"TEST", 1, &input).expect("serialize");
        let output: Dummy = deserialize_with_header(&bytes, *b"TEST", 1).expect("deserialize");
        assert_eq!(input, output);
    }

    #[test]
    fn roundtrip_bincode_with_header() {
        let input = Dummy { value: 99 };
        let bytes =
            serialize_bincode_with_header(*b"TEST", 1, &input).expect("serialize bincode");
        let output: Dummy =
            deserialize_bincode_with_header(&bytes, *b"TEST", 1).expect("deserialize bincode");
        assert_eq!(input, output);
    }

    #[test]
    fn bincode_data_not_readable_by_postcard() {
        // u32::MAX is encoded as 4 bytes (ff ff ff ff) by bincode.
        // postcard reads it as a varint where all 4 bytes have the continuation
        // bit set, so it requires a 5th byte that never comes → decode error → None.
        let input = Dummy { value: u32::MAX };
        let bytes =
            serialize_bincode_with_header(*b"TEST", 1, &input).expect("serialize bincode");
        // postcard cannot decode the 4-byte LE payload as a complete varint
        let output: Option<Dummy> = deserialize_with_header(&bytes, *b"TEST", 1);
        assert!(output.is_none());
    }

    #[test]
    fn deserialize_fails_on_magic_mismatch() {
        let input = Dummy { value: 1 };
        let bytes = serialize_with_header(*b"GOOD", 1, &input).expect("serialize");
        let output: Option<Dummy> = deserialize_with_header(&bytes, *b"BAD!", 1);
        assert!(output.is_none());
    }

    #[test]
    fn deserialize_fails_on_version_mismatch() {
        let input = Dummy { value: 1 };
        let bytes = serialize_with_header(*b"TEST", 1, &input).expect("serialize");
        let output: Option<Dummy> = deserialize_with_header(&bytes, *b"TEST", 2);
        assert!(output.is_none());
    }

    // --- BinFile tests ---

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("snotra_binfmt_test_{}", tag));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// Helper: create a BinFile pointing to a specific directory (bypasses Config::config_dir)
    fn bin_file_in(dir: &Path, magic: [u8; 4], version: u32, filename: &str) -> BinFile {
        BinFile {
            magic,
            version,
            path: dir.join(filename),
        }
    }

    #[test]
    fn binfile_save_load_roundtrip() {
        let dir = temp_dir("binfile_roundtrip");
        let bf = bin_file_in(&dir, *b"TEST", 1, "data.bin");

        let input = Dummy { value: 42 };
        assert!(bf.save(&input));

        let output: Dummy = bf.load().expect("load");
        assert_eq!(input, output);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn binfile_save_is_atomic_no_tmp_left() {
        let dir = temp_dir("binfile_atomic");
        let bf = bin_file_in(&dir, *b"TEST", 1, "data.bin");

        let input = Dummy { value: 7 };
        assert!(bf.save(&input));

        // The .tmp file should not exist after a successful save
        let tmp = dir.join("data.bin.tmp");
        assert!(!tmp.exists(), ".tmp file should be cleaned up");
        assert!(bf.path().exists(), "final file should exist");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn binfile_save_overwrites_previous() {
        let dir = temp_dir("binfile_overwrite");
        let bf = bin_file_in(&dir, *b"TEST", 1, "data.bin");

        assert!(bf.save(&Dummy { value: 1 }));
        assert!(bf.save(&Dummy { value: 2 }));

        let output: Dummy = bf.load().expect("load");
        assert_eq!(output.value, 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn binfile_load_returns_none_when_missing() {
        let dir = temp_dir("binfile_missing");
        let bf = bin_file_in(&dir, *b"TEST", 1, "nonexistent.bin");

        let result: Option<Dummy> = bf.load();
        assert!(result.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn binfile_remove_deletes_file() {
        let dir = temp_dir("binfile_remove");
        let bf = bin_file_in(&dir, *b"TEST", 1, "data.bin");

        assert!(bf.save(&Dummy { value: 1 }));
        assert!(bf.path().exists());

        bf.remove();
        assert!(!bf.path().exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn binfile_remove_noop_when_missing() {
        let dir = temp_dir("binfile_remove_noop");
        let bf = bin_file_in(&dir, *b"TEST", 1, "nonexistent.bin");
        bf.remove(); // should not panic
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn binfile_load_with_fallback_current_version() {
        let dir = temp_dir("binfile_fb_current");
        let bf = bin_file_in(&dir, *b"TEST", 3, "data.bin");

        let input = Dummy { value: 100 };
        assert!(bf.save(&input));

        let (output, ver): (Dummy, u32) =
            bf.load_with_fallback(&[(2, false), (1, true)]).expect("load");
        assert_eq!(output, input);
        assert_eq!(ver, 3);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn binfile_load_with_fallback_postcard_legacy() {
        let dir = temp_dir("binfile_fb_postcard");
        // Write a v2 postcard file
        let path = dir.join("data.bin");
        let bytes = serialize_with_header(*b"TEST", 2, &Dummy { value: 55 }).unwrap();
        std::fs::write(&path, &bytes).unwrap();

        // Create a BinFile expecting v3 as current
        let bf = bin_file_in(&dir, *b"TEST", 3, "data.bin");
        let (output, ver): (Dummy, u32) =
            bf.load_with_fallback(&[(2, false), (1, true)]).expect("load v2 fallback");
        assert_eq!(output, Dummy { value: 55 });
        assert_eq!(ver, 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn binfile_load_with_fallback_bincode_legacy() {
        let dir = temp_dir("binfile_fb_bincode");
        // Write a v1 bincode file
        let path = dir.join("data.bin");
        let bytes = serialize_bincode_with_header(*b"TEST", 1, &Dummy { value: 77 }).unwrap();
        std::fs::write(&path, &bytes).unwrap();

        // Create a BinFile expecting v3 as current
        let bf = bin_file_in(&dir, *b"TEST", 3, "data.bin");
        let (output, ver): (Dummy, u32) =
            bf.load_with_fallback(&[(2, false), (1, true)]).expect("load v1 fallback");
        assert_eq!(output, Dummy { value: 77 });
        assert_eq!(ver, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn binfile_load_with_fallback_no_match() {
        let dir = temp_dir("binfile_fb_nomatch");
        // Write a file with a different magic
        let path = dir.join("data.bin");
        let bytes = serialize_with_header(*b"NOPE", 1, &Dummy { value: 1 }).unwrap();
        std::fs::write(&path, &bytes).unwrap();

        let bf = bin_file_in(&dir, *b"TEST", 3, "data.bin");
        let result: Option<(Dummy, u32)> = bf.load_with_fallback(&[(2, false), (1, true)]);
        assert!(result.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn binfile_load_bytes_returns_raw_contents() {
        let dir = temp_dir("binfile_load_bytes");
        let bf = bin_file_in(&dir, *b"TEST", 1, "data.bin");

        let input = Dummy { value: 42 };
        assert!(bf.save(&input));

        let raw = bf.load_bytes().expect("load_bytes");
        // Verify the raw bytes can be deserialized manually
        let output: Dummy =
            deserialize_with_header(&raw, *b"TEST", 1).expect("manual deserialize");
        assert_eq!(input, output);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn binfile_save_creates_parent_directories() {
        let dir = temp_dir("binfile_mkdir");
        let nested = dir.join("sub").join("dir");
        let bf = BinFile {
            magic: *b"TEST",
            version: 1,
            path: nested.join("data.bin"),
        };

        assert!(bf.save(&Dummy { value: 1 }));
        assert!(bf.path().exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
