"""
Rust redaction CLI stub (worker-plane helper).

This is intentionally a thin subprocess wrapper. It is NOT an authority.
Rust is the authority for:
  - canonical hashing
  - redaction
  - audit event emission
  - replay integrity

Python should treat this as a "tool call" boundary.
"""

from __future__ import annotations

import json
import os
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict, Optional


@dataclass(frozen=True)
class RustRedactionResult:
    call_id: str
    pre_hash: str
    post_hash: str
    transform_log_hash: str


def run_rust_redact_only(
    *,
    repo_root: Path,
    request_json: Path,
    audit_log: Path,
    policy_decision_id: str = "policy_decision_unspecified",
    requires_approval: bool = True,
    policy_id: str = "policy_unspecified",
    profile: str = "strict",
    summary_budget_chars: int = 1200,
    ts_prepared: float = 0.0,
    ts_redacted: float = 0.0,
    rust_dir: Optional[Path] = None,
) -> RustRedactionResult:
    """
    Call the Rust CLI: pie-control redact-only ...

    NOTE:
    - This function does not validate redaction. Rust does.
    - This function does not parse/inspect pre-redaction content beyond passing a filepath.
    """
    rust_dir = rust_dir or (repo_root / "rust")
    cmd = [
        "cargo",
        "run",
        "-q",
        "-p",
        "pie_control_cli",
        "--bin",
        "pie-control",
        "--",
        "redact-only",
        "--repo-root",
        str(repo_root),
        "--request-json",
        str(request_json),
        "--audit-log",
        str(audit_log),
        "--policy-decision-id",
        policy_decision_id,
        "--policy-id",
        policy_id,
        "--profile",
        profile,
        "--summary-budget-chars",
        str(summary_budget_chars),
        "--ts-prepared",
        str(ts_prepared),
        "--ts-redacted",
        str(ts_redacted),
    ]
    if not requires_approval:
        cmd.append("--requires-approval=false")

    env = os.environ.copy()
    # Keep cargo quiet and deterministic-ish; user controls full rust toolchain.
    env.setdefault("RUST_BACKTRACE", "0")

    proc = subprocess.run(
        cmd,
        cwd=str(rust_dir),
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        raise RuntimeError(
            f"Rust redaction failed (code={proc.returncode}).\nSTDOUT:\n{proc.stdout}\nSTDERR:\n{proc.stderr}"
        )

    # Last line is expected to be the JSON summary from Rust
    out = proc.stdout.strip().splitlines()[-1]
    data: Dict[str, Any] = json.loads(out)
    return RustRedactionResult(
        call_id=data["call_id"],
        pre_hash=data["pre_hash"],
        post_hash=data["post_hash"],
        transform_log_hash=data["transform_log_hash"],
    )