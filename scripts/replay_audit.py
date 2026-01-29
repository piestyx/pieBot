"""
Replay an audit log file.
"""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from packages.core.audit import replay_audit_log  # noqa: E402


def main() -> int:
    if len(sys.argv) != 2:
        print("Usage: scripts/replay_audit.py <path-to-audit.jsonl>")
        return 2
    p = Path(sys.argv[1]).expanduser()
    res = replay_audit_log(p)
    if not res.ok:
        print(f"REPLAY FAIL: {res.error}")
        return 1
    print(f"REPLAY OK: run_id={res.run_id} events={res.events} state_hash={res.replay_state_hash}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())