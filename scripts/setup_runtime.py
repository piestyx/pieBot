"""
Setup the `runtime/` directory structure by creating necessary 
subdirectories and a sentinel file to prevent accidental commits.
"""

from __future__ import annotations

from pathlib import Path

RUNTIME_DIRS = [
    "runtime/state",
    "runtime/memory",
    "runtime/logs",
    "runtime/artifacts",
    "runtime/artifacts/diffs",
]

def main() -> int:
    root = Path(__file__).resolve().parents[1]
    for rel in RUNTIME_DIRS:
        (root / rel).mkdir(parents=True, exist_ok=True)
    # Sentinel to detect accidental commits / wrong assumptions
    (root / "runtime/.generated").write_text(
        "This directory is generated-only. Do not commit.\n",
        encoding="utf-8",
    )
    return 0

if __name__ == "__main__":
    raise SystemExit(main())