//! pieBot_common
//!
//! Canonical JSON serialization + SHA-256 hashing utilities.
//! This exists to guarantee determinism for:
//! - audit event hashing
//! - redaction pre/post integrity hashes
//! - artifact hash references
//!
//! IMPORTANT: Do not "pretty print". Hashes must be computed over canonical bytes.

use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CanonError {
    #[error("failed to serialize json: {0}")]
    Json(#[from] serde_json::Error),
}

/// Serialize to canonical JSON bytes:
/// - stable key ordering (we enforce sorting via Value roundtrip)
/// - no whitespace
/// - UTF-8
pub fn canonical_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, CanonError> {
    let v = serde_json::to_value(value)?;
    let sorted = sort_json_value(v);
    Ok(serde_json::to_vec(&sorted)?)
}

/// Return "sha256:<hex>" of canonical JSON bytes.
pub fn sha256_canonical_json<T: Serialize>(value: &T) -> Result<String, CanonError> {
    let bytes = canonical_json_bytes(value)?;
    Ok(sha256_bytes(&bytes))
}

/// Return "sha256:<hex>" of raw bytes.
pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    format!("sha256:{}", hex::encode(digest))
}

fn sort_json_value(v: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match v {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut out = serde_json::Map::new();
            for (k, v) in entries {
                out.insert(k, sort_json_value(v));
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(sort_json_value).collect()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct Obj {
        b: u32,
        a: u32,
    }

    #[test]
    fn canonical_hash_is_stable() {
        let x = Obj { b: 2, a: 1 };
        let y = Obj { a: 1, b: 2 };
        let hx = sha256_canonical_json(&x).unwrap();
        let hy = sha256_canonical_json(&y).unwrap();
        assert_eq!(hx, hy);
    }
}