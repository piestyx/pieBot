"""
Audit helpers for server app. No side effects on import.
"""

from __future__ import annotations

from pathlib import Path

from packages.core.audit import AuditWriter


def get_audit_writer(runtime_root: Path) -> AuditWriter:
    return AuditWriter(runtime_root / "logs" / "audit.jsonl")
