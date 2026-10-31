---
status: accepted
date: 2026-10-29
---

# 0001 — Code an LLM can read and change

Sindri is written mostly by agents, so the tree has to be legible to one holding a
narrow slice of it: find the module, read its first paragraph, change it, run the gate.
Each line below saves a reader a search; none of it is ceremony.

- **Files stay small.** No source file over 400 lines, checked by
  `scripts/audit-line-length.sh` against an empty allowlist. Split the file, do not
  allowlist it.
- **A module opens with `//!`**: one paragraph on what it is for, so it can be placed
  without reading its body. Inner attributes go below it — the first line is the one
  that gets scanned — and `scripts/audit-module-docs.sh` holds every file to that.
- **Comments say what the code cannot** — the rule behind it, the trap it avoids. A
  comment that repeats the line beneath it is noise.
- **Tests hold the spec.** A behaviour worth a sentence is worth a test name, and the
  name states the behaviour rather than the function.
- **`make check` is done**: fmt, clippy, the audit, the tests, the evals. Nothing lands
  on a subset.
