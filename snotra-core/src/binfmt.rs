use serde::de::DeserializeOwned;
use serde::Serialize;

const HEADER_LEN: usize = 8;

pub fn serialize_with_header<T: Serialize>(
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
    bincode::deserialize(&bytes[HEADER_LEN..]).ok()
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
}
