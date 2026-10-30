## Agent skills

### Issue tracker

Issues live in this repo's GitHub Issues (`glemsom/sindri`), driven by the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Default five-role vocabulary: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `GLOSSARY.md` at the repo root. See `docs/agents/domain.md`.

## Ground rules for this POC

Read `docs/adr/0001-code-an-agent-can-maintain.md` before editing; it is the only ADR.

- Linux only. Windows and macOS are never targets, so no cross-platform abstraction earns its keep.
- Rust, built and gated through devbox or containers so a build can be reproduced (`make check`).
- Prefer an existing crate over a hand-rolled one; the loop sits on rig.rs.
- One tool, `bash`, behind the sandbox. Uniqueness of `bash` replaces a
  tool-per-task catalogue: the model composes shell tools instead.
- The TUI assumes UTF-8, truecolour and, for the face, the Kitty graphics protocol.
- No sandbox, no agent: a `bwrap` that cannot start is a failure to report, never to paper over.
