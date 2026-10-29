# 0007 — What the `bash` tool returns to the model

- **Status:** accepted
- **Date:** 2025-10-29

## Context

A bare exit code is ambiguous. In a pipeline, the status may come from a
downstream command (probe: `rm … | head -1` reported **141**, SIGPIPE, not the
sandbox's 1). bwrap itself can fail before the command ever runs.

## Decision

The tool returns a structured outcome, rendered to the model as text:

- **outcome**: `exited` | `timed_out` | `sandbox_error`
- **exit_code**: only when `outcome == exited`
- **stdout / stderr**: head+tail truncated to a cap, full text spilled to a file
  in the session temp, whose path the model is told
- **duration_ms**, **truncated**

Classification uses bwrap's `--json-status-fd`: the `exit-code` document only
appears if mounts succeeded, so its absence means `sandbox_error` — never a
command failure. Timeouts are our own signal, never inferred from 137.

The system prompt tells the model how bash reports status (last command of the
script, `set -e`/`pipefail` available) and to check exit codes explicitly.

## Consequences

- The model can tell "the sandbox blocked me" from "my command failed", which is
  the difference between re-planning and retrying blindly.
- Timeout and sandbox faults are eval-able without parsing prose.
