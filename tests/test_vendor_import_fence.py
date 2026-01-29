"""
Tests to ensure that no code outside of the memory.gsama_adapter 
module imports vendor.gsama internals directly. This enforces the 
intended abstraction boundary around GSAMA usage.
"""

from __future__ import annotations

from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]

# Anything that suggests importing vendor gsama internals directly.
BAD_PATTERNS = [
    re.compile(r"^\s*import\s+vendor\.gsama", re.M),
    re.compile(r"^\s*from\s+vendor\.gsama\s+import", re.M),
]

ALLOWED_PREFIX = str(Path("packages/memory/gsama_adapter").as_posix())


def _git_ls_files_py() -> list[str]:
    p = subprocess.run(["git", "ls-files", "*.py"], cwd=ROOT, capture_output=True, text=True)
    assert p.returncode == 0, p.stderr
    return [x.strip() for x in p.stdout.splitlines() if x.strip()]


def test_no_vendor_gsama_imports_outside_adapter():
    offenders: list[str] = []
    for rel in _git_ls_files_py():
        if rel.startswith(ALLOWED_PREFIX):
            continue
        text = (ROOT / rel).read_text(encoding="utf-8", errors="ignore")
        for pat in BAD_PATTERNS:
            if pat.search(text):
                offenders.append(rel)
                break
    assert not offenders, f"Vendor GSAMA imports must be adapter-only. Offenders: {offenders}"