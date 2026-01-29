"""
Simple in-process working memory with TTL and size limits.
Contains:
    - WorkingMemory class for storing key-value pairs with TTL
    - WorkingMemoryFull exception for handling memory full 
      situations
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Dict, Optional
import time


class WorkingMemoryFull(RuntimeError):
    pass


@dataclass
class _Entry:
    value: Any
    expires_at: float
    run_id: Optional[str]
    approx_bytes: int


def _now() -> float:
    return time.time()


def _approx_size(value: Any) -> int:
    # Cheap + deterministic-ish sizing. We don't want deep recursion here.
    # Enough to enforce a cap and fail closed.
    try:
        s = repr(value)
    except Exception:
        s = "<unrepr>"
    return len(s.encode("utf-8", errors="ignore"))


class WorkingMemory:
    """
    In-process short-term memory.
    - TTL-based expiry
    - Hard cap on entry count and approx bytes
    - Optional run_id scoping, with clear_run(run_id)
    """

    def __init__(self, max_entries: int = 256, max_bytes: int = 256_000) -> None:
        if max_entries <= 0:
            raise ValueError("max_entries must be > 0")
        if max_bytes <= 0:
            raise ValueError("max_bytes must be > 0")
        self.max_entries = max_entries
        self.max_bytes = max_bytes
        self._items: Dict[str, _Entry] = {}
        self._bytes_used = 0

    def _evict_expired(self) -> None:
        t = _now()
        expired = [k for k, e in self._items.items() if e.expires_at <= t]
        for k in expired:
            self._drop(k)

    def _drop(self, key: str) -> None:
        e = self._items.pop(key, None)
        if e is not None:
            self._bytes_used = max(0, self._bytes_used - e.approx_bytes)

    def get(self, key: str) -> Optional[Any]:
        self._evict_expired()
        e = self._items.get(key)
        if not e:
            return None
        return e.value

    def set(self, key: str, value: Any, ttl_seconds: float, run_id: Optional[str] = None) -> bool:
        if ttl_seconds <= 0:
            # Fail closed: don't store garbage entries.
            return False

        self._evict_expired()

        approx = _approx_size(value)
        expires_at = _now() + float(ttl_seconds)

        # If overwriting, remove old first so accounting is correct.
        if key in self._items:
            self._drop(key)

        # Hard cap checks (fail closed).
        if len(self._items) + 1 > self.max_entries:
            return False
        if self._bytes_used + approx > self.max_bytes:
            return False

        self._items[key] = _Entry(value=value, expires_at=expires_at, run_id=run_id, approx_bytes=approx)
        self._bytes_used += approx
        return True

    def clear_run(self, run_id: str) -> None:
        self._evict_expired()
        doomed = [k for k, e in self._items.items() if e.run_id == run_id]
        for k in doomed:
            self._drop(k)

    def clear_all(self) -> None:
        self._items.clear()
        self._bytes_used = 0

    def stats(self) -> Dict[str, Any]:
        self._evict_expired()
        return {
            "entries": len(self._items),
            "bytes_used": self._bytes_used,
            "max_entries": self.max_entries,
            "max_bytes": self.max_bytes,
        }