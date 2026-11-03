# A bench for a built-in skill

One task shaped like a capability, run twice — with the built-in in the catalog, and with
the table that ships it emptied — scored by a rule and reported as one table. The rule and
its thresholds are `bench.py`'s, stated once there.

No number here is on the gate: it would come from a model and a bill. The offline pass
proves the pipeline, not a model, and the guard rails (an arm that is not a name, a sweep
of no arms or zero repetitions, a record line for an arm that does not exist, a report over
a file no sweep wrote) are covered by `scripts/tests/skill-ablation.sh`, which
`make scripts-check` runs.

```sh
export SINDRI_PROVIDER=openai SINDRI_API_KEY=… SINDRI_BASE_URL=https://…
export BENCH_MODEL=<the model the two arms are compared on>

./build_arm.sh with                    # the tree as it stands   → bin-with
./build_arm.sh without                 # the built-in table empty → bin-without
REPS=6 TAG=$BENCH_MODEL ./sweep.sh results/$BENCH_MODEL.jsonl with without
python3 bench.py report results/$BENCH_MODEL.jsonl
```

## The two arms are two builds, not a flag

`build_arm.sh without` empties `BUILTINS` in `crates/core/src/skills/builtin.rs`, builds,
and puts the file back byte for byte: no run can forget to set it, and no product code
carries a switch that exists only for a bench. With the table empty the catalog holds no
skill at all — the fixture ships none and `run_one.sh` points `HOME` at an empty directory
— so the `skill` tool is not registered and the run pays for neither the catalog line nor
a body.

## One run, one line

`run_one.sh ARM RUNDIR` works in a fresh `git init`ed copy of `fixture/` and prints one
JSON line: the task, arm, tag, repetition and attempt; `in_tokens` (the provider's own
count) beside `out_tokens`, `turns`, `tool_calls`; `activated` and `skill_calls`, which
tell an **unused** built-in from a **useless** one; `suite_calls`; and
`checks.{ok,fixed,proved}`.

The arm is read from the binary: `run_one.sh` asks `bin-<arm> doctor` and refuses a run
whose label its binary contradicts. The record shape is the prompt bench's plus `arm` for
its `variant`, and those three counters.

    right  ⇔  fixed ∧ proved
    fixed  ⇔  `sh tests/run.sh`, run by the bench in the run's tree, exits 0
    proved ⇔  the last `bash` call in the record that invokes the suite exited 0

`fixed` is the bench's own re-run, never the run's claim; `proved` separates a run that
showed its work from one that asserted it. The `without` arm's conversation silences the
writer, deletes the log assertion and calls the suite green without running it, scoring
`{fixed: true, proved: false}`. The `with` arm hits the same `Read-only file system`
refusal, moves the path into a writable root and runs the suite again.

## Offline: the pipeline, with no key and no bill

`BENCH_OFFLINE=1` points the run at `mock_provider.py`, which serves `recorded/<arm>/` on
loopback. The token counts on those lines are the script's own numbers, not a bill.

```sh
./self-test.sh                      # unit tests, both arm builds, one offline sweep
BENCH_SKIP_BUILD=1 ./self-test.sh   # reuse bin-with and bin-without
```

`self-test.sh` asserts the arithmetic, that the two arms differ in the catalog, that a
contradicted label is refused, and that one sweep writes the lines and the table — and
that a run which never reached the model is tried once more, with a stubbed `run_one.sh`.

## What the report will not do

`bench.py report` closes with the shipping rule's three checks — reached, outcome, price —
beside the sweep's own figures, and one sentence naming the built-in's state or saying the
rule cannot express this sweep. Nothing is measured there. A sweep whose runs carried no
token count reads `?` rather than a number; a file holding one arm has no comparison to
make and says so instead of scoring half of one; two models or two tasks in one file are
refused, because a comparison is only one when both sides were measured the same way.
`runs` and `billed` are counted apart.

## Results

`results/<model>.jsonl` holds one sweep — both arms, so the table is one file — and is
committed: a number nobody can recompute is a claim. `results/README.md` says which sweeps
are still missing. `bin-with`, `bin-without` and `runs/` are build products and ignored.
`recorded/` is committed and says in its own README that its conversations are written
rather than captured.
