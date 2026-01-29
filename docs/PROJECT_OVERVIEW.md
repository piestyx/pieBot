# pieBot — Project Overview & Operating Principles

> Internal architecture + execution guardrails. This is the canonical reference for what pieBot is, what it is not, and how decisions are made.

---

## 1. Purpose

pieBot is a **local‑first AI system** built around architectural leverage, not model size.

Goals:

- Deliver high‑quality agent workflows using **cheap / local models** where possible.
- Externalize identity, memory, and task state to avoid context bloat.
- Treat LLMs as **replaceable workers**, not the system itself.
- Enable long‑horizon tasks with deterministic recovery and auditability.
- Remain inspectable, debuggable, and boring in the right places.

Non‑goals:

- Anthropomorphic behavior
- “AI companion” design
- Cloud‑locked cognition
- Infinite context stuffing
- Token‑heavy identity replay

---

## 2. System Philosophy

### Intelligence placement

Intelligence lives in:

- State structure
- Causal ordering
- Memory stratification
- Task decomposition
- Tool orchestration

Not in:

- Prompt size
- Model branding
- Persona text

### Design law

> Models transform state. They do not _own_ it.

---

## 3. Core Architecture

```
┌──────────────────────────────────────┐
| Environment / Sensors / Files / APIs |
└──────────────────────────────────────┘
                   ↓
            ┌─────────────┐
            │ Orchestrator│
            └──────┬──────┘
                   ↓
           ┌────────────────┐
           │   GSAMA Layer  │   ← identity / goals / trajectory / constraints
           └────────────────┘
                   ↓
        ┌───────────────────────┐
        │ Memory System (3-tier)│
        └───────────────────────┘
                   ↓
           ┌────────────────┐
           │  Model Router  │
           └────────────────┘
                   ↓
         ┌───────────────────┐
         | Specialist Models |
         └───────────────────┘
```

---

## 4. Memory Model (mandatory)

### Layer 1 — GSAMA (State / Identity)

- Long‑lived system identity
- Goals    
- Active trajectories
- Constraints
- Commitments
- Causal ordering

Never stored as raw chat history.  
Never reconstructed from tokens.
Can store trajectory using the below type of schema

`{"schema_version": 1, "timestamp": 1767244581.1718805, "tick_index": 0, "run_id": "17b272d6-f922-494b-ad4b-f4cf62fea0bf", "subsystem": "gameplay", "phase": "bootstrap_test", "entropy": 0.0, "tags": {"run_id": "17b272d6-f922-494b-ad4b-f4cf62fea0bf", "subsystem": "gameplay", "phase": "bootstrap_test"}, "z_b64": "YGBFPbNoKL4kDPM9QU8YPjv4nb4M3lK+Jp2lPIvXTL2nHy67ESMKvpRnDj735vs9ABUrPPWJNj5XaZc9GiYLvnvabj2gRhu+SkAOPvtaAbz7e++8FIjcvaP4RT5ZMMi8y7iKvQAXZL3rZaw9KbZsPcmrhT2Ah4s9KmetPv6fg7005qW9E8cDvgt/xz240TY+rZ2TvNEMCL7+ggW+2bTSPW+38D0W6a89nYnXvSJhFj3TKZc8FKcNPTcdDT7A1BA97uDbPQMYLzz0RTs9TXTMPZ72a74UEE+9vlaYvY/pzr1CODK9BhVyPio1DL4hzBw+0EGIvtfqWL2g19I84Nu9PQ==", "gsama_entry_id": "selftest_17b272d6-f922-494b-ad4b-f4cf62fea0bf"}`

### Layer 2 — OpenMemory (Episodic / Structured)

- Task history
- Project states
- Named threads
- File‑linked context
- Tool outputs

Indexed. Addressable. Mutable.

### Layer 3 — Working Memory (Ephemeral)

- Current task window
- Tool call parameters
- Temporary scratchpad

Cleared aggressively.

---

## 5. Orchestrator Responsibilities

The orchestrator:

- Observes environment inputs (files, schedules, events, sensors)
- Determines next task based on GSAMA state
- Selects required agent type
- Routes to appropriate model via router
- Enforces mutation boundaries
- Validates state deltas
- Handles retries / rollback
- Schedules follow‑ups

It is deterministic.  
It is model‑agnostic.

---

## 6. Model Router

Mapping: `AgentType → Model`

Examples:

|Agent Type|Model Class|
|---|---|
|Planner|reasoning‑optimized local model|
|Coder|code‑tuned model|
|Summarizer|small fast model|
|Critic|cheap verifier|

Rules:

- Only one model loaded at a time (VRAM discipline)
- Hot‑swap allowed
- No identity stored in prompt
- All state passed as structured object

---

## 7. Tooling Model

Tools are:

- Deterministic
- Auditable
- Permission‑gated
- Invoked via explicit schema

Tool results become memory objects, not chat text.

Look again at the CaveAgent implementation for their state based mechanics and code execution

---

## 8. UX Principles

The UI must:

- Expose filesystem state
- Expose memory layers
- Expose diffs
- Expose task queue
- Allow manual override
- Never hide actions

### Code changes

All code modifications must be produced as:

```
git diff
```

User may:

- Review
- Apply
- Reject
- Auto‑approve

No silent mutation.

### Tabs

Minimum tabs:

- The Home page holding:
	- Column to the left for file navigation
	- Central interface with LLM models and routing controls (sub tabs for different conversations?)
	- Column to the right for IDE (with LLM integration)
- OpenMemory Local WebUI viewer
- Automation (n8n Local WebUI viewer)
- Settings (Hold API keys redacted but hover over or reveal mechanic to show)

---

## 9. Security Posture

Rules:

- No open ports by default
- No cloud calls without explicit config
- No silent telemetry
- Secrets isolated
- Tools sandboxed

Local ≠ safe. Security is explicit. It should be surprising to learn that this was "vibe-coded"

---

## 10. Performance Targets

|Metric|Target|
|---|---|
|Token rate|≥ 100 tok/s local|
|Context size|< 8k typical|
|Model swap|< 2s|
|Crash recovery|deterministic|

---

## 11. Evaluation Layer

Optional council stage:

- Multiple cheap critic models
- Validate constraints
- Score solution quality
- Only escalate to large model if disagreement

---

## 12. Project Boundaries

pieBot will not:

- Compete on marketing
- Chase benchmarks
- Emulate consciousness
- Store persona text
- Depend on frontier models

It will:

- Be boring
- Be fast
- Be inspectable
- Be cheap
- Be reliable

---

## 13. Development Rules

- Read docs before integrating anything
- Prefer reuse of mechanical plumbing
- Strip training code
- Externalize state first
- Optimize architecture before model size
- No feature without state diagram

---

## 14. Success Definition

pieBot succeeds if:

- It solves real tasks cheaper than cloud agents
- It recovers from crashes without losing intent
- It scales by architecture, not tokens
- It remains understandable 6 months later

---

End of document.