"""
Test for audit log verification
"""

from pathlib import Path

from packages.core.audit import AuditWriter, verify_audit_log


def test_audit_appends_and_verifies(tmp_path: Path):
    log = tmp_path / "runtime" / "logs" / "audit.jsonl"
    w = AuditWriter(log)
    w.append("r1", "RunStarted", {"x": "y"})
    w.append("r1", "RunCompleted", {"ok": True})
    ok, err = verify_audit_log(log)
    assert ok is True
    assert err is None


def test_audit_tamper_detected(tmp_path: Path):
    log = tmp_path / "runtime" / "logs" / "audit.jsonl"
    w = AuditWriter(log)
    w.append("r1", "RunStarted", {"x": "y"})
    w.append("r1", "RunCompleted", {"ok": True})

    # Tamper with first line
    lines = log.read_text(encoding="utf-8").splitlines()
    lines[0] = lines[0].replace('"y"', '"z"')
    log.write_text("\n".join(lines) + "\n", encoding="utf-8")

    ok, err = verify_audit_log(log)
    assert ok is False
    assert err is not None