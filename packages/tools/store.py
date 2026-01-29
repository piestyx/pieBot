"""Store tool result payloads as JSON artifacts."""


from __future__ import annotations

from pathlib import Path
from typing import Any, Dict
import hashlib

from packages.core.codec import canonical_json_bytes


def _sha256(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def store_tool_result(runtime_root: Path, call_id: str, payload: Dict[str, Any]) -> Dict[str, Any]:
    """
    Store tool result payload as a JSON artifact under runtime/artifacts/tool_results/.
    Returns path + sha256.
    """
    out_dir = runtime_root / "artifacts" / "tool_results"
    out_dir.mkdir(parents=True, exist_ok=True)
    out_path = out_dir / f"{call_id}.json"

    data = canonical_json_bytes(payload)
    out_path.write_bytes(data)

    return {
        "artifact_path": str(out_path),
        "artifact_hash": _sha256(data),
        "bytes": len(data),
    }