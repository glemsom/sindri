# One pass over many files

The plan is one JSON list, an edit per entry: the file, the anchor it is found
by, and what replaces it. Nothing is written until every anchor in the plan has
been counted, so one wrong anchor leaves every file exactly as it was and the
pass says which file and how many times.

```python
#!/usr/bin/env python3
"""Every anchor is counted before any file is written: a bad one names its file."""
import json
import pathlib
import sys


def main(plan_path):
    plan = json.loads(pathlib.Path(plan_path).read_text())
    # One copy per file, so two edits of one file are made in the order the plan
    # lists them and the file is written once.
    texts, problems = {}, []
    for edit in plan:
        name = edit["file"]
        if name not in texts:
            path = pathlib.Path(name)
            texts[name] = path.read_text() if path.is_file() else None
        text = texts[name]
        if text is None:
            problems.append(f"{name}: no such file")
            continue
        found = text.count(edit["anchor"])
        if found != 1:
            problems.append(
                f"{name}: the anchor appears {found} times, not once: {edit['anchor']!r}"
            )
            continue
        texts[name] = text.replace(edit["anchor"], edit["replacement"], 1)
    if problems:
        raise SystemExit("\n".join(problems))  # nothing has been written
    for name, text in texts.items():
        pathlib.Path(name).write_text(text)
    print(f"{len(texts)} files changed")


main(sys.argv[1])
```

The list is written to a file rather than piped in because the same plan is then
re-run after the fix, and a plan in a file is the one both runs used. A file the
plan names and the tree does not hold is its own problem — `no such file` — and
not an anchor counted zero times. Add the narrowest check to the same pass once
the writes are done: `python3 apply.py plan.json && cargo test -p the-one-crate`
is one `bash` call that proves what it changed.
