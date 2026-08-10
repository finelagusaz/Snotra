//! `magic` + `version` 付きバイナリファイル入出力の共通処理（`BinFile`）。
//!
//! `index.bin` / `history.bin` / `window.bin` 等の永続形式が共有するヘッダ（magic +
//! version）検証と、tmp → rename による原子的保存を提供する。

use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fs;
use std::io::Read;
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
        Some(Self::new_in(&dir, magic, version, filename))
    }

    /// Create a new BinFile handle rooted at `dir` (injectable, e.g. for tests).
    /// Infallible: unlike `new`, the caller already resolved the directory.
    pub fn new_in(dir: &Path, magic: [u8; 4], version: u32, filename: &str) -> Self {
        Self {
            magic,
            version,
            path: dir.join(filename),
        }
    }

    /// Load data from the file using the current version (postcard format).
    pub fn load<T: DeserializeOwned>(&self) -> Option<T> {
        let bytes = fs::read(&self.path).ok()?;
        try_deserialize_with_header(&bytes, self.magic, self.version).ok()
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
    pub fn load_with_fallback<T: DeserializeOwned>(&self, fallbacks: &[u32]) -> Option<(T, u32)> {
        let bytes = fs::read(&self.path).ok()?;

        // Try current version (always postcard)
        if let Ok(data) = try_deserialize_with_header(&bytes, self.magic, self.version) {
            return Some((data, self.version));
        }

        // Try each fallback
        for &ver in fallbacks {
            if let Ok(data) = try_deserialize_with_header(&bytes, self.magic, ver) {
                return Some((data, ver));
            }
        }

        None
    }

    /// Atomically save data: write to `.tmp`, remove old file, rename `.tmp`.
    /// Returns `true` on success. `#[must_use]`: callers must check for failure and
    /// surface it (log/eprintln) rather than silently discard it (issue #428).
    #[must_use]
    pub fn save<T: Serialize>(&self, data: &T) -> bool {
        let Ok(bytes) = try_serialize_with_header(self.magic, self.version, data) else {
            return false;
        };
        self.save_bytes(&bytes)
    }

    /// Atomically save an already serialized header + payload buffer.
    ///
    /// This lets callers serialize while holding a state lock, then perform only
    /// the filesystem I/O after releasing it.
    #[must_use]
    pub(crate) fn save_bytes(&self, bytes: &[u8]) -> bool {
        if let Some(dir) = self.path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let tmp = self.path.with_extension("bin.tmp");
        if fs::write(&tmp, bytes).is_err() {
            return false;
        }
        let _ = fs::remove_file(&self.path);
        fs::rename(&tmp, &self.path).is_ok()
    }

    /// ヘッダーを検証し、**その直後の最初のフィールドだけ**を復号する。
    ///
    /// **本体を読まない。** 読むのは先頭 `HEADER_LEN + max_payload` バイトだけで、
    /// 17 MiB の `index.bin` から `built_at` を取り出すために全体を確保しない。
    ///
    /// **版は問わない**（`self.version` と一致しなくても読む）。「その値が全版で
    /// 先頭にある」ことを保証するのは呼び出し側の責務である。ヘッダーが読めない
    /// （短い・magic 違い）ときと復号できないときは `None`。
    pub fn peek_first_field<T: DeserializeOwned>(&self, max_payload: usize) -> Option<T> {
        let f = fs::File::open(&self.path).ok()?;
        let mut head = Vec::with_capacity(HEADER_LEN + max_payload);
        f.take((HEADER_LEN + max_payload) as u64)
            .read_to_end(&mut head)
            .ok()?;
        peek_first_field_from_bytes(&head, self.magic)
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

/// Result-based serialization.
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

/// ヘッダーが名乗る形式バージョンを読む（本体は復号しない）。短すぎれば `None`。
///
/// **ヘッダーの配置（magic 4 B + version u32 LE）を知る場所をここ 1 つに閉じるための口である。**
/// 版を発見する用途は [`try_deserialize_with_header`] では満たせない——あちらは期待する版を
/// **入力**に取るので、置かれているものが何かは答えられない。写しを外へ出すと、ヘッダーを
/// 変える日にコンパイラも `governance:check` も指さず、出るのは
/// 「v1701077335 は旧版である」のようなもっともらしい嘘になる。
///
/// **本体を読まない**ことが要点である。計測ハーネスは 51 MiB の一時確保を避けるために
/// 先頭 8 バイトだけを読む。
pub fn peek_version(bytes: &[u8]) -> Option<u32> {
    let raw: [u8; 4] = bytes.get(4..8)?.try_into().ok()?;
    Some(u32::from_le_bytes(raw))
}

/// バイト列に対する [`BinFile::peek_first_field`]。golden テストが**保存せずに**
/// 同じ復号を当てるための口である。
pub fn peek_first_field_from_bytes<T: DeserializeOwned>(bytes: &[u8], magic: [u8; 4]) -> Option<T> {
    if bytes.len() < HEADER_LEN || bytes[0..4] != magic {
        return None;
    }
    // **版は照合しない。** 先頭フィールドが全版で同じ意味を持つことに賭ける口であり
    // （[`crate::indexer::index_built_at_in`] が旧版の `index.bin` からも読む）、
    // ここで現行版を要求すると読めるはずのものが読めなくなる。
    postcard::take_from_bytes::<T>(&bytes[HEADER_LEN..])
        .ok()
        .map(|(value, _rest)| value)
}

/// Result-based deserialization.
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
    let file_version = peek_version(bytes).ok_or(BinError::BufferTooShort)?;
    if file_version != version {
        return Err(BinError::VersionMismatch {
            expected: version,
            actual: file_version,
        });
    }
    postcard::from_bytes(&bytes[HEADER_LEN..]).map_err(|_| BinError::DeserializeFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Dummy {
        value: u32,
    }

    // --- BinFile tests ---
    // 実装メモ: header roundtrip / magic mismatch / version mismatch coverage lives in
    // the `try_roundtrip_with_header` / `try_deserialize_magic_mismatch` /
    // `try_deserialize_version_mismatch` tests below (Result-based API).

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("snotra_binfmt_test_{}", tag));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// Helper: create a BinFile pointing to a specific directory (bypasses Config::config_dir)
    fn bin_file_in(dir: &Path, magic: [u8; 4], version: u32, filename: &str) -> BinFile {
        BinFile::new_in(dir, magic, version, filename)
    }

    #[test]
    fn binfile_new_in_joins_dir_and_filename() {
        let dir = temp_dir("binfile_new_in");
        let bf = BinFile::new_in(&dir, *b"TEST", 1, "data.bin");
        assert_eq!(bf.path(), dir.join("data.bin"));
        assert_eq!(bf.magic, *b"TEST");
        assert_eq!(bf.version, 1);
        let _ = std::fs::remove_dir_all(&dir);
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

        let (output, ver): (Dummy, u32) = bf.load_with_fallback(&[2]).expect("load");
        assert_eq!(output, input);
        assert_eq!(ver, 3);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn binfile_load_with_fallback_postcard_legacy() {
        let dir = temp_dir("binfile_fb_postcard");
        // Write a v2 postcard file
        let path = dir.join("data.bin");
        let bytes = try_serialize_with_header(*b"TEST", 2, &Dummy { value: 55 }).unwrap();
        std::fs::write(&path, &bytes).unwrap();

        // Create a BinFile expecting v3 as current
        let bf = bin_file_in(&dir, *b"TEST", 3, "data.bin");
        let (output, ver): (Dummy, u32) = bf.load_with_fallback(&[2]).expect("load v2 fallback");
        assert_eq!(output, Dummy { value: 55 });
        assert_eq!(ver, 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn binfile_load_with_fallback_no_match() {
        let dir = temp_dir("binfile_fb_nomatch");
        // Write a file with a different magic
        let path = dir.join("data.bin");
        let bytes = try_serialize_with_header(*b"NOPE", 1, &Dummy { value: 1 }).unwrap();
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
            try_deserialize_with_header(&raw, *b"TEST", 1).expect("manual deserialize");
        assert_eq!(input, output);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn binfile_save_creates_parent_directories() {
        let dir = temp_dir("binfile_mkdir");
        let nested = dir.join("sub").join("dir");
        let bf = BinFile::new_in(&nested, *b"TEST", 1, "data.bin");

        assert!(bf.save(&Dummy { value: 1 }));
        assert!(bf.path().exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- try_deserialize_with_header / try_serialize_with_header tests ---

    #[test]
    fn try_roundtrip_with_header() {
        let input = Dummy { value: 42 };
        let bytes = try_serialize_with_header(*b"TEST", 1, &input).expect("serialize");
        let output: Dummy = try_deserialize_with_header(&bytes, *b"TEST", 1).expect("deserialize");
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
    fn peek_first_field_reads_only_the_head_and_ignores_the_version() {
        #[derive(serde::Serialize)]
        struct Payload {
            first: u64,
            rest: Vec<u32>,
        }
        let dir = temp_dir("binfile_peek_first");
        // 版 9 で書く（`BinFile` が名乗る版とわざと違える）。
        let bytes = try_serialize_with_header(
            *b"TEST",
            9,
            &Payload {
                first: 1_700_000_000,
                rest: vec![1, 2, 3],
            },
        )
        .expect("serialize");
        let bf = BinFile::new_in(&dir, *b"TEST", 1, "data.bin");
        assert!(bf.save_bytes(&bytes), "save");

        // 版が一致しなくても先頭フィールドは読める（版に依らない口である）。
        assert_eq!(bf.peek_first_field::<u64>(16), Some(1_700_000_000));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn peek_first_field_rejects_a_foreign_magic_and_a_truncated_file() {
        let dir = temp_dir("binfile_peek_reject");
        let bf = BinFile::new_in(&dir, *b"TEST", 1, "data.bin");

        // magic 違い。
        let other = try_serialize_with_header(*b"XXXX", 1, &7u64).expect("serialize");
        assert!(bf.save_bytes(&other), "save");
        assert_eq!(bf.peek_first_field::<u64>(16), None, "magic 不一致は None");

        // ヘッダーより短い。
        assert!(bf.save_bytes(&[1, 2, 3]), "save");
        assert_eq!(bf.peek_first_field::<u64>(16), None, "切り詰めは None");

        // ファイル不在。
        bf.remove();
        assert_eq!(bf.peek_first_field::<u64>(16), None, "不在は None");

        let _ = std::fs::remove_dir_all(&dir);
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
        let output: Large = try_deserialize_with_header(&bytes, *b"LRGE", 2).expect("deserialize");
        assert_eq!(input, output);
    }
}
