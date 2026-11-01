# A bench for the system prompt

`docs/system-prompt.md` records what this measured and what it changed. This
directory is the machine that produced the numbers: a fixture repo, four tasks
with a pass rule each, the thirteen prompts tried, and two scripts that run one
prompt or a sweep of them against a real provider.

Nothing here is on the gate — it needs a credential and a bill, and its numbers
come from a model rather than from the tree.

```sh
export SINDRI_PROVIDER=openai SINDRI_API_KEY=… SINDRI_BASE_URL=https://…
export BENCH_MODEL=<the model the prompts are compared on>

python3 make_fixture.py fixture          # the repo every run works in
./build_variant.sh v02                   # patch the prompt, build, keep bin-v02
./sweep.sh results/v02.jsonl v02          # four tasks, one JSON line per run
```

`run_one.sh` does one run: a fresh copy of `fixture/`, the task's question, then
the answer, the task's verdict and what the run cost — input tokens summed over
the turns, output tokens, turns, and the bytes of tool output the model read.

The prompt under test lives in `variants.py`; `build_variant.sh` writes it into
`crates/core/src/provider/rig_wire.rs`, builds, and copies the binary, so
`git checkout crates/core/src/provider/rig_wire.rs` afterwards restores the tree.

`bin-*/`, `fixture/`, `runs/` and `results/` are build products and are ignored.
