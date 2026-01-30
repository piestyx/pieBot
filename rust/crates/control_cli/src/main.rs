use clap::{Parser, Subcommand};
use pie_audit_log::{verify_log, AuditAppender};
use pie_redaction::{ModelRequest, RedactionEngine, RedactionProfile};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
enum CliError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("redaction error: {0}")]
    Redaction(#[from] pie_redaction::RedactionError),
    #[error("audit error: {0}")]
    Audit(#[from] pie_audit_log::AuditLogError),
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

    /// Verify a hash-chained audit log JSONL and print final hash.
    VerifyAudit {
        #[arg(long)]
        audit_log: PathBuf,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("ERROR: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), CliError> {
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
    }
}

fn ensure_runtime_dirs(repo_root: &Path) -> Result<(), CliError> {
    let logs = repo_root.join("runtime").join("logs");
    let artifacts = repo_root.join("runtime").join("artifacts");
    fs::create_dir_all(logs)?;
    fs::create_dir_all(artifacts)?;
    Ok(())
}