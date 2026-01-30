//! pieBot_audit_log
//!
//! Append-only JSONL audit log with hash chaining.
//! - Each record includes: event, prev_hash, hash
//! - Hash is computed over canonical JSON of (event + prev_hash)
//! - Verifier replays and checks integrity end-to-end

use pieBot_audit_spec::AuditEvent;
use pieBot_common::sha256_canonical_json;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuditLogError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("canonical json error: {0}")]
    Canon(#[from] pieBot_common::CanonError),
    #[error("hash mismatch at line {line}: expected {expected}, got {got}")]
    HashMismatch { line: usize, expected: String, got: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    pub prev_hash: String, // sha256:... or "sha256:00..00" for genesis
    pub hash: String,      // sha256:...
    pub event: AuditEvent,
}

#[derive(Debug, Clone, Serialize)]
struct HashPayload<'a> {
    prev_hash: &'a str,
    event: &'a AuditEvent,
}

pub fn genesis_hash() -> String {
    "sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string()
}

pub fn compute_record_hash(prev_hash: &str, event: &AuditEvent) -> Result<String, AuditLogError> {
    let payload = HashPayload { prev_hash, event };
    Ok(sha256_canonical_json(&payload)?)
}

pub struct AuditAppender {
    file: File,
    last_hash: String,
}

impl AuditAppender {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, AuditLogError> {
        let path = path.as_ref();
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self { file, last_hash: genesis_hash() })
    }

    pub fn with_last_hash(mut self, last_hash: String) -> Self {
        self.last_hash = last_hash;
        self
    }

    pub fn append(&mut self, event: AuditEvent) -> Result<AuditRecord, AuditLogError> {
        let prev_hash = self.last_hash.clone();
        let hash = compute_record_hash(&prev_hash, &event)?;
        let record = AuditRecord { prev_hash, hash: hash.clone(), event };
        let line = serde_json::to_string(&record)?;
        self.file.write_all(line.as_bytes())?;
        self.file.write_all(b"\n")?;
        self.file.flush()?;
        self.last_hash = hash;
        Ok(record)
    }
}

pub fn verify_log(path: impl AsRef<Path>) -> Result<String, AuditLogError> {
    let f = File::open(path)?;
    let reader = BufReader::new(f);
    let mut expected_prev = genesis_hash();

    for (idx, line) in reader.lines().enumerate() {
        let line_no = idx + 1;
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let rec: AuditRecord = serde_json::from_str(&line)?;
        if rec.prev_hash != expected_prev {
            return Err(AuditLogError::HashMismatch {
                line: line_no,
                expected: expected_prev,
                got: rec.prev_hash,
            });
        }
        let computed = compute_record_hash(&rec.prev_hash, &rec.event)?;
        if computed != rec.hash {
            return Err(AuditLogError::HashMismatch {
                line: line_no,
                expected: computed,
                got: rec.hash,
            });
        }
        expected_prev = rec.hash;
    }

    Ok(expected_prev)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pieBot_audit_spec::*;
    use std::fs;

    #[test]
    fn hash_chain_verifies() {
        let tmp = std::env::temp_dir().join("pieBot_audit_test.jsonl");
        let _ = fs::remove_file(&tmp);

        let mut app = AuditAppender::open(&tmp).unwrap();
        let call_id = CallId(uuid::Uuid::new_v4());

        let e1 = AuditEvent::ModelCallDispatched(ModelCallDispatched {
            schema_version: 1,
            run_id: RunId("r1".into()),
            tick_id: TickId(1),
            ts: 1.0,
            model_call: call_id.clone(),
            provider: "openai".into(),
            model: "m".into(),
            endpoint_fingerprint: "sha256:abc".into(),
            request_post_hash: "sha256:def".into(),
        });
        app.append(e1).unwrap();

        let last = verify_log(&tmp).unwrap();
        assert!(last.starts_with("sha256:"));
    }
}