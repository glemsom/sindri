# The built-in skill, ablated

`docs/system-prompt.md` measured the preamble: a tax paid on every turn, and a
switch that decides how the model works. A built-in skill is a different tax with
the same shape — one line in the catalog, re-sent in the `skill` tool's
description on every turn of every run, plus a body paid once if it is loaded at
all — and the question it raises is the one ADR 0004 §3 names and does not
answer: *is this worth its catalog line?*

This page is the argument. The machine that measures it — the two arms, the task,
the rule and the commands — is `scripts/bench-skill-ablation/README.md`: one task
shaped like the capability the built-in claims, run twice on the same model, the
same repetitions and the same fixture repository, with `sandbox-refusal` in the
catalog and with the table that ships it emptied. **The numbers are not yet
measured**; what follows is what the measurement is for, and where it has been
proven without one.

## Honesty and size are two questions

A skill no run opened and a skill every run opened to no effect are the same bill
and not the same fact, so the bench keeps three things apart.

**Whether the answer is right** is a rule, not an eye: the tree the run left
passes its own suite — re-run by the bench, never believed from the answer — *and*
the run itself ran that suite, since a command that only mentions the test file in
an `echo`, a comment or a `grep` is a mention, and mentions prove nothing. The
first half is the tree's, and catches a run that claimed work it did not do; the
second is the record's, and catches one that silenced the assertion instead. A
body that spends its line on the difference between those two is reading the rule
correctly.

**What it cost** is the provider's own input tokens summed over the turns of a
run: not the catalog's bytes on their own, because the line is re-sent while the
model is choosing a tool, and a body that is loaded is carried in the conversation
for every turn after it arrives. The gap between the arms is therefore `line ×
turns` plus `body × turns after the load`, which is a skill's price in the
currency its bill is in.

**Whether it was reached** is a field and not an inference: the record already
carries a `skill` call as a first-class event, so a load is read from the
transcript and never guessed from a `cat` that may have meant something else.
Without it an unused skill and a useless one are the same number.

## The three states

None of those numbers is interesting alone; what they are for is to place a
built-in in one of three states, and the states do not need a number to be
described:

- **Unused.** The body is never loaded in the arm that ships it. The line is a tax
  every run pays and no run is reached by, and the thing to fix is the pointer,
  not the body: the catalog entry is the routing key the model reads while it is
  choosing a tool, and a body nobody loads can be neither useless nor useful. This
  is ADR 0004 §3's untested risk, made a number.
- **Useless.** Loaded, and the arm that ships it is right no more often than the
  arm without it. The body changed the conversation and not the outcome, and the
  run paid for it twice — the line on every turn, the body on every turn after the
  load.
- **Useful.** Loaded, and right more often than the arm without it by more than
  the spread of the repetitions. Only then does the price question have an answer,
  and it is asked against the arm without the skill: the run as it would be if the
  line were not shipped.

Two limits are written down before any number arrives. The neighbouring
measurement put the spread of one prompt's bill at roughly ±20% over six
repetitions, so a difference smaller than that is the model's noise and not the
skill's effect. And with six repetitions an arm can be right 6/6 while the other
is 5/6 and the two are indistinguishable, which is why the rule below asks for a
gap the repetitions can carry rather than any gap at all.

## The shipping rule

A built-in earns its catalog line when, over six repetitions of a task it claims
to cover, on a model worth shipping to:

1. **it is reached** — loaded in at least five of the six runs. A body loaded in a
   third of runs is a description problem first; shipping the body again will not
   fix it.
2. **it changes the outcome** — six of six right with it, and at most four of six
   without. One run in six is the spread of six repetitions; two is the smallest
   gap that is not noise.
3. **it pays for itself** — the mean input tokens with it are no more than 1.25×
   the mean without it. The ±20% band is where a difference is not visible: a
   built-in that buys nothing measurable and costs more than a quarter again is
   dropped, and one that buys correctness is kept at almost any price under that
   ratio, because a wrong run costs a repetition of the whole task.

A built-in that fails (1) is not shipped and its description is rewritten; one
that fails (2) or (3) is not worth a catalog line and is either folded into a
skill a task triggers or dropped. The thresholds are stated before the sweep on
purpose: a rule chosen after the numbers is not a rule. If a sweep comes back with
a result the rule cannot express — a gap of exactly one run, a cost that is lower
with the skill because the body spends fewer turns — the rule is the thing to
revise, and the revision is where a reader should look for the argument.

## The results

Not yet measured. The environment this bench was built in has no model
credential — `~/.config/sindri/config.toml` holds none, and there is network
reachability and nothing to authenticate with — so **no live run has been made and
no number here is estimated, extrapolated or filled in**. Every cell of the table
is a cell with no measurement behind it.

| arm | runs | billed | mean input tokens | median input tokens | activated | right |
| --- | --- | --- | --- | --- | --- | --- |
| with | not yet measured | not yet measured | not yet measured | not yet measured | not yet measured | not yet measured |
| without | not yet measured | not yet measured | not yet measured | not yet measured | not yet measured | not yet measured |

`billed` is the count the mean and the median are over, and it is a column of its
own because it is not `runs`: a run that never reached the model carries no token
count, is left out of the arithmetic, and would otherwise make an arm of six runs
look like an arm of two.

Under its table the report applies the rule above: each of the three conditions
with what the sweep measured against what it asks, and then one sentence — the
state the built-in is in, or the note that this sweep is one the rule cannot
express. So the reading is not a second opinion on the same numbers: the checks
are counted from the lines, and the sentence is the rule's, which is also why a
sweep the thresholds do not fit says so instead of being stretched to a verdict.

`sandbox-refusal` is shipped on the argument ADR 0004 §2 makes for the split —
procedure in the body, the enforced boundary in the `bash` description — and
**not** on a measurement, which this page refuses to invent in its place. Until a
sweep has written a file into `scripts/bench-skill-ablation/results/`, the
criterion "the bench shows it earning its line" is **unmet**, and this is the
paragraph that says so. The command that fills it, and the file it lands in, are
the README's.

## The task, and what it can and cannot show

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

## What is proven, and what is not

**Proven, offline and without a key:** that the machine parses, scores, records
and reports. The recorded `with` conversation loads the skill, runs the suite,
reads the `Read-only file system` refusal, makes the log's path a parameter with a
writable default, runs the suite again and answers; the `without` one reaches for
`sudo`, silences the writer, deletes the log assertion and claims the suite is
green without running it. The rule scores them `{fixed, proved, ok} = {true,
true, true}` and `{true, false, false}` — which is the rule discriminating on a
difference a reader can see, and the only claim the offline pass makes.

**Not proven, and not claimed:** any model's behaviour. The recorded bodies are
written to the wire format rather than captured from a model (the same honesty
`crates/evals/fixtures` keeps, for the same reason), and the token counts on those
lines are the script's own numbers. An offline pass is evidence that the machine
works; it is not evidence about a model, and it must never be quoted as one.

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
re-run passes it. That is `fixed` meaning what it says — the tree the run left does
pass its own suite — and `proved`, which the run loses, is the only half that
notices.

And an ablation measures the built-in against its absence, never against a better
one: nothing here says the description could not route better, which is why a
failure of (1) sends a reader to the catalog entry rather than to the body.
