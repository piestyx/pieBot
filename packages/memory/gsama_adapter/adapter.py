"""
GSAMA adapter boundary and persistence for pieBot.
Contract:
    - No other pieBot module imports GSAMA internals directly.
    - Persisted state lives under runtime/state/.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict
import json

from packages.core.codec import canonical_json_bytes
from packages.core.types import StateDelta


DEFAULT_STATE_FILENAME = "gsama_state.json"


@dataclass
class GsamaAdapter:
    """
    Thin boundary around vendor/gsama.

    Contract:
      - No other pieBot module imports GSAMA internals directly.
      - Persisted state lives under runtime/state/.
    """

    state_path: Path
    _state: Dict[str, Any]

    def snapshot(self) -> Dict[str, Any]:
        # Return a deep-ish copy to avoid accidental mutation by callers.
        return json.loads(json.dumps(self._state))

    def serialize(self) -> bytes:
        return canonical_json_bytes(self._state)

    def persist(self) -> None:
        self.state_path.parent.mkdir(parents=True, exist_ok=True)
        self.state_path.write_bytes(self.serialize())

    def apply_delta(self, delta: StateDelta) -> None:
        """
        Stage 2A: allow only trivial patching.
        We keep this deliberately simple until GSAMA canonical delta semantics are nailed.

        delta.patches format: list of {op,path,value?} where path is dot-separated for now.
        Supported ops: "set"
        """
        for p in delta.patches:
            op = p.get("op")
            path = p.get("path")
            if op != "set" or not isinstance(path, str) or not path:
                raise ValueError(f"Unsupported patch: {p}")
            value = p.get("value")
            _set_dot_path(self._state, path, value)
        self.persist()


def _set_dot_path(d: Dict[str, Any], path: str, value: Any) -> None:
    parts = path.split(".")
    cur: Dict[str, Any] = d
    for key in parts[:-1]:
        nxt = cur.get(key)
        if not isinstance(nxt, dict):
            nxt = {}
            cur[key] = nxt
        cur = nxt
    cur[parts[-1]] = value


def load_or_init(path: Path) -> GsamaAdapter:
    """
    Load state from disk or initialize a default skeleton.

    NOTE: We are NOT instantiating vendor GSAMA objects here yet.
    Stage 2A is strictly the persistence + boundary + import fence.
    """
    if path.exists():
        raw = path.read_text(encoding="utf-8")
        state = json.loads(raw) if raw.strip() else {}
    else:
        state = {
            "version": 1,
            "gsama": {
                "notes": "Initialized by pieBot gsama_adapter. Vendor wiring comes in Stage 2+.",
            },
        }
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(canonical_json_bytes(state))
    return GsamaAdapter(state_path=path, _state=state)