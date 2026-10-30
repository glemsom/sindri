# 0015 — One module starts a run: `session::Session`

- **Status:** accepted
- **Date:** 2026-10-30

## Context

Starting a run is the one thing every front end does, and no module owns it. The
same order — session directory, then record, then policy, then executor, then
tool, then agent, then the cancel flag — is written by hand in four places:
`batch::run` (`crates/cli/src/batch.rs:94-151`), `Front::run`
(`crates/cli/src/frame.rs:113-167`), `Front::fresh_session`
(`crates/cli/src/frame.rs:219-239`), and the policy-and-executor half in
`sandbox::exec` (`crates/cli/src/sandbox.rs:31-44`).

A convention nobody owns drifts, and this one has:

- Batch mode and `sandbox exec` install the SIGINT handler; the frame installs it
  nowhere, so a `kill -INT` outside the frame's keys leaves no `cancelled` end to
  the record.
- `renew` forgets the thread *before* it proves a session exists
  (`frame.rs:197-204`), and `fresh_session` swallows both failures with `.ok()?`
  — a failed `/new` loses the conversation, keeps the old session recording, and
  reports a blanket reason.
- The record's end is written by two implementations with different habits: batch
  prints a write failure, the frame drops it with `let _ =`
  (`frame.rs:176,186`).

The interface is also untestable from where the code lives: `session::prepare`
and `session::rebound` read `current_dir()` and `PATH` inline
(`crates/core/src/session.rs:46,74-85`), and `SessionDir::create` reads the state
root inline (`paths.rs:48`), so no test in `crates/core` can name a workspace and
`crates/core/tests/support/mod.rs` re-implements the setup by hand.

ADR-0001 says front ends are thin over one core, ADR-0006 makes the record
mandatory, and ADR-0004 fixes what a failure exits with. None of the three says
where a run is started.

## Decision

1. **`sindri_core::session::Session` is the only place a run is started.** The
   vocabulary is the glossary's: a *session* is one run, identified by a GUID and
   recorded under `sessions/<guid>`. The value a front end holds and the thing on
   disk are the same concept, so they get the same word.
2. **One constructor.** `Session::open(env, config) -> Result<Session, StartError>`
   creates the session directory and its `HOME`, starts the record, and derives
   the policy. `/new` is `open` again, adopted only on `Ok`, which is what makes
   "prove before you forget" true by construction. A session replaced this way is
   *superseded*: it gets no end written, because it did not stop, it was left.
3. **Per-run parts are built, not owned.** `executor(timeout, cancel)` and
   `agent(dials, cancel, events)` build what one prompt needs, because a policy
   can change between prompts and the frame keeps one agent for the whole thread.
   `finish(outcome) -> io::Result<()>` writes the session's end and reports the
   write failure to its caller.
4. **`rebase(config)`** re-derives the policy against the same session, so a
   `/settings` save is in force for the next command (ADR-0014).
5. **One error.** `StartError` names what failed — the sandbox, the session
   directory, the record — and carries the path where there is one. The core
   prints nothing and maps no exit code; a front end owns those words.
6. **The run environment is a value.** `Env { workspace, search_path, state }`,
   with `Env::ambient()` as the one place the process is read, is taken by
   `open`. Everything below it takes values, so a test opens a session in a
   temporary workspace without touching the process.
7. **`sandbox exec` is a caller; `doctor` is not.** The diagnostic shares the
   session half and the executor, because "a diagnostic cannot probe a different
   sandbox than the run it is diagnosing" (`sandbox.rs:20-22`). `doctor` builds a
   policy and a probe of its own and writes no record; it keeps `policy_for`.
8. **The cancel flag stays the front end's.** One `Cancel` per process, installed
   once, cloned into every run; the front end clears it between prompts, so
   `Cancel` gains a `clear`. Signal handling does not enter `Session`, and the
   `OnceLock` in `crates/cli/src/signal.rs` keeps meaning what it says.
9. **The session's facts are the session's.** `id()`, `temp()` and the posture
   from `policy().describe()` are the module's. The branch and the model's
   context window are the rail's, composed by the front end into
   `sindri_tui::Launch`, because neither is a fact about a session.

## Consequences

- A fifth front end costs one `open` and the answers it needs; thinness is
  enforced by the interface rather than by care.
- The ordering bug is unreachable: there is no way to forget a thread before a
  session exists, because forgetting is the front end's one line after an `Ok`.
- A front end that cannot write a record fails where it stands, in its own
  words, for both `start` and `finish`.
- `session::prepare`, `session::rebound` and `session::begin` stop being the
  interface and become the module's implementation, so the four marks in
  `crates/core` shrink rather than multiply.
- `crates/core/tests/run.rs` can prove what no test could: a record on disk, a
  cancelled end, a policy that names a saved extra path, and two `open`s leaving
  the first record untouched.
- `Session` straddles two lifetimes — one per process, many runs — which is
  deliberate: a front end holds it for the whole launch, and nothing in the
  interface assumes a single run.
- `Cancel` stays one-way apart from the `clear` the front end needs; a future
  front end that wants per-run cancellation without a process flag can add it
  without reopening this decision.
- ADR-0012 is untouched: the frame still asks through `Driver`, and the front
  end answers with an agent the session built.
