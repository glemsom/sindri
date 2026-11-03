"""The two tasks this bench runs, and the capability each is shaped like.

The first is the capability the `sandbox-refusal` body is supposed to buy —
**recover from a refused write** — and its score is in `bench.py`: the suite passes
in the tree the run left behind, and the record shows the run running it last.
Nothing in this file scores it; the prompt is the question and `bench.py` is the
rule.

The second is the capability the `search-and-read` body is supposed to buy — **find
one fact in a tree for the fewest bytes** — and its score is the price of the read:
two runs that reach the same answer are compared by the input tokens they spent
reaching it, because that is the only place a reading skill can win. A reading
skill that answered more cheaply *and wrongly* has bought nothing, so the answer is
checked as well and the check is here rather than in `bench.py`: the file that asks
the question is the file that says what counts as an answer to it.

The inspect fixture is the same kind of repository the first one is — small, real,
runnable — with one fact in it that is cheap to find and expensive to read: a retry
delay capped at a constant. A run that searches for the cap pays for a line; a run
that `cat`s the sources pays for all of them.
"""

import pathlib
import re

# The file the inspect task's fact is in, relative to the fixture's root, and the
# two numbers its answer turns on: the delay the module's cap produces for the
# attempt the prompt asks about (0.25 * 2 ** 7 is 32, and 32 is past the cap), and
# the source line that applies the cap. The line is found in the fixture rather
# than written down here, so a fixture that moves cannot leave a stale number
# behind; the cap is written down here and asserted against the fixture by a test,
# because a check that read its expectation out of the tree it is checking would
# agree with the tree by construction.
SOURCE = "src/backoff.py"
CAP = 8.0
DECIDES = "return _ceiling"

# The two tasks, stated once for the scripts that build, run and report them: the
# skill each is an ablation of, the axis it is scored on — `outcome`, where the
# work has to hold afterwards, or `price`, where the tokens are the measurement —
# and the fixture repository it stands in. The scripts read the two names back
# (`bench.py tasks`, and `tasks.py prompt|fixture TASK`) rather than spelling either
# again, so a third task is one entry here and no shell to edit.
TASKS = {
    "refused-write": {
        "skill": "sandbox-refusal",
        "axis": "outcome",
        "fixture": "fixture/refused-write",
        "prompt": (
            "`sh tests/run.sh` fails in this repository. Fix the source so the suite "
            "passes, and prove the fix: run the suite once it is green, and let the "
            "answer say what the suite printed and where the greeting log ends up."
        ),
    },
    "inspect": {
        "skill": "search-and-read",
        "axis": "price",
        "fixture": "fixture/inspect",
        "prompt": (
            "In `src/backoff.py`, `retry_delay` is asked for the delay after attempt 7. "
            "What does it return, and which line applies the cap? Quote that line, as "
            "`path:line`. Then name every file in the repository that mentions "
            "`_ceiling`. Last line: the delay, in seconds."
        ),
    },
}

USAGE = "usage: tasks.py prompt|skill|axis|fixture TASK"


def task(name):
    """One task's entry, or the refusal naming the ones the bench holds. A typo is
    not a task: a sweep labelled with one would write lines no report can read."""
    try:
        return TASKS[name]
    except KeyError:
        raise SystemExit(
            f"tasks: the bench holds no task {name!r}: {', '.join(TASKS)}"
        ) from None


def prompt(name):
    """The question this task asks, as the run is given it."""
    return task(name)["prompt"]


def skill(name):
    """The built-in this task is an ablation of: the only body whose load counts
    as this task's activation."""
    return task(name)["skill"]


def axis(name):
    """What this task is scored on: the outcome it had to reach, or the price it
    reached it for."""
    return task(name)["axis"]


def fixture(name):
    """The repository this task stands in, relative to the bench's own directory."""
    return task(name)["fixture"]


def fixture_root(name):
    """The bench's own copy of a task's fixture: where the answer to its question
    is read from.

    Never the run's tree. A run is given a copy to read, and a check that read its
    expectation out of that copy would move the answer to whatever the run did to
    it — a run that edited the file would be scored against its own edit, and one
    that deleted the line would make the check refuse to score at all."""
    return pathlib.Path(__file__).resolve().parent / task(name)["fixture"]


def mentioning(name):
    """Every file in the fixture that mentions the cap, as paths from the tree's
    root: the second half of the inspect task's answer, read out of the fixture
    rather than listed here, so a fixture that gains a file the answer has to name
    costs no edit in this file."""
    root = fixture_root(name)
    found = set()
    for path in root.rglob("*"):
        if not path.is_file() or ".git" in path.parts or "__pycache__" in path.parts:
            continue
        if "_ceiling" in path.read_text(errors="replace"):
            found.add(str(path.relative_to(root)))
    return found


def deciding_line(path):
    """The line of `path` that applies the cap, counted from one: the line an
    answer to the inspect task has to quote."""
    lines = [
        number
        for number, text in enumerate(pathlib.Path(path).read_text().splitlines(), 1)
        if text.strip().startswith(DECIDES)
    ]
    if len(lines) != 1:
        raise SystemExit(
            f"tasks: {path} holds {len(lines)} lines starting with {DECIDES!r}, so "
            "the inspect task has no one line to name"
        )
    return lines[0]


def states_the_delay(answer):
    """Whether the answer *said* the delay, rather than merely carrying the number.

    The cap's value is in the fixture's source as `_ceiling = 8.0`, so a run that
    pasted a search's output answers nothing and contains the figure anyway. The
    check reads for the figure beside the word for the wait the prompt asked about,
    which is what separates an answer from a dump of the material it was cut from.
    """
    figure = re.escape(f"{CAP:g}")
    return re.search(
        rf"(?:delay|wait|returns?|waits)\b[^.\n]{{0,40}}?(?<![\d.]){figure}(?:\.0)?(?![\d])",
        answer,
    ) is not None


def inspected(task_name, answer):
    """Whether an answer said what the inspect task asked for, and where it fell
    short: the deciding line quoted as `path:line`, the delay the cap produces, and
    every file that mentions the cap. All three, because the price of a read is only
    worth counting when the thing bought is the same answer."""
    root = fixture_root(task_name)
    line = deciding_line(root / SOURCE)
    wanted = sorted(mentioning(task_name))
    named = f"{SOURCE}:{line}" in answer
    delay = states_the_delay(answer)
    missing = [name for name in wanted if name not in answer]
    return {
        "ok": named and delay and not missing,
        "line": named,
        "delay": delay,
        "missing": missing,
    }


def main(argv):
    if len(argv) == 2 and argv[0] in {"prompt", "skill", "axis", "fixture"}:
        print({"prompt": prompt, "skill": skill, "axis": axis, "fixture": fixture}[
            argv[0]
        ](argv[1]))
        return 0
    raise SystemExit(USAGE)


if __name__ == "__main__":
    import sys

    sys.exit(main(sys.argv[1:]))
