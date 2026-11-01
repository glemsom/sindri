"""Build the fixture repo the prompt benchmarks run in."""
import os, sys, textwrap
from pathlib import Path

root = Path(sys.argv[1])
(root / "src").mkdir(parents=True, exist_ok=True)
(root / "tests").mkdir(parents=True, exist_ok=True)

(root / "README.md").write_text("""# ledgerkit

A tiny reporting tool: reads a JSON config, normalises it, and writes a
report file. Run the tests with:

    python3 -m unittest discover -s tests -q
""")

(root / "src/retry.py").write_text('''"""Retry timing: the one place a backoff delay is decided."""

BASE_DELAY = 0.25
MAX_DELAY = 30.0


def compute_retry_delay(attempt, jitter=0.0):
    """Exponential backoff, capped, with optional jitter in seconds."""
    if attempt < 1:
        raise ValueError("attempt must be at least 1")
    delay = BASE_DELAY * (2 ** (attempt - 1))
    delay = min(delay, MAX_DELAY)
    if jitter:
        delay += jitter
    return round(delay, 3)
''')

(root / "src/queue.py").write_text('''"""The job queue: a list and a drain loop."""

import time

from retry import compute_retry_delay

QUEUE = []


def enqueue(job):
    QUEUE.append(job)


def drain(handler, attempts=3):
    while QUEUE:
        job = QUEUE.pop(0)
        for attempt in range(1, attempts + 1):
            try:
                handler(job)
                break
            except Exception:
                if attempt == attempts:
                    raise
                time.sleep(compute_retry_delay(attempt))
''')

(root / "src/worker.py").write_text('''"""A worker that fails the first two times and then succeeds."""

import sys

from retry import compute_retry_delay

FAILURES = [2]


class Flaky:
    def __init__(self):
        self.calls = 0

    def handle(self, job):
        self.calls += 1
        if self.calls <= FAILURES[0]:
            raise RuntimeError("not yet")
        return job


def sleep_for(attempt):
    return compute_retry_delay(attempt, jitter=0.05)


if __name__ == "__main__":
    print(sleep_for(int(sys.argv[1]) if sys.argv[1:] else 1))
''')

(root / "tests/test_retry.py").write_text('''import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

import retry


class RetryTest(unittest.TestCase):
    def test_backoff_doubles(self):
        self.assertEqual(retry.compute_retry_delay(1), 0.25)
        self.assertEqual(retry.compute_retry_delay(3), 1.0)

    def test_capped(self):
        self.assertEqual(retry.compute_retry_delay(20), 30.0)


if __name__ == "__main__":
    unittest.main()
''')

# ledger.py: a long file whose interesting function sits in the middle.
header = '''"""Ledger entries: the long, boring half of the tool."""

import json


'''
blocks = []
for i in range(1, 61):
    blocks.append(f'''def entry_{i:02d}(row, scale=1):
    """Fold row {i:02d} into a running total."""
    total = 0
    for key, value in sorted(row.items()):
        if not isinstance(value, (int, float)):
            continue
        total += value * scale
        if total > 1000:
            total = 1000
    return {{"index": {i}, "total": total, "keys": len(row)}}


''')

middle = '''def normalise_config(config):
    """Validate a config mapping before anything reads it.

    Every known key is kept as written; a key nobody recognises is a mistake
    loud enough to refuse the whole config rather than silently ignore it.
    """
    known = {"path", "scale", "verbose", "entries"}
    unknown = set(config) - known
    if unknown:
        return {"status": "rejected", "unknown": sorted(unknown), "accepted": {}}
    accepted = {
        "path": str(config.get("path", "report.txt")),
        "scale": float(config.get("scale", 1)),
        "verbose": bool(config.get("verbose", False)),
        "entries": sorted(config.get("entries", [])),
    }
    return {"status": "accepted", "unknown": [], "accepted": accepted}


'''

half = len(blocks) // 2
body = header + "".join(blocks[:half]) + middle + "".join(blocks[half:]) + '''def bucket_key(name):
    """The grouping key a report line is filed under."""
    return name.strip().lower().replace(" ", "_")


def bucket_keys(rows):
    """Every bucket key the rows fall under, sorted."""
    return sorted({bucket_key(row.get("name", "")) for row in rows})


def group(rows):
    """Group rows by their bucket key."""
    grouped = {}
    for row in rows:
        grouped.setdefault(bucket_key(row.get("name", "")), []).append(row)
    return grouped
'''
(root / "src/ledger.py").write_text(body)

(root / "src/cli.py").write_text('''"""The command line: read a config, group rows, write a report."""

import argparse
import json
import os
import sys

from ledger import group, normalise_config

ROWS = [
    {"name": "Alpha One", "value": 3},
    {"name": "alpha one", "value": 4},
    {"name": "Beta", "value": 10},
]


def write_report(path, grouped):
    with open(path, "w", encoding="utf-8") as handle:
        json.dump({key: len(rows) for key, rows in grouped.items()}, handle, indent=2, sort_keys=True)


def main(argv=None):
    parser = argparse.ArgumentParser(prog="ledgerkit")
    parser.add_argument("--config", default=None)
    parser.add_argument("--out", default="report.txt")
    args = parser.parse_args(argv)
    config = {}
    if args.config:
        with open(args.config, encoding="utf-8") as handle:
            config = json.load(handle)
    normalised = normalise_config(config)
    if normalised["status"] != "accepted":
        print("refused:", ", ".join(normalised["unknown"]), file=sys.stderr)
        return 2
    grouped = group(ROWS)
    write_report(args.out, grouped)
    print(f"wrote {args.out}" if normalised["accepted"]["verbose"] else "done")
    return 0


if __name__ == "__main__":
    sys.exit(main())
''')

(root / "tests/test_ledger.py").write_text('''import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

import ledger


class LedgerTest(unittest.TestCase):
    def test_bucket_key_normalises(self):
        self.assertEqual(ledger.bucket_key(" Alpha One "), "alpha_one")

    def test_bucket_keys_are_sorted(self):
        rows = [{"name": "Beta"}, {"name": "alpha one"}, {"name": "Alpha One"}]
        self.assertEqual(ledger.bucket_keys(rows), ["alpha_one", "beta"])


if __name__ == "__main__":
    unittest.main()
''')

(root / "tests/test_cli.py").write_text('''import json
import os
import subprocess
import sys
import tempfile
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CLI = os.path.join(ROOT, "src", "cli.py")


def run(*args, cwd=None):
    return subprocess.run(
        [sys.executable, CLI, *args],
        capture_output=True, text=True, cwd=cwd or ROOT,
    )


class CliTest(unittest.TestCase):
    def test_writes_a_report(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = os.path.join(tmp, "report.txt")
            done = run("--out", out)
            self.assertEqual(done.returncode, 0, done.stderr)
            with open(out, encoding="utf-8") as handle:
                self.assertEqual(json.load(handle), {"alpha_one": 2, "beta": 1})

    def test_dry_run_writes_nothing(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = os.path.join(tmp, "report.txt")
            done = run("--out", out, "--dry-run")
            self.assertEqual(done.returncode, 0, done.stderr)
            self.assertFalse(os.path.exists(out), "a dry run must not write the report")

    def test_unknown_key_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            config = os.path.join(tmp, "config.json")
            with open(config, "w", encoding="utf-8") as handle:
                json.dump({"path": "x", "wat": 1}, handle)
            done = run("--config", config)
            self.assertEqual(done.returncode, 2)


if __name__ == "__main__":
    unittest.main()
''')
print("fixture:", root)
