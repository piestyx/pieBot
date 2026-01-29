# PROVENANCE

## Execution Contract

- No tool execution without policy check
- No mutation without explicit arm + audit entry
- No “free text tool args”
- No code copied without:
    - source repo URL
    - commit hash/tag
    - license
    - what changed + why
    - tests that pin behaviour

## Memory Contract

- GSAMA only via adapter
- GSAMA stores vectors + tags only (no raw chat)
- Working memory TTL + max size
- OpenMemory is episodic store, not identity

## Networking Contract

- No open ports by default
- Bind localhost only unless explicitly enabled
- Remote exposure requires auth + mTLS or explicit tunnel

## Diff Contract

- Code edits are `git diff` proposals, never silent writes
