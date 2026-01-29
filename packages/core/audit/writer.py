"""
Append-only audit log with hash chaining. 
Each entry is a JSON object with fields:
  - run_id: str
  - type: str
  - ts_utc: str (ISO 8601 UTC timestamp)
  - payload: Dict[str, Any] (redacted)
  - prev_hash: Optional[str] (hash of previous entry)
  - hash: str (SHA-256 hash of the entry excluding the hash field itself)
"""

from __future__ import annotations

from dataclasses import asdict
from pathlib import Path
from typing import Any, Dict, Optional, Tuple
import json
import time

from packages.core.codec import stable_sha256, canonical_json_bytes
from packages.core.types import AuditEvent
from packages.policy.engine import redact_text


def _utc_iso() -> str:
    # Keep it deterministic enough; real-time is fine here because audit is time-stamped truth.
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


def _redact_payload(payload: Dict[str, Any]) -> Dict[str, Any]:
    """
    Conservative redaction:
      - If a value is a string, run redact_text()
      - If dict/list, recurse
      - Else keep as-is
    """
    def walk(v: Any) -> Any:
        if isinstance(v, str):
            return redact_text(v)
        if isinstance(v, dict):
            return {k: walk(v[k]) for k in v}
        if isinstance(v, list):
            return [walk(x) for x in v]
        return v

    return walk(payload)  # type: ignore[return-value]


class AuditWriter:
    """
    Append-only JSONL audit log with hash chaining.

    File format: one JSON object per line:
      { run_id, type, ts_utc, payload, prev_hash, hash }
    """

    def __init__(self, path: Path) -> None:
        self.path = path
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self._last_hash: Optional[str] = None
        if self.path.exists():
            # Recover last hash from file tail if present.
            self._last_hash = _read_last_hash(self.path)

    def append(self, run_id: str, type: str, payload: Dict[str, Any]) -> AuditEvent:
        ev = AuditEvent(
            run_id=run_id,
            type=type,  # AuditEventType validated by caller at higher layers
            ts_utc=_utc_iso(),
            payload=_redact_payload(payload),
            prev_hash=self._last_hash,
            hash=None,
        )
        # Hash must be computed over canonical representation excluding the hash field itself.
        ev_dict = asdict(ev)
        ev_dict["hash"] = None
        h = stable_sha256(ev_dict)
        ev = AuditEvent(**{**asdict(ev), "hash": h})
        self._write_line(ev)
        self._last_hash = h
        return ev

    def _write_line(self, ev: AuditEvent) -> None:
        line = canonical_json_bytes(asdict(ev)).decode("utf-8")
        with self.path.open("a", encoding="utf-8") as f:
            f.write(line + "\n")


def _read_last_hash(path: Path) -> Optional[str]:
    try:
        with path.open("rb") as f:
            f.seek(0, 2)
            size = f.tell()
            if size == 0:
                return None
            # Read last ~8KB for tail scan (enough for last line)
            f.seek(max(0, size - 8192))
            tail = f.read().decode("utf-8", errors="ignore").splitlines()
            for line in reversed(tail):
                line = line.strip()
                if not line:
                    continue
                obj = json.loads(line)
                return obj.get("hash")
    except Exception:
        return None
    return None


def verify_audit_log(path: Path) -> Tuple[bool, Optional[str]]:
    """
    Verify hash chain and hashes.
    Returns (ok, error_message).
    """
    if not path.exists():
        return False, "audit log does not exist"

    prev: Optional[str] = None
    line_no = 0
    for line_no, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if not line.strip():
            continue
        obj = json.loads(line)
        # Check prev hash consistency
        if obj.get("prev_hash") != prev:
            return False, f"prev_hash mismatch at line {line_no}"
        # Recompute hash
        expected_hash = obj.get("hash")
        obj2 = dict(obj)
        obj2["hash"] = None
        actual = stable_sha256(obj2)
        if expected_hash != actual:
            return False, f"hash mismatch at line {line_no}"
        prev = expected_hash
    return True, None