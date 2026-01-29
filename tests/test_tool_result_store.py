"""
Tests for the tool result store functionality.
"""


from __future__ import annotations

from pathlib import Path
import json

from packages.core.audit import AuditWriter
from packages.policy.engine import PolicyEngine
from packages.tools.registry import ToolRegistry
from packages.tools.builtins import fs_read_file_spec


def test_tool_result_written_as_artifact(tmp_path: Path):
    repo = tmp_path / "repo"
    repo.mkdir()
    (repo / "hello.txt").write_text("hi", encoding="utf-8")

    runtime = tmp_path / "runtime"
    audit = AuditWriter(runtime / "logs" / "audit.jsonl")
    reg = ToolRegistry(policy=PolicyEngine(), audit=audit, repo_root=repo, runtime_root=runtime)
    reg.register(fs_read_file_spec)

    res = reg.invoke(run_id="r1", tool_name="fs.read_file", args={"path": "hello.txt"})
    assert res.ok is True

    # Artifact exists
    out_dir = runtime / "artifacts" / "tool_results"
    files = list(out_dir.glob("*.json"))
    assert len(files) == 1
    obj = json.loads(files[0].read_text(encoding="utf-8"))
    assert obj["ok"] is True
    assert obj["result"]["text"] == "hi"