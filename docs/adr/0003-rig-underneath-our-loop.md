---
status: accepted
date: 2026-10-29
---

# 0003 — rig-core for provider wires, our own turn loop on top

## Decision

- `rig-core = "=0.45.0"` exact pin, never a caret range (`crates/core/Cargo.toml`).
- Every rig type is confined to `crates/core/src/provider/rig_wire.rs`; a rig rewrite touches that file and nothing else.
- The loop is ours, driven via `Model::call` / `Model::stream`, never `AgentRunner`.
- The `bash` tool is ours, not `rig-tools::Shell`: its description derives from `SandboxPolicy` (ADR-0002).
- Our own HTTP client with explicit timeouts; rig ships none.

## Consequences

- Turn cap, exit 130, partial answer on cap, timeouts and the JSON shape (ADR-0004) live in loop code we own and test.
- Upgrading rig is one file, not a refactor.
- rig's retries and turn bookkeeping are given up; our tests own them.
