# The lines the table came from

One file per model, `<model>.jsonl`, holding exactly the lines `sweep.sh`
appended — both arms in one file, because the table is a comparison and a file
holding one arm cannot make one.

```sh
export BENCH_MODEL=deepseek-v4.1-flash
REPS=6 TAG=$BENCH_MODEL ./sweep.sh results/$BENCH_MODEL.jsonl with without
python3 bench.py report results/$BENCH_MODEL.jsonl
```

## What is not here yet

**Every sweep.** No line of the bench's results table has been measured: the environment the bench was built in had no credential, only network
reachability, so the live numbers cannot be produced here. Nothing in that table
is estimated, extrapolated or filled with a placeholder — a row that reads
`not yet measured` is a row with no number, and the command above is what puts
one there.

The offline pass (`./self-test.sh`) is not a substitute and does not land here:
its lines are the recorded conversation's own scripted usage counts, and
`self-test.sh` writes them to a temporary directory, never to `results/`. A file
in this directory means a model was asked and paid for.
