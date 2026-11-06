# rig owns the wire and the loop bookkeeping; we own lifecycle policy

Dependencies: `rig-core` and `rig-agent`. In the loop we drive `AgentRunner`; the model is
reached through `rig-core`'s `DynModel`; retry and headers live in a `Transport`
implementation; budgets, stop reasons, skill dedup and records live in hooks.

**Why not `rig-core` alone.** `rig_agent::run::AgentRun` is a sans-I/O state machine that
owns "no models, tools or hooks — drivers provide those services and lifecycle policy". Turn
state, tool-call batching, invalid-call recovery, history projection, streaming correlation,
mid-batch resume and empty turns are the subtlest code in the spec and they are already
written and conformance-tested. Re-implementing them buys nothing.

**Why not our own driver yet.** `AgentRunner` is a driver *over* `AgentRun`, and every
policy the spec demands has a hook slot: `RunStartAction::Stop`,
`ModelSelectionAction::Stop`, `CompletionCallAction::Stop`, `ModelTurnAction::{Stop, Retry}`,
`ToolCallAction::{Stop, Skip, Patch}`. `ToolCallAction::Skip(reason)` is, literally,
"two identical loads in one turn are one load". If a hook ever cannot express a §4 rule,
the fallback is a driver of our own over the same `AgentRun` — a new file, not a migration,
because history and stop reasons are already the same objects.

**The amendment this forces.** §10 said "exactly one file reads it". With rig-agent in the
picture that is three of our files (transport, hooks, tools). The rule becomes: **exactly
one module tree may name rig** — everything else in core speaks Sindri's own types.
