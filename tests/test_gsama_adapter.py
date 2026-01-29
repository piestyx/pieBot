"""
Tests for GSAMAAdapter in memory.gsama_adapter module. Ensures that 
state loading, initialization, and delta application work as expected.
"""

from pathlib import Path

from packages.memory.gsama_adapter import load_or_init
from packages.core.types import StateDelta


def test_load_or_init_creates_state(tmp_path: Path):
    p = tmp_path / "runtime" / "state" / "gsama_state.json"
    a = load_or_init(p)
    assert p.exists()
    snap = a.snapshot()
    assert snap["version"] == 1


def test_apply_delta_persists(tmp_path: Path):
    p = tmp_path / "runtime" / "state" / "gsama_state.json"
    a = load_or_init(p)
    d = StateDelta(run_id="r1", patches=[{"op": "set", "path": "gsama.counter", "value": 123}], reason="test")
    a.apply_delta(d)
    a2 = load_or_init(p)
    assert a2.snapshot()["gsama"]["counter"] == 123