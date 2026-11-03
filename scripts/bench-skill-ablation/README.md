# A bench for a built-in skill

One task, shaped like a capability a body is supposed to buy, run three ways — with
the built-in table as it ships, with the table emptied, and with the built-in in place
but the doctrine's trigger sentence gone — scored by a rule and reported as one table. The rule and its thresholds are `bench.py`'s, stated once there. It is
read on the first two arms, because it asks whether a body earns its catalog line; the
third arm is asked whether the words that ask for a load are what puts the load before
the first command.

| task | the skill it ablates | scored on |
| --- | --- | --- |
| `inspect` | `search-and-read-files` | **price** — the same answer for fewer input tokens |

One file holds one task, and a report refuses a file that mixes two for the same reason
it refuses one that mixes two models. `bench.py` still knows the other axis — `outcome`,
where a body is scored on the work holding afterwards — but no shipped skill claims that
purchase today: it went with the skill it was measured for, so a task that uses it is one
entry in `tasks.py` and no script to edit.

No number here is on the gate: it would come from a model and a bill. The offline pass
proves the pipeline, not a model, and the guard rails (an arm that is not a name, a task
that is not a name, a sweep of no arms or zero repetitions, a record line for an arm that
does not exist, a report over a file no sweep wrote) are covered by
`scripts/tests/skill-ablation.sh`, which `make scripts-check` runs, together with the
bench's own unit tests.

```sh
export SINDRI_PROVIDER=openai SINDRI_API_KEY=… SINDRI_BASE_URL=https://…
export BENCH_MODEL=<the model the three arms are compared on>

./build_arm.sh with                    # the tree as it stands          → bin-with
./build_arm.sh without                 # the built-in table empty       → bin-without
./build_arm.sh no-trigger              # the doctrine a sentence short  → bin-no-trigger

REPS=6 TAG=$BENCH_MODEL ./sweep.sh results/$BENCH_MODEL-inspect.jsonl with without no-trigger
python3 bench.py report results/$BENCH_MODEL-inspect.jsonl
```

`BENCH_TASK` is the task's name and defaults to `inspect`; `bench.py tasks` prints
the names, and `bench.py arms` the arms.

## The three arms are three builds, not a flag

`build_arm.sh without` empties `BUILTINS` in `crates/core/src/skills/builtin.rs`.
`build_arm.sh no-trigger` writes the shipped doctrine one sentence shorter into
`crates/core/src/agent/skill/doctrine.md` — the line that asks for a load before a task is
touched, `Call it before acting when a task is one it covers, and not otherwise.`, and
nothing else. Both put every file back byte for byte: no run can forget to set them, and no
product code carries a switch that exists only for a bench. With the table empty the catalog
holds no skill at all — neither fixture ships one and `run_one.sh` points `HOME` at an empty
directory — so the `skill` tool is not registered and the run pays for neither the catalog
line nor a body.

The doctrine's removal is `doctrines.py`'s, and it refuses a text that no longer carries
the sentence: an arm that is the shipped arm under another name would report one wording
twice, which is the one number this bench must never make. That is also the question the
arm's identity is settled by — `build_arm.sh` and `run_one.sh` ask the binary whether it
carries the sentence in the text it ships, and a label its binary contradicts is refused
before a run happens.

## One run, one line

`run_one.sh ARM RUNDIR` works in a fresh `git init`ed copy of the task's fixture and prints
one JSON line: the task, arm, tag, repetition and attempt; `in_tokens` (the provider's own
count) beside `out_tokens`, `turns`, `tool_calls`; `activated` and `skill_calls`, which tell
an **unused** built-in from a **useless** one; `loaded_before_bash`, which says whether a
skill's body entered the conversation before the run's first command — the fact the third
arm is scored on, read from the order the transcript was written in; `suite_calls`; and
`checks`, whose shape is the task's axis.

The arm is read from the binary: `run_one.sh` asks `bin-<arm> doctor` and refuses a run
whose label its binary contradicts — the sentence is asked of the text the binary carries,
and the catalog is asked of `doctor`. The record shape is the prompt bench's plus `arm` for
its `variant`, `task` for the question it answered, those three counters, and
`loaded_before_bash`.

    right  ⇔  fixed ∧ proved                       (the `outcome` axis)
    fixed  ⇔  `sh tests/run.sh`, run by the bench in the run's tree, exits 0
    proved ⇔  the last `bash` call in the record that invokes the suite exited 0

`fixed` is the bench's own re-run, never the run's claim; `proved` separates a run that
showed its work from one that asserted it. No task on this bench uses the axis today —
it is what a body buying work-holding-afterwards would be scored on, and it is held here
so the next such body is measured by the same rule rather than a new one.

The `inspect` task is scored on the other axis, and its check is `tasks.py`'s because the
file that asks a question is the file that says what counts as an answer to it: the line
that applies the cap quoted as `path:line`, the delay it produces, and every file in the
tree that mentions `_ceiling`. All three, because the price of a read is only worth
counting when the thing bought is the same answer. `checks` then reads
`{ok, line, delay, missing}`.

## The inspect task is the one a reading skill can win

A body that tells a model to search before it opens cannot make it reach an answer it
would not otherwise reach — the `without` arm gets the same answer, at the price of
`cat`ing four files. So the arms are compared by `in_tokens`, and the rule's outcome check
asks only that every run with the body was right. That is the whole of the second axis: a
reading skill earns its catalog line by spending fewer tokens on one answer, and a body
that costs more than the reads it replaces has bought nothing, whatever it answered.

The price axis therefore reads the rule's third check with a floor of its own. The outcome
axis allows a body a quarter again in tokens to buy an outcome — that is the 1.25x — but a
body whose whole purchase *is* the price cannot spend a quarter again and still have bought
anything, so there the check is `under 1.00x` and a sweep that comes out at 1.20x is a
recorded no.

The two arms differ by the *table*, not by one body: `without` empties all of it, so a
delta is the catalog's rather than `search-and-read`'s own. That is what the ticket asks
for — the table as it ships against no table at all — and it is also the honest reading of
what ships: a body a second one beside it would erase is a body with no claim to the line.

The fixture is `fixture/inspect/`, a small Python queue whose retry delay is capped at a
constant: cheap to find with one `rg -n`, expensive to find by reading the sources whole.
A task's fixture is the repository its question is about, and none is shared.

## The third arm is the sentence, and its number is a rate

`no-trigger` ships the catalog the `with` arm ships and the body it ships, and differs by
one line of the doctrine. The shipping rule's three checks — reached, outcome, price — read
`with` and `without`, because that rule asks whether a body earns its catalog line; the
third arm is scored by the same rule like every other line, but what it is *asked* is
something else: whether the words that ask for a load are what puts the load first. The
report's `loaded before bash` column counts the runs it did, over the runs that ran a
command — a run that never reached the model wrote nothing to read and a run that answered
without running anything has no first command to load before, and both are left out of the
rate rather than counted as runs the sentence failed to reach — and the sentence under
the table says what the pair came to: the sentence earns its place, moves nothing, or is the
words to rewrite. A sweep of two arms holds no third arm and the report says nothing about
a sentence neither of them moved.

The doctrine was repunctuated and rewrapped when it became a file of its own, and no word of
it changed. `Call it before acting when a task is one it covers` was the front half of a
sentence carrying the shape of a result; it is now a sentence of its own, and the prose is
wrapped one sentence to a line, so the third arm is the absence of one line and not a
rewrite of its neighbours.

## Offline: the pipeline, with no key and no bill

`BENCH_OFFLINE=1` points the run at `mock_provider.py`, which serves
`recorded/<task>/<arm>/` on loopback. The token counts on those lines are the script's own
numbers, not a bill.

```sh
./self-test.sh                      # unit tests, three arm builds, one offline sweep
BENCH_SKIP_BUILD=1 ./self-test.sh   # reuse the three arm binaries
```

`self-test.sh` asserts the arithmetic, that the three arms differ in the catalog and in the
sentence, that a contradicted label is refused, that a sweep of the task writes the lines
and the table — that the reading task's arms answer the same question and are told apart by
what it cost, and that a run which never reached the model is tried once more, with a
stubbed `run_one.sh`.

## What the report will not do

`bench.py report` closes with the shipping rule's three checks — reached, outcome, price —
beside the sweep's own figures, and one sentence naming the built-in's state or saying the
rule cannot express this sweep. Nothing is measured there. A sweep whose runs carried no
token count reads `?` rather than a number; a file holding one arm has no comparison to
make and says so instead of scoring half of one; two models or two tasks in one file are
refused, because a comparison is only one when both sides were measured the same way.
`runs` and `billed` are counted apart, and the load's place is counted over the runs that
ran a command, for the same reason. The third arm's column and sentence are read out of
the same file, so the wording is measured on the same task as the body it belongs to.

## If the split loses, the report says so

The rule's three states are the whole of what this bench decides, and they are decided in
a file a reader can check: `unused` means the catalog entry is what to rewrite, `useless`
means the body changed the conversation and not the outcome, and on the price axis `does
not pay for itself` means the body cost more than the reads it replaces. A sweep that comes
out that way is not a sweep to re-run until it flatters the body: the verdict sentence is
committed in the results file beside the numbers — the report is the record, and
`results/README.md` is where a sweep lands whether it flatters the tree or not — and the
ticket that acts on it removes the body or rewrites the description. A split that loses is
a recorded no, and the two branches of the price axis are held by the gate
(`scripts/tests/skill-ablation.sh`) so neither can be quietly dropped.

## Results

`results/<model>-<task>.jsonl` holds one sweep — all three arms, so the table is one file —
and is committed: a number nobody can recompute is a claim. `results/README.md` says which
sweeps are still missing. `bin-with`, `bin-without`, `bin-no-trigger` and `runs/` are build
products and ignored. `recorded/` is committed and says in its own README that its
conversations are written rather than captured.
