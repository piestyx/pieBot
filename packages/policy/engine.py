"""
Policy engine for deciding whether to allow tool actions based on 
risk classes and environment configurations. 

Ensures: 
  - READ actions are always allowed. 
  - EXEC actions are denied unless explicitly allowed via environment 
    variable. 
  - NETWORK actions are denied unless explicitly allowed via 
    environment variable. 
  - WRITE actions are denied unless execution is armed via environment 
    variable, requiring approval.
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from typing import Any, Dict, Optional
import os
import re

class RiskClass(str, Enum):
    READ = "READ"
    WRITE = "WRITE"
    EXEC = "EXEC"
    NETWORK = "NETWORK"

@dataclass(frozen=True)
class PolicyDecision:
    allow: bool
    reason: str
    requires_approval: bool = False

REDACT_PATTERNS = [
    re.compile(r"(?i)(api[_-]?key\s*[:=]\s*)(['\"][^'\"]+['\"])"),
    re.compile(r"(?i)(authorization\s*[:=]\s*)(['\"][^'\"]+['\"])"),
    re.compile(r"(?i)sk-[A-Za-z0-9]{20,}"),
]

def redact_text(s: str) -> str:
    out = s
    for pat in REDACT_PATTERNS:
        out = pat.sub("[REDACTED]", out)
    return out

def env_flag(name: str, default: str = "false") -> bool:
    v = os.environ.get(name, default).strip().lower()
    return v in {"1", "true", "yes", "y", "on"}

class PolicyEngine:
    """
    Defaults:
      - EXEC denied unless ALLOW_EXEC=true
      - NETWORK denied unless ALLOW_NETWORK=true
      - WRITE denied unless EXECUTION_ARM=true (+ approval)
    """
    def __init__(
        self,
        execution_arm_env: str = "EXECUTION_ARM",
        allow_exec_env: str = "ALLOW_EXEC",
        allow_network_env: str = "ALLOW_NETWORK",
    ) -> None:
        self.execution_arm_env = execution_arm_env
        self.allow_exec_env = allow_exec_env
        self.allow_network_env = allow_network_env

    def decide(self, tool_name: str, risk: RiskClass, args: Optional[Dict[str, Any]] = None) -> PolicyDecision:
        args = args or {}
        armed = env_flag(self.execution_arm_env, "false")
        allow_exec = env_flag(self.allow_exec_env, "false")
        allow_network = env_flag(self.allow_network_env, "false")

        if risk == RiskClass.READ:
            return PolicyDecision(True, "READ allowed by default", False)

        if risk == RiskClass.EXEC and not allow_exec:
            return PolicyDecision(False, "EXEC denied by default (ALLOW_EXEC=false)", False)

        if risk == RiskClass.NETWORK and not allow_network:
            return PolicyDecision(False, "NETWORK denied by default (ALLOW_NETWORK=false)", False)

        if risk == RiskClass.WRITE and not armed:
            return PolicyDecision(False, "WRITE denied (EXECUTION_ARM=false)", True)

        # If we are here, risk is allowed by config, but may still require approval for mutation.
        if risk in {RiskClass.WRITE, RiskClass.EXEC, RiskClass.NETWORK}:
            return PolicyDecision(True, f"{risk.value} allowed by config; approval required", True)

        return PolicyDecision(False, "Unknown risk class", False)