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

## Amendment

1. **A missing value is not a failure at the frame.** Added after the table above; it changes what one door does with `ConfigError::Missing`, and nothing else. Batch mode, `sandbox exec` and every other config-resolving caller keep the rule: a value the run cannot do without is one line on stderr and exit 1. The frame is the exception, because it is the door a first run meets before any value exists at all: it resolves the same config with the model and the credential optional (`Config::for_front_end`, the same read `doctor` and `sandbox exec` make), opens on what it has, and stages the settings modal on the row it does not (`crates/tui/src/settings.rs`). Exit 0, nothing on stderr, and no card — a card has no field to type a key into. `-b` has no modal, so `sindri -b` with no key is still exit 1, one line, nothing on stdout.
