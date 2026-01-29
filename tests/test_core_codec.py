"""
Tests for stable_sha256 hashing function in core.codec module. 
Required to ensure consistent hashing of data structures regardless 
of order. Used across the repo for GSAMA caching and deduplication.
"""

from packages.core.codec import stable_sha256
from packages.core.types import TaskRequest, AgentType, ModelRequest

def test_hash_stable_for_equivalent_dict_order():
    a = {"b": 2, "a": 1}
    b = {"a": 1, "b": 2}
    assert stable_sha256(a) == stable_sha256(b)

def test_hash_stable_for_dataclass():
    t1 = TaskRequest(run_id="r1", task_id="t1", user_intent="x", metadata={"b": 2, "a": 1})
    t2 = TaskRequest(run_id="r1", task_id="t1", user_intent="x", metadata={"a": 1, "b": 2})
    assert stable_sha256(t1) == stable_sha256(t2)

def test_model_request_serializable_hashable():
    r = ModelRequest(run_id="r1", agent_type=AgentType.planner, input={"x": [3,2,1]})
    h = stable_sha256(r)
    assert isinstance(h, str) and len(h) == 64
