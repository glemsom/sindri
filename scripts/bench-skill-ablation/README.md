# A bench for a built-in skill

This directory is the machine: one task shaped like a capability, run twice —
with the built-in in the catalog, and with the table that ships it emptied —
scored by a rule, and reported as one table. The rule, and the thresholds a
report applies, are `bench.py`'s, stated once there.

No number here is on the gate: it would come from a model and a bill, and
the offline pass below proves the pipeline rather than a model. The guard rails —
an arm that is not a name, a sweep of no arms or of zero repetitions, a record
line for an arm that does not exist, a report over a file no sweep wrote — are
covered by `scripts/tests/skill-ablation.sh`, which `make scripts-check` runs.

```sh
export SINDRI_PROVIDER=openai SINDRI_API_KEY=… SINDRI_BASE_URL=https://…
export BENCH_MODEL=<the model the two arms are compared on>

./build_arm.sh with                    # the tree as it stands   → bin-with
./build_arm.sh without                 # the built-in table empty → bin-without
REPS=6 TAG=$BENCH_MODEL ./sweep.sh results/$BENCH_MODEL.jsonl with without
python3 bench.py report results/$BENCH_MODEL.jsonl
```

## The two arms are two builds, not a flag

`build_arm.sh without` empties `BUILTINS` in `crates/core/src/skills/builtin.rs`,
builds, and puts the file back byte for byte. A run cannot forget to set it, and
no product code carries a switch that exists only for a bench. With the table
empty the catalog holds no skill at all — the fixture repository ships none and
`run_one.sh` points `HOME` at an empty directory — so the `skill` tool is not
registered and the run pays for neither the catalog line nor a body.

## One run, one line

`run_one.sh ARM RUNDIR` works in a fresh `git init`ed copy of `fixture/` and
prints one JSON line: the task, the arm, the tag, the repetition and the attempt;
`in_tokens` — the provider's own count summed over the turns of the run —
beside `out_tokens`, `turns` and `tool_calls`; `activated` and `skill_calls`,
which is what tells an **unused** built-in from a **useless** one; `suite_calls`;
and `checks.{ok,fixed,proved}`, the rule.

The arm is read from the binary, not from the flag: `run_one.sh` asks
`bin-<arm> doctor` and refuses a run whose label its binary contradicts. The
record shape is the prompt bench's — `task`, `tag`, `repetition`, `attempt`,
`in_tokens`, `out_tokens`, `turns`, `tool_calls`, `checks`, `rc`, `elapsed_s`,
`stopped` — with `arm` where that one writes `variant`, and `activated`,
`skill_calls` and `suite_calls` added, because an ablation needs to know whether
the body was ever loaded.

The rule is in `bench.py`, stated once and tested in `tests/`:

    right  ⇔  fixed ∧ proved
    fixed  ⇔  `sh tests/run.sh`, run by the bench in the run's tree, exits 0
    proved ⇔  the last `bash` call in the record that invokes the suite exited 0

`fixed` is the bench's own re-run, never the run's claim. `proved` is the half
that separates a run that showed its work from one that asserted it: the
recorded conversation of the `without` arm silences the writer, deletes the log
assertion and says the suite is green without running it, and scores `fixed:
true, proved: false`. The `with` arm hits the same `Read-only file system`
refusal, moves the path into a writable root and runs the suite again.

## Offline: the pipeline, with no key and no bill

`BENCH_OFFLINE=1` points the run at `mock_provider.py`, which serves the recorded
conversation in `recorded/<arm>/` on loopback — the same honesty as
`crates/evals`: the bodies are written to the wire format, not captured from a
model, so what is replayed is our loop and our product. The token counts on those
lines are the script's own numbers, not a model's bill.

```sh
./self-test.sh                 # unit tests, both arm builds, one offline sweep
BENCH_SKIP_BUILD=1 ./self-test.sh   # reuse bin-with and bin-without
```

`self-test.sh` asserts the arithmetic (the record line, the rule, the activation
field, the table), then that the two arms really differ in the catalog and that a
label one of them contradicts is refused, then that one sweep through both arms
writes the lines and the table the doc prints — and that a run which never
reached the model is tried once more, with a stubbed `run_one.sh` so the retry is
the thing under test rather than a provider's mood.

## What the report will not do

`bench.py report` ends with the shipping rule `bench.py` applies: the three
checks — reached, outcome, price — with the sweep's own figures beside the
threshold each is held to, and one sentence saying which of the built-in's
states it is in, or that the rule cannot express this sweep. Nothing is measured
there: the means, the activations and the rights are the table's, and the price of
a sweep whose runs carried no token count reads `?` rather than a number, with a
sentence saying there is no price to read. A file holding one arm has no comparison
to make at all, so it says that instead of scoring half of one.

`bench.py report` counts `runs` and `billed` apart, because a line with no
`in_tokens` is left out of the mean and a row of six runs over two billed runs
would otherwise read as a mean of six. Different counts across the two arms get
a sentence under the table. Two models or two tasks in one file are refused: a
comparison of one against the other is only a comparison when both sides were
measured the same way.

## Results

`results/<model>.jsonl` holds one sweep — both arms, so the table is one file —
and is committed: a number nobody can recompute is a claim. `results/README.md`
says which sweeps are still missing.

`bin-with`, `bin-without` and `runs/` are build products and are ignored.
`fixture/` is committed, because the repository the task is asked in is a fixture
and not an artefact. `recorded/` is committed too and says in its own README that
its conversations are written rather than captured — a scripted reply that could
be mistaken for a model's is the one thing this bench must never ship quietly.
