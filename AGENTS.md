## Agent skills

### Issue tracker

Issues live in this repo's GitHub Issues (`glemsom/sindri`), driven by the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Default five-role vocabulary: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `GLOSSARY.md` at the repo root. See `docs/agents/domain.md`.

## Ground rules for this POC

How the code is written: `docs/adr/0001-llm-friendly-code.md`, the one record.

- Linux only: Windows and macOS are never targets, so no cross-platform abstraction earns
  its keep.
- Rust on rig.rs: prefer an existing crate over a hand-rolled one.
- One tool, `bash`, behind the sandbox; the model composes shell tools instead.
- The TUI assumes UTF-8, truecolour and, for the face, the Kitty graphics protocol.
- No sandbox, no agent: a `bwrap` that cannot start is a failure to report, never to paper over.
