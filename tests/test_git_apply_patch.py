"""
Git apply_patch tool tests.
"""

from __future__ import annotations

from pathlib import Path
import os
import subprocess

from packages.core.audit import AuditWriter
from packages.policy.engine import PolicyEngine
from packages.tools.registry import ToolRegistry
from packages.tools.builtins import git_apply_patch_spec


def _git(cmd, cwd: Path) -> None:
    p = subprocess.run(["git"] + cmd, cwd=str(cwd), capture_output=True, text=True)
    assert p.returncode == 0, (p.stderr or p.stdout)


def test_apply_patch_blocked_without_arm(tmp_path: Path, monkeypatch):
    repo = tmp_path / "repo"
    repo.mkdir()
    _git(["init"], repo)
    (repo / "a.txt").write_text("old\n", encoding="utf-8")
    _git(["add", "a.txt"], repo)
    _git(["-c", "user.email=x@y.z", "-c", "user.name=x", "commit", "-m", "init"], repo)

    runtime = tmp_path / "runtime"
    (runtime / "artifacts" / "diffs").mkdir(parents=True)
    (runtime / "artifacts" / "diffs" / "p.diff").write_text("", encoding="utf-8")

    monkeypatch.setenv("EXECUTION_ARM", "false")
    monkeypatch.setenv("PIEBOT_APPROVAL_TOKEN", "t")

    audit = AuditWriter(runtime / "logs" / "audit.jsonl")
    reg = ToolRegistry(policy=PolicyEngine(), audit=audit, repo_root=repo, runtime_root=runtime)
    reg.register(git_apply_patch_spec)

    res = reg.invoke(run_id="r1", tool_name="git.apply_patch", args={"diff_file": "p.diff", "approval_token": "t"})
    assert res.ok is False
    assert "blocked by policy" in (res.error or "")


def test_apply_patch_blocked_without_approval(tmp_path: Path, monkeypatch):
    repo = tmp_path / "repo"
    repo.mkdir()
    _git(["init"], repo)
    (repo / "a.txt").write_text("old\n", encoding="utf-8")
    _git(["add", "a.txt"], repo)
    _git(["-c", "user.email=x@y.z", "-c", "user.name=x", "commit", "-m", "init"], repo)

    runtime = tmp_path / "runtime"
    diffs = runtime / "artifacts" / "diffs"
    diffs.mkdir(parents=True)
    (diffs / "p.diff").write_text("", encoding="utf-8")

    monkeypatch.setenv("EXECUTION_ARM", "true")
    monkeypatch.setenv("PIEBOT_APPROVAL_TOKEN", "expected")

    audit = AuditWriter(runtime / "logs" / "audit.jsonl")
    reg = ToolRegistry(policy=PolicyEngine(), audit=audit, repo_root=repo, runtime_root=runtime)
    reg.register(git_apply_patch_spec)

    res = reg.invoke(run_id="r1", tool_name="git.apply_patch", args={"diff_file": "p.diff", "approval_token": "wrong"})
    assert res.ok is False
    assert (res.error or "") == "approval required"


def test_apply_patch_succeeds_with_arm_and_approval(tmp_path: Path, monkeypatch):
    repo = tmp_path / "repo"
    repo.mkdir()
    _git(["init"], repo)
    (repo / "a.txt").write_text("old\n", encoding="utf-8")
    _git(["add", "a.txt"], repo)
    _git(["-c", "user.email=x@y.z", "-c", "user.name=x", "commit", "-m", "init"], repo)

    # Create a diff that changes a.txt
    (repo / "a.txt").write_text("new\n", encoding="utf-8")
    diff = subprocess.run(["git", "diff", "--no-color"], cwd=str(repo), capture_output=True, text=True)
    assert diff.returncode == 0
    patch_text = diff.stdout
    assert "new" in patch_text
    # Reset working tree
    _git(["checkout", "--", "a.txt"], repo)

    runtime = tmp_path / "runtime"
    diffs = runtime / "artifacts" / "diffs"
    diffs.mkdir(parents=True)
    (diffs / "p.diff").write_text(patch_text, encoding="utf-8")

    monkeypatch.setenv("EXECUTION_ARM", "true")
    monkeypatch.setenv("PIEBOT_APPROVAL_TOKEN", "ok")

    audit = AuditWriter(runtime / "logs" / "audit.jsonl")
    reg = ToolRegistry(policy=PolicyEngine(), audit=audit, repo_root=repo, runtime_root=runtime)
    reg.register(git_apply_patch_spec)

    res = reg.invoke(run_id="r1", tool_name="git.apply_patch", args={"diff_file": "p.diff", "approval_token": "ok"})
    assert res.ok is True
    assert res.result.get("applied") is True

    # Verify file changed
    assert (repo / "a.txt").read_text(encoding="utf-8") == "new\n"