"""
Deterministic model used for tests and dry runs.
Returns a ToolPlan based on ObservationEvent data.
"""

from __future__ import annotations

import uuid
from pathlib import Path
from typing import Dict, Any

from packages.core.types import AgentType, ObservationEvent, ToolCall, ToolPlan


class NullModel:
    """
    Simple deterministic planner.
    Supported observation formats:
      - kind="file_read", data={"path": "..."}
      - kind="text", data={"text": "..."}  (optional heuristic)
    """

    def propose_plan(self, observation: ObservationEvent) -> ToolPlan:
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

        # default: do nothing
        return ToolPlan(run_id=run_id, agent_type=AgentType.planner, tool_calls=[], note="no-op")
