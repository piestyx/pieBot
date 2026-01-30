use clap::{Parser, Subcommand};
use pie_audit_log::{verify_log, AuditAppender};
use pie_common::sha256_bytes;
use pie_redaction::{ModelRequest, RedactionEngine, RedactionProfile, SanitizedModelRequest, CallManifest};
use pie_audit_spec as spec;
use pie_providers::{OpenAICompatProvider, Provider};
use std::time::Instant;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
enum CliError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("canonical json error: {0}")]
    Canon(#[from] pie_common::CanonError),
    #[error("redaction error: {0}")]
    Redaction(#[from] pie_redaction::RedactionError),
    #[error("audit error: {0}")]
    Audit(#[from] pie_audit_log::AuditLogError),
    #[error("provider error: {0}")]
    Provider(#[from] pie_providers::ProviderError),
}

#[derive(Parser)]
#[command(name = "pie-control", version, about = "pieBot Rust control-plane utilities (6B boundary)")]
struct Args {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Redact a ModelRequest JSON (internal) into sanitized artifacts and emit audit events.
    ///
    /// This is the Stage 6B boundary in practice.
    RedactOnly {
        /// Repo root containing runtime/
        #[arg(long)]
        repo_root: PathBuf,

        /// Path to ModelRequest JSON file (internal/unsafe)
        #[arg(long)]
        request_json: PathBuf,

        /// Audit log JSONL path to append to (e.g. runtime/logs/audit_rust.jsonl)
        #[arg(long)]
        audit_log: PathBuf,

        /// Policy decision id (string) associated with NETWORK/tool decision
        #[arg(long, default_value = "policy_decision_unspecified")]
        policy_decision_id: String,

        /// Whether approval is required to dispatch (recorded in audit only)
        #[arg(long, default_value_t = true)]
        requires_approval: bool,

        /// Policy id used inside redaction block
        #[arg(long, default_value = "policy_unspecified")]
        policy_id: String,

        /// Redaction profile: "strict" or "explicit_allowlist"
        #[arg(long, default_value = "strict")]
        profile: String,

        /// Summary budget chars (used for size-based hashing in prompt)
        #[arg(long, default_value_t = 1200)]
        summary_budget_chars: u64,

        /// Timestamp for ModelCallPrepared (float seconds)
        #[arg(long, default_value_t = 0.0)]
        ts_prepared: f64,

        /// Timestamp for ModelRequestRedacted (float seconds)
        #[arg(long, default_value_t = 0.0)]
        ts_redacted: f64,
    },

    /// Dispatch a call by pointing at the call directory created by redact-only.
    /// This reads:
    /// - call_manifest.json
    /// - request_post.json
    DispatchDir {
        #[arg(long)]
        repo_root: PathBuf,

        /// Directory runtime/artifacts/models/<run>/<call>/
        #[arg(long)]
        call_dir: PathBuf,

        #[arg(long)]
        audit_log: PathBuf,

        #[arg(long)]
        base_url: Option<String>,

        #[arg(long)]
        api_key: Option<String>,

        #[arg(long, default_value_t = 0.0)]
        ts_dispatched: f64,

        #[arg(long, default_value_t = 0.0)]
        ts_completed: f64,
    },

    Dispatch {
        /// Repo root containing runtime/
        #[arg(long)]
        repo_root: PathBuf,

        /// Path to SanitizedModelRequest JSON (typically runtime/artifacts/models/<run>/<call>/request_post.json)
        #[arg(long)]
        sanitized_json: PathBuf,

        /// Audit log JSONL path to append to
        #[arg(long)]
        audit_log: PathBuf,

        /// Provider base URL (e.g. http://localhost:8000 or https://api.openai.com)
        /// Can be supplied via env PIEBOT_PROVIDER_BASE_URL.
        #[arg(long)]
        base_url: Option<String>,

        /// API key (optional). Can be supplied via env PIEBOT_PROVIDER_API_KEY.
        #[arg(long)]
        api_key: Option<String>,

        /// Call id (UUID) that matches the artifacts folder; used for audit linkage + artifact placement.
        #[arg(long)]
        call_id: String,

        /// Timestamp for ModelCallDispatched
        #[arg(long, default_value_t = 0.0)]
        ts_dispatched: f64,

        /// Timestamp for ModelCallCompleted
        #[arg(long, default_value_t = 0.0)]
        ts_completed: f64,
    },

    /// Verify a hash-chained audit log JSONL and print final hash.
    VerifyAudit {
        #[arg(long)]
        audit_log: PathBuf,
    },
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("ERROR: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), CliError> {
    let args = Args::parse();
    match args.cmd {
        Command::VerifyAudit { audit_log } => {
            let last = verify_log(audit_log)?;
            println!("{last}");
            Ok(())
        }
        Command::RedactOnly {
            repo_root,
            request_json,
            audit_log,
            policy_decision_id,
            requires_approval,
            policy_id,
            profile,
            summary_budget_chars,
            ts_prepared,
            ts_redacted,
        } => {
            ensure_runtime_dirs(&repo_root)?;

            let bytes = fs::read(&request_json)?;
            let req: ModelRequest = serde_json::from_slice(&bytes)?;

            let mut audit = AuditAppender::open(&audit_log)?;

            let prof = match profile.as_str() {
                "strict" => RedactionProfile::Strict,
                "explicit_allowlist" => RedactionProfile::ExplicitAllowlist(
                    // Keep empty for now (refs-only boundary). Expand later if needed.
                    pie_redaction::RedactionAllowlist { context_paths: vec![] },
                ),
                other => {
                    return Err(CliError::Redaction(pie_redaction::RedactionError::InvalidAllowlist(
                        format!("unknown profile: {other}"),
                    )))
                }
            };

            let engine = RedactionEngine::new(policy_id, prof, summary_budget_chars);

            let result = engine.redact_and_audit(
                &repo_root,
                &mut audit,
                &req,
                policy_decision_id,
                requires_approval,
                ts_prepared,
                ts_redacted,
            )?;

            // Print useful outputs for scripting
            println!(
                "{{\"call_id\":\"{}\",\"pre_hash\":\"{}\",\"post_hash\":\"{}\",\"transform_log_hash\":\"{}\"}}",
                result.call_id,
                result.artifacts.pre_request_hash,
                result.artifacts.post_request_hash,
                result.artifacts.transform_log_hash
            );
            Ok(())
        }
        Command::DispatchDir {
            repo_root,
            call_dir,
            audit_log,
            base_url,
            api_key,
            ts_dispatched,
            ts_completed,
        } => {
            ensure_runtime_dirs(&repo_root)?;
            let manifest_path = call_dir.join("call_manifest.json");
            let post_path = call_dir.join("request_post.json");

            let mbytes = fs::read(&manifest_path)?;
            let manifest: CallManifest = serde_json::from_slice(&mbytes)?;

            // Delegate to Dispatch logic by reusing the same code path:
            // We just reconstruct args inline.
            let base_url = base_url
                .or_else(|| std::env::var("PIEBOT_PROVIDER_BASE_URL").ok())
                .unwrap_or_else(|| "https://api.openai.com".to_string());
            let api_key = api_key.or_else(|| std::env::var("PIEBOT_PROVIDER_API_KEY").ok());

            let bytes = fs::read(&post_path)?;
            let req: SanitizedModelRequest = serde_json::from_slice(&bytes)?;

            let call_uuid = Uuid::parse_str(&manifest.call_id)
                .map_err(|_| CliError::Provider(pie_providers::ProviderError::InvalidResponse("invalid call_id in manifest".into())))?;

            // Emit dispatched
            let mut audit = AuditAppender::open(&audit_log)?;
            let endpoint_fp = sha256_bytes(format!("provider:{}|base_url:{}|model:{}", req.provider.0, base_url, req.model.0).as_bytes());
            let dispatched = spec::AuditEvent::ModelCallDispatched(spec::ModelCallDispatched {
                schema_version: 1,
                run_id: spec::RunId(req.run_id.0.clone()),
                tick_id: spec::TickId(req.tick_id.0),
                ts: ts_dispatched,
                model_call: spec::CallId(call_uuid),
                provider: req.provider.0.clone(),
                model: req.model.0.clone(),
                endpoint_fingerprint: endpoint_fp.clone(),
                request_post_hash: req.integrity.post_hash.clone(),
            });
            audit.append(dispatched)?;

            let provider = OpenAICompatProvider::new(base_url.clone(), api_key.clone());
            let start = Instant::now();
            let resp = provider.dispatch(&req).await;
            let latency_ms = start.elapsed().as_millis() as u64;

            let artifacts_dir = call_dir.clone();
            let (status, provider_request_id_hash, response_hash, response_size, _raw_path, norm_path) = match resp {
                Ok(ok) => {
                    let raw_path = artifacts_dir.join("response_raw.json");
                    let raw_bytes = pie_common::canonical_json_bytes(&ok.raw_json)?;
                    fs::write(&raw_path, &raw_bytes)?;
                    let response_hash = sha256_bytes(&raw_bytes);

                    let norm_path = artifacts_dir.join("reply_normalized.json");
                    let norm_bytes = pie_common::canonical_json_bytes(&ok.normalized)?;
                    fs::write(&norm_path, &norm_bytes)?;

                    let pid_hash = sha256_bytes(ok.normalized.provider_request_id.unwrap_or_default().as_bytes());
                    (spec::CallStatus::Ok, pid_hash, response_hash, raw_bytes.len() as u64, raw_path, norm_path)
                }
                Err(e) => {
                    let raw_path = artifacts_dir.join("response_raw.json");
                    let err_obj = serde_json::json!({"error": format!("{e}")});
                    let raw_bytes = pie_common::canonical_json_bytes(&err_obj)?;
                    fs::write(&raw_path, &raw_bytes)?;
                    let response_hash = sha256_bytes(&raw_bytes);

                    let norm_path = artifacts_dir.join("reply_normalized.json");
                    let placeholder = serde_json::json!({"content":"", "finish_reason":"error", "usage":{"input_tokens":null,"output_tokens":null}, "provider_request_id": null});
                    let norm_bytes = pie_common::canonical_json_bytes(&placeholder)?;
                    fs::write(&norm_path, &norm_bytes)?;

                    let pid_hash = sha256_bytes(b"");
                    (spec::CallStatus::Error, pid_hash, response_hash, raw_bytes.len() as u64, raw_path, norm_path)
                }
            };

            let norm_hash = sha256_bytes(fs::read(&norm_path)?.as_slice());
            let completed = spec::AuditEvent::ModelCallCompleted(spec::ModelCallCompleted {
                schema_version: 1,
                run_id: spec::RunId(req.run_id.0.clone()),
                tick_id: spec::TickId(req.tick_id.0),
                ts: ts_completed,
                model_call: spec::CallId(call_uuid),
                result: spec::ModelCallResult {
                    status,
                    latency_ms,
                    provider_request_id_hash,
                    response_hash: response_hash.clone(),
                    response_size_bytes: response_size,
                },
                artifacts: spec::CompletionArtifacts {
                    response_artifact: spec::ArtifactRef { r#type: "artifact_ref".into(), hash: response_hash.clone() },
                    normalized_reply_artifact: spec::ArtifactRef { r#type: "artifact_ref".into(), hash: norm_hash },
                },
            });
            audit.append(completed)?;

            println!(
                "{{\"call_id\":\"{}\",\"status\":\"{:?}\",\"latency_ms\":{},\"response_hash\":\"{}\"}}",
                manifest.call_id, status, latency_ms, response_hash
            );
            Ok(())
        }
        Command::Dispatch {
            repo_root,
            sanitized_json,
            audit_log,
            base_url,
            api_key,
            call_id,
            ts_dispatched,
            ts_completed,
        } => {
            ensure_runtime_dirs(&repo_root)?;

            let base_url = base_url
                .or_else(|| std::env::var("PIEBOT_PROVIDER_BASE_URL").ok())
                .unwrap_or_else(|| "https://api.openai.com".to_string());
            let api_key = api_key.or_else(|| std::env::var("PIEBOT_PROVIDER_API_KEY").ok());


            let bytes = fs::read(&sanitized_json)?;
            let req: SanitizedModelRequest = serde_json::from_slice(&bytes)?;

            // Defensive: ensure integrity hashes exist (should have been set during redaction)
            if !req.integrity.pre_hash.starts_with("sha256:") || !req.integrity.post_hash.starts_with("sha256:") {
                return Err(CliError::Provider(pie_providers::ProviderError::InvalidResponse(
                    "sanitized request missing integrity hashes".into(),
                )));
            }

            let call_uuid = Uuid::parse_str(&call_id)
                .map_err(|_| CliError::Provider(pie_providers::ProviderError::InvalidResponse("invalid call_id".into())))?;


            // Emit ModelCallDispatched
            let mut audit = AuditAppender::open(&audit_log)?;
            let endpoint_fp = sha256_bytes(format!("provider:{}|base_url:{}|model:{}", req.provider.0, base_url, req.model.0).as_bytes());
 
            let dispatched = spec::AuditEvent::ModelCallDispatched(spec::ModelCallDispatched {
                schema_version: 1,
                run_id: spec::RunId(req.run_id.0.clone()),
                tick_id: spec::TickId(req.tick_id.0),
                ts: ts_dispatched,
                model_call: spec::CallId(call_uuid),
                provider: req.provider.0.clone(),
                model: req.model.0.clone(),
                endpoint_fingerprint: endpoint_fp.clone(),
                request_post_hash: req.integrity.post_hash.clone(),
            });
            audit.append(dispatched)?;


            // Dispatch via provider (OpenAI-compatible for Stage 6B baseline)
            let provider = OpenAICompatProvider::new(base_url.clone(), api_key.clone());
            let start = Instant::now();
            let resp = provider.dispatch(&req).await;
            let latency_ms = start.elapsed().as_millis() as u64;

            // Determine artifacts dir (same folder as request_post.json)
            let artifacts_dir = sanitized_json
                .parent()
                .ok_or_else(|| CliError::Provider(pie_providers::ProviderError::InvalidResponse("sanitized_json has no parent".into())))?
                .to_path_buf();

            // Always store raw response artifact, even on error (as structured object)
            let (status, provider_request_id_hash, response_hash, response_size, _raw_path, norm_path) = match resp {
                Ok(ok) => {
                    let raw_path = artifacts_dir.join("response_raw.json");
                    let raw_bytes = pie_common::canonical_json_bytes(&ok.raw_json)?;
                    fs::write(&raw_path, &raw_bytes)?;
                    let response_hash = sha256_bytes(&raw_bytes);

                    let norm_path = artifacts_dir.join("reply_normalized.json");
                    let norm_bytes = pie_common::canonical_json_bytes(&ok.normalized)?;
                    fs::write(&norm_path, &norm_bytes)?;

                    let pid_hash = sha256_bytes(ok.normalized.provider_request_id.unwrap_or_default().as_bytes());
                    (spec::CallStatus::Ok, pid_hash, response_hash, raw_bytes.len() as u64, raw_path, norm_path)
                }
                Err(e) => {
                    let raw_path = artifacts_dir.join("response_raw.json");
                    let err_obj = serde_json::json!({"error": format!("{e}")});
                    let raw_bytes = pie_common::canonical_json_bytes(&err_obj)?;
                    fs::write(&raw_path, &raw_bytes)?;
                    let response_hash = sha256_bytes(&raw_bytes);

                    // normalized reply absent on error; still write placeholder for replay determinism
                    let norm_path = artifacts_dir.join("reply_normalized.json");
                    let placeholder = serde_json::json!({"content":"", "finish_reason":"error", "usage":{"input_tokens":null,"output_tokens":null}, "provider_request_id": null});
                    let norm_bytes = pie_common::canonical_json_bytes(&placeholder)?;
                    fs::write(&norm_path, &norm_bytes)?;

                    let pid_hash = sha256_bytes(b"");
                    (spec::CallStatus::Error, pid_hash, response_hash, raw_bytes.len() as u64, raw_path, norm_path)
                }
            };

            // Emit ModelCallCompleted
            let norm_hash = sha256_bytes(fs::read(&norm_path)?.as_slice());
            let completed = spec::AuditEvent::ModelCallCompleted(spec::ModelCallCompleted {
                schema_version: 1,
                run_id: spec::RunId(req.run_id.0.clone()),
                tick_id: spec::TickId(req.tick_id.0),
                ts: ts_completed,
                model_call: spec::CallId(call_uuid),
                result: spec::ModelCallResult {
                    status,
                    latency_ms,
                    provider_request_id_hash,
                    response_hash: response_hash.clone(),
                    response_size_bytes: response_size,
                },
                artifacts: spec::CompletionArtifacts {
                    response_artifact: spec::ArtifactRef { r#type: "artifact_ref".into(), hash: response_hash.clone() },
                    normalized_reply_artifact: spec::ArtifactRef { r#type: "artifact_ref".into(), hash: norm_hash },
                },
            });
            audit.append(completed)?;

            println!(
                "{{\"call_id\":\"{}\",\"status\":\"{:?}\",\"latency_ms\":{},\"response_hash\":\"{}\"}}",
                call_id, status, latency_ms, response_hash
            );
            Ok(())
        }
    }
}

fn ensure_runtime_dirs(repo_root: &Path) -> Result<(), CliError> {
    let logs = repo_root.join("runtime").join("logs");
    let artifacts = repo_root.join("runtime").join("artifacts");
    fs::create_dir_all(logs)?;
    fs::create_dir_all(artifacts)?;
    Ok(())
}