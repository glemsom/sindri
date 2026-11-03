# Written, not captured

Every `*.json` under `<task>/<arm>/` was written by hand to the Anthropic wire format:
no credential was available where either task was built, so there was nothing to
capture. They are fixed replies served in filename order, and they prove that the
pipeline in front of a model parses, scores, records and reports.

The `usage` counts inside them are the script's own numbers, not a bill. A table
built from them measures the mock.

## `refused-write/` — the capability task

- `with/` loads the `sandbox-refusal` built-in, runs the suite, reads the `Read-only
  file system` refusal, reads the two files naming the log's directory, makes that
  path a parameter defaulting into the run's scratch root, and proves it by running
  the suite again. `{fixed, proved, ok} = {true, true, true}`.
- `without/` reaches for `sudo`, is refused, silences the writer, deletes the log
  assertion from `tests/run.sh` and says the suite passes without running it.
  `{true, false, false}` — the three moves the built-in's body forbids, made in order.
- `no-trigger/` is `with/`'s work in the order an arm without the trigger sentence
  leaves it: the failure first, the built-in loaded after it. It scores the same and
  carries `loaded_before_bash: false` against `with/`'s `true`.

## `inspect/` — the reading task

Both shipped arms reach the same answer, because that is the premise of a reading
task: what the body is supposed to buy is the price of getting there, and only the
scripted token counts differ.

- `with/` loads the `search-and-read` built-in first, then does what its body asks —
  one `rg -n` for the cap and every file that names it, then `sed -n '38,48p'` for
  the line that applies it. 5160 input tokens over four turns.
- `without/` `cat`s all four files, then greps the tests, and answers the same
  thing. 12400 input tokens over three turns: the default the body is there to
  correct, priced.
- `no-trigger/` is `with/`'s work with the search before the load. It answers the
  same and carries `loaded_before_bash: false`.

Neither arm of either task measures a model. `../mock_provider.py` serves them on
loopback; `self-test.sh` is the only reader and writes its lines to a temporary
directory, never to `results/`.
