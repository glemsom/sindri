---
status: accepted
date: 2026-10-29
---

# 0012 — The frame's two seams: a driver, and a tap

## Decision

The frame asks, a front end answers (`crates/tui/src/driver.rs`):

```rust
pub trait Driver: Send {
    fn run(&mut self, prompt: &str, cancel: Cancel, tap: Tap) -> Finished;
}
```

- The frame owns terminal, keys, queue and rail; `crates/cli/src/frame.rs` owns the agent, executor, recorder and provider.
- `Finished` carries `Stop`, `Tokens`, `error: Option<String>` — no `Report`, no `Usage`, no rig type crosses (ADR-0003).
- The `Cancel` that crosses is the core's own flag; a cancel the executor does not read is not a cancel.

`Tap` is a clonable handle: `event(Event)`, `output(Side, &str)` for one line of a running command, `over(CallOver)` for how it ended. All three are messages to the frame's loop, so neither side blocks on the other.

- `SandboxExecutor::with_output_sink` hands each line to a sink as it arrives, mirroring `with_status_sink` and `drain_documents`; the bytes are the model's bytes.
- The exit status needs no new port: the front end wraps the executor and reports `over` on the way back, being the only thing that sees both the call and its result.
- The terminal is asked what it can draw once per launch, by `crates/tui/src/graphics.rs` alone, and `Face::new` reads that one answer rather than asking its own (ADR-0016 §1, issue #56). It is asked only when `KITTY_WINDOW_ID` is set, or `TERM` / `TERM_PROGRAM` / `LC_TERMINAL` names kitty, ghostty, wezterm or foot: `Picker::from_query_stdio` spends up to a second and reads stdin.

## Consequences

- Core gained one port and one method; `Event` did not change.
- Ordering between `Event` and `output` is not guaranteed: the driver forwards on a thread, so the frame buffers output that arrives early and attaches it when the call is announced.
- A second front end costs one trait impl; the frame's tests need no model.
