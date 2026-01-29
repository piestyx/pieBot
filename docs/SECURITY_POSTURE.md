# SECURITY_POSTURE

- Default bind: 127.0.0.1 only
- Explicit config switch to bind externally
- Startup “doctor” that fails if:
    - bound publicly without auth
    - dangerous tools enabled without approvals
    - secrets detected in logs
- Threat model table:
    - exposed port → full account takeover
    - prompt injection → unsafe command execution
    - data exfiltration via cloud model calls
    - plugin supply-chain risk