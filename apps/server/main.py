"""
Docstring for apps.server.main
"""

from __future__ import annotations

import argparse
import uuid
from pathlib import Path

from packages.policy.engine import PolicyEngine
from packages.tools.registry import ToolRegistry
from packages.tools.builtins import fs_list_dir_spec, fs_read_file_spec, git_diff_spec, git_apply_patch_spec

from apps.server.audit import get_audit_writer
from apps.server.models.null_model import NullModel
from packages.models import ModelRouter
from apps.server.orchestrator import Orchestrator
from packages.core.types import ObservationEvent


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--runtime", default="runtime", help="runtime directory")
    ap.add_argument("--read-file", default="", help="relative path to read (repo-root relative)")
    ap.add_argument("--router", default="configs/router.yaml", help="path to router.yaml")
    args = ap.parse_args()

    runtime_root = Path(args.runtime)
    repo_root = Path(".")

    audit = get_audit_writer(runtime_root)
    policy = PolicyEngine()
    tools = ToolRegistry(policy=policy, audit=audit, repo_root=repo_root, runtime_root=runtime_root)

    tools.register(fs_list_dir_spec)
    tools.register(fs_read_file_spec)
    tools.register(git_diff_spec)
    tools.register(git_apply_patch_spec)

    router = ModelRouter.load(Path(args.router))
    orch = Orchestrator(tools=tools, audit=audit, router=router, runtime_root=runtime_root)


    run_id = uuid.uuid4().hex
    obs = ObservationEvent(run_id=run_id, kind="file_read", data={"path": args.read_file}) if args.read_file else ObservationEvent(run_id=run_id, kind="text", data={"text": ""})

    res = orch.run_once(obs)
    print(res)
    return 0 if res.ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
