---
status: accepted
date: 2026-10-29
---

# 0008 — Rules that keep this codebase maintainable by an AI agent

## Decision

1. Source files ≤ 400 lines, enforced by `scripts/audit-line-length.sh` with `scripts/line-length-allowlist.txt` (currently empty).
2. One module = one job, stated in its module doc comment.
3. `clippy -D warnings` with curated lints in `clippy.toml`, never blanket `pedantic`.
4. `cargo nextest`; `assert_cmd` for the CLI as a process; exact-literal JSON snapshots; a mock provider server for the loop.
5. `make check` is the definition of done: `fmt clippy audit test relink eval scripts-check` (Makefile).
6. Virtual workspace; no crate named `core` (it shadows the sysroot).
7. Provider types confined to one adapter module (ADR-0003).

## Consequences

- An agent localises a change without reading the world.
- Build diagnostics stay legible: an unreadable error is an unfixable error.
