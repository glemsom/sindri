---
status: accepted
date: 2026-10-29
---

# 0004 — The batch output contract

## Decision

**stdout** — the answer plus exactly one trailing `\n`. With `--format json`, one compact object, fixed field order (`crates/core/src/output.rs`); later fields append only, absent data means the field is absent, never `null`:

```text
answer, session, turns, stopped, model, duration_ms, tool_calls, usage
```

**stderr** — silent unless `-v`, which prints reasoning text and one line per tool call after the run. Nothing else writes there but errors.

**stdin** — piped stdin is context appended after the prompt (`git diff | sindri -b "summarise"`); `sindri -b` with no prompt argument means stdin *is* the prompt. A non-batch launch with piped stdin is refused, exit 2, with a hint.

**Exit codes** (`crates/types/src/lib.rs`, `ExitCode`):

| code | meaning |
| --- | --- |
| 0 | answer produced |
| 1 | model or provider error |
| 2 | usage error, including refused piped stdin |
| 3 | sandbox or environment setup failure |
| 100 | turn cap reached (partial answer allowed; `stopped` says why) |
| 130 | cancelled (SIGINT); stdout empty |

**Cancellation.** SIGINT raises one atomic flag shared by the loop and the executor. The loop stops between turns; a running command's whole process group is signalled, then killed; a cancelled command yields no tool result. `--die-with-parent` makes a `kill -9` of Sindri take the command with it. An in-flight request is not interrupted, is never retried after a cancel, and reports as 130, never 1.
