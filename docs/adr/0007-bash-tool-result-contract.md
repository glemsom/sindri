---
status: accepted
date: 2026-10-29
---

# 0007 — What the `bash` tool returns to the model

## Decision

One structured result, rendered as text (`crates/core/src/exec/result.rs`):

```text
outcome: exited{exit_code} | timed_out | sandbox_error{detail}
stdout, stderr        head+tail truncated (8192 + 8192 bytes)
spill                 full text, in the session temp; the path is given to the model
duration_ms, truncated
```

- `exit_code` exists only under `exited`; a signal reports 128 + its number, as a shell would.
- Classification uses `--json-status-fd`: the `exit-code` document appears only if mounts succeeded, so its absence is `sandbox_error`, never a command failure.
- Timeouts are our own signal, never inferred from 137.
- Probe: `rm … | head -1` reports 141 (SIGPIPE) — a command status, not a sandbox fault.
- The system prompt states that bash reports the last command's status and offers `set -e` / `pipefail`.

## Consequences

- The model can tell "the sandbox blocked me" from "my command failed": re-plan versus retry.
- Timeout and sandbox faults are eval-able without parsing prose.
