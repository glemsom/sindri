# Eval fixtures

Two kinds of document, and neither is a live capture.

**`repo/`** — the fixture repository every eval works in: a one-command
greeting in POSIX shell, `bin/greet`, `src/greet.sh`, `src/lib.sh` and
`tests/run.sh`. Its test suite fails on purpose (`say_hello` drops the name it
is given), which is the bug eval 5 asks the model to fix; eval 4 therefore
proves a rename by the front end still running rather than by the suite.

**`<n>-<name>/*.json`** — the conversation each eval replays, one Anthropic
`message` body per turn, served in filename order by the mock provider
(`sindri-harness`). Same honesty as `crates/harness/fixtures/openai`: the
environment this was built in had no credential, so the bodies were written to
the wire format the provider documents rather than captured from it. They are
replayed so CI proves our client, our loop and the product's promises without a
key, a network or a bill.

`diff.patch` is the stdin eval 1 pipes in, as the user in the spec does.

Re-recording means running the same prompts against a real provider
(`make eval-live`) and writing each turn's body into the matching directory.
The live run asserts the same structural properties, and skips the assertions
that can only read a recorded conversation (what the model was shown, how many
requests it made).
