# 0006 — Session records

- **Status:** accepted
- **Date:** 2025-10-29

## Decision

Every run creates `$XDG_STATE_HOME/sindri/sessions/<guid>/`:

- `session.json` — metadata: provider, model, whether the run was sandboxed or
  unsafe, start/end, turns, usage, stop reason — and the failure, when one ended
  it. Rewritten in one atomic rename when the run ends, so a reader never sees
  half a document. Nothing else is touched until then.
- `transcript.jsonl` — one JSON object per line: the prompt, each assistant
  turn, each tool call, each tool result. Written and flushed as the run goes,
  because the end is exactly what a crash takes away.
- `tmp/` — the writable session temp, plus `home/` mounted as `HOME`
- `status.json` — bwrap's `--json-status-fd` documents, one per line, annotated
  with the call they belong to and written the moment each arrives. A killed run
  therefore keeps the `child-pid` document, which is how a post-mortem tells "the
  sandbox never started" from "the command never finished".

A record that still says `stopped: running` is a run that crashed. Credentials
are redacted before a single byte is written. Everything the run was asked or
answered, and every command it ran, is read back with
`sindri sessions show <guid>`.

v0.1 deletes nothing and prunes nothing; a manual `sessions prune` command is a
later issue. The `session` field in the JSON output is this directory's GUID.

## Consequences

- After a failure, the whole run is inspectable without re-running it.
- TUI session restore needs no new storage design.
- Disk grows; accepted for now, revisit before 1.0.
