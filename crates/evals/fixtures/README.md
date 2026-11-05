# Eval fixtures

The repository each eval works in, the one skill and the one set of house rules a
case turns on, and the conversations replayed — none of them a live capture.

**`repo/`** — the fixture repository: `bin/greet`, `src/greet.sh`, `src/lib.sh`,
`tests/run.sh`. Its suite fails on purpose (`say_hello` drops the name it is given),
which is the bug eval 5 asks the model to fix; eval 4 therefore proves a rename by the
front end by the run still working, not by the suite.

**`repo/.agents/skills/house-style/`** — the one skill it ships, including the
`# house: spindle-4411` marker. Eval 9's check requires the answer to state the rule
and fails it if the answer does so without a prior `skill` call. The marker is in that
skill body and no other file of the tree, which is what makes it proof of the channel.
Each copy is `git init`ed, so the repo scope is walked from the copy's root and nothing
above it is read.

**`10-house-rules/AGENTS.md`** — eval 10's rules, copied to the root of that case alone:
every answer opens with `## house report`, and `sh tests/run.sh` is never run. Neither
appears in the fixture's own README, so a run that kept them was handed them.

**`<n>-<name>/*.json`** — the conversation each eval replays, one Anthropic `message`
body per turn, served in filename order by the mock provider (`sindri-harness`). Like
`crates/harness/fixtures/openai`, the bodies were written to the documented wire format
rather than captured: no credential was available. CI therefore proves our client and
our loop without a key, a network or a bill.

`diff.patch` is the stdin eval 1 pipes in.

Re-recording means `make eval-live` against a real provider and writing each turn's body
into the matching directory; the live run skips the assertions that can only read a
recorded conversation.
