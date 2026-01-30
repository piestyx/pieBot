"""
Stage 6A orchestrator: planner -> executor -> critic pipeline with bounded retries,
where models are resolved by role via ModelRouter.
Deterministic, replayable, no model calls during replay.
"""

from __future__ import annotations

from pathlib import Path
from typing import List

from packages.core.types import ObservationEvent, RunResult, ToolPlan, ToolResult, CriticDecision
from packages.core.audit import AuditWriter
from packages.tools.registry import ToolRegistry
from packages.models.router import ModelRouter


class Orchestrator:
    def __init__(
        self,
        *,
        tools: ToolRegistry,
        audit: AuditWriter,
        router: ModelRouter,
        runtime_root: Path,
        max_attempts: int = 2,
    ) -> None:
        self._tools = tools
        self._audit = audit
        self._router = router
        self._runtime_root = runtime_root
        self._max_attempts = max_attempts

    def _audit_plan(self, run_id: str, plan: ToolPlan, attempt: int) -> None:
        self._audit.append(
            run_id,
            "PlanProposed",
            {
                "attempt": attempt,
                "agent_type": plan.agent_type.value,
                "tool_calls": [
                    {"tool_name": c.tool_name, "args": c.args, "call_id": c.call_id}
                    for c in plan.tool_calls
                ],
                "note": plan.note,
            },
        )

    def run_once(self, observation: ObservationEvent) -> RunResult:
        run_id = observation.run_id

        self._audit.append(run_id, "RunStarted", {"run_id": run_id})
        self._audit.append(run_id, "ObservationCaptured", {"kind": observation.kind, "data": observation.data})

        results: List[ToolResult] = []

        for attempt in range(1, self._max_attempts + 1):
            try:
                planner = self._router.get_backend_for_role("planner")
                executor = self._router.get_backend_for_role("executor")
                critic = self._router.get_backend_for_role("critic")
                # PLANNER
                plan: ToolPlan = planner.plan(observation)
                self._audit_plan(run_id, plan, attempt)

                # EXECUTOR (may transform plan)
                exec_plan: ToolPlan = executor.execute(plan)
                self._audit_plan(run_id, exec_plan, attempt)

                # Execute tools (registry handles policy/approval/audit/artifacts)
                attempt_results: List[ToolResult] = []
                for call in exec_plan.tool_calls:
                    res = self._tools.invoke(run_id=run_id, tool_name=call.tool_name, args=call.args)
                    attempt_results.append(res)
                    results.append(res)

                # CRITIC
                report = critic.critique(observation, attempt_results)
                self._audit.append(
                    run_id,
                    "CriticReport",
                    {
                        "attempt": attempt,
                        "decision": report.decision.value,
                        "reason": report.reason,
                        "retry_hint": report.retry_hint,
                    },
                )

                if report.decision == CriticDecision.passed:
                    self._audit.append(run_id, "RunCompleted", {"attempts": attempt})
                    return RunResult(run_id=run_id, ok=True, tool_results=results)

                if report.decision == CriticDecision.retry:
                    if attempt < self._max_attempts:
                        continue
                    # final attempt exhausted -> fail
                    self._audit.append(run_id, "RunFailed", {"error": report.reason, "attempts": attempt})
                    return RunResult(run_id=run_id, ok=False, tool_results=results, error=report.reason)

                # failed
                self._audit.append(run_id, "RunFailed", {"error": report.reason, "attempts": attempt})
                return RunResult(run_id=run_id, ok=False, tool_results=results, error=report.reason)

            except Exception as e:
                err = f"{e.__class__.__name__}: {e}"
                self._audit.append(run_id, "RunFailed", {"error": err, "attempts": attempt})
                return RunResult(run_id=run_id, ok=False, tool_results=results, error=err)

        # Should be unreachable due to return paths above.
        self._audit.append(run_id, "RunFailed", {"error": "max attempts exceeded"})
        return RunResult(run_id=run_id, ok=False, tool_results=results, error="max attempts exceeded")

