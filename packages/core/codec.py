"""
Docstring for packages.core.codec
"""

from __future__ import annotations

import json
import hashlib
from dataclasses import asdict, is_dataclass
from typing import Any

def canonicalize(obj: Any) -> Any:
    if is_dataclass(obj):
        return canonicalize(asdict(obj))
    if isinstance(obj, dict):
        return {k: canonicalize(obj[k]) for k in sorted(obj.keys())}
    if isinstance(obj, (list, tuple)):
        return [canonicalize(x) for x in obj]
    return obj

def canonical_json_bytes(obj: Any) -> bytes:
    canon = canonicalize(obj)
    s = json.dumps(canon, separators=(",", ":"), ensure_ascii=False)
    return s.encode("utf-8")

def stable_sha256(obj: Any) -> str:
    return hashlib.sha256(canonical_json_bytes(obj)).hexdigest()
