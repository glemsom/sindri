# 0006 — Session records

- **Status:** accepted
- **Date:** 2025-10-29

## Decision

Every run creates `$XDG_STATE_HOME/sindri/sessions/<guid>/`:

- `session.json` — metadata: provider, model, start/end, turns, usage, stop reason
- `transcript.jsonl` — messages, tool calls, tool results, in order
- `tmp/` — the writable session temp, plus `home/` mounted as `HOME`
- `status.json` — bwrap's `--json-status-fd` documents, kept for post-mortems

v0.1 deletes nothing and prunes nothing; a manual `sessions prune` command is a
later issue. The `session` field in the JSON output is this directory's GUID.

## Consequences

- After a failure, the whole run is inspectable without re-running it.
- TUI session restore needs no new storage design.
- Disk grows; accepted for now, revisit before 1.0.
