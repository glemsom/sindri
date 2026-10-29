# 0002 — One tool (`bash`), and the sandbox policy is the single source of truth

- **Status:** accepted
- **Date:** 2025-10-29

## Context

Cross-platform agents need native tools per capability. Sindri targets Linux
only, so it can follow the Unix philosophy: one `bash` tool, composed by the
model into pipelines and small scripts.

A sandbox that is enforced differently from how it is described will lie to the
model sooner or later — the model then plans against a fiction.

## Decision

1. The model gets exactly one tool: `bash`.
2. One `SandboxPolicy` value holds the writable set, network posture and temp
   mapping. Two pure functions derive from it: `to_argv()` (the bwrap
   invocation) and `describe()` (the model-facing text and the `sindri doctor`
   output). Neither may be written by hand anywhere else.
3. `--yolo-unsafe` bypasses bwrap and is reflected in `describe()` and in the
   session record.

## Consequences

- Drift between enforcement and description becomes a type-level impossibility,
  backed by a round-trip test over the generated argv.
- Verified against bwrap 0.13.1: `--ro-bind / /` must come first or writable
  binds are shadowed; `--dev /dev` is mandatory or `/dev/null` is unusable;
  `HOME` must be writable or `git` fails outright; `--die-with-parent` and
  `--new-session` are both load-bearing.
