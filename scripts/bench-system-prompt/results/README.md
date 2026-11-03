# The lines the tables came from

One file per model per variant, `<model>-<variant>.jsonl`, holding exactly the lines
`sweep.sh` appended — one variant per file, because `median.jq` refuses a file that mixes
two. The sweep does not rewrite them, so a file here is the record of one sweep:

```sh
export BENCH_MODEL=deepseek-v4.1-flash
REPS=6 TAG=$BENCH_MODEL ./sweep.sh results/$BENCH_MODEL-v00.jsonl v00
jq -s -f median.jq results/$BENCH_MODEL-v00.jsonl    # the median pass
```

`runs/`, `fixture/` and `bin-*/` stay ignored — build products, and the transcripts in
`runs/` are large. This directory is the exception: a number nobody can recompute is a
claim.

## The rewritten preamble's sweep

`v00` is read out of `crates/core/src/preamble.rs` and moves with it, so it is not the
name to record a measurement under: `v15` is the rewritten preamble of #416 pinned in
`variants.py`, and a `-v15` file is a measurement of those words whatever the tree does
next. Neither file exists yet — see below — and the command that writes this one is:

```sh
export BENCH_MODEL=<the model> REPS=6 TAG=<the model>
./build_variant.sh v15
./sweep.sh results/$BENCH_MODEL-v15.jsonl v15
jq -s -f median.jq results/$BENCH_MODEL-v15.jsonl
```

## What is not here yet

The sweeps behind the prompt table predate `sweep.sh`'s repetition loop, which truncated
its output file once per variant, and no result file was committed: the raw lines behind
either table are gone. The tables record what was observed and cannot be recomputed from
the tree. Re-running the eight the second table rests on — four variants on two models,
six repetitions each — is what closes it:

```sh
for v in v00 v02 v11 v13; do
  for m in deepseek-v4.1-flash claude-haiku-5-5; do
    BENCH_MODEL=$m REPS=6 TAG=$m ./sweep.sh results/$m-$v.jsonl $v
  done
done
```

`-v15` is missing beside them: the prompt the tree ships now was measured before this
bench could record a sweep, and the stance's own comment carries the figure it produced
(0.42x the input tokens of the same model told nothing) — a figure this tree cannot
recompute and does not pretend to. The three commands above are what turn it into one.

It needs a credential and a bill, which is why the bench is not on `make check` and why
this file is a note rather than a number.
