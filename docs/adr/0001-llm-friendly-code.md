---
status: accepted
date: 2026-10-29
---

# 0001 — Code an LLM can read and change

Sindri is written mostly by agents, so the tree must be legible to one holding a narrow
slice of it: find the module, read its first paragraph, change it, run the gate.

- **Rust modules stay small enough to navigate.** No `*.rs` file over 800 lines, held by
  `scripts/audit-line-length.sh`, with no allowlist and one way out: split the file. `*.sh`
  and `*.md` are outside the limit (`scripts/tests/line-length.sh` pins that scope);
  `GLOSSARY.md` is a stated choice rather than a violation.
- **A module opens with `//!`.** One paragraph on what it is for, a second only for a
  separate decision, so the file is placed without reading its body. Inner attributes go
  below it, since the first line is the one scanned. `scripts/audit-module-docs.sh` holds
  every file to at least eight words after the marker.
- **A crate root places its modules.** Every `mod` in a `crates/*/src/lib.rs` is trailed by
  one clause cut from that module's paragraph, so "which file?" is answered where the
  search starts. `scripts/audit-module-map.sh` reports a missing clause and a clause the
  paragraph does not carry.
- **Comments say what the code cannot** — the rule behind it, the trap it avoids. A
  comment that repeats the line beneath it is noise.
- **Tests hold the spec.** A test name states the behaviour, not the function:
  `scripts/audit-test-names.sh` holds every `#[test]` name to four words and to no `test_`,
  `it_`, `works` or `check_` first word.
- **`make check` is done.** Nothing lands on a subset.
