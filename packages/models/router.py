"""Model Router
Routes roles to model specifications and backends.
"""


from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Dict

import yaml  # requires pyyaml

from packages.models.profiles import ModelSpec, RoutingSpec
from packages.models.backends.null_backend import NullBackend


@dataclass
class ModelRouter:
    models: Dict[str, ModelSpec]
    routing: RoutingSpec

    @staticmethod
    def _norm_scalar(v: Any) -> str:
        """
        YAML parses the literal `null` into Python None.
        We treat None as the canonical string "null" for model ids and kinds.
        """
        if v is None:
            return "null"
        return str(v)


    @classmethod
    def load(cls, path: Path) -> "ModelRouter":
        data = yaml.safe_load(path.read_text(encoding="utf-8")) or {}
        models_raw = (data.get("models") or {})
        routing_raw = (data.get("routing") or {})

        models: Dict[str, ModelSpec] = {}
        for name, cfg in models_raw.items():
            if cfg is None:
                continue
            name_s = cls._norm_scalar(name)
            kind_s = cls._norm_scalar(cfg.get("kind", "")).strip()
            caps = list(cfg.get("capabilities") or [])
            params = dict(cfg.get("params") or {})
            models[name_s] = ModelSpec(name=name_s, kind=kind_s, capabilities=caps, params=params)


        routing = RoutingSpec(mapping={cls._norm_scalar(k): cls._norm_scalar(v) for k, v in routing_raw.items()})
        return cls(models=models, routing=routing)

    def get_backend_for_role(self, role: str):
        """
        Resolve a role (e.g. 'planner') into a backend instance.
        """
        model_name = self.routing.mapping.get(role)
        if not model_name:
            raise KeyError(f"no model routed for role: {role}")

        spec = self.models.get(model_name)
        if not spec:
            raise KeyError(f"routed model not defined: role={role} model={model_name}")

        if spec.kind == "null":
            return NullBackend()

        raise NotImplementedError(f"model kind not implemented: {spec.kind}")
