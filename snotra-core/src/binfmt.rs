use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::error::BinError;

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
    #[allow(deprecated)]
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
    /// `fallbacks` is a slice of legacy version numbers (all postcard format).
    ///
    /// Returns `Some((data, version))` where `version` is the version that
    /// succeeded, so the caller can apply version-specific migrations.
    #[allow(deprecated)]
    pub fn load_with_fallback<T: DeserializeOwned>(
        &self,
        fallbacks: &[u32],
    ) -> Option<(T, u32)> {
        let bytes = fs::read(&self.path).ok()?;

        // Try current version (always postcard)
        if let Some(data) = deserialize_with_header(&bytes, self.magic, self.version) {
            return Some((data, self.version));
        }

        // Try each fallback
        for &ver in fallbacks {
            if let Some(data) = deserialize_with_header(&bytes, self.magic, ver) {
                return Some((data, ver));
            }
        }

        None
    }

    /// Atomically save data: write to `.tmp`, remove old file, rename `.tmp`.
    /// Returns `true` on success.
    #[allow(deprecated)]
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

#[deprecated(note = "use try_serialize_with_header for Result-based error handling")]
pub fn serialize_with_header<T: Serialize>(
    magic: [u8; 4],
    version: u32,
    payload: &T,
) -> Option<Vec<u8>> {
    try_serialize_with_header(magic, version, payload).ok()
}

/// Result-based serialization (preferred over Option-based `serialize_with_header`).
pub fn try_serialize_with_header<T: Serialize>(
    magic: [u8; 4],
    version: u32,
    payload: &T,
) -> Result<Vec<u8>, BinError> {
    // ヘッダを先頭に書き込み、postcard 本体を同じ Vec へ直接追記する（postcard::to_extend）。
    // to_allocvec で本体を別 Vec に確保してから extend_from_slice でコピーし直す二重バッファを
    // 避け、save 区間のピークメモリと総コピー量を削減する。出力バイト列は従来と同一。
    let mut buf = Vec::with_capacity(HEADER_LEN);
    buf.extend_from_slice(&magic);
    buf.extend_from_slice(&version.to_le_bytes());
    postcard::to_extend(payload, buf).map_err(|_| BinError::SerializeFailed)
}

#[deprecated(note = "use try_deserialize_with_header for Result-based error handling")]
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

/// Result-based deserialization (preferred over Option-based `deserialize_with_header`).
pub fn try_deserialize_with_header<T: DeserializeOwned>(
    bytes: &[u8],
    magic: [u8; 4],
    version: u32,
) -> Result<T, BinError> {
    if bytes.len() < HEADER_LEN {
        return Err(BinError::BufferTooShort);
    }
    let file_magic: [u8; 4] = bytes[0..4].try_into().unwrap();
    if file_magic != magic {
        return Err(BinError::MagicMismatch {
            expected: magic,
            actual: file_magic,
        });
    }
    let file_version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    if file_version != version {
        return Err(BinError::VersionMismatch {
            expected: version,
            actual: file_version,
        });
    }
    postcard::from_bytes(&bytes[HEADER_LEN..]).map_err(|_| BinError::DeserializeFailed)
}

#[cfg(test)]
#[allow(deprecated)]
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
            bf.load_with_fallback(&[2]).expect("load");
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
            bf.load_with_fallback(&[2]).expect("load v2 fallback");
        assert_eq!(output, Dummy { value: 55 });
        assert_eq!(ver, 2);

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
        let result: Option<(Dummy, u32)> = bf.load_with_fallback(&[2]);
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

    // --- try_deserialize_with_header / try_serialize_with_header tests ---

    #[test]
    fn try_roundtrip_with_header() {
        let input = Dummy { value: 42 };
        let bytes = try_serialize_with_header(*b"TEST", 1, &input).expect("serialize");
        let output: Dummy =
            try_deserialize_with_header(&bytes, *b"TEST", 1).expect("deserialize");
        assert_eq!(input, output);
    }

    #[test]
    fn try_deserialize_magic_mismatch() {
        let input = Dummy { value: 1 };
        let bytes = try_serialize_with_header(*b"GOOD", 1, &input).expect("serialize");
        let err = try_deserialize_with_header::<Dummy>(&bytes, *b"BAD!", 1).unwrap_err();
        assert!(matches!(
            err,
            BinError::MagicMismatch {
                expected: [b'B', b'A', b'D', b'!'],
                actual: [b'G', b'O', b'O', b'D'],
            }
        ));
    }

    #[test]
    fn try_deserialize_version_mismatch() {
        let input = Dummy { value: 1 };
        let bytes = try_serialize_with_header(*b"TEST", 1, &input).expect("serialize");
        let err = try_deserialize_with_header::<Dummy>(&bytes, *b"TEST", 2).unwrap_err();
        assert!(matches!(
            err,
            BinError::VersionMismatch {
                expected: 2,
                actual: 1,
            }
        ));
    }

    #[test]
    fn try_deserialize_short_buffer() {
        let err = try_deserialize_with_header::<Dummy>(&[0u8; 4], *b"TEST", 1).unwrap_err();
        assert!(matches!(err, BinError::BufferTooShort));
    }

    #[test]
    fn try_deserialize_empty_buffer() {
        let err = try_deserialize_with_header::<Dummy>(&[], *b"TEST", 1).unwrap_err();
        assert!(matches!(err, BinError::BufferTooShort));
    }

    #[test]
    fn try_deserialize_corrupt_payload() {
        // Valid header but garbage payload
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"TEST");
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        let err = try_deserialize_with_header::<Dummy>(&bytes, *b"TEST", 1).unwrap_err();
        assert!(matches!(err, BinError::DeserializeFailed));
    }

    #[test]
    fn try_serialize_roundtrip_large_struct() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct Large {
            values: Vec<u32>,
            name: String,
        }
        let input = Large {
            values: vec![1, 2, 3, 4, 5],
            name: "hello".to_string(),
        };
        let bytes = try_serialize_with_header(*b"LRGE", 2, &input).expect("serialize");
        let output: Large =
            try_deserialize_with_header(&bytes, *b"LRGE", 2).expect("deserialize");
        assert_eq!(input, output);
    }
}
