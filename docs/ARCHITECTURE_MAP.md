# ARCHITECTURE_MAP

### 1) System diagram (textual)

**Two planes:**

* **Control Plane (deterministic, must be safe):**

  * Orchestrator loop
  * Policy + permissions + approvals
  * Tool registry + choke point execution
  * Audit log
  * State persistence + replay

* **Worker Plane (probabilistic, replaceable):**

  * LLM providers / model calls
  * STT/TTS
  * embeddings / indexing
  * summarizers / critics

**Rule:** workers never mutate system state directly. They propose. Control plane decides.

---

### 2) Canonical module layout in pieBot (with boundaries)

```
apps/
  desktop/                # UI only: review + approve + display
  server/                 # orchestration runtime (deterministic)

packages/
  core/                   # contracts + shared types
  policy/                 # risk rules, approvals, redaction
  tools/                  # tool registry + adapters
  memory/                 # OpenMemory + working memory adapters
  models/                 # provider abstractions + router
  eval/                   # optional council (later)

vendor/
  gsama/                  # git submodule pinned tag
  openmemory/ (optional)  # if utilised later
runtime/                  # generated artifacts (not committed)
configs/                  # router + policy + tool configs
```

---

### 3) External data + code sources (explicit provenance map)

#### A) Code imported as vendor deps (in-repo but external origin)

| Component             | Location in pieBot   | Source repo           | Ownership | Update policy    |
| --------------------- | -------------------- | --------------------- | --------- | ---------------- |
| GSAMA                 | `vendor/gsama/`      | `clyptix-media/gsama` | external  | pinned tags only |
| (Optional) OpenMemory | `vendor/openmemory/` | (TBD)                 | external  | pinned tags only |

---

### 4) Dataflow: where data comes from, where it goes

#### Inputs (“Pillars / Feeds”)

These are *structured producers*. They do not call models.

* `FileSystemFeed` (repo tree, file changes)
* `GitFeed` (status, diffs, commits, test results)
* `WorkflowFeed` (n8n job state)
* `UserFeed` (chat messages, UI events)
* Optional later:

  * `EmailFeed`, `CalendarFeed`, `ChatFeed`, etc.

Each feed emits:

```
ObservationEvent {
  source: "git" | "fs" | "user" | "workflow" | ...
  timestamp
  payload: {...}   # structured
  signature/hash
}
```

#### Memory sinks

* **GSAMA**: receives *StateDelta proposals* from orchestrator only
* **OpenMemory**: receives episodic objects (tool results, summaries, decisions)
* **Working Memory**: receives transient context (current plan, tool args, scratch)

#### Outputs

* UI display events (stream)
* Proposed diffs
* Tool execution results
* Optional: TTS audio

---

### 5) Contracts (the enforcement layer)

In the map doc, reference the contracts file, but list the *core invariants*:

* No tool runs without `PolicyDecision(allow=true)`
* No mutation without `ExecutionArm=true` + explicit approval event
* All mutations are diff-first
* Every state mutation produces an `AuditEvent`
* GSAMA only accessed via adapter (single entrypoint)
* Default networking: none / localhost only

---

### 6) The “minimal vertical slice” wiring

**Slice v0:**

1. Desktop sends `TaskRequest`
2. Server creates `RunId`, logs `RunStarted`
3. Server pulls `ObservationEvents` from feeds (user input only at first)
4. Orchestrator picks `AgentType=planner`
5. Router calls model provider
6. Model returns `PlanProposal`
7. Policy evaluates plan
8. If code change needed → produce `git diff` artifact
9. Desktop reviews + approves
10. Server applies diff via tool adapter
11. Server logs `RunCompleted`
12. OpenMemory stores episode; GSAMA applies delta

---
