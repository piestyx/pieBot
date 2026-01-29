"""
Filesystem tools for reading files and listing directories within the repository.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any, Dict

from packages.policy.engine import RiskClass
from packages.tools.registry import ToolSpec, ToolContext


def _resolve_under(root: Path, rel: str) -> Path:
    p = (root / rel).resolve()
    r = root.resolve()
    if r not in p.parents and p != r:
        raise ValueError("path escapes repo root")
    return p


def _list_dir(args: Dict[str, Any], ctx: ToolContext) -> Dict[str, Any]:
    rel = str(args.get("path", "."))
    p = _resolve_under(ctx.repo_root, rel)
    if not p.exists():
        raise FileNotFoundError(rel)
    if not p.is_dir():
        raise NotADirectoryError(rel)
    items = []
    for child in sorted(p.iterdir(), key=lambda x: x.name):
        items.append(
            {
                "name": child.name,
                "is_dir": child.is_dir(),
                "is_file": child.is_file(),
            }
        )
    return {"path": rel, "items": items}


def _read_file(args: Dict[str, Any], ctx: ToolContext) -> Dict[str, Any]:
    rel = str(args.get("path"))
    if not rel:
        raise ValueError("missing path")
    max_bytes = int(args.get("max_bytes", 1_000_000))
    p = _resolve_under(ctx.repo_root, rel)
    if not p.exists():
        raise FileNotFoundError(rel)
    if not p.is_file():
        raise IsADirectoryError(rel)
    size = p.stat().st_size
    if size > max_bytes:
        raise ValueError(f"file too large: {size} > {max_bytes}")
    data = p.read_text(encoding="utf-8", errors="replace")
    return {"path": rel, "size": size, "text": data}


fs_list_dir_spec = ToolSpec(
    name="fs.list_dir",
    risk=RiskClass.READ,
    schema={"type": "object", "properties": {"path": {"type": "string"}}, "required": []},
    handler=_list_dir,
)

fs_read_file_spec = ToolSpec(
    name="fs.read_file",
    risk=RiskClass.READ,
    schema={"type": "object", "properties": {"path": {"type": "string"}, "max_bytes": {"type": "integer"}}, "required": ["path"]},
    handler=_read_file,
)