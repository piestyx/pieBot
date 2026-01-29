"""
Git-related tools for interacting with the repository.
"""

from __future__ import annotations

from typing import Any, Dict, List
import subprocess

from packages.policy.engine import RiskClass
from packages.tools.registry import ToolSpec, ToolContext


def _git_diff(args: Dict[str, Any], ctx: ToolContext) -> Dict[str, Any]:
    # Optional pathspec limiting
    paths: List[str] = args.get("paths") or []
    if not isinstance(paths, list):
        raise ValueError("paths must be a list")

    cmd = ["git", "diff", "--no-color"]
    if paths:
        cmd += ["--"] + [str(p) for p in paths]

    p = subprocess.run(
        cmd,
        cwd=str(ctx.repo_root),
        capture_output=True,
        text=True,
    )
    # git diff returns 0 even if diff exists; nonzero indicates error
    if p.returncode not in (0,):
        raise RuntimeError((p.stderr or p.stdout).strip() or f"git diff failed: {p.returncode}")

    return {"diff": p.stdout}


git_diff_spec = ToolSpec(
    name="git.diff",
    risk=RiskClass.READ,
    schema={"type": "object", "properties": {"paths": {"type": "array", "items": {"type": "string"}}}, "required": []},
    handler=_git_diff,
)