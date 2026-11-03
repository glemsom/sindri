# The built-in skill, ablated

`docs/system-prompt.md` measured the preamble: a tax paid on every turn, and a
switch that decides how the model works. A built-in skill is a different tax with
the same shape — one line in the catalog, re-sent in the `skill` tool's
description on every turn of every run, plus a body paid once if it is loaded at
all — and the question it raises is the one ADR 0004 §3 names and does not
answer: *is this worth its catalog line?*

The built-in this bench ablates is `sandbox-refusal`: what a run does when the
environment refuses a command — a write denied, a fetch with no credential. So:
one task shaped like the capability it claims — **recover from a refused
write** — run twice on the same model, the same repetitions and the same fixture
repository, with the built-in in the catalog and with the table that ships it
emptied. Every run is scored by a rule, and every line carries what the
run cost and whether the body was ever loaded. The machine is in
`scripts/bench-skill-ablation`; **the numbers below are not yet measured**, and
this document says which parts are proven and which are not.

## What is counted

*Input tokens, summed over the turns of a run.* Not the prompt, and not the
catalog's bytes on their own: the line is re-sent in the tool description while
the model is choosing a tool, and a body that is loaded is carried in the
conversation for the rest of the run, so it is paid again on every turn after it
arrives. The sum is what the work actually cost, and the difference between the
two arms is `line × turns` plus `body × turns after the load`. The counter is the
provider's own accounting, accumulated over the turns by `crates/core` and read
from the run's JSON output — never counted by the bench.

*Whether the built-in was activated — a field, not an inference.* A `skill` call
whose result opened with `loaded` put the body in the conversation. The record
already carries that call as a first-class event — ADR 0002 refuses to infer a
load from the shape of a command line — so `activated` is read from the
transcript and never guessed from a `cat` that may have meant something else.
This field is what tells an **unused** built-in — a line every run pays for and
no run reaches for — from a **useless** one: a body that was loaded and did not
change the answer. Without it the two are the same number.

*Right or wrong per run, by a rule, not by eye.* The task's capability is a
change that is proven, and the rule says exactly what that means:

    right  ⇔  fixed ∧ proved
    fixed  ⇔  `sh tests/run.sh`, run by the bench in the tree the run left, exits 0
    proved ⇔  the last `bash` call in the record that invokes the suite exited 0

`fixed` is the bench's own re-run, never the run's claim, so a run that says "the
tests pass" over a tree that does not pass is wrong however confident its answer
reads. `proved` is the half that separates a run that showed its work from one
that asserted it: a command that only mentions `tests/run.sh` — in an `echo`, a
comment, a `grep` — is a mention, and mentions prove nothing. The suite counts as
invoked when a shell runs it (`sh tests/run.sh`, `bash -e tests/run.sh`,
`./tests/run.sh`), which is the narrowest reading of "ran it" that does not need
to parse a shell. `turns`, `tool_calls`, `suite_calls` and the label the run
stopped on are recorded beside the score, because a difference in tokens is
unreadable without knowing whether the run stopped early.

## The two arms are two builds

`build_arm.sh without` empties the `BUILTINS` table in
`crates/core/src/skills/builtin.rs`, builds, and restores the file byte for
byte. It is the shape `scripts/bench-system-prompt/build_variant.sh` already
uses for the prompt, for the same reason: no product code carries a switch that
exists only for a bench, and a run cannot forget to set the arm. With the table
empty the catalog holds no skill at all — the fixture repository ships none, and
the run's `HOME` is an empty directory — so the `skill` tool is not registered
and the run pays neither the line nor a body; the `bash` description loses the
sentence that points at the tool, because `crates/core/src/agent/tools.rs`
writes it only when there is a catalog to point at. Verified by the bench's own
self-test: `bin-with doctor` lists one built-in, `bin-without doctor` lists none.

The arm is the binary's catalog and not the word on the command line: `run_one.sh`
asks the binary it is about to run, and refuses a `with` label on a binary that
ships nothing. A run whose arm was taken on trust is a line in the wrong row.

## The task

A five-file POSIX-shell repository whose suite fails because `src/lib.sh` names
the greeting log's directory as `/var/lib/greet`, which the read-only host root
refuses — and one prompt:

> `sh tests/run.sh` fails in this repository. Fix the source so the suite passes,
> and prove the fix: run the suite once it is green, and let the answer say what
> the suite printed and where the greeting log ends up.

The built-in's body is what to do about a refusal — read the message rather than
the exit code, move the artefact into a writable root, make the path a parameter
with a writable default, prove it with the same command that failed — and this is
the capability it is supposed to buy. The three defaults its body forbids are the
`without` arm's recorded conversation: escalate, silence the writer, delete the
assertion.

The task is deliberately winnable without the skill — a competent run finds
`/var/lib/greet` and moves it — because a bench whose task is unwinnable measures
the task and not the skill. What the skill is supposed to change is how quickly
the run stops escalating and how often it leaves a path behind that outlives the
run, and that is what the two arms are compared on.

## The results

Not yet measured. The environment this bench was built in has no model
credential — `~/.config/sindri/config.toml` holds none, and there is network
reachability and nothing to authenticate with — so **no live run has been made
and no number here is estimated, extrapolated or filled in**. Every cell of the
table is a cell with no measurement behind it.

| arm | runs | billed | mean input tokens | median input tokens | activated | right |
| --- | --- | --- | --- | --- | --- | --- |
| with | not yet measured | not yet measured | not yet measured | not yet measured | not yet measured | not yet measured |
| without | not yet measured | not yet measured | not yet measured | not yet measured | not yet measured | not yet measured |

Every number the shipping rule below compares is one of those twelve cells, and
none of them has been measured: `sandbox-refusal` is shipped on the argument ADR
0004 §2 makes for the split — procedure in the body, the enforced boundary in the
`bash` description — and **not** on a measurement, which this document refuses to
invent in its place. The command that produces the numbers is under
*Reproducibility*, and the file it lands in is named in
`scripts/bench-skill-ablation/results/README.md`. Until a sweep has written that
file, the criterion "the bench shows it earning its line" is **unmet**, and this
is the paragraph that says so.

`billed` is the count the mean and the median are over, and it is a column of its
own because it is not `runs`: a run that never reached the model carries no token
count, is left out of the arithmetic, and would otherwise make an arm of six runs
look like an arm of two. When the two counts differ, `bench.py report` prints the
difference under the table, and when the two arms were measured over different
numbers of runs it says that too — the shipping rule below compares two means,
and two means over different repetition counts are not a comparison.

The table is the shape `python3 bench.py report` prints, and the columns are the
fields the method turns on: how many runs, how many of them a provider billed,
the price of the run, the price of the run in the middle, whether the body was
ever loaded, and how often the rule said right. One command fills it (see
*Reproducibility*), and it fills one file per model with both arms in it, because
the table is a comparison and a file holding one arm cannot make one. Two models
or two tasks in one file are refused rather than averaged, for the reason
`median.jq` gives beside the prompt bench: that is a mistake in the file name,
not a spread.

## What is proven here, and what is not

**Proven, offline, without a key.** `scripts/bench-skill-ablation/self-test.sh`
runs the whole pipeline and asserts it: the unit tests of the record line, the
rule, the activation field and the table; the two arm builds and the fact that
the second really ships no built-in; one sweep through both arms against
recorded conversations served on loopback; and the report those lines make. The
recorded `with` conversation loads the skill, runs the suite, reads the
`Read-only file system` refusal, makes the log's path a parameter with a
writable default, runs the suite again and answers; the `without` one reaches
for `sudo`, silences the writer, deletes the log assertion and claims the suite
is green without running it. The rule scores them `{fixed, proved, ok} =
{true, true, true}` and `{true, false, false}` — which is the rule discriminating
on a difference a reader can see, and it is the only claim the offline pass
makes.

**Not proven, and not claimed.** Any model's behaviour. The recorded bodies are
written to the wire format rather than captured from a model (the same honesty
`crates/evals/fixtures` keeps, for the same reason), and the token counts on
those lines are the script's own numbers. An offline pass is evidence that the
machine parses, scores, records and reports; it is not evidence about a model,
and it must never be quoted as one.

## The reading

None of the four numbers is interesting alone; what they are for is to place
each built-in in one of three states, and the states do not need a number to be
described:

- **Unused.** `activated` near zero in the `with` arm. The line is a tax every
  run pays and no run is reached by, and the thing to fix is the pointer, not the
  body: the catalog entry is the routing key the model reads while it is choosing
  a tool, and a body nobody loads cannot be useless or useful. This is ADR
  0004 §3's untested risk, made a number.
- **Useless.** Activated, and `right` in the `with` arm is where `right` in the
  `without` arm is. The body changed the conversation and not the outcome, and
  the run paid for it twice — the line on every turn, the body on every turn
  after the load.
- **Useful.** Activated, and `right` in the `with` arm is higher than in the
  `without` arm by more than the spread of the repetitions. Only then does the
  price question have an answer, and it is asked against the arm without the
  skill, which is the run as it would be if the line were not shipped.

Two limits are already known from the neighbouring measurement and are written
down before any number arrives. The preamble bench put the spread of one prompt's
bill at roughly ±20% over six repetitions, so a difference smaller than that here
is the model's noise and not the skill's effect. And a difference in `right` of
one run in six is likewise inside the spread: with `REPS=6` an arm can be right
6/6 and the other 5/6 while the two are indistinguishable, which is why the rule
below asks for a gap the repetitions can carry rather than any gap at all.

## The shipping rule

A built-in earns its catalog line when, over six repetitions of a task it claims
to cover, on a model worth shipping to:

1. **it is reached** — `activated` in at least five of the six runs. A body that
   is loaded in a third of runs is a description problem first; shipping the
   body again will not fix it.
2. **it changes the outcome** — `right` in the `with` arm is 6/6 and `right` in
   the `without` arm is at most 4/6. One run in six is the spread of six
   repetitions; two is the smallest gap that is not noise.
3. **it pays for itself** — the mean summed input tokens of the `with` arm is no
   more than 1.25× the mean of the `without` arm. The catalog line is a per-turn
   tax and the body is a per-turn-after-load tax, and the neighbour's ±20% is the
   band inside which a difference is not visible: a built-in that buys nothing
   measurable and costs more than a quarter again is dropped, and one that buys
   correctness is kept at almost any price under that ratio, because a wrong run
   costs a repetition of the whole task.

A built-in that fails (1) is not shipped and its description is rewritten; one
that fails (2) or (3) is not worth a catalog line and is either folded into a
skill a task triggers or dropped. The thresholds are stated before the sweep on
purpose: a rule chosen after the numbers is not a rule. If a sweep comes back
with a result that the rule cannot express — a gap of exactly one run, a cost
that is lower with the skill because the body spends fewer turns — the rule is
the thing to revise, and the revision is where a reader should look for the
argument.

## What this does not prove

One task, one fixture repository of five files, and one half of the capability
the built-in claims. The task is a denied write; the body's second half — a fetch
with no credential — has no task here, because a task needing a credential to be
*absent* is a task whose fixture would have to fail differently on every machine
that runs it, and a bench that cannot be re-run is not a measurement. A built-in
that helps with the refusal it was measured on may still be useless on the one it
was not.

The rule reads an exit code, so it cannot tell a suite that was fixed from a suite
that was weakened: the `without` arm deletes an assertion and the bench's own
re-run passes it. That is `fixed` meaning what it says — the tree the run left
does pass its own suite — and `proved`, which the run loses, is the only half that
notices. A body that spends its line on the difference between those two is
reading the rule correctly, and a rule that could not be fooled this way would
have to read the diff, which is a different bench.

A bench that measures whether a body makes a refused command be recovered from,
not a benchmark of coding ability, and a built-in that helps here may still be
useless on the tasks it claims to cover — six repetitions on one model is a
narrow window. The token sums are the provider's accounting, with the ±20% spread
that implies. And an ablation measures the built-in against
its absence, never against a better one: nothing here says the description could
not route better, which is why a failure of (1) sends a reader to the catalog
entry rather than to the body.

## Reproducibility

The offline proof, no key and no bill:

```sh
cd scripts/bench-skill-ablation
./self-test.sh
```

The live table, with your own credential and at your own cost — the arm builds
are two `cargo build`s, and `REPS=6` is six complete passes of the task on each
arm:

```sh
cd scripts/bench-skill-ablation
export SINDRI_PROVIDER=openai SINDRI_API_KEY=… SINDRI_BASE_URL=https://…
export BENCH_MODEL=<the model the two arms are compared on>

./build_arm.sh with
./build_arm.sh without
REPS=6 TAG=$BENCH_MODEL ./sweep.sh results/$BENCH_MODEL.jsonl with without
python3 bench.py report results/$BENCH_MODEL.jsonl
```

`results/<model>.jsonl` is the sweep's own record — one line per run, both arms —
and it is committed, because a number nobody can recompute is a claim. The lines
behind the table above do not exist yet; `scripts/bench-skill-ablation/results/README.md`
says so and names the file each sweep lands in.
