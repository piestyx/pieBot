"""
Deterministic model used for tests and dry runs.
Stage 5B: planner -> executor -> critic pipeline.

- plan(): produces a ToolPlan from an ObservationEvent
- execute(): can transform/validate a plan (noop for NullModel)
- critique(): decides passed/retry/failed deterministically
"""

from __future__ import annotations

import uuid

from typing import List

from packages.core.types import (
    AgentType,
    ObservationEvent,
    ToolCall,
    ToolPlan,
    ToolResult,
    CriticDecision,
    CriticReport,
)


class NullModel:
    """
    Simple deterministic planner.
    Supported observation formats:
      - kind="file_read", data={"path": "..."}
      - kind="text", data={"text": "..."}  (optional heuristic)
    """

    def plan(self, observation: ObservationEvent) -> ToolPlan:
        run_id = observation.run_id

        if observation.kind == "file_read":
            path = str(observation.data.get("path", "")).strip()
            if not path:
                return ToolPlan(run_id=run_id, agent_type=AgentType.planner, tool_calls=[], note="missing path")

            call = ToolCall(
                run_id=run_id,
                tool_name="fs.read_file",
                args={"path": path},
                call_id=uuid.uuid4().hex,
            )
            return ToolPlan(run_id=run_id, agent_type=AgentType.planner, tool_calls=[call], note="read file")

        return ToolPlan(run_id=run_id, agent_type=AgentType.planner, tool_calls=[], note="no-op")

    def execute(self, plan: ToolPlan) -> ToolPlan:
        # Null executor does not transform the plan; it only relabels the stage.
        return ToolPlan(run_id=plan.run_id, agent_type=AgentType.executor, tool_calls=plan.tool_calls, note=plan.note)

    def critique(self, observation: ObservationEvent, tool_results: List[ToolResult]) -> CriticReport:
        run_id = observation.run_id

        if any(not r.ok for r in tool_results):
            # Deterministic: request retry when any tool fails.
            # Orchestrator will cap attempts and convert final retry->failed.
            return CriticReport(run_id=run_id, decision=CriticDecision.retry, reason="tool failure")

        return CriticReport(run_id=run_id, decision=CriticDecision.passed, reason="all tool calls ok")
