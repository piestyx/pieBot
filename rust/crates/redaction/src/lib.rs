//! pieBot_redaction
//!
//! Stage 6B security boundary:
//! - Build internal (unsafe) ModelRequest
//! - Deterministically redact to SanitizedModelRequest (safe outbound)
//! - Produce a transform log (artifact)
//! - Hash pre/post + log, write artifacts
//! - Emit audit events:
//!   - ModelCallPrepared
//!   - ModelRequestRedacted
//!
//! IMPORTANT:
//! - Redaction MUST be deterministic.
//! - Anything memory/tool/diff/file-content-like should be removed or replaced with hash refs by default.

use pie_audit_log::AuditAppender;
use pie_audit_spec as spec;
use pie_common::{canonical_json_bytes, sha256_bytes, sha256_canonical_json};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

// ----------------------------
// Errors
// ----------------------------

#[derive(Debug, Error)]
pub enum RedactionError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("canonical json error: {0}")]
    Canon(#[from] pie_common::CanonError),
    #[error("audit log error: {0}")]
    Audit(#[from] pie_audit_log::AuditLogError),
    #[error("invalid allowlist entry: {0}")]
    InvalidAllowlist(String),
}

// ----------------------------
// Request/Response primitives
// ----------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RunId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TickId(pub u64);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Planner,
    Executor,
    Critic,
    Summarizer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptMessage {
    pub role: String,   // "system"|"user"|"assistant"
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prompt {
    pub format: String, // "chat"
    pub messages: Vec<PromptMessage>,
    pub max_output_tokens: u64,
    pub temperature: f64,
    pub top_p: f64,
    pub stop: Vec<String>,
}

/// Internal, unsafe request (never outbound).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRequest {
    pub schema_version: u8,
    pub run_id: RunId,
    pub tick_id: TickId,
    pub role: AgentRole,
    pub provider: ProviderId,
    pub model: ModelId,

    pub prompt: Prompt,

    /// Potentially sensitive structured context the control plane may hold.
    /// This is allowed to exist internally, but must not leak outbound.
    #[serde(default)]
    pub context: serde_json::Value,
}

// ----------------------------
// Redaction outputs
// ----------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashRef {
    pub r#type: String, // "hash_ref"
    pub value: String,  // sha256:...
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextRefs {
    #[serde(default)]
    pub gsama: Vec<HashRef>,
    #[serde(default)]
    pub working_memory: Vec<HashRef>,
    #[serde(default)]
    pub openmemory: Vec<HashRef>,
    #[serde(default)]
    pub artifacts: Vec<HashRef>,
    #[serde(default)]
    pub files: Vec<HashRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransformKind {
    Drop,
    ReplaceWithHash,
    ReplaceWithRef,
    Summarize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformReplacement {
    pub r#type: String, // "hash_ref" etc
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionTransform {
    pub kind: TransformKind,
    pub path: String,   // deterministic JSON-ish pointer (simple)
    pub reason: String, // stable reason key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement: Option<TransformReplacement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionBlock {
    pub policy_id: String,
    pub profile: String, // "strict"|"explicit_allowlist"
    pub summary_budget_chars: u64,
    pub transform_log: Vec<RedactionTransform>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityBlock {
    pub pre_hash: String,
    pub post_hash: String,
    pub nonce: String,
}

/// Safe outbound request. This is the only thing you send to a provider backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanitizedModelRequest {
    pub schema_version: u8,
    pub run_id: RunId,
    pub tick_id: TickId,
    pub role: AgentRole,
    pub provider: ProviderId,
    pub model: ModelId,

    pub prompt: Prompt,
    pub context_refs: ContextRefs,
    pub redaction: RedactionBlock,
    pub integrity: IntegrityBlock,
}

#[derive(Debug, Clone)]
pub struct ArtifactBundle {
    pub pre_request_path: PathBuf,
    pub post_request_path: PathBuf,
    pub transform_log_path: PathBuf,
    pub pre_request_hash: String,
    pub post_request_hash: String,
    pub transform_log_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallManifest {
    pub schema_version: u8,
    pub call_id: String,
    pub pre_hash: String,
    pub post_hash: String,
    pub transform_log_hash: String,
}

#[derive(Debug, Clone)]
pub struct RedactionResult {
    pub call_id: Uuid,
    pub sanitized: SanitizedModelRequest,
    pub artifacts: ArtifactBundle,
}

// ----------------------------
// Profiles + allowlist
// ----------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionAllowlist {
    /// Explicit allowlist of JSON pointer-ish paths inside `context` that may be copied outbound.
    /// Keep this boring. No glob. No regex.
    #[serde(default)]
    pub context_paths: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum RedactionProfile {
    Strict,
    ExplicitAllowlist(RedactionAllowlist),
}

impl RedactionProfile {
    pub fn name(&self) -> &'static str {
        match self {
            RedactionProfile::Strict => "strict",
            RedactionProfile::ExplicitAllowlist(_) => "explicit_allowlist",
        }
    }
}

// ----------------------------
// Artifact writing
// ----------------------------

fn ensure_dir(p: &Path) -> Result<(), RedactionError> {
    fs::create_dir_all(p)?;
    Ok(())
}

fn write_json_artifact(path: &Path, value: &impl Serialize) -> Result<(String, u64), RedactionError> {
    let bytes = canonical_json_bytes(value)?;
    ensure_dir(path.parent().unwrap_or_else(|| Path::new(".")))?;
    fs::write(path, &bytes)?;
    let h = sha256_bytes(&bytes);
    Ok((h, bytes.len() as u64))
}

fn models_artifact_dir(base: &Path, run_id: &RunId, call_id: &Uuid) -> PathBuf {
    base.join("runtime")
        .join("artifacts")
        .join("models")
        .join(&run_id.0)
        .join(call_id.to_string())
}

// ----------------------------
// Redaction engine
// ----------------------------

pub struct RedactionEngine {
    pub policy_id: String,
    pub profile: RedactionProfile,
    pub summary_budget_chars: u64,
}

impl RedactionEngine {
    pub fn new(policy_id: String, profile: RedactionProfile, summary_budget_chars: u64) -> Self {
        Self { policy_id, profile, summary_budget_chars }
    }

    /// Perform redaction + write artifacts + emit audit events.
    ///
    /// `repo_root` is the project root where `runtime/` exists.
    pub fn redact_and_audit(
        &self,
        repo_root: &Path,
        audit: &mut AuditAppender,
        request: &ModelRequest,
        // These feed into ModelCallPrepared’s policy metadata
        policy_decision_id: String,
        requires_approval: bool,
        ts_prepared: f64,
        ts_redacted: f64,
    ) -> Result<RedactionResult, RedactionError> {
        let call_id = Uuid::new_v4();

        // 1) Hash + artifact pre request
        let pre_hash = sha256_canonical_json(request)?;
        let artifacts_dir = models_artifact_dir(repo_root, &request.run_id, &call_id);
        ensure_dir(&artifacts_dir)?;

        let pre_path = artifacts_dir.join("request_pre.json");
        let (pre_artifact_hash, pre_size) = write_json_artifact(&pre_path, request)?;
        // sanity: artifact hash should match the canonical hash we computed
        // Not strictly required, but indicates canonicalization matches.
        let _ = (pre_hash.clone(), pre_artifact_hash.clone(), pre_size);

        // 2) Redact to sanitized request + transforms
        let (sanitized, transforms, context_refs) = self.redact_request(request)?;

        // 3) Compute post hash + write post + transform log artifacts
        let post_hash = sha256_canonical_json(&sanitized)?;
        let post_path = artifacts_dir.join("request_post.json");
        let (post_artifact_hash, _post_size) = write_json_artifact(&post_path, &sanitized)?;

        let transform_log_path = artifacts_dir.join("transform_log.json");
        let (transform_log_hash, _log_size) = write_json_artifact(&transform_log_path, &transforms)?;

        // Write call manifest for ergonomic downstream dispatch
        let manifest = CallManifest {
            schema_version: 1,
            call_id: call_id.to_string(),
            pre_hash: pre_hash.clone(),
            post_hash: post_hash.clone(),
            transform_log_hash: transform_log_hash.clone(),
        };
        let _ = write_json_artifact(&artifacts_dir.join("call_manifest.json"), &manifest)?;

        // 3.1) Patch integrity block in-memory (authoritative hashes)
        // NOTE: artifacts already written above; this only corrects the returned struct and
        // any future outbound use of the SanitizedModelRequest object.
        let mut sanitized_fixed = sanitized.clone();
        sanitized_fixed.integrity.pre_hash = pre_hash.clone();
        sanitized_fixed.integrity.post_hash = post_hash.clone();
        
        // 4) Emit audit: ModelCallPrepared
        let prepared = spec::AuditEvent::ModelCallPrepared(spec::ModelCallPrepared {
            schema_version: 1,
            run_id: spec::RunId(request.run_id.0.clone()),
            tick_id: spec::TickId(request.tick_id.0),
            ts: ts_prepared,
            actor: spec::Actor { subsystem: "models".into(), backend: request.provider.0.clone() },
            model_call: spec::ModelCallMeta {
                call_id: spec::CallId(call_id),
                role: map_role(&request.role),
                provider: request.provider.0.clone(),
                model: request.model.0.clone(),
            },
            integrity: spec::IntegrityPre {
                request_pre_hash: pre_hash.clone(),
                request_pre_size_bytes: pre_size,
            },
            policy: spec::PolicyMeta {
                decision_id: policy_decision_id,
                risk_class: spec::RiskClass::Network,
                requires_approval,
            },
        });
        audit.append(prepared)?;

        // 5) Emit audit: ModelRequestRedacted
        let redacted_evt = spec::AuditEvent::ModelRequestRedacted(spec::ModelRequestRedacted {
            schema_version: 1,
            run_id: spec::RunId(request.run_id.0.clone()),
            tick_id: spec::TickId(request.tick_id.0),
            ts: ts_redacted,
            model_call: spec::CallId(call_id),
            redaction: spec::RedactionMeta {
                profile: self.profile.name().into(),
                transform_count: transforms.len() as u64,
                transform_log_hash: transform_log_hash.clone(),
                summary_budget_chars: self.summary_budget_chars,
            },
            integrity: spec::IntegrityRedacted {
                request_pre_hash: pre_hash.clone(),
                request_post_hash: post_hash.clone(),
                request_post_size_bytes: canonical_json_bytes(&sanitized)?.len() as u64,
            },
            artifacts: spec::RedactionArtifacts {
                pre_request_artifact: spec::ArtifactRef { r#type: "artifact_ref".into(), hash: pre_artifact_hash.clone() },
                post_request_artifact: spec::ArtifactRef { r#type: "artifact_ref".into(), hash: post_artifact_hash.clone() },
                transform_log_artifact: spec::ArtifactRef { r#type: "artifact_ref".into(), hash: transform_log_hash.clone() },
            },
        });
        audit.append(redacted_evt)?;

        // Return result
        let bundle = ArtifactBundle {
            pre_request_path: pre_path,
            post_request_path: post_path,
            transform_log_path,
            pre_request_hash: pre_hash,
            post_request_hash: post_hash,
            transform_log_hash,
        };

        // Ensure sanitized includes the derived context_refs (deterministically)
        let mut sanitized_out = sanitized_fixed;
        sanitized_out.context_refs = context_refs;

        Ok(RedactionResult {
            call_id,
            sanitized: sanitized_out,
            artifacts: bundle,
        })
    }

    fn redact_request(
        &self,
        request: &ModelRequest,
    ) -> Result<(SanitizedModelRequest, Vec<RedactionTransform>, ContextRefs), RedactionError> {
        let mut transforms: Vec<RedactionTransform> = Vec::new();
        let mut refs = ContextRefs {
            gsama: vec![],
            working_memory: vec![],
            openmemory: vec![],
            artifacts: vec![],
            files: vec![],
        };

        // Default: context is never copied outbound.
        // We instead derive a deterministic hash reference for any "interesting" top-level keys.
        //
        // For now we treat these top-level context keys as sensitive by definition:
        // - "gsama"
        // - "working_memory"
        // - "openmemory"
        // - "tool_results"
        // - "diff"
        // - "files"
        //
        // Anything else: drop and record a hash ref to the whole context object.
        let ctx = &request.context;

        // Always hash the whole context so you can prove what was omitted without leaking it.
        let ctx_bytes = canonical_json_bytes(ctx)?;
        let ctx_hash = sha256_bytes(&ctx_bytes);
        transforms.push(RedactionTransform {
            kind: TransformKind::ReplaceWithHash,
            path: "context".into(),
            reason: "context_omitted".into(),
            replacement: Some(TransformReplacement { r#type: "hash_ref".into(), value: ctx_hash.clone() }),
        });

        // Also extract hash refs for known sensitive buckets if present.
        // This supports later policy-based allowlists without ever sending content.
        if let Some(obj) = ctx.as_object() {
            for (k, v) in obj.iter() {
                let h = sha256_bytes(&canonical_json_bytes(v)?);
                let href = HashRef { r#type: "hash_ref".into(), value: h.clone() };

                match k.as_str() {
                    "gsama" => refs.gsama.push(href),
                    "working_memory" => refs.working_memory.push(href),
                    "openmemory" => refs.openmemory.push(href),
                    "tool_results" | "tool_result" => refs.artifacts.push(href),
                    "diff" | "diffs" => refs.artifacts.push(href),
                    "files" | "file" => refs.files.push(href),
                    _ => {
                        // Unknown context bucket: treat as generic artifact ref (still not outbound content)
                        refs.artifacts.push(href);
                    }
                }

                transforms.push(RedactionTransform {
                    kind: TransformKind::ReplaceWithHash,
                    path: format!("context.{}", k),
                    reason: "context_bucket_hashed".into(),
                    replacement: Some(TransformReplacement { r#type: "hash_ref".into(), value: h }),
                });
            }
        }

        // If explicit allowlist is set, we may copy specific context paths outbound.
        // NOTE: We still record transforms for any copied paths.
        let mut allow_copied: Vec<(String, serde_json::Value)> = vec![];
        if let RedactionProfile::ExplicitAllowlist(allow) = &self.profile {
            for p in allow.context_paths.iter() {
                let v = get_by_simple_path(ctx, p)
                    .ok_or_else(|| RedactionError::InvalidAllowlist(p.clone()))?;
                allow_copied.push((p.clone(), v.clone()));
                transforms.push(RedactionTransform {
                    kind: TransformKind::ReplaceWithRef,
                    path: format!("context.{}", p),
                    reason: "explicit_allowlist_copied".into(),
                    replacement: None,
                });
            }
        }

        // Build outbound prompt:
        // For now, we DO NOT attempt semantic scanning; we keep it structural and deterministic.
        // Any sensitive content should be kept out of the prompt projection upstream.
        // We still defensively hash-replace any message that is extremely large (likely a dump).
        let mut prompt = request.prompt.clone();
        for (i, msg) in prompt.messages.iter_mut().enumerate() {
            if msg.content.len() > (self.summary_budget_chars as usize) {
                let h = sha256_bytes(msg.content.as_bytes());
                msg.content = format!("<redacted:large_message {}>", h);
                transforms.push(RedactionTransform {
                    kind: TransformKind::ReplaceWithHash,
                    path: format!("prompt.messages[{}].content", i),
                    reason: "message_too_large_hashed".into(),
                    replacement: Some(TransformReplacement { r#type: "hash_ref".into(), value: h }),
                });
            }
        }

        // Nonce is deterministic per run/tick/provider/model (no randomness).
        // This prevents “helpful” provider retries from being indistinguishable.
        let nonce_material = format!(
            "run:{}|tick:{}|role:{:?}|provider:{}|model:{}|policy:{}",
            request.run_id.0, request.tick_id.0, request.role, request.provider.0, request.model.0, self.policy_id
        );
        let nonce = sha256_bytes(nonce_material.as_bytes());

        // Build sanitized request (context is not embedded; only refs + redaction log)
        let mut sanitized = SanitizedModelRequest {
            schema_version: 1,
            run_id: request.run_id.clone(),
            tick_id: request.tick_id.clone(),
            role: request.role.clone(),
            provider: request.provider.clone(),
            model: request.model.clone(),
            prompt,
            context_refs: refs.clone(),
            redaction: RedactionBlock {
                policy_id: self.policy_id.clone(),
                profile: self.profile.name().into(),
                summary_budget_chars: self.summary_budget_chars,
                transform_log: vec![], // filled below
            },
            integrity: IntegrityBlock {
                pre_hash: "sha256:pending".into(),
                post_hash: "sha256:pending".into(),
                nonce,
            },
        };

        // Set transform log directly in sanitized (but also returned separately for artifact writing)
        sanitized.redaction.transform_log = transforms.clone();

        // If allowlist copied values exist, we still do NOT include them directly in this struct,
        // because the outbound schema you specified uses refs only.
        //
        // If you later want to permit copying, it should go into prompt messages as bounded summaries
        // or as separate, explicitly policy-approved attachments with size limits.
        //
        // For now: record that they were selected, but do not embed.
        if !allow_copied.is_empty() {
            transforms.push(RedactionTransform {
                kind: TransformKind::Drop,
                path: "context.allowlist_copied_values".into(),
                reason: "allowlist_copy_not_embedded_refs_only".into(),
                replacement: None,
            });
        }

        // Fill integrity hashes later in redact_and_audit (we need pre/post)
        Ok((sanitized, transforms, refs))
    }
}

fn map_role(r: &AgentRole) -> spec::AgentRole {
    match r {
        AgentRole::Planner => spec::AgentRole::Planner,
        AgentRole::Executor => spec::AgentRole::Executor,
        AgentRole::Critic => spec::AgentRole::Critic,
        AgentRole::Summarizer => spec::AgentRole::Summarizer,
    }
}

/// Very simple dotted path accessor for allowlists:
/// - "a.b.c"
/// Only supports objects (no arrays).
fn get_by_simple_path<'a>(root: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut cur = root;
    if path.trim().is_empty() {
        return Some(root);
    }
    for seg in path.split('.') {
        if seg.is_empty() {
            return None;
        }
        cur = cur.get(seg)?;
    }
    Some(cur)
}

// ----------------------------
// Tests
// ----------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use pie_audit_log::verify_log;
    use std::fs;

    fn tmp_root() -> PathBuf {
        std::env::temp_dir().join("pie_redaction_repo_root")
    }

    #[test]
    fn redaction_is_deterministic_for_same_input() {
        let root = tmp_root();
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("runtime/logs")).unwrap();

        let mut audit = AuditAppender::open(root.join("runtime/logs/audit_rust.jsonl")).unwrap();

        let req = ModelRequest {
            schema_version: 1,
            run_id: RunId("run1".into()),
            tick_id: TickId(1),
            role: AgentRole::Planner,
            provider: ProviderId("openai".into()),
            model: ModelId("gpt".into()),
            prompt: Prompt {
                format: "chat".into(),
                messages: vec![
                    PromptMessage { role: "system".into(), content: "sys".into() },
                    PromptMessage { role: "user".into(), content: "hello".into() },
                ],
                max_output_tokens: 64,
                temperature: 0.2,
                top_p: 1.0,
                stop: vec![],
            },
            context: serde_json::json!({
                "gsama": { "z": [1,2,3] },
                "working_memory": { "secret": "dont leak" },
                "diff": "diff --git a/x b/x"
            }),
        };

        let eng = RedactionEngine::new("policy123".into(), RedactionProfile::Strict, 1200);

        let r1 = eng.redact_and_audit(
            &root,
            &mut audit,
            &req,
            "pol_dec_1".into(),
            true,
            1.0,
            2.0,
        ).unwrap();

        // Same input again should produce same *sanitized* post-hash (call_id differs, but payload does not)
        let r2 = eng.redact_and_audit(
            &root,
            &mut audit,
            &req,
            "pol_dec_1".into(),
            true,
            3.0,
            4.0,
        ).unwrap();

        assert_eq!(r1.artifacts.post_request_hash, r2.artifacts.post_request_hash);

        // Verify audit chain integrity
        let last = verify_log(root.join("runtime/logs/audit_rust.jsonl")).unwrap();
        assert!(last.starts_with("sha256:"));
    }

    #[test]
    fn large_message_is_hashed() {
        let req = ModelRequest {
            schema_version: 1,
            run_id: RunId("run1".into()),
            tick_id: TickId(1),
            role: AgentRole::Planner,
            provider: ProviderId("openai".into()),
            model: ModelId("gpt".into()),
            prompt: Prompt {
                format: "chat".into(),
                messages: vec![
                    PromptMessage { role: "user".into(), content: "x".repeat(2000) },
                ],
                max_output_tokens: 64,
                temperature: 0.2,
                top_p: 1.0,
                stop: vec![],
            },
            context: serde_json::json!({}),
        };

        let eng = RedactionEngine::new("policy123".into(), RedactionProfile::Strict, 1200);
        let (san, transforms, _refs) = eng.redact_request(&req).unwrap();
        assert!(san.prompt.messages[0].content.starts_with("<redacted:large_message "));
        assert!(transforms.iter().any(|t| t.reason == "message_too_large_hashed"));
    }
}
