"""
CI checks for the repository.
Ensures:
- runtime/ is not tracked
- Canonical docs exist
- No secrets in tracked files
- No public bind defaults (0.0.0.0 / ::)
- vendor/gsama is pinned to a tag
"""

from __future__ import annotations

import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

CANON_DOCS = [
    "docs/PROJECT_OVERVIEW.md",
    "docs/ARCHITECTURE_MAP.md",
    "docs/SECURITY_POSTURE.md",
    "docs/PROVENANCE.md",
    "docs/CONTRACTS.md",
]

SECRET_PATTERNS = [
    re.compile(r"(?i)api[_-]?key\s*[:=]\s*['\"][A-Za-z0-9_\-]{16,}['\"]"),
    re.compile(r"(?i)sk-[A-Za-z0-9]{20,}"),  # common OpenAI-ish shape
    re.compile(r"(?i)anthropic[_-]?api[_-]?key\s*[:=]"),
    re.compile(r"(?i)BEGIN\s+PRIVATE\s+KEY"),
]

PUBLIC_BIND_PATTERNS = [
    re.compile(r"0\.0\.0\.0"),
    re.compile(r"\[::\]"),
    re.compile(r"host\s*=\s*['\"]0\.0\.0\.0['\"]"),
]

def _run(cmd: list[str]) -> tuple[int, str]:
    p = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
    out = (p.stdout or "") + (p.stderr or "")
    return p.returncode, out.strip()

def fail(msg: str) -> None:
    raise SystemExit(f"FAIL: {msg}")

def check_runtime_not_tracked() -> None:
    rc, out = _run(["git", "ls-files", "runtime"])
    if rc != 0:
        fail(f"git ls-files failed: {out}")
    if out.strip():
        fail("runtime/ contains tracked files. runtime/ MUST be generated-only and gitignored.")

def check_canon_docs_exist() -> None:
    missing = [p for p in CANON_DOCS if not (ROOT / p).exists()]
    if missing:
        fail(f"Missing canonical docs: {missing}")

def iter_repo_text_files() -> list[Path]:
    # Only scan tracked files to avoid dev junk.
    rc, out = _run(["git", "ls-files"])
    if rc != 0:
        fail(f"git ls-files failed: {out}")
    files = []
    for rel in out.splitlines():
        p = ROOT / rel
        if p.is_file() and p.suffix.lower() in {".py", ".md", ".txt", ".json", ".yaml", ".yml", ".toml", ".ini"}:
            files.append(p)
    return files

def check_no_secrets() -> None:
    offenders: list[str] = []
    for p in iter_repo_text_files():
        try:
            text = p.read_text(encoding="utf-8", errors="ignore")
        except Exception:
            continue
        for pat in SECRET_PATTERNS:
            if pat.search(text):
                offenders.append(str(p.relative_to(ROOT)))
                break
    if offenders:
        fail(f"Potential secrets detected in: {sorted(set(offenders))}")

def check_no_public_bind_defaults() -> None:
    offenders: list[str] = []
    for p in iter_repo_text_files():
        try:
            text = p.read_text(encoding="utf-8", errors="ignore")
        except Exception:
            continue
        for pat in PUBLIC_BIND_PATTERNS:
            if pat.search(text):
                offenders.append(str(p.relative_to(ROOT)))
                break
    if offenders:
        fail(f"Public bind patterns found (0.0.0.0 / ::). Default must be localhost-only. Files: {sorted(set(offenders))}")

def check_gsama_pinned_to_tag() -> None:
    # Enforce: vendor/gsama is a submodule AND the current commit is exactly at a tag.
    gm = ROOT / ".gitmodules"
    if not gm.exists():
        # If you haven’t added gsama yet, don’t block Stage 1 — but Stage 2A will.
        return
    text = gm.read_text(encoding="utf-8", errors="ignore")
    if "path = vendor/gsama" not in text:
        return
    if not (ROOT / "vendor/gsama").exists():
        fail("vendor/gsama is declared in .gitmodules but directory is missing.")

    rc, out = _run(["git", "-C", "vendor/gsama", "describe", "--tags", "--exact-match"])
    if rc != 0 or not out.strip():
        fail("vendor/gsama is not pinned to an exact tag (git describe --exact-match failed). Pin to a tag per GSAMA contract.")

def main() -> int:
    check_runtime_not_tracked()
    check_canon_docs_exist()
    check_no_secrets()
    check_no_public_bind_defaults()
    check_gsama_pinned_to_tag()
    print("OK: repo checks passed")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())