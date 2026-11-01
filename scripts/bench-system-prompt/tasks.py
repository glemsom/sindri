"""The four benchmark tasks: the question, and the check that decides it.

Two ask for a reading (T1 a search over the tree, T2 a region of a 760-line
file) and two for an edit (T3 a flag the fixture's test already expects, T4 a
rename that must not touch `bucket_keys`, the longer name it is a prefix of).
The prompt is the only thing that changes between variants; these texts and the
checks stay still, so a difference in tokens or in a verdict is the prompt's.
"""
import json, os, pathlib, subprocess, sys

TASKS = {
 "T1": {
  "prompt": "List every line of the project that mentions `compute_retry_delay`, as `path:line`, the definition first and nothing else. Then, last line: the delay it returns for attempt 5.",
  },
 "T2": {
  "prompt": "In src/ledger.py, what does `normalise_config` do with a config key it does not recognise? Quote the exact source line that decides it, with its line number.",
  },
 "T3": {
  "prompt": "Add a `--dry-run` flag to src/cli.py that makes the run write no report file. tests/test_cli.py already tests it; make `python3 -m unittest discover -s tests -q` pass.",
  },
 "T4": {
  "prompt": "Rename the helper `bucket_key` in src/ledger.py to `group_key` everywhere it is used, and keep `python3 -m unittest discover -s tests -q` passing.",
  },
}

def run(args, cwd):
    return subprocess.run(args, cwd=cwd, capture_output=True, text=True, timeout=300)

KNOWN_FAILURES = {"test_dry_run_writes_nothing"}

def parse_failures(output):
    """The test ids that failed or errored, minus the one the fixture ships broken."""
    found = set()
    for line in output.splitlines():
        line = line.strip()
        for marker in ("FAIL: ", "ERROR: "):
            if line.startswith(marker):
                found.add(line[len(marker):].split(" ")[0].split(".")[-1])
    return sorted(found - KNOWN_FAILURES)


def check(task, repo, answer):
    if task == "T1":
        want = {"src/retry.py:7", "src/queue.py:5", "src/queue.py:24",
                "src/worker.py:5", "src/worker.py:22",
                "tests/test_retry.py:12", "tests/test_retry.py:13",
                "tests/test_retry.py:16"}
        import re
        got = set()
        for path, line in re.findall(r"([\w./-]+\.pyw?):(\d+)", answer):
            path = path.lstrip("./")
            if os.path.exists(os.path.join(repo, path)):
                got.add(f"{path}:{int(line)}")
        missing, extra = sorted(want - got), sorted(got - want)
        delay = "delay-ok" if "4.0" in answer else "delay-wrong"
        return {"ok": not missing and not extra and delay == "delay-ok",
                "missing": missing, "extra": extra, "delay": delay,
                "found": len(want & got)}
    if task == "T2":
        low = answer.lower()
        line = [n for n, text in enumerate(open(os.path.join(repo, "src/ledger.py")), 1)
                if '"rejected"' in text]
        needle = "rejected"
        quoted = needle in low and "unknown" in low
        right_line = any(str(n) in answer for n in line)
        return {"ok": quoted, "quoted": quoted, "right_line": right_line}
    if task == "T3":
        tests = run([sys.executable, "-m", "unittest", "discover", "-s", "tests", "-q"], repo)
        out = os.path.join(repo, "DRYRUN.txt")
        dry = run([sys.executable, "src/cli.py", "--out", out, "--dry-run"], repo)
        untouched = not os.path.exists(out)
        return {"ok": tests.returncode == 0 and dry.returncode == 0 and untouched,
                "tests": tests.returncode, "dry_rc": dry.returncode, "no_write": untouched,
                "tail": tests.stderr[-300:]}
    if task == "T4":
        import re
        sources = {}
        for path in pathlib.Path(repo).rglob("*.py"):
            sources[str(path.relative_to(repo))] = path.read_text()
        joined = "\n".join(sources.values())
        # the trap: `bucket_keys` is a longer name that the old one is a prefix of
        left = len(re.findall(r"\bbucket_key\b", joined))
        renamed = len(re.findall(r"\bgroup_key\b", joined))
        kept = len(re.findall(r"\bbucket_keys\b", joined))
        tests = run([sys.executable, "-m", "unittest", "discover", "-s", "tests", "-q"], repo)
        new_failures = parse_failures(tests.stderr + tests.stdout)
        over_renamed = len(re.findall(r"\bgroup_keys\b", joined))
        ok = left == 0 and renamed > 0 and kept > 0 and not new_failures and over_renamed == 0
        return {"ok": ok, "left": left, "renamed": renamed, "kept_longer_name": kept,
                "over_renamed": over_renamed, "new_failures": new_failures,
                "tests": tests.returncode}
    raise SystemExit(f"no such task {task}")

if __name__ == "__main__":
    task, repo, answer_file = sys.argv[1:4]
    answer = open(answer_file).read() if os.path.exists(answer_file) else ""
    print(json.dumps(check(task, repo, answer)))
