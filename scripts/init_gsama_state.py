from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from packages.memory.gsama_adapter import load_or_init  # noqa: E402
from packages.memory.gsama_adapter.adapter import DEFAULT_STATE_FILENAME  # noqa: E402

def main() -> int:
    state_path = ROOT / "runtime" / "state" / DEFAULT_STATE_FILENAME
    load_or_init(state_path)
    print(f"OK: gsama state initialized at {state_path}")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())