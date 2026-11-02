"""The one task this bench runs, and the capability it is shaped like.

The built-in's body is a doctrine of order: read before you write, read the
result of the command you just ran, and say what you did. The task is the
capability that doctrine is supposed to buy — **prove your change** — and its
score is in `bench.py`: the suite passes in the tree the run left behind, and
the record shows the run running it last. Nothing in this file scores anything;
the prompt is the question and `bench.py` is the rule.
"""

TASK = "prove-your-change"

PROMPT = (
    "`sh tests/run.sh` fails in this repository. Fix the source so the suite "
    "passes, and prove the fix: run the suite once it is green, and let the "
    "answer say what the suite printed."
)
