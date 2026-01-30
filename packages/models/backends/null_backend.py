"""Backend wrapper for the deterministic NullModel.
Matches the 5B role-split interface."""


from __future__ import annotations

from typing import List

from packages.core.types import ObservationEvent, ToolPlan, ToolResult, CriticReport
from apps.server.models.null_model import NullModel


class NullBackend:
    """
    Backend wrapper for the deterministic NullModel.
    Matches the 5B role-split interface.
    """

    def __init__(self) -> None:
        self._m = NullModel()

    def plan(self, observation: ObservationEvent) -> ToolPlan:
        return self._m.plan(observation)

    def execute(self, plan: ToolPlan) -> ToolPlan:
        return self._m.execute(plan)

    def critique(self, observation: ObservationEvent, tool_results: List[ToolResult]) -> CriticReport:
        return self._m.critique(observation, tool_results)
