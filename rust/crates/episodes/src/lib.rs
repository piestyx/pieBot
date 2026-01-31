//! Stage 7A — Episodic Memory (authoritative, deterministic, provider-agnostic)
//!
//! Storage layout (repo root):
//!   runtime/memory/episodes/
//!     episodes.jsonl   (append-only)
//!     index.json       (deterministic index, rewritten canonically)
//!
//! NOTE:
//! - This store is authoritative.
//! - External OpenMemory services are optional and must mirror from here later (Stage 7C).

use pie_common::{canonical_json_bytes, sha256_canonical_json, CanonError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{PathBuf};
use thiserror::Error;
use uuid::Uuid;

// ----------------------------
// Schema
// ----------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct RunId(pub String);

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(transparent)]
pub struct TickId(pub u64);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRef {
    /// sha256:... or other hash ids (future)
    pub hash: String,
    /// optional type hint (e.g. "audit_event", "tool_result", "diff")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub schema_version: u8,
    pub episode_id: Uuid,
    pub run_id: RunId,
    pub tick_id: TickId,
    pub thread_id: String,
    pub tags: Vec<String>,
    pub title: String,
    pub summary: String,
    #[serde(default)]
    pub artifacts: Vec<ArtifactRef>,
    /// unix seconds or monotonic seconds; caller decides. Stored verbatim.
    pub created_ts: f64,
    /// sha256 of canonical JSON excluding this `hash` field.
    pub hash: String,
}

// Internal struct used only for hash computation (excludes `hash`)
#[derive(Debug, Clone, Serialize)]
struct EpisodeUnsigned<'a> {
    schema_version: u8,
    episode_id: &'a Uuid,
    run_id: &'a RunId,
    tick_id: TickId,
    thread_id: &'a str,
    tags: &'a [String],
    title: &'a str,
    summary: &'a str,
    artifacts: &'a [ArtifactRef],
    created_ts: f64,
}

impl Episode {
    /// Create an episode with deterministic hashing.
    /// Caller is responsible for enforcing summary/title length caps at policy layer.
    pub fn new(
        run_id: RunId,
        tick_id: TickId,
        thread_id: impl Into<String>,
        tags: Vec<String>,
        title: impl Into<String>,
        summary: impl Into<String>,
        artifacts: Vec<ArtifactRef>,
        created_ts: f64,
    ) -> Result<Self, EpisodeError> {
        let episode_id = Uuid::new_v4();
        let thread_id = thread_id.into();
        let title = title.into();
        let summary = summary.into();

        let unsigned = EpisodeUnsigned {
            schema_version: 1,
            episode_id: &episode_id,
            run_id: &run_id,
            tick_id,
            thread_id: &thread_id,
            tags: &tags,
            title: &title,
            summary: &summary,
            artifacts: &artifacts,
            created_ts,
        };

        let hash = sha256_canonical_json(&unsigned)?;

        Ok(Episode {
            schema_version: 1,
            episode_id,
            run_id,
            tick_id,
            thread_id,
            tags,
            title,
            summary,
            artifacts,
            created_ts,
            hash,
        })
    }

    /// Recompute expected hash and verify integrity.
    pub fn verify_hash(&self) -> Result<(), EpisodeError> {
        let unsigned = EpisodeUnsigned {
            schema_version: self.schema_version,
            episode_id: &self.episode_id,
            run_id: &self.run_id,
            tick_id: self.tick_id,
            thread_id: &self.thread_id,
            tags: &self.tags,
            title: &self.title,
            summary: &self.summary,
            artifacts: &self.artifacts,
            created_ts: self.created_ts,
        };
        let expected = sha256_canonical_json(&unsigned)?;
        if expected != self.hash {
            return Err(EpisodeError::HashMismatch {
                expected,
                got: self.hash.clone(),
            });
        }
        Ok(())
    }
}

// ----------------------------
// Store + Index
// ----------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeIndexEntry {
    pub episode_id: Uuid,
    pub run_id: RunId,
    pub tick_id: TickId,
    pub thread_id: String,
    pub tags: Vec<String>,
    pub hash: String,
    /// Line number in episodes.jsonl (0-based). Deterministic, stable on append.
    pub line_no: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EpisodeIndex {
    pub schema_version: u8,
    pub entries: Vec<EpisodeIndexEntry>,
}

#[derive(Debug, Error)]
pub enum EpisodeError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("canonical json error: {0}")]
    Canon(#[from] CanonError),
    #[error("hash mismatch: expected {expected}, got {got}")]
    HashMismatch { expected: String, got: String },
    #[error("store corruption: {0}")]
    Corrupt(String),
}

pub struct EpisodeStore {
    repo_root: PathBuf,
}

impl EpisodeStore {
    pub fn new(repo_root: impl Into<PathBuf>) -> Self {
        Self { repo_root: repo_root.into() }
    }

    pub fn base_dir(&self) -> PathBuf {
        self.repo_root.join("runtime").join("memory").join("episodes")
    }

    pub fn episodes_path(&self) -> PathBuf {
        self.base_dir().join("episodes.jsonl")
    }

    pub fn index_path(&self) -> PathBuf {
        self.base_dir().join("index.json")
    }

    pub fn ensure_dirs(&self) -> Result<(), EpisodeError> {
        fs::create_dir_all(self.base_dir())?;
        Ok(())
    }

    pub fn load_index(&self) -> Result<EpisodeIndex, EpisodeError> {
        self.ensure_dirs()?;
        let p = self.index_path();
        if !p.exists() {
            return Ok(EpisodeIndex { schema_version: 1, entries: vec![] });
        }
        let bytes = fs::read(p)?;
        let idx: EpisodeIndex = serde_json::from_slice(&bytes)?;
        Ok(idx)
    }

    fn write_index(&self, idx: &EpisodeIndex) -> Result<(), EpisodeError> {
        self.ensure_dirs()?;
        let bytes = canonical_json_bytes(idx)?;
        fs::write(self.index_path(), bytes)?;
        Ok(())
    }

    fn current_line_count(&self) -> Result<u64, EpisodeError> {
        let p = self.episodes_path();
        if !p.exists() {
            return Ok(0);
        }
        let f = fs::File::open(p)?;
        let reader = BufReader::new(f);
        Ok(reader.lines().count() as u64)
    }

    /// Append an episode (authoritative).
    /// - Verifies episode hash
    /// - Appends JSONL line
    /// - Updates index deterministically
    pub fn append(&self, ep: &Episode) -> Result<(), EpisodeError> {
        self.ensure_dirs()?;
        ep.verify_hash()?;

        let line_no = self.current_line_count()?;
        let ep_bytes = canonical_json_bytes(ep)?;

        // Append to JSONL
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.episodes_path())?;
        f.write_all(&ep_bytes)?;
        f.write_all(b"\n")?;
        f.flush()?;

        // Update index
        let mut idx = self.load_index()?;
        if idx.schema_version == 0 {
            idx.schema_version = 1;
        }
        idx.entries.push(EpisodeIndexEntry {
            episode_id: ep.episode_id,
            run_id: ep.run_id.clone(),
            tick_id: ep.tick_id,
            thread_id: ep.thread_id.clone(),
            tags: ep.tags.clone(),
            hash: ep.hash.clone(),
            line_no,
        });
        self.write_index(&idx)?;
        Ok(())
    }

    /// Deterministic query (Stage 7B later can add richer options, but this covers 7A baseline)
    ///
    /// Filters:
    /// - thread_id (optional)
    /// - tags (must include all provided tags)
    /// - since_tick (inclusive)
    /// - limit (max results)
    ///
    /// Ordering:
    /// - by tick_id asc, then line_no asc (stable)
    pub fn query(
        &self,
        thread_id: Option<&str>,
        tags_all: &[String],
        since_tick: Option<TickId>,
        limit: usize,
    ) -> Result<Vec<EpisodeIndexEntry>, EpisodeError> {
        let idx = self.load_index()?;
        let mut out: Vec<EpisodeIndexEntry> = idx
            .entries
            .into_iter()
            .filter(|e| {
                if let Some(t) = thread_id {
                    if e.thread_id != t {
                        return false;
                    }
                }
                if let Some(st) = since_tick {
                    if e.tick_id < st {
                        return false;
                    }
                }
                // tags_all must be subset of entry tags
                for want in tags_all {
                    if !e.tags.iter().any(|x| x == want) {
                        return false;
                    }
                }
                true
            })
            .collect();

        out.sort_by(|a, b| {
            a.tick_id
                .cmp(&b.tick_id)
                .then_with(|| a.line_no.cmp(&b.line_no))
        });

        if out.len() > limit {
            out.truncate(limit);
        }
        Ok(out)
    }

    /// Load a full episode by index entry.
    /// This is deterministic because we reference by line_no and verify the hash.
    pub fn load_episode_by_entry(&self, entry: &EpisodeIndexEntry) -> Result<Episode, EpisodeError> {
        let p = self.episodes_path();
        if !p.exists() {
            return Err(EpisodeError::Corrupt("episodes.jsonl missing".into()));
        }
        let f = fs::File::open(p)?;
        let reader = BufReader::new(f);
        let line = reader
            .lines()
            .nth(entry.line_no as usize)
            .ok_or_else(|| EpisodeError::Corrupt(format!("missing line {}", entry.line_no)))??;

        let ep: Episode = serde_json::from_str(&line)?;
        ep.verify_hash()?;
        if ep.hash != entry.hash {
            return Err(EpisodeError::HashMismatch {
                expected: entry.hash.clone(),
                got: ep.hash.clone(),
            });
        }
        Ok(ep)
    }
}

// ----------------------------
// Tests
// ----------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn store_in_tmp() -> (TempDir, EpisodeStore) {
        let td = TempDir::new().unwrap();
        let store = EpisodeStore::new(td.path().to_path_buf());
        (td, store)
    }

    #[test]
    fn episode_hash_is_deterministic_for_same_content_except_id() {
        // Different episode_id is expected to change hash.
        let e1 = Episode::new(
            RunId("run_demo".into()),
            TickId(1),
            "main",
            vec!["tag:a".into()],
            "t",
            "s",
            vec![],
            1.0,
        )
        .unwrap();

        let e2 = Episode::new(
            RunId("run_demo".into()),
            TickId(1),
            "main",
            vec!["tag:a".into()],
            "t",
            "s",
            vec![],
            1.0,
        )
        .unwrap();

        assert_ne!(e1.episode_id, e2.episode_id);
        assert_ne!(e1.hash, e2.hash);
        e1.verify_hash().unwrap();
        e2.verify_hash().unwrap();
    }

    #[test]
    fn append_writes_jsonl_and_index_and_query_is_deterministic() {
        let (_td, store) = store_in_tmp();

        let e1 = Episode::new(
            RunId("run_demo".into()),
            TickId(1),
            "main",
            vec!["thread:main".into(), "role:planner".into()],
            "tick1",
            "first",
            vec![],
            10.0,
        )
        .unwrap();
        let e2 = Episode::new(
            RunId("run_demo".into()),
            TickId(2),
            "main",
            vec!["thread:main".into(), "role:planner".into()],
            "tick2",
            "second",
            vec![],
            11.0,
        )
        .unwrap();

        store.append(&e1).unwrap();
        store.append(&e2).unwrap();

        // query by thread + tag
        let q = store
            .query(Some("main"), &vec!["role:planner".into()], Some(TickId(1)), 10)
            .unwrap();
        assert_eq!(q.len(), 2);
        assert!(q[0].tick_id <= q[1].tick_id);

        // load by entry verifies hash
        let full = store.load_episode_by_entry(&q[0]).unwrap();
        assert_eq!(full.thread_id, "main");
    }
}