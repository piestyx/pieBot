"""Replay audit logs to verify integrity and consistency.

"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Optional
import json

from .writer import verify_audit_log
from packages.core.codec import stable_sha256


@dataclass(frozen=True)
class ReplayResult:
    ok: bool
    error: Optional[str] = None
    events: int = 0
    run_id: Optional[str] = None
    replay_state_hash: Optional[str] = None


def replay_audit_log(path: Path) -> ReplayResult:
    ok, err = verify_audit_log(path)
    if not ok:
        return ReplayResult(ok=False, error=f"audit verification failed: {err}")

    lines = [ln for ln in path.read_text(encoding="utf-8").splitlines() if ln.strip()]
    if not lines:
        return ReplayResult(ok=False, error="empty audit log")

    # Deterministic derived state: hash over (prev_state_hash + event_hash + type)
    state_hash = "GENESIS"

    first = json.loads(lines[0])
    run_id = first.get("run_id")
    if not run_id:
        return ReplayResult(ok=False, error="missing run_id on first event")

    # Ordering invariants (minimal but strict enough to catch corruption)
    seen_start = False
    seen_end = False

    for i, ln in enumerate(lines, start=1):
        ev = json.loads(ln)
        if ev.get("run_id") != run_id:
            return ReplayResult(ok=False, error=f"mixed run_id at line {i}")

        etype = ev.get("type")
        ehash = ev.get("hash")
        if not etype or not ehash:
            return ReplayResult(ok=False, error=f"missing type/hash at line {i}")

        if i == 1:
            if etype != "RunStarted":
                return ReplayResult(ok=False, error="first event must be RunStarted")
            seen_start = True
        else:
            if not seen_start:
                return ReplayResult(ok=False, error="RunStarted missing")
            if seen_end:
                return ReplayResult(ok=False, error="events after terminal event")

        if etype in {"RunCompleted", "RunFailed"}:
            seen_end = True

        # Deterministic replay-state update
        state_hash = stable_sha256({"prev": state_hash, "event_hash": ehash, "type": etype})

    if not seen_end:
        return ReplayResult(ok=False, error="missing terminal event (RunCompleted/RunFailed)")

    return ReplayResult(ok=True, events=len(lines), run_id=run_id, replay_state_hash=state_hash)
