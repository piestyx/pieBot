use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;


fn write_append_req(dir: &TempDir) -> PathBuf {
    let p = dir.path().join("episode_append.json");
    let body = r#"
{
  "schema_version": 1,
  "run_id": "run_demo",
  "tick_id": 1,
  "thread_id": "main",
  "tags": ["memory_domain:dev", "role:planner", "status:ok"],
  "title": "Stage 7B.1 test",
  "summary": "integration test episode append/query/get",
  "artifacts": [{"hash":"sha256:deadbeef","kind":"audit_ref"}],
  "created_ts": 0.0
}
"#;
    fs::write(&p, body).unwrap();
    p
}

fn audit_log_path(dir: &TempDir) -> PathBuf {
    // keep it inside the temp repo root
    dir.path().join("runtime").join("logs").join("audit_rust.jsonl")
}

#[test]
fn episodes_cli_append_query_get_roundtrip() {
    let repo = TempDir::new().unwrap();

    // Ensure runtime/logs exists so audit appender can create file
    fs::create_dir_all(repo.path().join("runtime").join("logs")).unwrap();

    let req = write_append_req(&repo);
    let audit = audit_log_path(&repo);

    let pie_control = assert_cmd::cargo::cargo_bin!("pie-control");

    // 1) episode-append
    Command::new(&pie_control)
        .args([
            "episode-append",
            "--repo-root",
            repo.path().to_str().unwrap(),
            "--request-json",
            req.to_str().unwrap(),
            "--audit-log",
            audit.to_str().unwrap(),
            "--ts",
            "0.0",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"episode_id\""))
        .stdout(predicate::str::contains("\"episode_hash\""));

    // 2) episode-query
    let query_out = Command::new(&pie_control)
        .args([
            "episode-query",
            "--repo-root",
            repo.path().to_str().unwrap(),
            "--thread-id",
            "main",
            "--tag",
            "role:planner",
            "--tag",
            "status:ok",
            "--since-tick",
            "0",
            "--limit",
            "20",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let s = String::from_utf8(query_out).unwrap();
    assert!(s.contains("\"run_id\":\"run_demo\""));
    assert!(s.contains("\"thread_id\":\"main\""));
    assert!(s.contains("\"role:planner\"") || s.contains("role:planner"));

    // Extract episode_id from the query output in a dumb but stable way
    // (We avoid bringing in a JSON parser just for the test.)
    let marker = "\"episode_id\":\"";
    let start = s.find(marker).expect("episode_id missing") + marker.len();
    let end = s[start..].find('"').unwrap() + start;
    let episode_id = &s[start..end];

    // 3) episode-get
    Command::new(&pie_control)
        .args([
            "episode-get",
            "--repo-root",
            repo.path().to_str().unwrap(),
            "--episode-id",
            episode_id,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"schema_version\":1"))
        .stdout(predicate::str::contains("\"run_id\":\"run_demo\""))
        .stdout(predicate::str::contains("\"thread_id\":\"main\""))
        .stdout(predicate::str::contains("\"hash\":\"sha256:"));

    // Sanity: episodes files exist
    assert!(repo
        .path()
        .join("runtime")
        .join("memory")
        .join("episodes")
        .join("episodes.jsonl")
        .exists());
    assert!(repo
        .path()
        .join("runtime")
        .join("memory")
        .join("episodes")
        .join("index.json")
        .exists());
}