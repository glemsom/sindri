---
status: accepted
date: 2026-10-29
---

# 0001 — Batch mode first, one core behind every front end

## Decision

- Batch mode is finished before any TUI code; its contract is fixed first.
- The core knows no terminal: it emits `Event` on a channel and returns a `Report`.
- `crates/cli/src/batch.rs`, `crates/cli/src/frame.rs` and `crates/tui` are front ends; `crates/core` imports none of them.
- A front end costs one `Driver` impl (ADR-0012) and no change in the core.

## Consequences

- The stdout/stderr/JSON/exit-code contract (ADR-0004) is fixed while it is cheap.
- The TUI is a rendering exercise, not a rewrite.
- No CI check enforces the import rule; review does.
