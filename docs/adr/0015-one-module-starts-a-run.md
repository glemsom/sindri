---
status: accepted
date: 2026-10-30
amended: 2026-10-30
---

# 0015 — `session::Session` is the only place a run is started

## Decision

1. `Session::open(env, config) -> Result<Session, StartError>` (`crates/core/src/session.rs`) creates the session dir and `home`, starts the record and derives the policy. Four front ends had each written that order by hand; a *session* is one run, GUID-named and recorded under `sessions/<guid>`.
2. One constructor, and `/new` is `open` again, adopted only on `Ok`. The replaced session is *superseded*: it gets no end written — it was left, not stopped.
3. Per-run parts are built, not owned: `executor(timeout, cancel)` and `agent(dials, cancel)`. `finish(outcome) -> io::Result<()>` writes the end and reports the write failure to its caller.
4. `rebase(config)` re-derives the policy against the same session, so a `/settings` save is in force for the next command (ADR-0014).
5. One error: `StartError` names the sandbox, session dir or record and carries the path. Core prints nothing and maps no exit code.
6. `Env { workspace, search_path, state }`, with `Env::ambient()` the only read of the process, so a test opens a session in a temporary workspace.
7. `sandbox exec` is a caller; `doctor` shares the executor but builds its own policy and probe and writes no record.
8. One `Cancel` per process, installed once by the front end (`signal::install`), cloned into every run, cleared between prompts. Signal handling never enters `Session`.
9. `id()`, `temp()` and `policy().describe()` are the session's; the branch and the context window belong to the rail's `Launch`.

## Consequences

- The ordering bug is unreachable: nothing can forget a thread before a session exists (ADR-0012 untouched — the frame still asks through `Driver`).
- A front end that cannot write a record fails where it stands, for `start` and `finish` alike.
- The old `session::prepare`, `rebound` and `begin` are gone; nothing outside the module calls them.
- A `Session` straddles two lifetimes, one per process and many runs: a front end holds it for the whole launch.

## Amendment

10. **The words are the front end's.** Added after 1–9 shipped; it amends none of them. Boot (`crates/cli/src/boot.rs`: `open`, `prepare`, `configure`, `renew`) turns what it could not do into a `Failure`, and `crates/cli/src/failure.rs` owns the one line and the one exit code (ADR-0004); every front end calls `report` and adds nothing.
    - A check the diagnostic itself fails is its verdict: `sindri: doctor: <why>`.
    - The repository-config refusal is boot's one note, said the same way by every config-resolving caller; the frame's save is not a caller, because stderr under the frame is corruption.
    - `crates/cli/tests/failure.rs`: one broken environment, three front ends, one line and one code.
