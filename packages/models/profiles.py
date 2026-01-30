"""Data models for profiles (router.yaml)."""


from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Dict, List


@dataclass(frozen=True)
class ModelSpec:
    """
    A configured model entry from router.yaml.
    """
    name: str
    kind: str  # "null", later "openai_compat", etc.
    capabilities: List[str] = field(default_factory=list)
    params: Dict[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class RoutingSpec:
    """
    Role -> model name mapping from router.yaml.
    """
    mapping: Dict[str, str] = field(default_factory=dict)
