"""
Test cases for the ToolRegistry functionality.
"""

from __future__ import annotations

from pathlib import Path

from packages.core.audit import AuditWriter, verify_audit_log
from packages.policy.engine import PolicyEngine, RiskClass
from packages.tools.registry import ToolRegistry, ToolSpec
from packages.tools.builtins import fs_list_dir_spec, fs_read_file_spec, git_diff_spec


def test_registry_invokes_read_tool_and_audits(tmp_path: Path):
    repo = tmp_path / "repo"
    repo.mkdir()
    (repo / "hello.txt").write_text("hi", encoding="utf-8")

    runtime = tmp_path / "runtime"
    log = runtime / "logs" / "audit.jsonl"

    audit = AuditWriter(log)
    policy = PolicyEngine()
    reg = ToolRegistry(policy=policy, audit=audit, repo_root=repo, runtime_root=runtime)
    reg.register(fs_read_file_spec)

    res = reg.invoke(run_id="r1", tool_name="fs.read_file", args={"path": "hello.txt"})
    assert res.ok is True
    assert res.result["text"] == "hi"

    ok, err = verify_audit_log(log)
    assert ok is True, err


def test_registry_blocks_exec_by_default(tmp_path: Path):
    repo = tmp_path / "repo"
    repo.mkdir()
    runtime = tmp_path / "runtime"
    log = runtime / "logs" / "audit.jsonl"

    audit = AuditWriter(log)
    policy = PolicyEngine()  # ALLOW_EXEC defaults false
    reg = ToolRegistry(policy=policy, audit=audit, repo_root=repo, runtime_root=runtime)

    def _noop(args, ctx):
        return {"ok": True}

    reg.register(ToolSpec(name="danger.exec", risk=RiskClass.EXEC, schema={"type": "object"}, handler=_noop))
    res = reg.invoke(run_id="r1", tool_name="danger.exec", args={})
    assert res.ok is False
    assert "blocked by policy" in (res.error or "")


def test_builtin_specs_registerable(tmp_path: Path):
    repo = tmp_path / "repo"
    repo.mkdir()
    runtime = tmp_path / "runtime"
    log = runtime / "logs" / "audit.jsonl"

    audit = AuditWriter(log)
    policy = PolicyEngine()
    reg = ToolRegistry(policy=policy, audit=audit, repo_root=repo, runtime_root=runtime)
    reg.register(fs_list_dir_spec)
    reg.register(fs_read_file_spec)
    reg.register(git_diff_spec)
    assert reg.get_spec("fs.list_dir") is not None
    assert reg.get_spec("git.diff") is not None
