# The run loop is ours, because rig-agent is not in the tree

The loop is not a thing that can be deferred: §4 is acceptance criteria. rig-core 0.46 ships no
agent module — it is the release that dissolved the run vocabulary into `rig-core` — and no
`rig-agent` crate is in this build's registry at all: `cargo metadata --offline` resolves
`rig-core`, `rig-http`, `rig-reqwest`, `rig-tungstenite` and `rig-derive` and nothing else of the
family, and a build with no network cannot fetch a name its lockfile does not hold. What 0.46
offers is one `Model` that streams one reply, and a `Local` pass-through wire: no turn state, no
tool-call batching, no hook. There is nothing to drive.

So the loop is a driver of our own, built over two seams of our own rather than over rig's, and
turn state, tool-call batching, streaming correlation, invalid-call recovery, history projection
and mid-batch resume are ours. What stays rig's is the dialect, the framing, the decoder and the
wire's own retry verdict — all behind the provider seam, which the loop never names (§10).

**The two seams.** The loop asks a `Responder` (one `Request` in; one `Reply` or one `Failure`
out) and hands each call the reply asked for to an `Executor` (one `Call` in; `Acted` — the text
the model is handed back, the output bytes it cost, how long it took — or the one line that stops
the run as `error`). Those two types are the whole of the loop's outside world, which is why
`cargo test` drives two hundred and fifty turns, an hour of command wall clock and a signal with
no network and no clock. `Shell` is the `Executor` over `bash::run`, and the session temp is an
input it is handed: this module decides what a command cost, the record writes the result.

**The stop set is a type.** `Stop` is `final`, `max_turns`, `budget`, `cancelled`, `error`, and
`Stop::word` and `Stop::exit_code` match it with no wildcard arm, so a sixth reason cannot be
added without every surface being made to say it and §15's one mapping being edited. `State` is
the same fact before it lands — `Running` until a stop reason does — which is what lets a record
say `running` while the run goes. `Stop::exit_code` is §15's table: `0`, `100`, `101`, `130`,
`1`, and `Run::answer` hands back text only for `final`, so a failed or cancelled run has nothing
to print on stdout.

**Precedence is one function.** `Run::settle` is ADR-0006 in six lines: a set cancel wins over
whatever else fired, and otherwise the first trigger in time keeps the state it took. Two guards
make it unambiguous: the loop starts no turn once a budget is spent, and a turn that produced a
complete answer stops as `final`. The two budgets are run-scoped — the elapsed and the bytes of
every command in the run — because batching hides work inside one script, and a turn-scoped
budget would let a run spend an hour in each of two hundred and fifty turns.

**Cancellation.** `^C` is a flag and nothing else: the handler stores into one atomic and
returns, because anything that allocates in a signal handler is undefined behaviour; `signal(2)`
is declared in the file rather than pulled in as a dependency, since a cancellation is one flag.
rig constructs no cancellation error, so a cancel does not cut the in-flight request short — the
loop observes the flag before every turn and after every call, and the run stops when the request
does. Exit 130 and nothing on stdout follow from `Stop::Cancelled`, not from the front end.
