"""
Data types used throughout the system.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Any, Dict, List, Optional, Literal

RunId = str

class AgentType(str, Enum):
    planner = "planner"
    executor = "executor"
    critic = "critic"

@dataclass(frozen=True)
class TaskRequest:
    run_id: RunId
    task_id: str
    user_intent: str
    metadata: Dict[str, Any] = field(default_factory=dict)

@dataclass(frozen=True)
class ToolPlan:
    run_id: RunId
    agent_type: AgentType
    tool_calls: List[ToolCall] = field(default_factory=list)
    note: Optional[str] = None

@dataclass(frozen=True)
class ToolCall:
    run_id: RunId
    tool_name: str
    args: Dict[str, Any]
    call_id: str

@dataclass(frozen=True)
class ToolResult:
    run_id: RunId
    call_id: str
    ok: bool
    result: Dict[str, Any] = field(default_factory=dict)
    error: Optional[str] = None

@dataclass(frozen=True)
class RunResult:
    run_id: RunId
    ok: bool
    tool_results: List[ToolResult] = field(default_factory=list)
    error: Optional[str] = None

@dataclass(frozen=True)
class ModelRequest:
    run_id: RunId
    agent_type: AgentType
    input: Dict[str, Any]

@dataclass(frozen=True)
class ModelReply:
    run_id: RunId
    agent_type: AgentType
    output: Dict[str, Any]

@dataclass(frozen=True)
class StateDelta:
    run_id: RunId
    patches: List[Dict[str, Any]]  # json-patch-like (op/path/value)
    reason: str

AuditEventType = Literal[
    "RunStarted",
    "ObservationCaptured",
    "PlanProposed",
    "PolicyDecision",
    "ApprovalRequested",
    "ApprovalGranted",
    "ApprovalDenied",
    "ToolExecuted",
    "ToolResultStored",
    "StateDeltaApplied",
    "RunCompleted",
    "RunFailed",
]

@dataclass(frozen=True)
class AuditEvent:
    run_id: RunId
    type: AuditEventType
    ts_utc: str  # ISO8601
    payload: Dict[str, Any]
    prev_hash: Optional[str] = None
    hash: Optional[str] = None

@dataclass(frozen=True)
class ObservationEvent:
    run_id: RunId
    kind: str  # e.g. "text", "file"
    data: Dict[str, Any] = field(default_factory=dict)
