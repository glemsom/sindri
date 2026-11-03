---
status: accepted
date: 2026-11-02
---

# 0004 — A built-in skill: the instructions ship inside the binary

ADR 0002 §3 says a skill is found in someone's home directory or in the repository, and that
those two paths are ours. A skill shipped inside the binary makes both statements false, so
the third scope is written down here before the code that reads it. What it is, where it sits
among the three and what a reader can see of it belong to `GLOSSARY.md`'s **built-in skill**
entry, so they are not repeated here. This record holds the three decisions only it makes: the
instructions ship in the artefact that enforces them (§1), a body holds the procedure and never
the boundary (§2), and nothing yet measures whether a run reaches one (§3).

## 1. The instructions ship in the artefact that enforces them

A built-in skill is bytes compiled into the binary — `include_str!` of a `SKILL.md` that lives
in the tree beside the module that reads it — and not a directory any scan walks. It has no
path, because a path is a thing that can be absent, stale or unwritable, and a skill the binary
ships cannot be any of those.

That is the whole of the argument for embedding rather than installing. A file written into
`~/.agents/skills` at install time drifts from the binary that enforces what it describes: an
upgrade replaces the binary and leaves last release's instructions behind, so the model is
told one boundary and the sandbox applies another, and nothing reports the disagreement.
Embedded, the instructions and the enforcement ship in the same artefact or neither does.

`install.sh` stays what it is — one binary, checksummed, put in one directory — because a skill
tree is a second thing to download, verify and place, and a thing it does not write cannot be
left half-written.

## 2. A body holds the procedure, and never the boundary

A built-in could restate the sandbox boundary in its body, and that is the failure this section
exists to prevent. The `bash` description holds the invariant — the boundary a caller is told
about because it is enforced — and it is derived from the sandbox policy itself, so the words
the model reads and the mounts it runs under are one derivation and cannot disagree. The one
sentence the policy does not derive is the **pointer** to the catalog, and it is appended only
where the model may see one. A built-in body holds the procedure: what a run does in this
workspace, in what order, with which tool, and why.

The split follows from what each can know. A description derived from the policy is wrong only
if the policy is, and the policy is what `--yolo-unsafe` and the mounts are read into; a body
is prose about a way of working, and the binary it shipped in says nothing about whether the
procedure is still the one to follow. Anything that fits in one sentence belongs to neither
body but to the policy's own description, which is where the sentence is generated: two places
stating one fact are two places to disagree, and the place that can be generated from the
enforcement is the one that stays true.

## 3. The untested risk, named

A built-in is needed *before* the first `bash` call. That is a different timing from a task
skill, which a prompt triggers when a task is recognised as one it covers: a built-in covers
every task, so the model has to reach for it before it knows what the task is made of, and a
model that has loaded nothing yet is exactly the model deciding whether to load anything.

Nothing in this tree measures that. Whether a model calls `skill` before its first `bash`, and
whether the catalog's one-line-per-skill description is enough to make it, is an open question
this record names rather than answers. It is settled by measurement — the same kind of
measurement the preamble's tax was settled with, and the one `scripts/bench-skill-ablation`
begins — and not by argument, and a built-in whose body is only ever read after the command it
governs has failed is a built-in that paid for itself and bought nothing.

## The alternative that lost

Ship the skills as files in the repository's own `.agents/skills` — the built-ins committed
under a directory that the repo scope already scans — and let the existing walk find them.

It loses the thing the scope exists for. A run outside this repository would then be told
nothing, and a run inside it would be told "how this repository works" in the repository
scope's own words, which is the inference `GLOSSARY.md`'s **built-in skill** refuses: the same
body would read as house rules the moment it spent a day in a working tree, and a user
overriding it would be overriding a repository's file, not Sindri's. It also puts the built-ins
under the precedence of a nearer scope and inside a directory a user may edit, so an instruction
the binary depends on would be a file that the next edit could change without the binary changing
with it.

What the loss buys is one scan path instead of two, and one path is not worth a body whose
provenance is whatever directory happens to hold it.
