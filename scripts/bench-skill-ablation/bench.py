"""The ablation bench's arithmetic: one run's line, the rule that scores it, and
the table three arms make.

This file is the bench's own arithmetic, stated once so a reader can check the
rule instead of trusting the table. Nothing here asks a model anything: a run's
line is read out of what the binary already printed and what the record already
wrote, and the score is computed from the tree the run left behind.

The rule, in full:

    right  ⇔  fixed  ∧  proved
    fixed  ⇔  `sh tests/run.sh`, run by the bench in the run's own tree, exits 0
    proved ⇔  the last `bash` call in the record that invokes the suite exited 0

`fixed` is the bench's own re-run, never the model's claim, so a run that says
"all tests pass" without running anything is right only if the change really
holds — and `proved` is the half that separates a run that showed its work from
one that asserted it.

The third arm is the shipped one with the doctrine's trigger sentence removed. It
is scored by the rule like every other line, and what it is asked is not the rule
but a rate: whether the sentence is what puts a load before the first command.
That is a fact the transcript carries — `loaded_before_bash`, read from the order
the run wrote its entries in — and never a claim the arm makes about itself. A run
with no first command has no place the load could have come before, so its line
carries no answer and the rate is over the runs that ran one.

A suite invocation is recognised by the shell that runs it (`sh tests/run.sh`,
`bash -e tests/run.sh`, `./tests/run.sh`), not by the string appearing anywhere in
the command: `echo '# tests/run.sh'` is a mention, and a mention proves nothing.

The rule above scores one of the bench's two tasks. The other is scored on a second
axis, because a reading skill cannot buy an outcome the model would not otherwise
reach — it buys the same answer for fewer tokens, and the place that shows is
`in_tokens`. So a task states the axis it is scored on (`tasks.py`), and the report
takes the same three checks from the table either way: a body nobody loads is
**unused**, a body that reaches no outcome is **useless**, and on the price axis a
body whose runs cost more than they save `does not pay for itself`. The table, the
counts and the sentence are the same arithmetic on both; only which check decides
differs.
"""
import argparse
import json
import pathlib
import re
import statistics
import subprocess
import sys
from fractions import Fraction

# The tasks and the built-in each ablates are `tasks.py`'s, so this file asserts
# nothing about a skill it was not asked about: a run in a bare fixture repository
# sees every built-in the binary ships, and a load of another one is a load of
# something else (`activated` reads one name and no other).
import tasks  # noqa: E402 - beside the other imports, and after the ones it answers to

# The three arms, stated once for the whole bench: `with` ships the built-in and
# the shipped doctrine, `without` ships no built-in at all, and `no-trigger` ships
# the built-in with the doctrine one sentence shorter — the shipped words and one
# line less, so what the arms differ by is that sentence and nothing else.
# `build_arm.sh` and `run_one.sh` read this list back through the `arms`
# subcommand rather than spelling it again, so a fourth arm is one line here.
ARMS = ("with", "without", "no-trigger")

# The suite the fixture repository ships, and the proof the task asks for. An
# invocation is recognised where a shell would start one — at the start of the
# command or after a separator that ends the one before it — so `echo sh
# tests/run.sh` and `rg 'sh tests/run.sh' .` are mentions, and a mention is not a
# run. `X=1 sh tests/run.sh` and `if sh tests/run.sh` still count, because they do.
_START = (
    r"(?:^|[;&|(`{}!]|\b(?:if|then|elif|else|do|while|until)\b)"
    r"\s*(?:\w+=\S*\s+)*"
)
SUITE = re.compile(
    _START + r"(?:sh|bash|dash|zsh)\s+(?:-\S+\s+)*(?:\./)?tests/run\.sh\b"
    r"|" + _START + r"(?:\./)?tests/run\.sh\b"
)

# What a report over an empty file says: a table of nothing is not a
# measurement, and a reader must never mistake one for a zero.
NOT_MEASURED = (
    "not yet measured: no run has been recorded, so no arm has a number. "
    "See `scripts/bench-skill-ablation/README.md` for the sweep that fills it."
)


def entries(rundir):
    """Every line of the run's transcript, in the order it was written: the one
    place a run says what it actually did."""
    sessions = pathlib.Path(rundir) / "state/sindri/sessions"
    if not sessions.is_dir():
        return []
    found = []
    for session in sorted(path for path in sessions.iterdir() if path.is_dir()):
        transcript = session / "transcript.jsonl"
        if not transcript.is_file():
            continue
        for line in transcript.read_text(errors="replace").splitlines():
            try:
                found.append(json.loads(line))
            except json.JSONDecodeError:
                continue
    return found


def calls(entries):
    """Each `bash` call the record shows, with the exit code its result
    reported: the command as the model wrote it, and `None` where the call never
    ran or reported no code."""
    commands = {}
    order = []
    for entry in entries:
        kind = entry.get("type")
        if kind == "tool_call" and entry.get("tool") == "bash":
            call_id = entry.get("call_id")
            commands[call_id] = {"command": entry.get("command", ""), "exit_code": None}
            order.append(call_id)
        elif kind == "tool_result" and entry.get("call_id") in commands:
            commands[entry["call_id"]]["exit_code"] = entry.get("exit_code")
    return [commands[call_id] for call_id in order]


def suite_calls(entries):
    """The calls that ran the suite, in order, with the code each reported."""
    return [call for call in calls(entries) if SUITE.search(call["command"])]


def proved(entries):
    """The rule's second half: the run showed the suite passing, last."""
    runs = suite_calls(entries)
    return bool(runs) and runs[-1]["exit_code"] == 0


def fixed(repo):
    """The rule's first half: the bench re-runs the suite in the run's tree."""
    try:
        done = subprocess.run(
            ["sh", "tests/run.sh"],
            cwd=str(repo),
            capture_output=True,
            text=True,
            timeout=120,
        )
    except (OSError, subprocess.SubprocessError):
        return False
    return done.returncode == 0


def scored(entries, fixed):
    """The rule, as one value a table can count and a reader can check."""
    proof = proved(entries)
    return {"ok": bool(fixed) and proof, "fixed": bool(fixed), "proved": proof}


def activations(entries):
    """Every `skill` call the record shows, with the word its result opened
    with: `loaded` is a body the conversation now carries, `already_loaded` is a
    second copy the loop refused, and the rest are calls that loaded nothing."""
    asked = {}
    order = []
    for entry in entries:
        kind = entry.get("type")
        if kind == "tool_call" and entry.get("tool") == "skill":
            asked[entry.get("call_id")] = [entry.get("name", ""), None]
            order.append(entry.get("call_id"))
        elif kind == "tool_result" and entry.get("call_id") in asked:
            asked[entry["call_id"]][1] = entry.get("outcome")
    return [tuple(asked[call_id]) for call_id in order]


def activated(entries, skill):
    """Whether this skill's body entered the run: the difference between a
    built-in that was unused and one that was tried and did not help."""
    return any(name == skill and outcome == "loaded"
               for name, outcome in activations(entries))


def loaded_before_bash(entries, skill):
    """Whether this bench's skill entered the conversation before the run's first
    command: the fact the third arm is scored on, because a run that loads the
    skill after it has already run something is a run the trigger sentence did
    not reach in time.

    `None` when the run has no first command to be before: a run that never
    reached the model wrote nothing to read, and a run that answered without
    running anything says nothing about whether a sentence reached it — counting
    either as a run the sentence failed to reach is the absent-read-as-zero
    mistake this file refuses everywhere else.

    What came before the command is the entries before it, read by the same
    predicate `activated` uses, so the two cannot disagree about what a load is —
    and a load asked in the same wave as the command is not before it, because
    the body arrives with the wave's results and the command had already run."""
    first = next((at for at, entry in enumerate(entries)
                  if entry.get("type") == "tool_call"
                  and entry.get("tool") == "bash"), None)
    if first is None:
        return None
    return activated(entries[:first], skill)


def scored_line(task, repo, answer, entries):
    """What the task's own axis scores this run's answer and tree as.

    A task scored on its outcome is the rule at the top of this file: the suite
    passing in the tree the run left, and the run having shown it pass. A task
    scored on its price is what the answer named — the check is `tasks.py`'s,
    because the file that asks a question is the file that says what counts as an
    answer to it — and its price is the token count every line already carries."""
    if tasks.axis(task) == "price":
        return tasks.inspected(task, answer)
    return scored(entries, fixed(repo))


def named_task(name):
    """One task's name, or the refusal naming the ones the bench holds: a word that
    is not a task would be a sweep of a fixture and a question no report can read."""
    try:
        tasks.task(name)
    except SystemExit:
        raise SystemExit(
            f"bench: the bench holds no task {name!r}: {', '.join(tasks.TASKS)}"
        ) from None
    return name


def named_arm(name):
    """One arm's name, or the refusal arms.sh states in the same words."""
    if name not in ARMS:
        raise SystemExit(
            f"bench: arm must be {', '.join(ARMS[:-1])} or {ARMS[-1]}, not {name!r}"
        )
    return name


def record(*, task, arm, tag, repetition, attempt, model, run, entries, repo,
           answer, elapsed_s, rc=0):
    """One run as one line: what the arms are compared on, and nothing a
    later reader has to guess. `in_tokens` is the run's own summed usage — the
    provider's accounting over the turns — and is null when nobody reported it,
    which is what keeps a run that never reached a model out of a mean.

    The skill is the task's, not the line's: an arm that ships a catalog of three
    bodies is this task's ablation only through the one it covers, so the name
    `activated` is read against is the task's and never the catalog's."""
    task, arm = named_task(task), named_arm(arm)
    skill = tasks.skill(task)
    usage = run.get("usage") or {}
    return {
        "task": task,
        "arm": arm,
        "tag": tag,
        "repetition": repetition,
        "attempt": attempt,
        "model": model,
        "in_tokens": usage.get("input_tokens"),
        "out_tokens": usage.get("output_tokens"),
        "cached_tokens": usage.get("cached_input_tokens"),
        "turns": run.get("turns"),
        "tool_calls": run.get("tool_calls"),
        "activated": activated(entries, skill),
        "loaded_before_bash": loaded_before_bash(entries, skill),
        "skill_calls": [list(call) for call in activations(entries)],
        "suite_calls": len(suite_calls(entries)),
        "checks": scored_line(task, repo, answer, entries),
        "stopped": run.get("stopped"),
        "rc": rc,
        "elapsed_s": elapsed_s,
    }


def summarize(lines):
    """One row per arm: how many runs, how many of them carry a token count, what
    those came to, how many activated the skill, how many loaded it before the
    first command, and how many were right.

    `billed` is the count the mean and the median are over, and it is a field of
    its own because it is not `runs`: a run that never reached the model carries
    no `in_tokens` and is left out of the arithmetic, so a row whose two counts
    differ is an arm measured over fewer runs than it made — and a comparison
    across two such arms is not like for like. `commanded` is that count for the
    load's place, for the same reason: a run with no first command has no place
    the load could have come before, and it is left out of the rate rather than
    counted as a run the sentence failed to reach."""
    arms = {}
    for line in lines:
        arm = arms.setdefault(
            line.get("arm", "?"),
            {"runs": 0, "tokens": [], "activated": 0, "loaded_first": 0,
             "commanded": 0, "right": 0},
        )
        arm["runs"] += 1
        if line.get("in_tokens") is not None:
            arm["tokens"].append(line["in_tokens"])
        if line.get("activated"):
            arm["activated"] += 1
        if line.get("loaded_before_bash") is not None:
            arm["commanded"] += 1
        if line.get("loaded_before_bash"):
            arm["loaded_first"] += 1
        if (line.get("checks") or {}).get("ok"):
            arm["right"] += 1
    return {
        name: {
            "runs": arm["runs"],
            "billed": len(arm["tokens"]),
            "mean": round(statistics.fmean(arm["tokens"])) if arm["tokens"] else None,
            "median": round(statistics.median(arm["tokens"])) if arm["tokens"] else None,
            "activated": arm["activated"],
            "loaded_first": arm["loaded_first"],
            "commanded": arm["commanded"],
            "right": arm["right"],
        }
        for name, arm in arms.items()
    }


def comparable(lines):
    """One sweep is one model on one task: two of either is a mistake in the file
    names, not a spread, so it is refused rather than averaged. The same reason
    `median.jq` refuses a file holding two variants."""
    for field in ("model", "task"):
        named = sorted({line.get(field) for line in lines},
                       key=lambda value: (value is None, str(value)))
        if len(named) > 1:
            raise SystemExit(
                f"report: these lines hold more than one {field}, so they are two "
                f"sweeps and not one comparison: {named}"
            )


def unwarned(table):
    """The two things a reader of the table has to be told before believing it:
    a row whose means are over fewer runs than it made, and two rows whose means
    are over different numbers of runs."""
    notes = []
    for name in sorted(table):
        row = table[name]
        if row["billed"] != row["runs"]:
            notes.append(
                f"`{name}` made {row['runs']} runs and {row['billed']} of them carried "
                f"a token count; the others are left out of its mean."
            )
    for field, word in (("runs", "made"), ("billed", "were billed for")):
        counts = {name: table[name][field] for name in table}
        if len(set(counts.values())) > 1:
            counts = ", ".join(f"{name} {n}" for name, n in sorted(counts.items()))
            notes.append(
                f"the arms {word} different numbers of runs ({counts}), so the row "
                f"beside each other is not like for like."
            )
    return notes


# The shipping rule a report applies before any sweep arrives, as the three
# thresholds it holds each sweep to: reached in at least five of six runs, an
# outcome of six of six with the built-in against at most four of six without, and
# no more than a quarter again the mean input tokens without it. They live here, as
# ratios, so a sweep of another length is read by the same rule. A task scored on
# its price holds the same three checks and reads the third as the decisive one: the
# outcome it has to reach is the same answer on both arms, and what it is scored on
# is what the answer cost.
#
# The price axis reads the third check with a floor of its own. A body that buys an
# outcome is allowed a quarter again in tokens to buy it; a body whose whole purchase
# *is* the price cannot spend a quarter again and still have bought anything, so
# there the check is a payment or it is not.
REACHED = 5 / 6
DECISIVE = 4 / 6
PRICE = 1.25
PAYS = 1.0


def shipping(table, skill, axis):
    """The shipping rule applied to one sweep's table: what the three states of a
    built-in come to as a sentence a reader acts on, rather than a paragraph they
    apply by hand to the numbers.

    Nothing here is measured and nothing is invented: every figure is the table's,
    and a sweep a check cannot be computed from says so — a mean over no billed
    run is not a price, and a table holding one arm has no comparison."""
    mine, theirs = table.get("with"), table.get("without")
    if not mine or not theirs:
        return (f"the rule needs both arms in one file; this holds "
                f"{', '.join(sorted(table)) or 'none'}.")
    reached = mine["activated"] / mine["runs"]
    right_with = mine["right"] / mine["runs"]
    right_without = theirs["right"] / theirs["runs"]
    if mine["mean"] is None or theirs["mean"] is None or not theirs["mean"]:
        price = None
        ratio = None
    else:
        ratio = mine["mean"] / theirs["mean"]
        price = ratio < PAYS if axis == "price" else ratio <= PRICE
    measured_outcome = (f"{mine['right']}/{mine['runs']} with, "
                        f"{theirs['right']}/{theirs['runs']} without")
    checks = [
        ("reached", f"{mine['activated']}/{mine['runs']}",
         "at least 5/6", reached >= REACHED),
        ("outcome", measured_outcome,
         "6/6 with, at most 4/6 without" if axis == "outcome"
         else "every run with it right",
         right_with == 1 and (right_without <= DECISIVE or axis == "price")),
        ("price", "no mean without a billed run" if ratio is None else f"{ratio:.2f}x",
         "under 1.00x" if axis == "price" else "at most 1.25x", price),
    ]
    rows = ["### The shipping rule", "",
            "| check | measured | needs | holds |",
            "| --- | --- | --- | --- |"]
    for name, measured, needs, holds in checks:
        rows.append(f"| {name} | {measured} | {needs} | "
                    f"{'yes' if holds else ('no' if holds is not None else '?')} |")
    rows += ["", verdict(mine, theirs, ratio, skill, axis)]
    return "\n".join(rows)


def verdict(mine, theirs, ratio, skill, axis):
    """The sentence the rule comes to: one of the page's three states, or the
    page's own note that this sweep is one the rule cannot express.

    The states are not shades of the same number — an unused catalog line is a
    description to rewrite, a useless body is one to drop, and a useful one is
    kept at almost any price under the ratio — so the sentence says which, and the
    two axes reach them by their own road: an outcome is bought with the work, a
    price with the tokens the same work took."""
    runs, without_runs = mine["runs"], theirs["runs"]
    reached = mine["activated"] / runs
    right_with = mine["right"] / runs
    right_without = theirs["right"] / without_runs
    if reached < REACHED:
        return (f"`{skill}` is **unused**: loaded in {mine['activated']} of "
                f"{runs} runs, under the five of six the rule asks for. The line "
                f"is a tax no run is reached by, so the thing to fix is the "
                f"catalog entry — rewrite the description, not the body.")
    if right_with < 1:
        return (f"`{skill}` is **useless**: {mine['right']} of {runs} runs with it "
                f"were right, so the body buys no outcome to weigh against the "
                f"line it costs. Not worth a catalog line: drop it or fold it into "
                f"a skill a task triggers.")
    if axis == "price":
        return priced(mine, theirs, ratio, skill)
    if ratio is not None and DECISIVE < right_without < 1:
        return (f"the rule cannot express this sweep: {theirs['right']} of "
                f"{without_runs} runs without the skill is one run from six of six, "
                f"which is the spread of six repetitions rather than a gap the "
                f"rule can hold. The rule is the thing to revise, and the revision "
                f"is where the argument goes.")
    if ratio is not None and ratio > PRICE:
        return (f"`{skill}` does not pay for itself: {ratio:.2f}x the mean input "
                f"tokens without it, past the 1.25x the rule allows. Not worth a "
                f"catalog line: drop it.")
    if right_without > DECISIVE:
        return (f"`{skill}` is **useless**: right in {theirs['right']} of "
                f"{without_runs} runs without it, against every run with it, so the "
                f"body changed the conversation and not the outcome. Not worth a "
                f"catalog line: drop it or fold it into a skill a task triggers.")
    if ratio is None:
        return unpriced(skill)
    return f"`{skill}` earns its catalog line."


def unpriced(skill):
    """The one sentence both axes owe a sweep with no token count in it: a price
    over no billed run is not a payment, and saying so is not saying zero."""
    return (f"`{skill}` has no price to read: no run of either arm carried a token "
            f"count, so nothing says what the line costs. Measure again before "
            f"believing anything here.")


def priced(mine, theirs, ratio, skill):
    """The verdict for a task scored on what its runs cost: the same three states,
    read off the ratio instead of the outcomes.

    The run that says the body did not pay has to be able to say so plainly — a
    reading skill earns its catalog line by spending fewer tokens on the same
    answer, so a ratio past the rule's is the whole of the argument against it, and
    an outcome that improved as well is a note beside the number and never a reason
    to keep a body that costs more than it saves."""
    runs, without_runs = mine["runs"], theirs["runs"]
    if ratio is None:
        return unpriced(skill)
    answered = (f"{mine['right']}/{runs} with it right against "
                f"{theirs['right']}/{without_runs} without")
    if ratio < PAYS:
        return (f"`{skill}` earns its catalog line: {ratio:.2f}x the mean input "
                f"tokens without it, on the same task and the same answer "
                f"({answered}).")
    return (f"`{skill}` does not pay for itself: {ratio:.2f}x the mean input "
            f"tokens without it, and {answered}. A body whose whole purchase is the "
            f"price has to cost less than the reads it replaces, so this one is a "
            f"catalog line to drop — not to keep because its outcome held.")


def load_rate(arm):
    """The share of one arm's runs that loaded the skill before the first command,
    exactly, over the runs that ran one: `None` when none did, because a rate over
    no run is not a zero."""
    if not arm["commanded"]:
        return None
    return Fraction(arm["loaded_first"], arm["commanded"])


def wording(table):
    """The third arm as one sentence: whether the trigger sentence is what puts a
    load before the first command.

    The arm is the shipped doctrine one line shorter, so the pair is the whole
    measurement — the shipped rate over the rate with the sentence gone, on the
    same catalog and the same body. `None` when the sweep holds no third arm:
    two arms say nothing about a sentence neither of them moved."""
    mine, quiet = table.get("with"), table.get("no-trigger")
    if not mine or not quiet:
        return None
    asked, silent = load_rate(mine), load_rate(quiet)
    measured = (f"a skill was loaded before the first command in "
                f"{mine['loaded_first']} of {mine['commanded']} runs with the "
                f"trigger sentence and {quiet['loaded_first']} of "
                f"{quiet['commanded']} without it")
    if asked is None or silent is None:
        return (f"the trigger sentence has no rate to read: no run of one arm "
                f"ran a command, so nothing says whether a load came before the "
                f"first one. Measure again before believing anything here.")
    if asked > silent:
        return (f"the trigger sentence earns its place: {measured}, so the words "
                f"are what ask for the load before the first command.")
    if asked == silent:
        return (f"the trigger sentence moves nothing: {measured}, so the load "
                f"happens first either way and the line is paid for nothing.")
    return (f"the trigger sentence is the words to rewrite: {measured}, so "
            f"removing it asks for the load earlier than the shipped line does.")


def report(lines):
    """The table: one row per arm — the shipped one with the built-in in the
    catalog, the arm with no built-in at all, and the third arm one sentence
    shorter than the shipped one — so a shipped line is measured against its own
    absence and its own wording.

    The task the lines came from says which skill the table is about and what it is
    scored on, and both are read out of the file rather than passed in, so a report
    is the same command over a sweep whatever task that sweep measured."""
    if not lines:
        return NOT_MEASURED
    comparable(lines)
    task = named_task(lines[0].get("task"))
    skill, axis = tasks.skill(task), tasks.axis(task)
    table = summarize(lines)
    rows = [
        "| arm | runs | billed | mean input tokens | median input tokens | "
        "activated | loaded before bash | right |",
        "| --- | --- | --- | --- | --- | --- | --- | --- |",
    ]
    for name in sorted(table, key=lambda arm: (arm != "with", arm)):
        row = table[name]
        cells = [
            f"{row['runs']}",
            f"{row['billed']}",
            "-" if row["mean"] is None else f"{row['mean']}",
            "-" if row["median"] is None else f"{row['median']}",
            f"{row['activated']}/{row['runs']}",
            "-" if not row["commanded"]
            else f"{row['loaded_first']}/{row['commanded']}",
            f"{row['right']}/{row['runs']}",
        ]
        rows.append(f"| {name} | " + " | ".join(cells) + " |")
    notes = unwarned(table) + scored_on(task, skill, axis)
    if notes:
        rows += ["", *[f"* {note}" for note in notes]]
    rows += ["", shipping(table, skill, axis)]
    sentence = wording(table)
    if sentence:
        rows += ["", "### The sentence", "", sentence]
    return "\n".join(rows)


def scored_on(task, skill, axis):
    """The one line every table owes its reader before the numbers: the task the
    sweep measured, the body it ablated, and what the arms were compared on — an
    outcome they had to reach, or the price they reached it for. Two tables of the
    same bench are two questions, and a reader who is not told which cannot read
    either."""
    measured = ("the work had to hold afterwards" if axis == "outcome"
                else "the answer had to be the same and the tokens are the score")
    return [f"`{task}` ablates `{skill}`, scored on {axis}: {measured}."]


def read(path):
    """One sweep file back as lines; a file that is not there holds no run."""
    path = pathlib.Path(path)
    if not path.is_file():
        return []
    found = []
    for line in path.read_text().splitlines():
        if line.strip():
            found.append(json.loads(line))
    return found


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)

    one = sub.add_parser("record", help="one run's line, read from its directory")
    one.add_argument("rundir")
    # neither name is `choices=`: the refusal that names what the bench holds is
    # the bench's own, in the words the scripts and the reader use
    one.add_argument("--task", required=True)
    one.add_argument("--arm", required=True)
    one.add_argument("--tag", default="run")
    one.add_argument("--repetition", type=int, default=1)
    one.add_argument("--attempt", type=int, default=1)
    one.add_argument("--model", default="")
    one.add_argument("--elapsed", type=int, default=0)

    sub.add_parser("arms", help="the arms, space-separated, for the shell scripts")
    sub.add_parser("tasks", help="the tasks, space-separated, for the shell scripts")

    table = sub.add_parser("report", help="one sweep file as a table")
    table.add_argument("files", nargs="+")

    args = parser.parse_args(argv)
    if args.command == "arms":
        print(" ".join(ARMS))
        return 0
    if args.command == "tasks":
        print(" ".join(tasks.TASKS))
        return 0
    if args.command == "record":
        rundir = pathlib.Path(args.rundir)
        try:
            run = json.loads((rundir / "out.json").read_text())
        except (OSError, json.JSONDecodeError):
            run = {}
        code = (rundir / "rc.txt").read_text().strip() if (rundir / "rc.txt").is_file() else "0"
        line = record(
            task=args.task,
            arm=args.arm,
            tag=args.tag,
            repetition=args.repetition,
            attempt=args.attempt,
            model=args.model,
            run=run,
            entries=entries(rundir),
            repo=rundir / "repo",
            answer=run.get("answer", ""),
            elapsed_s=args.elapsed,
            rc=int(code) if code.lstrip("-").isdigit() else -1,
        )
        print(json.dumps(line))
        return 0
    lines = []
    for path in args.files:
        lines.extend(read(path))
    print(report(lines))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
