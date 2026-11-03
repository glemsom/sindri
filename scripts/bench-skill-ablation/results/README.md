# The lines the table came from

One file per model, `<model>.jsonl`, holding exactly the lines `sweep.sh` appended — both
arms in one file, because the table is a comparison.

```sh
export BENCH_MODEL=deepseek-v4.1-flash
REPS=6 TAG=$BENCH_MODEL ./sweep.sh results/$BENCH_MODEL.jsonl with without
python3 bench.py report results/$BENCH_MODEL.jsonl
```

## What is not here yet

**Every sweep.** No row of the results table has been measured: only network reachability
was available where the bench was built, never a credential. A row reading
`not yet measured` is a row with no number, and the command above is what puts one there.
Nothing here is estimated or filled with a placeholder.

The offline pass (`./self-test.sh`) is not a substitute and does not land here: its lines
are the recorded conversation's scripted usage counts, written to a temporary directory. A
file in this directory means a model was asked and paid for.
