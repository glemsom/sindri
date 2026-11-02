# Written, not captured

Every `*.json` in `with/` and `without/` was written by hand to the Anthropic
wire format. None of them is a capture of a model: the environment this bench was
built in had no credential, so there was nothing to capture. They are scripts —
fixed replies served in filename order — and they exist to prove that the pipeline
in front of a model parses, scores, records and reports.

The `usage` counts inside them are the script's own numbers, not a bill: `with`
reaches a summed 5270 input tokens and `without` 720 because those are the
figures typed into the replies. A table built from them measures the mock.

What each arm's conversation is *for*:

- `with/` loads the built-in, reads the suite, patches `src/greet.sh`, runs
  `sh tests/run.sh` and answers. Scores `{fixed, proved, ok} = {true, true, true}`.
- `without/` patches the same file and says the suite is green without running it.
  Scores `{true, false, false}` — the difference `bench.py`'s rule exists to draw.

`mock_provider.py` in the parent directory serves them on loopback; `self-test.sh`
is the only thing that reads them, and it writes its lines to a temporary
directory, never to `results/`. See `../results/README.md` for why that matters.
