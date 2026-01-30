"""Tests for ModelRouter."""


from __future__ import annotations

from pathlib import Path

from packages.models import ModelRouter


def test_router_loads_and_resolves_null(tmp_path: Path):
    cfg = tmp_path / "router.yaml"
    cfg.write_text(
        """
models:
  null:
    kind: null
    capabilities: []
routing:
  planner: null
  executor: null
  critic: null
""".strip(),
        encoding="utf-8",
    )

    r = ModelRouter.load(cfg)
    b = r.get_backend_for_role("planner")
    assert hasattr(b, "plan")
