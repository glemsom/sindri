# A bench for a built-in skill

One task shaped like a capability, run three ways — with the built-in in the catalog, with
the table that ships it emptied, and with the built-in in place but the doctrine's trigger
sentence gone — scored by a rule and reported as one table. The rule and its thresholds are
`bench.py`'s, stated once there. It is read on the first two arms, because it asks whether a
body earns its catalog line; the third arm is asked whether the words that ask for a load
are what puts the load before the first command.

No number here is on the gate: it would come from a model and a bill. The offline pass
proves the pipeline, not a model, and the guard rails (an arm that is not a name, a sweep
of no arms or zero repetitions, a record line for an arm that does not exist, a report over
a file no sweep wrote) are covered by `scripts/tests/skill-ablation.sh`, which
`make scripts-check` runs.

```sh
export SINDRI_PROVIDER=openai SINDRI_API_KEY=… SINDRI_BASE_URL=https://…
export BENCH_MODEL=<the model the three arms are compared on>

./build_arm.sh with                    # the tree as it stands          → bin-with
./build_arm.sh without                 # the built-in table empty       → bin-without
./build_arm.sh no-trigger              # the doctrine a sentence short  → bin-no-trigger
REPS=6 TAG=$BENCH_MODEL ./sweep.sh results/$BENCH_MODEL.jsonl with without no-trigger
python3 bench.py report results/$BENCH_MODEL.jsonl
```

## The three arms are three builds, not a flag

`build_arm.sh without` empties `BUILTINS` in `crates/core/src/skills/builtin.rs`.
`build_arm.sh no-trigger` writes the shipped doctrine one sentence shorter into
`crates/core/src/agent/skill/doctrine.md` — the line that asks for a load before a task is
touched, `Call it before acting when a task is one it covers, and not otherwise.`, and
nothing else. Both put every file back byte for byte: no run can forget to set them, and no
product code carries a switch that exists only for a bench. With the table empty the catalog
holds no skill at all — the fixture ships none and `run_one.sh` points `HOME` at an empty
directory — so the `skill` tool is not registered and the run pays for neither the catalog
line nor a body.

The doctrine's removal is `doctrines.py`'s, and it refuses a text that no longer carries
the sentence: an arm that is the shipped arm under another name would report one wording
twice, which is the one number this bench must never make. That is also the question the
arm's identity is settled by — `build_arm.sh` and `run_one.sh` ask the binary whether it
carries the sentence in the text it ships, and a label its binary contradicts is refused
before a run happens.

## One run, one line

`run_one.sh ARM RUNDIR` works in a fresh `git init`ed copy of `fixture/` and prints one
JSON line: the task, arm, tag, repetition and attempt; `in_tokens` (the provider's own
count) beside `out_tokens`, `turns`, `tool_calls`; `activated` and `skill_calls`, which
tell an **unused** built-in from a **useless** one; `loaded_before_bash`, which says
whether a skill's body entered the conversation before the run's first command — the fact
the third arm is scored on, read from the order the transcript was written in; `suite_calls`;
and `checks.{ok,fixed,proved}`.

The arm is read from the binary: `run_one.sh` asks `bin-<arm> doctor` and refuses a run
whose label its binary contradicts — the sentence is asked of the text the binary carries,
and the catalog is asked of `doctor`. The record shape is the prompt bench's plus `arm` for
its `variant`, those three counters, and `loaded_before_bash`.

    right  ⇔  fixed ∧ proved
    fixed  ⇔  `sh tests/run.sh`, run by the bench in the run's tree, exits 0
    proved ⇔  the last `bash` call in the record that invokes the suite exited 0

`fixed` is the bench's own re-run, never the run's claim; `proved` separates a run that
showed its work from one that asserted it. The `without` arm's conversation silences the
writer, deletes the log assertion and calls the suite green without running it, scoring
`{fixed: true, proved: false}`. The `with` arm hits the same `Read-only file system`
refusal, moves the path into a writable root and runs the suite again.

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
words to rewrite. A sweep of two arms holds no third arm and the report says nothing about a
sentence neither of them moved.

The doctrine was repunctuated and rewrapped when it became a file of its own, and no word of
it changed. `Call it before acting when a task is one it covers` was the front half of a
sentence carrying the shape of a result; it is now a sentence of its own, and the prose is
wrapped one sentence to a line, so the third arm is the absence of one line and not a
rewrite of its neighbours. The model reads the same words in a different shape: the trigger
sentence now sits after the result it describes, and the line breaks are the file's rather
than the string literal's.

## Offline: the pipeline, with no key and no bill

`BENCH_OFFLINE=1` points the run at `mock_provider.py`, which serves `recorded/<arm>/` on
loopback. The token counts on those lines are the script's own numbers, not a bill.

```sh
./self-test.sh                      # unit tests, three arm builds, one offline sweep
BENCH_SKIP_BUILD=1 ./self-test.sh   # reuse the three arm binaries
```

`self-test.sh` asserts the arithmetic, that the three arms differ in the catalog and in the
sentence, that a contradicted label is refused, that one sweep writes the lines and the
table — and that a run which never reached the model is tried once more, with a stubbed
`run_one.sh`.

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

## Results

`results/<model>.jsonl` holds one sweep — all three arms, so the table is one file — and is
committed: a number nobody can recompute is a claim. `results/README.md` says which sweeps
are still missing. `bin-with`, `bin-without`, `bin-no-trigger` and `runs/` are build products and ignored.
`recorded/` is committed and says in its own README that its conversations are written
rather than captured.
