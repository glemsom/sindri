# 0004 — The batch output contract

- **Status:** accepted
- **Date:** 2025-10-29

## Context

Batch mode is a Unix filter: its stdout is consumed by other programs and must
be exactly the answer.

## Decision

**stdout** — by default exactly the answer plus one trailing newline. With
`--format json`, one compact object, fixed field order:
`answer, session, turns, stopped, model, duration_ms, tool_calls, usage`.
Absent data means the field is absent, never `null`.

**stderr** — silent by default. `-v` prints reasoning text and tool-call lines
after the run. Nothing else ever writes to stderr except errors.

**stdin** — piped stdin is *context*, appended after the prompt
(`git diff | sindri -b "summarise"`). `sindri -b` with no prompt argument means
stdin *is* the prompt. A non-batch launch with a piped stdin is refused with
exit 2 and a hint, never silently drained.

**Exit codes**

| code | meaning |
| ---- | ------- |
| 0    | answer produced |
| 1    | model or provider error |
| 2    | usage error, including refused piped stdin |
| 3    | sandbox or environment setup failure |
| 100  | turn cap reached (partial answer allowed; `stopped` says why) |
| 130  | cancelled (SIGINT); nothing on stdout |

**Cancellation.** SIGINT raises one flag, shared by the turn loop and the bash
executor; the signal handler does nothing else, because one atomic store is the
only thing a handler may safely do. The loop stops between turns, and a command
already running has its whole process group signalled and then killed, so no
child outlives the cancel. A cancelled command reports no tool result: there is
no model left to read it. `--die-with-parent` in the sandbox argv is what makes
a `kill -9` of Sindri take the command with it.

A provider request already in flight is not interruptible — our own client
bounds it — but it is never retried after a cancel, and a failure it produces is
reported as a cancellation rather than a provider error, so the shell still
sees 130 and not 1.

## Consequences

- Scripts can distinguish "the model failed" from "our sandbox is broken".
- stdout stays byte-clean, so `sindri -b … | jq` and `| wc -l` both work.
