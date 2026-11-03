## Agent skills

### Issue tracker

Issues live in this repo's GitHub Issues (`glemsom/sindri`), driven by the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

This repository's five triage labels, and what each means, are stated once: see
`docs/agents/triage-labels.md`.

### Domain docs

Single-context: `GLOSSARY.md` at the repo root. See `docs/agents/domain.md`.

## Ground rules for this POC

Four records hold the decisions: `docs/adr/0001-llm-friendly-code.md` for how the code is
written, `docs/adr/0002-two-tools.md` for the three tools and why neither `skill` nor `open`
is a `bash` command, `docs/adr/0004-built-in-skills.md` for the scope a skill is found in, and
`docs/adr/0005-program-status.md` for what a run tells the terminal it is doing.
`docs/adr/0003-a-third-tool-open.md` is superseded and points at 0002.

- Linux only: Windows and macOS are never targets, so no cross-platform abstraction earns
  its keep.
- Rust on rig.rs: prefer an existing crate over a hand-rolled one.
- Three tools, and `docs/adr/0002-two-tools.md` argues each: `bash` runs everything behind
  the sandbox, and the model composes shell tools instead of being handed one per job;
  `skill` loads instructions written for the model; `open` hands the host's browser one
  target, a URL or a file, because no environment inside the sandbox has a display to open
  one on.
- The TUI assumes UTF-8, truecolour and, for the face, the Kitty graphics protocol.
- No sandbox, no agent: a `bwrap` that cannot start is a failure to report, never to paper over.
