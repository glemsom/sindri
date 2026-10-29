# 0012 — The frame's two seams: a driver, and a tap

- **Status:** accepted
- **Date:** 2026-10-29

## Context

ADR-0011 decided that `crates/tui` renders the core's `Event` stream and holds
no loop, no retry policy and no sandbox code. Rendering it raised two questions
that decision did not answer, both about what crosses the boundary:

1. **Who runs the prompt?** The frame needs a run started, cancelled and
   reported on, but the agent, the executor, the provider client and the record
   all live behind the core. Core must not learn what a terminal is, and the
   provider's own types must not reach the front end (ADR-0003).
2. **How does a live tail exist at all?** The executor collected stdout and
   stderr to the end and returned them, so "the last five lines while it runs"
   — ADR-0011 §9 — had nothing to read. The exit status is the executor port's
   return value, which only the port's *caller* ever sees.

## Decision

**A `Driver` trait: the frame asks, a front end answers.**

```rust
pub trait Driver: Send {
    fn run(&mut self, prompt: &str, cancel: Cancel, tap: Tap) -> Finished;
}
```

The frame owns the terminal, the keys, the queue and the rail; the front end
(`crates/cli/src/frame.rs`) owns the agent, the executor, the recorder and the
provider. `Finished` carries what the rail and transcript need in the frame's
own words — `Stop`, `Tokens`, `error: Option<String>` — so no `Report`, no
`Usage` and no `rig` type crosses the seam. The `Cancel` that crosses is the
core's own flag, because a cancel that is not the same flag the executor reads
is not a cancel.

**A `Tap`: the run's events and a command's output, on one channel.**

`Tap` is a clonable handle the driver is given: `event(Event)` for the core's
stream, `output(Side, &str)` for a line of a running command, `over(CallOver)`
for how a command ended. All three become messages to the frame's own loop, so
the frame never blocks on the run and the run never blocks on the frame.

**An `OutputSink` port in the executor, and the exit status where it already
was.** `SandboxExecutor::with_output_sink` hands each line to a sink as it
arrives, next to the existing `with_status_sink` for bubblewrap's documents
(ADR-0006); it mirrors `drain_documents`, and the bytes are the same bytes the
model is given. The exit status needs no new port: the front end wraps the port
and reports `over` on the way back, because the wrapper is the only thing that
sees both the call and its result.

**The face is only asked where it can plausibly answer.** `Picker::from_query_stdio`
spends up to a second waiting for a terminal that will not reply, and it reads
stdin while it does. So the frame queries only when the environment names a
Kitty-capable terminal (`KITTY_WINDOW_ID`, or `TERM`/`TERM_PROGRAM`/`LC_TERMINAL`
naming kitty, ghostty, wezterm or foot). Everywhere else the answer is known
before the question: no face, no wait, no eaten keystroke.

## Consequences

- The core gained one port and one method; it still imports no front end, and
  `Event` did not change for the third slice running.
- The live tail is honest: it shows exactly the bytes the model will read, from
  the same reader, rather than a second process or a tee.
- A front end that implements `Driver` can be swapped — a second front end
  costs one trait impl, and the frame's tests need no model at all.
- Ordering between `Event` and `output` is not guaranteed: the driver forwards
  events through a thread, so the frame buffers output that arrives before the
  call that produced it and attaches it when the call is announced.
- One conversation per launch is not yet one thread of messages: the core's
  `Agent::run` builds its history per call, so a later prompt does not see an
  earlier prompt's messages. Growing the thread is the core's next change, and
  the frame's transcript already grows, so only the model-facing history moves.
