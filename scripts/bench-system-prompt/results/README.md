# The lines the tables came from

One file per model per variant, `<model>-<variant>.jsonl`, holding exactly the
lines `sweep.sh` appended — one variant per file, because `median.jq` refuses a
file that mixes two. The sweep does not rewrite them, so a file here is the
record of one sweep and nothing else:

```sh
export BENCH_MODEL=deepseek-v4.1-flash
REPS=6 TAG=$BENCH_MODEL ./sweep.sh results/$BENCH_MODEL-v00.jsonl v00
jq -s -f median.jq results/$BENCH_MODEL-v00.jsonl    # the median pass
```

`runs/`, `fixture/` and `bin-*/` stay ignored — they are build products, and the
transcripts in `runs/` are large. This directory is the exception, because a
number nobody can recompute is a claim.

## What is not here yet

The sweeps behind `docs/system-prompt.md` were run before `sweep.sh` had a
repetition loop: it truncated its output file once per variant, and no result
file was ever committed, so the raw lines behind either table are gone. The
tables stand as a record of what was observed and cannot yet be recomputed from
the tree.

Re-running the eight that the second table rests on — four variants on two
models, six repetitions each — is what closes it, and the filenames above are
where they go:

```sh
for v in v00 v02 v11 v13; do
  for m in deepseek-v4.1-flash claude-haiku-5-5; do
    BENCH_MODEL=$m REPS=6 TAG=$m ./sweep.sh results/$m-$v.jsonl $v
  done
done
```

It needs a credential and a bill, which is why the bench is not on `make check`
and why this file is a note rather than a number.
