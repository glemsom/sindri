# 0001 — Batch mode first, TUI later, one core behind both

- **Status:** accepted
- **Date:** 2025-10-29

## Context

Sindri has two intended front ends: a full-screen TUI for humans and a Unix
filter (`git diff | sindri -b "summarise"`) for scripts. The TUI is the
attractive part; the filter is the part that defines the product's contract.

## Decision

Build batch mode to completion before any TUI code. The agent core knows
nothing about a terminal: it emits events on a channel and returns a result.
`batch` and (later) `tui` are thin front ends over that core.

## Consequences

- The stdout/stderr/JSON/exit-code contract is fixed early, when it is cheap.
- The TUI becomes a rendering exercise, not a rewrite.
- No front end may be imported by the core; a CI check would catch it.
