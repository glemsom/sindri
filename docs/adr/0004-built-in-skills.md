---
status: accepted
date: 2026-11-02
---

# 0004 — A built-in skill: the instructions ship inside the binary

ADR 0002 §3 says a skill is found in someone's home directory or in the repository, and
that those two paths are ours. A skill shipped inside the binary makes both statements
false, so the third scope is written down here before the code that reads it. What a
**built-in skill** is, and where it sits among the three scopes, is `GLOSSARY.md`'s. This
record holds the three decisions only it makes.

## 1. The instructions ship in the artefact that enforces them

A built-in is bytes compiled into the binary — `include_str!` of a `SKILL.md` that lives in
the tree beside the module that reads it — and not a directory any scan walks. It has no
path, because a path is a thing that can be absent, stale or unwritable.

That is the whole argument for embedding rather than installing. A file written into
`~/.agents/skills` at install time drifts from the binary that enforces what it describes:
an upgrade replaces the binary and leaves last release's instructions behind, so the model
is told one boundary and the sandbox applies another, and nothing reports the disagreement.
Embedded, the instructions and the enforcement ship in the same artefact or neither does.
`install.sh` stays one binary, checksummed, in one directory, because a thing it does not
write cannot be left half-written.

## 2. A body holds the procedure, and never the boundary

A built-in could restate the sandbox boundary, and that is the failure this section exists
to prevent. The `bash` description holds the invariant, derived from the policy itself, so
the words the model reads and the mounts it runs under cannot disagree. A built-in body
holds the procedure: what a run does in this workspace, in what order, with which tool, and
why. A body is prose about a way of working; the binary it shipped in says nothing about
whether the procedure is still the one to follow, which is why it may not carry a fact the
enforcement can generate.

## 3. The untested risk, named

A built-in is needed *before* the first `bash` call. That is a different timing from a task
skill, which a prompt triggers once the task is recognised as one it covers: a built-in
covers every task, so the model has to reach for it before it knows what the task is made
of. Nothing in this tree measures whether a model calls `skill` before its first `bash`, or
whether one catalog line per skill is enough to make it. That is settled by measurement —
the one `scripts/bench-skill-ablation` begins — and not by argument.

## The alternative that lost

Ship the built-ins as files in the repository's own `.agents/skills`, under a directory the
repo scope already scans. A run outside this repository would then be told nothing, and a
run inside it would read the same body as house rules the moment it spent a day in a working
tree: a user overriding it would be overriding a repository's file, not Sindri's. One scan
path is not worth a body whose provenance is whatever directory happens to hold it.
