---
status: accepted
date: 2026-10-29
---

# 0002 — One tool (`bash`), and `SandboxPolicy` is the only source of truth

## Decision

1. The model gets exactly one tool: `bash`, composed into pipelines and scripts.
2. `SandboxPolicy` (`crates/core/src/sandbox/policy.rs`) holds the writable set, the network posture and the temp mapping.
3. Two pure derivations, and no hand-written third:
   - `to_argv()` — `crates/core/src/sandbox/argv.rs`
   - `describe()` — `crates/core/src/sandbox/describe.rs`; the model-facing text and `sindri doctor`
4. `--yolo-unsafe` skips bwrap; `describe()` says so and the record says `unsafe` (ADR-0006).

## Consequences

- Enforcement and description cannot drift; a round-trip test covers the generated argv.
- bwrap argv order is contract (verified against bwrap 0.13.1):

| position | flag | why |
| --- | --- | --- |
| first | `--ro-bind / /` | writable binds are shadowed if it comes later |
| after | `--bind <path> <path>` per writable path, then the session temp | the only write targets |
| mandatory | `--dev /dev` | otherwise `/dev/null` is unusable |
| mandatory | writable `HOME` (session temp `home/`) | otherwise `git` fails outright |
| always | `--die-with-parent`, `--new-session` | neither is optional |
