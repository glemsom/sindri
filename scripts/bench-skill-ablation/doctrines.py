"""The doctrine a run is measured with, as a text the bench can move.

`crates/core/src/agent/skill/doctrine.md` is compiled into the binary with
`include_str!`, so an arm is the text the binary carries and not a flag a run
could forget to set — the same reason `build_arm.sh` empties the built-in table
rather than passing `--no-skills`. This module is the third arm's text and the
question the bench asks a binary about itself:

    python3 doctrines.py without-trigger crates/core/src/agent/skill/doctrine.md
    python3 doctrines.py carries bin-with
    python3 doctrines.py sentence

`without-trigger` prints the shipped text with the one sentence that asks for a
load removed, so the arm differs from the shipped one by that sentence alone. It
refuses a text that does not carry the sentence: an arm that is the shipped arm
under another name reports one wording twice, which is the one number this bench
must never make.

`carries` answers whether a binary carries the sentence in the text it ships, so
a run whose label and binary disagree is refused before it runs. The question is
asked of the words: the doctrine is prose and wraps, and the binary carries the
wraps with it.
"""
import pathlib
import sys

# The sentence the third arm removes: the doctrine's trigger sentence, with the
# clause that limits it to a task the skill covers. The line saying a load
# happens once is a sentence of its own and stays, so the arm differs by this one
# line and nothing else.
TRIGGER = "Call it before acting when a task is one it covers."

USAGE = ("usage: doctrines.py without-trigger FILE | carries BINARY | sentence")


def without_trigger(text):
    """The shipped doctrine with its trigger sentence gone: every other line is
    the shipped line, in the shipped order, with its terminator.

    The sentence has to be a line of its own — one, and alone — because the arm
    exists to differ by that sentence: a line carrying it and its neighbours
    would take them with it, and a second copy of it would leave one behind."""
    lines = text.splitlines(keepends=True)
    carrying = [line for line in lines if TRIGGER in line]
    if not carrying:
        raise SystemExit(
            "doctrines: the doctrine carries no trigger sentence, so the third arm "
            f"would be the shipped one under another name: {TRIGGER!r} is not in it"
        )
    if len(carrying) > 1:
        raise SystemExit(
            "doctrines: the doctrine carries the trigger sentence on more than one "
            f"line, so removing one would leave another: {TRIGGER!r}"
        )
    if carrying[0].strip() != TRIGGER:
        raise SystemExit(
            "doctrines: the trigger sentence shares its line with other words, so "
            f"removing it would take them with it: {carrying[0].strip()!r}"
        )
    return "".join(line for line in lines if line is not carrying[0])


def carries(binary):
    """Whether this binary carries the trigger sentence in the text it ships.

    The doctrine is compiled in byte for byte, so the question is asked of the
    text as it stands: a prose rewrapped so the sentence spans two lines answers
    `no`, and `without_trigger` refuses that same text. One fact, and not two
    opinions about it — which is why there is no whitespace-collapsing fallback
    here to disagree with the arm's own guard."""
    text = pathlib.Path(binary).read_bytes().decode("utf-8", "replace")
    return TRIGGER in text


def main(argv):
    # The sentence is printed so a shell that wants to look for it asks for it
    # rather than keeping a copy that can drift from the one the arm removes.
    if argv == ["sentence"]:
        print(TRIGGER)
        return 0
    if len(argv) != 2:
        raise SystemExit(USAGE)
    what, path = argv
    if what == "without-trigger":
        sys.stdout.write(without_trigger(pathlib.Path(path).read_text()))
        return 0
    if what == "carries":
        return 0 if carries(path) else 1
    raise SystemExit(f"doctrines: {what} is not a question this module answers\n{USAGE}")


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
