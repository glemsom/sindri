# The conversation lives in the core, and the frame that owns a launch holds one

A launch in the TUI holds one conversation: two prompts typed into it reach the wire as two requests,
the second asked after the first prompt and its answer, `/new` empties what is carried, and the
launch that ends takes it away. **It lives in the core**: the type is
`sindri_core::conversation::Conversation`, beside `Run`, and only an instance of it belongs to a
front end — `View` keeps one, hands it to the launch a prompt starts, and takes the exchange each run
stops with. `/new` empties it and marks the run it superseded, so that run reaches its own stop reason
and its words still never reach the next request.

**Where it lives is decided by the glossary, not by convenience.** A conversation is a decision —
which words the wire is asked with is as much a decision as which wire to ask — and the core is the
part of Sindri that makes every decision. A front end that assembled the turns a prompt is asked
with would be a second place that decides what a request holds, and the batch front end would be a
third, bound to drift from the terminal one the first time either changed. So the vocabulary sits in
the core: `Conversation` keeps the exchanges in the order they ended, `Run::kept` says what one run
contributes to it, and `Run::earlier` — the seam the wire was already asked through — is what the
next run joins with. The front end owns only the lifetime of an instance, which is the one thing it
already owns: a launch is the terminal's, not the core's.

**It is held by the frame because the frame is what survives a run.** A `Launch` is opened per prompt
and a `Run` ends with it; the `View` is the one thing in the TUI that outlives both, which is why the
loaded-skill set lives there and why `/new` can empty the conversation at all — the command is
answered inside the frame, and a value the frame did not hold would need a second seam from the
keyboard back into whatever did. The exchange a run stops with arrives by the seam a turn already
arrives by, so the joining is ordered: the frame takes it before it hands the next queued prompt to a
launch, which is what makes a queued prompt join at the turn it was asked and not one place off.

**One exchange per prompt, and never one per turn.** A run with tool calls has several turns, and
only the last of them is the answer the reader was given; replaying each turn as a prompt and an
answer would repeat the prompt once per call in the same breath as losing the calls themselves. So
the conversation keeps the prompt beside the words of the turn the run ended on — and an empty answer
when no turn ended, which is what a prompt whose run was cancelled on its first request is: the wire
already gives such a prompt the words alone, and it is not a prompt anybody may quietly forget.

## What a reader is held to

- Nothing a front end assembles reaches a request as history: a request is asked with
  `Conversation::exchanges`, and batch mode is asked with none, so it stays one prompt, one request.
- `/new` empties the conversation with the loaded-skill set and the transcript, and never stops the
  run in flight: that run still reaches its own stop reason, and what it ended on belongs to the
  conversation `/new` emptied, so it is never carried into the one that replaced it — the prompt
  typed after it is asked with nothing from before the reader's `/new`.
- A run stopped on anything but a final answer — error, budget, cancelled, max turns — still joins
  with the turns that ended before it stopped, or with its prompt alone when none did.
- The conversation is emptied by the launch that ends, so no state directory and no record learns a
  word about it: it is a fact of one terminal, held in memory.
