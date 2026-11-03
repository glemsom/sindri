---
status: accepted
date: 2026-11-02
---

# 0003 — A built-in skill: the instructions ship inside the binary

ADR 0002 §3 names two scopes, both on disk: the home directory and the repository. A skill
compiled into the binary makes that false, so the third scope is recorded here.

## 1. The instructions ship in the artefact that enforces them

A built-in is `include_str!` of a `SKILL.md` beside the module that reads it, with no path,
because a path is a thing that can be absent, stale or unwritable. A file written into
`~/.agents/skills` at install time drifts from the binary: an upgrade replaces one and
leaves last release's instructions behind, so the model is told one boundary and the
sandbox applies another, and nothing reports the disagreement.

## 2. A body holds the procedure, never the boundary

The `bash` description already holds the invariant, derived from the policy, so the words
the model reads and the mounts it runs under cannot disagree. A built-in body holds the
procedure — what a run does in this workspace, in what order, with which tool, and why —
and may not carry a fact the enforcement can generate.

## 3. The untested risk, named

A built-in is needed *before* the first `bash` call, unlike a task skill, which a prompt
triggers once the task is recognised as covered. Nothing here measures whether a model
calls `skill` before its first `bash`, or whether one catalog line per skill is enough to
make it do so. `scripts/bench-skill-ablation` settles that by measurement.

## The alternative that lost

Ship the built-ins as files in this repository's `.agents/skills`. A run outside the
repository would be told nothing, and a run inside it would read the same body as house
rules: a user overriding it would be overriding a repository's file, not Sindri's.
