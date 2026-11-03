# The lines the table came from

One file per model per task, `<model>-<task>.jsonl`, holding exactly the lines
`sweep.sh` appended — all three arms in one file, because the table is a comparison, and
one task in one file, because two tasks are two questions.

```sh
export BENCH_MODEL=deepseek-v4.1-flash
REPS=6 TAG=$BENCH_MODEL ./sweep.sh results/$BENCH_MODEL-inspect.jsonl with without no-trigger
python3 bench.py report results/$BENCH_MODEL-inspect.jsonl
```

## What is not here yet

**Every sweep.** No row of the results table has been measured: only network reachability
was available where the task was built, never a credential. A row reading `not yet
measured` is a row with no number, and the command above is what puts one there. Nothing
here is estimated or filled with a placeholder.

The sweep the split rests on is therefore outstanding:

- `-inspect`: whether the `search-and-read-files` body earns its line by spending fewer
  input tokens on the same answer, and whether the doctrine's trigger sentence is what
  asks for a load before the first command. This is the one
  that settles the untested risk `docs/adr/0003-built-in-skills.md` §3 names — whether a
  skill needed before the first command is loaded before it — on a task where the body's
  own subject, reading fewer bytes, is what is being counted.

The offline pass (`./self-test.sh`) is not a substitute and does not land here: its lines
are the recorded conversations' scripted usage counts, written to a temporary directory. A
file in this directory means a model was asked and paid for.
