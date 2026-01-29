"""
Registry and invocation for tools available to the agent.
Must go through a central registry for policy enforcement and auditing.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable, Dict, Optional
import traceback
import uuid

from packages.core.types import ToolCall, ToolResult
from packages.policy.engine import PolicyEngine, RiskClass
from packages.core.audit import AuditWriter
from packages.tools.approval import is_approved

ToolHandler = Callable[[Dict[str, Any], "ToolContext"], Dict[str, Any]]


@dataclass(frozen=True)
class ToolSpec:
    name: str
    risk: RiskClass
    schema: Dict[str, Any]  # lightweight JSON-schema-ish for now
    handler: ToolHandler


@dataclass(frozen=True)
class ToolContext:
    repo_root: Path
    runtime_root: Path


class ToolRegistry:
    """
    Single choke point for all tool execution.
    Every invocation:
      - policy decision recorded
      - tool executed recorded
      - result recorded
    """

    def __init__(
        self,
        *,
        policy: PolicyEngine,
        audit: AuditWriter,
        repo_root: Path,
        runtime_root: Path,
    ) -> None:
        self._policy = policy
        self._audit = audit
        self._ctx = ToolContext(repo_root=repo_root, runtime_root=runtime_root)
        self._tools: Dict[str, ToolSpec] = {}

    def register(self, spec: ToolSpec) -> None:
        if spec.name in self._tools:
            raise ValueError(f"Tool already registered: {spec.name}")
        self._tools[spec.name] = spec

    def get_spec(self, name: str) -> Optional[ToolSpec]:
        return self._tools.get(name)

    def invoke(self, *, run_id: str, tool_name: str, args: Dict[str, Any]) -> ToolResult:
        spec = self._tools.get(tool_name)
        call_id = uuid.uuid4().hex

        if spec is None:
            self._audit.append(run_id, "ToolExecuted", {"tool_name": tool_name, "call_id": call_id, "args": args})
            res = ToolResult(run_id=run_id, call_id=call_id, ok=False, result={}, error="unknown tool")
            self._audit.append(run_id, "ToolResultStored", {"tool_name": tool_name, "call_id": call_id, "ok": False, "error": "unknown tool"})
            return res

        # Policy decision (single choke point)
        decision = self._policy.decide(tool_name, spec.risk, args)
        self._audit.append(
            run_id,
            "PolicyDecision",
            {
                "tool_name": tool_name,
                "call_id": call_id,
                "risk": spec.risk.value,
                "allow": decision.allow,
                "requires_approval": decision.requires_approval,
                "reason": decision.reason,
            },
        )

        if not decision.allow:
            self._audit.append(
                run_id,
                "ToolExecuted",
                {"tool_name": tool_name, "call_id": call_id, "args": args, "blocked": True},
            )
            res = ToolResult(
                run_id=run_id,
                call_id=call_id,
                ok=False,
                result={},
                error=f"blocked by policy: {decision.reason}",
            )
            self._audit.append(
                run_id,
                "ToolResultStored",
                {"tool_name": tool_name, "call_id": call_id, "ok": False, "error": res.error},
            )
            return res

        # 4B: enforce approval gate for any tool requiring approval.
        if decision.requires_approval:
            tok = args.get("approval_token")
            approved = is_approved(tok if isinstance(tok, str) else None)

            # Log approval gate result (minimal for now)
            self._audit.append(
                run_id,
                "ApprovalRequested",
                {"tool_name": tool_name, "call_id": call_id, "approved": approved},
            )

            if not approved:
                res = ToolResult(
                    run_id=run_id,
                    call_id=call_id,
                    ok=False,
                    result={},
                    error="approval required",
                )
                self._audit.append(
                    run_id,
                    "ToolResultStored",
                    {"tool_name": tool_name, "call_id": call_id, "ok": False, "error": res.error},
                )
                return res

        self._audit.append(
            run_id,
            "ToolExecuted",
            {"tool_name": tool_name, "call_id": call_id, "args": args},
        )

        try:
            out = spec.handler(args, self._ctx)
            res = ToolResult(run_id=run_id, call_id=call_id, ok=True, result=out, error=None)
        except Exception as e:
            tb = traceback.format_exc(limit=3)
            res = ToolResult(
                run_id=run_id,
                call_id=call_id,
                ok=False,
                result={"traceback": tb},
                error=f"{e.__class__.__name__}: {e}",
            )

        self._audit.append(
            run_id,
            "ToolResultStored",
            {
                "tool_name": tool_name,
                "call_id": call_id,
                "ok": res.ok,
                "error": res.error,
                "result_keys": sorted(list(res.result.keys())),
            },
        )
        return res

