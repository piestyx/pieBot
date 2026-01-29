"""Tests for audit log replay functionality.
"""

from pathlib import Path

from packages.core.audit import AuditWriter, replay_audit_log


def test_replay_passes(tmp_path: Path):
    log = tmp_path / "runtime" / "logs" / "audit.jsonl"
    w = AuditWriter(log)
    w.append("r1", "RunStarted", {"x": "y"})
    w.append("r1", "ObservationCaptured", {"obs": 1})
    w.append("r1", "RunCompleted", {"ok": True})
    res = replay_audit_log(log)
    assert res.ok is True
    assert res.events == 3
    assert res.replay_state_hash is not None


def test_replay_fails_on_order_change(tmp_path: Path):
    log = tmp_path / "runtime" / "logs" / "audit.jsonl"
    w = AuditWriter(log)
    w.append("r1", "RunStarted", {"x": "y"})
    w.append("r1", "ObservationCaptured", {"obs": 1})
    w.append("r1", "RunCompleted", {"ok": True})

    lines = log.read_text(encoding="utf-8").splitlines()
    # Swap lines 2 and 3 (breaks: terminal not last, hashes also inconsistent after verify)
    lines[1], lines[2] = lines[2], lines[1]
    log.write_text("\n".join(lines) + "\n", encoding="utf-8")

    res = replay_audit_log(log)
    assert res.ok is False