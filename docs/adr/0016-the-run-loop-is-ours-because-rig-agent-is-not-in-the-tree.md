# The run loop is ours, because rig-agent is not in the tree

ADR-0004 made the loop a driver over `rig_agent::AgentRunner`. rig-core 0.46 has no
`AgentRunner` to drive, this build has no `rig-agent` to link, and a loop is not a thing that
can be deferred: §4 is acceptance criteria. So the fallback ADR-0004 itself named is taken —
a driver of our own — and it is built over two seams of our own rather than over rig's.

**What 0.46 actually offers.** `rig_core::driver::Model` pairs a wire with a transport and takes
one request to one streamed reply; `driver::Local` is a pass-through wire for a runtime that
builds its own events. There is no agent module, no run state machine, no
`RunStartAction`/`ModelTurnAction`/`ToolCallAction` hook, and no `AgentRun`. The registry this
build resolves against holds `rig-core`, `rig-http`, `rig-reqwest`, `rig-tungstenite` and
`rig-derive`; nothing named `rig-agent` is there, and with no network a dependency the lockfile
does not hold cannot be added — it would not compile, which is not a seam but a stop.

**What that costs.** Turn state, tool-call batching, streaming correlation, invalid-call
recovery, history projection and mid-batch resume were the argument for rig-agent, and three of
them are now ours. What remains rig's is what A15 and A16 left it: the dialect, the framing, the
decoder, and the wire's own retry verdict — all behind the provider seam, which the loop never
names.

**The two seams.** The loop asks a `Responder` (one `Request` in; one `Reply` or one `Failure`
out) and hands each call the reply asked for to an `Executor` (one `Call` in; `Acted` — the text
the model is handed back, the output bytes it cost, how long it took — or the one line that stops
the run as `error`). Those two types are the whole of the loop's outside world, which is why
`cargo test` drives two hundred and fifty turns, an hour of command wall clock and a signal with
no network and no clock. `Shell` is the `Executor` over `bash::run`, and the session temp is an
input it is handed: this module decides what a command cost, #16 decides where an overflow of it
goes, and the record (#18) writes the result.

**The stop set is a type.** `Stop` is `final`, `max_turns`, `budget`, `cancelled`, `error`, and
`Stop::word` and `Stop::exit_code` match it with no wildcard arm, so a sixth reason cannot be
added without every surface being made to say it and §15's one mapping being edited. `State` is
the same fact before it lands — `Running` until a stop reason does — which is what lets a record
say `running` while the run goes (#18). `Stop::exit_code` is §15's table: `0`, `100`, `101`,
`130`, `1`, and `Run::answer` hands back text only for `final`, so a failed or cancelled run has
nothing to print on stdout.

**Precedence is one function.** `Run::settle` is ADR-0006 in six lines: a set cancel wins over
whatever else fired, and otherwise the first trigger in time keeps the state it took. Two guards
make it unambiguous: the loop starts no turn once a budget is spent, and a turn that produced a
complete answer stops as `final`. The two budgets are run-scoped — the elapsed and the bytes of
every command in the run — because batching hides work inside one script, and a turn-scoped
budget would let a run spend an hour in each of two hundred and fifty turns.

**Cancellation.** `^C` is a flag and nothing else: the handler stores into one atomic and returns,
because anything that allocates in a signal handler is undefined behaviour; `signal(2)` is
declared in the file rather than pulled in as a dependency, since a cancellation is one flag. rig
constructs no cancellation error, so a cancel does not cut the in-flight request short — the loop
observes the flag before every turn and after every call, and the run stops when the request does
(A15's "not yet"). Exit 130 and nothing on stdout follow from `Stop::Cancelled`, not from the
front end.

**Not yet.** Projecting a run's history onto a wire's own messages — tool results with their call
ids, per dialect — is the conversation half the `Request` value exists to feed; today a
`Responder` has the prompt, the history, the tool names and the loaded skills, and nothing in the
provider seam turns that into `CompletionRequest` messages. The skill tool (#32) is not
implemented, so `Shell` refuses a call that names it by name rather than pretending; the front
end (#19) reads `Stop::exit_code` and `Run::answer` and gets no second opinion. A17 records the
gap these facts come from.
