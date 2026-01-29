"""Test orchestrator tick functionality.
"""


from __future__ import annotations

import uuid
from pathlib import Path

from apps.server.audit import get_audit_writer
from apps.server.models.null_model import NullModel
from apps.server.orchestrator import Orchestrator

from packages.core.audit import verify_audit_log
from packages.core.audit import replay_audit_log
from packages.core.types import ObservationEvent
from packages.policy.engine import PolicyEngine
from packages.tools.registry import ToolRegistry
from packages.tools.builtins import fs_read_file_spec


def test_orchestrator_tick_audited_and_replayable(tmp_path: Path):
    # Fake repo root
    repo = tmp_path / "repo"
    repo.mkdir()
    (repo / "hello.txt").write_text("hi", encoding="utf-8")

    # Runtime root
    runtime = tmp_path / "runtime"

    audit = get_audit_writer(runtime)
    policy = PolicyEngine()
    tools = ToolRegistry(policy=policy, audit=audit, repo_root=repo, runtime_root=runtime)
    tools.register(fs_read_file_spec)

    model = NullModel()
    orch = Orchestrator(tools=tools, audit=audit, model=model, runtime_root=runtime)

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
