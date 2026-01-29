"""Git-related tools for interacting with the 
repository.
"""

from __future__ import annotations

from typing import Any, Dict, List
import subprocess
import hashlib
from pathlib import Path

from packages.tools.approval import is_approved

from packages.policy.engine import RiskClass
from packages.tools.registry import ToolSpec, ToolContext

"""
Git diff tool.
"""

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

"""
Git apply_patch tool.
"""

def _resolve_under(root: Path, rel: str) -> Path:
    p = (root / rel).resolve()
    r = root.resolve()
    if r not in p.parents and p != r:
        raise ValueError("path escapes runtime root")
    return p

def _sha256_bytes(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()

def _apply_patch(args: Dict[str, Any], ctx: ToolContext) -> Dict[str, Any]:
    diff_file = str(args.get("diff_file") or "").strip()
    token = args.get("approval_token")

    if not diff_file:
        raise ValueError("missing diff_file")
    if "/" in diff_file or "\\" in diff_file or ".." in diff_file:
        raise ValueError("diff_file must be a filename only")
    if not is_approved(token if isinstance(token, str) else None):
        raise PermissionError("approval required")

    diffs_dir = ctx.runtime_root / "artifacts" / "diffs"
    patch_path = _resolve_under(diffs_dir, diff_file)
    if not patch_path.exists():
        raise FileNotFoundError(str(patch_path))
    if not patch_path.is_file():
        raise ValueError("diff_file is not a file")

    data = patch_path.read_bytes()
    diff_hash = _sha256_bytes(data)

    p = subprocess.run(
        ["git", "apply", "--whitespace=nowarn", str(patch_path)],
        cwd=str(ctx.repo_root),
        capture_output=True,
        text=True,
    )
    if p.returncode != 0:
        err = (p.stderr or p.stdout).strip()
        raise RuntimeError(f"git apply failed: {err}")

    return {"applied": True, "diff_file": diff_file, "diff_hash": diff_hash}

git_apply_patch_spec = ToolSpec(
    name="git.apply_patch",
    risk=RiskClass.WRITE,
    schema={
        "type": "object",
        "properties": {
            "diff_file": {"type": "string"},
            "approval_token": {"type": "string"},
        },
        "required": ["diff_file", "approval_token"],
    },
    handler=_apply_patch,
)
