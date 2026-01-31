//! pieBot_audit_spec
//!
//! Strongly-typed audit events for provider + redaction pipeline.
//! Mirrors Stage 6B event requirements:
//! - ModelCallPrepared
//! - ModelRequestRedacted
//! - ModelCallDispatched
//! - ModelCallCompleted
//! - OpenMemory query events
//! NOTE: schema_version increments are per-event, not global.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RunId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TickId(pub u64);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CallId(pub Uuid);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub r#type: String, // "artifact_ref"
    pub hash: String,   // sha256:...
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Planner,
    Executor,
    Critic,
    Summarizer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskClass {
    Read,
    Write,
    Exec,
    Network,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Actor {
    pub subsystem: String, // "models"
    pub backend: String,   // "openai" etc
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityPre {
    pub request_pre_hash: String, // sha256:...
    pub request_pre_size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityRedacted {
    pub request_pre_hash: String,
    pub request_post_hash: String,
    pub request_post_size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyMeta {
    pub decision_id: String,
    pub risk_class: RiskClass,
    pub requires_approval: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCallMeta {
    pub call_id: CallId,
    pub role: AgentRole,
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCallPrepared {
    pub schema_version: u8,
    pub run_id: RunId,
    pub tick_id: TickId,
    pub ts: f64,
    pub actor: Actor,
    pub model_call: ModelCallMeta,
    pub integrity: IntegrityPre,
    pub policy: PolicyMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionMeta {
    pub profile: String,              // "strict" etc
    pub transform_count: u64,
    pub transform_log_hash: String,   // sha256:...
    pub summary_budget_chars: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRequestRedacted {
    pub schema_version: u8,
    pub run_id: RunId,
    pub tick_id: TickId,
    pub ts: f64,
    pub model_call: CallId,
    pub redaction: RedactionMeta,
    pub integrity: IntegrityRedacted,
    pub artifacts: RedactionArtifacts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionArtifacts {
    pub pre_request_artifact: ArtifactRef,
    pub post_request_artifact: ArtifactRef,
    pub transform_log_artifact: ArtifactRef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCallDispatched {
    pub schema_version: u8,
    pub run_id: RunId,
    pub tick_id: TickId,
    pub ts: f64,
    pub model_call: CallId,
    pub provider: String,
    pub model: String,
    pub endpoint_fingerprint: String, // sha256:...
    pub request_post_hash: String,    // sha256:...
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallStatus {
    Ok,
    Error,
    Timeout,
    RateLimited,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCallResult {
    pub status: CallStatus,
    pub latency_ms: u64,
    pub provider_request_id_hash: String,
    pub response_hash: String,
    pub response_size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCallCompleted {
    pub schema_version: u8,
    pub run_id: RunId,
    pub tick_id: TickId,
    pub ts: f64,
    pub model_call: CallId,
    pub result: ModelCallResult,
    pub artifacts: CompletionArtifacts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionArtifacts {
    pub response_artifact: ArtifactRef,
    pub normalized_reply_artifact: ArtifactRef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum AuditEvent {
    ModelCallPrepared(ModelCallPrepared),
    ModelRequestRedacted(ModelRequestRedacted),
    ModelCallDispatched(ModelCallDispatched),
    ModelCallCompleted(ModelCallCompleted),
    EpisodeAppended(EpisodeAppended),
    EpisodeMirrorAttempted(EpisodeMirrorAttempted),
    EpisodeMirrored(EpisodeMirrored),
    EpisodeMirrorFailed(EpisodeMirrorFailed),
    EpisodeQueryPerformed(EpisodeQueryPerformed),
    EpisodeQueryFailed(EpisodeQueryFailed),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeAppended {
    pub schema_version: u8,
    pub run_id: RunId,
    pub tick_id: TickId,
    pub ts: f64,
    pub episode_id: Uuid,
    pub thread_id: String,
    pub tags: Vec<String>,
    pub title: String,
    /// Hash of the episode (sha256 of canonical JSON excluding hash field)
    pub episode_hash: String,
    /// Reference to the episode artifact bytes as stored/hashed
    pub episode_artifact: ArtifactRef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeMirrorAttempted {
    pub schema_version: u8,
    pub run_id: RunId,
    pub tick_id: TickId,
    pub ts: f64,
    pub episode_id: Uuid,
    pub episode_hash: String,
    pub target: String, // e.g. "openmemory"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeMirrored {
    pub schema_version: u8,
    pub run_id: RunId,
    pub tick_id: TickId,
    pub ts: f64,
    pub episode_id: Uuid,
    pub episode_hash: String,
    pub target: String,    // e.g. "openmemory"
    pub remote_id: String, // returned by OpenMemory
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeMirrorFailed {
    pub schema_version: u8,
    pub run_id: RunId,
    pub tick_id: TickId,
    pub ts: f64,
    pub episode_id: Uuid,
    pub episode_hash: String,
    pub target: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeQueryPerformed {
    pub schema_version: u8,
    pub run_id: RunId,
    pub tick_id: TickId,
    pub ts: f64,
    pub target: String,         // "openmemory"
    pub query_hash: String,     // sha256:... of UTF-8 query bytes
    pub query_len: u64,
    pub k: u32,
    pub user_id: Option<String>,
    pub alias: Option<String>,
    pub result_count: u32,
    pub response_hash: String,  // sha256:... (canonical json)
    pub response_artifact: ArtifactRef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeQueryFailed {
    pub schema_version: u8,
    pub run_id: RunId,
    pub tick_id: TickId,
    pub ts: f64,
    pub target: String,
    pub query_hash: String,
    pub query_len: u64,
    pub k: u32,
    pub user_id: Option<String>,
    pub alias: Option<String>,
    pub error: String,
}
