# A bench for the system prompt

The machine that produced the numbers: a fixture repo, four tasks with a pass rule each,
the prompts tried, and two scripts that run one prompt or a sweep of them against a real
provider.

Nothing here is on the gate — it needs a credential and a bill, and its numbers come from a
model rather than from the tree.

```sh
export SINDRI_PROVIDER=openai SINDRI_API_KEY=… SINDRI_BASE_URL=https://…
export BENCH_MODEL=<the model the prompts are compared on>

python3 make_fixture.py fixture          # the repo every run works in
./build_variant.sh v02                   # patch the prompt, build, keep bin-v02
REPS=6 TAG=$BENCH_MODEL ./sweep.sh results/$BENCH_MODEL-v02.jsonl v02
```

`run_one.sh` does one run in a fresh copy of `fixture/`: the task's question, then the
answer, the task's verdict and what the run cost — input tokens summed over the turns,
output tokens, turns, and the bytes of tool output the model read.

The variants are the texts in `variants.py`; `build_variant.sh` writes one into the
`STANCE` literal of `crates/core/src/preamble.rs` — the one file the tree ships its prompt
in — builds and copies the binary, and puts that file back byte for byte on every way out,
a failed build included. `v00` is not one of those texts: it is the tree's own `STANCE`,
which `variants.py` reads out of that file. A variant replaces the stance and nothing else;
the Environment section is composed per run and is the same in every arm.

`scripts/tests/bench-prompt.sh` checks both halves on the gate — the baseline is read out of
the file that ships the prompt, and the source comes back byte for byte — without a build or
a bill.

## Repetitions

`REPS` is the number of complete passes over the four tasks, six by default. The sweep
appends one JSON line per run, each carrying its own `variant`, `tag`, `repetition` and
`attempt`: the repetition makes a median possible, the tag keeps two sweeps in one file
apart. `TAG` names the run directories under `runs/` and defaults to `run`; the repetition
is part of that name, so a repeated tag never overwrites the transcript that explains a
number. A repetition the file already holds for the same variant and tag is refused rather
than recorded twice under one number.

A retry is not a repetition: a run that never reached the model is tried once more, and
only the line that came back is recorded, carrying `attempt: 2`.

## From lines to the numbers in the doc

One file per model per variant, and `median.jq` turns one into the median pass:

```sh
jq -s -f median.jq results/deepseek-v4.1-flash-v00.jsonl
```

A file holding two variants is refused, not averaged — that is a mistake in the file name,
not a spread. The per-task means in the first table come out of the same file.

`results/README.md` says what belongs there and which sweeps are still missing.
`bin-*/`, `fixture/` and `runs/` are build products and ignored; `results/` is committed,
because a number nobody can recompute is a claim.

`self-test.sh` proves the loop and the median without a key or a bill: it copies the
scripts to a temporary directory, stubs `run_one.sh`, and asserts the line counts, the tag,
the refusal and the median. It is manual, like the rest of the bench.
