"""
Stage 5A orchestrator: single tick, deterministic wiring.
No UI, no async, no background loops.
"""

from __future__ import annotations

import uuid
from dataclasses import asdict
from pathlib import Path
from typing import List

from packages.core.types import ObservationEvent, RunResult, ToolPlan, ToolResult
from packages.core.audit import AuditWriter, verify_audit_log
from packages.tools.registry import ToolRegistry


class Orchestrator:
    def __init__(
        self,
        *,
        tools: ToolRegistry,
        audit: AuditWriter,
        model,  # NullModel or real model later
        runtime_root: Path,
    ) -> None:
        self._tools = tools
        self._audit = audit
        self._model = model
        self._runtime_root = runtime_root

    def run_once(self, observation: ObservationEvent) -> RunResult:
        run_id = observation.run_id

        self._audit.append(run_id, "RunStarted", {"run_id": run_id})
        self._audit.append(run_id, "ObservationCaptured", {"kind": observation.kind, "data": observation.data})

        try:
            plan: ToolPlan = self._model.propose_plan(observation)
            self._audit.append(
                run_id,
                "PlanProposed",
                {
                    "agent_type": plan.agent_type.value,
                    "tool_calls": [
                        {"tool_name": c.tool_name, "args": c.args, "call_id": c.call_id}
                        for c in plan.tool_calls
                    ],
                    "note": plan.note,
                },
            )

            results: List[ToolResult] = []
            for call in plan.tool_calls:
                # Registry is the choke point: policy + approval + audit + artifact store happen there.
                res = self._tools.invoke(run_id=run_id, tool_name=call.tool_name, args=call.args)
                results.append(res)

                # Fail-fast in 5A (simple). 5B will add retry/critic patterns.
                if not res.ok:
                    self._audit.append(run_id, "RunFailed", {"error": res.error, "call_id": res.call_id})
                    return RunResult(run_id=run_id, ok=False, tool_results=results, error=res.error)

            self._audit.append(run_id, "RunCompleted", {"tool_calls": len(plan.tool_calls)})
            return RunResult(run_id=run_id, ok=True, tool_results=results)

        except Exception as e:
            self._audit.append(run_id, "RunFailed", {"error": f"{e.__class__.__name__}: {e}"})
            return RunResult(run_id=run_id, ok=False, tool_results=[], error=f"{e.__class__.__name__}: {e}")
