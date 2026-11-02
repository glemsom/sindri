"""The ablation bench's arithmetic: one run's line, the rule that scores it, and
the table two arms make.

`docs/skill-ablation.md` records what this measured; this file is the machine's
own part of it, stated once so a reader can check the rule instead of trusting
the table. Nothing here asks a model anything: a run's line is read out of what
the binary already printed and what the record already wrote, and the score is
computed from the tree the run left behind.

The rule, in full:

    right  ⇔  fixed  ∧  proved
    fixed  ⇔  `sh tests/run.sh`, run by the bench in the run's own tree, exits 0
    proved ⇔  the last `bash` call in the record that invokes the suite exited 0

`fixed` is the bench's own re-run, never the model's claim, so a run that says
"all tests pass" without running anything is right only if the change really
holds — and `proved` is the half that separates a run that showed its work from
one that asserted it. A suite invocation is recognised by the shell that runs it
(`sh tests/run.sh`, `bash -e tests/run.sh`, `./tests/run.sh`), not by the string
appearing anywhere in the command: `echo '# tests/run.sh'` is a mention, and a
mention proves nothing.
"""
import argparse
import json
import pathlib
import re
import statistics
import subprocess
import sys

# The built-in this bench ablates: the one the binary ships, and the only skill
# a run in the bare fixture repository can see.
SKILL = "sindri"

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


def activated(entries, skill=SKILL):
    """Whether this skill's body entered the run: the difference between a
    built-in that was unused and one that was tried and did not help."""
    return any(name == skill and outcome == "loaded"
               for name, outcome in activations(entries))


def record(*, task, arm, tag, repetition, attempt, model, run, entries,
           fixed, elapsed_s, rc=0):
    """One run as one line: what the two arms are compared on, and nothing a
    later reader has to guess. `in_tokens` is the run's own summed usage — the
    provider's accounting over the turns — and is null when nobody reported it,
    which is what keeps a run that never reached a model out of a mean."""
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
        "activated": activated(entries),
        "skill_calls": [list(call) for call in activations(entries)],
        "suite_calls": len(suite_calls(entries)),
        "checks": scored(entries, fixed),
        "stopped": run.get("stopped"),
        "rc": rc,
        "elapsed_s": elapsed_s,
    }


def summarize(lines):
    """One row per arm: how many runs, how many of them carry a token count, what
    those came to, how many activated the skill, and how many were right.

    `billed` is the count the mean and the median are over, and it is a field of
    its own because it is not `runs`: a run that never reached the model carries
    no `in_tokens` and is left out of the arithmetic, so a row whose two counts
    differ is an arm measured over fewer runs than it made — and a comparison
    across two such arms is not like for like."""
    arms = {}
    for line in lines:
        arm = arms.setdefault(
            line.get("arm", "?"),
            {"runs": 0, "tokens": [], "activated": 0, "right": 0},
        )
        arm["runs"] += 1
        if line.get("in_tokens") is not None:
            arm["tokens"].append(line["in_tokens"])
        if line.get("activated"):
            arm["activated"] += 1
        if (line.get("checks") or {}).get("ok"):
            arm["right"] += 1
    return {
        name: {
            "runs": arm["runs"],
            "billed": len(arm["tokens"]),
            "mean": round(statistics.fmean(arm["tokens"])) if arm["tokens"] else None,
            "median": round(statistics.median(arm["tokens"])) if arm["tokens"] else None,
            "activated": arm["activated"],
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


def report(lines):
    """The table: the arm with the built-in in the catalog beside the arm
    without it, so a shipped line is measured against its own absence."""
    if not lines:
        return NOT_MEASURED
    comparable(lines)
    table = summarize(lines)
    rows = [
        "| arm | runs | billed | mean input tokens | median input tokens | activated | right |",
        "| --- | --- | --- | --- | --- | --- | --- |",
    ]
    for name in sorted(table, key=lambda arm: (arm != "with", arm)):
        row = table[name]
        cells = [
            f"{row['runs']}",
            f"{row['billed']}",
            "-" if row["mean"] is None else f"{row['mean']}",
            "-" if row["median"] is None else f"{row['median']}",
            f"{row['activated']}/{row['runs']}",
            f"{row['right']}/{row['runs']}",
        ]
        rows.append(f"| {name} | " + " | ".join(cells) + " |")
    notes = unwarned(table)
    if notes:
        rows += ["", *[f"* {note}" for note in notes]]
    return "\n".join(rows)


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
    one.add_argument("--task", required=True)
    one.add_argument("--arm", required=True, choices=["with", "without"])
    one.add_argument("--tag", default="run")
    one.add_argument("--repetition", type=int, default=1)
    one.add_argument("--attempt", type=int, default=1)
    one.add_argument("--model", default="")
    one.add_argument("--elapsed", type=int, default=0)

    table = sub.add_parser("report", help="one sweep file as a table")
    table.add_argument("files", nargs="+")

    args = parser.parse_args(argv)
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
            fixed=fixed(rundir / "repo"),
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
