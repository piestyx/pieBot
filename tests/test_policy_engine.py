"""
Tests for PolicyEngine in policy.engine module. Ensures that EXEC, 
NETWORK, and WRITE actions are handled according to environment 
variable configurations.
"""

from packages.policy.engine import PolicyEngine, RiskClass

def test_exec_blocked_by_default(monkeypatch):
    monkeypatch.delenv("ALLOW_EXEC", raising=False)
    pe = PolicyEngine()
    d = pe.decide("any.exec", RiskClass.EXEC, {})
    assert d.allow is False

def test_network_blocked_by_default(monkeypatch):
    monkeypatch.delenv("ALLOW_NETWORK", raising=False)
    pe = PolicyEngine()
    d = pe.decide("any.net", RiskClass.NETWORK, {})
    assert d.allow is False

def test_write_requires_arm(monkeypatch):
    monkeypatch.setenv("EXECUTION_ARM", "false")
    pe = PolicyEngine()
    d = pe.decide("git.apply_patch", RiskClass.WRITE, {})
    assert d.allow is False
    assert d.requires_approval is True

def test_write_allowed_with_arm_but_requires_approval(monkeypatch):
    monkeypatch.setenv("EXECUTION_ARM", "true")
    pe = PolicyEngine()
    d = pe.decide("git.apply_patch", RiskClass.WRITE, {})
    assert d.allow is True
    assert d.requires_approval is True