"""
Tests to verify the functionality of the WorkingMemory class, 
including TTL expiry, hard caps on entries and bytes, and run_id 
scoping.
"""

import time

from packages.memory.working_memory import WorkingMemory


def test_ttl_expiry():
    wm = WorkingMemory(max_entries=10, max_bytes=10_000)
    assert wm.set("k", "v", ttl_seconds=0.01) is True
    assert wm.get("k") == "v"
    time.sleep(0.02)
    assert wm.get("k") is None


def test_hard_cap_entries_fail_closed():
    wm = WorkingMemory(max_entries=1, max_bytes=10_000)
    assert wm.set("a", "1", ttl_seconds=10) is True
    assert wm.set("b", "2", ttl_seconds=10) is False


def test_hard_cap_bytes_fail_closed():
    wm = WorkingMemory(max_entries=10, max_bytes=5)  # tiny
    assert wm.set("a", "1234567890", ttl_seconds=10) is False


def test_clear_run():
    wm = WorkingMemory(max_entries=10, max_bytes=10_000)
    assert wm.set("r1.k1", "v1", ttl_seconds=10, run_id="r1") is True
    assert wm.set("r2.k1", "v2", ttl_seconds=10, run_id="r2") is True
    wm.clear_run("r1")
    assert wm.get("r1.k1") is None
    assert wm.get("r2.k1") == "v2"