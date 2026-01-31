use clap::{Parser, Subcommand};
use dotenvy::from_path as dotenv_from_path;
use serde_json::json;
use serde_json::Value as JsonValue;
use pie_audit_log::{verify_log, AuditAppender};
use pie_common::sha256_bytes;
use pie_redaction::{ModelRequest, RedactionEngine, RedactionProfile, SanitizedModelRequest, CallManifest};
use pie_audit_spec as spec;
use pie_providers::{OpenAICompatProvider, Provider};
use pie_episodes as episodes;
use pie_openmemory_mirror as om;
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
    #[error("episodes error: {0}")]
    Episodes(#[from] episodes::EpisodeError),
    #[error("openmemory error: {0}")]
    OpenMemory(#[from] om::OpenMemoryError),    
}

#[derive(Parser)]
#[command(name = "pie-control", version, about = "pieBot Rust control-plane utilities (6B boundary)")]
struct Args {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Debug, serde::Deserialize)]
struct EpisodeAppendRequest {
    schema_version: u8,
    run_id: String,
    tick_id: u64,
    #[serde(default = "default_thread")]
    thread_id: String,
    #[serde(default)]
    tags: Vec<String>,
    title: String,
    summary: String,
    #[serde(default)]
    artifacts: Vec<EpisodeAppendArtifact>,
    #[serde(default)]
    created_ts: f64,
}

#[derive(Debug, serde::Deserialize)]
struct EpisodeAppendArtifact {
    hash: String,
    #[serde(default)]
    kind: Option<String>,
}

fn default_thread() -> String {
    "main".to_string()
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
        /// Can be supplied via env OPENAI_BASE_URL.
        #[arg(long)]
        base_url: Option<String>,

        /// API key (optional). Can be supplied via env OPENAI_API_KEY.
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

    /// Append a deterministic episode to runtime/memory/episodes and emit an audit event.
    ///
    /// Writes:
    /// - runtime/memory/episodes/episodes.jsonl (append-only)
    /// - runtime/memory/episodes/index.json (canonical rewrite)
    ///
    /// Appends audit:
    /// - EpisodeAppended
    EpisodeAppend {
        #[arg(long)]
        repo_root: PathBuf,

        /// JSON request describing the episode content
        #[arg(long)]
        request_json: PathBuf,

        /// Audit log JSONL path to append to
        #[arg(long)]
        audit_log: PathBuf,

        /// Timestamp for EpisodeAppended
        #[arg(long, default_value_t = 0.0)]
        ts: f64,
    },

    /// Query the deterministic episode index in runtime/memory/episodes.
    ///
    /// Filters:
    /// - optional thread_id
    /// - tags must include all provided --tag values
    /// - optional since_tick (inclusive)
    /// - limit
    ///
    /// Output:
    /// - JSON array of index entries sorted deterministically
    EpisodeQuery {
        #[arg(long)]
        repo_root: PathBuf,

        #[arg(long)]
        thread_id: Option<String>,

        /// Provide multiple times: --tag role:planner --tag status:ok
        #[arg(long = "tag")]
        tags: Vec<String>,

        #[arg(long)]
        since_tick: Option<u64>,

        #[arg(long, default_value_t = 20)]
        limit: usize,
    },

    /// Load a full episode by episode_id (verifies hash + index).
    ///
    /// Output:
    /// - full Episode JSON (as stored), including `hash`
    EpisodeGet {
        #[arg(long)]
        repo_root: PathBuf,

        #[arg(long)]
        episode_id: String,
    },

    /// Verify a hash-chained audit log JSONL and print final hash.
    VerifyAudit {
        #[arg(long)]
        audit_log: PathBuf,
    },

    /// Mirror a locally-stored episode into OpenMemory (best-effort, non-authoritative).
    ///
    /// This does NOT affect deterministic replay. It only emits audit events describing the attempt/result.
    EpisodeMirror {
        #[arg(long)]
        repo_root: PathBuf,

        #[arg(long)]
        episode_id: String,

        #[arg(long)]
        audit_log: PathBuf,

        /// OpenMemory base URL (default matches local backend dev server).
        #[arg(long, default_value = "http://127.0.0.1:8080")]
        base_url: String,

        /// Optional OpenMemory API key. If omitted, reads OPENMEMORY_API_KEY env var.
        #[arg(long)]
        api_key: Option<String>,

        /// Optional OpenMemory user_id (for multi-user isolation). Defaults to thread_id if omitted.
        #[arg(long)]
        user_id: Option<String>,

        #[arg(long, default_value_t = 2000)]
        timeout_ms: u64,

        #[arg(long, default_value_t = 0.0)]
        ts: f64,
    },
    /// Query OpenMemory (/memory/query) and return reference-only results (no raw content).
    EpisodeQueryRemote {
        #[arg(long)]
        repo_root: std::path::PathBuf,

        /// Query text (will be hashed in audit; not stored verbatim).
        #[arg(long)]
        query: String,

        /// Top-K results.
        #[arg(long, default_value_t = 5)]
        k: u32,

        /// Optional OpenMemory user_id filter.
        #[arg(long)]
        user_id: Option<String>,

        /// Optional minimum similarity score (0-1). Server may ignore depending on deployment.
        #[arg(long)]
        min_score: Option<f64>,

        /// Base URL of OpenMemory backend.
        #[arg(long, default_value = "http://127.0.0.1:8080")]
        base_url: String,

        /// Audit log path.
        #[arg(long)]
        audit_log: std::path::PathBuf,

        /// Optional run_id for audit (defaults to run_demo).
        #[arg(long, default_value = "run_demo")]
        run_id: String,

        /// Optional tick_id for audit (defaults to 0).
        #[arg(long, default_value_t = 0)]
        tick_id: u64,

        /// Timestamp for audit events.
        #[arg(long, default_value_t = 0.0)]
        ts: f64,

        /// Request timeout in ms.
        #[arg(long, default_value_t = 10_000)]
        timeout_ms: u64,
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

            // Load .env from repo root or CWD (best-effort, but visible)
            let repo_env = repo_root.join(".env");
            if repo_env.exists() {
                let _ = dotenv_from_path(&repo_env);
                eprintln!("loaded env from {}", repo_env.display());
            } else if Path::new(".env").exists() {
                let _ = dotenv_from_path(".env");
                eprintln!("loaded env from ./.env");
            } else {
                eprintln!("no .env file found (expected at {} or CWD)", repo_env.display());
            }

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

        Command::EpisodeAppend { repo_root, request_json, audit_log, ts } => {
            // Load repo_root/.env if present (local-only secrets; not required for episodes but keeps behavior consistent)
            let repo_env = repo_root.join(".env");
            if repo_env.exists() {
                let _ = dotenv_from_path(&repo_env);
                eprintln!("loaded env from {}", repo_env.display());
            } else if Path::new(".env").exists() {
                let _ = dotenv_from_path(".env");
                eprintln!("loaded env from ./.env");
            }

            let bytes = fs::read(&request_json)?;
            let req: EpisodeAppendRequest = serde_json::from_slice(&bytes)?;
            if req.schema_version != 1 {
                return Err(CliError::Episodes(episodes::EpisodeError::Corrupt(format!(
                    "unsupported EpisodeAppendRequest schema_version {}",
                    req.schema_version
                ))));
            }

            let artifacts: Vec<episodes::ArtifactRef> = req
                .artifacts
                .into_iter()
                .map(|a| episodes::ArtifactRef { hash: a.hash, kind: a.kind })
                .collect();

            let ep = episodes::Episode::new(
                episodes::RunId(req.run_id.clone()),
                episodes::TickId(req.tick_id),
                req.thread_id.clone(),
                req.tags.clone(),
                req.title.clone(),
                req.summary.clone(),
                artifacts,
                req.created_ts,
            )?;

            // Append to authoritative store
            let store = episodes::EpisodeStore::new(repo_root.clone());
            store.append(&ep)?;

            // Emit audit event
            let mut audit = AuditAppender::open(&audit_log)?;
            let evt = spec::AuditEvent::EpisodeAppended(spec::EpisodeAppended {
                schema_version: 1,
                run_id: spec::RunId(req.run_id),
                tick_id: spec::TickId(req.tick_id),
                ts,
                episode_id: ep.episode_id,
                thread_id: ep.thread_id.clone(),
                tags: ep.tags.clone(),
                title: ep.title.clone(),
                episode_hash: ep.hash.clone(),
                episode_artifact: spec::ArtifactRef { r#type: "artifact_ref".into(), hash: ep.hash.clone() },
            });
            audit.append(evt)?;

            println!(
                "{{\"episode_id\":\"{}\",\"episode_hash\":\"{}\"}}",
                ep.episode_id, ep.hash
            );
            Ok(())
        }      
        
        Command::EpisodeQuery { repo_root, thread_id, tags, since_tick, limit } => {
            let store = episodes::EpisodeStore::new(repo_root);
            let since = since_tick.map(episodes::TickId);
            let results = store.query(thread_id.as_deref(), &tags, since, limit)?;

            // Print stable JSON array (no pretty print; callers can jq if needed).
            // Fields chosen match EpisodeIndexEntry.
            let out = results
                .into_iter()
                .map(|e| {
                    json!({
                        "episode_id": e.episode_id.to_string(),
                        "run_id": e.run_id.0,
                        "tick_id": e.tick_id.0,
                        "thread_id": e.thread_id,
                        "tags": e.tags,
                        "hash": e.hash,
                        "line_no": e.line_no
                    })
                })
                .collect::<Vec<_>>();

            println!("{}", serde_json::to_string(&out)?);
            Ok(())
        }

        Command::EpisodeGet { repo_root, episode_id } => {
            let store = episodes::EpisodeStore::new(repo_root);
            let idx = store.load_index()?;

            let uid = Uuid::parse_str(&episode_id)
                .map_err(|_| CliError::Episodes(episodes::EpisodeError::Corrupt("invalid episode_id".into())))?;

            let entry = idx
                .entries
                .iter()
                .find(|e| e.episode_id == uid)
                .ok_or_else(|| CliError::Episodes(episodes::EpisodeError::Corrupt("episode_id not found in index".into())))?;

            let ep = store.load_episode_by_entry(entry)?;

            // Print full episode JSON as stored (includes hash).
            // No pretty print; deterministic pipelines can hash canonical bytes separately.
            println!("{}", serde_json::to_string(&ep)?);
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

            // Load .env from repo root or CWD (best-effort, but visible)
            let repo_env = repo_root.join(".env");
            if repo_env.exists() {
                let _ = dotenv_from_path(&repo_env);
                eprintln!("loaded env from {}", repo_env.display());
            } else if Path::new(".env").exists() {
                let _ = dotenv_from_path(".env");
                eprintln!("loaded env from ./.env");
            } else {
                eprintln!("no .env file found (expected at {} or CWD)", repo_env.display());
            }

            ensure_runtime_dirs(&repo_root)?;
            let manifest_path = call_dir.join("call_manifest.json");
            let post_path = call_dir.join("request_post.json");

            let mbytes = fs::read(&manifest_path)?;
            let manifest: CallManifest = serde_json::from_slice(&mbytes)?;

            // Delegate to Dispatch logic by reusing the same code path:
            // We just reconstruct args inline.
            let base_url = base_url
                .or_else(|| std::env::var("OPENAI_BASE_URL").ok())
                .unwrap_or_else(|| "https://api.openai.com".to_string());
            let api_key = api_key.or_else(|| std::env::var("OPENAI_API_KEY").ok());

            // Helpful guardrail: if you're pointing at OpenAI and no API key is set, fail loudly.
            if api_key.as_deref().unwrap_or("").is_empty()
                && base_url.contains("api.openai.com")
            {
                return Err(CliError::Provider(pie_providers::ProviderError::InvalidResponse(
                    "OPENAI_API_KEY is required for https://api.openai.com (set it in .env or env var)".into(),
                )));
            }

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
                .or_else(|| std::env::var("OPENAI_BASE_URL").ok())
                .unwrap_or_else(|| "https://api.openai.com".to_string());
            let api_key = api_key.or_else(|| std::env::var("OPENAI_API_KEY").ok());


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

        Command::EpisodeMirror { repo_root, episode_id, audit_log, base_url, api_key, user_id, timeout_ms, ts } => {            // Load .env exactly like other commands (local-only convenience)
            let repo_env = repo_root.join(".env");
            if repo_env.exists() {
                let _ = dotenv_from_path(&repo_env);
                eprintln!("loaded env from {}", repo_env.display());
            } else if Path::new(".env").exists() {
                let _ = dotenv_from_path(".env");
                eprintln!("loaded env from ./.env");
            }

            let store = episodes::EpisodeStore::new(repo_root);
            let idx = store.load_index()?;

            let uid = Uuid::parse_str(&episode_id)
                .map_err(|_| CliError::Episodes(episodes::EpisodeError::Corrupt("invalid episode_id".into())))?;

            let entry = idx.entries.iter()
                .find(|e| e.episode_id == uid)
                .ok_or_else(|| CliError::Episodes(episodes::EpisodeError::Corrupt("episode_id not found in index".into())))?;

            let ep = store.load_episode_by_entry(entry)?;

            // Audit appender
            let mut app = AuditAppender::open(&audit_log)?;

            let attempted = spec::AuditEvent::EpisodeMirrorAttempted(spec::EpisodeMirrorAttempted {
                schema_version: 1,
                run_id: spec::RunId(ep.run_id.0.clone()),
                tick_id: spec::TickId(ep.tick_id.0),
                ts,
                episode_id: ep.episode_id,
                episode_hash: ep.hash.clone(),
                target: "openmemory".to_string(),
            });
            app.append(attempted)?;

            // Build OpenMemory request payload.
            // Content = title + summary (keeps it readable in OpenMemory dashboards).
            let mut content = String::new();
            if !ep.title.trim().is_empty() {
                content.push_str(ep.title.trim());
                content.push_str("\n\n");
            }
            content.push_str(ep.summary.trim());

            // Metadata: keep it tight and explicit.
            let meta: JsonValue = json!({
                "source": "pieBot",
                "episode_id": ep.episode_id,
                "episode_hash": ep.hash,
                "run_id": ep.run_id,
                "tick_id": ep.tick_id,
                "thread_id": ep.thread_id,
                "tags": ep.tags,
                "created_ts": ep.created_ts,
            });

            // Match local-agent-core behavior: OPENMEMORY_API_KEY or OM_API_KEY
            let key = api_key.or_else(|| {
                std::env::var("OPENMEMORY_API_KEY")
                    .ok()
                    .or_else(|| std::env::var("OM_API_KEY").ok())
            });

            // No key? Make it explicit (without leaking secrets).
            if key.is_none() {
                eprintln!("openmemory: no api key found (set OPENMEMORY_API_KEY or OM_API_KEY, or pass --api-key)");
            }
            let om_user_id = user_id.or_else(|| Some(ep.thread_id.clone()));

            let client = om::OpenMemoryClient::new(base_url, key, timeout_ms)?;


            let req = om::AddMemoryRequest {
                content,
                tags: ep.tags.clone(),
                metadata: Some(meta),
                user_id: om_user_id,
            };

            match client.add_memory(&req).await {
                Ok(resp) => {
                    let mirrored = spec::AuditEvent::EpisodeMirrored(spec::EpisodeMirrored {
                        schema_version: 1,
                        run_id: spec::RunId(ep.run_id.0.clone()),
                        tick_id: spec::TickId(ep.tick_id.0),
                        ts,
                        episode_id: ep.episode_id,
                        episode_hash: ep.hash.clone(),
                        target: "openmemory".to_string(),
                        remote_id: resp.id.clone(),
                    });
                    app.append(mirrored)?;

                    println!("{}", serde_json::to_string(&json!({
                        "episode_id": ep.episode_id.to_string(),
                        "episode_hash": ep.hash,
                        "target": "openmemory",
                        "remote_id": resp.id,
                        "primary_sector": resp.primary_sector,
                        "sectors": resp.sectors
                    }))?);
                    Ok(())
                }
                Err(e) => {
                    let failed = spec::AuditEvent::EpisodeMirrorFailed(spec::EpisodeMirrorFailed {
                        schema_version: 1,
                        run_id: spec::RunId(ep.run_id.0.clone()),
                        tick_id: spec::TickId(ep.tick_id.0),
                        ts,
                        episode_id: ep.episode_id,
                        episode_hash: ep.hash.clone(),
                        target: "openmemory".to_string(),
                        error: e.to_string(),
                    });
                    app.append(failed)?;

                    println!("{}", serde_json::to_string(&json!({
                        "episode_id": ep.episode_id.to_string(),
                        "episode_hash": ep.hash,
                        "target": "openmemory",
                        "status": "Error",
                        "error": e.to_string()
                    }))?);
                    Ok(())
                }
            }
        }
        
        Command::EpisodeQueryRemote {
            repo_root,
            query,
            k,
            user_id,
            min_score,
            base_url,
            audit_log,
            run_id,
            tick_id,
            ts,
            timeout_ms,
        } => {
            // Load .env (repo root first, then cwd) exactly like other commands.
            let repo_env = repo_root.join(".env");
            if repo_env.exists() {
                let _ = dotenv_from_path(&repo_env);
                eprintln!("loaded env from {}", repo_env.display());
            } else if std::path::Path::new(".env").exists() {
                let _ = dotenv_from_path(".env");
                eprintln!("loaded env from ./.env");
            }

            // Key resolution matches local-agent-core behavior:
            // OPENMEMORY_API_KEY or OM_API_KEY.
            let api_key = std::env::var("OPENMEMORY_API_KEY")
                .ok()
                .or_else(|| std::env::var("OM_API_KEY").ok());

            let client = pie_openmemory_mirror::OpenMemoryClient::new(base_url, api_key, timeout_ms)?;

            let req = pie_openmemory_mirror::QueryMemoryRequest {
                query: query.clone(),
                k: Some(k),
                user_id: user_id.clone(),
                min_score,
            };
            // Audit appender
            let mut app = AuditAppender::open(&audit_log)?;
            let rid = pie_audit_spec::RunId(run_id);
            let tid = pie_audit_spec::TickId(tick_id);

            // Hash query for audit (never store verbatim in log)
            let q_hash = sha256_bytes(query.as_bytes());
            let q_len = query.as_bytes().len() as u64;

            match client.query_memory(&req).await {
                Ok(parsed) => {
                    // Store raw response as artifact (hash-addressed)
                    let call_id = Uuid::new_v4().to_string();
                    let rel_dir = std::path::PathBuf::from("runtime")
                        .join("artifacts")
                        .join("memory")
                        .join("openmemory_queries")
                        .join(&call_id);
                    let out_dir = repo_root.join(&rel_dir);
                    std::fs::create_dir_all(&out_dir)?;

                    let raw_bytes = pie_common::canonical_json_bytes(&parsed.raw)?;
                    let resp_hash = sha256_bytes(&raw_bytes);
                    let resp_path = out_dir.join("response.json");
                    std::fs::write(&resp_path, &raw_bytes)?;

                    let art = pie_audit_spec::ArtifactRef {
                        r#type: "artifact_ref".to_string(),
                        hash: resp_hash.clone(),
                    };

                    let ev = pie_audit_spec::AuditEvent::EpisodeQueryPerformed(pie_audit_spec::EpisodeQueryPerformed {
                        schema_version: 1,
                        run_id: rid,
                        tick_id: tid,
                        ts,
                        target: "openmemory".to_string(),
                        query_hash: q_hash.clone(),
                        query_len: q_len,
                        k,
                        user_id,
                        alias: None,
                        result_count: parsed.hits.len() as u32,
                        response_hash: resp_hash.clone(),
                        response_artifact: art,
                    });
                    app.append(ev)?;

                    // Print refs only: id + score + content_hash
                    let safe = serde_json::json!({
                        "target": "openmemory",
                        "query_hash": q_hash,
                        "k": k,
                        "result_count": parsed.hits.len(),
                        "response_hash": resp_hash,
                        "hits": parsed.hits.iter().map(|h| serde_json::json!({
                            "id": h.id,
                            "score": h.score,
                            "content_hash": h.content_hash,
                        })).collect::<Vec<_>>(),
                    });
                    println!("{}", serde_json::to_string(&safe)?);
                    Ok(())
                }
                Err(e) => {
                    let ev = pie_audit_spec::AuditEvent::EpisodeQueryFailed(pie_audit_spec::EpisodeQueryFailed {
                        schema_version: 1,
                        run_id: rid,
                        tick_id: tid,
                        ts,
                        target: "openmemory".to_string(),
                        query_hash: q_hash,
                        query_len: q_len,
                        k,
                        user_id,
                        alias: None,
                        error: e.to_string(),
                    });
                    app.append(ev)?;

                    let out = serde_json::json!({
                        "target": "openmemory",
                        "status": "Error",
                        "error": e.to_string(),
                    });
                    println!("{}", serde_json::to_string(&out)?);
                    Ok(())
                }
            }
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