---
status: accepted
date: 2026-10-29
---

# 0006 — Every run leaves a record

## Decision

`$XDG_STATE_HOME/sindri/sessions/<guid>/` holds:

| entry | contents |
| --- | --- |
| `session.json` | provider, model, enforcement, start/end, turns, tool_calls, duration_ms, usage, `stopped`, `error`. Written at start (`stopped: running`), rewritten by atomic rename at the end |
| `transcript.jsonl` | one JSON object per line: the prompt, each assistant turn, each tool call, each tool result. Flushed as the run goes |
| `status.json` | bwrap `--json-status-fd` documents, one JSON line each, annotated with the call they belong to |
| `tmp/` | the writable session temp |
| `tmp/home/` | the sandbox `HOME`, empty and writable |

- A record still saying `stopped: running` is a crashed or superseded run.
- Credentials are redacted before the first byte is written.
- Read back with `sindri sessions show <guid>`, listed in the frame by `/sessions` and
  copied out of the rail with `Ctrl-G`; the GUID is the JSON output's `session` field.
  `/export [path]` writes the same entries as one markdown file. Both read; neither writes
  a record.
- v0.1 prunes nothing; `sessions prune` is a later issue.

## Consequences

- A failure is inspectable without a re-run; session restore needs no new storage design.
- Disk grows; revisit before 1.0.
