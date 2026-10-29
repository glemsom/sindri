# 0008 — Rules that keep this codebase maintainable by an AI agent

- **Status:** accepted
- **Date:** 2025-10-29

## Decision

1. Source files ≤ 400 lines, enforced by a CI script with an explicit allowlist.
2. One module = one job, stated in its module doc comment.
3. `clippy -D warnings` with a curated lint set — not blanket `pedantic`, which
   flags trivial functions and produces noise that derails automated edits.
4. `cargo nextest` for tests, `assert_cmd` for the CLI contract as a process,
   exact-literal snapshots for the JSON shape (a whole expected object written
   out in the test, rather than a new dependency), a mock provider server for
   the loop.
5. `make check` (wrapping devbox) is the single definition of done: fmt, clippy,
   400-line audit, tests.
6. Virtual workspace, never a crate named `core` (it shadows the sysroot).
7. Provider types confined to one adapter module (ADR-0003).

## Consequences

- An agent can localise a change without reading the world.
- Build diagnostics stay legible, which matters more for a model than a human:
  an unreadable error is an unfixable error.
