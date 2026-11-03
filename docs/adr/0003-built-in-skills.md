---
status: accepted
date: 2026-11-02
---

# 0003 — A built-in skill: the instructions ship inside the binary

ADR 0002 §3 says a skill lives in someone's home directory or in the repository, and that
those two paths are ours. A skill shipped inside the binary makes both statements false,
so the third scope is recorded here. What a **built-in skill** is, and where it sits among
the three scopes, is `GLOSSARY.md`'s; this record holds the three decisions only it makes.

## 1. The instructions ship in the artefact that enforces them

A built-in is bytes compiled in — `include_str!` of a `SKILL.md` that lives in the tree
beside the module that reads it — and not a directory any scan walks. It has no path,
because a path is a thing that can be absent, stale or unwritable.

A file written into `~/.agents/skills` at install time drifts from the binary that
enforces what it describes: an upgrade replaces the binary and leaves last release's
instructions behind, so the model is told one boundary and the sandbox applies another,
and nothing reports the disagreement. Embedded, the two ship in one artefact or neither
does — which is also why `install.sh` stays one checksummed binary in one directory.

## 2. A body holds the procedure, never the boundary

A built-in could restate the sandbox boundary, and that is the failure this section
prevents. The `bash` description already holds the invariant, derived from the policy, so
the words the model reads and the mounts it runs under cannot disagree. A built-in body
holds the procedure — what a run does in this workspace, in what order, with which tool,
and why — and may not carry a fact the enforcement can generate, because the binary says
nothing about whether the procedure is still the one to follow.

## 3. The untested risk, named

A built-in is needed *before* the first `bash` call, unlike a task skill, which a prompt
triggers once the task is recognised as covered. Nothing in this tree measures whether a
model calls `skill` before its first `bash`, or whether one catalog line per skill is
enough to make it do so. `scripts/bench-skill-ablation` is where that is settled by
measurement rather than argument.

## The alternative that lost

Ship the built-ins as files in the repository's own `.agents/skills`. A run outside this
repository would then be told nothing, and a run inside it would read the same body as
house rules: a user overriding it would be overriding a repository's file, not Sindri's.
One scan path is not worth a body whose provenance is whatever directory holds it.
