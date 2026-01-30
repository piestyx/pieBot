"""Tests for orchestrator pipeline with retries and audit 
logging."""


from __future__ import annotations

import uuid
from pathlib import Path

from apps.server.audit import get_audit_writer
from apps.server.orchestrator import Orchestrator

from packages.core.audit import verify_audit_log, replay_audit_log
from packages.core.types import ObservationEvent
from packages.policy.engine import PolicyEngine
from packages.tools.registry import ToolRegistry
from packages.tools.builtins import fs_read_file_spec
from packages.models import ModelRouter


def _make_router(tmp_path: Path) -> ModelRouter:
    cfg = tmp_path / "router.yaml"
    cfg.write_text(
        """
models:
  null:
    kind: null
    capabilities: []
routing:
  planner: null
  executor: null
  critic: null
""".strip(),
        encoding="utf-8",
    )
    return ModelRouter.load(cfg)


def _make_orch(repo: Path, runtime: Path, tmp_path: Path) -> Orchestrator:
    audit = get_audit_writer(runtime)
    policy = PolicyEngine()
    tools = ToolRegistry(policy=policy, audit=audit, repo_root=repo, runtime_root=runtime)
    tools.register(fs_read_file_spec)
    router = _make_router(tmp_path)
    return Orchestrator(tools=tools, audit=audit, router=router, runtime_root=runtime, max_attempts=2)


def test_pipeline_passes(tmp_path: Path):
    repo = tmp_path / "repo"
    repo.mkdir()
    (repo / "hello.txt").write_text("hi", encoding="utf-8")

    runtime = tmp_path / "runtime"
    orch = _make_orch(repo, runtime, tmp_path)

    run_id = uuid.uuid4().hex
    obs = ObservationEvent(run_id=run_id, kind="file_read", data={"path": "hello.txt"})
    res = orch.run_once(obs)

    assert res.ok is True
    assert len(res.tool_results) == 1
    assert res.tool_results[0].ok is True
    assert res.tool_results[0].result["text"] == "hi"

    log_path = runtime / "logs" / "audit.jsonl"
    ok, err = verify_audit_log(log_path)
    assert ok is True, err
    rep = replay_audit_log(log_path)
    assert rep.ok is True


def test_pipeline_retries_then_fails(tmp_path: Path):
    repo = tmp_path / "repo"
    repo.mkdir()
    # no file created -> fs.read_file fails

    runtime = tmp_path / "runtime"
    orch = _make_orch(repo, runtime, tmp_path)

    run_id = uuid.uuid4().hex
    obs = ObservationEvent(run_id=run_id, kind="file_read", data={"path": "missing.txt"})
    res = orch.run_once(obs)

    assert res.ok is False
    assert len(res.tool_results) == 2  # max_attempts=2 -> 2 failed calls
    assert all(r.ok is False for r in res.tool_results)

    # Ensure audit verifies and replay passes
    log_path = runtime / "logs" / "audit.jsonl"
    ok, err = verify_audit_log(log_path)
    assert ok is True, err
    rep = replay_audit_log(log_path)
    assert rep.ok is True
