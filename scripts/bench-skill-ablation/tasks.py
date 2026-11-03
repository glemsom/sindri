"""The one task this bench runs, and the capability it is shaped like.

The built-in's body is what to do when the environment refuses a command: a
write denied, a fetch with no credential. The task is the capability that body
is supposed to buy — **recover from a refused write** — and its score is in
`bench.py`: the suite passes in the tree the run left behind, and the record
shows the run running it last. Nothing in this file scores anything; the prompt
is the question and `bench.py` is the rule.

The fixture's suite fails because the greeting log is written to a hard-coded
`/var/lib/greet`, which the sandbox's read-only host root refuses. The move the
body teaches — find a writable root, then make the path a parameter with a
writable default — is what makes the suite pass; the default the body forbids —
escalate, silence the writer, delete the assertion — is what the `without` arm's
recorded conversation does.
"""

TASK = "refused-write"

PROMPT = (
    "`sh tests/run.sh` fails in this repository. Fix the source so the suite "
    "passes, and prove the fix: run the suite once it is green, and let the "
    "answer say what the suite printed and where the greeting log ends up."
)
