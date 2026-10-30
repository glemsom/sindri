---
status: accepted
date: 2026-10-29
---

# 0001 — Code an agent can maintain

This is the only architecture decision record. It is a rulebook, not a rationale:
an agent reads it before it edits, and every rule is checkable.

## Decision

1. **Source files ≤ 400 lines**, enforced by `scripts/audit-line-length.sh` against
   `scripts/line-length-allowlist.txt` (empty on purpose). A file that outgrows the
   limit is split, not allowlisted.
2. **One module, one job**, stated in its module doc comment (`//!`). The first
   paragraph says what the module is for; the next edits add to that shape rather than
   working around it.
3. **A comment states what the code cannot.** Why, or the rule the code implements.
   No citations to external records and no restating what the line already says.
   A decision lives in code, a test, `GLOSSARY.md`, or nothing.
4. **`clippy -D warnings`**, with a curated set in `clippy.toml`. Never blanket
   `pedantic`: an unreadable diagnostic is an unfixable one.
5. **Tests are the specification.** `cargo nextest`; `assert_cmd` drives the CLI as a
   process; JSON snapshots are exact literals; the loop runs against a mock provider
   server. A behaviour worth a paragraph in a document is worth a test here.
6. **`make check` is the definition of done**: `fmt clippy audit test relink eval
   scripts-check` (see `Makefile`). Nothing merges on a subset.
7. **Virtual workspace; no crate named `core`** (it shadows the sysroot). Provider
   types stay confined to their adapter module.
8. **New decisions are not new ADRs.** Write the rule where the code is: module doc
   comment, test name, `GLOSSARY.md` entry. Keep `GLOSSARY.md` and the code in step;
   a glossary entry that drifts from the code makes every reader a liar.

## Consequences

- An agent localises a change without reading the world: one module, one doc comment,
  one test.
- The searchable truth is the tree itself, so a stale document cannot mislead.
- Anything historical — why a rejected design lost — is not recorded; the code and its
  tests are.
