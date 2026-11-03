---
status: accepted
date: 2026-10-29
---

# 0001 — Code an LLM can read and change

Sindri is written mostly by agents, so the tree has to be legible to one holding a narrow
slice of it: find the module, read its first paragraph, change it, run the gate. Each
rule below saves that reader a search; none of it is ceremony.

- **Rust modules stay small enough to navigate.** No `*.rs` file over 800 lines, held by
  `scripts/audit-line-length.sh`, which has no allowlist and one way out: split the file.
  The limit is about module navigation, so `*.sh` and `*.md` are outside it
  (`scripts/tests/line-length.sh` pins that scope) — `GLOSSARY.md`, far outside it, is a
  stated choice rather than a violation.
- **A module opens with `//!`.** One paragraph on what it is for, a second only when it
  carries a separate decision, so the file can be placed without reading its body. Inner
  attributes go below it, since the first line is the one that gets scanned.
  `scripts/audit-module-docs.sh` holds every file to at least eight words after the
  marker: a bare `//!` is a placeholder, and the audit names the file that stopped at one.
- **A crate root places its modules.** Every `mod` and `pub mod` in a `crates/*/src/lib.rs`
  is trailed by one clause, so "which file?" is answered where the search starts. The
  clause is cut from that module's paragraph, never written twice, and is at least three
  words — below that it is a label, which sends the reader back to the file it was meant
  to spare them. `scripts/audit-module-map.sh` reports a missing clause, a label and a
  word the paragraph does not carry, each against the file that fell short.
- **Comments say what the code cannot** — the rule behind it, the trap it avoids. A
  comment that repeats the line beneath it is noise.
- **Tests hold the spec.** A behaviour worth a sentence is worth a test name, and the name
  states the behaviour rather than the function: `scripts/audit-test-names.sh` holds every
  `#[test]` name to four words and to no `test_`, `it_`, `works` or `check_` first word.
- **`make check` is done**: fmt, clippy, the audits, the tests, the evals. Nothing lands
  on a subset.
