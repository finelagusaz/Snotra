use std::fmt;

/// Errors from binary file serialization/deserialization.
#[derive(Debug)]
pub enum BinError {
    MagicMismatch {
        expected: [u8; 4],
        actual: [u8; 4],
    },
    VersionMismatch {
        expected: u32,
        actual: u32,
    },
    DeserializeFailed,
    SerializeFailed,
    IoError(std::io::Error),
    BufferTooShort,
}

impl fmt::Display for BinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MagicMismatch { expected, actual } => {
                write!(
                    f,
                    "magic mismatch: expected {}, got {}",
                    String::from_utf8_lossy(expected),
                    String::from_utf8_lossy(actual),
                )
            }
            Self::VersionMismatch { expected, actual } => {
                write!(
                    f,
                    "version mismatch: expected {}, got {}",
                    expected, actual
                )
            }
            Self::DeserializeFailed => write!(f, "deserialization failed"),
            Self::SerializeFailed => write!(f, "serialization failed"),
            Self::IoError(e) => write!(f, "I/O error: {}", e),
            Self::BufferTooShort => write!(f, "buffer too short for header"),
        }
    }
}

impl std::error::Error for BinError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::IoError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for BinError {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e)
    }
}

/// Errors from config validation.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigError {
    HotkeyModifierEmpty,
    HotkeyKeyEmpty,
    HotkeySystemConflict { modifier: String, key: String },
    MaxResultsZero,
    WindowWidthTooSmall(u32),
    FuzzyCapRatioOutOfRange { value: f64 },
    ScanPathEmpty { index: usize },
    InstantCommandPrefixEmpty,
    InstantCommandPrefixSlash,
    InstantCommandDuplicateName { name: String },
    MigemoMinCharsZero,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bin_error_display_magic_mismatch() {
        let err = BinError::MagicMismatch {
            expected: *b"TEST",
            actual: *b"NOPE",
        };
        let msg = format!("{}", err);
        assert!(msg.contains("magic mismatch"));
        assert!(msg.contains("TEST"));
        assert!(msg.contains("NOPE"));
    }

    #[test]
    fn bin_error_display_version_mismatch() {
        let err = BinError::VersionMismatch {
            expected: 3,
            actual: 1,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("version mismatch"));
        assert!(msg.contains("3"));
        assert!(msg.contains("1"));
    }

    #[test]
    fn bin_error_display_deserialize_failed() {
        let err = BinError::DeserializeFailed;
        assert_eq!(format!("{}", err), "deserialization failed");
    }

    #[test]
    fn bin_error_display_serialize_failed() {
        let err = BinError::SerializeFailed;
        assert_eq!(format!("{}", err), "serialization failed");
    }

    #[test]
    fn bin_error_display_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = BinError::IoError(io_err);
        let msg = format!("{}", err);
        assert!(msg.contains("I/O error"));
        assert!(msg.contains("file not found"));
    }

    #[test]
    fn bin_error_display_buffer_too_short() {
        let err = BinError::BufferTooShort;
        assert_eq!(format!("{}", err), "buffer too short for header");
    }

    #[test]
    fn bin_error_source_io_error() {
        use std::error::Error;
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let err = BinError::IoError(io_err);
        let source = err.source().expect("IoError should have a source");
        assert!(source.to_string().contains("access denied"));
    }

    #[test]
    fn bin_error_source_non_io_variants_return_none() {
        use std::error::Error;
        let variants: Vec<BinError> = vec![
            BinError::MagicMismatch {
                expected: *b"AAAA",
                actual: *b"BBBB",
            },
            BinError::VersionMismatch {
                expected: 1,
                actual: 2,
            },
            BinError::DeserializeFailed,
            BinError::SerializeFailed,
            BinError::BufferTooShort,
        ];
        for variant in &variants {
            assert!(
                variant.source().is_none(),
                "{:?} should have no source",
                variant
            );
        }
    }

    #[test]
    fn bin_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "broken");
        let bin_err: BinError = io_err.into();
        assert!(matches!(bin_err, BinError::IoError(_)));
    }
}
