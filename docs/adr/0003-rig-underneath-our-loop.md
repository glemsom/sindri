# 0003 — rig-core for providers, our own turn loop on top

- **Status:** accepted
- **Date:** 2025-10-29

## Context

rig-core supplies provider wires, message types, streaming and usage/cache
accounting for many providers. It also supplies its own agent loop
(`AgentRunner`). rig-core 0.45 was a breaking rewrite; the project moves weekly.

Sindri's guarantees — turn cap semantics, cancel to exit 130, partial answer on
cap, HTTP timeouts, the exact JSON shape — all live in loop territory.

## Decision

- Dependency is pinned to an **exact version** (`rig-core = "=0.45.x"`), never a
  caret range.
- Every rig type is confined to one adapter module. A rig rewrite must touch that
  module and nothing else.
- We drive the loop ourselves via rig's completion API (`Model::call` /
  `Model::stream`), not `AgentRunner`.
- We implement the `bash` tool ourselves rather than adopting `rig-tools::Shell`,
  because the tool's description must derive from `SandboxPolicy` (ADR-0002).
- We build our own HTTP client with explicit timeouts; rig ships none.

## Consequences

- Upgrading rig is a scheduled task in one file, not a refactor.
- We give up rig's built-in retries and turn bookkeeping; our tests own them.
